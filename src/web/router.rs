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

        // Reject cross-origin POST requests to prevent drive-by motor control (BUG-30)
        if method == Method::Post {
            let headers = request.headers().to_vec();
            let host_hdr = headers.iter().find(|h| h.field.equiv("Host")).map(|h| h.value.as_str());
            let origin_hdr = headers.iter().find(|h| h.field.equiv("Origin")).map(|h| h.value.as_str());

            if let (Some(origin), Some(host)) = (origin_hdr, host_hdr) {
                let clean_origin = origin.trim_start_matches("http://").trim_start_matches("https://");
                let clean_origin_host = clean_origin.split('/').next().unwrap_or("");
                if !clean_origin_host.eq_ignore_ascii_case(host) {
                    eprintln!("[SECURITY] Rejected cross-origin POST from Origin '{}' targeting Host '{}'", origin, host);
                    self.respond_json(request, StatusCode(403), &ApiResponse::<()>::err("Cross-origin requests forbidden"));
                    return;
                }
            }
        }

        // Return 405 for preflight OPTIONS requests (CORS is disabled)
        if method == Method::Options {
            let res = Response::empty(StatusCode(405));
            if let Err(e) = request.respond(res) {
                eprintln!("[HTTP WARN] Failed sending OPTIONS response: {}", e);
            }
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
                let status = self.controller.get_system_status();
                self.respond_json(request, StatusCode(200), &ApiResponse::ok(status));
            }

            // Battery Endpoint
            (Method::Get, "/api/battery") => {
                let battery = self.controller.get_battery();
                self.respond_json(request, StatusCode(200), &ApiResponse::ok(battery));
            }

            // Clean System Shutdown (BUG-29)
            (Method::Post, "/api/shutdown") => {
                println!("[SYSTEM] System poweroff requested via Web API.");
                self.respond_json(request, StatusCode(200), &ApiResponse::ok("System is powering off..."));
                let is_mock = self.controller.is_mock();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if is_mock {
                        println!("[MOCK] Simulation poweroff completed.");
                    } else {
                        if let Err(e) = std::process::Command::new("systemctl").arg("poweroff").status() {
                            eprintln!("[SYSTEM ERROR] Failed executing systemctl poweroff: {}", e);
                        }
                    }
                });
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
            "polarity" => {
                match self.read_json_body::<PolarityPayload>(&mut request) {
                    Ok(payload) => match self.controller.set_polarity(port, &payload.polarity) {
                        Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Motor polarity updated")),
                        Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                    },
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }
            _ => {
                self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err(format!("Unknown motor action: {}", action)));
            }
        }
    }

    fn read_json_body<T: serde::de::DeserializeOwned>(&self, request: &mut Request) -> Result<T, String> {
        if let Some(len) = request.body_length() {
            if len > 4096 {
                return Err("Request body exceeds 4KB limit".into());
            }
        }
        let mut body = String::new();
        use std::io::Read;
        request.as_reader().take(4096).read_to_string(&mut body).map_err(|e| format!("Failed to read request body: {}", e))?;
        serde_json::from_str::<T>(&body).map_err(|e| format!("Invalid JSON payload: {}", e))
    }

    fn respond_static(&self, request: Request, content: &str, content_type: &str) {
        let res = Response::from_string(content)
            .with_header(Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap())
            .with_header(Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, must-revalidate"[..]).unwrap());
        if let Err(e) = request.respond(res) {
            eprintln!("[HTTP WARN] Failed to send static response: {}", e);
        }
    }

    fn respond_json<T: serde::Serialize>(&self, request: Request, status: StatusCode, data: &T) {
        let json_str = serde_json::to_string(data).unwrap_or_else(|_| "{\"success\":false,\"error\":\"JSON serialization error\"}".into());
        let res = Response::new(
            status,
            vec![
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap(),
            ],
            Cursor::new(json_str.into_bytes()),
            None,
            None,
        );
        if let Err(e) = request.respond(res) {
            eprintln!("[HTTP WARN] Failed to send JSON response: {}", e);
        }
    }
}
