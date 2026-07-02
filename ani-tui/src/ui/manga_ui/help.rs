//! Shared keybinding help bar shown at the bottom of manga screens.
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Render a single-line help bar at the bottom of `area`.
/// Keys are dimmed; descriptions are normal. Keys must be compact tuples.
pub fn render_help_bar(frame: &mut Frame, area: Rect, bindings: &[(&str, &str)]) {
    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in bindings.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            format!("[{}]", key),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            *desc,
            Style::default().fg(Color::DarkGray),
        ));
    }

    let line = Line::from(spans);
    let para = Paragraph::new(line);
    frame.render_widget(para, area);
}

/// Compute a compact bottom strip rect of the given `area`.
pub fn bottom_strip(area: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area)[1]
}
