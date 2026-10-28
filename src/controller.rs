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
pub const DEFAULT_WATCHDOG_TIMEOUT_MS: u64 = 5000;

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
    cmd_lock: Mutex<[u64; 4]>,
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
            Self::reset_new_motors(&motors, &[]);
            let sensors = Sensor::find_all();
            (HardwareBackend::Real, motors, sensors)
        };

        let initial_battery = match &backend {
            HardwareBackend::Mock(_) => BatteryStatus { voltage_v: 7.8, current_a: 0.12 },
            HardwareBackend::Real => Self::query_real_battery(),
        };

        let now = Instant::now();
        let controller = Arc::new(Self {
            backend,
            cached_status: Arc::new(RwLock::new(Vec::new())),
            motors: Arc::new(RwLock::new(boot_motors)),
            cached_sensors: Arc::new(RwLock::new(Vec::new())),
            sensors: Arc::new(RwLock::new(boot_sensors)),
            cached_battery: Arc::new(RwLock::new(initial_battery)),
            poll_interval_ms,
            watchdog: Mutex::new(WatchdogState {
                last_activity: now,
                timeout: Duration::from_millis(DEFAULT_WATCHDOG_TIMEOUT_MS),
                active: false,
            }),
            cmd_lock: Mutex::new([0; 4]),
            rescan_signal: Arc::new((Mutex::new(false), Condvar::new())),
        });
        controller.poll_hardware();

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

    /// Sends a `reset` command to any discovered motor port not already present in `existing_ports`.
    ///
    /// Shared by initial controller boot (`existing_ports = &[]`) and runtime hardware rescans
    /// so newly plugged motors are initialized without interrupting motors that were already active.
    fn reset_new_motors(motors: &[Motor], existing_ports: &[String]) {
        for m in motors {
            if !existing_ports.iter().any(|p| p.eq_ignore_ascii_case(&m.port)) {
                if let Err(e) = m.send_command("reset") {
                    eprintln!("[WARN] Reset failed for motor on Port {}: {}", m.port, e);
                }
            }
        }
    }

    /// Merges newly scanned motors with existing motor handles to preserve open file descriptors
    /// for ports whose kernel `sysfs_path` has not changed (`BUG-36`).
    fn merge_motors(existing: &[Motor], scanned: Vec<Motor>) -> Vec<Motor> {
        scanned
            .into_iter()
            .map(|new_m| {
                existing
                    .iter()
                    .find(|old_m| old_m.port == new_m.port && old_m.sysfs_path == new_m.sysfs_path)
                    .cloned()
                    .unwrap_or(new_m)
            })
            .collect()
    }

    /// Samples all motors and sensors from the active backend and updates in-memory caches.
    ///
    /// Shared by `MotorController::new`, `run_poller_loop`, and mock `rescan` so backend dispatch
    /// and `RwLock` cache writes exist in a single method.
    fn poll_hardware(&self) {
        let statuses = match &self.backend {
            HardwareBackend::Mock(mock) => mock.poll_and_get_all_status(),
            HardwareBackend::Real => {
                let current_motors = self.motors.read().unwrap_or_else(|e| e.into_inner());
                Self::query_real_motors(&current_motors)
            }
        };

        let sensor_statuses = match &self.backend {
            HardwareBackend::Mock(mock) => mock.poll_and_get_all_sensors(),
            HardwareBackend::Real => {
                let current_sensors = self.sensors.read().unwrap_or_else(|e| e.into_inner());
                Self::query_real_sensors(&current_sensors)
            }
        };

        if let Ok(mut cache) = self.cached_status.write() {
            *cache = statuses;
        }
        if let Ok(mut cache) = self.cached_sensors.write() {
            *cache = sensor_statuses;
        }
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
        let dir = Path::new("/sys/class/power_supply/lego-ev3-battery");
        let voltage_v = crate::sysfs::read_attr_parsed(dir, "voltage_now", 7_800_000.0_f32) / 1_000_000.0;
        let current_a = (crate::sysfs::read_attr_parsed(dir, "current_now", 120_000.0_f32) / 1_000_000.0).abs();

        BatteryStatus { voltage_v, current_a }
    }

    /// Background polling loop running continuously on a dedicated thread.
    ///
    /// 1. Checks if a hardware rescan was requested and rescans if signaled.
    /// 2. Reads motor and sensor statuses via `poll_hardware()`.
    /// 3. Checks the safety watchdog timer (`check_watchdogs`).
    /// 4. Sleeps for `poll_interval_ms` (typically 50 ms).
    ///
    /// Runs indefinitely on the background poller thread.
    /// Keeps lock durations minimal so HTTP reader threads are never blocked.
    fn run_poller_loop(&self) {
        let interval = Duration::from_millis(self.poll_interval_ms);
        let mut battery_tick: u64 = 0;
        let battery_interval_ticks = (2000 / self.poll_interval_ms).max(1);

        loop {
            let mut did_rescan = false;
            {
                let (lock, _) = &*self.rescan_signal;
                let requested = lock.lock().unwrap_or_else(|e| e.into_inner());
                if *requested {
                    if let HardwareBackend::Real = &self.backend {
                        println!("[CONTROLLER] Hardware rescan triggered by client.");
                        let scanned = Motor::find_all();
                        let existing_ports: Vec<String> = {
                            let r = self.motors.read().unwrap_or_else(|e| e.into_inner());
                            r.iter().map(|m| m.port.clone()).collect()
                        };
                        Self::reset_new_motors(&scanned, &existing_ports);
                        if let Ok(mut w) = self.motors.write() {
                            *w = Self::merge_motors(&w, scanned);
                        }
                        if let Ok(mut w) = self.sensors.write() {
                            *w = Sensor::find_all();
                        }
                    }
                    did_rescan = true;
                }
            }

            if let HardwareBackend::Real = &self.backend {
                battery_tick += 1;
                if battery_tick >= battery_interval_ticks {
                    battery_tick = 0;
                    if let Ok(mut battery_guard) = self.cached_battery.write() {
                        *battery_guard = Self::query_real_battery();
                    }
                }
            }

            self.poll_hardware();

            if did_rescan {
                let (lock, cvar) = &*self.rescan_signal;
                let mut requested = lock.lock().unwrap_or_else(|e| e.into_inner());
                *requested = false;
                cvar.notify_all();
            }

            self.check_watchdogs();
            thread::sleep(interval);
        }
    }

    /// Enforces the 5-second motor idle watchdog timer.
    ///
    /// If watchdog is active and `last_activity.elapsed() > timeout` (5000 ms),
    /// automatically calls `coast_all_motors()` to halt motion and relax braked motors.
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
            println!("[WATCHDOG] No motor commands received within timeout. Coasting all motors.");
            if let Err(e) = self.coast_all_motors() {
                eprintln!("[WATCHDOG ERROR] Failed coasting motors: {}", e);
            }
        }
    }

    /// Looks up a physical motor instance by port letter, rescanning only when missing or requested.
    ///
    /// Returns the cached `Motor` handle immediately on the fast path without calling `stat()` (`exists()`).
    /// When `force_rescan` is true or the port is not in the cache, rescans `Motor::find_all()`.
    ///
    /// Shared helper for `with_real_motor` so normal commands execute with zero directory/stat syscalls.
    fn get_real_motor(&self, port: &str, force_rescan: bool) -> Result<Motor, String> {
        if !force_rescan {
            let guard = self.motors.read().unwrap_or_else(|e| e.into_inner());
            if let Some(m) = guard.iter().find(|m| m.port.eq_ignore_ascii_case(port)) {
                return Ok(m.clone());
            }
        }

        let scanned = Motor::find_all();
        let mut w = self.motors.write().unwrap_or_else(|e| e.into_inner());
        if !scanned.is_empty() {
            *w = Self::merge_motors(&w, scanned);
        }
        w.iter()
            .find(|m| m.port.eq_ignore_ascii_case(port))
            .cloned()
            .ok_or_else(|| format!("Motor on Port {} is not connected", port))
    }

    /// Executes an operation on a physical motor, automatically rescanning once if the sysfs node was re-enumerated.
    ///
    /// Runs `op` against the cached `Motor` without a `stat()` check; if `op` returns an `io::Error`
    /// (for example, after a cable is unplugged and plugged back in), rescans sysfs and retries once.
    ///
    /// Shared by `raw_run_forever`, `run_to_rel_pos`, `stop`, and `reset` to eliminate duplicate error conversion and retry code.
    fn with_real_motor(&self, port: &str, mut op: impl FnMut(&Motor) -> std::io::Result<()>) -> Result<(), String> {
        let motor = self.get_real_motor(port, false)?;
        if op(&motor).is_ok() {
            return Ok(());
        }
        let rescanned = self.get_real_motor(port, true)?;
        op(&rescanned).map_err(|e| e.to_string())
    }

    /// Arms or refreshes the 5-second motor idle watchdog timer.
    ///
    /// Sets `active = true`, refreshes `last_activity`, and sets timeout duration (clamped 100..5000 ms).
    ///
    /// Called whenever a motor command (move or stop) is executed.
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
    /// Clones `cached_status`, `cached_sensors`, and `cached_battery` without touching the motor watchdog.
    ///
    /// Rule 6 Seam: Called by HTTP worker threads on `GET /api/status`. Runs in <0.05 ms without sysfs I/O.
    pub fn get_system_status(&self) -> SystemStatus {
        SystemStatus {
            motors: self.cached_status.read().unwrap_or_else(|e| e.into_inner()).clone(),
            sensors: self.cached_sensors.read().unwrap_or_else(|e| e.into_inner()).clone(),
            battery: self.cached_battery.read().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }

    /// Fetches the most recent cached battery metrics.
    ///
    /// Clones `cached_battery` from in-memory cache.
    ///
    /// Used by `main.rs` to display battery voltage on the LCD screen without reading sysfs files.
    pub fn get_battery(&self) -> BatteryStatus {
        self.cached_battery.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Retrieves cached telemetry for an individual motor port.
    ///
    /// Searches `cached_status` for the given port and returns its `MotorStatus`.
    ///
    /// Used by `resolve_max_speed` to find motor rating limits.
    pub fn get_port_motor_status(&self, port: &str) -> Result<MotorStatus, String> {
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

    /// Checks and updates the last-seen sequence number for motor ports `A` through `D`.
    ///
    /// Returns `false` when `seq` is lower than the highest sequence number already executed
    /// on that motor port, or `true` after recording the new sequence number. Shared by
    /// `execute_port_command` and `execute_batch` under `cmd_lock`.
    fn accept_motor_seq(seqs: &mut [u64; 4], port: &str, seq: Option<u64>) -> bool {
        let idx = match port {
            "A" => 0,
            "B" => 1,
            "C" => 2,
            "D" => 3,
            _ => return true,
        };
        if let Some(s) = seq {
            if s < seqs[idx] {
                return false;
            }
            seqs[idx] = s;
        }
        true
    }

    /// Dispatches a resolved port command to motor or sensor handlers.
    ///
    /// Serializes hardware access and per-port sequence validation with `cmd_lock` so stale
    /// out-of-order HTTP requests do not overwrite a newer motor command.
    ///
    /// Main command dispatch entry point called by `POST /api/port/{port}`.
    pub fn execute_port_command(
        &self,
        port: &str,
        cmd: &ResolvedPortCommand,
        seq: Option<u64>,
    ) -> Result<String, String> {
        let mut seqs = self.cmd_lock.lock().unwrap_or_else(|e| e.into_inner());
        if !Self::accept_motor_seq(&mut seqs, port, seq) {
            return Ok(format!("Ignored stale command on Port {}", port));
        }
        self.execute_port_command_inner(port, cmd)
    }

    fn execute_port_command_inner(
        &self,
        port: &str,
        cmd: &ResolvedPortCommand,
    ) -> Result<String, String> {
        if is_sensor_port(port) {
            match cmd {
                ResolvedPortCommand::SetMode { mode } => {
                    self.set_sensor_mode(port, mode)?;
                    Ok(format!("Sensor on Port {} mode set to {}", port, mode))
                }
                _ => Err(format!("Port {} is a sensor and only accepts mode commands", port)),
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
    /// Sets `speed_sp` and sends `run-forever` command without redundant `stop_action` writes.
    ///
    /// Internal hardware helper called by `run_forever`.
    fn raw_run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.run_forever(port, speed);
                Ok(())
            }
            HardwareBackend::Real => self.with_real_motor(port, |motor| {
                motor.set_speed_sp(speed)?;
                motor.send_command("run-forever")
            }),
        }
    }

    /// Starts continuous motor rotation with 5-second safety watchdog tracking.
    ///
    /// Arms the watchdog timer and executes `raw_run_forever`.
    ///
    /// Public action method combining hardware execution with watchdog lifecycle management.
    pub fn run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
        self.arm_watchdog(None);
        self.raw_run_forever(port, speed)
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
        self.arm_watchdog(None);
        let action = stop_action.unwrap_or_else(|| "brake".into());
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.run_to_rel_pos(port, speed, rel_pos, Some(action));
                Ok(())
            }
            HardwareBackend::Real => self.with_real_motor(port, |motor| {
                motor.set_stop_action(&action)?;
                motor.set_speed_sp(speed)?;
                motor.set_position_sp(rel_pos)?;
                motor.send_command("run-to-rel-pos")
            }),
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
    /// Sends stop action and stop command to hardware, updates cache, and refreshes the 5-second idle timer.
    ///
    /// Public action method for single motor stops.
    pub fn stop(&self, port: &str, action: Option<String>) -> Result<(), String> {
        self.arm_watchdog(None);
        let action_str = action.unwrap_or_else(|| "brake".into());
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.stop(port, Some(action_str));
                Ok(())
            }
            HardwareBackend::Real => {
                self.with_real_motor(port, |motor| {
                    motor.set_stop_action(&action_str)?;
                    motor.send_command("stop")
                })?;
                self.mark_motor_stopped(port);
                Ok(())
            }
        }
    }

    /// Clears encoder readings and fault flags on a motor port.
    ///
    /// Sends reset command to the motor driver and marks the motor stopped in cache.
    ///
    /// Public action method for resetting encoder angles to zero.
    pub fn reset(&self, port: &str) -> Result<(), String> {
        self.arm_watchdog(None);
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.reset(port);
                Ok(())
            }
            HardwareBackend::Real => {
                self.with_real_motor(port, |motor| motor.send_command("reset"))?;
                self.mark_motor_stopped(port);
                Ok(())
            }
        }
    }

    /// Executes a batch of port commands with unified watchdog management and partial-failure resilience.
    ///
    /// Iterates through `batch.commands` under `cmd_lock`, ensuring a single transient port error
    /// does not prevent the remaining healthy motors in the batch from executing.
    ///
    /// Called by `POST /api/batch` for multi-motor synchronization.
    pub fn execute_batch(&self, batch: &BatchCommandPayload) -> Result<Vec<String>, String> {
        let mut seqs = self.cmd_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut results = Vec::new();
        let mut errors = Vec::new();
        let mut touched_motor = false;

        for item in &batch.commands {
            let Some(norm_port) = normalize_port_name(&item.port) else {
                errors.push(format!("Invalid port '{}'", item.port));
                continue;
            };
            let max_speed = self.resolve_max_speed(&norm_port);
            let resolved = match item.payload.resolve(max_speed) {
                Ok(r) => r,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };

            let item_seq = item.payload.seq.or(batch.seq);
            if !Self::accept_motor_seq(&mut seqs, &norm_port, item_seq) {
                results.push(format!("Ignored stale command on Port {}", norm_port));
                continue;
            }

            match self.execute_port_command_inner(&norm_port, &resolved) {
                Ok(msg) => {
                    results.push(msg);
                    if is_motor_port(&norm_port) {
                        touched_motor = true;
                    }
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }

        if touched_motor {
            self.arm_watchdog(batch.heartbeat_ms);
        }

        if !errors.is_empty() {
            Err(errors.join("; "))
        } else {
            Ok(results)
        }
    }

    /// Stops all motors with the specified stop action.
    ///
    /// Disarms watchdog on coast or re-arms 5-second idle timer on brake.
    ///
    /// Shared helper deduplicating `coast_all_motors` and `emergency_stop`.
    fn stop_all_motors(&self, stop_action: &str) -> Result<(), String> {
        let mut seqs = self.cmd_lock.lock().unwrap_or_else(|e| e.into_inner());
        if stop_action == "coast" {
            *seqs = [0; 4];
            if let Ok(mut wd) = self.watchdog.lock() {
                wd.active = false;
            }
        } else {
            self.arm_watchdog(None);
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
    /// Invoked automatically by watchdog timeout when 5 seconds pass without motor commands.
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
            HardwareBackend::Mock(_) => {
                self.poll_hardware();
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
            heartbeat_ms: Some(250),
            seq: None,
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
        assert_eq!(DEFAULT_WATCHDOG_TIMEOUT_MS, 5000);
        let controller = MotorController::new(true, 15);
        drive_dual(&controller, 500, 500, "brake");
        thread::sleep(Duration::from_millis(40));

        let st1 = controller.get_system_status();
        let b1 = st1.motors.iter().find(|m| m.port == "B").unwrap();
        let c1 = st1.motors.iter().find(|m| m.port == "C").unwrap();
        assert_eq!(b1.speed, 500);
        assert_eq!(c1.speed, 500);

        thread::sleep(Duration::from_millis(320));

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
        controller.arm_watchdog(Some(250));
        thread::sleep(Duration::from_millis(40));

        let st1 = controller.get_system_status();
        let a1 = st1.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a1.speed, 400);

        thread::sleep(Duration::from_millis(320));

        let st2 = controller.get_system_status();
        let a2 = st2.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a2.speed, 0);
        assert_eq!(controller.get_mock_stop_action("A"), Some("coast".into()));
    }

    #[test]
    fn test_partial_batch_failure_executes_remaining_ports_and_relaxes_brake_to_coast() {
        let controller = MotorController::new(true, 15);
        let batch = BatchCommandPayload {
            heartbeat_ms: Some(250),
            seq: None,
            commands: vec![
                BatchItemPayload {
                    port: "Z".into(),
                    payload: PortCommandPayload { speed: Some(300), ..Default::default() },
                },
                BatchItemPayload {
                    port: "B".into(),
                    payload: PortCommandPayload {
                        speed: Some(0),
                        stop_action: Some("brake".into()),
                        ..Default::default()
                    },
                },
            ],
        };
        assert!(controller.execute_batch(&batch).is_err());
        assert_eq!(controller.get_mock_stop_action("B"), Some("brake".into()));

        thread::sleep(Duration::from_millis(320));
        assert_eq!(controller.get_mock_stop_action("B"), Some("coast".into()));
    }

    #[test]
    fn test_stale_sequence_numbers_are_ignored() {
        let controller = MotorController::new(true, 15);

        // 1. Newer RUN (seq 11) arrives before stale STOP (seq 10) on Port A.
        controller
            .execute_port_command("A", &ResolvedPortCommand::RunForever { speed: 600 }, Some(11))
            .unwrap();
        let msg = controller
            .execute_port_command(
                "A",
                &ResolvedPortCommand::Stop { stop_action: "brake".into() },
                Some(10),
            )
            .unwrap();
        assert!(msg.contains("Ignored stale command"));
        thread::sleep(Duration::from_millis(30));
        let st1 = controller.get_system_status();
        assert_eq!(st1.motors.iter().find(|m| m.port == "A").unwrap().speed, 600);

        // 2. Newer STOP batch (seq 21) arrives before stale RUN batch (seq 20) on Port B.
        let stop_batch = BatchCommandPayload {
            heartbeat_ms: Some(5000),
            seq: Some(21),
            commands: vec![BatchItemPayload {
                port: "B".into(),
                payload: PortCommandPayload {
                    speed: Some(0),
                    stop_action: Some("brake".into()),
                    ..Default::default()
                },
            }],
        };
        let stale_run_batch = BatchCommandPayload {
            heartbeat_ms: Some(5000),
            seq: Some(20),
            commands: vec![BatchItemPayload {
                port: "B".into(),
                payload: PortCommandPayload {
                    speed: Some(800),
                    ..Default::default()
                },
            }],
        };
        controller.execute_batch(&stop_batch).unwrap();
        let res = controller.execute_batch(&stale_run_batch).unwrap();
        assert!(res[0].contains("Ignored stale command"));
        thread::sleep(Duration::from_millis(30));
        let st2 = controller.get_system_status();
        assert_eq!(st2.motors.iter().find(|m| m.port == "B").unwrap().speed, 0);
    }
}
