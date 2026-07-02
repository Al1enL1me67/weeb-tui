//! Manga Home screen - Tab-toggle between Popular and Last Read panels.
use crate::ui::app::{App, MangaHomePanel};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw_manga_home(app: &mut App, frame: &mut Frame) {
    let area = frame.size();
    // Reserve bottom 1 line for help bar
    let main = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    match app.manga_home_active_panel {
        MangaHomePanel::Popular => draw_popular(app, frame, main[0]),
        MangaHomePanel::LastRead => draw_last_read(app, frame, main[0]),
    }

    // Help bar - show Tab hint to toggle
    let other = match app.manga_home_active_panel {
        MangaHomePanel::Popular => "Last Read",
        MangaHomePanel::LastRead => "Popular",
    };
    super::help::render_help_bar(frame, main[1], &[
        ("Tab", other),
        ("↑↓", "navigate"),
        ("Enter", if app.manga_home_active_panel == MangaHomePanel::LastRead { "resume" } else { "open" }),
        ("s", "search"),
        ("m", "anime mode"),
        ("q/Esc", "quit"),
    ]);
}

fn draw_popular(app: &mut App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // Left: list
    let list_block = Block::default().borders(Borders::ALL).title(" Popular Manga ");
    let inner = list_block.inner(chunks[0]);
    frame.render_widget(list_block, chunks[0]);

    let mut lines: Vec<Line> = Vec::new();
    for (i, m) in app.manga_popular.iter().enumerate() {
        let is_sel = i == app.manga_popular_selected;
        let style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let prefix = if is_sel { "▶ " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(&m.title, style),
        ]));
    }
    let list = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(list, inner);

    // Right: cover + details for selected
    if let Some(m) = app.manga_popular.get(app.manga_popular_selected).cloned() {
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(chunks[1]);

        let bytes = app.manga_popular_images.get(&m.id).cloned();
        if let Some(b) = bytes {
            app.render_image_with_ratatui(frame, right[0], &b);
        } else {
            let ph = Paragraph::new("Loading...")
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Cover"));
            frame.render_widget(ph, right[0]);
        }

        let details = Paragraph::new(m.description.as_str())
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: true });
        frame.render_widget(details, right[1]);
    }
}

fn draw_last_read(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Last Read ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.manga_last_read.is_empty() {
        let empty = Paragraph::new("No reading history yet.\nRead some manga and your progress will appear here.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, h) in app.manga_last_read.iter().enumerate() {
        let is_sel = i == app.manga_last_read_selected;
        let style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let prefix = if is_sel { "▶ " } else { "  " };

        // Title column (padded to fixed width)
        let title_display = truncate_str(&h.manga_title, 30);
        let progress = format!(
            "Ch.{:.0} · Page {}/{}",
            h.chapter_number,
            h.last_page_read + 1,
            h.total_pages,
        );

        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(title_display.to_string(), style),
            Span::raw("  "),
            Span::styled(progress, Style::default().fg(Color::Cyan)),
        ]));
    }
    let list = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(list, inner);
}

fn truncate_str(s: &str, max: usize) -> &str {
    if s.chars().count() > max {
        let end = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
        &s[..end]
    } else {
        s
    }
}
