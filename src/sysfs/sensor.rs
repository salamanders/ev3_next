//! # Linux Lego Sensor Driver Interface
//!
//! This module reads sensor measurements from input ports 1, 2, 3, and 4 on the EV3 brick.
//! It discovers devices under `/sys/class/lego-sensor/sensorN`, reads scaled values, and changes sensor modes.
//!
//! Sensors are input peripherals with mode switching (`COL-COLOR`, `US-DIST-CM`) and decimal scaling.
//! Separating sensor logic from `motor.rs` isolates input processing from motor commands.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

/// Snapshot of sensor readings for a single input port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorStatus {
    pub port: String,
    pub address: String,
    pub driver_name: String,
    pub connected: bool,
    pub mode: String,
    pub modes: Vec<String>,
    pub value0: f32,
    pub units: String,
}

impl SensorStatus {
    /// Creates an offline placeholder status for a disconnected sensor port.
    ///
    /// Returns default zero values with `connected = false` and `driver_name = "none"`.
    ///
    /// Shared by `Sensor::read_status` and `MotorController` when an expected port has no connected sensor.
    pub fn disconnected(port: &str) -> Self {
        Self {
            port: port.to_string(),
            address: format!("in{}", port),
            driver_name: "none".into(),
            connected: false,
            mode: "".into(),
            modes: vec![],
            value0: 0.0,
            units: "".into(),
        }
    }
}

/// Internal sensor state storing active mode and cached file descriptor.
#[derive(Default)]
struct SensorState {
    mode: String,
    decimals: u32,
    units: String,
    cached_file: Option<File>,
}

/// Handle to a physical EV3 sensor discovered in sysfs.
#[derive(Clone)]
pub struct Sensor {
    pub port: String,
    pub sysfs_path: PathBuf,
    pub address: String,
    pub driver_name: String,
    pub modes: Vec<String>,
    state: Arc<Mutex<SensorState>>,
}

impl Sensor {
    /// Scans standard Linux kernel path for connected sensors.
    ///
    /// Calls `find_all_in` with the standard ev3dev path `/sys/class/lego-sensor`.
    ///
    /// Provides the standard production entry point while keeping `find_all_in` testable.
    pub fn find_all() -> Vec<Sensor> {
        Self::find_all_in(Path::new("/sys/class/lego-sensor"))
    }

    /// Discovers all sensors in a designated directory.
    ///
    /// Iterates through sensor subdirectories, parses port addresses ("in1".."in4"),
    /// reads device parameters (`driver_name`, `mode`, `modes`, `decimals`, `units`),
    /// and returns a sorted list of `Sensor` instances.
    ///
    /// Allows unit tests to scan a temporary test directory with mock sysfs files
    /// without requiring root access or a physical EV3 brick.
    pub fn find_all_in(base_path: &Path) -> Vec<Sensor> {
        let mut sensors = Vec::new();
        if !base_path.exists() {
            return sensors;
        }

        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(addr) = fs::read_to_string(path.join("address")) {
                    let port = match super::parse_sensor_address(&addr) {
                        Some(p) => p,
                        None => continue,
                    };

                    let address = addr.trim().to_string();
                    let driver_name = fs::read_to_string(path.join("driver_name"))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|_| "lego-sensor".into());
                    let mode = fs::read_to_string(path.join("mode"))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();
                    let modes: Vec<String> = fs::read_to_string(path.join("modes"))
                        .map(|s| s.split_whitespace().map(|m| m.to_string()).collect())
                        .unwrap_or_default();
                    let decimals = fs::read_to_string(path.join("decimals"))
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0);
                    let units = fs::read_to_string(path.join("units"))
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default();

                    sensors.push(Sensor {
                        port: port.to_string(),
                        sysfs_path: path,
                        address,
                        driver_name,
                        modes,
                        state: Arc::new(Mutex::new(SensorState {
                            mode,
                            decimals,
                            units,
                            cached_file: None,
                        })),
                    });
                }
            }
        }

        // Sort by port number: "1", "2", "3", "4"
        sensors.sort_by(|a, b| a.port.cmp(&b.port));
        sensors
    }

    /// Changes the operating mode of the sensor.
    ///
    /// Writes the new mode string to `/sys/class/lego-sensor/sensorN/mode`,
    /// resets the cached file descriptor, and re-reads `decimals` and `units`.
    ///
    /// Called when the user changes a sensor mode in the browser UI (e.g. from `COL-REFLECT` to `COL-COLOR`).
    /// Changing modes alters the unit scale and decimal exponent in kernel sysfs.
    pub fn set_mode(&self, mode: &str) -> io::Result<()> {
        let mut st = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        st.cached_file = None;

        let mut file = OpenOptions::new().write(true).open(self.sysfs_path.join("mode"))?;
        file.write_all(mode.as_bytes())?;
        file.flush()?;

        st.decimals = fs::read_to_string(self.sysfs_path.join("decimals"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        st.units = fs::read_to_string(self.sysfs_path.join("units"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        st.mode = mode.to_string();

        Ok(())
    }

    /// Reads current sensor value scaled by the decimal exponent.
    ///
    /// 1. Reads raw integer string from `/sys/class/lego-sensor/sensorN/value0` using `read_sysfs_cached`.
    /// 2. Scales the integer using `decimals`: for example, `raw_val = 254` and `decimals = 1` yields `25.4`.
    /// 3. Returns a complete `SensorStatus` record with active `mode` and `units`.
    ///
    /// Called by the background polling thread in `MotorController` every 50 ms.
    /// Uses cached file descriptors to eliminate repeated file open syscalls on the ARM9 CPU.
    pub fn read_status(&self) -> SensorStatus {
        let mut st = match self.state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mode = st.mode.clone();
        let decimals = st.decimals;
        let units = st.units.clone();

        let mut buf = [0u8; 32];
        let path = self.sysfs_path.join("value0");
        let (connected, value0) = match crate::sysfs::read_sysfs_cached(&mut st.cached_file, &path, &mut buf) {
            Ok(n) => {
                let parsed = std::str::from_utf8(&buf[..n])
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok());
                match parsed {
                    Some(raw_val) => {
                        let val = if decimals > 0 {
                            raw_val as f32 / 10f32.powi(decimals as i32)
                        } else {
                            raw_val as f32
                        };
                        (true, val)
                    }
                    None => {
                        st.cached_file = None;
                        (false, 0.0)
                    }
                }
            }
            Err(_) => (false, 0.0),
        };

        SensorStatus {
            port: self.port.clone(),
            address: self.address.clone(),
            driver_name: self.driver_name.clone(),
            connected,
            mode,
            modes: self.modes.clone(),
            value0,
            units,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_discovery_and_readings() {
        let temp_dir = std::env::temp_dir().join(format!("test_ev3_sensor_{}", std::process::id()));
        let s1_dir = temp_dir.join("sensor0");
        let s2_dir = temp_dir.join("sensor1");
        fs::create_dir_all(&s1_dir).unwrap();
        fs::create_dir_all(&s2_dir).unwrap();

        // Sensor 1: Touch on in1
        fs::write(s1_dir.join("address"), "ev3-ports:in1\n").unwrap();
        fs::write(s1_dir.join("driver_name"), "lego-ev3-touch\n").unwrap();
        fs::write(s1_dir.join("mode"), "TOUCH\n").unwrap();
        fs::write(s1_dir.join("modes"), "TOUCH\n").unwrap();
        fs::write(s1_dir.join("decimals"), "0\n").unwrap();
        fs::write(s1_dir.join("units"), "state\n").unwrap();
        fs::write(s1_dir.join("value0"), "1\n").unwrap();

        // Ultrasonic on in2 with 1 decimal place (254 -> 25.4 cm).
        fs::write(s2_dir.join("address"), "ev3-ports:in2\n").unwrap();
        fs::write(s2_dir.join("driver_name"), "lego-ev3-us\n").unwrap();
        fs::write(s2_dir.join("mode"), "US-DIST-CM\n").unwrap();
        fs::write(s2_dir.join("modes"), "US-DIST-CM US-DIST-IN\n").unwrap();
        fs::write(s2_dir.join("decimals"), "1\n").unwrap();
        fs::write(s2_dir.join("units"), "cm\n").unwrap();
        fs::write(s2_dir.join("value0"), "254\n").unwrap();

        let sensors = Sensor::find_all_in(&temp_dir);
        assert_eq!(sensors.len(), 2);

        let s1 = sensors.iter().find(|s| s.port == "1").unwrap();
        let st1 = s1.read_status();
        assert_eq!(st1.driver_name, "lego-ev3-touch");
        assert_eq!(st1.value0, 1.0);
        assert_eq!(st1.units, "state");

        let s2 = sensors.iter().find(|s| s.port == "2").unwrap();
        let st2 = s2.read_status();
        assert_eq!(st2.driver_name, "lego-ev3-us");
        assert_eq!(st2.value0, 25.4);
        assert_eq!(st2.units, "cm");
        assert!(st2.connected);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_sensor_mode_change_updates_decimals_and_units() {
        let temp_dir = std::env::temp_dir().join(format!("test_ev3_sensor_mode_{}", std::process::id()));
        let s_dir = temp_dir.join("sensor0");
        fs::create_dir_all(&s_dir).unwrap();

        fs::write(s_dir.join("address"), "ev3-ports:in2\n").unwrap();
        fs::write(s_dir.join("driver_name"), "lego-ev3-us\n").unwrap();
        fs::write(s_dir.join("mode"), "US-DIST-CM\n").unwrap();
        fs::write(s_dir.join("modes"), "US-DIST-CM US-DIST-IN\n").unwrap();
        fs::write(s_dir.join("decimals"), "1\n").unwrap();
        fs::write(s_dir.join("units"), "cm\n").unwrap();
        fs::write(s_dir.join("value0"), "254\n").unwrap();

        let sensors = Sensor::find_all_in(&temp_dir);
        let s = &sensors[0];
        assert_eq!(s.read_status().mode, "US-DIST-CM");

        // Simulate changing mode to US-DIST-IN
        fs::write(s_dir.join("mode"), "US-DIST-IN\n").unwrap();
        fs::write(s_dir.join("decimals"), "0\n").unwrap();
        fs::write(s_dir.join("units"), "in\n").unwrap();
        fs::write(s_dir.join("value0"), "10\n").unwrap();

        s.set_mode("US-DIST-IN").unwrap();
        let updated = s.read_status();
        assert_eq!(updated.mode, "US-DIST-IN");
        assert_eq!(updated.units, "in");
        assert_eq!(updated.value0, 10.0);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
