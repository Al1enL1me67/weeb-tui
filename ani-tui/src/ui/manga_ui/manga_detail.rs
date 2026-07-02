//! Manga Detail screen - shows manga info and chapter list.
use crate::manga::models::Manga;
use crate::ui::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw_manga_detail(app: &mut App, frame: &mut Frame) {
    let area = frame.size();
    let manga = app.manga_detail.clone();
    let chapters = app.manga_chapters.clone();
    let selected = app.manga_chapter_selected;

    // Reserve bottom 1 line for help bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(outer[0]);

    draw_cover(app, frame, main[0], manga.as_ref());
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main[1]);
    draw_info(frame, right[0], manga.as_ref());
    draw_chapters(frame, right[1], &chapters, selected);

    // Help bar
    super::help::render_help_bar(frame, outer[1], &[
        ("↑↓", "select chapter"),
        ("Enter", "read"),
        ("Esc", "back"),
        ("m", "anime mode"),
    ]);
}

fn draw_cover(app: &mut App, frame: &mut Frame, area: Rect, manga: Option<&Manga>) {
    let block = Block::default().borders(Borders::ALL).title("Cover");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(inner);

    if let Some(m) = manga {
        let bytes = app.manga_detail_images.get(&m.id).cloned();
        if let Some(b) = bytes {
            app.render_image_with_ratatui(frame, chunks[0], &b);
        }
        let mut lines = vec![
            Line::from(Span::styled(&m.title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::from(format!("Status: {}", m.status.as_label())),
        ];
        if !m.genres.is_empty() {
            lines.push(Line::from(format!("Genres: {}", m.genres.iter().map(|g| g.title.clone()).collect::<Vec<_>>().join(", "))));
        }
        let p = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(p, chunks[1]);
    }
}

fn draw_info(frame: &mut Frame, area: Rect, manga: Option<&Manga>) {
    let block = Block::default().borders(Borders::ALL).title("Description");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(m) = manga {
        let mut lines: Vec<Line> = Vec::new();
        for l in m.description.lines() {
            lines.push(Line::from(l));
        }
        if let Some(a) = &m.author {
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Author: {}", a.name)));
        }
        let p = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(p, inner);
    }
}

fn draw_chapters(frame: &mut Frame, area: Rect, chapters: &[crate::manga::models::Chapter], selected: usize) {
    let items: Vec<ListItem> = chapters.iter().enumerate().map(|(i, c)| {
        let vol = c.volume_number.as_deref().unwrap_or("?");
        let num = &c.chapter_number;
        let title = if c.title.is_empty() { "" } else { &c.title };
        let txt = format!("Vol.{} Ch.{} {}", vol, num, title);
        let style = if i == selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(txt).style(style)
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!("Chapters ({})", chapters.len())))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(list, area);
}
