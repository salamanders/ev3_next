use std::fs::OpenOptions;
use std::io::Write;
use std::net::UdpSocket;
use std::path::Path;

pub struct DisplayController {
    mock_mode: bool,
    tty_path: &'static str,
}

impl DisplayController {
    pub const TTY_PATH: &'static str = "/dev/tty1";
    pub const MAX_COLS: usize = 21; // Restrict to 21 chars to avoid fbcon auto-wrap glitch (BUG-22)

    pub fn new(mock_mode: bool) -> Self {
        Self {
            mock_mode,
            tty_path: Self::TTY_PATH,
        }
    }

    /// Detect active IP address on the primary network interface
    pub fn detect_ip() -> String {
        // Probe USB subnet first, then external network
        let ip_opt = Self::probe_ip("192.168.2.1:80")
            .or_else(|| Self::probe_ip("8.8.8.8:80"));

        ip_opt.unwrap_or_else(|| "No network".to_string())
    }

    fn probe_ip(target: &str) -> Option<String> {
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect(target).ok()?;
        let addr = socket.local_addr().ok()?;
        let ip = addr.ip().to_string();
        if ip == "0.0.0.0" {
            None
        } else {
            Some(ip)
        }
    }

    /// Render compact text layout that fits within 10-row or 16-row consoles without scrolling (BUG-32)
    pub fn format_ready_screen(ip: &str, port: u16, battery_v: f32) -> String {
        let mut lines = Vec::new();

        lines.push("=== EV3 MOTOR WEB ===".to_string());
        lines.push(Self::truncate_pad("Status: ONLINE"));
        if ip == "No network" {
            lines.push(Self::truncate_pad("IP: (Waiting...)"));
        } else if port == 80 {
            lines.push(Self::truncate_pad(&format!("IP: {}", ip)));
        } else {
            lines.push(Self::truncate_pad(&format!("IP: {}:{}", ip, port)));
        }
        lines.push(Self::truncate_pad(&format!("Batt: {:.1} V", battery_v)));
        lines.push(Self::truncate_pad("Stop: Spacebar/UI"));
        lines.push("=====================".to_string());

        // Join lines with newline; last line has NO trailing newline (BUG-22)
        let mut output = String::from("\x1b[?25l\x1b[2J\x1b[H"); // Hide cursor + Clear screen + Home cursor
        for (i, line) in lines.iter().enumerate() {
            let truncated = if line.chars().count() > Self::MAX_COLS {
                line.chars().take(Self::MAX_COLS).collect::<String>()
            } else {
                line.clone()
            };

            output.push_str(&truncated);
            if i + 1 < lines.len() {
                output.push('\n');
            }
        }

        output
    }

    fn truncate_pad(s: &str) -> String {
        if s.chars().count() > Self::MAX_COLS {
            s.chars().take(Self::MAX_COLS).collect()
        } else {
            s.to_string()
        }
    }

    /// Write arbitrary formatted screen string to /dev/tty1 or stdout in mock
    pub fn show_screen(&self, content: &str) {
        if self.mock_mode || !Path::new(self.tty_path).exists() {
            return;
        }

        match OpenOptions::new().write(true).open(self.tty_path) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    eprintln!("[WARN] Failed writing to LCD console {}: {}", self.tty_path, e);
                }
            }
            Err(e) => {
                eprintln!("[WARN] Could not open LCD console {}: {}", self.tty_path, e);
            }
        }
    }

    /// Display the ready screen on EV3 LCD console (/dev/tty1) or simulation log
    pub fn show_ready(&self, ip: &str, port: u16, battery_v: f32) {
        let content = Self::format_ready_screen(ip, port, battery_v);

        if self.mock_mode || !Path::new(self.tty_path).exists() {
            println!("[DISPLAY] LCD Ready Screen rendered (IP: {}, Port: {}, Battery: {:.1}V)", ip, port, battery_v);
            return;
        }

        self.show_screen(&content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_ready_screen_dimensions() {
        let screen = DisplayController::format_ready_screen("192.168.2.2", 80, 7.8);
        
        // Strip escape sequence header
        let body = screen.trim_start_matches("\x1b[?25l\x1b[2J\x1b[H");
        let lines: Vec<&str> = body.split('\n').collect();

        assert!(lines.len() <= 10, "Screen must fit in compact 10-row console");
        for (idx, line) in lines.iter().enumerate() {
            assert!(
                line.chars().count() <= DisplayController::MAX_COLS,
                "Line {} exceeded max columns ({}): '{}'",
                idx + 1,
                line.chars().count(),
                line
            );
        }
        assert!(!screen.ends_with('\n'), "Last row must not have a trailing newline (BUG-22)");
    }

    #[test]
    fn test_detect_ip_fallback() {
        let ip = DisplayController::detect_ip();
        assert!(!ip.is_empty());
    }

    #[test]
    fn test_mock_display_show() {
        let display = DisplayController::new(true);
        display.show_ready("127.0.0.1", 8080, 8.0);
    }

    #[test]
    fn test_format_ready_screen_long_ip_does_not_clip() {
        let screen = DisplayController::format_ready_screen("192.168.100.200", 80, 7.8);
        let body = screen.trim_start_matches("\x1b[?25l\x1b[2J\x1b[H");
        let lines: Vec<&str> = body.split('\n').collect();

        assert!(lines.contains(&"IP: 192.168.100.200"), "Full IP address must be present without clipping");
        for (idx, line) in lines.iter().enumerate() {
            assert!(
                line.chars().count() <= DisplayController::MAX_COLS,
                "Line {} exceeded max columns ({}): '{}'",
                idx + 1,
                line.chars().count(),
                line
            );
        }
    }
}
