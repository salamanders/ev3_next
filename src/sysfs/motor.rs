use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorStatus {
    pub port: String,
    pub address: String,
    pub driver_name: String,
    pub position: i32,
    pub speed: i32,
    pub duty_cycle: i32,
    pub state: Vec<String>,
    pub max_speed: i32,
    pub count_per_rot: i32,
    pub connected: bool,
    pub polarity: String,
}

impl MotorStatus {
    /// Generates an offline placeholder state for an unlinked channel.
    pub fn disconnected(port: &str) -> Self {
        Self {
            port: port.to_string(),
            address: format!("out{}", port),
            driver_name: "none".into(),
            position: 0,
            speed: 0,
            duty_cycle: 0,
            state: vec![],
            max_speed: 1050,
            count_per_rot: 360,
            connected: false,
            polarity: "normal".into(),
        }
    }
}

#[derive(Default)]
struct MotorState {
    polarity: String,
    position: Option<File>,
    speed: Option<File>,
    duty_cycle: Option<File>,
    state_file: Option<File>,
}

#[derive(Clone)]
pub struct Motor {
    pub port: String,
    pub sysfs_path: PathBuf,
    pub address: String,
    pub driver_name: String,
    pub max_speed: i32,
    pub count_per_rot: i32,
    state: Arc<Mutex<MotorState>>,
}

impl Motor {
    /// Scans standard Linux kernel classes for available actuators.
    pub fn find_all() -> Vec<Motor> {
        Self::find_all_in(Path::new("/sys/class/tacho-motor"))
    }

    /// Discovers and validates actuator sysfs trees under a designated root.
    pub fn find_all_in(base_path: &Path) -> Vec<Motor> {
        let mut motors = Vec::new();
        if !base_path.exists() {
            return motors;
        }

        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(addr) = fs::read_to_string(path.join("address")) {
                    let port = match crate::web::handlers::parse_motor_address(&addr) {
                        Some(p) => p,
                        None => continue,
                    };

                    let address = addr.trim().to_string();
                    let driver_name = fs::read_to_string(path.join("driver_name"))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|_| "tacho-motor".into());
                    let max_speed = fs::read_to_string(path.join("max_speed"))
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(1050);
                    let count_per_rot = fs::read_to_string(path.join("count_per_rot"))
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(360);
                    let polarity = fs::read_to_string(path.join("polarity"))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|_| "normal".into());

                    motors.push(Motor {
                        port: port.to_string(),
                        sysfs_path: path,
                        address,
                        driver_name,
                        max_speed,
                        count_per_rot,
                        state: Arc::new(Mutex::new(MotorState {
                            polarity,
                            ..Default::default()
                        })),
                    });
                }
            }
        }

        // Sort by port name A, B, C, D
        motors.sort_by(|a, b| a.port.cmp(&b.port));
        motors
    }

    /// Obtains currently synchronized directional rotation configuration.
    pub fn polarity(&self) -> String {
        self.state.lock().map(|g| g.polarity.clone()).unwrap_or_else(|_| "normal".into())
    }

    /// Writes clamped velocity target values to the kernel interface.
    pub fn set_speed_sp(&self, speed: i32) -> io::Result<()> {
        let clamped = speed.clamp(-self.max_speed, self.max_speed);
        self.write_attr("speed_sp", &clamped.to_string())
    }

    /// Configures pulse-width modulation limits on the output driver.
    pub fn set_duty_cycle_sp(&self, duty: i32) -> io::Result<()> {
        let clamped = duty.clamp(-100, 100);
        self.write_attr("duty_cycle_sp", &clamped.to_string())
    }

    /// Commits angular step displacement goals into the kernel node.
    pub fn set_position_sp(&self, pos: i32) -> io::Result<()> {
        self.write_attr("position_sp", &pos.to_string())
    }

    /// Designates active rotation durations for timed kinematic cycles.
    pub fn set_time_sp(&self, ms: u32) -> io::Result<()> {
        self.write_attr("time_sp", &ms.to_string())
    }

    /// Assigns the terminal deceleration behavior upon motion completion.
    pub fn set_stop_action(&self, action: &str) -> io::Result<()> {
        self.write_attr("stop_action", action)
    }

    /// Dispatches operational directives to trigger driver execution.
    pub fn send_command(&self, cmd: &str) -> io::Result<()> {
        self.write_attr("command", cmd)
    }

    /// Flips rotational sign conventions across hardware registers.
    pub fn set_polarity(&self, polarity: &str) -> io::Result<()> {
        let val = if polarity == "inversed" { "inversed" } else { "normal" };
        self.write_attr("polarity", val)?;
        if let Ok(mut st) = self.state.lock() {
            st.polarity = val.to_string();
        }
        Ok(())
    }

    // Deserializes signed integer attribute from buffered descriptor.
    fn read_i32(file_opt: &mut Option<File>, path: &Path, buf: &mut [u8]) -> Option<i32> {
        let n = crate::sysfs::read_sysfs_cached(file_opt, path, buf).ok()?;
        std::str::from_utf8(&buf[..n]).ok()?.trim().parse().ok()
    }

    /// Ingests live telemetry metrics using cached descriptors and buffers.
    pub fn poll_dynamic_status(&self) -> Option<MotorStatus> {
        if !self.sysfs_path.exists() {
            return None;
        }

        let mut st = self.state.lock().ok()?;
        let mut buf = [0u8; 64];

        let position = Self::read_i32(&mut st.position, &self.sysfs_path.join("position"), &mut buf)?;
        let speed = Self::read_i32(&mut st.speed, &self.sysfs_path.join("speed"), &mut buf).unwrap_or(0);
        let duty_cycle = Self::read_i32(&mut st.duty_cycle, &self.sysfs_path.join("duty_cycle"), &mut buf).unwrap_or(0);

        let state = if let Ok(n) = crate::sysfs::read_sysfs_cached(
            &mut st.state_file,
            &self.sysfs_path.join("state"),
            &mut buf,
        ) {
            std::str::from_utf8(&buf[..n])
                .unwrap_or_default()
                .split_whitespace()
                .map(String::from)
                .collect()
        } else {
            Vec::new()
        };

        let polarity = st.polarity.clone();

        Some(MotorStatus {
            port: self.port.clone(),
            address: self.address.clone(),
            driver_name: self.driver_name.clone(),
            position,
            speed,
            duty_cycle,
            state,
            max_speed: self.max_speed,
            count_per_rot: self.count_per_rot,
            connected: true,
            polarity,
        })
    }

    /// Returns current measurements with safe fallback upon disconnects.
    pub fn read_status(&self) -> MotorStatus {
        self.poll_dynamic_status().unwrap_or_else(|| MotorStatus {
            port: self.port.clone(),
            address: self.address.clone(),
            driver_name: self.driver_name.clone(),
            position: 0,
            speed: 0,
            duty_cycle: 0,
            state: vec![],
            max_speed: self.max_speed,
            count_per_rot: self.count_per_rot,
            connected: false,
            polarity: self.polarity(),
        })
    }

    /// Dispatches string payloads into designated kernel file paths.
    fn write_attr(&self, attr: &str, val: &str) -> io::Result<()> {
        let path = self.sysfs_path.join(attr);
        let mut file = OpenOptions::new().write(true).open(&path)?;
        file.write_all(val.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Constructs isolated scratch filesystem trees for hardware mocks.
    fn make_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("ev3_test_{}_{}", name, nanos));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    // Verifies tree inspection identifies valid hardware ports correctly.
    #[test]
    fn test_real_motor_discovery_and_address_parsing() {
        let root = make_test_dir("discovery");
        let m0 = root.join("motor0");
        let m1 = root.join("motor1");
        let m2 = root.join("motor2");
        fs::create_dir_all(&m0).unwrap();
        fs::create_dir_all(&m1).unwrap();
        fs::create_dir_all(&m2).unwrap();

        // motor0 -> ev3-ports:outA
        fs::write(m0.join("address"), "ev3-ports:outA\n").unwrap();
        fs::write(m0.join("driver_name"), "lego-ev3-l-motor\n").unwrap();
        fs::write(m0.join("max_speed"), "1050\n").unwrap();
        fs::write(m0.join("count_per_rot"), "360\n").unwrap();
        fs::write(m0.join("polarity"), "normal\n").unwrap();

        // motor1 -> outB
        fs::write(m1.join("address"), "outB\n").unwrap();
        fs::write(m1.join("driver_name"), "lego-ev3-m-motor\n").unwrap();
        fs::write(m1.join("max_speed"), "1560\n").unwrap();
        fs::write(m1.join("count_per_rot"), "360\n").unwrap();
        fs::write(m1.join("polarity"), "inversed\n").unwrap();

        // motor2 -> unknown port, should be skipped
        fs::write(m2.join("address"), "unknown:outZ\n").unwrap();

        let motors = Motor::find_all_in(&root);
        assert_eq!(motors.len(), 2);
        assert_eq!(motors[0].port, "A");
        assert_eq!(motors[0].max_speed, 1050);
        assert_eq!(motors[0].driver_name, "lego-ev3-l-motor");

        assert_eq!(motors[1].port, "B");
        assert_eq!(motors[1].max_speed, 1560);
        assert_eq!(motors[1].driver_name, "lego-ev3-m-motor");

        let _ = fs::remove_dir_all(&root);
    }

    // Asserts velocity thresholds truncate out-of-range register writes.
    #[test]
    fn test_real_motor_clamping_and_commands() {
        let root = make_test_dir("commands");
        let m = root.join("motor0");
        fs::create_dir_all(&m).unwrap();
        fs::write(m.join("address"), "outA\n").unwrap();
        fs::write(m.join("max_speed"), "1000\n").unwrap();

        // Attribute files for writing
        fs::write(m.join("speed_sp"), "0\n").unwrap();
        fs::write(m.join("duty_cycle_sp"), "0\n").unwrap();
        fs::write(m.join("command"), "\n").unwrap();
        fs::write(m.join("polarity"), "normal\n").unwrap();

        let mut motors = Motor::find_all_in(&root);
        assert_eq!(motors.len(), 1);
        let motor = &mut motors[0];

        // Clamping high speed
        motor.set_speed_sp(2000).unwrap();
        let speed_sp = fs::read_to_string(m.join("speed_sp")).unwrap();
        assert_eq!(speed_sp.trim(), "1000");

        // Clamping negative speed
        motor.set_speed_sp(-1500).unwrap();
        let speed_sp_neg = fs::read_to_string(m.join("speed_sp")).unwrap();
        assert_eq!(speed_sp_neg.trim(), "-1000");

        // Clamping duty cycle
        motor.set_duty_cycle_sp(150).unwrap();
        let duty_sp = fs::read_to_string(m.join("duty_cycle_sp")).unwrap();
        assert_eq!(duty_sp.trim(), "100");

        // Command write
        motor.send_command("run-forever").unwrap();
        let cmd = fs::read_to_string(m.join("command")).unwrap();
        assert_eq!(cmd.trim(), "run-forever");

        // Polarity write
        motor.set_polarity("inversed").unwrap();
        let pol = fs::read_to_string(m.join("polarity")).unwrap();
        assert_eq!(pol.trim(), "inversed");
        assert_eq!(motor.polarity(), "inversed");

        let _ = fs::remove_dir_all(&root);
    }

    // Confirms telemetry cache refreshes while unlinking cleanly on unplug.
    #[test]
    fn test_real_motor_dynamic_polling_and_disconnected() {
        let root = make_test_dir("polling");
        let m = root.join("motor0");
        fs::create_dir_all(&m).unwrap();
        fs::write(m.join("address"), "outC\n").unwrap();

        // Write telemetry files
        fs::write(m.join("position"), "240\n").unwrap();
        fs::write(m.join("speed"), "450\n").unwrap();
        fs::write(m.join("duty_cycle"), "50\n").unwrap();
        fs::write(m.join("state"), "running holding\n").unwrap();

        let motors = Motor::find_all_in(&root);
        assert_eq!(motors.len(), 1);
        let motor = &motors[0];

        let status = motor.poll_dynamic_status().expect("Must poll status successfully");
        assert_eq!(status.port, "C");
        assert_eq!(status.position, 240);
        assert_eq!(status.speed, 450);
        assert_eq!(status.duty_cycle, 50);
        assert!(status.state.contains(&"running".to_string()));
        assert!(status.state.contains(&"holding".to_string()));
        assert!(status.connected);

        // Remove motor directory to simulate device disconnect / node unbind
        fs::remove_dir_all(&m).unwrap();
        let disconnected_status = motor.read_status();
        assert!(!disconnected_status.connected);

        let _ = fs::remove_dir_all(&root);
    }
}
