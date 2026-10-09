use std::fs;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use crate::sysfs::{MockController, Motor, MotorStatus, Sensor};
use crate::web::handlers::{AllPortsStatus, ResolvedPortCommand, SensorStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub voltage_v: f32,
    pub current_a: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub motors: Vec<MotorStatus>,
    pub sensors: Vec<SensorStatus>,
    pub battery: BatteryStatus,
}

pub enum HardwareBackend {
    Real,
    Mock(MockController),
}

struct WatchdogState {
    last_tank_drive: Option<Instant>,
    tank_drive_ports: (String, String),
    tank_drive_active: bool,
    last_status_poll: Instant,
    continuous_active: bool,
}

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
                last_tank_drive: None,
                tank_drive_ports: ("B".into(), "C".into()),
                tank_drive_active: false,
                last_status_poll: now,
                continuous_active: false,
            }),
            rescan_signal,
        });

        // Background polling thread updates RAM telemetry without blocking HTTP requests (Rule 6)
        let c_clone = controller.clone();
        thread::Builder::new()
            .name("sysfs-poller".into())
            .spawn(move || {
                c_clone.run_poller_loop();
            })
            .expect("Failed to spawn telemetry polling thread");

        controller
    }

    fn query_real_motors(motors: &[Motor]) -> Vec<MotorStatus> {
        ["A", "B", "C", "D"]
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

    fn query_real_sensors(sensors: &[Sensor]) -> Vec<SensorStatus> {
        ["1", "2", "3", "4"]
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

    fn run_poller_loop(&self) {
        let interval = Duration::from_millis(self.poll_interval_ms);
        let mut battery_tick: u64 = 0;
        let battery_interval_ticks = (2000 / self.poll_interval_ms).max(1);

        loop {
            // Check if hardware rescan was requested
            {
                let (lock, cvar) = &*self.rescan_signal;
                let mut requested = lock.lock().unwrap();
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
                    *requested = false;
                    cvar.notify_all();
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

            self.check_watchdogs();
            thread::sleep(interval);
        }
    }

    fn check_watchdogs(&self) {
        let (stop_td, td_ports, stop_cr) = {
            let mut wd = match self.watchdog.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("[WATCHDOG ERROR] Watchdog lock poisoned: {}", e);
                    return;
                }
            };

            let mut stop_td = false;
            let mut td_ports = (String::new(), String::new());
            if wd.tank_drive_active {
                if let Some(t) = wd.last_tank_drive {
                    if t.elapsed() > Duration::from_millis(400) {
                        wd.tank_drive_active = false;
                        stop_td = true;
                        td_ports = wd.tank_drive_ports.clone();
                    }
                }
            }

            let mut stop_cr = false;
            if wd.continuous_active && wd.last_status_poll.elapsed() > Duration::from_millis(1000) {
                wd.continuous_active = false;
                stop_cr = true;
            }

            (stop_td, td_ports, stop_cr)
        };

        if stop_td {
            println!("[WATCHDOG] Tank drive heartbeat timed out (>400ms). Halting drive motors {} & {}.", td_ports.0, td_ports.1);
            if let Err(e) = self.stop(&td_ports.0, Some("brake".into())) {
                eprintln!("[WATCHDOG ERROR] Failed stopping port {}: {}", td_ports.0, e);
            }
            if let Err(e) = self.stop(&td_ports.1, Some("brake".into())) {
                eprintln!("[WATCHDOG ERROR] Failed stopping port {}: {}", td_ports.1, e);
            }
        }

        if stop_cr {
            println!("[WATCHDOG] Client connection poll timed out (>1000ms). Halting all continuous motors.");
            if let Err(e) = self.emergency_stop() {
                eprintln!("[WATCHDOG ERROR] Failed emergency stopping motors: {}", e);
            }
        }
    }

    fn get_real_motor(&self, port: &str) -> Result<Motor, String> {
        self.motors
            .read()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|m| m.port.eq_ignore_ascii_case(port))
            .cloned()
            .ok_or_else(|| format!("Motor on Port {} is not connected", port))
    }

    pub fn touch_client_poll(&self) {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.last_status_poll = Instant::now();
        }
    }

    pub fn get_system_status(&self) -> SystemStatus {
        self.touch_client_poll();
        SystemStatus {
            motors: self.cached_status.read().unwrap().clone(),
            sensors: self.cached_sensors.read().unwrap().clone(),
            battery: self.cached_battery.read().unwrap().clone(),
        }
    }

    pub fn get_battery(&self) -> BatteryStatus {
        self.cached_battery.read().unwrap().clone()
    }

    pub fn get_port_motor_status(&self, port: &str) -> Result<MotorStatus, String> {
        self.touch_client_poll();
        let cache = self.cached_status.read().map_err(|e| e.to_string())?;
        cache
            .iter()
            .find(|m| m.port.eq_ignore_ascii_case(port))
            .cloned()
            .ok_or_else(|| format!("Port '{}' not found", port))
    }

    pub fn get_port_status(&self, port: &str) -> Result<serde_json::Value, String> {
        if ["A", "B", "C", "D"].iter().any(|p| p.eq_ignore_ascii_case(port)) {
            let status = self.get_port_motor_status(port)?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        } else if ["1", "2", "3", "4"].iter().any(|p| p.eq_ignore_ascii_case(port)) {
            self.touch_client_poll();
            let cache = self.cached_sensors.read().map_err(|e| e.to_string())?;
            let status = cache
                .iter()
                .find(|s| s.port == port)
                .cloned()
                .ok_or_else(|| format!("Port '{}' not found", port))?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        } else {
            Err(format!("Port '{}' not found", port))
        }
    }

    pub fn get_all_ports_status(&self) -> AllPortsStatus {
        self.touch_client_poll();
        let motors = self.cached_status.read().unwrap().clone();
        let sensors = self.cached_sensors.read().unwrap().clone();
        AllPortsStatus { motors, sensors }
    }

    pub fn set_sensor_mode(&self, port: &str, mode: &str) -> Result<(), String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => mock.set_sensor_mode(port, mode),
            HardwareBackend::Real => {
                let sensor = self.get_real_sensor(port)?;
                sensor.set_mode(mode).map_err(|e| e.to_string())
            }
        }
    }

    fn get_real_sensor(&self, port: &str) -> Result<Sensor, String> {
        self.sensors
            .read()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|s| s.port == port)
            .cloned()
            .ok_or_else(|| format!("Sensor on Port {} is not connected", port))
    }

    pub fn execute_port_command(
        &self,
        port: &str,
        cmd: &ResolvedPortCommand,
    ) -> Result<String, String> {
        if ["1", "2", "3", "4"].iter().any(|p| p.eq_ignore_ascii_case(port)) {
            match cmd {
                ResolvedPortCommand::SetMode { mode } => {
                    self.set_sensor_mode(port, mode)?;
                    Ok(format!("Sensor on Port {} mode set to {}", port, mode))
                }
                _ => Err(format!("Port {} is a sensor and only accepts mode commands", port)),
            }
        } else if ["A", "B", "C", "D"].iter().any(|p| p.eq_ignore_ascii_case(port)) {
            match cmd {
                ResolvedPortCommand::RunForever { speed } => {
                    self.run_forever(port, *speed)?;
                    Ok(format!("Motor on Port {} running forever at speed {}", port, speed))
                }
                ResolvedPortCommand::RunTimed { speed, time_ms, stop_action } => {
                    self.run_timed(port, *speed, *time_ms, Some(stop_action.clone()))?;
                    Ok(format!("Motor on Port {} running timed ({} ms at speed {})", port, time_ms, speed))
                }
                ResolvedPortCommand::RunToRelPos { speed, position_sp, stop_action } => {
                    self.run_to_rel_pos(port, *speed, *position_sp, Some(stop_action.clone()))?;
                    Ok(format!("Motor on Port {} stepping {} counts at speed {}", port, position_sp, speed))
                }
                ResolvedPortCommand::RunDirect { duty_cycle } => {
                    self.run_direct(port, *duty_cycle)?;
                    Ok(format!("Motor on Port {} direct duty {}%", port, duty_cycle))
                }
                ResolvedPortCommand::Stop { stop_action } => {
                    self.stop(port, Some(stop_action.clone()))?;
                    Ok(format!("Motor on Port {} stopped ({})", port, stop_action))
                }
                ResolvedPortCommand::Reset => {
                    self.reset(port)?;
                    Ok(format!("Motor on Port {} reset", port))
                }
                ResolvedPortCommand::SetPolarity { polarity } => {
                    self.set_polarity(port, polarity)?;
                    Ok(format!("Motor on Port {} polarity set to {}", port, polarity))
                }
                ResolvedPortCommand::SetMode { .. } => {
                    Err(format!("Port {} is a motor and does not support mode commands", port))
                }
            }
        } else {
            Err(format!("Invalid port '{}'", port))
        }
    }

    pub fn is_mock(&self) -> bool {
        matches!(self.backend, HardwareBackend::Mock(_))
    }

    pub fn set_polarity(&self, port: &str, polarity: &str) -> Result<(), String> {
        let val = if polarity == "inversed" { "inversed" } else { "normal" };
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.set_polarity(port, val);
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_polarity(val).map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    pub fn run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.continuous_active = true;
        }
        self.raw_run_forever(port, speed)
    }

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

    pub fn run_timed(&self, port: &str, speed: i32, time_ms: u32, stop_action: Option<String>) -> Result<(), String> {
        let action = stop_action.unwrap_or_else(|| "brake".into());
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.run_timed(port, speed, time_ms, Some(action));
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_stop_action(&action).map_err(|e| e.to_string())?;
                motor.set_speed_sp(speed).map_err(|e| e.to_string())?;
                motor.set_time_sp(time_ms).map_err(|e| e.to_string())?;
                motor.send_command("run-timed").map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    pub fn run_to_rel_pos(&self, port: &str, speed: i32, rel_pos: i32, stop_action: Option<String>) -> Result<(), String> {
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

    pub fn run_direct(&self, port: &str, duty_cycle: i32) -> Result<(), String> {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.continuous_active = true;
        }

        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.run_direct(port, duty_cycle);
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_duty_cycle_sp(duty_cycle).map_err(|e| e.to_string())?;
                motor.send_command("run-direct").map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    pub fn stop(&self, port: &str, action: Option<String>) -> Result<(), String> {
        let action_str = action.unwrap_or_else(|| "brake".into());
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.stop(port, Some(action_str));
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.set_stop_action(&action_str).map_err(|e| e.to_string())?;
                motor.send_command("stop").map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    pub fn reset(&self, port: &str) -> Result<(), String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.reset(port);
                Ok(())
            }
            HardwareBackend::Real => {
                let motor = self.get_real_motor(port)?;
                motor.send_command("reset").map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }

    pub fn tank_drive(&self, left_port: &str, right_port: &str, left_speed: i32, right_speed: i32) -> Result<(), String> {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.last_tank_drive = Some(Instant::now());
            wd.tank_drive_ports = (left_port.to_string(), right_port.to_string());
            wd.tank_drive_active = left_speed != 0 || right_speed != 0;
        }

        if left_speed == 0 && right_speed == 0 {
            self.stop(left_port, Some("brake".into()))?;
            self.stop(right_port, Some("brake".into()))?;
            return Ok(());
        }

        self.raw_run_forever(left_port, left_speed)?;
        self.raw_run_forever(right_port, right_speed)?;
        Ok(())
    }

    pub fn emergency_stop(&self) -> Result<(), String> {
        if let Ok(mut wd) = self.watchdog.lock() {
            wd.tank_drive_active = false;
            wd.continuous_active = false;
        }

        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.emergency_stop();
                Ok(())
            }
            HardwareBackend::Real => {
                let mut errors = Vec::new();
                let motors = self.motors.read().map_err(|e| e.to_string())?;
                for motor in motors.iter() {
                    if let Err(e) = motor.set_stop_action("brake") {
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

    pub fn rescan(&self) -> Result<AllPortsStatus, String> {
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
                Ok(self.get_all_ports_status())
            }
            HardwareBackend::Real => {
                let (lock, cvar) = &*self.rescan_signal;
                let mut requested = lock.lock().map_err(|e| e.to_string())?;
                *requested = true;
                let result = cvar
                    .wait_timeout_while(requested, Duration::from_secs(2), |req| *req)
                    .map_err(|e| e.to_string())?;
                if result.1.timed_out() {
                    return Err("Hardware rescan timed out".into());
                }
                Ok(self.get_all_ports_status())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tank_drive_watchdog_timeout() {
        let controller = MotorController::new(true, 15);
        // Start tank drive
        controller.tank_drive("B", "C", 500, 500).unwrap();
        thread::sleep(Duration::from_millis(40));

        // Immediately check: motors B and C should be running
        let st1 = controller.get_system_status();
        let b1 = st1.motors.iter().find(|m| m.port == "B").unwrap();
        let c1 = st1.motors.iter().find(|m| m.port == "C").unwrap();
        assert_eq!(b1.speed, 500);
        assert_eq!(c1.speed, 500);

        // Sleep 500ms without heartbeat (timeout is 400ms)
        thread::sleep(Duration::from_millis(500));

        // After timeout, watchdog should halt motors B and C
        let st2 = controller.get_system_status();
        let b2 = st2.motors.iter().find(|m| m.port == "B").unwrap();
        let c2 = st2.motors.iter().find(|m| m.port == "C").unwrap();
        assert_eq!(b2.speed, 0);
        assert_eq!(c2.speed, 0);
    }

    #[test]
    fn test_controller_polarity_and_battery() {
        let controller = MotorController::new(true, 15);
        let st = controller.get_system_status();
        assert_eq!(st.battery.voltage_v, 7.8);
        assert_eq!(st.battery.current_a, 0.12);

        controller.set_polarity("A", "inversed").unwrap();
        thread::sleep(Duration::from_millis(40));

        let st_inv = controller.get_system_status();
        let a = st_inv.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a.polarity, "inversed");
    }

    #[test]
    fn test_client_disconnect_watchdog_timeout() {
        let controller = MotorController::new(true, 15);
        // Start continuous run on motor A
        controller.run_forever("A", 400).unwrap();
        thread::sleep(Duration::from_millis(40));

        // Check running
        let st1 = controller.get_system_status();
        let a1 = st1.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a1.speed, 400);

        // Sleep >1000ms without calling get_system_status or touch_client_poll
        thread::sleep(Duration::from_millis(1100));

        // Background poller should have triggered emergency stop
        let st2 = controller.get_system_status();
        let a2 = st2.motors.iter().find(|m| m.port == "A").unwrap();
        assert_eq!(a2.speed, 0);
    }

    #[test]
    fn test_controller_rescan_mock() {
        let controller = MotorController::new(true, 15);
        let rescan = controller.rescan().expect("rescan should succeed in mock mode");
        assert_eq!(rescan.motors.len(), 4);
        assert_eq!(rescan.sensors.len(), 4);
        assert!(rescan.motors.iter().any(|m| m.port == "A"));
        assert!(rescan.motors.iter().any(|m| m.port == "B"));
    }
}
