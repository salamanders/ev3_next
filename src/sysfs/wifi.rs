//! # ConnMan Wi-Fi Provisioning Interface
//!
//! This module automates Wi-Fi configuration for the ConnMan network daemon on ev3dev Linux.
//! It discovers credentials in `wifi.txt` files on boot media and writes `/var/lib/connman/ev3_wifi.config`.
//!
//! Headless EV3 robots need network access on boot before any SSH or web session can start.
//! Isolating ConnMan configuration in `wifi.rs` keeps network bootstrapping separate from the web API.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Outcome of automatic Wi-Fi setup attempts.
#[derive(Debug, PartialEq, Eq)]
pub enum WifiProvisionResult {
    NotFound,
    Provisioned { ssid: String, path: PathBuf },
    InvalidFormat(String),
    Error(String),
}

pub type WifiCredentials = (PathBuf, String, String);
pub type WifiError = (PathBuf, String);

/// Manager for discovering Wi-Fi setup files and writing ConnMan daemon configs.
pub struct WifiManager;

impl WifiManager {
    /// Candidate filesystem paths where a user may place `wifi.txt`.
    pub const CANDIDATE_PATHS: &'static [&'static str] = &[
        "/wifi.txt",
        "/boot/wifi.txt",
        "/media/boot/wifi.txt",
        "wifi.txt",
        "/home/robot/wifi.txt",
    ];

    /// Path to the persistent ConnMan Wi-Fi configuration file on ev3dev.
    pub const CONNMAN_CONFIG_PATH: &'static str = "/var/lib/connman/ev3_wifi.config";

    /// Scans candidate storage locations for a setup file containing Wi-Fi credentials.
    ///
    /// Calls `find_wifi_credentials_in` with the standard `CANDIDATE_PATHS`.
    ///
    /// Production entry point for discovering `wifi.txt` across boot media partitions.
    pub fn find_wifi_credentials() -> Result<Option<WifiCredentials>, WifiError> {
        Self::find_wifi_credentials_in(Self::CANDIDATE_PATHS)
    }

    /// Scans a specific list of file paths for Wi-Fi credentials.
    ///
    /// Checks each path in order. If a file exists, reads and parses SSID and passphrase.
    ///
    /// Allows unit tests to provide isolated scratch paths without scanning the root filesystem.
    pub fn find_wifi_credentials_in(paths: &[&str]) -> Result<Option<WifiCredentials>, WifiError> {
        for path_str in paths {
            let p = Path::new(path_str);
            if p.exists() && p.is_file() {
                match fs::read_to_string(p) {
                    Ok(content) => match Self::parse_credentials(&content) {
                        Some((ssid, pass)) => return Ok(Some((p.to_path_buf(), ssid, pass))),
                        None => {
                            return Err((
                                p.to_path_buf(),
                                "File found but lines invalid (SSID missing or passphrase not 8-63 chars)".into(),
                            ))
                        }
                    },
                    Err(e) => return Err((p.to_path_buf(), format!("Failed reading file: {}", e))),
                }
            }
        }
        Ok(None)
    }

    /// Extracts network SSID and passphrase from two-line text content.
    ///
    /// Trims leading and trailing whitespace. First non-empty line is SSID; second is passphrase.
    /// Enforces WPA2 requirement that passphrases must be between 8 and 63 characters.
    ///
    /// Pure parsing function verified by unit tests without disk access.
    pub fn parse_credentials(content: &str) -> Option<(String, String)> {
        let mut lines = content.lines().map(|l| l.trim()).filter(|l| !l.is_empty());
        let ssid = lines.next()?.to_string();
        let pass = lines.next()?.to_string();
        if ssid.is_empty() || pass.len() < 8 || pass.len() > 63 {
            return None;
        }
        Some((ssid, pass))
    }

    /// Checks if existing network configuration matches discovered credentials.
    ///
    /// Reads `config_path` if present and compares text with `format_connman_config(ssid, pass)`.
    ///
    /// Avoids rewriting identical configuration files on every boot.
    /// Rewriting the ConnMan config triggers a Wi-Fi disconnect and full channel rescan.
    pub fn is_config_identical(config_path: &Path, ssid: &str, pass: &str) -> bool {
        if let Ok(existing) = fs::read_to_string(config_path) {
            existing == Self::format_connman_config(ssid, pass)
        } else {
            false
        }
    }

    /// Auto-provisions Wi-Fi using default candidate paths and system ConnMan config location.
    ///
    /// Calls `provision_from` using standard paths `CANDIDATE_PATHS` and `CONNMAN_CONFIG_PATH`.
    ///
    /// Production entry point called at startup in `main.rs`.
    pub fn auto_provision(mock_mode: bool) -> WifiProvisionResult {
        Self::provision_from(Self::CANDIDATE_PATHS, Path::new(Self::CONNMAN_CONFIG_PATH), mock_mode)
    }

    /// Auto-provisions Wi-Fi using custom search paths and destination configuration path.
    ///
    /// 1. Finds credentials using `find_wifi_credentials_in(candidate_paths)`.
    /// 2. In `mock_mode`, returns success immediately without writing files or calling system tools.
    /// 3. Checks if `config_path` already contains identical settings via `is_config_identical`.
    ///    If identical, skips writing to avoid dropping existing Wi-Fi connections.
    /// 4. Writes configuration file and invokes `connmanctl enable wifi` and `connmanctl scan wifi`.
    ///
    /// Allows unit tests to verify provisioning logic and duplicate checks with custom paths.
    pub fn provision_from(candidate_paths: &[&str], config_path: &Path, mock_mode: bool) -> WifiProvisionResult {
        let (path, ssid, pass) = match Self::find_wifi_credentials_in(candidate_paths) {
            Ok(Some(creds)) => creds,
            Ok(None) => return WifiProvisionResult::NotFound,
            Err((path, err)) => return WifiProvisionResult::InvalidFormat(format!("{:?}: {}", path, err)),
        };

        if mock_mode {
            println!("[WIFI] Auto-provisioned from {:?} for SSID '{}' (MOCK)", path, ssid);
            return WifiProvisionResult::Provisioned { ssid, path };
        }

        if Self::is_config_identical(config_path, &ssid, &pass) {
            println!(
                "[WIFI] ConnMan config already provisioned and matches credentials for SSID '{}'",
                ssid
            );
            return WifiProvisionResult::Provisioned { ssid, path };
        }

        match Self::write_connman_config_to(config_path, &ssid, &pass) {
            Ok(()) => {
                println!("[WIFI] Wrote {:?} for SSID '{}'", config_path, ssid);

                // Enable wifi and scan for network access points
                if let Err(e) = Command::new("connmanctl").args(["enable", "wifi"]).status() {
                    eprintln!("[WIFI WARN] Failed executing connmanctl enable wifi: {}", e);
                }
                if let Err(e) = Command::new("connmanctl").args(["scan", "wifi"]).status() {
                    eprintln!("[WIFI WARN] Failed executing connmanctl scan wifi: {}", e);
                }

                WifiProvisionResult::Provisioned { ssid, path }
            }
            Err(e) => {
                eprintln!("[WIFI ERROR] Failed writing ConnMan config: {}", e);
                WifiProvisionResult::Error(e.to_string())
            }
        }
    }

    /// Formats a standard ConnMan `.config` INI document.
    ///
    /// Returns the INI structure required by ConnMan, configuring DHCP and AutoConnect.
    ///
    /// Pure string formatter tested for syntax compliance.
    pub fn format_connman_config(ssid: &str, pass: &str) -> String {
        format!(
            "[global]\nName = EV3_WiFi\nDescription = EV3 Auto-Connect Wi-Fi\n\n[service_ev3_wifi]\nType = wifi\nName = {}\nPassphrase = {}\nIPv4 = dhcp\nAutoConnect = true\n",
            ssid, pass
        )
    }

    /// Writes ConnMan configuration to default system path `/var/lib/connman/ev3_wifi.config`.
    ///
    /// Calls `write_connman_config_to` with `CONNMAN_CONFIG_PATH`.
    ///
    /// Standard production write helper.
    pub fn write_connman_config(ssid: &str, pass: &str) -> std::io::Result<()> {
        Self::write_connman_config_to(Path::new(Self::CONNMAN_CONFIG_PATH), ssid, pass)
    }

    /// Writes ConnMan configuration to a designated file path with restricted permissions.
    ///
    /// Ensures parent directories exist, writes formatted config text, and sets file permissions to 0600.
    ///
    /// Allows writing to temporary directories during tests.
    /// Restricting permissions to 0600 prevents non-root processes from reading the Wi-Fi key.
    pub fn write_connman_config_to(config_path: &Path, ssid: &str, pass: &str) -> std::io::Result<()> {
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let content = Self::format_connman_config(ssid, pass);
        fs::write(config_path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(config_path, perms)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_credentials_valid() {
        let content = "MyHomeNetwork\nSecretPassword123\n";
        let res = WifiManager::parse_credentials(content);
        assert_eq!(
            res,
            Some(("MyHomeNetwork".to_string(), "SecretPassword123".to_string()))
        );
    }

    #[test]
    fn test_parse_credentials_windows_crlf() {
        let content = "Office_WiFi\r\nSuperSecretPass!\r\n";
        let res = WifiManager::parse_credentials(content);
        assert_eq!(
            res,
            Some(("Office_WiFi".to_string(), "SuperSecretPass!".to_string()))
        );
    }

    #[test]
    fn test_parse_credentials_invalid_pass_length() {
        // Less than 8 characters
        let content = "MySSID\nshort\n";
        assert_eq!(WifiManager::parse_credentials(content), None);

        // Empty
        let content = "\n\n";
        assert_eq!(WifiManager::parse_credentials(content), None);
    }

    #[test]
    fn test_format_connman_config() {
        let config = WifiManager::format_connman_config("TestNetwork", "Password123");
        assert!(config.contains("[global]"));
        assert!(config.contains("Name = TestNetwork"));
        assert!(config.contains("Passphrase = Password123"));
        assert!(config.contains("IPv4 = dhcp"));
        assert!(config.contains("AutoConnect = true"));
    }

    #[test]
    fn test_wifi_provision_skips_when_config_identical() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_connman_ev3_wifi.config");
        let content = WifiManager::format_connman_config("HomeNet", "SuperSecretPass");
        fs::write(&test_file, content).unwrap();

        assert!(WifiManager::is_config_identical(&test_file, "HomeNet", "SuperSecretPass"));
        assert!(!WifiManager::is_config_identical(&test_file, "HomeNet", "WrongPass123"));
        assert!(!WifiManager::is_config_identical(&test_file, "OtherNet", "SuperSecretPass"));

        let nonexistent = temp_dir.join("nonexistent_connman.config");
        assert!(!WifiManager::is_config_identical(&nonexistent, "HomeNet", "SuperSecretPass"));

        let _ = fs::remove_file(test_file);
    }
}
