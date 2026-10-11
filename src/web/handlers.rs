use serde::{Deserialize, Serialize};
use crate::sysfs::MotorStatus;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct JoystickPayload {
    pub x_port: Option<String>,
    pub y_port: Option<String>,
    pub x_speed: Option<i32>,
    pub y_speed: Option<i32>,
    pub stop_action: Option<String>,
}

impl JoystickPayload {
    /// Fallbacks omitted axes to standard dual-drive letter names.
    pub fn resolve_ports(&self) -> (String, String) {
        let x = self.x_port.as_ref().and_then(|s| normalize_port_name(s)).unwrap_or_else(|| "B".to_string());
        let y = self.y_port.as_ref().and_then(|s| normalize_port_name(s)).unwrap_or_else(|| "C".to_string());
        (x, y)
    }

    /// Extracts coordinate rates from primary axis values.
    pub fn resolve_speeds(&self) -> (i32, i32) {
        (self.x_speed.unwrap_or(0), self.y_speed.unwrap_or(0))
    }

    /// Normalizes braking preference strings with fallback default.
    pub fn resolve_stop_action(&self) -> &str {
        match self.stop_action.as_deref() {
            Some("coast") => "coast",
            Some("hold") => "hold",
            _ => "brake",
        }
    }
}


#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PortCommandPayload {
    pub command: Option<String>,
    pub speed: Option<i32>,
    pub degrees: Option<i32>,
    pub time_ms: Option<u32>,
    pub duty_cycle: Option<i32>,
    pub stop_action: Option<String>,
    pub polarity: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BatchItemPayload {
    pub port: String,
    #[serde(flatten)]
    pub payload: PortCommandPayload,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BatchCommandPayload {
    pub heartbeat_ms: Option<u64>,
    pub commands: Vec<BatchItemPayload>,
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
    /// Maps input JSON fields to executable hardware operation variants.
    pub fn resolve(&self, max_speed: i32) -> Result<ResolvedPortCommand, String> {
        let stop_act = self.stop_action.clone().unwrap_or_else(|| "brake".into());

        if let Some(ref cmd) = self.command {
            match cmd.to_ascii_lowercase().as_str() {
                "stop" => return Ok(ResolvedPortCommand::Stop { stop_action: stop_act }),
                "reset" => return Ok(ResolvedPortCommand::Reset),
                "run-forever" => {
                    let sp = self.speed.unwrap_or(max_speed / 2).clamp(-max_speed, max_speed);
                    return Ok(ResolvedPortCommand::RunForever { speed: sp });
                }
                "run-to-rel-pos" | "step" => {
                    let sp = self.speed.unwrap_or(max_speed / 2).clamp(-max_speed, max_speed);
                    let deg = self.degrees.unwrap_or(90);
                    return Ok(ResolvedPortCommand::RunToRelPos {
                        speed: sp,
                        position_sp: deg,
                        stop_action: stop_act,
                    });
                }
                "run-timed" => {
                    let sp = self.speed.unwrap_or(max_speed / 2).clamp(-max_speed, max_speed);
                    let ms = self.time_ms.unwrap_or(1000);
                    return Ok(ResolvedPortCommand::RunTimed {
                        speed: sp,
                        time_ms: ms,
                        stop_action: stop_act,
                    });
                }
                "run-direct" => {
                    let duty = self.duty_cycle.unwrap_or(50).clamp(-100, 100);
                    return Ok(ResolvedPortCommand::RunDirect { duty_cycle: duty });
                }
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

        // Inferred commands when 'command' is omitted
        if let Some(ref pol) = self.polarity {
            return Ok(ResolvedPortCommand::SetPolarity { polarity: pol.clone() });
        }
        if let Some(ref m) = self.mode {
            return Ok(ResolvedPortCommand::SetMode { mode: m.clone() });
        }
        if let Some(deg) = self.degrees {
            let sp = self.speed.unwrap_or(max_speed / 2).clamp(-max_speed, max_speed);
            return Ok(ResolvedPortCommand::RunToRelPos {
                speed: sp,
                position_sp: deg,
                stop_action: stop_act,
            });
        }
        if let Some(ms) = self.time_ms {
            let sp = self.speed.unwrap_or(max_speed / 2).clamp(-max_speed, max_speed);
            return Ok(ResolvedPortCommand::RunTimed {
                speed: sp,
                time_ms: ms,
                stop_action: stop_act,
            });
        }
        if let Some(duty) = self.duty_cycle {
            return Ok(ResolvedPortCommand::RunDirect { duty_cycle: duty.clamp(-100, 100) });
        }
        if let Some(sp) = self.speed {
            if sp == 0 {
                return Ok(ResolvedPortCommand::Stop { stop_action: stop_act });
            }
            return Ok(ResolvedPortCommand::RunForever { speed: sp.clamp(-max_speed, max_speed) });
        }
        if self.stop_action.is_some() {
            return Ok(ResolvedPortCommand::Stop { stop_action: stop_act });
        }

        Err("Could not infer command from payload (specify 'command', 'speed', or 'degrees')".into())
    }
}

pub const MOTOR_PORTS: [&str; 4] = ["A", "B", "C", "D"];
pub const SENSOR_PORTS: [&str; 4] = ["1", "2", "3", "4"];

/// Checks whether an identifier denotes an actuator output channel.
pub fn is_motor_port(port: &str) -> bool {
    MOTOR_PORTS.iter().any(|p| p.eq_ignore_ascii_case(port))
}

/// Checks whether an identifier denotes a peripheral input channel.
pub fn is_sensor_port(port: &str) -> bool {
    SENSOR_PORTS.iter().any(|p| p.eq_ignore_ascii_case(port))
}

// Matches kernel sysfs address text against device port candidate prefixes.
fn parse_channel_address(address: &str, patterns: &[(&str, &str, &'static str)]) -> Option<&'static str> {
    let clean = address.trim().to_ascii_uppercase();
    patterns.iter().find_map(|&(prefix, suffix, port)| {
        if clean.contains(prefix) || clean.ends_with(suffix) {
            Some(port)
        } else {
            None
        }
    })
}

/// Extracts actuator socket letter from kernel path text.
pub fn parse_motor_address(address: &str) -> Option<&'static str> {
    parse_channel_address(address, &[
        ("OUTA", ":A", "A"),
        ("OUTB", ":B", "B"),
        ("OUTC", ":C", "C"),
        ("OUTD", ":D", "D"),
    ])
}

/// Extracts input channel index from kernel path text.
pub fn parse_sensor_address(address: &str) -> Option<&'static str> {
    parse_channel_address(address, &[
        ("IN1", ":1", "1"),
        ("IN2", ":2", "2"),
        ("IN3", ":3", "3"),
        ("IN4", ":4", "4"),
    ])
}

/// Converts mixed-case socket aliases into canonical uppercase tokens.
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

pub use crate::sysfs::SensorStatus;

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
    /// Wraps successful computation results into standard transport envelope.
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Wraps failure diagnostics into standard transport envelope.
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

    // Verifies alias resolution maps uppercase and prefixed inputs correctly.
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

    // Asserts partial payloads map to expected execution variants.
    #[test]
    fn test_resolve_inferred_commands() {
        // 1. Inferred RunForever from integer speed
        let payload1: PortCommandPayload = serde_json::from_str(r#"{"speed": 500}"#).unwrap();
        assert_eq!(
            payload1.resolve(1050).unwrap(),
            ResolvedPortCommand::RunForever { speed: 500 }
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

        // 3. Inferred RunDirect from duty_cycle
        let payload3: PortCommandPayload = serde_json::from_str(r#"{"duty_cycle": 75}"#).unwrap();
        assert_eq!(
            payload3.resolve(1050).unwrap(),
            ResolvedPortCommand::RunDirect { duty_cycle: 75 }
        );

        // 4. Inferred Stop from speed 0
        let payload4: PortCommandPayload = serde_json::from_str(r#"{"speed": 0}"#).unwrap();
        assert_eq!(
            payload4.resolve(1050).unwrap(),
            ResolvedPortCommand::Stop { stop_action: "brake".into() }
        );

        // 5. Explicit run-timed command
        let payload5: PortCommandPayload = serde_json::from_str(r#"{"command": "run-timed", "speed": 400, "time_ms": 1500}"#).unwrap();
        assert_eq!(
            payload5.resolve(1050).unwrap(),
            ResolvedPortCommand::RunTimed {
                speed: 400,
                time_ms: 1500,
                stop_action: "brake".into()
            }
        );

        // 6. Inferred SetMode for sensors
        let payload6: PortCommandPayload = serde_json::from_str(r#"{"mode": "COL-COLOR"}"#).unwrap();
        assert_eq!(
            payload6.resolve(1050).unwrap(),
            ResolvedPortCommand::SetMode { mode: "COL-COLOR".into() }
        );
    }
}
