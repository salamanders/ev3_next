use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
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
}

#[derive(Clone)]
pub struct Motor {
    pub port: String,
    pub sysfs_path: PathBuf,
    pub address: String,
    pub driver_name: String,
    pub max_speed: i32,
    pub count_per_rot: i32,
}

impl Motor {
    /// Enumerate all connected motors from `/sys/class/tacho-motor/`
    pub fn find_all() -> Vec<Motor> {
        let mut motors = Vec::new();
        let base_path = Path::new("/sys/class/tacho-motor");
        if !base_path.exists() {
            return motors;
        }

        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(addr) = fs::read_to_string(path.join("address")) {
                    let addr_clean = addr.trim().to_uppercase();
                    let port = if addr_clean.contains("OUTA") || addr_clean.ends_with(":A") {
                        "A"
                    } else if addr_clean.contains("OUTB") || addr_clean.ends_with(":B") {
                        "B"
                    } else if addr_clean.contains("OUTC") || addr_clean.ends_with(":C") {
                        "C"
                    } else if addr_clean.contains("OUTD") || addr_clean.ends_with(":D") {
                        "D"
                    } else {
                        continue;
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

                    motors.push(Motor {
                        port: port.to_string(),
                        sysfs_path: path,
                        address,
                        driver_name,
                        max_speed,
                        count_per_rot,
                    });
                }
            }
        }

        // Sort by port name A, B, C, D
        motors.sort_by(|a, b| a.port.cmp(&b.port));
        motors
    }

    pub fn set_speed_sp(&self, speed: i32) -> io::Result<()> {
        let clamped = speed.clamp(-self.max_speed, self.max_speed);
        self.write_attr("speed_sp", &clamped.to_string())
    }

    pub fn set_duty_cycle_sp(&self, duty: i32) -> io::Result<()> {
        let clamped = duty.clamp(-100, 100);
        self.write_attr("duty_cycle_sp", &clamped.to_string())
    }

    pub fn set_position_sp(&self, pos: i32) -> io::Result<()> {
        self.write_attr("position_sp", &pos.to_string())
    }

    pub fn set_time_sp(&self, ms: u32) -> io::Result<()> {
        self.write_attr("time_sp", &ms.to_string())
    }

    pub fn set_stop_action(&self, action: &str) -> io::Result<()> {
        self.write_attr("stop_action", action)
    }

    pub fn send_command(&self, cmd: &str) -> io::Result<()> {
        self.write_attr("command", cmd)
    }

    /// Read dynamic telemetry using cached static properties
    pub fn poll_dynamic_status(&self) -> Option<MotorStatus> {
        let pos_str = self.read_attr("position").ok()?;
        let position = pos_str.parse().ok()?;
        let speed = self.read_attr("speed").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let duty_cycle = self.read_attr("duty_cycle").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let state_raw = self.read_attr("state").unwrap_or_default();
        let state: Vec<String> = state_raw.split_whitespace().map(String::from).collect();

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
        })
    }

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
        })
    }

    fn write_attr(&self, attr: &str, val: &str) -> io::Result<()> {
        let path = self.sysfs_path.join(attr);
        let mut file = OpenOptions::new().write(true).open(&path)?;
        file.write_all(val.as_bytes())
    }

    fn read_attr(&self, attr: &str) -> io::Result<String> {
        let path = self.sysfs_path.join(attr);
        let mut file = File::open(&path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(content.trim().to_string())
    }
}
