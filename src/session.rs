use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Find the most recent Claude session UUID for a given directory
pub fn find_latest_session_uuid(directory: &std::path::Path) -> Option<String> {
    let claude_dir = dirs::home_dir()?.join(".claude/projects");

    // Encode directory path: /home/user/project -> -home-user-project
    let encoded_path = directory.to_string_lossy().replace('/', "-");

    let project_dir = claude_dir.join(&encoded_path);

    if !project_dir.exists() {
        return None;
    }

    // Find the most recently modified .jsonl file
    let mut newest: Option<(std::time::SystemTime, String)> = None;

    if let Ok(entries) = std::fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                if let Ok(metadata) = path.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let uuid = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string());

                        if let Some(uuid) = uuid {
                            if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                                newest = Some((modified, uuid));
                            }
                        }
                    }
                }
            }
        }
    }

    newest.map(|(_, uuid)| uuid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SessionStatus {
    Starting,
    Idle,
    Thinking,
    Running,
    Exited,
}

impl SessionStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            SessionStatus::Starting => "...",
            SessionStatus::Idle => ">",
            SessionStatus::Thinking => "*",
            SessionStatus::Running => "~",
            SessionStatus::Exited => "x",
        }
    }
}

pub struct Session {
    pub name: String,
    pub conversation_name: Option<String>,
    pub session_id: Option<String>,
    pub directory: PathBuf,
    pub claude_cmd: String,
    pub status: SessionStatus,
    pub context_percent: Option<u8>,
    pub needs_attention: bool,
    parser: vt100::Parser,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    reader_handle: Option<std::thread::JoinHandle<()>>,
    output_buffer: Arc<Mutex<Vec<u8>>>,
}

impl Session {
    pub fn new(
        name: String,
        directory: PathBuf,
        claude_cmd: String,
        claude_config_dir: Option<PathBuf>,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        Self::new_with_args(
            name,
            directory,
            claude_cmd,
            claude_config_dir,
            cols,
            rows,
            Vec::new(),
        )
    }

    pub fn new_with_args(
        name: String,
        directory: PathBuf,
        claude_cmd: String,
        claude_config_dir: Option<PathBuf>,
        cols: u16,
        rows: u16,
        extra_args: Vec<String>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(&claude_cmd);
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.cwd(&directory);

        // Set environment for better terminal support
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        // Set Claude config directory if specified
        if let Some(config_dir) = &claude_config_dir {
            cmd.env("CLAUDE_CONFIG_DIR", config_dir.to_string_lossy().as_ref());
        }

        let _child = pair.slave.spawn_command(cmd)?;

        let master = pair.master;
        let writer = master.take_writer()?;
        let mut reader = master.try_clone_reader()?;

        let output_buffer = Arc::new(Mutex::new(Vec::new()));
        let buffer_clone = Arc::clone(&output_buffer);

        // Spawn thread to read PTY output
        let reader_handle = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut buffer = buffer_clone.lock().unwrap();
                        buffer.extend_from_slice(&buf[..n]);
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            name,
            conversation_name: None,
            session_id: None,
            directory,
            claude_cmd,
            status: SessionStatus::Starting,
            context_percent: None,
            needs_attention: false,
            parser: vt100::Parser::new(rows, cols, 10000), // Enable 10k line scrollback
            master,
            writer,
            reader_handle: Some(reader_handle),
            output_buffer,
        })
    }

    pub fn process_output(&mut self) {
        let data: Vec<u8> = {
            let mut buffer = self.output_buffer.lock().unwrap();
            std::mem::take(&mut *buffer)
        };

        if !data.is_empty() {
            self.parser.process(&data);
            self.detect_status(&data);
            self.detect_conversation_name();
            self.detect_context_percent();
        }
    }

    fn detect_status(&mut self, data: &[u8]) {
        // Simple status detection based on output patterns
        let text = String::from_utf8_lossy(data);
        let old_status = self.status;

        // Braille spinner characters used by Claude Code
        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let has_spinner = spinner_chars.iter().any(|c| text.contains(*c));

        if text.contains("Thinking") || has_spinner {
            self.status = SessionStatus::Thinking;
        } else if text.contains("> ") || text.ends_with("> ") {
            // Only transition to Idle if we see the prompt
            self.status = SessionStatus::Idle;
        } else if self.status == SessionStatus::Starting && !text.is_empty() {
            // Don't immediately go to Idle on first output
            self.status = SessionStatus::Idle;
        }

        // Set attention flag when transitioning from working to idle
        if self.status == SessionStatus::Idle
            && (old_status == SessionStatus::Thinking || old_status == SessionStatus::Running)
        {
            self.needs_attention = true;
        }
    }

    pub fn clear_attention(&mut self) {
        self.needs_attention = false;
    }

    fn detect_conversation_name(&mut self) {
        // Try window title first (OSC escape sequences)
        let title = self.parser.screen().title();
        if !title.is_empty() {
            // Claude Code sets title like "Claude Code - conversation_name"
            let name = if let Some(pos) = title.find(" - ") {
                title[pos + 3..].to_string()
            } else {
                title.to_string()
            };
            if !name.is_empty() && name != "Claude Code" {
                self.conversation_name = Some(name);
                return;
            }
        }

        // Fallback: parse statusline format "Claude Code XX% (project-name)"
        let screen = self.parser.screen();
        let cols = screen.size().1;

        // Check first few rows for statusline
        for row in 0..3 {
            let mut line = String::new();
            for col in 0..cols {
                if let Some(cell) = screen.cell(row, col) {
                    line.push_str(&cell.contents());
                }
            }

            // Look for pattern: (name) at end of line with Claude Code
            if line.contains("Claude Code") {
                if let Some(start) = line.rfind('(') {
                    if let Some(end) = line.rfind(')') {
                        if start < end {
                            let name = line[start + 1..end].trim().to_string();
                            if !name.is_empty() {
                                self.conversation_name = Some(name);
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn display_name(&self) -> &str {
        self.conversation_name.as_deref().unwrap_or(&self.name)
    }

    fn detect_context_percent(&mut self) {
        // Scan the entire screen buffer for context and usage percentages
        let screen = self.parser.screen();
        let rows = screen.size().0;
        let cols = screen.size().1;

        // Build full screen text
        let mut full_text = String::new();
        for row in 0..rows {
            for col in 0..cols {
                if let Some(cell) = screen.cell(row, col) {
                    full_text.push_str(&cell.contents());
                }
            }
            full_text.push('\n');
        }

        // Try to extract all stats from statusline format: CTX:XX%|5H:XX%|WK:XX%
        self.extract_all_stats(&full_text);
    }

    fn extract_all_stats(&mut self, text: &str) {
        if let Some(percent) = self.extract_context_percent(text) {
            self.context_percent = Some(percent);
        }
        if let Some(sid) = self.extract_session_id(text) {
            self.session_id = Some(sid);
        }
    }

    fn extract_session_id(&self, text: &str) -> Option<String> {
        // Pattern: "SID:uuid" from our statusline script
        if let Ok(re) = regex_lite::Regex::new(
            r"SID:([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})",
        ) {
            if let Some(caps) = re.captures(text) {
                return caps.get(1).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    fn extract_context_percent(&self, text: &str) -> Option<u8> {
        // Pattern 0: "CTX:XX%" from our statusline script (highest priority)
        if let Ok(re) = regex_lite::Regex::new(r"CTX:(\d{1,3})%") {
            if let Some(caps) = re.captures(text) {
                if let Ok(num) = caps.get(1)?.as_str().parse::<u8>() {
                    if num <= 100 {
                        return Some(num);
                    }
                }
            }
        }

        let text_lower = text.to_lowercase();

        // Pattern 1: "XX% of context" or "context: XX%" or "XX% context"
        if let Ok(re) =
            regex_lite::Regex::new(r"(\d{1,3})\s*%\s*(?:of\s+)?context|context[:\s]+(\d{1,3})\s*%")
        {
            if let Some(caps) = re.captures(&text_lower) {
                let num_str = caps.get(1).or(caps.get(2))?.as_str();
                if let Ok(num) = num_str.parse::<u8>() {
                    if num <= 100 {
                        return Some(num);
                    }
                }
            }
        }

        // Pattern 2: "XXk/YYYk" or "XX.Xk / YYY.Yk" token format
        if let Ok(re) = regex_lite::Regex::new(r"(\d+(?:\.\d+)?)\s*k\s*/\s*(\d+(?:\.\d+)?)\s*k") {
            if let Some(caps) = re.captures(&text_lower) {
                let used: f64 = caps.get(1)?.as_str().parse().ok()?;
                let total: f64 = caps.get(2)?.as_str().parse().ok()?;
                if total > 0.0 {
                    let percent = ((used / total) * 100.0).min(100.0) as u8;
                    return Some(percent);
                }
            }
        }

        // Pattern 3: "X,XXX / YYY,YYY tokens" or similar
        if let Ok(re) = regex_lite::Regex::new(r"([\d,]+)\s*/\s*([\d,]+)\s*(?:tokens?)?") {
            if let Some(caps) = re.captures(&text_lower) {
                let used_str = caps.get(1)?.as_str().replace(',', "");
                let total_str = caps.get(2)?.as_str().replace(',', "");
                let used: f64 = used_str.parse().ok()?;
                let total: f64 = total_str.parse().ok()?;
                if total > 1000.0 && used <= total {
                    let percent = ((used / total) * 100.0).min(100.0) as u8;
                    return Some(percent);
                }
            }
        }

        None
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.parser.set_size(rows, cols);
        Ok(())
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn cursor_position(&self) -> (u16, u16) {
        let screen = self.parser.screen();
        (screen.cursor_position().0, screen.cursor_position().1)
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let current = self.parser.screen().scrollback();
        // vt100 scrollback() returns current position, scrollback grows as content scrolls
        // We need to track max scrollback ourselves or use a large value
        let new_pos = current + lines;
        self.parser.set_scrollback(new_pos);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        let current = self.parser.screen().scrollback();
        let new_pos = current.saturating_sub(lines);
        self.parser.set_scrollback(new_pos);
    }

    pub fn reset_scroll(&mut self) {
        self.parser.set_scrollback(0);
    }

    pub fn scrollback_position(&self) -> usize {
        self.parser.screen().scrollback()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Detach the reader thread - it will exit when the PTY is closed
        // Don't join as it may be blocked on read()
        let _ = self.reader_handle.take();
    }
}
