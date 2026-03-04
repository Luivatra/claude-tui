use crate::session::Session;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn render_terminal(frame: &mut Frame, area: Rect, session: &Session, show_border: bool) {
    let screen = session.screen();
    let (cursor_row, cursor_col) = session.cursor_position();
    let is_scrolled = session.scrollback_position() > 0;

    // Calculate content area (inside border if shown)
    let content_area = if show_border {
        Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        }
    } else {
        area
    };

    let mut lines: Vec<Line> = Vec::new();

    // vt100's cell() automatically respects scrollback position set via set_scrollback()
    for row in 0..content_area.height {
        let mut spans: Vec<Span> = Vec::new();

        for col in 0..content_area.width {
            let cell = screen.cell(row, col);

            if let Some(cell) = cell {
                let contents = cell.contents();
                let char_to_render = if contents.is_empty() {
                    " ".to_string()
                } else {
                    contents.to_string()
                };

                let mut style = Style::default();
                style = style.fg(convert_color(cell.fgcolor()));
                style = style.bg(convert_color(cell.bgcolor()));

                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                // Highlight cursor position (only when not scrolled back)
                if !is_scrolled && row == cursor_row && col == cursor_col {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                spans.push(Span::styled(char_to_render, style));
            } else {
                spans.push(Span::raw(" "));
            }
        }

        lines.push(Line::from(spans));
    }

    // Clear the area first
    frame.render_widget(Clear, area);

    let paragraph = Paragraph::new(lines).style(Style::default().bg(Color::Black));

    if show_border {
        let block = Block::default()
            .title(format!(" {} ", session.display_name()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Gray));
        frame.render_widget(paragraph.block(block), area);
    } else {
        frame.render_widget(paragraph, area);
    }
}

fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
