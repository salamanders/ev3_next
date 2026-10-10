use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crate::web::handlers::SensorStatus;

#[derive(Default)]
struct CachedSensorFiles {
    value0: Option<File>,
}

#[derive(Clone)]
pub struct Sensor {
    pub port: String, // "1", "2", "3", "4"
    pub sysfs_path: PathBuf,
    pub address: String,
    pub driver_name: String,
    mode: Arc<Mutex<String>>,
    pub modes: Vec<String>,
    pub decimals: u32,
    pub units: String,
    cached_files: Arc<Mutex<CachedSensorFiles>>,
}

impl Sensor {
    /// Enumerate connected sensors from default sysfs path
    pub fn find_all() -> Vec<Sensor> {
        Self::find_all_in(Path::new("/sys/class/lego-sensor"))
    }

    /// Enumerate connected sensors from specified base path (supports testing and hardware)
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
                        mode: Arc::new(Mutex::new(mode)),
                        modes,
                        decimals,
                        units,
                        cached_files: Arc::new(Mutex::new(CachedSensorFiles::default())),
                    });
                }
            }
        }

        sensors.sort_by(|a, b| a.port.cmp(&b.port));
        sensors
    }

    pub fn set_mode(&self, mode: &str) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).open(self.sysfs_path.join("mode"))?;
        file.write_all(mode.as_bytes())?;
        file.flush()?;
        if let Ok(mut m) = self.mode.lock() {
            *m = mode.to_string();
        }
        if let Ok(mut cf) = self.cached_files.lock() {
            cf.value0 = None;
        }
        Ok(())
    }

    pub fn read_status(&self) -> SensorStatus {
        let mode = self.mode.lock().map(|g| g.clone()).unwrap_or_default();
        let raw_val = self.read_value0().unwrap_or(0);
        let value0 = if self.decimals > 0 {
            let divisor = 10f32.powi(self.decimals as i32);
            raw_val as f32 / divisor
        } else {
            raw_val as f32
        };

        SensorStatus {
            port: self.port.clone(),
            address: self.address.clone(),
            driver_name: self.driver_name.clone(),
            connected: true,
            mode,
            modes: self.modes.clone(),
            value0,
            units: self.units.clone(),
        }
    }

    fn read_value0(&self) -> io::Result<i32> {
        let mut buf = [0u8; 32];
        let mut cf_guard = self.cached_files.lock().unwrap();
        let path = self.sysfs_path.join("value0");

        if cf_guard.value0.is_none() {
            cf_guard.value0 = Some(File::open(&path)?);
        }

        if let Some(file) = cf_guard.value0.as_mut() {
            file.seek(SeekFrom::Start(0))?;
            let n = file.read(&mut buf)?;
            if n > 0 {
                let s = std::str::from_utf8(&buf[..n])
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                return s.trim().parse::<i32>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
            }
        }
        Err(io::Error::other("Could not read value0"))
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

        // Sensor 2: Ultrasonic on in2 with 1 decimal place (e.g. 254 -> 25.4 cm)
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

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
