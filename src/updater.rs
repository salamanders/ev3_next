//! # On-Demand Widget GitHub CRUD Updater
//!
//! This module synchronizes local widget scripts (`web_assets/widgets/*.js`) with the
//! remote GitHub repository when requested via `POST /update`.
//!
//! Over-the-air updates require network timeouts, temporary `.tmp` file staging, atomic renames,
//! and safe cleanup of removed widgets. Separating the updater into `updater.rs` keeps external
//! `curl` process execution and filesystem sync isolated from real-time motor control.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use serde::{Deserialize, Serialize};

/// Entry returned by the GitHub Contents API for a file in `web_assets/widgets`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteWidgetEntry {
    pub name: String,
    #[serde(default)]
    pub download_url: Option<String>,
}

/// Summary of widget Create, Update, and Delete operations performed during a sync.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateResult {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub updated_count: usize,
    pub errors: Vec<String>,
}

/// Validates a filename against path traversal exploits.
///
/// Ensures the filename is non-empty, does not start with `.`, and contains only ASCII
/// alphanumeric characters, underscores, dashes, or dots. Prevents a malformed remote entry
/// from writing or deleting files outside `widgets_dir`.
pub fn is_safe_component_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.contains('\0')
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Executes system `curl` with standard timeout and User-Agent flags.
///
/// When `dest_path` is `Some`, writes response bytes directly to disk via `-o`;
/// otherwise returns the captured stdout buffer. Shared by directory listing and file downloads
/// so TLS flags, timeouts, and User-Agent headers are defined once.
fn run_curl(url: &str, timeout_secs: u64, dest_path: Option<&Path>) -> Result<Vec<u8>, String> {
    let timeout_str = timeout_secs.to_string();
    let mut cmd = Command::new("curl");
    cmd.args([
        "-k",
        "-s",
        "-f",
        "-m",
        &timeout_str,
        "--connect-timeout",
        &timeout_str,
        "-H",
        "User-Agent: ev3-web-motor",
        url,
    ]);
    if let Some(dest) = dest_path {
        cmd.arg("-o").arg(dest);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed executing curl for {}: {}", url, e))?;

    if !output.status.success() {
        return Err(format!(
            "curl transfer failed for {}: code {:?}",
            url,
            output.status.code()
        ));
    }

    Ok(output.stdout)
}

/// Fetches the list of widget files from the remote GitHub Contents API.
///
/// Invokes `run_curl` and deserializes the JSON array of `RemoteWidgetEntry` items.
/// Using `/usr/bin/curl` avoids linking TLS libraries into the binary on the 64 MB EV3 brick.
pub fn fetch_remote_widget_list(
    api_url: &str,
    timeout_secs: u64,
) -> Result<Vec<RemoteWidgetEntry>, String> {
    let stdout = run_curl(api_url, timeout_secs, None)?;
    serde_json::from_slice::<Vec<RemoteWidgetEntry>>(&stdout)
        .map_err(|e| format!("Invalid widget list JSON payload: {}", e))
}

/// Downloads a remote file to a staging path using system `curl`.
///
/// Creates parent directories if needed, downloads the URL to `dest_path`, and verifies that
/// the downloaded file is non-empty. Deletes partial files if the transfer fails.
pub fn download_file(url: &str, dest_path: &Path, timeout_secs: u64) -> Result<(), String> {
    if let Some(parent) = dest_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Err(e) = run_curl(url, timeout_secs, Some(dest_path)) {
        let _ = std::fs::remove_file(dest_path);
        return Err(e);
    }

    let is_non_empty = std::fs::metadata(dest_path)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    if !is_non_empty {
        let _ = std::fs::remove_file(dest_path);
        return Err(format!("Downloaded file is empty for {}", url));
    }

    Ok(())
}

/// Synchronizes local `widgets_dir` (`*.js`) with the remote GitHub widget directory (CRUD).
///
/// 1. Fetches the remote directory listing of `.js` widget files.
/// 2. Downloads each remote `.js` file to `<name>.js.tmp`.
/// 3. If the local `<name>.js` does not exist (Create) or its bytes differ (Update), atomically
///    renames `<name>.js.tmp` to `<name>.js`; otherwise removes `<name>.js.tmp`.
/// 4. If all remote downloads succeeded and at least one `.js` widget exists remotely, deletes
///    any local `<name>.js` file that was removed from the remote repository (Delete).
pub fn check_and_update_widgets(
    api_url: &str,
    widgets_dir: &Path,
    timeout_secs: u64,
) -> Result<UpdateResult, String> {
    let remote_entries = fetch_remote_widget_list(api_url, timeout_secs)?;
    let _ = std::fs::create_dir_all(widgets_dir);

    let mut result = UpdateResult::default();
    let mut remote_js_names = HashSet::new();

    for entry in &remote_entries {
        if !entry.name.ends_with(".js") {
            continue;
        }
        if !is_safe_component_name(&entry.name) {
            result
                .errors
                .push(format!("Unsafe widget filename rejected: '{}'", entry.name));
            continue;
        }
        let Some(ref download_url) = entry.download_url else {
            continue;
        };

        remote_js_names.insert(entry.name.clone());

        let final_path = widgets_dir.join(&entry.name);
        let staged_path = widgets_dir.join(format!("{}.tmp", entry.name));

        match download_file(download_url, &staged_path, timeout_secs) {
            Ok(()) => {
                let staged_bytes = std::fs::read(&staged_path).unwrap_or_default();
                let local_bytes = std::fs::read(&final_path).ok();
                let is_new = local_bytes.is_none();

                if local_bytes.as_deref() != Some(staged_bytes.as_slice()) {
                    if let Err(e) = std::fs::rename(&staged_path, &final_path) {
                        result
                            .errors
                            .push(format!("Failed saving {}: {}", entry.name, e));
                        let _ = std::fs::remove_file(&staged_path);
                    } else {
                        if is_new {
                            result.created += 1;
                        } else {
                            result.updated += 1;
                        }
                        result.updated_count += 1;
                    }
                } else {
                    let _ = std::fs::remove_file(&staged_path);
                }
            }
            Err(e) => {
                result.errors.push(e);
                let _ = std::fs::remove_file(&staged_path);
            }
        }
    }

    // Delete local .js files that are no longer in the remote repository
    if result.errors.is_empty() && !remote_js_names.is_empty() {
        if let Ok(entries) = std::fs::read_dir(widgets_dir) {
            for entry in entries.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.ends_with(".js") && !remote_js_names.contains(&file_name) {
                    if std::fs::remove_file(entry.path()).is_ok() {
                        result.deleted += 1;
                        result.updated_count += 1;
                    }
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let res = check_and_update_widgets("http://127.0.0.1:1/widgets", &widgets_dir, 1);
        assert!(res.is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_updater_crud_creates_updates_skips_identical_and_deletes_removed() {
        use tiny_http::{Response, StatusCode};

        let test_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("ev3_test_crud_{}", test_id));
        let widgets_dir = temp_dir.join("widgets");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&widgets_dir).unwrap();

        // Seed local directory with:
        // - joystick.js (unchanged)
        // - slider.js (outdated, should be updated)
        // - obsolete.js (removed remotely, should be deleted)
        std::fs::write(widgets_dir.join("joystick.js"), "// same").unwrap();
        std::fs::write(widgets_dir.join("slider.js"), "// old").unwrap();
        std::fs::write(widgets_dir.join("obsolete.js"), "// delete me").unwrap();

        let remote_server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let remote_port = remote_server.server_addr().to_ip().unwrap().port();
        let base = format!("http://127.0.0.1:{}", remote_port);
        let listing = format!(
            r#"[
                {{"name": "joystick.js", "download_url": "{base}/joystick.js"}},
                {{"name": "slider.js", "download_url": "{base}/slider.js"}},
                {{"name": "record.js", "download_url": "{base}/record.js"}}
            ]"#
        );

        let _remote_handle = std::thread::spawn(move || {
            for _ in 0..4 {
                if let Ok(req) = remote_server.recv() {
                    let body = match req.url() {
                        "/widgets" => listing.clone(),
                        "/joystick.js" => "// same".to_string(),
                        "/slider.js" => "// updated".to_string(),
                        "/record.js" => "// brand new".to_string(),
                        _ => "".to_string(),
                    };
                    let res = Response::from_string(body).with_status_code(StatusCode(200));
                    let _ = req.respond(res);
                }
            }
        });

        let api_url = format!("http://127.0.0.1:{}/widgets", remote_port);
        let res = check_and_update_widgets(&api_url, &widgets_dir, 3).unwrap();
        assert_eq!(res.created, 1);
        assert_eq!(res.updated, 1);
        assert_eq!(res.deleted, 1);
        assert_eq!(res.updated_count, 3);
        assert!(res.errors.is_empty());

        assert_eq!(
            std::fs::read_to_string(widgets_dir.join("joystick.js")).unwrap(),
            "// same"
        );
        assert_eq!(
            std::fs::read_to_string(widgets_dir.join("slider.js")).unwrap(),
            "// updated"
        );
        assert_eq!(
            std::fs::read_to_string(widgets_dir.join("record.js")).unwrap(),
            "// brand new"
        );
        assert!(!widgets_dir.join("obsolete.js").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
