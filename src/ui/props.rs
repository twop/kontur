// ── Properties panel renderer ─────────────────────────────────────────────────
//
// Renders a [`PropPanel`] as a bordered, top-right-anchored overlay.
//
// Visual structure per section:
//
//   ╭─ properties ──────────────────╮
//   │ layout                        │   ← section name row
//   │   󰘔 manual   󰯫 content        │   ← items row (horizontal)
//   │ corners                       │
//   │   󰕤 rounded   󰔡 sharp         │
//   │ …                             │
//   ╰───────────────────────────────╯

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::prop_panel::{PropPanel, PropSection};

// ── Style helpers ─────────────────────────────────────────────────────────────

/// The three visual tiers for a property item (icon + label pair).
#[derive(Clone, Copy)]
enum ItemTier {
    /// Keyboard cursor is here.
    Focused,
    /// This item is the current value, but not focused.
    Selected,
    /// Neither.
    Inactive,
}

fn item_style(tier: ItemTier) -> Style {
    match tier {
        ItemTier::Focused => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::ITALIC),
        ItemTier::Selected => Style::default().fg(Color::Yellow),
        ItemTier::Inactive => Style::default().fg(Color::DarkGray),
    }
}

fn section_name_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::ITALIC)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

// ── Line builders ─────────────────────────────────────────────────────────────

fn section_header_line(section: &PropSection, focused: bool) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!(" {}", section.name),
        section_name_style(focused),
    )])
}

fn section_items_line(
    section: &PropSection,
    section_idx: usize,
    focused_section: usize,
    focused_item: usize,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    // Leading indent.
    spans.push(Span::raw("   "));

    for (item_idx, item) in section.items.iter().enumerate() {
        let tier = if section_idx == focused_section && item_idx == focused_item {
            ItemTier::Focused
        } else if item.selected {
            ItemTier::Selected
        } else {
            ItemTier::Inactive
        };
        let style = item_style(tier);

        spans.push(Span::styled(item.icon.to_owned(), style));
        spans.push(Span::styled(" ", style));
        spans.push(Span::styled(item.label.to_owned(), style));

        // Separator between items (except after the last one).
        if item_idx + 1 < section.items.len() {
            spans.push(Span::raw("   "));
        }
    }

    Line::from(spans)
}

// ── Panel entry point ─────────────────────────────────────────────────────────

/// Render the property panel anchored to the **top-right** corner of the frame.
pub fn render_props_panel(frame: &mut Frame, panel: &PropPanel) {
    if panel.sections.is_empty() {
        return;
    }

    // Build one header line + one items line per section.
    let mut lines: Vec<Line> = Vec::with_capacity(panel.sections.len() * 2);
    for (sec_idx, section) in panel.sections.iter().enumerate() {
        let is_focused_section = sec_idx == panel.focused_section;
        lines.push(section_header_line(section, is_focused_section));
        lines.push(section_items_line(
            section,
            sec_idx,
            panel.focused_section,
            panel.focused_item,
        ));

        if sec_idx != panel.sections.len() - 1 {
            // insert new line between sections
            lines.push(Line::raw(""));
        }
    }

    // Compute content dimensions.
    let content_w = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.chars().count())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(10) as u16;
    let content_h = lines.len() as u16;

    let fa = frame.area();
    // +2 on each axis for the rounded border.
    let panel_w = (content_w + 2).min(fa.width);
    let panel_h = (content_h + 2).min(fa.height);

    let x = fa.width.saturating_sub(panel_w);
    let y = 0;
    let area = Rect::new(x, y, panel_w, panel_h);

    let block = Block::default()
        .title(" properties ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
