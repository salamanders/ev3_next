use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
pub enum WifiProvisionResult {
    NotFound,
    Provisioned { ssid: String, path: PathBuf },
    InvalidFormat(String),
    Error(String),
}

pub type WifiCredentials = (PathBuf, String, String);
pub type WifiError = (PathBuf, String);

pub struct WifiManager;

impl WifiManager {
    /// Candidate locations for wifi.txt
    pub const CANDIDATE_PATHS: &'static [&'static str] = &[
        "/wifi.txt",
        "/boot/wifi.txt",
        "/media/boot/wifi.txt",
        "wifi.txt",
        "/home/robot/wifi.txt",
    ];

    /// Find and parse wifi.txt from candidate paths
    pub fn find_wifi_credentials() -> Result<Option<WifiCredentials>, WifiError> {
        for path_str in Self::CANDIDATE_PATHS {
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

    /// Parse 2 lines: line 1 = SSID, line 2 = Password
    pub fn parse_credentials(content: &str) -> Option<(String, String)> {
        let mut lines = content.lines().map(|l| l.trim()).filter(|l| !l.is_empty());
        let ssid = lines.next()?.to_string();
        let pass = lines.next()?.to_string();
        if ssid.is_empty() || pass.len() < 8 || pass.len() > 63 {
            return None;
        }
        Some((ssid, pass))
    }

    /// Check for wifi.txt and provision if present
    pub fn auto_provision(mock_mode: bool) -> WifiProvisionResult {
        let (path, ssid, pass) = match Self::find_wifi_credentials() {
            Ok(Some(creds)) => creds,
            Ok(None) => return WifiProvisionResult::NotFound,
            Err((path, err)) => return WifiProvisionResult::InvalidFormat(format!("{:?}: {}", path, err)),
        };

        if mock_mode {
            println!("[WIFI] Auto-provisioned from {:?} for SSID '{}' (MOCK)", path, ssid);
            return WifiProvisionResult::Provisioned { ssid, path };
        }

        match Self::write_connman_config(&ssid, &pass) {
            Ok(()) => {
                println!("[WIFI] Wrote /var/lib/connman/ev3_wifi.config for SSID '{}'", ssid);

                // Enable wifi and scan
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

    pub fn write_connman_config(ssid: &str, pass: &str) -> std::io::Result<()> {
        let config_dir = Path::new("/var/lib/connman");
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }

        let config_path = config_dir.join("ev3_wifi.config");
        let content = format!(
            "[service_ev3_wifi]\nType = wifi\nName = {}\nPassphrase = {}\n",
            ssid, pass
        );

        fs::write(&config_path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&config_path, perms)?;
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
}
