//! # Central Hardware Controller and Watchdog
//!
//! This module coordinates motor and sensor hardware, provides cached telemetry, and enforces safety rules.
//! It acts as the intermediary between incoming HTTP requests and low-level sysfs driver operations.
//!
//! `MotorController` exists between the web routing layer and low-level sysfs drivers for three reasons:
//! 1. Rule 6 (No sysfs reads on HTTP threads): Background telemetry thread updates an in-memory cache
//!    every 50 ms. HTTP worker threads serve `GET /api/status` from RAM in under 0.05 ms without blocking.
//! 2. Hardware abstraction: Unifies real Linux sysfs hardware (`HardwareBackend::Real`) and host simulation
//!    (`HardwareBackend::Mock`) behind a single clean Rust API.
//! 3. Safety watchdog: Automatically coasts all motors if client communication stops for over 2 seconds.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use crate::sysfs::{MockController, Motor, MotorStatus, Sensor};
use crate::web::{
    is_motor_port, is_sensor_port, normalize_port_name, BatchCommandPayload,
    ResolvedPortCommand, SensorStatus, MOTOR_PORTS, SENSOR_PORTS,
};

/// Snapshot of battery voltage and current consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub voltage_v: f32,
    pub current_a: f32,
}

/// Unified system status combining all motors, sensors, and battery metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub motors: Vec<MotorStatus>,
    pub sensors: Vec<SensorStatus>,
    pub battery: BatteryStatus,
}

/// Hardware backend driver variants.
pub enum HardwareBackend {
    Real,
    Mock(MockController),
}

/// Default watchdog timeout duration in milliseconds.
pub const DEFAULT_WATCHDOG_TIMEOUT_MS: u64 = 2000;

/// Internal watchdog state tracking client activity.
struct WatchdogState {
    last_activity: Instant,
    timeout: Duration,
    active: bool,
}

/// Central controller managing EV3 hardware devices and safety timers.
pub struct MotorController {
    backend: HardwareBackend,
    cached_status: Arc<RwLock<Vec<MotorStatus>>>,
    motors: Arc<RwLock<Vec<Motor>>>,
    cached_sensors: Arc<RwLock<Vec<SensorStatus>>>,
    sensors: Arc<RwLock<Vec<Sensor>>>,
    cached_battery: Arc<RwLock<BatteryStatus>>,
    poll_interval_ms: u64,
    watchdog: Mutex<WatchdogState>,
    rescan_signal: Arc<(Mutex<bool>, Condvar)>,
}

impl MotorController {
    /// Creates a controller instance and launches the background telemetry thread.
    ///
    /// 1. Discovers connected motors and sensors (or initializes in-memory mock devices).
    /// 2. Takes an initial hardware status snapshot.
    /// 3. Spawns the dedicated `sysfs-poller` thread running `run_poller_loop`.
    ///
    /// Initialization entry point called once by `main()` during server startup.
    pub fn new(mock_mode: bool, poll_interval_ms: u64) -> Arc<Self> {
        let (backend, boot_motors, boot_sensors) = if mock_mode {
            println!("[CONTROLLER] Initializing in MOCK mode (simulated motors and sensors).");
            (HardwareBackend::Mock(MockController::new()), Vec::new(), Vec::new())
        } else {
            println!("[CONTROLLER] Initializing in REAL hardware mode (sysfs /sys/class/tacho-motor and lego-sensor).");
            let motors = Motor::find_all();
            for m in &motors {
                if let Err(e) = m.send_command("reset") {
                    eprintln!("[WARN] Initial reset failed for motor on Port {}: {}", m.port, e);
                }
            }
            let sensors = Sensor::find_all();
            (HardwareBackend::Real, motors, sensors)
        };

        let initial_status = match &backend {
            HardwareBackend::Mock(mock) => mock.poll_and_get_all_status(),
            HardwareBackend::Real => Self::query_real_motors(&boot_motors),
        };

        let initial_sensors = match &backend {
            HardwareBackend::Mock(mock) => mock.poll_and_get_all_sensors(),
            HardwareBackend::Real => Self::query_real_sensors(&boot_sensors),
        };

        let initial_battery = match &backend {
            HardwareBackend::Mock(_) => BatteryStatus { voltage_v: 7.8, current_a: 0.12 },
            HardwareBackend::Real => Self::query_real_battery(),
        };

        let now = Instant::now();
        let motors_arc = Arc::new(RwLock::new(boot_motors));
        let sensors_arc = Arc::new(RwLock::new(boot_sensors));
        let rescan_signal = Arc::new((Mutex::new(false), Condvar::new()));
        let controller = Arc::new(Self {
            backend,
            cached_status: Arc::new(RwLock::new(initial_status)),
            motors: motors_arc,
            cached_sensors: Arc::new(RwLock::new(initial_sensors)),
            sensors: sensors_arc,
            cached_battery: Arc::new(RwLock::new(initial_battery)),
            poll_interval_ms,
            watchdog: Mutex::new(WatchdogState {
                last_activity: now,
                timeout: Duration::from_millis(DEFAULT_WATCHDOG_TIMEOUT_MS),
                active: false,
            }),
            rescan_signal,
        });

        // Background thread updates RAM cache without blocking HTTP threads (Rule 6).
        let c_clone = controller.clone();
        thread::Builder::new()
            .name("sysfs-poller".into())
            .spawn(move || {
                c_clone.run_poller_loop();
            })
            .expect("Failed to spawn telemetry polling thread");

        controller
    }

    /// Queries dynamic metrics across all physical motor ports A, B, C, and D.
    ///
    /// Maps over `MOTOR_PORTS`, calling `read_status()` on connected motors or returning `disconnected`.
    ///
    /// Called exclusively by the background polling thread.
    fn query_real_motors(motors: &[Motor]) -> Vec<MotorStatus> {
        MOTOR_PORTS
            .iter()
            .map(|&port| {
                motors
                    .iter()
                    .find(|m| m.port == port)
                    .map(|m| m.read_status())
                    .unwrap_or_else(|| MotorStatus::disconnected(port))
            })
            .collect()
    }

    /// Samples measurement values across connected physical sensor ports 1, 2, 3, and 4.
    ///
    /// Maps over `SENSOR_PORTS`, calling `read_status()` on connected sensors or returning `disconnected`.
    ///
    /// Called exclusively by the background polling thread.
    fn query_real_sensors(sensors: &[Sensor]) -> Vec<SensorStatus> {
        SENSOR_PORTS
            .iter()
            .map(|&port| {
                sensors
                    .iter()
                    .find(|s| s.port == port)
                    .map(|s| s.read_status())
                    .unwrap_or_else(|| SensorStatus::disconnected(port))
            })
            .collect()
    }

    /// Reads physical power supply voltage and current from sysfs nodes.
    ///
    /// Reads `/sys/class/power_supply/lego-ev3-battery/voltage_now` and `current_now`.
    /// Converts microvolts and microamps to volts and amps.
    ///
    /// Called on a slow 2-second cycle by the background poller to avoid excess sysfs reads.
    fn query_real_battery() -> BatteryStatus {
        let volt_path = Path::new("/sys/class/power_supply/lego-ev3-battery/voltage_now");
        let curr_path = Path::new("/sys/class/power_supply/lego-ev3-battery/current_now");

        let voltage_v = fs::read_to_string(volt_path)
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|uv| uv / 1_000_000.0)
            .unwrap_or(7.8);

        let current_a = fs::read_to_string(curr_path)
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|ua| (ua / 1_000_000.0).abs())
            .unwrap_or(0.12);

        BatteryStatus { voltage_v, current_a }
    }

    /// Background polling loop running continuously on a dedicated thread.
    ///
    /// 1. Checks if a hardware rescan was requested and rescans if signaled.
    /// 2. Reads motor and sensor statuses.
    /// 3. Updates in-memory `cached_status` and `cached_sensors` with write locks.
    /// 4. Checks the safety watchdog timer (`check_watchdogs`).
    /// 5. Sleeps for `poll_interval_ms` (typically 50 ms).
    ///
    /// Runs indefinitely on the background poller thread.
    /// Keeps lock durations minimal so HTTP reader threads are never blocked.
    fn run_poller_loop(&self) {
        let interval = Duration::from_millis(self.poll_interval_ms);
        let mut battery_tick: u64 = 0;
        let battery_interval_ticks = (2000 / self.poll_interval_ms).max(1);

        loop {
            let mut did_rescan = false;
            // Check if hardware rescan was requested
            {
                let (lock, _) = &*self.rescan_signal;
                let requested = lock.lock().unwrap();
                if *requested {
                    if let HardwareBackend::Real = &self.backend {
                        println!("[CONTROLLER] Hardware rescan triggered by client.");
                        let scanned = Motor::find_all();
                        let existing_ports: Vec<String> = {
                            let r = self.motors.read().unwrap();
                            r.iter().map(|m| m.port.clone()).collect()
                        };
                        for m in &scanned {
                            if !existing_ports.iter().any(|p| p.eq_ignore_ascii_case(&m.port)) {
                                if let Err(e) = m.send_command("reset") {
                                    eprintln!("[WARN] Reset failed on newly discovered port {}: {}", m.port, e);
                                }
                            }
                        }
                        if let Ok(mut w) = self.motors.write() {
                            *w = scanned;
                        }

                        let scanned_sensors = Sensor::find_all();
                        if let Ok(mut w) = self.sensors.write() {
                            *w = scanned_sensors;
                        }
                    }
                    did_rescan = true;
                }
            }

            let statuses = match &self.backend {
                HardwareBackend::Mock(mock) => mock.poll_and_get_all_status(),
                HardwareBackend::Real => {
                    battery_tick += 1;
                    if battery_tick >= battery_interval_ticks {
                        battery_tick = 0;
                        if let Ok(mut battery_guard) = self.cached_battery.write() {
                            *battery_guard = Self::query_real_battery();
                        }
                    }
                    let current_motors = self.motors.read().unwrap();
                    Self::query_real_motors(&current_motors)
                }
            };

            let sensor_statuses = match &self.backend {
                HardwareBackend::Mock(mock) => mock.poll_and_get_all_sensors(),
                HardwareBackend::Real => {
                    let current_sensors = self.sensors.read().unwrap();
                    Self::query_real_sensors(&current_sensors)
                }
            };

            if let Ok(mut cache) = self.cached_status.write() {
                *cache = statuses;
            }

            if let Ok(mut cache) = self.cached_sensors.write() {
                *cache = sensor_statuses;
            }

            if did_rescan {
                let (lock, cvar) = &*self.rescan_signal;
                let mut requested = lock.lock().unwrap();
                *requested = false;
                cvar.notify_all();
            }

            self.check_watchdogs();
            thread::sleep(interval);
        }
    }

    /// Enforces the unified safety watchdog timer.
    ///
    /// If watchdog is active and `last_activity.elapsed() > timeout` (2000 ms),
    /// automatically calls `coast_all_motors()` to halt robot motion.
    ///
    /// Called at the end of every polling cycle by the background poller thread.
    fn check_watchdogs(&self) {
        let should_stop = {
            let mut wd = match self.watchdog.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("[WATCHDOG ERROR] Watchdog lock poisoned: {}", e);
                    return;
                }
            };

            if wd.active && wd.last_activity.elapsed() > wd.timeout {
                wd.active = false;
                true
            } else {
                false
            }
        };

        if should_stop {
            if !self.is_any_motor_running() {
                return;
            }
            println!("[WATCHDOG] Client communication timed out (>2000ms). Coasting all motors.");
            if let Err(e) = self.coast_all_motors() {
                eprintln!("[WATCHDOG ERROR] Failed coasting motors: {}", e);
            }
        }
    }

    /// Checks whether any motor is actively rotating.
    ///
    /// Checks cached telemetry for any motor with non-zero speed or "running" state.
    ///
    /// Used by watchdog checks and stop actions to determine if motion is occurring.
    pub fn is_any_motor_running(&self) -> bool {
        match &self.backend {
            HardwareBackend::Mock(mock) => mock.is_any_motor_running(),
            HardwareBackend::Real => {
                if let Ok(cache) = self.cached_status.read() {
                    cache.iter().any(|m| {
                        m.connected
                            && (m.speed != 0
                                || m.duty_cycle != 0
                                || m.state.iter().any(|s| s == "running"))
                    })
                } else {
                    false
                }
            }
        }
    }

    /// Looks up a physical motor instance by port letter.
    ///
    /// Acquires read lock on `motors` and returns a cloned `Motor` handle for the given port.
    ///
    /// Shared helper for all real motor command executions.
    fn get_real_motor(&self, port: &str) -> Result<Motor, String> {
        self.motors
            .read()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|m| m.port.eq_ignore_ascii_case(port))
            .cloned()
            .ok_or_else(|| format!("Motor on Port {} is not connected", port))
    }

    /// Refreshes the watchdog activity timestamp upon client HTTP requests.
    ///
    /// Updates `last_activity = Instant::now()`.
    ///
    /// Called whenever the browser polls `/api/status` or sends motor commands.
    pub fn touch_client_poll(&self) {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.last_activity = Instant::now();
        }
    }

    /// Arms the watchdog timer when continuous motion begins.
    ///
    /// Sets `active = true`, refreshes `last_activity`, and sets timeout duration (clamped 100..5000 ms).
    ///
    /// Called when `run_forever` or batch continuous movement commands are received.
    fn arm_watchdog(&self, custom_timeout_ms: Option<u64>) {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.active = true;
            wd.last_activity = Instant::now();
            let ms = custom_timeout_ms.unwrap_or(DEFAULT_WATCHDOG_TIMEOUT_MS);
            wd.timeout = Duration::from_millis(ms.clamp(100, 5000));
        }
    }

    /// Returns the complete cached system status snapshot from RAM.
    ///
    /// Touches the watchdog timer and clones `cached_status`, `cached_sensors`, and `cached_battery`.
    ///
    /// Rule 6 Seam: Called by HTTP worker threads on `GET /api/status`. Runs in <0.05 ms without sysfs I/O.
    pub fn get_system_status(&self) -> SystemStatus {
        self.touch_client_poll();
        SystemStatus {
            motors: self.cached_status.read().unwrap().clone(),
            sensors: self.cached_sensors.read().unwrap().clone(),
            battery: self.cached_battery.read().unwrap().clone(),
        }
    }

    /// Fetches the most recent cached battery metrics.
    ///
    /// Clones `cached_battery` from in-memory cache.
    ///
    /// Used by `main.rs` to display battery voltage on the LCD screen without reading sysfs files.
    pub fn get_battery(&self) -> BatteryStatus {
        self.touch_client_poll();
        self.cached_battery.read().unwrap().clone()
    }

    /// Retrieves cached telemetry for an individual motor port.
    ///
    /// Searches `cached_status` for the given port and returns its `MotorStatus`.
    ///
    /// Used by `resolve_max_speed` to find motor rating limits.
    pub fn get_port_motor_status(&self, port: &str) -> Result<MotorStatus, String> {
        self.touch_client_poll();
        let cache = self.cached_status.read().map_err(|e| e.to_string())?;
        cache
            .iter()
            .find(|m| m.port.eq_ignore_ascii_case(port))
            .cloned()
            .ok_or_else(|| format!("Port '{}' not found", port))
    }

    /// Determines the maximum rated speed for a port.
    ///
    /// Returns the connected motor's `max_speed` attribute (defaulting to 1050 if disconnected).
    ///
    /// Used by command resolution to calculate default half-speed and clamp speed values.
    pub fn resolve_max_speed(&self, port: &str) -> i32 {
        if is_motor_port(port) {
            self.get_port_motor_status(port)
                .map(|s| s.max_speed)
                .unwrap_or(1050)
        } else {
            1050
        }
    }

    /// Changes the operating mode for a sensor port.
    ///
    /// Calls `set_mode` on the target sensor (or `MockController::set_sensor_mode`).
    ///
    /// Hardware interface called when browser users switch sensor modes.
    pub fn set_sensor_mode(&self, port: &str, mode: &str) -> Result<(), String> {
        self.touch_client_poll();
        match &self.backend {
            HardwareBackend::Mock(mock) => mock.set_sensor_mode(port, mode),
            HardwareBackend::Real => {
                let sensor = self.get_real_sensor(port)?;
                sensor.set_mode(mode).map_err(|e| e.to_string())
            }
        }
    }

    /// Looks up a physical sensor instance by port number.
    ///
    /// Acquires read lock on `sensors` and returns a cloned `Sensor` handle for the given port.
    ///
    /// Shared helper for real sensor mode changes.
    fn get_real_sensor(&self, port: &str) -> Result<Sensor, String> {
        self.sensors
            .read()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|s| s.port == port)
            .cloned()
            .ok_or_else(|| format!("Sensor on Port {} is not connected", port))
    }

    /// Dispatches a resolved port command to motor or sensor handlers.
    ///
    /// Validates whether the port is a sensor (1..4) or motor (A..D) and routes the command.
    ///
    /// Main command dispatch entry point called by `POST /api/port/{port}` and batch execution.
    pub fn execute_port_command(
        &self,
        port: &str,
        cmd: &ResolvedPortCommand,
    ) -> Result<String, String> {
        self.touch_client_poll();
        if is_sensor_port(port) {
            match cmd {
                ResolvedPortCommand::SetMode { mode } => {
                    self.set_sensor_mode(port, mode)?;
                    Ok(format!("Sensor on Port {} mode set to {}", port, mode))
                }
            }
        } else if is_motor_port(port) {
            match cmd {
                ResolvedPortCommand::RunForever { speed } => {
                    self.run_forever(port, *speed)?;
                    Ok(format!("Motor on Port {} running forever at speed {}", port, speed))
                }
                ResolvedPortCommand::RunToRelPos { speed, position_sp, stop_action } => {
                    self.run_to_rel_pos(port, *speed, *position_sp, Some(stop_action.clone()))?;
                    Ok(format!("Motor on Port {} stepping {} counts at speed {}", port, position_sp, speed))
                }
                ResolvedPortCommand::Stop { stop_action } => {
                    self.stop(port, Some(stop_action.clone()))?;
                    Ok(format!("Motor on Port {} stopped ({})", port, stop_action))
                }
                ResolvedPortCommand::Reset => {
                    self.reset(port)?;
                    Ok(format!("Motor on Port {} reset", port))
                }
                ResolvedPortCommand::SetMode { .. } => {
                    Err(format!("Port {} is a motor and does not support mode commands", port))
                }
            }
        } else {
            Err(format!("Invalid port '{}'", port))
        }
    }

    /// Returns true if running in mock hardware simulation mode.
    ///
    /// Checks `HardwareBackend::Mock` variant.
    ///
    /// Used by router and main to verify simulation state.
    pub fn is_mock(&self) -> bool {
        matches!(self.backend, HardwareBackend::Mock(_))
    }

    /// Directly sends continuous run instructions to hardware backend.
    ///
    /// Configures `stop_action = coast`, sets `speed_sp`, and sends `run-forever` command.
    ///
    /// Internal hardware helper called by `run_forever`.
    fn raw_run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.run_forever(port, speed);
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_stop_action("coast").map_err(|e| e.to_string())?;
                motor.set_speed_sp(speed).map_err(|e| e.to_string())?;
                motor.send_command("run-forever").map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    /// Starts continuous motor rotation with safety watchdog tracking.
    ///
    /// 1. If speed is non-zero, arms the watchdog timer.
    /// 2. Executes `raw_run_forever`.
    /// 3. If speed is zero and no other motors are running, disarms the watchdog.
    ///
    /// Public action method combining hardware execution with watchdog lifecycle management.
    pub fn run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
        if speed != 0 {
            self.arm_watchdog(None);
        }
        let res = self.raw_run_forever(port, speed);
        if speed == 0 && res.is_ok() && !self.is_any_motor_running() {
            if let Ok(mut wd) = self.watchdog.lock() {
                wd.active = false;
            }
        }
        res
    }

    /// Rotates a motor by a relative degree count.
    ///
    /// Sets stop action, speed, relative position setpoint, and dispatches `run-to-rel-pos`.
    ///
    /// Public action method for angular stepping movements.
    pub fn run_to_rel_pos(
        &self,
        port: &str,
        speed: i32,
        rel_pos: i32,
        stop_action: Option<String>,
    ) -> Result<(), String> {
        self.touch_client_poll();
        let action = stop_action.unwrap_or_else(|| "brake".into());
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.run_to_rel_pos(port, speed, rel_pos, Some(action));
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_stop_action(&action).map_err(|e| e.to_string())?;
                motor.set_speed_sp(speed).map_err(|e| e.to_string())?;
                motor.set_position_sp(rel_pos).map_err(|e| e.to_string())?;
                motor.send_command("run-to-rel-pos").map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    /// Updates internal in-memory cache when a motor stops or resets.
    ///
    /// Sets speed and duty_cycle to 0 and removes "running" state in `cached_status`.
    ///
    /// Shared helper called by `stop` and `reset` to provide immediate telemetry feedback.
    fn mark_motor_stopped(&self, port: &str) {
        if let Ok(mut cache) = self.cached_status.write() {
            if let Some(m) = cache
                .iter_mut()
                .find(|m| m.port.eq_ignore_ascii_case(port))
            {
                m.speed = 0;
                m.duty_cycle = 0;
                m.state.retain(|s| s != "running");
            }
        }
    }

    /// Halts motion on a specified motor port using the chosen stop action.
    ///
    /// Sends stop action and stop command to hardware, updates cache, and disarms watchdog if all motors stopped.
    ///
    /// Public action method for single motor stops.
    pub fn stop(&self, port: &str, action: Option<String>) -> Result<(), String> {
        self.touch_client_poll();
        let action_str = action.unwrap_or_else(|| "brake".into());
        let res = match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.stop(port, Some(action_str));
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_stop_action(&action_str).map_err(|e| e.to_string())?;
                motor.send_command("stop").map_err(|e| e.to_string())?;
                self.mark_motor_stopped(port);
                Ok(())
            }
        };
        if res.is_ok() && !self.is_any_motor_running() {
            if let Ok(mut wd) = self.watchdog.lock() {
                wd.active = false;
            }
        }
        res
    }

    /// Clears encoder readings and fault flags on a motor port.
    ///
    /// Sends reset command to the motor driver and marks the motor stopped in cache.
    ///
    /// Public action method for resetting encoder angles to zero.
    pub fn reset(&self, port: &str) -> Result<(), String> {
        self.touch_client_poll();
        let res = match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.reset(port);
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.send_command("reset").map_err(|e| e.to_string())?;
                self.mark_motor_stopped(port);
                Ok(())
            }
        };
        if res.is_ok() && !self.is_any_motor_running() {
            if let Ok(mut wd) = self.watchdog.lock() {
                wd.active = false;
            }
        }
        res
    }

    /// Executes a batch of port commands with unified watchdog management.
    ///
    /// Iterates through `batch.commands`, resolves commands, executes each, and arms the watchdog.
    ///
    /// Called by `POST /api/batch` for multi-motor synchronization (e.g. driving both left and right wheels).
    pub fn execute_batch(&self, batch: &BatchCommandPayload) -> Result<Vec<String>, String> {
        self.touch_client_poll();
        let mut results = Vec::new();
        let mut has_motion = false;

        for item in &batch.commands {
            let norm_port = normalize_port_name(&item.port)
                .ok_or_else(|| format!("Invalid port '{}'", item.port))?;
            let max_speed = self.resolve_max_speed(&norm_port);
            let resolved = item.payload.resolve(max_speed)?;
            let msg = self.execute_port_command(&norm_port, &resolved)?;
            results.push(msg);

            if is_motor_port(&norm_port) {
                if let ResolvedPortCommand::RunForever { speed } = &resolved {
                    if *speed != 0 {
                        has_motion = true;
                    }
                }
            }
        }

        if has_motion {
            self.arm_watchdog(batch.heartbeat_ms);
        } else if !self.is_any_motor_running() {
            if let Ok(mut wd) = self.watchdog.lock() {
                wd.active = false;
            }
        }

        Ok(results)
    }

    /// Stops all motors with the specified stop action.
    ///
    /// Disarms watchdog and sends stop action and command to all motors.
    ///
    /// Shared helper deduplicating `coast_all_motors` and `emergency_stop`.
    fn stop_all_motors(&self, stop_action: &str) -> Result<(), String> {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.active = false;
        }

        match &self.backend {
            HardwareBackend::Mock(mock) => {
                if stop_action == "coast" {
                    mock.coast_all();
                } else {
                    mock.emergency_stop();
                }
                Ok(())
            }
            HardwareBackend::Real => {
                let mut errors = Vec::new();
                let motors = self.motors.read().map_err(|e| e.to_string())?;
                for motor in motors.iter() {
                    if let Err(e) = motor.set_stop_action(stop_action) {
                        errors.push(format!("Port {} stop_action error: {}", motor.port, e));
                    }
                    if let Err(e) = motor.send_command("stop") {
                        errors.push(format!("Port {} stop command error: {}", motor.port, e));
                    }
                }
                if !errors.is_empty() {
                    Err(errors.join("; "))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Halts all motors using coast action to minimize gear train stress.
    ///
    /// Calls `stop_all_motors("coast")`.
    ///
    /// Invoked automatically by watchdog timeout when client communication drops.
    pub fn coast_all_motors(&self) -> Result<(), String> {
        self.stop_all_motors("coast")
    }

    /// Immediately brakes all motors with hold action.
    ///
    /// Calls `stop_all_motors("brake")`.
    ///
    /// Invoked by `POST /api/estop` and the UI emergency stop button.
    pub fn emergency_stop(&self) -> Result<(), String> {
        self.stop_all_motors("brake")
    }

    /// Re-enumerates connected motors and sensors on demand.
    ///
    /// Signals the background poller thread to scan sysfs directories and returns updated status.
    ///
    /// Invoked by `POST /api/rescan` when cables are plugged or unplugged.
    pub fn rescan(&self) -> Result<SystemStatus, String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                let motors = mock.poll_and_get_all_status();
                let sensors = mock.poll_and_get_all_sensors();
                if let Ok(mut cache) = self.cached_status.write() {
                    *cache = motors;
                }
                if let Ok(mut cache) = self.cached_sensors.write() {
                    *cache = sensors;
                }
                Ok(self.get_system_status())
            }
            HardwareBackend::Real => {
                let (lock, cvar) = &*self.rescan_signal;
                let mut requested = lock.lock().map_err(|e| e.to_string())?;
                *requested = true;
                let _ = cvar.wait_timeout_while(requested, Duration::from_millis(200), |req| *req);
                Ok(self.get_system_status())
            }
        }
    }

    /// Reads stop action from mock backend for test verification.
    ///
    /// Returns the stop action stored in the mock controller for the specified port.
    ///
    /// Used by watchdog unit tests to verify coast action was applied.
    #[cfg(test)]
    pub fn get_mock_stop_action(&self, port: &str) -> Option<String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => mock.get_stop_action(port),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::{BatchItemPayload, PortCommandPayload};

    fn drive_dual(c: &MotorController, b_spd: i32, c_spd: i32, stop: &str) {
        let batch = BatchCommandPayload {
            heartbeat_ms: Some(2000),
            commands: vec![
                BatchItemPayload {
                    port: "B".into(),
                    payload: PortCommandPayload {
                        speed: Some(b_spd),
                        stop_action: Some(stop.to_string()),
                        ..Default::default()
                    },
                },
                BatchItemPayload {
                    port: "C".into(),
                    payload: PortCommandPayload {
                        speed: Some(c_spd),
                        stop_action: Some(stop.to_string()),
                        ..Default::default()
                    },
                },
            ],
        };
        c.execute_batch(&batch).unwrap();
    }

    #[test]
    fn test_joystick_watchdog_timeout() {
        let controller = MotorController::new(true, 15);
        drive_dual(&controller, 500, 500, "brake");
        thread::sleep(Duration::from_millis(40));

        let st1 = controller.get_system_status();
        let b1 = st1.motors.iter().find(|m| m.port == "B").unwrap();
        let c1 = st1.motors.iter().find(|m| m.port == "C").unwrap();
        assert_eq!(b1.speed, 500);
        assert_eq!(c1.speed, 500);

        // Sleep >2000ms without heartbeat (timeout is 2000ms)
        thread::sleep(Duration::from_millis(2100));

        let st2 = controller.get_system_status();
        let b2 = st2.motors.iter().find(|m| m.port == "B").unwrap();
        let c2 = st2.motors.iter().find(|m| m.port == "C").unwrap();
        assert_eq!(b2.speed, 0);
        assert_eq!(c2.speed, 0);
        assert_eq!(controller.get_mock_stop_action("B"), Some("coast".into()));
        assert_eq!(controller.get_mock_stop_action("C"), Some("coast".into()));
    }

    #[test]
    fn test_controller_battery() {
        let controller = MotorController::new(true, 15);
        let st = controller.get_system_status();
        assert_eq!(st.battery.voltage_v, 7.8);
        assert_eq!(st.battery.current_a, 0.12);
        assert_eq!(controller.get_battery().voltage_v, 7.8);
    }

    #[test]
    fn test_client_disconnect_watchdog_timeout() {
        let controller = MotorController::new(true, 15);
        controller.run_forever("A", 400).unwrap();
        thread::sleep(Duration::from_millis(40));

        let st1 = controller.get_system_status();
        let a1 = st1.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a1.speed, 400);

        // Sleep >2000ms without client communication (timeout is 2000ms)
        thread::sleep(Duration::from_millis(2100));

        let st2 = controller.get_system_status();
        let a2 = st2.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a2.speed, 0);
    }
}
