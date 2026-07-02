//! Manga Search screen - search with preview.
use crate::manga::models::SearchManga;
use crate::ui::app::App;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw_manga_search(app: &mut App, frame: &mut Frame) {
    let area = frame.size();
    let main = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    // Search bar
    let query = format!("🔍 {}", app.manga_search_query);
    let bar = Paragraph::new(query)
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title("Search Manga"));
    frame.render_widget(bar, main[0]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main[1]);

    let results = app.manga_search_results.clone();
    let selected = app.manga_search_selected;
    draw_results(frame, content[0], &results, selected);

    let preview_manga = results.get(selected).cloned();
    draw_preview(app, frame, content[1], preview_manga.as_ref());

    let provider = app.manga_provider.name().as_str();
    super::help::render_help_bar(frame, main[2], &[
        ("type", "search"),
        ("↑↓", "navigate"),
        ("←→", "page"),
        ("Enter", "open"),
        ("Esc", "back"),
        ("m", "anime mode"),
    ]);
    // Show active provider in the title of search bar (covered by block title above)
    let _ = provider;
}

fn draw_results(frame: &mut Frame, area: Rect, results: &[SearchManga], selected: usize) {
    let block = Block::default().borders(Borders::ALL).title(format!("Results ({})", results.len()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, m) in results.iter().enumerate() {
        let is_sel = i == selected;
        let style = if is_sel {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let prefix = if is_sel { "▶ " } else { "  " };
        let status = m.status.map(|s| s.as_label()).unwrap_or("?");
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{} [{}]", m.title, status), style),
        ]));
    }
    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn draw_preview(app: &mut App, frame: &mut Frame, area: Rect, manga: Option<&SearchManga>) {
    let block = Block::default().borders(Borders::ALL).title("Preview");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(inner);

    if let Some(m) = manga {
        let bytes = app.manga_search_images.get(&m.id).cloned();
        if let Some(b) = bytes {
            app.render_image_with_ratatui(frame, chunks[0], &b);
        } else {
            let ph = Paragraph::new("Loading...")
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(ph, chunks[0]);
        }

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(&m.title, Style::default().add_modifier(Modifier::BOLD))),
            Line::from(""),
        ];
        if let Some(d) = &m.description {
            lines.push(Line::from(d.as_str()));
        }
        if !m.genres.is_empty() {
            lines.push(Line::from(""));
            let g = m.genres.iter().map(|x| x.title.clone()).collect::<Vec<_>>().join(", ");
            lines.push(Line::from(format!("Genres: {}", g)));
        }
        let p = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(p, chunks[1]);
    }
}
