use serde::{Deserialize, Serialize};

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
