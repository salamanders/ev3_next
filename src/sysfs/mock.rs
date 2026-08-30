use std::sync::{Arc, Mutex};
use std::time::Instant;
use super::motor::MotorStatus;

#[derive(Clone)]
pub struct MockController {
    motors: Arc<Mutex<[MockMotorState; 4]>>,
}

#[derive(Clone)]
struct MockMotorState {
    port: &'static str,
    driver_name: &'static str,
    max_speed: i32,
    count_per_rot: i32,
    speed_sp: i32,
    duty_cycle_sp: i32,
    position: f64,
    target_pos: Option<i32>,
    time_remaining_ms: Option<u32>,
    running: bool,
    holding: bool,
    stop_action: String,
    last_tick: Instant,
}

impl MockController {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            motors: Arc::new(Mutex::new([
                MockMotorState {
                    port: "A",
                    driver_name: "lego-ev3-l-motor (mock)",
                    max_speed: 1050,
                    count_per_rot: 360,
                    speed_sp: 0,
                    duty_cycle_sp: 0,
                    position: 0.0,
                    target_pos: None,
                    time_remaining_ms: None,
                    running: false,
                    holding: false,
                    stop_action: "brake".into(),
                    last_tick: now,
                },
                MockMotorState {
                    port: "B",
                    driver_name: "lego-ev3-l-motor (mock)",
                    max_speed: 1050,
                    count_per_rot: 360,
                    speed_sp: 0,
                    duty_cycle_sp: 0,
                    position: 0.0,
                    target_pos: None,
                    time_remaining_ms: None,
                    running: false,
                    holding: false,
                    stop_action: "brake".into(),
                    last_tick: now,
                },
                MockMotorState {
                    port: "C",
                    driver_name: "lego-ev3-m-motor (mock)",
                    max_speed: 1560,
                    count_per_rot: 360,
                    speed_sp: 0,
                    duty_cycle_sp: 0,
                    position: 0.0,
                    target_pos: None,
                    time_remaining_ms: None,
                    running: false,
                    holding: false,
                    stop_action: "brake".into(),
                    last_tick: now,
                },
                MockMotorState {
                    port: "D",
                    driver_name: "lego-ev3-m-motor (mock)",
                    max_speed: 1560,
                    count_per_rot: 360,
                    speed_sp: 0,
                    duty_cycle_sp: 0,
                    position: 0.0,
                    target_pos: None,
                    time_remaining_ms: None,
                    running: false,
                    holding: false,
                    stop_action: "brake".into(),
                    last_tick: now,
                },
            ])),
        }
    }

    #[allow(dead_code)]
    pub fn set_speed_sp(&self, port: &str, speed: i32) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.speed_sp = speed.clamp(-motor.max_speed, motor.max_speed);
        }
    }

    #[allow(dead_code)]
    pub fn set_duty_cycle_sp(&self, port: &str, duty: i32) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.duty_cycle_sp = duty.clamp(-100, 100);
            motor.speed_sp = (motor.max_speed * duty) / 100;
        }
    }

    #[allow(dead_code)]
    pub fn set_stop_action(&self, port: &str, action: &str) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.stop_action = action.to_string();
        }
    }

    pub fn run_forever(&self, port: &str, speed: i32) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.speed_sp = speed.clamp(-motor.max_speed, motor.max_speed);
            motor.duty_cycle_sp = (motor.speed_sp * 100) / motor.max_speed;
            motor.running = true;
            motor.holding = false;
            motor.target_pos = None;
            motor.time_remaining_ms = None;
            motor.last_tick = Instant::now();
        }
    }

    pub fn run_timed(&self, port: &str, speed: i32, time_ms: u32, stop_action: Option<String>) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.speed_sp = speed.clamp(-motor.max_speed, motor.max_speed);
            motor.duty_cycle_sp = (motor.speed_sp * 100) / motor.max_speed;
            motor.running = true;
            motor.holding = false;
            motor.target_pos = None;
            motor.time_remaining_ms = Some(time_ms);
            if let Some(action) = stop_action {
                motor.stop_action = action;
            }
            motor.last_tick = Instant::now();
        }
    }

    pub fn run_to_rel_pos(&self, port: &str, speed: i32, rel_pos: i32, stop_action: Option<String>) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            let target = (motor.position as i32) + rel_pos;
            let dir = if rel_pos >= 0 { 1 } else { -1 };
            motor.speed_sp = (speed.abs() * dir).clamp(-motor.max_speed, motor.max_speed);
            motor.duty_cycle_sp = (motor.speed_sp * 100) / motor.max_speed;
            motor.target_pos = Some(target);
            motor.time_remaining_ms = None;
            motor.running = true;
            motor.holding = false;
            if let Some(action) = stop_action {
                motor.stop_action = action;
            }
            motor.last_tick = Instant::now();
        }
    }

    pub fn run_direct(&self, port: &str, duty_cycle: i32) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.duty_cycle_sp = duty_cycle.clamp(-100, 100);
            motor.speed_sp = (motor.max_speed * motor.duty_cycle_sp) / 100;
            motor.running = true;
            motor.holding = false;
            motor.target_pos = None;
            motor.time_remaining_ms = None;
            motor.last_tick = Instant::now();
        }
    }

    pub fn stop(&self, port: &str, action: Option<String>) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.running = false;
            motor.speed_sp = 0;
            motor.duty_cycle_sp = 0;
            motor.target_pos = None;
            motor.time_remaining_ms = None;
            if let Some(act) = action {
                motor.stop_action = act;
            }
            motor.holding = motor.stop_action == "hold";
        }
    }

    pub fn reset(&self, port: &str) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.running = false;
            motor.speed_sp = 0;
            motor.duty_cycle_sp = 0;
            motor.position = 0.0;
            motor.target_pos = None;
            motor.time_remaining_ms = None;
            motor.holding = false;
        }
    }

    pub fn emergency_stop(&self) {
        let mut m = self.motors.lock().unwrap();
        for motor in m.iter_mut() {
            motor.running = false;
            motor.speed_sp = 0;
            motor.duty_cycle_sp = 0;
            motor.target_pos = None;
            motor.time_remaining_ms = None;
            motor.holding = true;
            motor.stop_action = "hold".into();
        }
    }

    pub fn poll_and_get_all_status(&self) -> Vec<MotorStatus> {
        let mut m = self.motors.lock().unwrap();
        let now = Instant::now();

        m.iter_mut().map(|motor| {
            let dt = now.duration_since(motor.last_tick).as_secs_f64();
            motor.last_tick = now;

            if motor.running {
                let delta_pos = (motor.speed_sp as f64) * dt;
                motor.position += delta_pos;

                // Handle timed runs
                if let Some(ref mut remaining) = motor.time_remaining_ms {
                    let elapsed_ms = (dt * 1000.0) as u32;
                    if *remaining <= elapsed_ms {
                        motor.running = false;
                        motor.speed_sp = 0;
                        motor.duty_cycle_sp = 0;
                        motor.time_remaining_ms = None;
                        motor.holding = motor.stop_action == "hold";
                    } else {
                        *remaining -= elapsed_ms;
                    }
                }

                // Handle relative/absolute position targets
                if let Some(target) = motor.target_pos {
                    let current = motor.position as i32;
                    let reached = if motor.speed_sp >= 0 {
                        current >= target
                    } else {
                        current <= target
                    };
                    if reached {
                        motor.position = target as f64;
                        motor.running = false;
                        motor.speed_sp = 0;
                        motor.duty_cycle_sp = 0;
                        motor.target_pos = None;
                        motor.holding = motor.stop_action == "hold";
                    }
                }
            }

            let mut state = Vec::new();
            if motor.running {
                state.push("running".to_string());
                if motor.duty_cycle_sp.abs() > 80 {
                    state.push("ramping".to_string());
                }
            } else if motor.holding {
                state.push("holding".to_string());
            }

            MotorStatus {
                port: motor.port.to_string(),
                address: format!("out{}", motor.port),
                driver_name: motor.driver_name.to_string(),
                position: motor.position.round() as i32,
                speed: if motor.running { motor.speed_sp } else { 0 },
                duty_cycle: if motor.running { motor.duty_cycle_sp } else { 0 },
                state,
                max_speed: motor.max_speed,
                count_per_rot: motor.count_per_rot,
                connected: true,
            }
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_mock_initial_state() {
        let mock = MockController::new();
        let statuses = mock.poll_and_get_all_status();
        assert_eq!(statuses.len(), 4);
        for (i, port) in ["A", "B", "C", "D"].iter().enumerate() {
            assert_eq!(&statuses[i].port, port);
            assert_eq!(statuses[i].position, 0);
            assert_eq!(statuses[i].speed, 0);
            assert!(!statuses[i].state.contains(&"running".to_string()));
        }
    }

    #[test]
    fn test_mock_run_forever_and_stop() {
        let mock = MockController::new();
        mock.run_forever("A", 500);
        thread::sleep(Duration::from_millis(50));
        let statuses = mock.poll_and_get_all_status();
        let motor_a = &statuses[0];
        assert_eq!(motor_a.speed, 500);
        assert!(motor_a.state.contains(&"running".to_string()));
        assert!(motor_a.position > 0);

        // Test Stop
        mock.stop("A", Some("brake".into()));
        let statuses = mock.poll_and_get_all_status();
        assert_eq!(statuses[0].speed, 0);
        assert!(!statuses[0].state.contains(&"running".to_string()));
    }

    #[test]
    fn test_mock_emergency_stop() {
        let mock = MockController::new();
        mock.run_forever("A", 800);
        mock.run_forever("B", 800);
        mock.run_forever("C", 800);
        mock.run_forever("D", 800);

        mock.emergency_stop();
        let statuses = mock.poll_and_get_all_status();
        for m in statuses {
            assert_eq!(m.speed, 0);
            assert!(!m.state.contains(&"running".to_string()));
            assert!(m.state.contains(&"holding".to_string()));
        }
    }

    #[test]
    fn test_mock_speed_clamping() {
        let mock = MockController::new();
        // Port A max speed is 1050
        mock.run_forever("A", 5000);
        let statuses = mock.poll_and_get_all_status();
        assert_eq!(statuses[0].speed, 1050);

        mock.run_forever("A", -5000);
        let statuses = mock.poll_and_get_all_status();
        assert_eq!(statuses[0].speed, -1050);
    }
}

