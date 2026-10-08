use serde::{Deserialize, Serialize};
use crate::sysfs::MotorStatus;

#[derive(Debug, Deserialize, Serialize)]
pub struct RunForeverPayload {
    pub speed: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RunTimedPayload {
    pub speed: i32,
    pub time_ms: u32,
    pub stop_action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RunToRelPosPayload {
    pub speed: i32,
    pub position_sp: i32,
    pub stop_action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RunDirectPayload {
    pub duty_cycle: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StopPayload {
    pub action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TankDrivePayload {
    pub left_port: Option<String>,
    pub right_port: Option<String>,
    pub left_speed: i32,
    pub right_speed: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PolarityPayload {
    pub polarity: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PortCommandPayload {
    pub command: Option<String>,
    pub speed: Option<serde_json::Value>,
    pub speed_sp: Option<serde_json::Value>,
    pub duty: Option<serde_json::Value>,
    pub duty_cycle: Option<serde_json::Value>,
    pub duty_cycle_sp: Option<serde_json::Value>,
    pub time_ms: Option<f64>,
    pub time_sp: Option<f64>,
    pub duration_s: Option<f64>,
    pub position_sp: Option<f64>,
    pub degrees: Option<f64>,
    pub stop_action: Option<String>,
    pub polarity: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedPortCommand {
    RunForever { speed: i32 },
    RunTimed { speed: i32, time_ms: u32, stop_action: String },
    RunToRelPos { speed: i32, position_sp: i32, stop_action: String },
    RunDirect { duty_cycle: i32 },
    Stop { stop_action: String },
    Reset,
    SetPolarity { polarity: String },
    SetMode { mode: String },
}

impl PortCommandPayload {
    fn parse_speed(val: &serde_json::Value, max_speed: i32) -> Result<i32, String> {
        if let Some(i) = val.as_i64() {
            return Ok(i.clamp(-max_speed as i64, max_speed as i64) as i32);
        }
        if let Some(f) = val.as_f64() {
            // If float is in [-1.0, 1.0] and not 0.0, treat as normalized fraction of max_speed
            if f.abs() <= 1.0 && f != 0.0 {
                let scaled = (f * max_speed as f64).round() as i32;
                return Ok(scaled.clamp(-max_speed, max_speed));
            }
            return Ok((f.round() as i32).clamp(-max_speed, max_speed));
        }
        Err("Invalid numeric format for speed".into())
    }

    fn parse_duty(val: &serde_json::Value) -> Result<i32, String> {
        if let Some(i) = val.as_i64() {
            return Ok(i.clamp(-100, 100) as i32);
        }
        if let Some(f) = val.as_f64() {
            if f.abs() <= 1.0 && f != 0.0 {
                let scaled = (f * 100.0).round() as i32;
                return Ok(scaled.clamp(-100, 100));
            }
            return Ok((f.round() as i32).clamp(-100, 100));
        }
        Err("Invalid numeric format for duty cycle".into())
    }

    pub fn resolve(&self, max_speed: i32) -> Result<ResolvedPortCommand, String> {
        let default_stop = self.stop_action.clone().unwrap_or_else(|| "brake".into());
        let speed_input = self.speed.as_ref().or(self.speed_sp.as_ref());
        let duty_input = self
            .duty
            .as_ref()
            .or(self.duty_cycle.as_ref())
            .or(self.duty_cycle_sp.as_ref());
        let time_ms_input = if let Some(s) = self.duration_s {
            Some((s * 1000.0).max(1.0) as u32)
        } else if let Some(m) = self.time_ms.or(self.time_sp) {
            Some(m.max(1.0) as u32)
        } else {
            None
        };
        let pos_sp_input = if let Some(d) = self.degrees {
            Some(d.round() as i32)
        } else if let Some(p) = self.position_sp {
            Some(p.round() as i32)
        } else {
            None
        };

        // 1. Explicit command specified
        if let Some(ref cmd) = self.command {
            match cmd.to_ascii_lowercase().as_str() {
                "run-forever" => {
                    let sp = match speed_input {
                        Some(v) => Self::parse_speed(v, max_speed)?,
                        None => max_speed / 2,
                    };
                    return Ok(ResolvedPortCommand::RunForever { speed: sp });
                }
                "run-timed" => {
                    let sp = match speed_input {
                        Some(v) => Self::parse_speed(v, max_speed)?,
                        None => max_speed / 2,
                    };
                    let ms = time_ms_input.unwrap_or(1000);
                    return Ok(ResolvedPortCommand::RunTimed {
                        speed: sp,
                        time_ms: ms,
                        stop_action: default_stop,
                    });
                }
                "run-to-rel-pos" | "step" => {
                    let sp = match speed_input {
                        Some(v) => Self::parse_speed(v, max_speed)?,
                        None => max_speed / 2,
                    };
                    let deg = pos_sp_input.unwrap_or(90);
                    return Ok(ResolvedPortCommand::RunToRelPos {
                        speed: sp,
                        position_sp: deg,
                        stop_action: default_stop,
                    });
                }
                "run-direct" => {
                    let duty = match duty_input {
                        Some(v) => Self::parse_duty(v)?,
                        None => 50,
                    };
                    return Ok(ResolvedPortCommand::RunDirect { duty_cycle: duty });
                }
                "stop" => return Ok(ResolvedPortCommand::Stop { stop_action: default_stop }),
                "reset" => return Ok(ResolvedPortCommand::Reset),
                "polarity" => {
                    let pol = self.polarity.clone().unwrap_or_else(|| "normal".into());
                    return Ok(ResolvedPortCommand::SetPolarity { polarity: pol });
                }
                "mode" => {
                    if let Some(ref m) = self.mode {
                        return Ok(ResolvedPortCommand::SetMode { mode: m.clone() });
                    }
                    return Err("Mode command requires 'mode' parameter".into());
                }
                other => return Err(format!("Unrecognized command: '{}'", other)),
            }
        }

        // 2. Inferred command when 'command' is omitted
        if let Some(ref pol) = self.polarity {
            return Ok(ResolvedPortCommand::SetPolarity { polarity: pol.clone() });
        }
        if let Some(ref m) = self.mode {
            return Ok(ResolvedPortCommand::SetMode { mode: m.clone() });
        }
        if let Some(ms) = time_ms_input {
            let sp = match speed_input {
                Some(v) => Self::parse_speed(v, max_speed)?,
                None => max_speed / 2,
            };
            return Ok(ResolvedPortCommand::RunTimed {
                speed: sp,
                time_ms: ms,
                stop_action: default_stop,
            });
        }
        if let Some(deg) = pos_sp_input {
            let sp = match speed_input {
                Some(v) => Self::parse_speed(v, max_speed)?,
                None => max_speed / 2,
            };
            return Ok(ResolvedPortCommand::RunToRelPos {
                speed: sp,
                position_sp: deg,
                stop_action: default_stop,
            });
        }
        if let Some(duty_val) = duty_input {
            let duty = Self::parse_duty(duty_val)?;
            return Ok(ResolvedPortCommand::RunDirect { duty_cycle: duty });
        }
        if let Some(v) = speed_input {
            let sp = Self::parse_speed(v, max_speed)?;
            if sp == 0 {
                return Ok(ResolvedPortCommand::Stop { stop_action: default_stop });
            }
            return Ok(ResolvedPortCommand::RunForever { speed: sp });
        }
        if self.stop_action.is_some() {
            return Ok(ResolvedPortCommand::Stop { stop_action: default_stop });
        }

        Err("Could not infer command from payload (specify 'command', 'speed', 'duty', 'duration_s', or 'degrees')".into())
    }
}

pub fn normalize_port_name(port: &str) -> Option<String> {
    let clean = port.trim().to_ascii_uppercase();
    match clean.as_str() {
        "A" | "OUTA" | "PORTA" => Some("A".into()),
        "B" | "OUTB" | "PORTB" => Some("B".into()),
        "C" | "OUTC" | "PORTC" => Some("C".into()),
        "D" | "OUTD" | "PORTD" => Some("D".into()),
        "1" | "IN1" | "PORT1" => Some("1".into()),
        "2" | "IN2" | "PORT2" => Some("2".into()),
        "3" | "IN3" | "PORT3" => Some("3".into()),
        "4" | "IN4" | "PORT4" => Some("4".into()),
        _ => None,
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllPortsStatus {
    pub motors: Vec<MotorStatus>,
    pub sensors: Vec<SensorStatus>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_port_name() {
        assert_eq!(normalize_port_name("a"), Some("A".into()));
        assert_eq!(normalize_port_name("outB"), Some("B".into()));
        assert_eq!(normalize_port_name("PortC"), Some("C".into()));
        assert_eq!(normalize_port_name("D"), Some("D".into()));
        assert_eq!(normalize_port_name("1"), Some("1".into()));
        assert_eq!(normalize_port_name("in2"), Some("2".into()));
        assert_eq!(normalize_port_name("PORT3"), Some("3".into()));
        assert_eq!(normalize_port_name("4"), Some("4".into()));
        assert_eq!(normalize_port_name("5"), None);
        assert_eq!(normalize_port_name("outE"), None);
    }

    #[test]
    fn test_resolve_inferred_commands() {
        // 1. Inferred RunTimed from duration_s
        let payload1: PortCommandPayload = serde_json::from_str(r#"{"speed": 500, "duration_s": 2.5}"#).unwrap();
        assert_eq!(
            payload1.resolve(1050).unwrap(),
            ResolvedPortCommand::RunTimed {
                speed: 500,
                time_ms: 2500,
                stop_action: "brake".into()
            }
        );

        // 2. Inferred RunToRelPos from degrees
        let payload2: PortCommandPayload = serde_json::from_str(r#"{"degrees": 180, "speed": 400}"#).unwrap();
        assert_eq!(
            payload2.resolve(1050).unwrap(),
            ResolvedPortCommand::RunToRelPos {
                speed: 400,
                position_sp: 180,
                stop_action: "brake".into()
            }
        );

        // 3. Inferred RunDirect from float duty
        let payload3: PortCommandPayload = serde_json::from_str(r#"{"duty": 0.75}"#).unwrap();
        assert_eq!(
            payload3.resolve(1050).unwrap(),
            ResolvedPortCommand::RunDirect { duty_cycle: 75 }
        );

        // 4. Inferred RunForever from float speed (normalized fraction)
        let payload4: PortCommandPayload = serde_json::from_str(r#"{"speed": 0.5}"#).unwrap();
        assert_eq!(
            payload4.resolve(1000).unwrap(),
            ResolvedPortCommand::RunForever { speed: 500 }
        );

        // 5. Inferred Stop from speed 0
        let payload5: PortCommandPayload = serde_json::from_str(r#"{"speed": 0}"#).unwrap();
        assert_eq!(
            payload5.resolve(1050).unwrap(),
            ResolvedPortCommand::Stop { stop_action: "brake".into() }
        );

        // 6. Raw sysfs keys: speed_sp and time_sp
        let payload6: PortCommandPayload = serde_json::from_str(r#"{"command": "run-timed", "speed_sp": 400, "time_sp": 1500}"#).unwrap();
        assert_eq!(
            payload6.resolve(1050).unwrap(),
            ResolvedPortCommand::RunTimed {
                speed: 400,
                time_ms: 1500,
                stop_action: "brake".into()
            }
        );
    }
}
