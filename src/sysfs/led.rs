//! # Linux LED Driver Interface
//!
//! This module controls the two multi-color status LEDs surrounding the screen buttons on the EV3 brick.
//! It writes brightness values to Linux sysfs files under `/sys/class/leds/led{0,1}:{red,green}:brick-status/brightness`.
//!
//! Status LEDs indicate system state (amber for starting, green for ready, red for error).
//! Isolating LED control in `led.rs` keeps hardware notification logic independent of motor and sensor logic.

use std::fs;
use std::path::Path;

/// Color options for the brick status LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedColor {
    Green,
    Red,
    Amber,
}

/// Controller managing left and right status LEDs on the EV3 brick.
pub struct LedController {
    mock_mode: bool,
}

impl LedController {
    const LEFT_RED: &'static str = "/sys/class/leds/led0:red:brick-status/brightness";
    const LEFT_GREEN: &'static str = "/sys/class/leds/led0:green:brick-status/brightness";
    const RIGHT_RED: &'static str = "/sys/class/leds/led1:red:brick-status/brightness";
    const RIGHT_GREEN: &'static str = "/sys/class/leds/led1:green:brick-status/brightness";

    /// Creates an LED controller with simulation mode support.
    ///
    /// Initializes the controller struct. When `mock_mode` is true, sysfs writes are logged to console.
    ///
    /// Called during server initialization in `main.rs` before hardware network binds.
    pub fn new(mock_mode: bool) -> Self {
        Self { mock_mode }
    }

    /// Sets colors on both left and right brick LEDs.
    ///
    /// Converts `LedColor` values into red and green brightness integers (0 or 255).
    /// Writes brightness values to the left and right sysfs brightness files.
    ///
    /// Shared helper called by `set_starting`, `set_ready`, and `set_error`.
    fn set(&self, left: LedColor, right: LedColor) {
        if self.mock_mode {
            println!("[LED] Status updated: Left={:?}, Right={:?}", left, right);
            return;
        }

        let (left_r, left_g) = Self::color_to_values(left);
        let (right_r, right_g) = Self::color_to_values(right);

        Self::write_sysfs(Self::LEFT_RED, left_r);
        Self::write_sysfs(Self::LEFT_GREEN, left_g);
        Self::write_sysfs(Self::RIGHT_RED, right_r);
        Self::write_sysfs(Self::RIGHT_GREEN, right_g);
    }

    /// Sets both LEDs to amber to signal system boot initialization.
    ///
    /// Turns on both red and green LED channels (255, 255) to mix an amber color.
    ///
    /// Called at the start of `main.rs` while network configuration and hardware discovery run.
    pub fn set_starting(&self) {
        self.set(LedColor::Amber, LedColor::Amber);
    }

    /// Sets both LEDs to green to signal that the server is ready.
    ///
    /// Turns on the green LED channels (0, 255) and turns off the red channels.
    ///
    /// Called in `main.rs` when the HTTP server binds and starts listening.
    pub fn set_ready(&self) {
        self.set(LedColor::Green, LedColor::Green);
    }

    /// Sets both LEDs to red to indicate a runtime error or bind failure.
    ///
    /// Turns on the red LED channels (255, 0) and turns off the green channels.
    ///
    /// Called in `main.rs` when server socket creation or initialization fails.
    pub fn set_error(&self) {
        self.set(LedColor::Red, LedColor::Red);
    }

    /// Converts an LED color enum into red and green brightness values (0..255).
    ///
    /// Returns `(red, green)` intensity pairs:
    /// Green is `(0, 255)`, Red is `(255, 0)`, Amber is `(255, 255)`.
    ///
    /// Pure mapping function separating color definitions from sysfs file writes.
    fn color_to_values(color: LedColor) -> (u8, u8) {
        match color {
            LedColor::Green => (0, 255),
            LedColor::Red => (255, 0),
            LedColor::Amber => (255, 255),
        }
    }

    /// Writes brightness level to a Linux sysfs file after clearing active triggers.
    ///
    /// 1. Checks if the file path exists on disk (skips safely on non-EV3 host systems).
    /// 2. If a neighbor `trigger` file exists, writes "none\n" to disable kernel blink triggers.
    /// 3. Writes the numeric brightness value (e.g. "255\n") to the brightness attribute file.
    ///
    /// Shared by all LED writes. ev3dev assigns default kernel triggers
    /// (such as heartbeat) to LEDs. Direct writes fail or get overwritten unless `trigger` is set to `none`.
    fn write_sysfs(path_str: &str, value: u8) {
        let path = Path::new(path_str);
        if !path.exists() {
            return;
        }

        // Clears kernel trigger so direct brightness writes work reliably.
        let trigger_path = path.with_file_name("trigger");
        if trigger_path.exists() {
            if let Err(e) = fs::write(&trigger_path, "none\n") {
                eprintln!("[WARN] Failed to write LED trigger to {:?}: {}", trigger_path, e);
            }
        }

        let val_str = format!("{}\n", value);
        if let Err(e) = fs::write(path, val_str) {
            eprintln!("[WARN] Failed to write LED brightness to {}: {}", path_str, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_led_color_values() {
        assert_eq!(LedController::color_to_values(LedColor::Green), (0, 255));
        assert_eq!(LedController::color_to_values(LedColor::Red), (255, 0));
        assert_eq!(LedController::color_to_values(LedColor::Amber), (255, 255));
    }

    #[test]
    fn test_mock_led_controller() {
        let leds = LedController::new(true);
        leds.set_starting();
        leds.set_ready();
        leds.set_error();
    }

    #[test]
    fn test_write_sysfs_clears_trigger_and_sets_brightness() {
        let temp_dir = std::env::temp_dir().join(format!("test_ev3_led_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        let brightness_path = temp_dir.join("brightness");
        let trigger_path = temp_dir.join("trigger");

        fs::write(&brightness_path, "0\n").unwrap();
        fs::write(&trigger_path, "heartbeat\n").unwrap();

        LedController::write_sysfs(brightness_path.to_str().unwrap(), 255);

        assert_eq!(fs::read_to_string(&trigger_path).unwrap(), "none\n");
        assert_eq!(fs::read_to_string(&brightness_path).unwrap(), "255\n");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
