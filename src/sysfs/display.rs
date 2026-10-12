//! # EV3 Monochrome LCD Framebuffer Console Interface
//!
//! This module renders system status information to the 178x128 monochrome LCD screen on the EV3 brick.
//! The screen is mapped by the ev3dev Linux kernel to virtual console `/dev/tty1`.
//!
//! With the default console font (`Lat15-Terminus12x6`), the physical screen fits 21 columns by 8 rows.
//! Isolating terminal escape sequences and LCD formatting in `display.rs` keeps text rendering quirks
//! separate from network server and motor control code.

use std::fs::OpenOptions;
use std::io::Write;
use std::net::UdpSocket;
use std::path::Path;

/// Controller managing text rendering to the EV3 brick LCD.
pub struct DisplayController {
    mock_mode: bool,
    tty_path: &'static str,
}

impl DisplayController {
    /// Default Linux virtual console path for the EV3 LCD.
    pub const TTY_PATH: &'static str = "/dev/tty1";
    /// Maximum character columns visible on the 178x128 screen without auto-wrapping.
    pub const MAX_COLS: usize = 21;

    /// Creates a display controller instance.
    ///
    /// Initializes console path `/dev/tty1` and simulation mode settings.
    ///
    /// Called during server initialization in `main.rs` and by the display refresh thread.
    pub fn new(mock_mode: bool) -> Self {
        Self {
            mock_mode,
            tty_path: Self::TTY_PATH,
        }
    }

    /// Detects active local network IP address using kernel routing probes.
    ///
    /// Probes the USB subnet first (`192.168.2.1:80`), then external network (`8.8.8.8:80`).
    /// Returns the active local IP string or "No network".
    ///
    /// Shared by display rendering in `main.rs` and the background widget updater.
    pub fn detect_ip() -> String {
        let ip_opt = Self::probe_ip("192.168.2.1:80")
            .or_else(|| Self::probe_ip("8.8.8.8:80"));

        ip_opt.unwrap_or_else(|| "No network".to_string())
    }

    /// Queries the Linux kernel routing table for the local outbound IP address.
    ///
    /// Connects a UDP datagram socket to `target` (e.g. `8.8.8.8:80`) and reads `local_addr().ip()`.
    ///
    /// Does not transmit any network packets over the air.
    /// Avoids spawning expensive child shell processes like `ip addr` or `ifconfig` on the 300 MHz CPU.
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

    /// Formats the multi-line ready screen text buffer for the 21x8 LCD screen.
    ///
    /// 1. Prepends VT100 terminal escape sequences to hide the cursor, clear the screen, and home the cursor.
    /// 2. Formats lines showing server status, local IP and port, battery voltage, and stop instruction.
    /// 3. Truncates each line to `MAX_COLS` (21 characters).
    /// 4. Joins lines with `\n`, omitting `\n` on the final line.
    ///
    /// Allows unit tests to verify line lengths and dimensions without filesystem access.
    /// Printing more than 21 characters per line or a newline on row 8 causes
    /// the Linux framebuffer console (`fbcon`) to scroll off the top of the LCD screen.
    pub fn format_ready_screen(ip: &str, port: u16, battery_v: f32) -> String {
        let ip_trimmed = ip.trim();
        let ip_line = if ip_trimmed.is_empty() || ip_trimmed == "No network" {
            "No network".to_string()
        } else if port == 80 {
            ip_trimmed.to_string()
        } else {
            format!("{}:{}", ip_trimmed, port)
        };

        let raw_lines = [
            "EV3 MOTOR WEB",
            "Status: ONLINE",
            &ip_line,
            &format!("Batt: {:.1} V", battery_v),
            "Stop: Spacebar/UI",
        ];

        // Terminal escape codes: \x1b[?25l (hide cursor), \x1b[2J (clear screen), \x1b[H (home)
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

    /// Writes raw terminal text buffer to `/dev/tty1`.
    ///
    /// Opens the Linux virtual console file `/dev/tty1` and writes the formatted text string.
    ///
    /// Isolates physical TTY device file I/O and handles permissions or missing file errors.
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

    /// Renders the ready splash screen with network IP, port, and battery level.
    ///
    /// Formats the ready screen text and outputs to `/dev/tty1` (or stdout in simulation mode).
    ///
    /// High-level presentation entry point called on server startup and by the periodic 5-second refresher.
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
        let body = screen.trim_start_matches("\x1b[?25l\x1b[2J\x1b[H");
        let lines: Vec<&str> = body.split('\n').collect();

        assert!(lines.len() <= 10, "Screen must fit in compact 8-row console");
        for (idx, line) in lines.iter().enumerate() {
            assert!(
                line.chars().count() <= DisplayController::MAX_COLS,
                "Line {} exceeded max columns ({}): '{}'",
                idx + 1,
                line.chars().count(),
                line
            );
        }
        assert!(!screen.ends_with('\n'), "Last row must not have a trailing newline");
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

        assert!(lines.contains(&"192.168.100.200"), "Full IP address must be present without clipping");
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

    #[test]
    fn test_format_ready_screen_non_standard_port_does_not_clip() {
        let screen = DisplayController::format_ready_screen("192.168.100.200", 8080, 7.8);
        let body = screen.trim_start_matches("\x1b[?25l\x1b[2J\x1b[H");
        let lines: Vec<&str> = body.split('\n').collect();

        assert!(lines.contains(&"192.168.100.200:8080"), "Full IP address and port must fit without clipping");
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
