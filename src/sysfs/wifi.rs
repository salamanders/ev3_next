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

    /// Connect to a Wi-Fi network using ConnMan provisioning
    pub fn connect_wifi(ssid: &str, pass: &str, mock_mode: bool) -> Result<(), String> {
        if pass.len() < 8 || pass.len() > 63 {
            return Err("Passphrase must be 8-63 characters".into());
        }
        if mock_mode {
            println!("[WIFI] Connected to '{}' (MOCK)", ssid);
            return Ok(());
        }

        Self::write_connman_config(ssid, pass).map_err(|e| format!("Failed writing config: {}", e))?;

        let _ = Command::new("connmanctl").args(["enable", "wifi"]).status();
        let _ = Command::new("connmanctl").args(["scan", "wifi"]).status();
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(())
    }

    /// Scan for available Wi-Fi networks using ConnMan
    pub fn scan_wifi_networks(mock_mode: bool) -> Vec<String> {
        if mock_mode {
            return vec![
                "Home_WiFi".to_string(),
                "RobotLab_5G".to_string(),
                "Guest_Access".to_string(),
            ];
        }

        let _ = Command::new("connmanctl").args(["enable", "wifi"]).status();
        let _ = Command::new("connmanctl").args(["scan", "wifi"]).status();

        let output = match Command::new("connmanctl").arg("services").output() {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(e) => {
                eprintln!("[WARN] Failed running connmanctl services: {}", e);
                return Vec::new();
            }
        };

        parse_connman_services(&output)
    }
}

/// Parse SSID list from connmanctl services output
pub fn parse_connman_services(output: &str) -> Vec<String> {
    let mut ssids = Vec::new();
    for line in output.lines() {
        if let Some(_wifi_idx) = line.find("wifi_") {
            // First 4 characters in connmanctl output are status flags (*, A, O, space)
            let content_slice = if line.len() > 4 { &line[4..] } else { line };
            if let Some(slice_wifi_idx) = content_slice.find("wifi_") {
                let clean_ssid = content_slice[..slice_wifi_idx].trim();
                if !clean_ssid.is_empty() && !ssids.contains(&clean_ssid.to_string()) {
                    ssids.push(clean_ssid.to_string());
                }
            }
        }
    }
    ssids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connman_services() {
        let sample = "
*AO Wired                ethernet_000000000000_cable
*A  HomeNetwork          wifi_0019e0000000_486f6d65_managed_psk
    Office_WiFi          wifi_0019e0000000_4f666669_managed_psk
    Guest_Net            wifi_0019e0000000_47756573_managed_none
";
        let ssids = parse_connman_services(sample);
        assert_eq!(ssids, vec!["HomeNetwork", "Office_WiFi", "Guest_Net"]);
    }

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
