use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::prop_panel::{PropItem, PropPanel, PropSection};

// ── Style helpers ─────────────────────────────────────────────────────────────

/// The four visual tiers for a property item (icon + label pair).
#[derive(Clone, Copy)]
enum ItemTier {
    /// Keyboard cursor is here AND this is the active value.
    FocusedSelected,
    /// Keyboard cursor is here, but this is not the active value.
    Focused,
    /// This item is the current value, but not focused.
    Selected,
    /// Neither.
    Inactive,
}

fn item_tier(
    section_idx: usize,
    item_idx: usize,
    focused_section: usize,
    focused_item: usize,
    item_selected: bool,
) -> ItemTier {
    if section_idx == focused_section && item_idx == focused_item {
        if item_selected {
            ItemTier::FocusedSelected
        } else {
            ItemTier::Focused
        }
    } else if item_selected {
        ItemTier::Selected
    } else {
        ItemTier::Inactive
    }
}

fn item_style(tier: ItemTier) -> Style {
    match tier {
        ItemTier::FocusedSelected => Style::default().fg(Color::Yellow).bold(),
        ItemTier::Focused => Style::default().fg(Color::Gray),
        ItemTier::Selected => Style::default().fg(Color::Yellow).bold(),
        ItemTier::Inactive => Style::default().fg(Color::Gray),
    }
}

fn section_name_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::Gray)
    }
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Visible char width of a single item's text: `icon + " " + label`.
fn item_text_width(item: &PropItem) -> usize {
    item.icon.chars().count() + 1 + item.label.chars().count()
}

// ── Line builders ─────────────────────────────────────────────────────────────

fn section_header_line(section: &PropSection, focused: bool) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("{}", section.name),
        section_name_style(focused),
    )])
}

/// Returns `[border_top, content, border_bottom]` lines for a section's items row.
///
/// The focused item gets a cyan box-drawing border with 1-space horizontal padding.
/// All other items are laid out inline with `   ` (3-space) separators.
fn section_item_lines(
    section: &PropSection,
    section_idx: usize,
    focused_section: usize,
    focused_item: usize,
) -> [Line<'static>; 2] {
    let cyan = Style::default().fg(Color::Cyan);

    let mut mid_spans: Vec<Span<'static>> = vec![];
    let mut bot_spans: Vec<Span<'static>> = vec![];

    for (item_idx, item) in section.items.iter().enumerate() {
        let tier = item_tier(
            section_idx,
            item_idx,
            focused_section,
            focused_item,
            item.selected,
        );
        let style = item_style(tier);
        let is_focused = section_idx == focused_section && item_idx == focused_item;

        mid_spans.push(Span::styled(item.icon.to_owned(), style));
        mid_spans.push(Span::styled(" ", style));
        mid_spans.push(Span::styled(item.label.to_owned(), style));

        if is_focused {
            let dashes: String = "─".repeat(item_text_width(item));
            bot_spans.push(Span::styled(format!("{}", dashes), cyan));
        } else {
            bot_spans.push(Span::raw(" ".repeat(item_text_width(item))));
        }

        // Separator between items
        if item_idx + 1 < section.items.len() {
            let sep = "   ";
            mid_spans.push(Span::raw(sep));
            bot_spans.push(Span::raw(sep));
        }
    }

    [Line::from(mid_spans), Line::from(bot_spans)]
}

// ── Panel entry point ─────────────────────────────────────────────────────────

/// Render the property panel anchored to the **top-right** corner of the frame.
pub fn render_props_panel(frame: &mut Frame, panel: &PropPanel) {
    if panel.sections.is_empty() {
        return;
    }

    // Build one header line + three item lines per section.
    let mut lines: Vec<Line> = Vec::with_capacity(panel.sections.len() * 4);
    for (sec_idx, section) in panel.sections.iter().enumerate() {
        let is_focused_section = sec_idx == panel.focused_section;
        lines.push(section_header_line(section, is_focused_section));

        let [items_line, selection_underline_line] =
            section_item_lines(section, sec_idx, panel.focused_section, panel.focused_item);

        lines.push(items_line);
        lines.push(selection_underline_line);

        if sec_idx != panel.sections.len() - 1 {
            // blank line between sections
            // lines.push(Line::raw(""));
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
