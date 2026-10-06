use std::io::Cursor;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, StatusCode};
use crate::controller::MotorController;
use crate::web::handlers::*;

// Embedded Single Page Application Web Assets (pre-gzipped at compile time)
const BASELINE_HTML_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/index.html.gz"));
const BASELINE_CSS_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/style.css.gz"));
const BASELINE_JS_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.js.gz"));

pub struct Router {
    controller: Arc<MotorController>,
    html_gz: Arc<[u8]>,
    css_gz: Arc<[u8]>,
    js_gz: Arc<[u8]>,
}


fn is_valid_gz(bytes: &[u8]) -> bool {
    bytes.len() >= 18 && bytes.starts_with(&[0x1f, 0x8b])
}

fn load_web_assets_from(assets_dir: &std::path::Path, is_mock: bool) -> (Arc<[u8]>, Arc<[u8]>, Arc<[u8]>) {
    let custom_dir = std::env::var("WEB_ASSETS_DIR").is_ok();

    // If running in mock mode on host without a custom test directory, and target directory does not exist,
    // immediately fall back to embedded baseline assets without attempting rootfs directory creation.
    if is_mock && !custom_dir && !assets_dir.exists() {
        println!("[ASSETS] Mock mode active: serving baseline embedded web assets from RAM (~12 KB)");
        return (
            Arc::from(BASELINE_HTML_GZ),
            Arc::from(BASELINE_CSS_GZ),
            Arc::from(BASELINE_JS_GZ),
        );
    }

    // Inspect directory for .gz files
    let mut gz_count = 0;
    if assets_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(assets_dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_file() {
                        let fname = entry.file_name();
                        if fname.to_string_lossy().ends_with(".gz") {
                            gz_count += 1;
                        }
                    }
                }
            }
        }
    }

    let html_path = assets_dir.join("index.html.gz");
    let css_path = assets_dir.join("style.css.gz");
    let js_path = assets_dir.join("app.js.gz");
    let has_all_three = html_path.is_file() && css_path.is_file() && js_path.is_file();

    // Self-healing check: if directory does not exist, contains fewer than 3 .gz files, or missing any required asset
    if !assets_dir.exists() || gz_count < 3 || !has_all_three {
        println!(
            "[ASSETS] Self-healing: directory {:?} missing or incomplete (found {} .gz files). Restoring baseline assets...",
            assets_dir, gz_count
        );
        if let Err(e) = std::fs::create_dir_all(assets_dir) {
            eprintln!("[ASSETS WARN] Failed to create {:?}: {}", assets_dir, e);
        } else {
            let write_html = std::fs::write(&html_path, BASELINE_HTML_GZ);
            let write_css = std::fs::write(&css_path, BASELINE_CSS_GZ);
            let write_js = std::fs::write(&js_path, BASELINE_JS_GZ);
            // Invalidate stale .version so the boot updater will re-download latest assets
            if let Err(e) = std::fs::remove_file(assets_dir.join(".version")) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("[ASSETS WARN] Failed to remove .version: {}", e);
                }
            }

            if let Err(e) = write_html {
                eprintln!("[ASSETS WARN] Failed writing index.html.gz: {}", e);
            }
            if let Err(e) = write_css {
                eprintln!("[ASSETS WARN] Failed writing style.css.gz: {}", e);
            }
            if let Err(e) = write_js {
                eprintln!("[ASSETS WARN] Failed writing app.js.gz: {}", e);
            }
        }
    }

    // Load assets from disk into RAM, verifying gzip integrity and falling back to embedded bytes
    let load_asset = |path: &std::path::Path, baseline: &'static [u8], name: &str| -> Arc<[u8]> {
        match std::fs::read(path) {
            Ok(bytes) if is_valid_gz(&bytes) => Arc::from(bytes.into_boxed_slice()),
            Ok(corrupted) => {
                eprintln!(
                    "[ASSETS WARN] Corrupted {} in {:?} ({} B, magic header invalid). Restoring embedded baseline.",
                    name, assets_dir, corrupted.len()
                );
                if let Err(e) = std::fs::write(path, baseline) {
                    eprintln!("[ASSETS WARN] Failed to restore baseline {}: {}", name, e);
                }
                if let Err(e) = std::fs::remove_file(assets_dir.join(".version")) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("[ASSETS WARN] Failed to remove .version: {}", e);
                    }
                }
                Arc::from(baseline)
            }
            Err(e) => {
                if is_mock {
                    println!("[ASSETS] Mock fallback for {}: {}", name, e);
                } else {
                    eprintln!(
                        "[ASSETS WARN] Could not read {} from {:?}: {}. Using embedded baseline.",
                        name, assets_dir, e
                    );
                }
                Arc::from(baseline)
            }
        }
    };

    let html_gz = load_asset(&html_path, BASELINE_HTML_GZ, "index.html.gz");
    let css_gz = load_asset(&css_path, BASELINE_CSS_GZ, "style.css.gz");
    let js_gz = load_asset(&js_path, BASELINE_JS_GZ, "app.js.gz");

    println!(
        "[ASSETS] Loaded pre-gzipped assets into RAM: HTML ({} B), CSS ({} B), JS ({} B). Total: {} B",
        html_gz.len(),
        css_gz.len(),
        js_gz.len(),
        html_gz.len() + css_gz.len() + js_gz.len()
    );

    (html_gz, css_gz, js_gz)
}

fn load_web_assets(is_mock: bool) -> (Arc<[u8]>, Arc<[u8]>, Arc<[u8]>) {
    use std::path::PathBuf;

    let assets_dir: PathBuf = std::env::var("WEB_ASSETS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/robot/web_assets"));

    load_web_assets_from(&assets_dir, is_mock)
}

impl Router {
    pub fn new(controller: Arc<MotorController>) -> Self {
        let is_mock = controller.is_mock();
        let (html_gz, css_gz, js_gz) = load_web_assets(is_mock);
        Self {
            controller,
            html_gz,
            css_gz,
            js_gz,
        }
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
            // Static Web Assets (supporting GET and HEAD)
            (Method::Get, "/") | (Method::Get, "/index.html") | (Method::Head, "/") | (Method::Head, "/index.html") => {
                self.respond_static_gz(request, Arc::clone(&self.html_gz), "text/html; charset=utf-8");
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

    fn decompress_gz(bytes: &[u8]) -> String {
        let mut decoder = GzDecoder::new(bytes);
        let mut s = String::new();
        decoder.read_to_string(&mut s).expect("Failed to decompress gz bytes");
        s
    }

    fn compress_test(data: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

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

    #[test]
    fn test_self_healing_missing_directory() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_missing_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);

        let (html, css, js) = load_web_assets_from(&temp_dir, false);

        // Verify directory was created and populated
        assert!(temp_dir.exists());
        assert!(temp_dir.join("index.html.gz").exists());
        assert!(temp_dir.join("style.css.gz").exists());
        assert!(temp_dir.join("app.js.gz").exists());

        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(css.as_ref(), BASELINE_CSS_GZ);
        assert_eq!(js.as_ref(), BASELINE_JS_GZ);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_self_healing_corrupted_or_fewer_than_3_files() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_partial_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Write only 1 partial file and a stale .version
        std::fs::write(temp_dir.join("index.html.gz"), b"partial").unwrap();
        std::fs::write(temp_dir.join(".version"), b"stale_sha").unwrap();

        let (html, css, js) = load_web_assets_from(&temp_dir, false);

        // Verify all 3 files are healed to baseline and .version is removed
        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(css.as_ref(), BASELINE_CSS_GZ);
        assert_eq!(js.as_ref(), BASELINE_JS_GZ);
        assert!(!temp_dir.join(".version").exists());

        let disk_html = std::fs::read(temp_dir.join("index.html.gz")).unwrap();
        assert_eq!(disk_html, BASELINE_HTML_GZ);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_self_healing_corrupted_zero_length_file() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_zero_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let valid_css = compress_test(b"body { color: blue; }");
        let valid_js = compress_test(b"console.log('test');");

        // 3 files exist, but index.html.gz is 0 bytes (corrupted)
        std::fs::write(temp_dir.join("index.html.gz"), b"").unwrap();
        std::fs::write(temp_dir.join("style.css.gz"), &valid_css).unwrap();
        std::fs::write(temp_dir.join("app.js.gz"), &valid_js).unwrap();
        std::fs::write(temp_dir.join(".version"), b"stale_sha_12345").unwrap();

        let (html, css, js) = load_web_assets_from(&temp_dir, false);

        // HTML should be restored to baseline, CSS & JS kept as valid custom
        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(css.as_ref(), valid_css.as_slice());
        assert_eq!(js.as_ref(), valid_js.as_slice());
        assert!(!temp_dir.join(".version").exists());

        let disk_html = std::fs::read(temp_dir.join("index.html.gz")).unwrap();
        assert_eq!(disk_html, BASELINE_HTML_GZ);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_existing_updated_assets_without_overwriting() {
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_assets_custom_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let custom_html = compress_test(b"<h1>Custom App</h1>");
        let custom_css = compress_test(b"h1 { color: red; }");
        let custom_js = compress_test(b"console.log('custom');");

        std::fs::write(temp_dir.join("index.html.gz"), &custom_html).unwrap();
        std::fs::write(temp_dir.join("style.css.gz"), &custom_css).unwrap();
        std::fs::write(temp_dir.join("app.js.gz"), &custom_js).unwrap();
        std::fs::write(temp_dir.join(".version"), b"valid_sha_abcdef").unwrap();

        let (html, css, js) = load_web_assets_from(&temp_dir, false);

        assert_eq!(html.as_ref(), custom_html.as_slice());
        assert_eq!(css.as_ref(), custom_css.as_slice());
        assert_eq!(js.as_ref(), custom_js.as_slice());
        assert!(temp_dir.join(".version").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_mock_fallback_when_directory_missing() {
        let non_existent = std::path::PathBuf::from("/non_existent_path_ev3_mock_test");
        let (html, css, js) = load_web_assets_from(&non_existent, true);
        assert_eq!(html.as_ref(), BASELINE_HTML_GZ);
        assert_eq!(css.as_ref(), BASELINE_CSS_GZ);
        assert_eq!(js.as_ref(), BASELINE_JS_GZ);
    }

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
            for _ in 0..7 {
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

        thread_handle.join().unwrap();
    }
}
