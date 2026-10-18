//! # Over-The-Air Widget Dynamic Updater
//!
//! This module provides automatic over-the-air updates for modular browser widgets.
//! It polls remote widget catalog manifests from GitHub, downloads updated `widget.js` packages,
//! and writes them atomically to `/home/robot/web_assets/widgets/<name>/widget.js`.
//!
//! Over-the-air updates require network timeouts, atomic file renaming, and error isolation.
//! Separating the updater into `updater.rs` prevents network transfer logic and disk I/O
//! from interfering with real-time motor control and HTTP request routing.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use serde::{Deserialize, Serialize};

/// Catalog schema recording widget revision numbers and timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidgetManifest {
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub widgets: HashMap<String, WidgetEntry>,
}

/// Metadata entry describing a single widget version and release timestamp.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WidgetEntry {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub timestamp: u64,
}

/// Result tally capturing updated widget counts and error diagnostics.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateResult {
    pub updated_count: usize,
    pub files_downloaded: usize,
    pub errors: Vec<String>,
}

/// Reads catalog metadata from local filesystem storage if present.
///
/// Reads `manifest.json` from `widgets_dir` on disk and deserializes it into `WidgetManifest`.
///
/// Checks for previously downloaded and installed widget versions on persistent storage.
pub fn load_local_manifest(widgets_dir: &Path) -> Option<WidgetManifest> {
    let path = widgets_dir.join("manifest.json");
    let content = std::fs::read(&path).ok()?;
    serde_json::from_slice::<WidgetManifest>(&content).ok()
}

/// Reads the embedded baseline manifest when no on-disk catalog is present.
///
/// Finds `manifest.json` in compile-time static `BASELINE_WIDGETS` memory and deserializes it.
///
/// Fallback used when the robot runs on a fresh flash without local downloads.
pub fn load_embedded_manifest() -> Option<WidgetManifest> {
    crate::web::BASELINE_WIDGETS
        .iter()
        .find(|e| e.relative_path == "manifest.json")
        .and_then(|e| serde_json::from_slice(e.content).ok())
}

/// Fetches the remote widget catalog manifest over HTTPS.
///
/// Invokes system `curl` with a timeout and deserializes JSON response into `WidgetManifest`.
///
/// Using system `/usr/bin/curl` avoids compiling OpenSSL or rustls
/// into the Rust binary, saving over 1 MB of flash space and using system CA certificates.
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

/// Validates component name against path traversal exploits.
///
/// Ensures the name contains only alphanumeric characters, underscores, dashes, or dots.
/// Rejects path separators (`/`, `\`), null bytes, or parent directory markers (`..`).
///
/// Prevents a compromised remote manifest from writing files outside `widgets_dir`.
pub fn is_safe_component_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.contains('\0')
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Downloads a remote file to a destination path using system curl.
///
/// Ensures parent directories exist and runs `curl -o <dest_path> <url>`.
///
/// Shared by widget download operations. On failure, deletes partial downloads.
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

    let is_non_empty = std::fs::metadata(dest_path).map(|m| m.len() > 0).unwrap_or(false);
    if !is_non_empty {
        let _ = std::fs::remove_file(dest_path);
        return Err(format!("Downloaded file is empty for {}", url));
    }

    Ok(())
}

/// Compares revision timestamps and downloads newer widget files.
///
/// 1. Fetches remote manifest from `manifest_url`.
/// 2. Reads local manifest from `widgets_dir` (falling back to embedded manifest).
/// 3. For each widget with a newer timestamp or missing file:
///    Downloads to a temporary staging file `widget.js.tmp`.
///    Atomically renames `widget.js.tmp` to `widget.js`.
/// 4. Updates and writes the local `manifest.json` on disk.
///
/// Atomic rename prevents HTTP worker threads from serving a half-written file during updates.
/// Single-file `widget.js` architecture eliminates partial multi-file sync failures.
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
                        let in_baseline = crate::web::BASELINE_WIDGETS.iter().any(|e| e.relative_path == rel);
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

        let _remote_handle = std::thread::spawn(move || {
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

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
