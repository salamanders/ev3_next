use std::io::{Cursor, Read};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, StatusCode};
use crate::controller::MotorController;
use crate::web::handlers::*;

// Embedded Single Page Application Web Assets
const BASELINE_HTML: &[u8] = include_bytes!("../../web_assets/index.html");
const BASELINE_DESIGN_HTML: &[u8] = include_bytes!("../../web_assets/design.html");
const BASELINE_CSS: &[u8] = include_bytes!("../../web_assets/style.css");
const BASELINE_JS: &[u8] = include_bytes!("../../web_assets/app.js");

pub struct BaselineWidget {
    pub relative_path: &'static str,
    pub content: &'static [u8],
}

pub const BASELINE_WIDGETS: &[BaselineWidget] = &[
    BaselineWidget {
        relative_path: "manifest.json",
        content: include_bytes!("../../web_assets/widgets/manifest.json"),
    },
    BaselineWidget {
        relative_path: "joystick/widget.js",
        content: include_bytes!("../../web_assets/widgets/joystick/widget.js"),
    },
    BaselineWidget {
        relative_path: "tank/widget.js",
        content: include_bytes!("../../web_assets/widgets/tank/widget.js"),
    },
    BaselineWidget {
        relative_path: "slider/widget.js",
        content: include_bytes!("../../web_assets/widgets/slider/widget.js"),
    },
    BaselineWidget {
        relative_path: "button/widget.js",
        content: include_bytes!("../../web_assets/widgets/button/widget.js"),
    },
    BaselineWidget {
        relative_path: "sensor/widget.js",
        content: include_bytes!("../../web_assets/widgets/sensor/widget.js"),
    },
];

pub struct Router {
    controller: Arc<MotorController>,
    widgets_dir: std::path::PathBuf,
}

/// Trims redundant standard network scheme suffixes from origin headers.
pub(crate) fn strip_default_port(host: &str) -> &str {
    let trimmed = host.trim();
    if let Some(stripped) = trimmed.strip_suffix(":80") {
        stripped
    } else if let Some(stripped) = trimmed.strip_suffix(":443") {
        stripped
    } else {
        trimmed
    }
}

impl Router {
    /// Populates plain frontend cache and prepares request dispatcher.
    pub fn new(controller: Arc<MotorController>) -> Self {
        let is_mock = controller.is_mock();
        let assets_dir: std::path::PathBuf = std::env::var("WEB_ASSETS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                if is_mock && !std::path::Path::new("/home/robot/web_assets").exists() {
                    std::path::PathBuf::from("web_assets")
                } else {
                    std::path::PathBuf::from("/home/robot/web_assets")
                }
            });
        Self::new_with_dir(controller, &assets_dir)
    }

    /// Initializes router with explicitly configured asset directory path.
    pub fn new_with_dir(controller: Arc<MotorController>, assets_dir: &std::path::Path) -> Self {
        let widgets_dir = assets_dir.join("widgets");
        println!("[ROUTER] Core web assets served directly from RAM; widgets from {:?}", widgets_dir);
        Self {
            controller,
            widgets_dir,
        }
    }

    /// Evaluates incoming HTTP verb and path to execute endpoint logic.
    pub fn handle_request(&self, mut request: Request) {
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("/");
        let method = request.method().clone();

        // Reject cross-origin requests to prevent unauthorized actuation.
        if method == Method::Post {
            let headers = request.headers().to_vec();
            let host_hdr = headers.iter().find(|h| h.field.equiv("Host")).map(|h| h.value.as_str());
            let origin_hdr = headers.iter().find(|h| h.field.equiv("Origin")).map(|h| h.value.as_str());

            if let (Some(origin), Some(host)) = (origin_hdr, host_hdr) {
                let clean_origin = origin.trim_start_matches("http://").trim_start_matches("https://");
                let clean_origin_host = clean_origin.split('/').next().unwrap_or("");
                let norm_origin = strip_default_port(clean_origin_host);
                let norm_host = strip_default_port(host);
                if !norm_origin.eq_ignore_ascii_case(norm_host) {
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
            // Static Web Assets (supporting GET and HEAD)
            (Method::Get, "/") | (Method::Head, "/") | (Method::Get, "/run") | (Method::Head, "/run") => {
                self.respond_static_plain_slice(request, BASELINE_HTML, "text/html; charset=utf-8", method == Method::Head);
            }
            (Method::Get, "/design") | (Method::Head, "/design") => {
                self.respond_static_plain_slice(request, BASELINE_DESIGN_HTML, "text/html; charset=utf-8", method == Method::Head);
            }
            (Method::Get, "/style.css") | (Method::Head, "/style.css") => {
                self.respond_static_plain_slice(request, BASELINE_CSS, "text/css; charset=utf-8", method == Method::Head);
            }
            (Method::Get, "/app.js") | (Method::Head, "/app.js") => {
                self.respond_static_plain_slice(request, BASELINE_JS, "application/javascript; charset=utf-8", method == Method::Head);
            }
            (Method::Get, "/favicon.ico") | (Method::Head, "/favicon.ico") => {
                let res = Response::empty(StatusCode(204));
                let _ = request.respond(res);
            }

            // Modular Widget Assets (supporting GET and HEAD)
            (Method::Get, p) if p.starts_with("/widgets/") => {
                self.handle_widget_request(request, p, false);
            }
            (Method::Head, p) if p.starts_with("/widgets/") => {
                self.handle_widget_request(request, p, true);
            }

            // Telemetry Endpoint (reads in <0.05ms from cache)
            (Method::Get, "/api/status") => {
                let status = self.controller.get_system_status();
                self.respond_json(request, StatusCode(200), &ApiResponse::ok(status));
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
            (Method::Post, "/api/estop") => {
                match self.controller.emergency_stop() {
                    Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("All motors stopped")),
                    Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                }
            }

            // Rescan Hardware Devices Endpoint
            (Method::Post, "/api/rescan") => {
                match self.controller.rescan() {
                    Ok(ports) => self.respond_json(request, StatusCode(200), &ApiResponse::ok(ports)),
                    Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                }
            }

            // Batch Port Commands: POST /api/batch
            (Method::Post, "/api/batch") => {
                match self.read_json_body::<BatchCommandPayload>(&mut request) {
                    Ok(batch) => {
                        match self.controller.execute_batch(&batch) {
                            Ok(results) => self.respond_json(request, StatusCode(200), &ApiResponse::ok(results)),
                            Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                        }
                    }
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }

            // Unified Port Command Endpoint: POST /api/port/{port}
            (Method::Post, p) if p.starts_with("/api/port/") => {
                self.handle_port_post(request, p);
            }

            _ => {
                self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err("Endpoint not found"));
            }
        }
    }

    // Decodes transport payload and dispatches instructions to hardware.
    fn handle_port_post(&self, mut request: Request, path: &str) {
        let raw_port = path.trim_start_matches("/api/port/");
        let normalized = match normalize_port_name(raw_port) {
            Some(p) => p,
            None => return self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(format!("Invalid port: '{}'", raw_port))),
        };

        let payload = match self.read_json_body::<PortCommandPayload>(&mut request) {
            Ok(p) => p,
            Err(e) => return self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
        };

        let max_speed = self.controller.resolve_max_speed(&normalized);
        match payload.resolve(max_speed) {
            Ok(cmd) => match self.controller.execute_port_command(&normalized, &cmd) {
                Ok(msg) => self.respond_json(request, StatusCode(200), &ApiResponse::ok(msg)),
                Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
            },
            Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
        }
    }

    // Deserializes size-bounded incoming stream payloads into concrete types.
    fn read_json_body<T: serde::de::DeserializeOwned>(&self, request: &mut Request) -> Result<T, String> {
        if let Some(len) = request.body_length() {
            if len > 4096 {
                return Err("Request body exceeds 4KB limit".into());
            }
        }
        let mut body = String::new();
        request.as_reader().take(4096).read_to_string(&mut body).map_err(|e| format!("Failed to read request body: {}", e))?;
        serde_json::from_str::<T>(&body).map_err(|e| format!("Invalid JSON payload: {}", e))
    }

    // Resolves and serves widget assets from disk with RAM baseline fallback.
    fn handle_widget_request(&self, request: Request, full_path: &str, is_head: bool) {
        let subpath = &full_path["/widgets/".len()..];
        if subpath.is_empty() {
            self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err("Widget file not found"));
            return;
        }

        let subpath_lower = subpath.to_ascii_lowercase();
        if subpath.contains("..")
            || subpath.starts_with('/')
            || subpath.starts_with('\\')
            || subpath.contains('\0')
            || subpath_lower.contains("%2e")
            || subpath_lower.contains("%2f")
            || subpath_lower.contains("%5c")
        {
            self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err("Invalid path traversal"));
            return;
        }

        let mime = if subpath.ends_with(".js") {
            "application/javascript; charset=utf-8"
        } else if subpath.ends_with(".json") {
            "application/json; charset=utf-8"
        } else if subpath.ends_with(".css") {
            "text/css; charset=utf-8"
        } else if subpath.ends_with(".html") {
            "text/html; charset=utf-8"
        } else {
            "application/octet-stream"
        };

        let file_path = self.widgets_dir.join(subpath);

        // 1. Serve plain asset on disk if present
        if let Ok(bytes) = std::fs::read(&file_path) {
            self.respond_static_plain_slice(request, &bytes, mime, is_head);
            return;
        }

        // 2. Fall back to embedded compile-time baseline widget in RAM
        if let Some(entry) = BASELINE_WIDGETS.iter().find(|e| e.relative_path == subpath) {
            self.respond_static_plain_slice(request, entry.content, mime, is_head);
            return;
        }

        self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err("Widget file not found"));
    }

    // Serves uncompressed static files with strict zero-cache policy headers.
    fn respond_static_plain_slice(&self, request: Request, content: &[u8], content_type: &str, is_head: bool) {
        let len = content.len();
        let body = if is_head { Vec::new() } else { content.to_vec() };
        let res = Response::new(
            StatusCode(200),
            vec![
                Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
                Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
            ],
            Cursor::new(body),
            Some(len),
            None,
        );
        if let Err(e) = request.respond(res) {
            eprintln!("[HTTP WARN] Failed to send static response: {}", e);
        }
    }

    // Serializes structures into textual wire representations.
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

#[cfg(test)]
mod tests {
    use super::*;

    // Verifies bundled memory binaries have proper text content.
    #[test]
    fn test_baseline_assets_embedded_and_valid_plain() {
        assert!(!BASELINE_HTML.is_empty());
        assert!(!BASELINE_DESIGN_HTML.is_empty());
        assert!(!BASELINE_CSS.is_empty());
        assert!(!BASELINE_JS.is_empty());

        let html = std::str::from_utf8(BASELINE_HTML).unwrap();
        assert!(html.contains("<!DOCTYPE html>") || html.contains("<html"));

        let design = std::str::from_utf8(BASELINE_DESIGN_HTML).unwrap();
        assert!(design.contains("<!DOCTYPE html>") || design.contains("<html"));

        let css = std::str::from_utf8(BASELINE_CSS).unwrap();
        assert!(css.contains("body") || css.contains("margin"));

        let js = std::str::from_utf8(BASELINE_JS).unwrap();
        assert!(js.contains("function") || js.contains("const") || js.contains("let") || js.contains("class"));
    }

    // Exercises network server across primary endpoints and HTTP methods.
    #[test]
    fn test_http_routes_serve_plain_assets_and_headers() {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let controller = MotorController::new(true, 50);
        let router = Arc::new(Router::new(controller));

        let server = tiny_http::Server::http("127.0.0.1:0").expect("Failed to bind ephemeral test server");
        let addr = server.server_addr().to_ip().unwrap();
        let port = addr.port();

        let router_clone = router.clone();
        let server_clone = server;
        let thread_handle = std::thread::spawn(move || {
            for _ in 0..16 {
                if let Ok(req) = server_clone.recv() {
                    router_clone.handle_request(req);
                }
            }
        });

        // 1. Test GET /
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: text/html; charset=utf-8"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // 2. Test HEAD /
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: text/html; charset=utf-8"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // 3. Test GET /style.css
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /style.css HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: text/css; charset=utf-8"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // 4. Test GET /app.js
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /app.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: application/javascript; charset=utf-8"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // Test GET /design
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /design HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: text/html; charset=utf-8"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
        }

        // Test GET /run
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /run HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: text/html; charset=utf-8"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
        }

        // 5. Test GET /favicon.ico
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 204 No Content"));
        }

        // 6. Test HEAD /favicon.ico
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"HEAD /favicon.ico HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 204 No Content"));
        }

        // 7. Test GET /api/status (Cache-Control: no-store)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /api/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Cache-Control: no-store"));
            assert!(resp_str.contains("Content-Type: application/json"));
        }

        // 8. Test POST /api/rescan
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"POST /api/rescan HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: application/json"));
            assert!(resp_str.contains("\"success\":true"));
        }

        // 9. Test POST /api/port/outA
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = b"{\"speed\": 500}";
            let req = format!(
                "POST /api/port/outA HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                std::str::from_utf8(body).unwrap()
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"success\":true"));
        }

        // 10. Test POST /api/port/2 (Change sensor mode)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = b"{\"mode\": \"COL-REFLECT\"}";
            let req = format!(
                "POST /api/port/2 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                std::str::from_utf8(body).unwrap()
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"success\":true"));
        }

        // 11. Test POST /api/estop
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"POST /api/estop HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"success\":true"));
            assert!(resp_str.contains("All motors stopped"));
        }

        // 12. Test POST /api/batch
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = b"{\"heartbeat_ms\": 2000, \"commands\": [{\"port\": \"A\", \"speed\": 200}]}";
            let req = format!(
                "POST /api/batch HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                std::str::from_utf8(body).unwrap()
            );
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"success\":true"));
        }

        // 13. Cross-origin POST with port 80 in Host omitted in Origin.
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let req = "POST /api/estop HTTP/1.1\r\nHost: localhost:80\r\nOrigin: http://localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
        }

        // 14. Test genuine cross-origin POST rejected with 403 Forbidden
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let req = "POST /api/estop HTTP/1.1\r\nHost: localhost\r\nOrigin: http://attacker.com\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 403 Forbidden"));
        }

        thread_handle.join().unwrap();
    }

    // Asserts standard network numbers are removed while keeping custom ones.
    #[test]
    fn test_strip_default_port() {
        assert_eq!(strip_default_port("localhost:80"), "localhost");
        assert_eq!(strip_default_port("localhost"), "localhost");
        assert_eq!(strip_default_port("ev3dev.local:80"), "ev3dev.local");
        assert_eq!(strip_default_port("ev3dev.local:443"), "ev3dev.local");
        assert_eq!(strip_default_port("localhost:8080"), "localhost:8080");
        assert_eq!(strip_default_port("localhost:1080"), "localhost:1080");
        assert_eq!(strip_default_port("[::1]:80"), "[::1]");
    }

    // Asserts static widget routes deliver headers and enforce traversal guard.
    #[test]
    fn test_widget_static_routes_headers_and_security() {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        let controller = MotorController::new(true, 50);
        let router = Arc::new(Router::new(controller));

        let server = tiny_http::Server::http("127.0.0.1:0").expect("Failed to bind test server");
        let port = server.server_addr().to_ip().unwrap().port();

        let router_clone = router.clone();
        let server_clone = server;
        let thread_handle = std::thread::spawn(move || {
            for _ in 0..7 {
                if let Ok(req) = server_clone.recv() {
                    router_clone.handle_request(req);
                }
            }
        });

        // 1. GET /widgets/manifest.json
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /widgets/manifest.json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: application/json; charset=utf-8"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // 2. GET /widgets/joystick/widget.js?v=1728670000
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /widgets/joystick/widget.js?v=1728670000 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: application/javascript; charset=utf-8"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // 3. HEAD /widgets/joystick/widget.js
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"HEAD /widgets/joystick/widget.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Type: application/javascript; charset=utf-8"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // 4. Path traversal attempt (GET /widgets/../../etc/passwd)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /widgets/../../etc/passwd HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 400 Bad Request"));
            assert!(resp_str.contains("Invalid path traversal"));
        }

        // 5. Non-existent widget file (GET /widgets/missing_file.js)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /widgets/missing_file.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 404 Not Found"));
        }

        // 6. Encoded path traversal attempt (GET /widgets/%2e%2e/passwd)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /widgets/%2e%2e/passwd HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 400 Bad Request"));
            assert!(resp_str.contains("Invalid path traversal"));
        }

        // 7. Empty subpath (GET /widgets/)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /widgets/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 404 Not Found"));
        }

        thread_handle.join().unwrap();
    }
}
