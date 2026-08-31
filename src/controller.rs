use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use crate::sysfs::{MockController, Motor, MotorStatus};

pub enum HardwareBackend {
    Real,
    Mock(MockController),
}

pub struct MotorController {
    backend: HardwareBackend,
    cached_status: Arc<RwLock<Vec<MotorStatus>>>,
    cached_motors: Arc<RwLock<Vec<Motor>>>,
    poll_interval_ms: u64,
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

        let controller = Arc::new(Self {
            backend,
            cached_status: Arc::new(RwLock::new(initial_status)),
            cached_motors: Arc::new(RwLock::new(initial_motors)),
            poll_interval_ms,
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
                });
            }
        }
        statuses
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
                    }

                    let motors = self.cached_motors.read().unwrap().clone();
                    Self::query_real_motors(&motors)
                }
            };

            if let Ok(mut cache) = self.cached_status.write() {
                *cache = statuses;
            }

            thread::sleep(interval);
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

    /// Read cached status from memory in <0.05ms (zero sysfs file I/O overhead)
    pub fn get_all_status(&self) -> Vec<MotorStatus> {
        self.cached_status.read().unwrap().clone()
    }

    pub fn run_forever(&self, port: &str, speed: i32) -> Result<(), String> {
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
        self.run_forever(left_port, left_speed)?;
        self.run_forever(right_port, right_speed)?;
        Ok(())
    }

    pub fn emergency_stop(&self) -> Result<(), String> {
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
