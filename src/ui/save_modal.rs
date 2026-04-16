use std::path::Path;

use ratatui::{
    layout::{Offset, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

/// Render the save-file modal overlay centred on `canvas_area`.
///
/// Layout (inside the rounded border):
/// ```text
/// ┌─ save ────────────────────────────────────────────┐
/// │  [textarea input                       ] .ktr     │
/// │  path: /resolved/full/path/filename.ktr           │
/// │                                                   │
/// └───────────────────────────────────────────────────┘
/// ```
///
/// `cwd` is displayed on the second row when the input is empty (instead of a
/// resolved-path preview).  It must be supplied by the caller so this function
/// performs no I/O.
pub(crate) fn render_save_modal(
    frame: &mut Frame,
    canvas_area: Rect,
    textarea: &ratatui_textarea::TextArea<'_>,
    _working_file: Option<&Path>,
    cwd: &Path,
) {
    // Modal dimensions.
    let modal_w = (canvas_area.width * 2 / 3).max(52).min(canvas_area.width);
    // 2 border + 1 input row + 1 path row + 1 blank + 1 hint row + 2 padding = 8
    let modal_h: u16 = 4;

    // Centre in canvas_area.
    let modal_x = canvas_area.x + canvas_area.width.saturating_sub(modal_w) / 2;
    let modal_y = canvas_area.y + canvas_area.height.saturating_sub(modal_h) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_w, modal_h);

    // Punch a hole and draw the border block.
    frame.render_widget(Clear, modal_area);
    let block = Block::default()
        .title("  save ")
        .borders(Borders::ALL)
        .title_style(Style::default().yellow())
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // inner layout (2 rows):
    // Row 1: input + ".ktr" suffix
    // Row 2: resolved path preview

    // ── Row 1: filename input ─────────────────────────────────────────────────
    let input_row = Rect::new(inner.x, inner.y, inner.width, 1);

    // The static ".ktr" label sits at the right edge; textarea gets the rest.
    let suffix = ".ktr";
    let suffix_w = suffix.len() as u16 + 1; // 1 space gap
    let input_w = input_row.width.saturating_sub(suffix_w);

    let ta_area = Rect::new(input_row.x, input_row.y, input_w, 1);
    let suffix_area = Rect::new(input_row.x + input_w, input_row.y, suffix_w, 1);

    frame.render_widget(
        textarea,
        ta_area.union(ta_area.offset(Offset::new(1, 0))), // +1 for cursor at right edge
    );
    frame.render_widget(
        Paragraph::new(suffix).style(Style::default().fg(Color::DarkGray)),
        suffix_area,
    );

    // ── Row 2: resolved path preview ─────────────────────────────────────────
    let path_row = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let input_text = textarea.lines().first().map(|s| s.as_str()).unwrap_or("");
    let resolved = crate::update::resolve_save_path(input_text);
    let resolved_str = resolved.to_string_lossy();

    let path_line = if !input_text.is_empty() {
        Line::from(vec![
            Span::styled("   → ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                resolved_str.to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        let cwd_str = cwd.to_string_lossy();
        Line::from(vec![
            Span::styled("cwd: ", Style::default().fg(Color::DarkGray)),
            Span::styled(cwd_str.to_string(), Style::default().fg(Color::DarkGray)),
        ])
    };
    frame.render_widget(Paragraph::new(path_line), path_row);
}
