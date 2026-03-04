mod app;
mod config;
mod input;
mod persistence;
mod session;
mod ui;
mod usage;

use anyhow::Result;
use app::{App, InputMode, ViewMode};
use clap::Parser;
use config::{Args, Config};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use input::{Action, InputHandler};
use persistence::PersistedState;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::time::Duration;

fn main() -> Result<()> {
    let args = Args::parse();

    // Load config
    let mut config = Config::load(args.config.as_ref())?;
    if args.claude_cmd != "claude" {
        config.default_claude_cmd = args.claude_cmd;
    }
    config.claude_config_dir = args.claude_config_dir;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(config);

    // Get initial terminal size
    let size = terminal.size()?;
    app.set_terminal_size(size.width, size.height);

    // Check for persisted sessions
    if let Ok(Some(state)) = PersistedState::load() {
        // Ask user if they want to restore
        let restore = ask_restore(&mut terminal, state.sessions.len())?;
        if restore {
            let _ = app.restore_sessions(&state);
        }
    }

    // If no sessions, create one
    if app.sessions.is_empty() {
        if let Err(e) = app.create_session(None, None) {
            // Restore terminal before showing error
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            eprintln!("Error: Unable to spawn {} because it doesn't exist on the filesystem and was not found in PATH", app.config.default_claude_cmd);
            eprintln!("Details: {}", e);
            eprintln!("\nTry running with: claude-tui --claude-cmd claude");
            return Ok(());
        }
    }

    let mut input_handler = InputHandler::new();

    // Main loop
    loop {
        // Poll for events FIRST for responsiveness
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(key) => {
                    // Handle input mode separately
                    if app.input_mode != InputMode::Normal {
                        match key.code {
                            KeyCode::Enter => {
                                if let Err(e) = app.confirm_input() {
                                    app.set_error(format!("Failed to create session: {}", e));
                                }
                            }
                            KeyCode::Esc => {
                                app.cancel_input();
                            }
                            KeyCode::Tab => {
                                app.tab_complete();
                            }
                            KeyCode::Backspace => {
                                app.input_buffer.pop();
                                app.clear_completions();
                            }
                            KeyCode::Char(c) => {
                                app.input_buffer.push(c);
                                app.clear_completions();
                            }
                            _ => {}
                        }
                    } else {
                        let action = input_handler.handle_key(key);
                        handle_action(&mut app, action)?;
                    }
                }
                Event::Mouse(mouse) => {
                    if app.input_mode == InputMode::Normal {
                        let action = input_handler.handle_mouse(mouse, app.sidebar_width);
                        handle_action(&mut app, action)?;
                    }
                }
                Event::Resize(cols, rows) => {
                    app.set_terminal_size(cols, rows);
                }
                Event::Paste(text) => {
                    // Forward paste to active session with bracketed paste sequences
                    if app.input_mode == InputMode::Normal {
                        if let Some(session) = app.active_session_mut() {
                            // Send bracketed paste start, text, and end
                            let _ = session.write(b"\x1b[200~");
                            let _ = session.write(text.as_bytes());
                            let _ = session.write(b"\x1b[201~");
                        }
                    } else {
                        // In input mode, just append to buffer
                        app.input_buffer.push_str(&text);
                        app.clear_completions();
                    }
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }

        // Process PTY output
        app.process_all_output();

        // Tick animation for attention indicator
        app.tick_animation();

        // Draw UI
        terminal.draw(|f| draw_ui(f, &app))?;

        // Small sleep to prevent busy-waiting
        std::thread::sleep(Duration::from_millis(8));
    }

    // Save state before exit
    let _ = app.save_state();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        PopKeyboardEnhancementFlags,
        DisableBracketedPaste,
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_action(app: &mut App, action: Action) -> Result<()> {
    match action {
        Action::CreateSession => {
            app.start_new_session_input();
        }
        Action::CreateSessionWithPicker => {
            app.start_new_session_input();
        }
        Action::CloseSession => {
            app.close_current_session();
            if app.sessions.is_empty() {
                app.should_quit = true;
            }
        }
        Action::RenameSession => {
            // TODO: implement rename UI
        }
        Action::NextSession => {
            app.next_session();
        }
        Action::PrevSession => {
            app.prev_session();
        }
        Action::JumpToSession(idx) => {
            app.jump_to_session(idx);
        }
        Action::ToggleTiled => {
            app.toggle_view_mode();
        }
        Action::SendToSession(data) => {
            if let Some(session) = app.active_session_mut() {
                session.reset_scroll();
                session.write(&data)?;
            }
        }
        Action::ScrollUp(lines, x, y) => {
            app.scroll_session_at(x, y, true, lines);
        }
        Action::ScrollDown(lines, x, y) => {
            app.scroll_session_at(x, y, false, lines);
        }
        Action::ClickSidebar(idx) => {
            app.jump_to_session(idx as usize);
        }
        Action::ClickTile(x, y) => {
            if app.view_mode == ViewMode::Tiled {
                // Calculate content area (right of sidebar)
                let content_area = Rect {
                    x: app.sidebar_width,
                    y: 0,
                    width: app.terminal_cols.saturating_sub(app.sidebar_width),
                    height: app.terminal_rows,
                };
                if let Some(idx) = ui::session_at_position(content_area, app.sessions.len(), x, y) {
                    app.jump_to_session(idx);
                }
            }
        }
        Action::Quit => {
            app.should_quit = true;
        }
        Action::None => {}
    }
    Ok(())
}

fn draw_ui(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Create vertical layout: main area + help bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(size);

    // Create layout with sidebar on left (in main area, above help bar)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(app.sidebar_width), Constraint::Min(1)])
        .split(main_chunks[0]);

    // Draw sidebar
    let usage = app.usage();
    ui::render_sidebar(
        frame,
        chunks[0],
        &app.sessions,
        app.active_index,
        &usage,
        app.animation_frame,
    );

    // Draw main content area
    match app.view_mode {
        ViewMode::FullScreen => {
            if let Some(session) = app.active_session() {
                ui::render_terminal(frame, chunks[1], session, true);
            } else {
                draw_empty_state(frame, chunks[1]);
            }
        }
        ViewMode::Tiled => {
            ui::render_tiled(frame, chunks[1], &app.sessions, app.active_index);
        }
    }

    // Draw help bar at bottom
    draw_help_bar(frame, main_chunks[1]);

    // Draw input dialog if in input mode
    if app.input_mode != InputMode::Normal {
        draw_input_dialog(frame, app);
    }
}

fn draw_empty_state(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" No Sessions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Gray));

    let text = Paragraph::new("Press Ctrl+B, c to create a new session")
        .style(Style::default().fg(Color::DarkGray))
        .block(block);

    frame.render_widget(text, area);
}

fn draw_help_bar(frame: &mut Frame, area: Rect) {
    let help_text = " Ctrl+B: prefix | c: new | n/p: next/prev | 1-9: jump | t: tile | x: close | Ctrl+Q: quit ";
    let help = Paragraph::new(help_text).style(Style::default().fg(Color::Black).bg(Color::White));

    frame.render_widget(help, area);
}

fn draw_input_dialog(frame: &mut Frame, app: &App) {
    let title = match app.input_mode {
        InputMode::NewSessionDirectory => " New Session - Enter Directory (Tab to complete) ",
        InputMode::Normal => return,
    };

    let height = if app.completions.len() > 1 { 7 } else { 5 };
    let area = centered_rect(70, height, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let mut lines = vec![ratatui::text::Line::from(format!("{}_", app.input_buffer))];

    // Show completion info if multiple matches
    if app.completions.len() > 1 {
        lines.push(ratatui::text::Line::from(""));
        lines.push(ratatui::text::Line::styled(
            format!(
                "({}/{}) Tab to cycle",
                app.completion_index + 1,
                app.completions.len()
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let text = Paragraph::new(lines)
        .style(Style::default().fg(Color::White))
        .block(block);

    frame.render_widget(text, area);
}

fn ask_restore(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    session_count: usize,
) -> Result<bool> {
    loop {
        terminal.draw(|f| {
            let area = centered_rect(50, 7, f.area());
            f.render_widget(Clear, area);

            let block = Block::default()
                .title(" Restore Sessions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let text = Paragraph::new(format!(
                "\nFound {} previous session(s).\n\nRestore? (y/n)",
                session_count
            ))
            .style(Style::default().fg(Color::White))
            .block(block);

            f.render_widget(text, area);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    event::KeyCode::Char('y') | event::KeyCode::Char('Y') => return Ok(true),
                    event::KeyCode::Char('n') | event::KeyCode::Char('N') => return Ok(false),
                    event::KeyCode::Esc => return Ok(false),
                    _ => {}
                }
            }
        }
    }
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width * percent_x / 100;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - height) / 2;

    Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height,
    }
}
