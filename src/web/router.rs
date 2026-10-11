use std::io::{Cursor, Read};
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, StatusCode};
use crate::controller::MotorController;
use crate::web::handlers::*;

// Embedded Single Page Application Web Assets (pre-gzipped at compile time)
const BASELINE_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/index.html.gz"));
const BASELINE_DESIGN_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/design.html.gz"));
const BASELINE_CSS_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/style.css.gz"));
const BASELINE_JS_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.js.gz"));

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
    html_gz: Arc<[u8]>,
    design_html_gz: Arc<[u8]>,
    css_gz: Arc<[u8]>,
    js_gz: Arc<[u8]>,
    widgets_dir: std::path::PathBuf,
}


// Inspects compression header tokens and minimal payload bounds.
fn is_valid_gz(bytes: &[u8]) -> bool {
    bytes.len() >= 18 && bytes.starts_with(&[0x1f, 0x8b])
}

struct AssetEntry {
    name: &'static str,
    baseline: &'static [u8],
}

const ASSET_ENTRIES: [AssetEntry; 4] = [
    AssetEntry { name: "index.html.gz", baseline: BASELINE_HTML_GZ },
    AssetEntry { name: "design.html.gz", baseline: BASELINE_DESIGN_HTML_GZ },
    AssetEntry { name: "style.css.gz", baseline: BASELINE_CSS_GZ },
    AssetEntry { name: "app.js.gz", baseline: BASELINE_JS_GZ },
];

// Restores embedded client payloads from memory if disk copies fail.
fn load_web_assets_from(assets_dir: &std::path::Path, is_mock: bool) -> (Arc<[u8]>, Arc<[u8]>, Arc<[u8]>, Arc<[u8]>) {
    let custom_dir = std::env::var("WEB_ASSETS_DIR").is_ok();

    if is_mock && !custom_dir && !assets_dir.exists() {
        println!("[ASSETS] Mock mode active: serving baseline embedded web assets from RAM (~18 KB)");
        return (
            Arc::from(BASELINE_HTML_GZ),
            Arc::from(BASELINE_DESIGN_HTML_GZ),
            Arc::from(BASELINE_CSS_GZ),
            Arc::from(BASELINE_JS_GZ),
        );
    }

    let has_all_four = assets_dir.is_dir() && ASSET_ENTRIES.iter().all(|a| assets_dir.join(a.name).is_file());

    if !has_all_four {
        println!(
            "[ASSETS] Self-healing: directory {:?} missing or incomplete. Restoring baseline assets...",
            assets_dir
        );
        let _ = std::fs::create_dir_all(assets_dir);
        let _ = std::fs::remove_file(assets_dir.join(".version"));
        for entry in &ASSET_ENTRIES {
            let p = assets_dir.join(entry.name);
            if !p.is_file() {
                if let Err(e) = std::fs::write(&p, entry.baseline) {
                    eprintln!("[ASSETS WARN] Failed writing {}: {}", entry.name, e);
                }
            }
        }
    }

    let mut loaded: Vec<Arc<[u8]>> = Vec::with_capacity(4);
    for entry in &ASSET_ENTRIES {
        let path = assets_dir.join(entry.name);
        let bytes = match std::fs::read(&path) {
            Ok(b) if is_valid_gz(&b) => Arc::from(b.into_boxed_slice()),
            Ok(corrupted) => {
                eprintln!(
                    "[ASSETS WARN] Corrupted {} in {:?} ({} B, magic header invalid). Restoring embedded baseline.",
                    entry.name, assets_dir, corrupted.len()
                );
                let _ = std::fs::write(&path, entry.baseline);
                let _ = std::fs::remove_file(assets_dir.join(".version"));
                Arc::from(entry.baseline)
            }
            Err(e) => {
                if !is_mock {
                    eprintln!(
                        "[ASSETS WARN] Could not read {} from {:?}: {}. Using embedded baseline.",
                        entry.name, assets_dir, e
                    );
                }
                Arc::from(entry.baseline)
            }
        };
        loaded.push(bytes);
    }

    println!(
        "[ASSETS] Loaded pre-gzipped assets into RAM: index ({} B), design ({} B), CSS ({} B), JS ({} B).",
        loaded[0].len(),
        loaded[1].len(),
        loaded[2].len(),
        loaded[3].len(),
    );

    (loaded[0].clone(), loaded[1].clone(), loaded[2].clone(), loaded[3].clone())
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

// Ensures default modular widget files exist in the specified target path.
pub fn restore_baseline_widgets(widgets_dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(widgets_dir);
    for entry in BASELINE_WIDGETS {
        let p = widgets_dir.join(entry.relative_path);
        let p_gz = widgets_dir.join(format!("{}.gz", entry.relative_path));
        if !p.is_file() && !p_gz.is_file() {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&p, entry.content) {
                eprintln!("[WIDGETS WARN] Failed restoring baseline {}: {}", entry.relative_path, e);
            }
        }
    }
}

impl Router {
    /// Populates compressed frontend cache and prepares request dispatcher.
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
        let is_mock = controller.is_mock();
        let (html_gz, design_html_gz, css_gz, js_gz) = load_web_assets_from(assets_dir, is_mock);
        let widgets_dir = assets_dir.join("widgets");
        if !is_mock || widgets_dir.exists() {
            restore_baseline_widgets(&widgets_dir);
        }
        Self {
            controller,
            html_gz,
            design_html_gz,
            css_gz,
            js_gz,
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
            (Method::Get, "/") | (Method::Get, "/index.html") | (Method::Head, "/") | (Method::Head, "/index.html")
            | (Method::Get, "/run") | (Method::Get, "/run.html") | (Method::Head, "/run") | (Method::Head, "/run.html") => {
                self.respond_static_gz(request, Arc::clone(&self.html_gz), "text/html; charset=utf-8");
            }
            (Method::Get, "/design") | (Method::Get, "/design.html") | (Method::Head, "/design") | (Method::Head, "/design.html") => {
                self.respond_static_gz(request, Arc::clone(&self.design_html_gz), "text/html; charset=utf-8");
            }
            (Method::Get, "/style.css") | (Method::Head, "/style.css") => {
                self.respond_static_gz(request, Arc::clone(&self.css_gz), "text/css; charset=utf-8");
            }
            (Method::Get, "/app.js") | (Method::Head, "/app.js") => {
                self.respond_static_gz(request, Arc::clone(&self.js_gz), "application/javascript; charset=utf-8");
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
            (Method::Get, "/api/status") | (Method::Get, "/api/telemetry") => {
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
            (Method::Post, "/api/emergency-stop") | (Method::Post, "/api/estop") => {
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

            // 2D Joystick Directional Command Endpoint
            (Method::Post, "/api/joystick") => {
                match self.read_json_body::<JoystickPayload>(&mut request) {
                    Ok(payload) => {
                        let (x_port, y_port) = payload.resolve_ports();
                        let (x_speed, y_speed) = payload.resolve_speeds();
                        let stop_action = payload.resolve_stop_action();
                        match self.controller.joystick_drive(&x_port, &y_port, x_speed, y_speed, Some(stop_action)) {
                            Ok(_) => self.respond_json(request, StatusCode(200), &ApiResponse::ok("Joystick command dispatched")),
                            Err(e) => self.respond_json(request, StatusCode(500), &ApiResponse::<()>::err(e)),
                        }
                    }
                    Err(e) => self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(e)),
                }
            }

            // Batch Port Commands: POST /api/batch and POST /api/ports.
            (Method::Post, "/api/batch") | (Method::Post, "/api/ports") => {
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

            // All Ports Endpoint (Motors and Sensors)
            (Method::Get, "/api/ports") => {
                let status = self.controller.get_all_ports_status();
                self.respond_json(request, StatusCode(200), &ApiResponse::ok(status));
            }

            // Single Port Query Endpoint: GET /api/port/{port}
            (Method::Get, p) if p.starts_with("/api/port/") => {
                self.handle_port_get(request, p);
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

    // Queries individual channel metrics and returns JSON representation.
    fn handle_port_get(&self, request: Request, path: &str) {
        let raw_port = path.trim_start_matches("/api/port/");
        let normalized = match normalize_port_name(raw_port) {
            Some(p) => p,
            None => return self.respond_json(request, StatusCode(400), &ApiResponse::<()>::err(format!("Invalid port: '{}'", raw_port))),
        };

        match self.controller.get_port_status(&normalized) {
            Ok(status) => self.respond_json(request, StatusCode(200), &ApiResponse::ok(status)),
            Err(e) => self.respond_json(request, StatusCode(404), &ApiResponse::<()>::err(e)),
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

    // Streams pre-compressed memory payloads with appropriate HTTP headers.
    fn respond_static_gz(&self, request: Request, content: Arc<[u8]>, content_type: &str) {
        let len = content.len();
        let res = Response::new(
            StatusCode(200),
            vec![
                Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
                Header::from_bytes(&b"Content-Encoding"[..], &b"gzip"[..]).unwrap(),
                Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
            ],
            Cursor::new(content),
            Some(len),
            None,
        );
        if let Err(e) = request.respond(res) {
            eprintln!("[HTTP WARN] Failed to send static gzip response: {}", e);
        }
    }

    // Resolves and serves widget assets supporting transparent gzip delivery.
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

        let gz_path = self.widgets_dir.join(format!("{}.gz", subpath));
        let file_path = self.widgets_dir.join(subpath);

        let gz_exists = gz_path.is_file();
        let plain_exists = file_path.is_file();

        if gz_exists && plain_exists {
            let is_gz_newer = match (
                gz_path.metadata().and_then(|m| m.modified()),
                file_path.metadata().and_then(|m| m.modified()),
            ) {
                (Ok(gz_m), Ok(pl_m)) => gz_m >= pl_m,
                _ => false,
            };

            if is_gz_newer {
                if let Ok(bytes) = std::fs::read(&gz_path) {
                    self.respond_static_gz_slice(request, &bytes, mime, is_head);
                    return;
                }
            } else {
                let _ = std::fs::remove_file(&gz_path);
                if let Ok(bytes) = std::fs::read(&file_path) {
                    self.respond_static_plain_slice(request, &bytes, mime, is_head);
                    return;
                }
            }
        } else if gz_exists {
            if let Ok(bytes) = std::fs::read(&gz_path) {
                self.respond_static_gz_slice(request, &bytes, mime, is_head);
                return;
            }
        } else if plain_exists {
            if let Ok(bytes) = std::fs::read(&file_path) {
                self.respond_static_plain_slice(request, &bytes, mime, is_head);
                return;
            }
        }

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

    // Serves pre-gzipped byte sequences with compression transport headers.
    fn respond_static_gz_slice(&self, request: Request, content: &[u8], content_type: &str, is_head: bool) {
        let len = content.len();
        let body = if is_head { Vec::new() } else { content.to_vec() };
        let res = Response::new(
            StatusCode(200),
            vec![
                Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap(),
                Header::from_bytes(&b"Content-Encoding"[..], &b"gzip"[..]).unwrap(),
                Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
            ],
            Cursor::new(body),
            Some(len),
            None,
        );
        if let Err(e) = request.respond(res) {
            eprintln!("[HTTP WARN] Failed to send static gzip response: {}", e);
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
    use flate2::read::GzDecoder;
    use std::io::Read;

    // Expands encoded byte slices into UTF-8 text strings.
    fn decompress_gz(bytes: &[u8]) -> String {
        let mut decoder = GzDecoder::new(bytes);
        let mut s = String::new();
        decoder.read_to_string(&mut s).expect("Failed to decompress gz bytes");
        s
    }

    // Encodes arbitrary input byte slices using flate2 stream.
    fn compress_test(data: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    // Verifies bundled memory binaries have proper headers plus text content.
    #[test]
    fn test_baseline_assets_embedded_and_valid_gzip() {
        assert!(!BASELINE_HTML_GZ.is_empty());
        assert!(!BASELINE_CSS_GZ.is_empty());
        assert!(!BASELINE_JS_GZ.is_empty());

        // Check magic bytes for gzip: 0x1f, 0x8b
        assert_eq!(&BASELINE_HTML_GZ[0..2], &[0x1f, 0x8b]);
        assert_eq!(&BASELINE_CSS_GZ[0..2], &[0x1f, 0x8b]);
        assert_eq!(&BASELINE_JS_GZ[0..2], &[0x1f, 0x8b]);

        let html = decompress_gz(BASELINE_HTML_GZ);
        assert!(html.contains("<!DOCTYPE html>") || html.contains("<html"));

        let css = decompress_gz(BASELINE_CSS_GZ);
        assert!(css.contains("body") || css.contains("margin"));

        let js = decompress_gz(BASELINE_JS_GZ);
        assert!(js.contains("function") || js.contains("const") || js.contains("let") || js.contains("document"));
    }

    // Asserts non-existent filesystem locations are recreated and populated.
    #[test]
    fn test_self_healing_missing_directory() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_missing_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let (html, _design, css, js) = load_web_assets_from(&temp_dir, false);

        // Verify directory was created and populated
        assert!(temp_dir.exists());
        assert!(temp_dir.join("index.html.gz").exists());
        assert!(temp_dir.join("design.html.gz").exists());
        assert!(temp_dir.join("style.css.gz").exists());
        assert!(temp_dir.join("app.js.gz").exists());

        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(_design.as_ref(), BASELINE_DESIGN_HTML_GZ);
        assert_eq!(css.as_ref(), BASELINE_CSS_GZ);
        assert_eq!(js.as_ref(), BASELINE_JS_GZ);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // Checks truncated bundles and obsolete marker tokens are reconstructed.
    #[test]
    fn test_self_healing_corrupted_or_fewer_than_3_files() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_partial_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Write only 1 partial file and a stale .version
        std::fs::write(temp_dir.join("index.html.gz"), b"partial").unwrap();
        std::fs::write(temp_dir.join(".version"), b"stale_sha").unwrap();

        let (html, _design, css, js) = load_web_assets_from(&temp_dir, false);

        // Verify all 4 files are healed to baseline and .version is removed
        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(_design.as_ref(), BASELINE_DESIGN_HTML_GZ);
        assert_eq!(css.as_ref(), BASELINE_CSS_GZ);
        assert_eq!(js.as_ref(), BASELINE_JS_GZ);
        assert!(!temp_dir.join(".version").exists());

        let disk_html = std::fs::read(temp_dir.join("index.html.gz")).unwrap();
        assert_eq!(disk_html, BASELINE_HTML_GZ);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // Asserts empty payloads trigger recovery while retaining sound items.
    #[test]
    fn test_self_healing_corrupted_zero_length_file() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_zero_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let valid_design = compress_test(b"<h1>Design</h1>");
        let valid_css = compress_test(b"body { color: blue; }");
        let valid_js = compress_test(b"console.log('test');");

        // 4 files exist, but index.html.gz is 0 bytes (corrupted)
        std::fs::write(temp_dir.join("index.html.gz"), b"").unwrap();
        std::fs::write(temp_dir.join("design.html.gz"), &valid_design).unwrap();
        std::fs::write(temp_dir.join("style.css.gz"), &valid_css).unwrap();
        std::fs::write(temp_dir.join("app.js.gz"), &valid_js).unwrap();
        std::fs::write(temp_dir.join(".version"), b"stale_sha_12345").unwrap();

        let (html, design, css, js) = load_web_assets_from(&temp_dir, false);

        // HTML should be restored to baseline, others kept as valid custom
        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(design.as_ref(), valid_design.as_slice());
        assert_eq!(css.as_ref(), valid_css.as_slice());
        assert_eq!(js.as_ref(), valid_js.as_slice());
        assert!(!temp_dir.join(".version").exists());

        let disk_html = std::fs::read(temp_dir.join("index.html.gz")).unwrap();
        assert_eq!(disk_html, BASELINE_HTML_GZ);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // Ensures valid custom on-disk bundles are preserved without replacement.
    #[test]
    fn test_load_existing_updated_assets_without_overwriting() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_custom_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let custom_html = compress_test(b"<h1>Custom App</h1>");
        let custom_design = compress_test(b"<h1>Custom Design</h1>");
        let custom_css = compress_test(b"h1 { color: red; }");
        let custom_js = compress_test(b"console.log('custom');");

        std::fs::write(temp_dir.join("index.html.gz"), &custom_html).unwrap();
        std::fs::write(temp_dir.join("design.html.gz"), &custom_design).unwrap();
        std::fs::write(temp_dir.join("style.css.gz"), &custom_css).unwrap();
        std::fs::write(temp_dir.join("app.js.gz"), &custom_js).unwrap();
        std::fs::write(temp_dir.join(".version"), b"valid_sha_abcdef").unwrap();

        let (html, design, css, js) = load_web_assets_from(&temp_dir, false);

        assert_eq!(html.as_ref(), custom_html.as_slice());
        assert_eq!(design.as_ref(), custom_design.as_slice());
        assert_eq!(css.as_ref(), custom_css.as_slice());
        assert_eq!(js.as_ref(), custom_js.as_slice());
        assert!(temp_dir.join(".version").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // Verifies simulation uses built-in memory buffers if disk path is absent.
    #[test]
    fn test_mock_fallback_when_directory_missing() {
        let non_existent = std::path::PathBuf::from("/non_existent_path_ev3_mock_test");
        let (html, design, css, js) = load_web_assets_from(&non_existent, true);
        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(design.as_ref(), BASELINE_DESIGN_HTML_GZ);
        assert_eq!(css.as_ref(), BASELINE_CSS_GZ);
        assert_eq!(js.as_ref(), BASELINE_JS_GZ);
    }

    // Exercises network server across primary endpoints and HTTP methods.
    #[test]
    fn test_http_routes_serve_gzipped_assets_and_headers() {
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
            for _ in 0..21 {
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
            assert!(resp_str.contains("Content-Encoding: gzip"));
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
            assert!(resp_str.contains("Content-Encoding: gzip"));
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
            assert!(resp_str.contains("Content-Encoding: gzip"));
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
            assert!(resp_str.contains("Content-Encoding: gzip"));
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
            assert!(resp_str.contains("Content-Encoding: gzip"));
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
            assert!(resp_str.contains("Content-Encoding: gzip"));
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

        // 9. Test GET /api/ports
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /api/ports HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"motors\":["));
            assert!(resp_str.contains("\"sensors\":["));
        }

        // 10. Test GET /api/port/A
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /api/port/A HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"port\":\"A\""));
            assert!(resp_str.contains("\"address\":\"outA\""));
        }

        // 11. Test POST /api/port/outA with inferred timed run
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = b"{\"speed\": 500, \"time_ms\": 1000}";
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

        // 12. Test GET /api/port/invalid (400 Bad Request)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /api/port/invalid HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 400 Bad Request"));
            assert!(resp_str.contains("\"success\":false"));
        }

        // 13. Test GET /api/port/1 (Sensor on input port 1)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /api/port/1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"port\":\"1\""));
            assert!(resp_str.contains("\"driver_name\":\"lego-ev3-touch (mock)\""));
        }

        // 14. Test POST /api/port/2 (Change sensor mode)
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

        // 15. Test GET /api/telemetry (alias for /api/status)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(b"GET /api/telemetry HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("\"success\":true"));
            assert!(resp_str.contains("\"motors\":["));
        }

        // 16. Test POST /api/estop (alias for /api/emergency-stop)
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

        // 17. Test POST /api/batch
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

        // 18. Cross-origin POST with port 80 in Host omitted in Origin.
        {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let req = "POST /api/estop HTTP/1.1\r\nHost: localhost:80\r\nOrigin: http://localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
        }

        // 19. Test genuine cross-origin POST rejected with 403 Forbidden
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

    #[test]
    fn test_restore_baseline_widgets_restores_individual_missing_files_when_manifest_exists() {
        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_restore_missing_{}", test_id));
        let widgets_dir = temp_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        // Write only manifest.json, omitting joystick/widget.js
        std::fs::write(widgets_dir.join("manifest.json"), b"{\"timestamp\": 1000}").unwrap();
        assert!(!widgets_dir.join("joystick").join("widget.js").exists());

        restore_baseline_widgets(&widgets_dir);

        // Missing joystick/widget.js should be restored from embedded baseline
        assert!(widgets_dir.join("joystick").join("widget.js").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
