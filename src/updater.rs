use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};

/// Catalog schema recording widget revision numbers and filenames.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidgetManifest {
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub widgets: HashMap<String, WidgetEntry>,
}

/// Metadata entry describing asset files for a single component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidgetEntry {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub files: Vec<String>,
}

/// Result tally capturing changed packages and transfer diagnostics.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UpdateResult {
    pub updated_count: usize,
    pub files_downloaded: usize,
    pub errors: Vec<String>,
}

/// Reads catalog metadata from local filesystem storage if present.
pub fn load_local_manifest(widgets_dir: &Path) -> Option<WidgetManifest> {
    let path = widgets_dir.join("manifest.json");
    let content = std::fs::read(&path).ok()?;
    serde_json::from_slice::<WidgetManifest>(&content).ok()
}

/// Fetches remote manifest index over network using system transfer tool.
pub fn fetch_remote_manifest(url: &str, timeout_secs: u64) -> Result<WidgetManifest, String> {
    let timeout_str = timeout_secs.to_string();
    let output = Command::new("curl")
        .args([
            "-s",
            "-f",
            "-m",
            &timeout_str,
            "--connect-timeout",
            &timeout_str,
            url,
        ])
        .output()
        .map_err(|e| format!("Failed to execute curl: {}", e))?;

    if !output.status.success() {
        return Err(format!("curl transfer failed: code {:?}", output.status.code()));
    }

    serde_json::from_slice::<WidgetManifest>(&output.stdout)
        .map_err(|e| format!("Invalid manifest JSON payload: {}", e))
}

/// Validates component name or filename against path traversal exploits.
pub fn is_safe_component_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.contains('\0')
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Downloads individual asset file atomically using temporary swap file.
pub fn download_file(url: &str, dest_path: &Path, timeout_secs: u64) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let tmp_dest = dest_path.with_extension("tmp_download");
    let timeout_str = timeout_secs.to_string();
    let output = Command::new("curl")
        .args([
            "-s",
            "-f",
            "-m",
            &timeout_str,
            "--connect-timeout",
            &timeout_str,
            url,
            "-o",
        ])
        .arg(&tmp_dest)
        .output()
        .map_err(|e| format!("Failed executing transfer process: {}", e))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_dest);
        return Err(format!("Download failed for {}: code {:?}", url, output.status.code()));
    }

    std::fs::rename(&tmp_dest, dest_path)
        .map_err(|e| format!("Failed moving temporary file: {}", e))
}

/// Compares revision timestamps and retrieves newer asset files.
pub fn check_and_update_widgets(
    manifest_url: &str,
    widgets_dir: &Path,
    timeout_secs: u64,
) -> Result<UpdateResult, String> {
    let remote_manifest = fetch_remote_manifest(manifest_url, timeout_secs)?;
    let local_manifest = load_local_manifest(widgets_dir);

    let base_url = manifest_url
        .rsplit_once('/')
        .map(|(base, _)| base)
        .unwrap_or(manifest_url);

    let mut result = UpdateResult::default();
    let mut modified_manifest = local_manifest.unwrap_or_else(|| WidgetManifest {
        timestamp: 0,
        widgets: HashMap::new(),
    });

    for (widget_name, remote_entry) in &remote_manifest.widgets {
        if !is_safe_component_name(widget_name) {
            result.errors.push(format!("Unsafe widget name rejected: '{}'", widget_name));
            continue;
        }

        let should_update = match modified_manifest.widgets.get(widget_name) {
            Some(local_entry) => {
                remote_entry.timestamp > local_entry.timestamp
                    || remote_entry.files.iter().any(|f| !widgets_dir.join(widget_name).join(f).exists())
            }
            None => true,
        };

        if should_update {
            let files = if remote_entry.files.is_empty() {
                vec!["widget.js".to_string()]
            } else {
                remote_entry.files.clone()
            };

            let mut all_files_valid = true;
            for file in &files {
                if !is_safe_component_name(file) {
                    all_files_valid = false;
                    result.errors.push(format!("Unsafe file name in widget '{}': '{}'", widget_name, file));
                    break;
                }
            }
            if !all_files_valid {
                continue;
            }

            let widget_target_dir = widgets_dir.join(widget_name);
            let _ = std::fs::create_dir_all(&widget_target_dir);

            let mut staged_downloads = Vec::new();
            let mut widget_download_ok = true;

            for file in &files {
                let file_url = format!("{}/{}/{}", base_url, widget_name, file);
                let final_path = widget_target_dir.join(file);
                let staged_path = widget_target_dir.join(format!("{}.tmp_stage", file));

                match download_file(&file_url, &staged_path, timeout_secs) {
                    Ok(()) => {
                        staged_downloads.push((staged_path, final_path));
                    }
                    Err(e) => {
                        widget_download_ok = false;
                        result.errors.push(e);
                        break;
                    }
                }
            }

            if widget_download_ok {
                for (staged, final_path) in staged_downloads {
                    if let Err(e) = std::fs::rename(&staged, &final_path) {
                        result.errors.push(format!("Failed promoting staged file: {}", e));
                    } else {
                        result.files_downloaded += 1;
                        let gz_companion = PathBuf::from(format!("{}.gz", final_path.display()));
                        if gz_companion.is_file() {
                            let _ = std::fs::remove_file(gz_companion);
                        }
                    }
                }
                modified_manifest.widgets.insert(widget_name.clone(), remote_entry.clone());
                result.updated_count += 1;
            } else {
                for (staged, _) in staged_downloads {
                    let _ = std::fs::remove_file(staged);
                }
            }
        }
    }

    if result.updated_count > 0 {
        modified_manifest.timestamp = remote_manifest.timestamp;
        let manifest_path = widgets_dir.join("manifest.json");
        let _ = std::fs::create_dir_all(widgets_dir);
        if let Ok(serialized) = serde_json::to_string_pretty(&modified_manifest) {
            let _ = std::fs::write(&manifest_path, serialized);
        }
    }

    Ok(result)
}

/// Spawns silent background thread checking remote repository on startup.
pub fn start_background_updater(
    manifest_url: String,
    widgets_dir: PathBuf,
    timeout_secs: u64,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("widget-updater".into())
        .spawn(move || {
            let ip = crate::sysfs::DisplayController::detect_ip();
            if ip == "No network" {
                println!("[WIDGETS] Offline: no active network. Serving local widgets.");
                return;
            }

            match check_and_update_widgets(&manifest_url, &widgets_dir, timeout_secs) {
                Ok(summary) => {
                    if summary.updated_count > 0 {
                        println!(
                            "[WIDGETS] Updated {} widget(s) ({} file(s)) from remote catalog.",
                            summary.updated_count, summary.files_downloaded
                        );
                    } else {
                        println!("[WIDGETS] Widgets up to date. Serving local widgets.");
                    }
                }
                Err(err) => {
                    println!("[WIDGETS] Background check complete ({}); serving local widgets.", err);
                }
            }
        })
        .expect("Failed to spawn widget-updater thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization_round_trip() {
        let mut widgets = HashMap::new();
        widgets.insert("joystick".into(), WidgetEntry {
            version: "1.2.0".into(),
            timestamp: 1728671111,
            files: vec!["widget.js".into(), "widget.css".into()],
        });

        let manifest = WidgetManifest {
            timestamp: 1728671111,
            widgets,
        };

        let json = serde_json::to_string(&manifest).expect("Serialization failed");
        let parsed: WidgetManifest = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(manifest, parsed);
    }

    #[test]
    fn test_load_local_manifest_missing_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ev3_test_updater_missing_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&temp_dir);
        assert_eq!(load_local_manifest(&temp_dir), None);
    }

    #[test]
    fn test_updater_offline_unreachable_returns_error_gracefully() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ev3_test_offline_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let widgets_dir = temp_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        let manifest_url = "http://127.0.0.1:1/manifest.json";
        let res = check_and_update_widgets(manifest_url, &widgets_dir, 1);
        assert!(res.is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_updater_ignores_same_or_older_timestamp() {
        use tiny_http::{Response, StatusCode};

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_no_update_{}", test_id));
        let widgets_dir = temp_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        let initial_manifest = r#"{
            "timestamp": 5000,
            "widgets": {
                "joystick": {
                    "version": "1.0.0",
                    "timestamp": 5000,
                    "files": ["widget.js"]
                }
            }
        }"#;
        std::fs::write(widgets_dir.join("manifest.json"), initial_manifest).unwrap();
        std::fs::create_dir_all(widgets_dir.join("joystick")).unwrap();
        std::fs::write(widgets_dir.join("joystick").join("widget.js"), "// existing").unwrap();

        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();
        let remote_manifest = r#"{
            "timestamp": 4000,
            "widgets": {
                "joystick": {
                    "version": "0.9.0",
                    "timestamp": 4000,
                    "files": ["widget.js"]
                }
            }
        }"#;

        let remote_handle = std::thread::spawn(move || {
            if let Ok(req) = remote_server.recv() {
                let res = Response::from_string(remote_manifest.to_string())
                    .with_status_code(StatusCode(200));
                let _ = req.respond(res);
            }
        });

        let manifest_url = format!("http://127.0.0.1:{}/manifest.json", remote_port);
        let res = check_and_update_widgets(&manifest_url, &widgets_dir, 3).unwrap();
        assert_eq!(res.updated_count, 0);
        assert_eq!(res.files_downloaded, 0);

        let content = std::fs::read_to_string(widgets_dir.join("joystick").join("widget.js")).unwrap();
        assert_eq!(content, "// existing");

        remote_handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_e2e_updater_fetches_newer_widgets_and_server_serves_them() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
        use tiny_http::{Response, StatusCode};
        use crate::controller::MotorController;
        use crate::web::Router;

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_assets_dir = std::env::temp_dir().join(format!("ev3_test_e2e_assets_{}", test_id));
        let widgets_dir = temp_assets_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        // 1. Initial shipped baseline widget on disk
        let shipped_content = "// shipped baseline joystick v1.0.0";
        let shipped_manifest = r#"{
            "timestamp": 1000,
            "widgets": {
                "joystick": {
                    "version": "1.0.0",
                    "timestamp": 1000,
                    "files": ["widget.js"]
                }
            }
        }"#;
        std::fs::write(widgets_dir.join("manifest.json"), shipped_manifest).unwrap();
        std::fs::create_dir_all(widgets_dir.join("joystick")).unwrap();
        std::fs::write(widgets_dir.join("joystick").join("widget.js"), shipped_content).unwrap();

        // 2. Spawn mock remote HTTP server with newer widget updates
        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();

        let newer_content = "// latest widgets from web are newer than shipped v2.0.0";
        let remote_manifest = r#"{
            "timestamp": 2000,
            "widgets": {
                "joystick": {
                    "version": "2.0.0",
                    "timestamp": 2000,
                    "files": ["widget.js"]
                }
            }
        }"#;

        let remote_handle = std::thread::spawn(move || {
            for _ in 0..2 {
                if let Ok(req) = remote_server.recv() {
                    let url = req.url().to_string();
                    if url.contains("manifest.json") {
                        let res = Response::from_string(remote_manifest.to_string())
                            .with_status_code(StatusCode(200));
                        let _ = req.respond(res);
                    } else if url.contains("joystick/widget.js") {
                        let res = Response::from_string(newer_content.to_string())
                            .with_status_code(StatusCode(200));
                        let _ = req.respond(res);
                    } else {
                        let _ = req.respond(Response::empty(StatusCode(404)));
                    }
                }
            }
        });

        // 3. Launch mock updater to check and update widgets
        let manifest_url = format!("http://127.0.0.1:{}/manifest.json", remote_port);
        let update_res = check_and_update_widgets(&manifest_url, &widgets_dir, 3)
            .expect("Updater execution failed");

        assert_eq!(update_res.updated_count, 1);
        assert_eq!(update_res.files_downloaded, 1);
        assert!(update_res.errors.is_empty());

        remote_handle.join().unwrap();

        // 4. Verify on-disk update
        let disk_content = std::fs::read_to_string(widgets_dir.join("joystick").join("widget.js"))
            .expect("Failed reading updated file on disk");
        assert_eq!(disk_content, newer_content);
        assert_ne!(disk_content, shipped_content);

        let local_manifest = load_local_manifest(&widgets_dir).expect("Failed to load local manifest");
        assert_eq!(local_manifest.timestamp, 2000);
        assert_eq!(local_manifest.widgets.get("joystick").unwrap().timestamp, 2000);

        // 5. Confirm EV3 HTTP server serves the newer file content over /widgets/*
        let controller = MotorController::new(true, 50);
        let router = Arc::new(Router::new_with_dir(controller, &temp_assets_dir));
        let ev3_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let ev3_port = ev3_server.server_addr().to_ip().unwrap().port();

        let router_clone = router.clone();
        let server_clone = Arc::new(ev3_server);
        let s_handle = server_clone.clone();
        let ev3_thread = std::thread::spawn(move || {
            if let Ok(req) = s_handle.recv() {
                router_clone.handle_request(req);
            }
        });

        let mut stream = TcpStream::connect(("127.0.0.1", ev3_port)).unwrap();
        let req = format!(
            "GET /widgets/joystick/widget.js?v=2000 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).unwrap();

        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).unwrap();
        let resp_str = String::from_utf8_lossy(&resp);

        assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
        assert!(resp_str.contains("Cache-Control: no-cache"));
        assert!(resp_str.contains("Content-Type: application/javascript; charset=utf-8"));
        assert!(resp_str.contains("// latest widgets from web are newer than shipped v2.0.0"));
        assert!(!resp_str.contains("// shipped baseline joystick v1.0.0"));

        ev3_thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
    }

    #[test]
    fn test_transparent_route_serves_gzip_when_present_and_plain_when_not() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use crate::controller::MotorController;
        use crate::web::Router;

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_assets_dir = std::env::temp_dir().join(format!("ev3_test_transparent_{}", test_id));
        let widgets_dir = temp_assets_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        // 1. Create a gzipped widget file on disk (widgets/button/widget.js.gz)
        let button_dir = widgets_dir.join("button");
        std::fs::create_dir_all(&button_dir).unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"// gzipped button content").unwrap();
        let gz_bytes = encoder.finish().unwrap();
        std::fs::write(button_dir.join("widget.js.gz"), &gz_bytes).unwrap();

        // 2. Create a plain uncompressed widget file on disk (widgets/slider/widget.js)
        let slider_dir = widgets_dir.join("slider");
        std::fs::create_dir_all(&slider_dir).unwrap();
        std::fs::write(slider_dir.join("widget.js"), b"// plain uncompressed slider content").unwrap();

        // 3. Start server
        let controller = MotorController::new(true, 50);
        let router = Arc::new(Router::new_with_dir(controller, &temp_assets_dir));
        let ev3_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let ev3_port = ev3_server.server_addr().to_ip().unwrap().port();

        let router_clone = router.clone();
        let server_clone = Arc::new(ev3_server);
        let s_handle = server_clone.clone();
        let ev3_thread = std::thread::spawn(move || {
            for _ in 0..2 {
                if let Ok(req) = s_handle.recv() {
                    router_clone.handle_request(req);
                }
            }
        });

        // Test button (should serve gzip)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", ev3_port)).unwrap();
            stream.write_all(b"GET /widgets/button/widget.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        // Test slider (should serve plain uncompressed, NO Content-Encoding: gzip)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", ev3_port)).unwrap();
            stream.write_all(b"GET /widgets/slider/widget.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
            assert!(resp_str.contains("// plain uncompressed slider content"));
        }

        ev3_thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
    }

    #[test]
    fn test_partial_download_failure_rolls_back_and_preserves_existing_files() {
        use tiny_http::{Response, StatusCode};

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_partial_{}", test_id));
        let widgets_dir = temp_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(widgets_dir.join("joystick")).unwrap();

        // 1. Existing baseline file on disk
        let original_content = "// original working joystick v1.0.0";
        std::fs::write(widgets_dir.join("joystick").join("widget.js"), original_content).unwrap();

        let initial_manifest = r#"{
            "timestamp": 1000,
            "widgets": {
                "joystick": {
                    "version": "1.0.0",
                    "timestamp": 1000,
                    "files": ["widget.js"]
                }
            }
        }"#;
        std::fs::write(widgets_dir.join("manifest.json"), initial_manifest).unwrap();

        // 2. Remote server provides manifest with 2 files, but fails on second file (widget.css)
        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();

        let remote_manifest = r#"{
            "timestamp": 2000,
            "widgets": {
                "joystick": {
                    "version": "2.0.0",
                    "timestamp": 2000,
                    "files": ["widget.js", "widget.css"]
                }
            }
        }"#;

        let remote_handle = std::thread::spawn(move || {
            for _ in 0..2 {
                if let Ok(req) = remote_server.recv() {
                    let url = req.url().to_string();
                    if url.contains("manifest.json") {
                        let res = Response::from_string(remote_manifest.to_string())
                            .with_status_code(StatusCode(200));
                        let _ = req.respond(res);
                    } else if url.contains("joystick/widget.js") {
                        let res = Response::from_string("// new v2 joystick that should rollback")
                            .with_status_code(StatusCode(200));
                        let _ = req.respond(res);
                    } else {
                        // Fail on widget.css
                        let _ = req.respond(Response::empty(StatusCode(500)));
                    }
                }
            }
        });

        let manifest_url = format!("http://127.0.0.1:{}/manifest.json", remote_port);
        let res = check_and_update_widgets(&manifest_url, &widgets_dir, 3)
            .expect("check_and_update_widgets should return Ok with error list");

        assert_eq!(res.updated_count, 0);
        assert!(!res.errors.is_empty());

        // 3. Verify original file on disk is completely preserved and unmodified
        let content_on_disk = std::fs::read_to_string(widgets_dir.join("joystick").join("widget.js")).unwrap();
        assert_eq!(content_on_disk, original_content);

        // 4. Verify no stray temporary staging files remain
        assert!(!widgets_dir.join("joystick").join("widget.js.tmp_stage").exists());

        // 5. Verify local manifest timestamp was NOT bumped
        let local_manifest = load_local_manifest(&widgets_dir).unwrap();
        assert_eq!(local_manifest.timestamp, 1000);

        remote_handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_updater_rejects_unsafe_path_traversal_in_manifest() {
        use tiny_http::{Response, StatusCode};

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_traversal_{}", test_id));
        let widgets_dir = temp_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();

        let malicious_manifest = r#"{
            "timestamp": 2000,
            "widgets": {
                "../escaped": {
                    "version": "2.0.0",
                    "timestamp": 2000,
                    "files": ["widget.js"]
                },
                "evil": {
                    "version": "2.0.0",
                    "timestamp": 2000,
                    "files": ["../../escaped_file.js"]
                }
            }
        }"#;

        let remote_handle = std::thread::spawn(move || {
            if let Ok(req) = remote_server.recv() {
                let res = Response::from_string(malicious_manifest.to_string())
                    .with_status_code(StatusCode(200));
                let _ = req.respond(res);
            }
        });

        let manifest_url = format!("http://127.0.0.1:{}/manifest.json", remote_port);
        let res = check_and_update_widgets(&manifest_url, &widgets_dir, 3)
            .expect("check_and_update_widgets should return Ok with error list");

        assert_eq!(res.updated_count, 0);
        assert_eq!(res.errors.len(), 2);
        assert!(res.errors.iter().any(|e| e.contains("Unsafe widget name rejected")));
        assert!(res.errors.iter().any(|e| e.contains("Unsafe file name in widget")));

        assert!(!temp_dir.join("escaped").exists());
        assert!(!temp_dir.join("escaped_file.js").exists());

        remote_handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_updating_plain_file_purges_stale_gzip_companion_and_serves_new_content() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use tiny_http::{Response, StatusCode};
        use crate::controller::MotorController;
        use crate::web::Router;

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_assets_dir = std::env::temp_dir().join(format!("ev3_test_stale_gz_{}", test_id));
        let widgets_dir = temp_assets_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
        let button_dir = widgets_dir.join("button");
        std::fs::create_dir_all(&button_dir).unwrap();

        // 1. Initial stale compressed button asset on disk
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"// old stale gzipped button content v1").unwrap();
        let old_gz_bytes = encoder.finish().unwrap();
        std::fs::write(button_dir.join("widget.js.gz"), &old_gz_bytes).unwrap();

        let initial_manifest = r#"{
            "timestamp": 1000,
            "widgets": {
                "button": {
                    "version": "1.0.0",
                    "timestamp": 1000,
                    "files": ["widget.js"]
                }
            }
        }"#;
        std::fs::write(widgets_dir.join("manifest.json"), initial_manifest).unwrap();

        // 2. Remote server offers updated plain uncompressed widget.js
        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();

        let newer_content = "// updated plain button content v2 without gzip";
        let remote_manifest = r#"{
            "timestamp": 2000,
            "widgets": {
                "button": {
                    "version": "2.0.0",
                    "timestamp": 2000,
                    "files": ["widget.js"]
                }
            }
        }"#;

        let remote_handle = std::thread::spawn(move || {
            for _ in 0..2 {
                if let Ok(req) = remote_server.recv() {
                    let url = req.url().to_string();
                    if url.contains("manifest.json") {
                        let res = Response::from_string(remote_manifest.to_string())
                            .with_status_code(StatusCode(200));
                        let _ = req.respond(res);
                    } else if url.contains("button/widget.js") {
                        let res = Response::from_string(newer_content.to_string())
                            .with_status_code(StatusCode(200));
                        let _ = req.respond(res);
                    }
                }
            }
        });

        let manifest_url = format!("http://127.0.0.1:{}/manifest.json", remote_port);
        let update_res = check_and_update_widgets(&manifest_url, &widgets_dir, 3).unwrap();
        assert_eq!(update_res.updated_count, 1);

        // Stale .gz companion should be deleted by updater
        assert!(!button_dir.join("widget.js.gz").exists());
        assert_eq!(std::fs::read_to_string(button_dir.join("widget.js")).unwrap(), newer_content);

        // 3. Confirm HTTP server serves newer plain content without Content-Encoding: gzip
        let controller = MotorController::new(true, 50);
        let router = Arc::new(Router::new_with_dir(controller, &temp_assets_dir));
        let ev3_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let ev3_port = ev3_server.server_addr().to_ip().unwrap().port();

        let router_clone = router.clone();
        let server_clone = Arc::new(ev3_server);
        let s_handle = server_clone.clone();
        let ev3_thread = std::thread::spawn(move || {
            if let Ok(req) = s_handle.recv() {
                router_clone.handle_request(req);
            }
        });

        let mut stream = TcpStream::connect(("127.0.0.1", ev3_port)).unwrap();
        stream.write_all(b"GET /widgets/button/widget.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
        let mut resp = Vec::new();
        stream.read_to_end(&mut resp).unwrap();
        let resp_str = String::from_utf8_lossy(&resp);

        assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
        assert!(!resp_str.contains("Content-Encoding: gzip"));
        assert!(resp_str.contains("// updated plain button content v2 without gzip"));

        remote_handle.join().unwrap();
        ev3_thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
    }
}
