use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LedColor {
    Off,
    Green,
    Red,
    Amber,
}

pub struct LedController {
    mock_mode: bool,
}

impl LedController {
    const LEFT_RED: &'static str = "/sys/class/leds/led0:red:brick-status/brightness";
    const LEFT_GREEN: &'static str = "/sys/class/leds/led0:green:brick-status/brightness";
    const RIGHT_RED: &'static str = "/sys/class/leds/led1:red:brick-status/brightness";
    const RIGHT_GREEN: &'static str = "/sys/class/leds/led1:green:brick-status/brightness";

    pub fn new(mock_mode: bool) -> Self {
        Self { mock_mode }
    }

    #[allow(dead_code)]
    pub fn set(&self, left: LedColor, right: LedColor) {
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

    pub fn set_starting(&self) {
        self.set(LedColor::Amber, LedColor::Amber);
    }

    pub fn set_ready(&self) {
        self.set(LedColor::Green, LedColor::Green);
    }

    pub fn set_error(&self) {
        self.set(LedColor::Red, LedColor::Red);
    }

    #[allow(dead_code)]
    pub fn set_off(&self) {
        self.set(LedColor::Off, LedColor::Off);
    }

    fn color_to_values(color: LedColor) -> (u8, u8) {
        match color {
            LedColor::Off => (0, 0),
            LedColor::Green => (0, 255),
            LedColor::Red => (255, 0),
            LedColor::Amber => (255, 255),
        }
    }

    fn write_sysfs(path_str: &str, value: u8) {
        let path = Path::new(path_str);
        if !path.exists() {
            // Path does not exist on non-EV3 hardware or simulation host
            return;
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
        assert_eq!(LedController::color_to_values(LedColor::Off), (0, 0));
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
        leds.set_off();
    }
}
