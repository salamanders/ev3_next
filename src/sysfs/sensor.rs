use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

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
    /// Generates offline placeholder representation for unlinked input.
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

#[derive(Default)]
struct SensorState {
    mode: String,
    decimals: u32,
    units: String,
    cached_file: Option<File>,
}

#[derive(Clone)]
pub struct Sensor {
    pub port: String, // "1", "2", "3", "4"
    pub sysfs_path: PathBuf,
    pub address: String,
    pub driver_name: String,
    pub modes: Vec<String>,
    state: Arc<Mutex<SensorState>>,
}

impl Sensor {
    /// Scans standard Linux kernel classes for attached input devices.
    pub fn find_all() -> Vec<Sensor> {
        Self::find_all_in(Path::new("/sys/class/lego-sensor"))
    }

    /// Inspects targeted directory trees to construct device representations.
    pub fn find_all_in(base_path: &Path) -> Vec<Sensor> {
        let mut sensors = Vec::new();
        if !base_path.exists() {
            return sensors;
        }

        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(addr) = fs::read_to_string(path.join("address")) {
                    let port = match crate::web::handlers::parse_sensor_address(&addr) {
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

        sensors.sort_by(|a, b| a.port.cmp(&b.port));
        sensors
    }

    /// Switches hardware capture configuration and re-queries scale factors.
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

    /// Computes scaled measurements with fallback for unplugged hardware.
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

    // Verifies tree inspection with fixed-point telemetry normalization.
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

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    // Validates reconfiguring capture schemes refreshes scaling exponents.
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
        let s = sensors.iter().find(|s| s.port == "2").unwrap();
        let st = s.read_status();
        assert_eq!(st.mode, "US-DIST-CM");
        assert_eq!(st.units, "cm");
        assert_eq!(st.value0, 25.4);

        // Simulate sysfs driver behavior when mode changes:
        // Updates decimals to 2, units to "in", value0 to "1000".
        fs::write(s_dir.join("decimals"), "2\n").unwrap();
        fs::write(s_dir.join("units"), "in\n").unwrap();
        fs::write(s_dir.join("value0"), "1000\n").unwrap();

        s.set_mode("US-DIST-IN").unwrap();

        let st2 = s.read_status();
        assert_eq!(st2.mode, "US-DIST-IN");
        assert_eq!(st2.units, "in");
        assert_eq!(st2.value0, 10.0);

        // Simulate mode change where decimals and units are omitted in sysfs
        let _ = fs::remove_file(s_dir.join("decimals"));
        let _ = fs::remove_file(s_dir.join("units"));
        fs::write(s_dir.join("value0"), "42\n").unwrap();
        s.set_mode("RAW").unwrap();

        let st3 = s.read_status();
        assert_eq!(st3.mode, "RAW");
        assert_eq!(st3.units, "");
        assert_eq!(st3.value0, 42.0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    // Asserts empty payloads trigger offline flags while purging open handles.
    #[test]
    fn test_sensor_disconnected_and_cached_fd_reset_on_error() {
        let temp_dir = std::env::temp_dir().join(format!("test_ev3_sensor_disc_{}", std::process::id()));
        let s_dir = temp_dir.join("sensor0");
        fs::create_dir_all(&s_dir).unwrap();

        fs::write(s_dir.join("address"), "ev3-ports:in1\n").unwrap();
        fs::write(s_dir.join("driver_name"), "lego-ev3-touch\n").unwrap();
        fs::write(s_dir.join("mode"), "TOUCH\n").unwrap();
        fs::write(s_dir.join("modes"), "TOUCH\n").unwrap();
        fs::write(s_dir.join("decimals"), "0\n").unwrap();
        fs::write(s_dir.join("units"), "state\n").unwrap();
        fs::write(s_dir.join("value0"), "1\n").unwrap();

        let sensors = Sensor::find_all_in(&temp_dir);
        let s = sensors.iter().find(|s| s.port == "1").unwrap();

        // 1. Initially connected
        let st = s.read_status();
        assert!(st.connected);
        assert_eq!(st.value0, 1.0);

        // 2. Disconnect (empty sysfs value0 on unplug / EOF)
        fs::write(s_dir.join("value0"), "").unwrap();
        let st_disc = s.read_status();
        assert!(!st_disc.connected);
        assert_eq!(st_disc.value0, 0.0);

        // 3. Reconnect (value0 populated again, cached fd reopens).
        fs::write(s_dir.join("value0"), "0\n").unwrap();
        let st_rec = s.read_status();
        assert!(st_rec.connected);
        assert_eq!(st_rec.value0, 0.0);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
