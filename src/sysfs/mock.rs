//! # Host Hardware Simulation Mock Interface
//!
//! This module simulates EV3 motors and sensors in host memory for developer testing.
//! It implements simulated kinematics for motors (ports A, B, C, D) and standard sensor readings (ports 1, 2, 3, 4).
//!
//! Host-First Simulation Rule (Rule 4): Web interface and control logic must be testable on host machines
//! at `http://localhost:8080/` without requiring physical EV3 hardware or Linux sysfs paths.

use std::sync::{Arc, Mutex};
use std::time::Instant;
use super::motor::MotorStatus;
use super::sensor::SensorStatus;

/// Controller providing in-memory simulation of motors and sensors.
#[derive(Clone)]
pub struct MockController {
    motors: Arc<Mutex<[MockMotorState; 4]>>,
    sensors: Arc<Mutex<[MockSensorState; 4]>>,
}

/// Simulated state for a single sensor input channel.
#[derive(Clone)]
struct MockSensorState {
    port: &'static str,
    address: &'static str,
    driver_name: &'static str,
    mode: String,
    modes: Vec<String>,
    value0: f32,
    units: &'static str,
}

/// Simulated state and kinematics for a single motor output channel.
#[derive(Clone)]
struct MockMotorState {
    port: &'static str,
    driver_name: &'static str,
    max_speed: i32,
    count_per_rot: i32,
    speed_sp: i32,
    position: f64,
    target_pos: Option<i32>,
    running: bool,
    holding: bool,
    stop_action: String,
    polarity: String,
    last_tick: Instant,
}

impl MockMotorState {
    /// Creates baseline state for a simulated motor.
    ///
    /// Initializes zero speed, stopped state, and normal polarity.
    ///
    /// Helper used during `MockController::new` for ports A, B, C, and D.
    fn new(port: &'static str, driver_name: &'static str, max_speed: i32, now: Instant) -> Self {
        Self {
            port,
            driver_name,
            max_speed,
            count_per_rot: 360,
            speed_sp: 0,
            position: 0.0,
            target_pos: None,
            running: false,
            holding: false,
            stop_action: "brake".into(),
            polarity: "normal".into(),
            last_tick: now,
        }
    }
}

impl MockController {
    /// Creates a simulated controller with default motors and sensors.
    ///
    /// Configures 2 large motors (A, B) and 2 medium motors (C, D).
    /// Configures 4 standard EV3 sensors: touch (1), color (2), ultrasonic (3), gyro (4).
    ///
    /// Instantiated by `MotorController::new` when `--mock` is passed or sysfs is absent.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            motors: Arc::new(Mutex::new([
                MockMotorState::new("A", "lego-ev3-l-motor (mock)", 1050, now),
                MockMotorState::new("B", "lego-ev3-l-motor (mock)", 1050, now),
                MockMotorState::new("C", "lego-ev3-m-motor (mock)", 1560, now),
                MockMotorState::new("D", "lego-ev3-m-motor (mock)", 1560, now),
            ])),
            sensors: Arc::new(Mutex::new([
                MockSensorState {
                    port: "1",
                    address: "in1",
                    driver_name: "lego-ev3-touch (mock)",
                    mode: "TOUCH".into(),
                    modes: vec!["TOUCH".into()],
                    value0: 0.0,
                    units: "state",
                },
                MockSensorState {
                    port: "2",
                    address: "in2",
                    driver_name: "lego-ev3-color (mock)",
                    mode: "COL-COLOR".into(),
                    modes: vec!["COL-COLOR".into(), "COL-REFLECT".into(), "COL-AMBIENT".into()],
                    value0: 2.0,
                    units: "color_id",
                },
                MockSensorState {
                    port: "3",
                    address: "in3",
                    driver_name: "lego-ev3-us (mock)",
                    mode: "US-DIST-CM".into(),
                    modes: vec!["US-DIST-CM".into(), "US-DIST-IN".into()],
                    value0: 25.4,
                    units: "cm",
                },
                MockSensorState {
                    port: "4",
                    address: "in4",
                    driver_name: "lego-ev3-gyro (mock)",
                    mode: "GYRO-ANG".into(),
                    modes: vec!["GYRO-ANG".into(), "GYRO-RATE".into()],
                    value0: 0.0,
                    units: "deg",
                },
            ])),
        }
    }

    /// Runs a simulated motor continuously at the requested speed.
    ///
    /// Clamps speed to `max_speed`, marks `running = true`, and clears relative position targets.
    ///
    /// Mirrors `Motor::set_speed_sp` and `Motor::send_command("run-forever")` in memory.
    pub fn run_forever(&self, port: &str, speed: i32) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.speed_sp = speed.clamp(-motor.max_speed, motor.max_speed);
            motor.running = speed != 0;
            motor.holding = false;
            motor.target_pos = None;
            motor.last_tick = Instant::now();
        }
    }

    /// Steps a simulated motor by an incremental degree count.
    ///
    /// Sets `target_pos = current_position + rel_pos`, configures direction, and sets `running = true`.
    ///
    /// Mirrors `Motor::run_to_rel_pos` in memory for testing relative position commands.
    pub fn run_to_rel_pos(&self, port: &str, speed: i32, rel_pos: i32, stop_action: Option<String>) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            let target = (motor.position as i32) + rel_pos;
            let dir = if rel_pos >= 0 { 1 } else { -1 };
            motor.speed_sp = (speed.abs() * dir).clamp(-motor.max_speed, motor.max_speed);
            motor.target_pos = Some(target);
            motor.running = true;
            motor.holding = false;
            if let Some(action) = stop_action {
                motor.stop_action = action;
            }
            motor.last_tick = Instant::now();
        }
    }

    /// Stops a simulated motor on the specified port.
    ///
    /// Zeros target speed, sets `running = false`, and applies `stop_action` ("brake", "hold", or "coast").
    ///
    /// Mirrors `Motor::stop` in memory.
    pub fn stop(&self, port: &str, action: Option<String>) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.running = false;
            motor.speed_sp = 0;
            motor.target_pos = None;
            if let Some(act) = action {
                motor.stop_action = act;
            }
            motor.holding = motor.stop_action == "hold";
        }
    }

    /// Reads active stop action on a simulated motor port.
    ///
    /// Returns the current `stop_action` string for the port.
    ///
    /// Used by controller unit tests to verify that watchdog timeouts apply "coast".
    #[cfg(test)]
    pub fn get_stop_action(&self, port: &str) -> Option<String> {
        let m = self.motors.lock().unwrap();
        m.iter().find(|m| m.port.eq_ignore_ascii_case(port)).map(|m| m.stop_action.clone())
    }

    /// Resets encoder position and speed on a simulated motor port.
    ///
    /// Zeros position to 0.0, zeros speed, and marks `running = false`.
    ///
    /// Mirrors `Motor::send_command("reset")` in memory.
    pub fn reset(&self, port: &str) {
        let mut m = self.motors.lock().unwrap();
        if let Some(motor) = m.iter_mut().find(|m| m.port.eq_ignore_ascii_case(port)) {
            motor.running = false;
            motor.speed_sp = 0;
            motor.position = 0.0;
            motor.target_pos = None;
            motor.holding = false;
        }
    }

    /// Stops all simulated motors with the given stop action.
    ///
    /// Iterates through all 4 simulated motors, clearing running states and setting `stop_action`.
    ///
    /// Shared helper called by `emergency_stop` and `coast_all`.
    fn stop_all(&self, stop_action: &str, holding: bool) {
        let mut m = self.motors.lock().unwrap();
        for motor in m.iter_mut() {
            motor.running = false;
            motor.speed_sp = 0;
            motor.target_pos = None;
            motor.holding = holding;
            motor.stop_action = stop_action.into();
        }
    }

    /// Immediately stops all simulated motors with brake and hold.
    ///
    /// Calls `stop_all("hold", true)`.
    ///
    /// Mirrors hardware emergency stop in host simulation.
    pub fn emergency_stop(&self) {
        self.stop_all("hold", true);
    }

    /// Halts all simulated motors using coast action.
    ///
    /// Calls `stop_all("coast", false)`.
    ///
    /// Mirrors hardware watchdog timeout coast in host simulation.
    pub fn coast_all(&self) {
        self.stop_all("coast", false);
    }

    /// Checks if any simulated motor is currently moving.
    ///
    /// Returns true if any motor has `running = true` and `speed_sp != 0`.
    ///
    /// Used by watchdog logic to avoid unnecessary stop commands when motors are already idle.
    pub fn is_any_motor_running(&self) -> bool {
        let m = self.motors.lock().unwrap();
        m.iter().any(|motor| motor.running && motor.speed_sp != 0)
    }

    /// Updates simulated motor kinematics based on elapsed time and returns telemetry.
    ///
    /// 1. Calculates elapsed time `dt` since `last_tick`.
    /// 2. Integrates position: `position += speed_sp * dt`.
    /// 3. If a target position was set, stops motor once the target is reached.
    /// 4. Generates a list of `MotorStatus` records matching real hardware format.
    ///
    /// Called by `MotorController` polling loop every 50 ms to simulate continuous physics.
    pub fn poll_and_get_all_status(&self) -> Vec<MotorStatus> {
        let mut m = self.motors.lock().unwrap();
        let now = Instant::now();

        m.iter_mut().map(|motor| {
            let dt = now.duration_since(motor.last_tick).as_secs_f64();
            motor.last_tick = now;

            if motor.running {
                let delta_pos = (motor.speed_sp as f64) * dt;
                motor.position += delta_pos;

                // Handle relative position targets
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
                        motor.target_pos = None;
                        motor.holding = motor.stop_action == "hold";
                    }
                }
            }

            let duty_cycle = if motor.running && motor.max_speed > 0 {
                (motor.speed_sp * 100) / motor.max_speed
            } else {
                0
            };

            let mut state = Vec::new();
            if motor.running {
                state.push("running".to_string());
                if duty_cycle.abs() > 80 {
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
                duty_cycle,
                state,
                max_speed: motor.max_speed,
                count_per_rot: motor.count_per_rot,
                connected: true,
                polarity: motor.polarity.clone(),
            }
        }).collect()
    }

    /// Reads telemetry records for all simulated sensors.
    ///
    /// Iterates through the 4 simulated sensors and returns formatted `SensorStatus` snapshots.
    ///
    /// Called by `MotorController` polling loop every 50 ms.
    pub fn poll_and_get_all_sensors(&self) -> Vec<SensorStatus> {
        let s = self.sensors.lock().unwrap();
        s.iter()
            .map(|sensor| SensorStatus {
                port: sensor.port.to_string(),
                address: sensor.address.to_string(),
                driver_name: sensor.driver_name.to_string(),
                connected: true,
                mode: sensor.mode.clone(),
                modes: sensor.modes.clone(),
                value0: sensor.value0,
                units: sensor.units.to_string(),
            })
            .collect()
    }

    /// Updates the simulated sensor operating mode.
    ///
    /// Validates that `mode` is supported on the target sensor port.
    /// Sets new mode and updates test measurement units (e.g. "pct", "cm", "deg").
    ///
    /// Mirrors `Sensor::set_mode` for testing sensor mode changes in host simulation.
    pub fn set_sensor_mode(&self, port: &str, mode: &str) -> Result<(), String> {
        let mut s = self.sensors.lock().unwrap();
        if let Some(sensor) = s.iter_mut().find(|s| s.port.eq_ignore_ascii_case(port)) {
            let matched_mode = sensor
                .modes
                .iter()
                .find(|m| m.eq_ignore_ascii_case(mode))
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Mode '{}' not supported on port {}. Available: {:?}",
                        mode, port, sensor.modes
                    )
                })?;

            sensor.mode = matched_mode;
            match sensor.mode.as_str() {
                "COL-REFLECT" | "COL-AMBIENT" => {
                    sensor.units = "pct";
                    sensor.value0 = 42.0;
                }
                "COL-COLOR" => {
                    sensor.units = "color_id";
                    sensor.value0 = 2.0;
                }
                "US-DIST-CM" => {
                    sensor.units = "cm";
                    sensor.value0 = 25.4;
                }
                "US-DIST-IN" => {
                    sensor.units = "in";
                    sensor.value0 = 10.0;
                }
                "GYRO-ANG" => {
                    sensor.units = "deg";
                    sensor.value0 = 0.0;
                }
                "GYRO-RATE" => {
                    sensor.units = "dps";
                    sensor.value0 = 0.0;
                }
                _ => {}
            }
            Ok(())
        } else {
            Err(format!("Sensor port '{}' not found", port))
        }
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

    #[test]
    fn test_mock_sensors() {
        let mock = MockController::new();
        let sensors = mock.poll_and_get_all_sensors();
        assert_eq!(sensors.len(), 4);
        assert_eq!(sensors[0].port, "1");
        assert_eq!(sensors[0].mode, "TOUCH");
        assert_eq!(sensors[1].port, "2");
        assert_eq!(sensors[1].mode, "COL-COLOR");
        assert_eq!(sensors[2].port, "3");
        assert_eq!(sensors[2].units, "cm");
        assert_eq!(sensors[3].port, "4");
        assert_eq!(sensors[3].units, "deg");

        // Change mode on Port 2
        assert!(mock.set_sensor_mode("2", "COL-REFLECT").is_ok());
        let updated = mock.poll_and_get_all_sensors();
        assert_eq!(updated[1].mode, "COL-REFLECT");
        assert_eq!(updated[1].units, "pct");

        // Invalid mode error
        assert!(mock.set_sensor_mode("2", "NONEXISTENT").is_err());

        // Invalid port error
        assert!(mock.set_sensor_mode("9", "TOUCH").is_err());
    }
}
