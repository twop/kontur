use std::path::Path;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
    Frame,
};

/// Render a one-row status bar at the very bottom of the terminal.
///
/// status(saved, unsaved) filename
pub(crate) fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    working_file: Option<&Path>,
    is_dirty: bool,
) {
    let (status_text, status_color) = match (is_dirty, &working_file) {
        (true, Some(_)) => ("  ", Color::Yellow),
        (false, Some(_)) => ("  ", Color::Green),
        (_, None) => ("", Color::Green),
    };

    let status_width = status_text.chars().count() as u16;

    frame.render_widget(
        Paragraph::new(status_text).style(Style::default().fg(status_color)),
        area,
    );

    let file_label = match working_file {
        Some(p) => {
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("[untracked file]");
            format!(" {}", name)
        }
        None => " [untracked file]".to_string(),
    };

    let status_area = Rect::new(
        area.x + status_width,
        area.y,
        area.width - status_width,
        area.height,
    );

    frame.render_widget(
        Paragraph::new(file_label.as_str()).style(Style::default().fg(Color::DarkGray)),
        status_area,
    );
}
