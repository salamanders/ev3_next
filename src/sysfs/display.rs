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
    // Restricts width to 21 chars to avoid fbcon auto-wrap glitch (BUG-22).
    pub const MAX_COLS: usize = 21;

    /// Instantiates terminal renderer with configured simulation toggles.
    pub fn new(mock_mode: bool) -> Self {
        Self {
            mock_mode,
            tty_path: Self::TTY_PATH,
        }
    }

    /// Resolves primary network interface address by routing UDP probe.
    pub fn detect_ip() -> String {
        // Probe USB subnet first, then external network
        let ip_opt = Self::probe_ip("192.168.2.1:80")
            .or_else(|| Self::probe_ip("8.8.8.8:80"));

        ip_opt.unwrap_or_else(|| "No network".to_string())
    }

    // Connects dummy datagram socket to determine local routing host.
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

    /// Generates compact multi-line ASCII splash view for small LCDs.
    pub fn format_ready_screen(ip: &str, port: u16, battery_v: f32) -> String {
        let ip_line = if ip == "No network" {
            "IP: (Waiting...)".to_string()
        } else if port == 80 {
            format!("IP: {}", ip)
        } else {
            format!("IP: {}:{}", ip, port)
        };

        let raw_lines = [
            "=== EV3 MOTOR WEB ===",
            "Status: ONLINE",
            &ip_line,
            &format!("Batt: {:.1} V", battery_v),
            "Stop: Spacebar/UI",
            "=====================",
        ];

        // Join lines with newline; last line has NO trailing newline (BUG-22)
        // Hides cursor, clears screen, and homes cursor position.
        let mut output = String::from("\x1b[?25l\x1b[2J\x1b[H");
        for (i, line) in raw_lines.iter().enumerate() {
            let truncated: String = line.chars().take(Self::MAX_COLS).collect();
            output.push_str(&truncated);
            if i + 1 < raw_lines.len() {
                output.push('\n');
            }
        }

        output
    }

    /// Emits escape sequences and text buffer to physical terminal device.
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

    /// Renders power and network splash presentation to hardware console.
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

    // Confirms layout row counts and column limits prevent visual wrapping.
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

    // Asserts interface lookup yields non-empty string without connection.
    #[test]
    fn test_detect_ip_fallback() {
        let ip = DisplayController::detect_ip();
        assert!(!ip.is_empty());
    }

    // Verifies virtual presentation pipeline executes safely without hardware.
    #[test]
    fn test_mock_display_show() {
        let display = DisplayController::new(true);
        display.show_ready("127.0.0.1", 8080, 8.0);
    }

    // Ensures four-octet network addresses fit within visible column widths.
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
