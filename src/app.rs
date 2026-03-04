use crate::config::Config;
use crate::persistence::{PersistedState, SessionState};
use crate::session::{find_latest_session_uuid, Session};
use crate::ui::calculate_grid;
use crate::usage::{UsageData, UsageFetcher};
use anyhow::Result;
use ratatui::layout::Rect;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    FullScreen,
    Tiled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    NewSessionDirectory,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub active_index: usize,
    pub view_mode: ViewMode,
    pub config: Config,
    pub should_quit: bool,
    pub sidebar_width: u16,
    pub terminal_cols: u16,
    pub terminal_rows: u16,
    pub error_message: Option<String>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub default_directory: PathBuf,
    pub completions: Vec<String>,
    pub completion_index: usize,
    usage_fetcher: UsageFetcher,
    pub animation_frame: u8,
}

impl App {
    pub fn new(config: Config) -> Self {
        let default_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let usage_fetcher =
            UsageFetcher::new(Duration::from_secs(60), config.claude_config_dir.clone());
        Self {
            sessions: Vec::new(),
            active_index: 0,
            view_mode: ViewMode::FullScreen,
            config,
            should_quit: false,
            sidebar_width: 30,
            terminal_cols: 80,
            terminal_rows: 24,
            error_message: None,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            default_directory,
            completions: Vec::new(),
            completion_index: 0,
            usage_fetcher,
            animation_frame: 0,
        }
    }

    pub fn usage(&self) -> UsageData {
        self.usage_fetcher.get()
    }

    pub fn start_new_session_input(&mut self) {
        self.input_mode = InputMode::NewSessionDirectory;
        self.input_buffer = self.default_directory.to_string_lossy().to_string();
    }

    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.completions.clear();
        self.completion_index = 0;
    }

    pub fn tab_complete(&mut self) {
        // If we already have completions, cycle through them
        if !self.completions.is_empty() {
            self.completion_index = (self.completion_index + 1) % self.completions.len();
            self.input_buffer = self.completions[self.completion_index].clone();
            return;
        }

        // Expand ~ to home directory for the input
        let expanded_input = if self.input_buffer.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                self.input_buffer.replacen('~', &home.to_string_lossy(), 1)
            } else {
                self.input_buffer.clone()
            }
        } else {
            self.input_buffer.clone()
        };

        let path = PathBuf::from(&expanded_input);

        // Determine directory to search and prefix to match
        let (search_dir, prefix) = if path.is_dir() && expanded_input.ends_with('/') {
            (path.clone(), String::new())
        } else {
            let parent = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let prefix = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            (parent, prefix)
        };

        if let Ok(entries) = std::fs::read_dir(&search_dir) {
            let base_path = if expanded_input.ends_with('/') {
                expanded_input.clone()
            } else {
                search_dir.to_string_lossy().to_string()
            };

            self.completions = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with(&prefix) && !name.starts_with('.') {
                        let full_path = if base_path.ends_with('/') {
                            format!("{}{}/", base_path.trim_end_matches('/'), name)
                        } else if base_path.is_empty() || base_path == "." {
                            format!("{}/", name)
                        } else {
                            format!("{}/{}/", base_path, name)
                        };
                        // Convert back to use ~ if it was originally used
                        let display_path = if self.input_buffer.starts_with('~') {
                            if let Some(home) = dirs::home_dir() {
                                full_path.replacen(&home.to_string_lossy().to_string(), "~", 1)
                            } else {
                                full_path
                            }
                        } else {
                            full_path
                        };
                        Some(display_path)
                    } else {
                        None
                    }
                })
                .collect();

            self.completions.sort();

            if !self.completions.is_empty() {
                self.completion_index = 0;
                self.input_buffer = self.completions[0].clone();
            }
        }
    }

    pub fn clear_completions(&mut self) {
        self.completions.clear();
        self.completion_index = 0;
    }

    pub fn confirm_input(&mut self) -> Result<()> {
        match &self.input_mode {
            InputMode::NewSessionDirectory => {
                let dir = if self.input_buffer.is_empty() {
                    self.default_directory.clone()
                } else {
                    PathBuf::from(&self.input_buffer)
                };
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.create_session_in_dir(None, None, dir)
            }
            InputMode::Normal => Ok(()),
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
    }

    #[allow(dead_code)]
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn set_terminal_size(&mut self, cols: u16, rows: u16) {
        self.terminal_cols = cols;
        self.terminal_rows = rows;
        self.resize_sessions_for_view_mode();
    }

    pub fn create_session(&mut self, name: Option<String>, claude_cmd: Option<&str>) -> Result<()> {
        self.create_session_in_dir(name, claude_cmd, self.default_directory.clone())
    }

    pub fn create_session_in_dir(
        &mut self,
        name: Option<String>,
        claude_cmd: Option<&str>,
        directory: PathBuf,
    ) -> Result<()> {
        let idx = self.sessions.len() + 1;
        let name = name.unwrap_or_else(|| format!("session-{}", idx));
        let cmd = self.config.get_claude_cmd(claude_cmd);

        // Create with fullscreen size initially
        let content_cols = self
            .terminal_cols
            .saturating_sub(self.sidebar_width)
            .saturating_sub(2);
        let content_rows = self.terminal_rows.saturating_sub(3); // 1 help bar + 2 border

        let session = Session::new(
            name,
            directory,
            cmd,
            self.config.claude_config_dir.clone(),
            content_cols,
            content_rows,
        )?;

        self.sessions.push(session);
        self.active_index = self.sessions.len() - 1;

        // Resize all sessions for current view mode (adding a session changes tile sizes)
        self.resize_sessions_for_view_mode();
        Ok(())
    }

    pub fn close_current_session(&mut self) {
        if self.sessions.is_empty() {
            return;
        }

        self.sessions.remove(self.active_index);

        if self.active_index >= self.sessions.len() && !self.sessions.is_empty() {
            self.active_index = self.sessions.len() - 1;
        }

        // Resize remaining sessions (tile sizes change when session count changes)
        self.resize_sessions_for_view_mode();
    }

    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_index = (self.active_index + 1) % self.sessions.len();
            self.sessions[self.active_index].clear_attention();
        }
    }

    pub fn prev_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_index = if self.active_index == 0 {
                self.sessions.len() - 1
            } else {
                self.active_index - 1
            };
            self.sessions[self.active_index].clear_attention();
        }
    }

    pub fn jump_to_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_index = index;
            // Clear attention flag when user switches to this session
            self.sessions[index].clear_attention();
        }
    }

    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    pub fn scroll_session_at(&mut self, x: u16, y: u16, up: bool, lines: usize) {
        // In tiled mode, scroll the tile under the cursor
        if self.view_mode == ViewMode::Tiled {
            let content_area = Rect {
                x: self.sidebar_width,
                y: 0,
                width: self.terminal_cols.saturating_sub(self.sidebar_width),
                height: self.terminal_rows,
            };
            if let Some(idx) =
                crate::ui::session_at_position(content_area, self.sessions.len(), x, y)
            {
                if let Some(session) = self.sessions.get_mut(idx) {
                    if up {
                        session.scroll_up(lines);
                    } else {
                        session.scroll_down(lines);
                    }
                    return;
                }
            }
        }
        // Fallback: scroll the active session
        if let Some(session) = self.active_session_mut() {
            if up {
                session.scroll_up(lines);
            } else {
                session.scroll_down(lines);
            }
        }
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::FullScreen => ViewMode::Tiled,
            ViewMode::Tiled => ViewMode::FullScreen,
        };
        self.resize_sessions_for_view_mode();
    }

    fn resize_sessions_for_view_mode(&mut self) {
        let content_area = Rect {
            x: 0,
            y: 0,
            width: self.terminal_cols.saturating_sub(self.sidebar_width),
            height: self.terminal_rows.saturating_sub(1), // minus help bar
        };

        match self.view_mode {
            ViewMode::FullScreen => {
                // Fullscreen: all sessions get same size (content area minus border)
                let cols = content_area.width.saturating_sub(2);
                let rows = content_area.height.saturating_sub(2);
                for session in &mut self.sessions {
                    let _ = session.resize(cols, rows);
                }
            }
            ViewMode::Tiled => {
                // Tiled: each session gets its tile size (minus border)
                let rects = calculate_grid(content_area, self.sessions.len());
                for (i, session) in self.sessions.iter_mut().enumerate() {
                    if let Some(rect) = rects.get(i) {
                        let cols = rect.width.saturating_sub(2);
                        let rows = rect.height.saturating_sub(2);
                        let _ = session.resize(cols, rows);
                    }
                }
            }
        }
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.get(self.active_index)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(self.active_index)
    }

    pub fn process_all_output(&mut self) {
        for session in &mut self.sessions {
            session.process_output();
        }
    }

    pub fn save_state(&self) -> Result<()> {
        let state = PersistedState {
            sessions: self
                .sessions
                .iter()
                .map(|s| SessionState {
                    name: s.name.clone(),
                    directory: s.directory.clone(),
                    claude_cmd: s.claude_cmd.clone(),
                    conversation_id: s.session_id.clone(),
                })
                .collect(),
            active_index: self.active_index,
        };
        state.save()
    }

    pub fn restore_sessions(&mut self, state: &PersistedState) -> Result<()> {
        // Calculate actual PTY size accounting for UI chrome
        let content_cols = self
            .terminal_cols
            .saturating_sub(self.sidebar_width)
            .saturating_sub(2);
        let content_rows = self.terminal_rows.saturating_sub(3); // 1 help bar + 2 border

        for session_state in &state.sessions {
            // Use saved session_id if available, otherwise find latest for this directory
            let extra_args = if let Some(ref uuid) = session_state.conversation_id {
                vec!["--resume".to_string(), uuid.clone()]
            } else if let Some(uuid) = find_latest_session_uuid(&session_state.directory) {
                vec!["--resume".to_string(), uuid]
            } else {
                vec![] // No session found, start fresh
            };

            let session = Session::new_with_args(
                session_state.name.clone(),
                session_state.directory.clone(),
                session_state.claude_cmd.clone(),
                self.config.claude_config_dir.clone(),
                content_cols,
                content_rows,
                extra_args,
            )?;
            self.sessions.push(session);
        }

        self.active_index = state
            .active_index
            .min(self.sessions.len().saturating_sub(1));

        // Resize all sessions for current view mode
        self.resize_sessions_for_view_mode();
        Ok(())
    }
}
