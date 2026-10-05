use crate::sysfs::display::DisplayController;
use crate::sysfs::keypad::Button;

#[derive(Debug, Clone, PartialEq)]
pub enum UiState {
    CheckingNetwork,
    ScanningWifi,
    SelectingWifi {
        ssids: Vec<String>,
        selected_idx: usize,
    },
    EnteringPassword {
        ssid: String,
        password: String,
        cursor_row: usize,
        cursor_col: usize,
        is_uppercase: bool,
        error_msg: Option<String>,
    },
    Connecting {
        ssid: String,
        status: String,
    },
    Ready {
        ip: String,
        port: u16,
        battery_v: f32,
    },
    ShutdownConfirm,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    None,
    Render,
    ScanWifi,
    ConnectWifi { ssid: String, pass: String },
    PowerOff,
}

pub struct UiController {
    state: UiState,
    _mock_mode: bool,
}

impl UiController {
    pub const KEYBOARD_ROW0_UPPER: [char; 10] = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J'];
    pub const KEYBOARD_ROW0_LOWER: [char; 10] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'];
    pub const KEYBOARD_ROW1_UPPER: [char; 10] = ['K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T'];
    pub const KEYBOARD_ROW1_LOWER: [char; 10] = ['k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't'];
    pub const KEYBOARD_ROW2_UPPER: [char; 10] = ['U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3'];
    pub const KEYBOARD_ROW2_LOWER: [char; 10] = ['u', 'v', 'w', 'x', 'y', 'z', '0', '1', '2', '3'];
    pub const KEYBOARD_ROW3: [char; 10] = ['4', '5', '6', '7', '8', '9', '.', '-', '_', '!'];

    pub fn new(mock_mode: bool) -> Self {
        Self {
            state: UiState::CheckingNetwork,
            _mock_mode: mock_mode,
        }
    }

    pub fn state(&self) -> &UiState {
        &self.state
    }

    pub fn set_state(&mut self, state: UiState) {
        self.state = state;
    }

    /// Process a button press from the keypad
    pub fn handle_button(&mut self, button: Button) -> UiAction {
        match &mut self.state {
            UiState::CheckingNetwork | UiState::ScanningWifi | UiState::Connecting { .. } => {
                // Background operations in progress; buttons ignored
                UiAction::None
            }
            UiState::SelectingWifi {
                ssids,
                selected_idx,
            } => {
                let total_items = ssids.len() + 1; // +1 for [Rescan Networks]
                match button {
                    Button::Up => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                            UiAction::Render
                        } else {
                            UiAction::None
                        }
                    }
                    Button::Down => {
                        if *selected_idx + 1 < total_items {
                            *selected_idx += 1;
                            UiAction::Render
                        } else {
                            UiAction::None
                        }
                    }
                    Button::Center => {
                        if *selected_idx < ssids.len() {
                            let chosen_ssid = ssids[*selected_idx].clone();
                            self.state = UiState::EnteringPassword {
                                ssid: chosen_ssid,
                                password: String::new(),
                                cursor_row: 0,
                                cursor_col: 0,
                                is_uppercase: true,
                                error_msg: None,
                            };
                            UiAction::Render
                        } else {
                            // User selected [Rescan Networks]
                            self.state = UiState::ScanningWifi;
                            UiAction::ScanWifi
                        }
                    }
                    Button::Back => {
                        // Rescan on back
                        self.state = UiState::ScanningWifi;
                        UiAction::ScanWifi
                    }
                    _ => UiAction::None,
                }
            }
            UiState::EnteringPassword {
                ssid,
                password,
                cursor_row,
                cursor_col,
                is_uppercase,
                error_msg,
            } => {
                *error_msg = None;
                match button {
                    Button::Up => {
                        if *cursor_row > 0 {
                            *cursor_row -= 1;
                            // When moving up from row 4 (control row with 3 items), map col back
                            if *cursor_row == 3 && *cursor_col > 9 {
                                *cursor_col = 9;
                            }
                            UiAction::Render
                        } else {
                            UiAction::None
                        }
                    }
                    Button::Down => {
                        if *cursor_row < 4 {
                            *cursor_row += 1;
                            // When moving down to row 4, map col to 0..=2
                            if *cursor_row == 4 {
                                *cursor_col = match *cursor_col {
                                    0..=3 => 0,
                                    4..=6 => 1,
                                    _ => 2,
                                };
                            }
                            UiAction::Render
                        } else {
                            UiAction::None
                        }
                    }
                    Button::Left => {
                        if *cursor_col > 0 {
                            *cursor_col -= 1;
                            UiAction::Render
                        } else {
                            UiAction::None
                        }
                    }
                    Button::Right => {
                        let max_col = if *cursor_row == 4 { 2 } else { 9 };
                        if *cursor_col < max_col {
                            *cursor_col += 1;
                            UiAction::Render
                        } else {
                            UiAction::None
                        }
                    }
                    Button::Center => {
                        if *cursor_row == 4 {
                            match *cursor_col {
                                0 => {
                                    // [<DEL]
                                    password.pop();
                                    UiAction::Render
                                }
                                1 => {
                                    // [CASE]
                                    *is_uppercase = !*is_uppercase;
                                    UiAction::Render
                                }
                                2 => {
                                    // [DONE]
                                    if password.len() < 8 {
                                        *error_msg = Some("Min 8 chars!".to_string());
                                        UiAction::Render
                                    } else {
                                        let target_ssid = ssid.clone();
                                        let target_pass = password.clone();
                                        self.state = UiState::Connecting {
                                            ssid: target_ssid.clone(),
                                            status: "Connecting...".to_string(),
                                        };
                                        UiAction::ConnectWifi {
                                            ssid: target_ssid,
                                            pass: target_pass,
                                        }
                                    }
                                }
                                _ => UiAction::None,
                            }
                        } else {
                            // Typing character
                            let ch = match *cursor_row {
                                0 => if *is_uppercase { Self::KEYBOARD_ROW0_UPPER[*cursor_col] } else { Self::KEYBOARD_ROW0_LOWER[*cursor_col] },
                                1 => if *is_uppercase { Self::KEYBOARD_ROW1_UPPER[*cursor_col] } else { Self::KEYBOARD_ROW1_LOWER[*cursor_col] },
                                2 => if *is_uppercase { Self::KEYBOARD_ROW2_UPPER[*cursor_col] } else { Self::KEYBOARD_ROW2_LOWER[*cursor_col] },
                                3 => Self::KEYBOARD_ROW3[*cursor_col],
                                _ => ' ',
                            };
                            if password.len() < 63 {
                                password.push(ch);
                            }
                            UiAction::Render
                        }
                    }
                    Button::Back => {
                        // Return to Wi-Fi selection
                        self.state = UiState::ScanningWifi;
                        UiAction::ScanWifi
                    }
                }
            }
            UiState::Ready { .. } => {
                match button {
                    Button::Back => {
                        // User pressed Back on Ready screen -> open shutdown confirmation
                        self.state = UiState::ShutdownConfirm;
                        UiAction::Render
                    }
                    _ => UiAction::None,
                }
            }
            UiState::ShutdownConfirm => {
                match button {
                    Button::Center => UiAction::PowerOff,
                    Button::Back => {
                        // Cancel shutdown, return to Ready
                        let ip = DisplayController::detect_ip();
                        self.state = UiState::Ready {
                            ip,
                            port: 80,
                            battery_v: 8.0,
                        };
                        UiAction::Render
                    }
                    _ => UiAction::None,
                }
            }
        }
    }

    /// Format the screen output (strictly 7 rows, <= 21 chars per row)
    pub fn format_screen(&self) -> String {
        let mut lines = Vec::new();

        match &self.state {
            UiState::CheckingNetwork => {
                lines.push("=== EV3 MOTOR WEB ===".to_string());
                lines.push("Checking network...".to_string());
                lines.push("Please wait...".to_string());
                lines.push("".to_string());
                lines.push("".to_string());
                lines.push("".to_string());
                lines.push("=====================".to_string());
            }
            UiState::ScanningWifi => {
                lines.push("=== EV3 MOTOR WEB ===".to_string());
                lines.push("Scanning Wi-Fi...".to_string());
                lines.push("Please wait...".to_string());
                lines.push("".to_string());
                lines.push("".to_string());
                lines.push("".to_string());
                lines.push("=====================".to_string());
            }
            UiState::SelectingWifi {
                ssids,
                selected_idx,
            } => {
                lines.push("=== SELECT WI-FI ===".to_string());
                // Display 4 visible slots
                let total_items = ssids.len() + 1;
                let start_idx = if *selected_idx >= 3 { *selected_idx - 2 } else { 0 };

                for slot in 0..4 {
                    let item_idx = start_idx + slot;
                    if item_idx < ssids.len() {
                        let prefix = if item_idx == *selected_idx { ">" } else { " " };
                        let name: String = ssids[item_idx].chars().take(19).collect();
                        lines.push(format!("{}{} ", prefix, name));
                    } else if item_idx == ssids.len() {
                        let prefix = if item_idx == *selected_idx { ">" } else { " " };
                        lines.push(format!("{}[Rescan]", prefix));
                    } else {
                        lines.push("".to_string());
                    }
                }
                lines.push(format!("[UP/DN]Move [CTR]OK ({}/{})", selected_idx + 1, total_items));
                lines.push("=====================".to_string());
            }
            UiState::EnteringPassword {
                ssid,
                password,
                cursor_row,
                cursor_col,
                is_uppercase,
                error_msg,
            } => {
                let ssid_display: String = ssid.chars().take(15).collect();
                lines.push(format!("SSID: {}", ssid_display));

                // 4 character rows
                for r in 0..4 {
                    let chars = match r {
                        0 => if *is_uppercase { Self::KEYBOARD_ROW0_UPPER } else { Self::KEYBOARD_ROW0_LOWER },
                        1 => if *is_uppercase { Self::KEYBOARD_ROW1_UPPER } else { Self::KEYBOARD_ROW1_LOWER },
                        2 => if *is_uppercase { Self::KEYBOARD_ROW2_UPPER } else { Self::KEYBOARD_ROW2_LOWER },
                        3 => Self::KEYBOARD_ROW3,
                        _ => unreachable!(),
                    };
                    let mut row_str = String::new();
                    for (c_idx, &ch) in chars.iter().enumerate() {
                        if *cursor_row == r && *cursor_col == c_idx {
                            row_str.push(ch); // Highlighted
                        } else {
                            row_str.push(ch);
                        }
                        if c_idx < 9 {
                            row_str.push(' ');
                        }
                    }
                    lines.push(row_str);
                }

                // Row 4: Controls [<DEL] [CASE] [DONE]
                let del_tag = if *cursor_row == 4 && *cursor_col == 0 { ">DEL" } else { " DEL" };
                let case_tag = if *cursor_row == 4 && *cursor_col == 1 { ">Aa " } else { " Aa " };
                let done_tag = if *cursor_row == 4 && *cursor_col == 2 { ">DONE" } else { " DONE" };
                lines.push(format!("{} {} {}", del_tag, case_tag, done_tag));

                // Password line
                if let Some(err) = error_msg {
                    lines.push(format!("ERR: {}", err));
                } else {
                    let masked = "*".repeat(password.len());
                    let display_pass: String = masked.chars().take(14).collect();
                    lines.push(format!("P:{}_", display_pass));
                }
            }
            UiState::Connecting { ssid, status } => {
                lines.push("=== CONNECTING ===".to_string());
                let ssid_display: String = ssid.chars().take(14).collect();
                lines.push(format!("SSID: {}", ssid_display));
                lines.push(status.clone());
                lines.push("Please wait...".to_string());
                lines.push("".to_string());
                lines.push("".to_string());
                lines.push("=====================".to_string());
            }
            UiState::Ready {
                ip,
                port,
                battery_v,
            } => {
                return DisplayController::format_ready_screen(ip, *port, *battery_v);
            }
            UiState::ShutdownConfirm => {
                lines.push("=== POWER OFF ===".to_string());
                lines.push("Shut down EV3 brick?".to_string());
                lines.push("".to_string());
                lines.push("[CENTER] = Power Off".to_string());
                lines.push("[BACK]   = Cancel".to_string());
                lines.push("".to_string());
                lines.push("=====================".to_string());
            }
        }

        // Clamp lines to 21 chars and pad/join with newlines
        let mut output = String::from("\x1b[?25l\x1b[2J\x1b[H");
        for (i, line) in lines.iter().enumerate() {
            let truncated: String = line.chars().take(DisplayController::MAX_COLS).collect();
            output.push_str(&truncated);
            if i + 1 < lines.len() {
                output.push('\n');
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_checking_network() {
        let ui = UiController::new(true);
        assert_eq!(*ui.state(), UiState::CheckingNetwork);
    }

    #[test]
    fn test_select_wifi_navigation_and_choice() {
        let mut ui = UiController::new(true);
        ui.set_state(UiState::SelectingWifi {
            ssids: vec!["HomeNet".to_string(), "OfficeNet".to_string()],
            selected_idx: 0,
        });

        // Down button moves cursor
        let action = ui.handle_button(Button::Down);
        assert_eq!(action, UiAction::Render);
        if let UiState::SelectingWifi { selected_idx, .. } = ui.state() {
            assert_eq!(*selected_idx, 1);
        } else {
            panic!("Expected SelectingWifi state");
        }

        // Center selects chosen network
        let action = ui.handle_button(Button::Center);
        assert_eq!(action, UiAction::Render);
        if let UiState::EnteringPassword { ssid, .. } = ui.state() {
            assert_eq!(ssid, "OfficeNet");
        } else {
            panic!("Expected EnteringPassword state");
        }
    }

    #[test]
    fn test_password_entry_typing_and_validation() {
        let mut ui = UiController::new(true);
        ui.set_state(UiState::EnteringPassword {
            ssid: "TestNet".to_string(),
            password: String::new(),
            cursor_row: 0,
            cursor_col: 0, // 'A'
            is_uppercase: true,
            error_msg: None,
        });

        // Type 'A'
        assert_eq!(ui.handle_button(Button::Center), UiAction::Render);

        // Move right to 'B'
        assert_eq!(ui.handle_button(Button::Right), UiAction::Render);
        assert_eq!(ui.handle_button(Button::Center), UiAction::Render);

        // Move down to row 4, col 2 [DONE]
        assert_eq!(ui.handle_button(Button::Down), UiAction::Render);
        assert_eq!(ui.handle_button(Button::Down), UiAction::Render);
        assert_eq!(ui.handle_button(Button::Down), UiAction::Render);
        assert_eq!(ui.handle_button(Button::Down), UiAction::Render);

        // Navigate to [DONE] (col 2 on row 4)
        assert_eq!(ui.handle_button(Button::Right), UiAction::Render);
        assert_eq!(ui.handle_button(Button::Right), UiAction::Render);

        // Press [DONE] with only 2 chars -> Should reject with Min 8 chars error
        assert_eq!(ui.handle_button(Button::Center), UiAction::Render);
        if let UiState::EnteringPassword { error_msg, .. } = ui.state() {
            assert_eq!(*error_msg, Some("Min 8 chars!".to_string()));
        } else {
            panic!("Expected EnteringPassword with error");
        }
    }

    #[test]
    fn test_shutdown_confirm_flow() {
        let mut ui = UiController::new(true);
        ui.set_state(UiState::Ready {
            ip: "192.168.1.10".to_string(),
            port: 80,
            battery_v: 7.9,
        });

        // Press Back -> transitions to ShutdownConfirm
        assert_eq!(ui.handle_button(Button::Back), UiAction::Render);
        assert_eq!(*ui.state(), UiState::ShutdownConfirm);

        // Press Center -> confirms PowerOff
        assert_eq!(ui.handle_button(Button::Center), UiAction::PowerOff);
    }
}
