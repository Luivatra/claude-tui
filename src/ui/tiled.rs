use crate::session::Session;
use crate::ui::terminal::render_terminal;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders},
    Frame,
};

pub fn render_tiled(frame: &mut Frame, area: Rect, sessions: &[Session], active_index: usize) {
    if sessions.is_empty() {
        return;
    }

    let session_count = sessions.len();
    let rects = calculate_grid(area, session_count);

    for (i, session) in sessions.iter().enumerate() {
        if i >= rects.len() {
            break;
        }

        let rect = rects[i];
        let is_active = i == active_index;

        // Draw border with different color for active session
        let border_style = if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let dir = session
            .directory
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "~".to_string());

        let context_str = session
            .context_percent
            .map(|p| format!(" {}%", p))
            .unwrap_or_else(|| " --%".to_string());

        let block = Block::default()
            .title(format!(
                " {} [{}] {}{} ({}) ",
                i + 1,
                session.status.icon(),
                session.display_name(),
                context_str,
                dir
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(rect);
        frame.render_widget(block, rect);

        // Render terminal content without additional border
        render_terminal(frame, inner, session, false);
    }
}

/// Find which session index is at the given coordinates
pub fn session_at_position(area: Rect, session_count: usize, x: u16, y: u16) -> Option<usize> {
    if session_count == 0 {
        return None;
    }

    let rects = calculate_grid(area, session_count);

    for (i, rect) in rects.iter().enumerate() {
        if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
            return Some(i);
        }
    }

    None
}

pub fn calculate_grid(area: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return vec![];
    }

    if count == 1 {
        return vec![area];
    }

    // Calculate grid dimensions
    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = (count + cols - 1) / cols;

    let row_constraints: Vec<Constraint> = (0..rows)
        .map(|_| Constraint::Percentage(100 / rows as u16))
        .collect();

    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    let mut rects = Vec::new();

    for (row_idx, row_area) in row_chunks.iter().enumerate() {
        let sessions_in_row = if row_idx == rows - 1 {
            count - (rows - 1) * cols
        } else {
            cols
        };

        let col_constraints: Vec<Constraint> = (0..sessions_in_row)
            .map(|_| Constraint::Percentage(100 / sessions_in_row as u16))
            .collect();

        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row_area);

        for col_area in col_chunks.iter() {
            rects.push(*col_area);
        }
    }

    rects
}
