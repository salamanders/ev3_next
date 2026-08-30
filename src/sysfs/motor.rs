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

pub struct Motor {
    pub port: String,
    pub sysfs_path: PathBuf,
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

                    motors.push(Motor {
                        port: port.to_string(),
                        sysfs_path: path,
                    });
                }
            }
        }

        // Sort by port name A, B, C, D
        motors.sort_by(|a, b| a.port.cmp(&b.port));
        motors
    }

    pub fn set_speed_sp(&self, speed: i32) -> io::Result<()> {
        self.write_attr("speed_sp", &speed.to_string())
    }

    pub fn set_duty_cycle_sp(&self, duty: i32) -> io::Result<()> {
        self.write_attr("duty_cycle_sp", &duty.to_string())
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

    pub fn read_status(&self) -> MotorStatus {
        let address = self.read_attr("address").unwrap_or_else(|_| format!("out{}", self.port));
        let driver_name = self.read_attr("driver_name").unwrap_or_else(|_| "tacho-motor".into());
        let position = self.read_attr("position").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let speed = self.read_attr("speed").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let duty_cycle = self.read_attr("duty_cycle").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let max_speed = self.read_attr("max_speed").ok().and_then(|s| s.parse().ok()).unwrap_or(1050);
        let count_per_rot = self.read_attr("count_per_rot").ok().and_then(|s| s.parse().ok()).unwrap_or(360);
        let state_raw = self.read_attr("state").unwrap_or_default();
        let state: Vec<String> = state_raw.split_whitespace().map(String::from).collect();

        MotorStatus {
            port: self.port.clone(),
            address,
            driver_name,
            position,
            speed,
            duty_cycle,
            state,
            max_speed,
            count_per_rot,
            connected: true,
        }
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
