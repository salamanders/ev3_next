//! # Linux Tacho-Motor Driver Interface
//!
//! This module provides direct sysfs control for LEGO Mindstorms EV3 motors connected to output ports A, B, C, and D.
//! It discovers devices under `/sys/class/tacho-motor/motorN`, reads dynamic metrics, and writes command attributes.
//!
//! Motors have active command files (`command`, `speed_sp`, `stop_action`) and dual status metrics (speed, position).
//! Keeping motor logic in `motor.rs` separates motor control from read-only sensor input in `sensor.rs`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

/// Snapshot of motor telemetry for a single output port.
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
    /// Creates an offline placeholder status for a disconnected motor port.
    ///
    /// Returns default zero values with `connected = false` and `driver_name = "none"`.
    ///
    /// Shared by `Motor::read_status` and `MotorController` when an expected port has no connected motor.
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

/// Internal cached file handles for fast periodic polling.
#[derive(Default)]
struct MotorState {
    polarity: String,
    position: Option<File>,
    speed: Option<File>,
    duty_cycle: Option<File>,
    state_file: Option<File>,
}

/// Handle to a physical EV3 motor discovered in sysfs.
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
    /// Scans standard Linux kernel path for connected motors.
    ///
    /// Calls `find_all_in` with the standard ev3dev path `/sys/class/tacho-motor`.
    ///
    /// Provides the standard production entry point while keeping `find_all_in` testable.
    pub fn find_all() -> Vec<Motor> {
        Self::find_all_in(Path::new("/sys/class/tacho-motor"))
    }

    /// Discovers all motors in a designated directory.
    ///
    /// Iterates through motor subdirectories, parses port addresses ("outA".."outD"),
    /// reads device parameters (`driver_name`, `max_speed`, `count_per_rot`, `polarity`),
    /// and returns a sorted list of `Motor` instances.
    ///
    /// Allows unit tests to scan a temporary test directory with mock sysfs files
    /// without requiring root access or a physical EV3 brick.
    pub fn find_all_in(base_path: &Path) -> Vec<Motor> {
        let mut motors = Vec::new();
        if !base_path.exists() {
            return motors;
        }

        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(addr) = fs::read_to_string(path.join("address")) {
                    let port = match super::parse_motor_address(&addr) {
                        Some(p) => p,
                        None => continue,
                    };

                    let address = addr.trim().to_string();
                    let driver_name = super::read_attr_trimmed(&path, "driver_name", "tacho-motor");
                    let max_speed = super::read_attr_parsed(&path, "max_speed", 1050);
                    let count_per_rot = super::read_attr_parsed(&path, "count_per_rot", 360);
                    let polarity = super::read_attr_trimmed(&path, "polarity", "normal");

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

        // Sort alphabetically by port name: A, B, C, D
        motors.sort_by(|a, b| a.port.cmp(&b.port));
        motors
    }

    /// Sets the target speed attribute in sysfs.
    ///
    /// Clamps the speed to `[-max_speed, max_speed]` and writes to `/sys/class/tacho-motor/motorN/speed_sp`.
    ///
    /// Clamping avoids kernel `EINVAL` errors from out-of-range speed values.
    pub fn set_speed_sp(&self, speed: i32) -> io::Result<()> {
        let clamped = speed.clamp(-self.max_speed, self.max_speed);
        self.write_attr("speed_sp", &clamped.to_string())
    }

    /// Sets the target relative position attribute in sysfs.
    ///
    /// Writes degree counts to `/sys/class/tacho-motor/motorN/position_sp`.
    ///
    /// Called by `MotorController::run_to_rel_pos` before dispatching `run-to-rel-pos` command.
    pub fn set_position_sp(&self, pos: i32) -> io::Result<()> {
        self.write_attr("position_sp", &pos.to_string())
    }

    /// Sets the stopping mode attribute in sysfs.
    ///
    /// Writes "coast", "brake", or "hold" to `/sys/class/tacho-motor/motorN/stop_action`.
    ///
    /// Called before issuing stop or movement commands to select braking behavior.
    pub fn set_stop_action(&self, action: &str) -> io::Result<()> {
        self.write_attr("stop_action", action)
    }

    /// Writes an operational command to trigger motor execution.
    ///
    /// Writes command strings like "run-forever", "run-to-rel-pos", "stop", or "reset"
    /// to `/sys/class/tacho-motor/motorN/command`.
    ///
    /// Serves as the motor driver trigger interface called by controller action methods.
    pub fn send_command(&self, cmd: &str) -> io::Result<()> {
        self.write_attr("command", cmd)
    }

    /// Reads and parses a 32-bit signed integer from a cached sysfs attribute.
    ///
    /// Uses `read_sysfs_cached` to read up to 64 bytes into `buf` and parses it as `i32`.
    ///
    /// Shared by `read_status` for position, speed, and duty_cycle attributes.
    fn read_i32(file_opt: &mut Option<File>, path: &Path, buf: &mut [u8]) -> Option<i32> {
        let n = crate::sysfs::read_sysfs_cached(file_opt, path, buf).ok()?;
        std::str::from_utf8(&buf[..n]).ok()?.trim().parse().ok()
    }

    /// Ingests live telemetry metrics using cached file descriptors.
    ///
    /// 1. Verifies that `sysfs_path` still exists on disk.
    /// 2. Reads `position`, `speed`, `duty_cycle`, and `state` attributes using cached file handles.
    /// 3. If reading `position` fails (motor unplugged), returns `MotorStatus::disconnected(&self.port)`.
    ///
    /// Called by the background polling thread in `MotorController` every 50 ms.
    /// Polling logic is merged directly here so all dynamic motor reading happens in one place.
    pub fn read_status(&self) -> MotorStatus {
        if !self.sysfs_path.exists() {
            return MotorStatus::disconnected(&self.port);
        }

        let mut st = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut buf = [0u8; 64];

        let position = match Self::read_i32(&mut st.position, &self.sysfs_path.join("position"), &mut buf) {
            Some(pos) => pos,
            None => return MotorStatus::disconnected(&self.port),
        };

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

        MotorStatus {
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
        }
    }

    /// Writes an attribute value to a motor sysfs file.
    ///
    /// Opens the specified attribute file for writing, writes the text value, and adds port context on error.
    ///
    /// Shared by `set_speed_sp`, `set_position_sp`, `set_stop_action`, and `send_command`.
    /// Adding port and attribute context prevents bare Linux kernel `os error 22 (EINVAL)` messages.
    fn write_attr(&self, attr: &str, val: &str) -> io::Result<()> {
        let path = self.sysfs_path.join(attr);
        let mut file = OpenOptions::new().write(true).open(&path).map_err(|e| {
            io::Error::new(e.kind(), format!("Port {}: failed to open {}: {}", self.port, attr, e))
        })?;
        file.write_all(val.as_bytes()).map_err(|e| {
            io::Error::new(e.kind(), format!("Port {}: failed to write '{}' to {}: {}", self.port, val, attr, e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Creates an isolated temporary directory for test sysfs trees.
    fn make_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("ev3_test_{}_{}", name, nanos));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

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

    #[test]
    fn test_real_motor_clamping_and_commands() {
        let root = make_test_dir("commands");
        let m = root.join("motor0");
        fs::create_dir_all(&m).unwrap();
        fs::write(m.join("address"), "outA\n").unwrap();
        fs::write(m.join("max_speed"), "1000\n").unwrap();

        // Attribute files for writing
        fs::write(m.join("speed_sp"), "0\n").unwrap();
        fs::write(m.join("position_sp"), "0\n").unwrap();
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

        // Position setpoint write
        motor.set_position_sp(360).unwrap();
        let pos_sp = fs::read_to_string(m.join("position_sp")).unwrap();
        assert_eq!(pos_sp.trim(), "360");

        // Command write
        motor.send_command("run-forever").unwrap();
        let cmd = fs::read_to_string(m.join("command")).unwrap();
        assert_eq!(cmd.trim(), "run-forever");

        let _ = fs::remove_dir_all(&root);
    }

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

        let status = motor.read_status();
        assert!(status.connected, "Must read status successfully");
        assert_eq!(status.port, "C");
        assert_eq!(status.position, 240);
        assert_eq!(status.speed, 450);
        assert_eq!(status.duty_cycle, 50);
        assert!(status.state.contains(&"running".to_string()));
        assert!(status.state.contains(&"holding".to_string()));

        // Remove motor directory to simulate device disconnect / cable unplug
        fs::remove_dir_all(&m).unwrap();
        let disconnected_status = motor.read_status();
        assert!(!disconnected_status.connected);

        let _ = fs::remove_dir_all(&root);
    }
}
