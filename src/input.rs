use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // Session management
    CreateSession,
    CreateSessionWithPicker,
    CloseSession,
    RenameSession,

    // Navigation
    NextSession,
    PrevSession,
    JumpToSession(usize),

    // View
    ToggleTiled,

    // Scrolling (lines, x, y coordinates for tiled mode)
    ScrollUp(usize, u16, u16),
    ScrollDown(usize, u16, u16),

    // Input
    SendToSession(Vec<u8>),
    ClickSidebar(u16), // session index clicked
    ClickTile(u16, u16), // x, y position in content area

    // App control
    Quit,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Normal,
    Prefix, // After Ctrl+B, waiting for command
}

pub struct InputHandler {
    mode: InputMode,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) -> Action {
        match self.mode {
            InputMode::Prefix => {
                self.mode = InputMode::Normal;
                self.handle_prefix_key(event)
            }
            InputMode::Normal => self.handle_normal_key(event),
        }
    }

    fn handle_normal_key(&mut self, event: KeyEvent) -> Action {
        // Check for Ctrl+B prefix
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('b') {
            self.mode = InputMode::Prefix;
            return Action::None;
        }

        // Ctrl+Q to quit
        if event.modifiers.contains(KeyModifiers::CONTROL) && event.code == KeyCode::Char('q') {
            return Action::Quit;
        }

        // Forward all other input to the active session
        let bytes = key_to_bytes(event);
        if !bytes.is_empty() {
            Action::SendToSession(bytes)
        } else {
            Action::None
        }
    }

    fn handle_prefix_key(&mut self, event: KeyEvent) -> Action {
        match event.code {
            KeyCode::Char('c') => {
                if event.modifiers.contains(KeyModifiers::SHIFT) {
                    Action::CreateSessionWithPicker
                } else {
                    Action::CreateSession
                }
            }
            KeyCode::Char('C') => Action::CreateSessionWithPicker,
            KeyCode::Char('n') => Action::NextSession,
            KeyCode::Char('p') => Action::PrevSession,
            KeyCode::Char('t') => Action::ToggleTiled,
            KeyCode::Char('x') => Action::CloseSession,
            KeyCode::Char('r') => Action::RenameSession,
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let num = c.to_digit(10).unwrap() as usize;
                if num > 0 {
                    Action::JumpToSession(num - 1)
                } else {
                    Action::None
                }
            }
            // If prefix key not recognized, send Ctrl+B followed by the key
            _ => {
                let mut bytes = vec![0x02]; // Ctrl+B
                bytes.extend(key_to_bytes(event));
                Action::SendToSession(bytes)
            }
        }
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, sidebar_width: u16) -> Action {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if event.column < sidebar_width {
                    // Clicked in sidebar - each session takes 2 lines, adjust for border
                    let row = event.row.saturating_sub(1); // subtract border
                    let session_index = (row / 2) as usize; // 2 lines per session
                    Action::ClickSidebar(session_index as u16)
                } else {
                    // Clicked in content area - pass absolute coordinates for tiled view handling
                    Action::ClickTile(event.column, event.row)
                }
            }
            MouseEventKind::ScrollUp => Action::ScrollUp(3, event.column, event.row),
            MouseEventKind::ScrollDown => Action::ScrollDown(3, event.column, event.row),
            _ => Action::None,
        }
    }
}

fn key_to_bytes(event: KeyEvent) -> Vec<u8> {
    let mut bytes = Vec::new();

    match event.code {
        KeyCode::Char(c) => {
            let has_ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = event.modifiers.contains(KeyModifiers::ALT);
            let has_shift = event.modifiers.contains(KeyModifiers::SHIFT);

            if has_ctrl && has_alt {
                // Ctrl+Alt+char
                bytes.push(0x1b); // ESC
                let ctrl_char = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a' - 1);
                bytes.push(ctrl_char);
            } else if has_ctrl {
                // Ctrl+char - convert to control character (works for a-z, @, [, \, ], ^, _)
                let lower = c.to_ascii_lowercase();
                if lower >= 'a' && lower <= 'z' {
                    let ctrl_char = (lower as u8) - b'a' + 1;
                    bytes.push(ctrl_char);
                } else {
                    // Handle Ctrl+special chars
                    match c {
                        '@' => bytes.push(0),    // Ctrl+@  = NUL
                        '[' => bytes.push(0x1b), // Ctrl+[  = ESC
                        '\\' => bytes.push(0x1c), // Ctrl+\  = FS
                        ']' => bytes.push(0x1d), // Ctrl+]  = GS
                        '^' => bytes.push(0x1e), // Ctrl+^  = RS
                        '_' => bytes.push(0x1f), // Ctrl+_  = US
                        '?' => bytes.push(0x7f), // Ctrl+?  = DEL
                        ' ' => bytes.push(0),    // Ctrl+Space = NUL
                        _ => {
                            // For other chars, just send as-is
                            let mut buf = [0u8; 4];
                            let s = c.encode_utf8(&mut buf);
                            bytes.extend_from_slice(s.as_bytes());
                        }
                    }
                }
            } else if has_alt && has_shift {
                // Alt+Shift+char
                bytes.push(0x1b); // ESC
                bytes.push(c as u8);
            } else if has_alt {
                // Alt+char
                bytes.push(0x1b); // ESC
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                bytes.extend_from_slice(s.as_bytes());
            } else {
                // Regular char (with or without shift - shift is handled by the char itself)
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                bytes.extend_from_slice(s.as_bytes());
            }
        }
        KeyCode::Enter => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                // Shift+Enter - Kitty keyboard protocol format for modified Enter
                bytes.extend_from_slice(b"\x1b[13;2u");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Enter
                bytes.extend_from_slice(b"\x1b[13;5u");
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                // Alt+Enter
                bytes.extend_from_slice(b"\x1b[13;3u");
            } else {
                bytes.push(b'\r');
            }
        }
        KeyCode::Backspace => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Backspace - delete word (send Ctrl+W or special sequence)
                bytes.push(0x17); // Ctrl+W
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                // Alt+Backspace - also delete word
                bytes.extend_from_slice(b"\x1b\x7f");
            } else {
                bytes.push(0x7f);
            }
        }
        KeyCode::BackTab => bytes.extend_from_slice(b"\x1b[Z"), // Shift+Tab
        KeyCode::Tab => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[Z"); // Shift+Tab
            } else {
                bytes.push(b'\t');
            }
        }
        KeyCode::Esc => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[27;2u");
            } else {
                bytes.push(0x1b);
            }
        }
        KeyCode::Up => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[1;2A");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[1;5A");
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                bytes.extend_from_slice(b"\x1b[1;3A");
            } else {
                bytes.extend_from_slice(b"\x1b[A");
            }
        }
        KeyCode::Down => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[1;2B");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[1;5B");
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                bytes.extend_from_slice(b"\x1b[1;3B");
            } else {
                bytes.extend_from_slice(b"\x1b[B");
            }
        }
        KeyCode::Right => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[1;2C");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[1;5C");
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                bytes.extend_from_slice(b"\x1b[1;3C");
            } else {
                bytes.extend_from_slice(b"\x1b[C");
            }
        }
        KeyCode::Left => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[1;2D");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[1;5D");
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                bytes.extend_from_slice(b"\x1b[1;3D");
            } else {
                bytes.extend_from_slice(b"\x1b[D");
            }
        }
        KeyCode::Home => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[1;2H");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[1;5H");
            } else {
                bytes.extend_from_slice(b"\x1b[H");
            }
        }
        KeyCode::End => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[1;2F");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[1;5F");
            } else {
                bytes.extend_from_slice(b"\x1b[F");
            }
        }
        KeyCode::PageUp => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[5;2~");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[5;5~");
            } else {
                bytes.extend_from_slice(b"\x1b[5~");
            }
        }
        KeyCode::PageDown => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[6;2~");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[6;5~");
            } else {
                bytes.extend_from_slice(b"\x1b[6~");
            }
        }
        KeyCode::Delete => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[3;2~");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[3;5~");
            } else {
                bytes.extend_from_slice(b"\x1b[3~");
            }
        }
        KeyCode::Insert => {
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                bytes.extend_from_slice(b"\x1b[2;2~");
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                bytes.extend_from_slice(b"\x1b[2;5~");
            } else {
                bytes.extend_from_slice(b"\x1b[2~");
            }
        }
        KeyCode::F(n) => {
            // Modifier codes: 2=Shift, 3=Alt, 4=Shift+Alt, 5=Ctrl, 6=Shift+Ctrl, 7=Alt+Ctrl, 8=Shift+Alt+Ctrl
            let modifier = if event.modifiers.contains(KeyModifiers::SHIFT | KeyModifiers::CONTROL) {
                Some(6)
            } else if event.modifiers.contains(KeyModifiers::CONTROL) {
                Some(5)
            } else if event.modifiers.contains(KeyModifiers::SHIFT) {
                Some(2)
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                Some(3)
            } else {
                None
            };

            let seq = match (n, modifier) {
                // F1-F4 use SS3 format, with modifiers use CSI format
                (1, None) => b"\x1bOP".to_vec(),
                (1, Some(m)) => format!("\x1b[1;{}P", m).into_bytes(),
                (2, None) => b"\x1bOQ".to_vec(),
                (2, Some(m)) => format!("\x1b[1;{}Q", m).into_bytes(),
                (3, None) => b"\x1bOR".to_vec(),
                (3, Some(m)) => format!("\x1b[1;{}R", m).into_bytes(),
                (4, None) => b"\x1bOS".to_vec(),
                (4, Some(m)) => format!("\x1b[1;{}S", m).into_bytes(),
                // F5-F12 use CSI format
                (5, None) => b"\x1b[15~".to_vec(),
                (5, Some(m)) => format!("\x1b[15;{}~", m).into_bytes(),
                (6, None) => b"\x1b[17~".to_vec(),
                (6, Some(m)) => format!("\x1b[17;{}~", m).into_bytes(),
                (7, None) => b"\x1b[18~".to_vec(),
                (7, Some(m)) => format!("\x1b[18;{}~", m).into_bytes(),
                (8, None) => b"\x1b[19~".to_vec(),
                (8, Some(m)) => format!("\x1b[19;{}~", m).into_bytes(),
                (9, None) => b"\x1b[20~".to_vec(),
                (9, Some(m)) => format!("\x1b[20;{}~", m).into_bytes(),
                (10, None) => b"\x1b[21~".to_vec(),
                (10, Some(m)) => format!("\x1b[21;{}~", m).into_bytes(),
                (11, None) => b"\x1b[23~".to_vec(),
                (11, Some(m)) => format!("\x1b[23;{}~", m).into_bytes(),
                (12, None) => b"\x1b[24~".to_vec(),
                (12, Some(m)) => format!("\x1b[24;{}~", m).into_bytes(),
                _ => vec![],
            };
            bytes.extend(seq);
        }
        _ => {}
    }

    bytes
}
