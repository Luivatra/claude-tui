use crate::session::Session;
use crate::usage::UsageData;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState},
    Frame,
};

pub fn render_sidebar(
    frame: &mut Frame,
    area: Rect,
    sessions: &[Session],
    active_index: usize,
    usage: &UsageData,
    animation_frame: u8,
) {
    // Split area: session list at top, usage bars at bottom
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    render_session_list(frame, chunks[0], sessions, active_index, animation_frame);
    render_usage_bars(frame, chunks[1], usage);
}

fn render_session_list(
    frame: &mut Frame,
    area: Rect,
    sessions: &[Session],
    active_index: usize,
    animation_frame: u8,
) {
    // Animation: blink every ~16 frames (8ms * 16 = ~128ms per blink)
    let blink_on = (animation_frame / 16).is_multiple_of(2);

    let items: Vec<ListItem> = sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let status_icon = session.status.icon();
            let name = session.display_name();
            let dir = session
                .directory
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "~".to_string());

            let style = if i == active_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            // Attention indicator: show blinking marker when session needs attention
            let (attention_icon, attention_style) = if session.needs_attention && i != active_index
            {
                if blink_on {
                    (
                        " !",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    (" !", Style::default().fg(Color::Black))
                }
            } else {
                ("", Style::default())
            };

            let status_style = match session.status {
                crate::session::SessionStatus::Thinking => Style::default().fg(Color::Cyan),
                crate::session::SessionStatus::Running => Style::default().fg(Color::Green),
                crate::session::SessionStatus::Exited => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::Gray),
            };

            let (context_str, context_style) = match session.context_percent {
                Some(p) if p >= 90 => (format!(" {}%", p), Style::default().fg(Color::Red)),
                Some(p) if p >= 70 => (format!(" {}%", p), Style::default().fg(Color::Yellow)),
                Some(p) => (format!(" {}%", p), Style::default().fg(Color::Green)),
                None => (" --%".to_string(), Style::default().fg(Color::DarkGray)),
            };

            let line1 = Line::from(vec![
                Span::styled(format!("{} ", status_icon), status_style),
                Span::styled(format!("{}: ", i + 1), Style::default().fg(Color::DarkGray)),
                Span::styled(name.to_string(), style),
                Span::styled(context_str, context_style),
                Span::styled(attention_icon.to_string(), attention_style),
            ]);
            let line2 = Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(dir, Style::default().fg(Color::DarkGray)),
            ]);

            ListItem::new(vec![line1, line2])
        })
        .collect();

    // Clear the area first
    frame.render_widget(Clear, area);

    let mut state = ListState::default();
    state.select(Some(active_index));

    let list = List::new(items)
        .style(Style::default().bg(Color::Black))
        .block(
            Block::default()
                .title(" Sessions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(60, 60, 80))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_usage_bars(frame: &mut Frame, area: Rect, usage: &UsageData) {
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // 5-hour usage bar
    let h5_percent = usage.five_hour_percent.unwrap_or(0) as u16;
    let h5_color = match h5_percent {
        p if p >= 80 => Color::Red,
        p if p >= 50 => Color::Yellow,
        _ => Color::Green,
    };
    let h5_reset = usage.format_reset(true);
    let h5_label = match usage.five_hour_percent {
        Some(p) => format!("5H: {}% ({})", p, h5_reset),
        None => "5H: --%".to_string(),
    };
    let h5_gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(h5_color).bg(Color::DarkGray))
        .percent(h5_percent)
        .label(Span::styled(h5_label, Style::default().fg(Color::White)));
    frame.render_widget(h5_gauge, chunks[0]);

    // Weekly usage bar
    let wk_percent = usage.seven_day_percent.unwrap_or(0) as u16;
    let wk_color = match wk_percent {
        p if p >= 80 => Color::Red,
        p if p >= 50 => Color::Yellow,
        _ => Color::Green,
    };
    let wk_reset = usage.format_reset(false);
    let wk_label = match usage.seven_day_percent {
        Some(p) => format!("WK: {}% ({})", p, wk_reset),
        None => "WK: --%".to_string(),
    };
    let wk_gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(wk_color).bg(Color::DarkGray))
        .percent(wk_percent)
        .label(Span::styled(wk_label, Style::default().fg(Color::White)));
    frame.render_widget(wk_gauge, chunks[1]);
}
