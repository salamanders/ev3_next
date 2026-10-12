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

/// Metadata entry describing a single component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidgetEntry {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub timestamp: u64,
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

/// Reads embedded baseline manifest when on-disk catalog is absent.
pub fn load_embedded_manifest() -> Option<WidgetManifest> {
    crate::web::router::BASELINE_WIDGETS
        .iter()
        .find(|e| e.relative_path == "manifest.json")
        .and_then(|e| serde_json::from_slice(e.content).ok())
}

/// Fetches remote manifest index over network using system transfer tool.
pub fn fetch_remote_manifest(url: &str, timeout_secs: u64) -> Result<WidgetManifest, String> {
    let timeout_str = timeout_secs.to_string();
    let output = Command::new("curl")
        .args([
            "-k",
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

/// Downloads individual asset file directly to destination path.
pub fn download_file(url: &str, dest_path: &Path, timeout_secs: u64) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let timeout_str = timeout_secs.to_string();
    let output = Command::new("curl")
        .args([
            "-k",
            "-s",
            "-f",
            "-m",
            &timeout_str,
            "--connect-timeout",
            &timeout_str,
            url,
            "-o",
        ])
        .arg(dest_path)
        .output()
        .map_err(|e| format!("Failed executing transfer process: {}", e))?;

    if !output.status.success() {
        let _ = std::fs::remove_file(dest_path);
        return Err(format!("Download failed for {}: code {:?}", url, output.status.code()));
    }

    Ok(())
}

/// Compares revision timestamps and retrieves newer asset files.
pub fn check_and_update_widgets(
    manifest_url: &str,
    widgets_dir: &Path,
    timeout_secs: u64,
) -> Result<UpdateResult, String> {
    let remote_manifest = fetch_remote_manifest(manifest_url, timeout_secs)?;
    let local_manifest = load_local_manifest(widgets_dir).or_else(load_embedded_manifest);

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
                    || {
                        let plain = widgets_dir.join(widget_name).join("widget.js");
                        let rel = format!("{}/widget.js", widget_name);
                        let in_baseline = crate::web::router::BASELINE_WIDGETS.iter().any(|e| e.relative_path == rel);
                        !plain.is_file() && !in_baseline
                    }
            }
            None => true,
        };

        if should_update {
            let widget_target_dir = widgets_dir.join(widget_name);
            let _ = std::fs::create_dir_all(&widget_target_dir);

            let file_url = format!("{}/{}/widget.js", base_url, widget_name);
            let final_path = widget_target_dir.join("widget.js");
            let staged_path = widget_target_dir.join("widget.js.tmp");

            match download_file(&file_url, &staged_path, timeout_secs) {
                Ok(()) => {
                    if let Err(e) = std::fs::rename(&staged_path, &final_path) {
                        result.errors.push(format!("Failed promoting staged file: {}", e));
                        let _ = std::fs::remove_file(&staged_path);
                    } else {
                        result.files_downloaded += 1;
                        result.updated_count += 1;
                        modified_manifest.widgets.insert(widget_name.clone(), remote_entry.clone());
                    }
                }
                Err(e) => {
                    result.errors.push(e);
                    let _ = std::fs::remove_file(&staged_path);
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
            let start = std::time::Instant::now();
            let max_wait = std::time::Duration::from_secs(60);
            let mut connected = false;

            while start.elapsed() < max_wait {
                let ip = crate::sysfs::DisplayController::detect_ip();
                if ip != "No network" && !ip.is_empty() {
                    connected = true;
                    break;
                }
                let remaining = max_wait.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(2).min(remaining));
            }

            if !connected {
                println!("[WIDGETS] Offline: no active network. Serving local widgets.");
                return;
            }

            match check_and_update_widgets(&manifest_url, &widgets_dir, timeout_secs) {
                Ok(summary) => {
                    for err in &summary.errors {
                        eprintln!("[WIDGETS WARN] {}", err);
                    }
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
                    "timestamp": 5000
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
                    "timestamp": 4000
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
                    "timestamp": 1000
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
                    "timestamp": 2000
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
    fn test_transparent_route_serves_disk_plain_when_present_and_ram_when_not() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::sync::Arc;
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

        // 1. Create a plain uncompressed widget file on disk (widgets/slider/widget.js)
        let slider_dir = widgets_dir.join("slider");
        std::fs::create_dir_all(&slider_dir).unwrap();
        std::fs::write(slider_dir.join("widget.js"), b"// plain uncompressed slider content").unwrap();

        // 2. Start server
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

        // Test slider (should serve plain uncompressed from disk)
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

        // Test button (not on disk, should fall back to embedded RAM baseline)
        {
            let mut stream = TcpStream::connect(("127.0.0.1", ev3_port)).unwrap();
            stream.write_all(b"GET /widgets/button/widget.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = Vec::new();
            stream.read_to_end(&mut resp).unwrap();
            let resp_str = String::from_utf8_lossy(&resp);

            assert!(resp_str.starts_with("HTTP/1.1 200 OK"));
            assert!(!resp_str.contains("Content-Encoding: gzip"));
            assert!(resp_str.contains("Cache-Control: no-cache"));
        }

        ev3_thread.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_assets_dir);
    }

    #[test]
    fn test_download_failure_preserves_existing_file() {
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
                    "timestamp": 1000
                }
            }
        }"#;
        std::fs::write(widgets_dir.join("manifest.json"), initial_manifest).unwrap();

        // 2. Remote server provides manifest with newer timestamp, but fails when serving widget.js
        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();

        let remote_manifest = r#"{
            "timestamp": 2000,
            "widgets": {
                "joystick": {
                    "version": "2.0.0",
                    "timestamp": 2000
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
                    } else {
                        // Fail on widget.js
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
        assert!(!widgets_dir.join("joystick").join("widget.js.tmp").exists());

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
                    "timestamp": 2000
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
        assert_eq!(res.errors.len(), 1);
        assert!(res.errors.iter().any(|e| e.contains("Unsafe widget name rejected")));

        assert!(!temp_dir.join("escaped").exists());

        remote_handle.join().unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

}

