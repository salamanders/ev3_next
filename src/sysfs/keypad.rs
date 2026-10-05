use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub time_sec: u32,
    pub time_usec: u32,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

// Ensure 16-byte alignment and size on 32-bit Linux ARM kernel 4.14
const _: () = assert!(std::mem::size_of::<InputEvent>() == 16);

pub const EV_KEY: u16 = 1;

pub const KEY_BACKSPACE: u16 = 14; // EV3 Back button
pub const KEY_ENTER: u16 = 28;     // EV3 Center button
pub const KEY_UP: u16 = 103;       // EV3 Up button
pub const KEY_LEFT: u16 = 105;     // EV3 Left button
pub const KEY_RIGHT: u16 = 106;    // EV3 Right button
pub const KEY_DOWN: u16 = 108;     // EV3 Down button

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    Center,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    Press,
    Release,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeypadEvent {
    pub button: Button,
    pub action: ButtonAction,
}

pub struct KeypadReader {
    mock_mode: bool,
    mock_queue: VecDeque<KeypadEvent>,
    file: Option<File>,
}

impl KeypadReader {
    pub const DEFAULT_DEVICE_PATH: &'static str = "/dev/input/by-path/platform-gpio_keys-event";

    pub fn new(mock_mode: bool) -> Self {
        Self::new_with_path(mock_mode, Self::DEFAULT_DEVICE_PATH)
    }

    pub fn new_with_path(mock_mode: bool, device_path: &str) -> Self {
        if mock_mode || !Path::new(device_path).exists() {
            return Self {
                mock_mode: true,
                mock_queue: VecDeque::new(),
                file: None,
            };
        }

        match File::open(device_path) {
            Ok(file) => {
                #[cfg(unix)]
                {
                    use std::os::unix::io::AsRawFd;
                    extern "C" {
                        fn ioctl(fd: std::os::raw::c_int, request: std::os::raw::c_ulong, ...) -> std::os::raw::c_int;
                    }
                    // EVIOCGRAB: _IOW('E', 0x90, int) = 0x40044590
                    // Grab device so keystrokes do not echo onto tty1
                    let fd = file.as_raw_fd();
                    unsafe {
                        let _ = ioctl(fd, 0x40044590, 1);
                    }
                }

                Self {
                    mock_mode: false,
                    mock_queue: VecDeque::new(),
                    file: Some(file),
                }
            }
            Err(e) => {
                eprintln!("[WARN] Could not open keypad device {}: {}. Running in mock keypad mode.", device_path, e);
                Self {
                    mock_mode: true,
                    mock_queue: VecDeque::new(),
                    file: None,
                }
            }
        }
    }

    /// Push an event for simulation or unit testing
    #[allow(dead_code)]
    pub fn push_mock_event(&mut self, event: KeypadEvent) {
        self.mock_queue.push_back(event);
    }

    /// Read the next event (blocking in real file mode, pop in mock mode)
    pub fn read_event(&mut self) -> Option<KeypadEvent> {
        if self.mock_mode || self.file.is_none() {
            return self.mock_queue.pop_front();
        }

        if let Some(ref mut file) = self.file {
            let mut buf = [0u8; 16];
            loop {
                match file.read_exact(&mut buf) {
                    Ok(()) => {
                        if let Some(event) = parse_input_event(&buf) {
                            return Some(event);
                        }
                    }
                    Err(e) => {
                        eprintln!("[WARN] Keypad read error: {}", e);
                        return None;
                    }
                }
            }
        }

        None
    }
}

/// Parse a raw 16-byte Linux input_event packet into a KeypadEvent
pub fn parse_input_event(raw: &[u8; 16]) -> Option<KeypadEvent> {
    let type_ = u16::from_le_bytes([raw[8], raw[9]]);
    let code = u16::from_le_bytes([raw[10], raw[11]]);
    let value = i32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]);

    if type_ != EV_KEY {
        return None;
    }

    let button = match code {
        KEY_UP => Button::Up,
        KEY_DOWN => Button::Down,
        KEY_LEFT => Button::Left,
        KEY_RIGHT => Button::Right,
        KEY_ENTER => Button::Center,
        KEY_BACKSPACE => Button::Back,
        _ => return None,
    };

    let action = match value {
        0 => ButtonAction::Release,
        1 => ButtonAction::Press,
        2 => ButtonAction::Repeat,
        _ => return None,
    };

    Some(KeypadEvent { button, action })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_raw_event(type_: u16, code: u16, value: i32) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[8..10].copy_from_slice(&type_.to_le_bytes());
        buf[10..12].copy_from_slice(&code.to_le_bytes());
        buf[12..16].copy_from_slice(&value.to_le_bytes());
        buf
    }

    #[test]
    fn test_input_event_size_is_16_bytes() {
        assert_eq!(std::mem::size_of::<InputEvent>(), 16);
    }

    #[test]
    fn test_parse_input_event_buttons() {
        // UP Press
        let raw = make_raw_event(EV_KEY, KEY_UP, 1);
        assert_eq!(
            parse_input_event(&raw),
            Some(KeypadEvent {
                button: Button::Up,
                action: ButtonAction::Press
            })
        );

        // DOWN Release
        let raw = make_raw_event(EV_KEY, KEY_DOWN, 0);
        assert_eq!(
            parse_input_event(&raw),
            Some(KeypadEvent {
                button: Button::Down,
                action: ButtonAction::Release
            })
        );

        // CENTER (Enter) Repeat
        let raw = make_raw_event(EV_KEY, KEY_ENTER, 2);
        assert_eq!(
            parse_input_event(&raw),
            Some(KeypadEvent {
                button: Button::Center,
                action: ButtonAction::Repeat
            })
        );

        // BACK (Backspace) Press
        let raw = make_raw_event(EV_KEY, KEY_BACKSPACE, 1);
        assert_eq!(
            parse_input_event(&raw),
            Some(KeypadEvent {
                button: Button::Back,
                action: ButtonAction::Press
            })
        );

        // LEFT Press
        let raw = make_raw_event(EV_KEY, KEY_LEFT, 1);
        assert_eq!(
            parse_input_event(&raw),
            Some(KeypadEvent {
                button: Button::Left,
                action: ButtonAction::Press
            })
        );

        // RIGHT Press
        let raw = make_raw_event(EV_KEY, KEY_RIGHT, 1);
        assert_eq!(
            parse_input_event(&raw),
            Some(KeypadEvent {
                button: Button::Right,
                action: ButtonAction::Press
            })
        );
    }

    #[test]
    fn test_parse_input_event_ignores_non_key_and_unknown() {
        // EV_SYN (0) or EV_REL (2)
        let raw = make_raw_event(0, 0, 0);
        assert_eq!(parse_input_event(&raw), None);

        // Unknown key code 999
        let raw = make_raw_event(EV_KEY, 999, 1);
        assert_eq!(parse_input_event(&raw), None);

        // Unknown value 5
        let raw = make_raw_event(EV_KEY, KEY_UP, 5);
        assert_eq!(parse_input_event(&raw), None);
    }

    #[test]
    fn test_mock_keypad_queue() {
        let mut reader = KeypadReader::new(true);
        assert_eq!(reader.read_event(), None);

        reader.push_mock_event(KeypadEvent {
            button: Button::Center,
            action: ButtonAction::Press,
        });
        reader.push_mock_event(KeypadEvent {
            button: Button::Up,
            action: ButtonAction::Press,
        });

        assert_eq!(
            reader.read_event(),
            Some(KeypadEvent {
                button: Button::Center,
                action: ButtonAction::Press
            })
        );
        assert_eq!(
            reader.read_event(),
            Some(KeypadEvent {
                button: Button::Up,
                action: ButtonAction::Press
            })
        );
        assert_eq!(reader.read_event(), None);
    }
}
