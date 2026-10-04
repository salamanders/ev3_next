use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use crate::sysfs::{MockController, Motor, MotorStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub voltage_v: f32,
    pub current_a: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub motors: Vec<MotorStatus>,
    pub battery: BatteryStatus,
}

pub enum HardwareBackend {
    Real,
    Mock(MockController),
}

pub struct MotorController {
    backend: HardwareBackend,
    cached_status: Arc<RwLock<Vec<MotorStatus>>>,
    cached_motors: Arc<RwLock<Vec<Motor>>>,
    cached_battery: Arc<RwLock<BatteryStatus>>,
    poll_interval_ms: u64,

    // Two-Tier Watchdog State (BUG-14)
    last_tank_drive_time: Arc<Mutex<Option<Instant>>>,
    tank_drive_ports: Arc<Mutex<(String, String)>>,
    tank_drive_active: Arc<Mutex<bool>>,
    last_status_poll_time: Arc<Mutex<Instant>>,
    continuous_run_active: Arc<Mutex<bool>>,
}

impl MotorController {
    pub fn new(mock_mode: bool, poll_interval_ms: u64) -> Arc<Self> {
        let (backend, initial_motors) = if mock_mode {
            println!("[CONTROLLER] Initializing in MOCK mode (simulated motors).");
            (HardwareBackend::Mock(MockController::new()), Vec::new())
        } else {
            println!("[CONTROLLER] Initializing in REAL hardware mode (sysfs /sys/class/tacho-motor).");
            (HardwareBackend::Real, Motor::find_all())
        };

        let initial_status = match &backend {
            HardwareBackend::Mock(mock) => mock.poll_and_get_all_status(),
            HardwareBackend::Real => Self::query_real_motors(&initial_motors),
        };

        let initial_battery = match &backend {
            HardwareBackend::Mock(_) => BatteryStatus { voltage_v: 7.8, current_a: 0.12 },
            HardwareBackend::Real => Self::query_real_battery(),
        };

        let now = Instant::now();
        let controller = Arc::new(Self {
            backend,
            cached_status: Arc::new(RwLock::new(initial_status)),
            cached_motors: Arc::new(RwLock::new(initial_motors)),
            cached_battery: Arc::new(RwLock::new(initial_battery)),
            poll_interval_ms,
            last_tank_drive_time: Arc::new(Mutex::new(None)),
            tank_drive_ports: Arc::new(Mutex::new(("B".into(), "C".into()))),
            tank_drive_active: Arc::new(Mutex::new(false)),
            last_status_poll_time: Arc::new(Mutex::new(now)),
            continuous_run_active: Arc::new(Mutex::new(false)),
        });

        // Spawn background polling thread to maintain cache without blocking HTTP handlers
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
        let mut statuses = Vec::new();
        for port_letter in ["A", "B", "C", "D"] {
            if let Some(m) = motors.iter().find(|m| m.port == port_letter) {
                statuses.push(m.read_status());
            } else {
                // Return disconnected placeholder
                statuses.push(MotorStatus {
                    port: port_letter.to_string(),
                    address: format!("out{}", port_letter),
                    driver_name: "none".into(),
                    position: 0,
                    speed: 0,
                    duty_cycle: 0,
                    state: vec![],
                    max_speed: 1050,
                    count_per_rot: 360,
                    connected: false,
                    polarity: "normal".into(),
                });
            }
        }
        statuses
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
        let mut ticks_since_discovery: u64 = 0;
        let discovery_interval_ticks = (2000 / self.poll_interval_ms).max(1);

        loop {
            let statuses = match &self.backend {
                HardwareBackend::Mock(mock) => mock.poll_and_get_all_status(),
                HardwareBackend::Real => {
                    ticks_since_discovery += 1;
                    if ticks_since_discovery >= discovery_interval_ticks {
                        ticks_since_discovery = 0;
                        if let Ok(mut motors_guard) = self.cached_motors.write() {
                            *motors_guard = Motor::find_all();
                        }
                        if let Ok(mut battery_guard) = self.cached_battery.write() {
                            *battery_guard = Self::query_real_battery();
                        }
                    }

                    let motors = self.cached_motors.read().unwrap().clone();
                    Self::query_real_motors(&motors)
                }
            };

            if let Ok(mut cache) = self.cached_status.write() {
                *cache = statuses;
            }

            // Run watchdog evaluations (BUG-14)
            self.check_watchdogs();

            thread::sleep(interval);
        }
    }

    fn check_watchdogs(&self) {
        // Tier 1: Tank Drive Watchdog (400 ms timeout)
        let mut need_stop_td = false;
        let mut td_ports = (String::new(), String::new());
        {
            let mut td_active = self.tank_drive_active.lock().unwrap();
            if *td_active {
                let last_td = self.last_tank_drive_time.lock().unwrap();
                if let Some(t) = *last_td {
                    if t.elapsed() > Duration::from_millis(400) {
                        *td_active = false;
                        need_stop_td = true;
                        td_ports = self.tank_drive_ports.lock().unwrap().clone();
                    }
                }
            }
        }
        if need_stop_td {
            println!("[WATCHDOG] Tank drive heartbeat timed out (>400ms). Halting drive motors {} & {}.", td_ports.0, td_ports.1);
            let _ = self.stop(&td_ports.0, Some("brake".into()));
            let _ = self.stop(&td_ports.1, Some("brake".into()));
        }

        // Tier 2: Client Connection Watchdog (1000 ms timeout)
        let mut need_stop_cr = false;
        {
            let mut cr_active = self.continuous_run_active.lock().unwrap();
            if *cr_active {
                let last_poll = *self.last_status_poll_time.lock().unwrap();
                if last_poll.elapsed() > Duration::from_millis(1000) {
                    *cr_active = false;
                    need_stop_cr = true;
                }
            }
        }
        if need_stop_cr {
            println!("[WATCHDOG] Client connection poll timed out (>1000ms). Halting all continuous motors.");
            let _ = self.emergency_stop();
        }
    }

    fn get_real_motor(&self, port: &str) -> Result<Motor, String> {
        // Fast path: check cached motors without disk I/O
        if let Ok(guard) = self.cached_motors.read() {
            if let Some(motor) = guard.iter().find(|m| m.port.eq_ignore_ascii_case(port)) {
                return Ok(motor.clone());
            }
        }

        // Slow path fallback: motor was newly connected, scan once and cache
        let refreshed = Motor::find_all();
        let found = refreshed.iter().find(|m| m.port.eq_ignore_ascii_case(port)).cloned();
        if let Ok(mut guard) = self.cached_motors.write() {
            *guard = refreshed;
        }

        found.ok_or_else(|| format!("Motor on Port {} not connected", port))
    }

    pub fn touch_client_poll(&self) {
        if let Ok(mut guard) = self.last_status_poll_time.lock() {
            *guard = Instant::now();
        }
    }

    /// Read cached status from memory in <0.05ms (zero sysfs file I/O overhead)
    #[allow(dead_code)]
    pub fn get_all_status(&self) -> Vec<MotorStatus> {
        self.touch_client_poll();
        self.cached_status.read().unwrap().clone()
    }

    pub fn get_system_status(&self) -> SystemStatus {
        self.touch_client_poll();
        SystemStatus {
            motors: self.cached_status.read().unwrap().clone(),
            battery: self.cached_battery.read().unwrap().clone(),
        }
    }

    pub fn get_battery(&self) -> BatteryStatus {
        self.cached_battery.read().unwrap().clone()
    }

    pub fn set_polarity(&self, port: &str, polarity: &str) -> Result<(), String> {
        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.set_polarity(port, polarity);
                Ok(())
            }
            HardwareBackend::Real => {
                let mut motor = self.get_real_motor(port)?;
                motor.set_polarity(polarity).map_err(|e| e.to_string())?;
                if let Ok(mut guard) = self.cached_motors.write() {
                    if let Some(m) = guard.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
                        let _ = m.set_polarity(polarity);
                    }
                }
                Ok(())
            }
        }
    }

    pub fn run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
        if let Ok(mut cr) = self.continuous_run_active.lock() {
            *cr = true;
        }

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
        if let Ok(mut cr) = self.continuous_run_active.lock() {
            *cr = true;
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
        // Update Tank Drive heartbeat watchdog state (BUG-14)
        if let Ok(mut last) = self.last_tank_drive_time.lock() {
            *last = Some(Instant::now());
        }
        if let Ok(mut ports) = self.tank_drive_ports.lock() {
            *ports = (left_port.to_string(), right_port.to_string());
        }
        if let Ok(mut active) = self.tank_drive_active.lock() {
            *active = left_speed != 0 || right_speed != 0;
        }

        self.run_forever_internal(left_port, left_speed)?;
        self.run_forever_internal(right_port, right_speed)?;
        Ok(())
    }

    // Helper for tank drive so we don't trigger the continuous_run_active watchdog on tank drive
    fn run_forever_internal(&self, port: &str, speed: i32) -> Result<(), String> {
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

    pub fn emergency_stop(&self) -> Result<(), String> {
        if let Ok(mut td) = self.tank_drive_active.lock() {
            *td = false;
        }
        if let Ok(mut cr) = self.continuous_run_active.lock() {
            *cr = false;
        }

        match &self.backend {
            HardwareBackend::Mock(mock) => {
                mock.emergency_stop();
                Ok(())
            }
            HardwareBackend::Real => {
                let motors = self.cached_motors.read().unwrap().clone();
                let mut errors = Vec::new();
                for motor in &motors {
                    if let Err(e) = motor.set_stop_action("hold") {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tank_drive_watchdog_timeout() {
        let controller = MotorController::new(true, 15);
        // Start tank drive
        controller.tank_drive("B", "C", 500, 500).unwrap();
        // Wait for background poller tick
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
        // Wait for background poller tick
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
}

