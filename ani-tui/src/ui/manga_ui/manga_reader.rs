//! Manga Reader screen - renders manga pages as images.
use crate::ui::app::App;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn draw_manga_reader(app: &mut App, frame: &mut Frame) {
    let area = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    let chapter = app.manga_reader_chapter.clone();
    let idx = app.manga_reader_index;
    let total = app.manga_reader_pages.len();

    // Status line (page count + title)
    let title = chapter.as_ref().map(|c| c.title.clone()).unwrap_or_default();
    let provider = app.manga_provider.name().as_str();
    let status = format!(
        "Page {}/{}  ·  {}  ·  [{}]",
        idx + 1, total, title, provider
    );
    let bar = Paragraph::new(status)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(bar, chunks[1]);

    // Help bar (bottom line of status strip)
    super::help::render_help_bar(frame, chunks[1], &[
        ("←h", "prev page"),
        ("→l/Space", "next page"),
        ("w", "next chapter"),
        ("b", "prev chapter"),
        ("Esc", "back"),
        ("q", "anime home"),
    ]);

    // Image: clone bytes out to avoid borrow conflict
    if let Some(Some(bytes)) = app.manga_reader_pages.get(idx).cloned() {
        app.render_image_with_ratatui(frame, chunks[0], &bytes);
    } else {
        let p = Paragraph::new("Loading page...")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(p, chunks[0]);
    }
}
