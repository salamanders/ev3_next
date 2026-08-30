use std::io::Cursor;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, StatusCode};
use crate::controller::MotorController;
use crate::web::handlers::*;

// Embedded Single Page Application Web Assets
const HTML_CONTENT: &str = include_str!("../../web_assets/index.html");
const CSS_CONTENT: &str = include_str!("../../web_assets/style.css");
const JS_CONTENT: &str = include_str!("../../web_assets/app.js");

pub struct Router {
    controller: Arc<MotorController>,
}

impl Router {
    pub fn new(controller: Arc<MotorController>) -> Self {
        Self { controller }
    }

    pub fn handle_request(&self, mut request: Request) {
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("/");
        let method = request.method().clone();

        // Handle CORS preflight
        if method == Method::Options {
            let res = Response::empty(StatusCode(204))
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap())
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..]).unwrap());
            let _ = request.respond(res);
            return;
        }

        match (method, path) {
            // Static Web Assets
            (Method::Get, "/") | (Method::Get, "/index.html") => {
                self.respond_static(request, HTML_CONTENT, "text/html; charset=utf-8");
            }
            (Method::Get, "/style.css") => {
                self.respond_static(request, CSS_CONTENT, "text/css; charset=utf-8");
            }
            (Method::Get, "/app.js") => {
                self.respond_static(request, JS_CONTENT, "application/javascript; charset=utf-8");
            }

            // Telemetry Endpoint (reads in <0.05ms from cache)
            (Method::Get, "/api/status") => {
                let status = self.controller.get_all_status();
                self.respond_json(request, StatusCode(200), &ApiResponse::ok(status));
            }

            // Global Emergency Stop
            (Method::Post, "/api/emergency-stop") => {
                match self.controller.emergency_stop() {
                    Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("All motors stopped")),
                    Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                }
            }

            // Tank Drive Endpoint
            (Method::Post, "/api/tank-drive") => {
                match self.read_json_body::<TankDrivePayload>(&mut request) {
                    Ok(payload) => {
                        let left_port = payload.left_port.as_deref().unwrap_or("B");
                        let right_port = payload.right_port.as_deref().unwrap_or("C");
                        match self.controller.tank_drive(left_port, right_port, payload.left_speed, payload.right_speed) {
                            Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Tank drive command dispatched")),
                            Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                        }
                    }
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }

            // Motor-specific Endpoints: /api/motor/{port}/{command}
            (Method::Post, p) if p.starts_with("/api/motor/") => {
                self.handle_motor_post(request, p);
            }

            _ => {
                self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err("Endpoint not found"));
            }
        }
    }

    fn handle_motor_post(&self, mut request: Request, path: &str) {
        let parts: Vec<&str> = path.trim_start_matches("/api/motor/").split('/').collect();
        if parts.len() < 2 {
            self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err("Invalid motor endpoint format"));
            return;
        }

        let port = parts[0];
        let action = parts[1];

        match action {
            "run-forever" => {
                match self.read_json_body::<RunForeverPayload>(&mut request) {
                    Ok(payload) => match self.controller.run_forever(port, payload.speed) {
                        Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor running forever")),
                        Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                    },
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }
            "run-timed" => {
                match self.read_json_body::<RunTimedPayload>(&mut request) {
                    Ok(payload) => match self.controller.run_timed(port, payload.speed, payload.time_ms, payload.stop_action) {
                        Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor running timed")),
                        Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                    },
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }
            "run-to-rel-pos" => {
                match self.read_json_body::<RunToRelPosPayload>(&mut request) {
                    Ok(payload) => match self.controller.run_to_rel_pos(port, payload.speed, payload.position_sp, payload.stop_action) {
                        Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor running to relative position")),
                        Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                    },
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }
            "run-direct" => {
                match self.read_json_body::<RunDirectPayload>(&mut request) {
                    Ok(payload) => match self.controller.run_direct(port, payload.duty_cycle) {
                        Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor running direct duty cycle")),
                        Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                    },
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }
            "stop" => {
                let payload_opt = self.read_json_body::<StopPayload>(&mut request).ok();
                let stop_action = payload_opt.and_then(|p| p.action);
                match self.controller.stop(port, stop_action) {
                    Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor stopped")),
                    Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                }
            }
            "reset" => {
                match self.controller.reset(port) {
                    Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor reset")),
                    Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                }
            }
            _ => {
                self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err(format!("Unknown motor action: {}", action)));
            }
        }
    }

    fn read_json_body<T: serde::de::DeserializeOwned>(&self, request: &mut Request) -> Result<T, String> {
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).map_err(|e| format!("Failed to read request body: {}", e))?;
        serde_json::from_str::<T>(&body).map_err(|e| format!("Invalid JSON payload: {}", e))
    }

    fn respond_static(&self, request: Request, content: &str, content_type: &str) {
        let res = Response::from_string(content)
            .with_header(Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap())
            .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"public, max-age=3600"[..]).unwrap())
            .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
        let _ = request.respond(res);
    }

    fn respond_json<T: serde::Serialize>(&self, request: Request, status: StatusCode, data: &T) {
        let json_str = serde_json::to_string(data).unwrap_or_else(|_| "{\"success\":false,\"error\":\"JSON serialization error\"}".into());
        let res = Response::new(
            status,
            vec![
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
            ],
            Cursor::new(json_str.into_bytes()),
            None,
            None,
        );
        let _ = request.respond(res);
    }
}
