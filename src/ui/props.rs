use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

use crate::{
    prop_panel::{PropItem, PropPanel, PropSection},
    state::PropPanelCoord,
};

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
    focused: PropPanelCoord,
    item_selected: bool,
) -> ItemTier {
    if section_idx == focused.section && item_idx == focused.item {
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

struct PropStyle {
    fg: Color,
    bold: bool,
    italic: bool,
}

impl PropStyle {
    fn into_style(self) -> Style {
        let mut modifiers = Modifier::empty();
        if self.bold {
            modifiers |= Modifier::BOLD;
        }
        if self.italic {
            modifiers |= Modifier::ITALIC;
        }
        Style::default().fg(self.fg).add_modifier(modifiers)
    }
}

fn prop_style_icon(tier: ItemTier) -> PropStyle {
    match tier {
        ItemTier::FocusedSelected | ItemTier::Selected => PropStyle {
            fg: Color::Yellow,
            bold: false,
            italic: false,
        },
        ItemTier::Focused | ItemTier::Inactive => PropStyle {
            fg: Color::Gray,
            bold: false,
            italic: false,
        },
    }
}

fn prop_style_text(tier: ItemTier) -> PropStyle {
    match tier {
        ItemTier::FocusedSelected => PropStyle {
            fg: Color::Yellow,
            bold: true,
            italic: false,
        },
        ItemTier::Focused => PropStyle {
            fg: Color::White,
            bold: false,
            italic: false,
        },
        ItemTier::Selected => PropStyle {
            fg: Color::Yellow,
            bold: true,
            italic: false,
        },
        ItemTier::Inactive => PropStyle {
            fg: Color::DarkGray,
            bold: false,
            italic: false,
        },
    }
}

fn section_name_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Gray).bold()
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
        section.name,
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
    focused: PropPanelCoord,
) -> [Line<'static>; 2] {
    let cyan = Style::default().fg(Color::Cyan);

    let mut items: Vec<Span<'static>> = vec![];
    let mut selection_spans: Vec<Span<'static>> = vec![];

    for (item_idx, item) in section.items.iter().enumerate() {
        let tier = item_tier(section_idx, item_idx, focused, item.selected);
        let icon_style = prop_style_icon(tier).into_style();
        let text_style = prop_style_text(tier).into_style();
        let is_focused = section_idx == focused.section && item_idx == focused.item;

        items.push(Span::styled(item.icon.to_owned(), icon_style));
        items.push(Span::styled(" ", text_style));
        items.push(Span::styled(item.label.to_owned(), text_style));

        if is_focused {
            let dashes: String = "─".repeat(item_text_width(item));
            selection_spans.push(Span::styled(format!("{}", dashes), cyan));
        } else {
            selection_spans.push(Span::raw(" ".repeat(item_text_width(item))));
        }

        // Separator between items
        if item_idx + 1 < section.items.len() {
            let sep = "   ";
            items.push(Span::raw(sep));
            selection_spans.push(Span::raw(sep));
        }
    }

    [Line::from(items), Line::from(selection_spans)]
}

// ── Panel entry point ─────────────────────────────────────────────────────────

/// Render the property panel anchored to the **top-right** corner of the frame.
pub fn render_props_panel(frame: &mut Frame, panel: &PropPanel) {
    if panel.sections.is_empty() {
        return;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(panel.sections.len() * 4);
    for (sec_idx, section) in panel.sections.iter().enumerate() {
        let is_focused_section = sec_idx == panel.focused.section;
        lines.push(section_header_line(section, is_focused_section));

        let [items_line, selection_underline_line] =
            section_item_lines(section, sec_idx, panel.focused);

        lines.push(items_line);
        lines.push(selection_underline_line);
    }

    // Compute content dimensions.
    let content_width = lines
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
    // +4 on each axis for the rounded border and padding
    let panel_w = (content_width + 4).min(fa.width);
    let panel_h = (content_h + 3).min(fa.height); // on the bottom there is already an empty line

    let x = fa.width.saturating_sub(panel_w);
    let y = 0;
    let area = Rect::new(x, y, panel_w, panel_h);

    let block = Block::default()
        .title(" properties ")
        .padding(Padding {
            left: 1,
            right: 1,
            top: 1,
            bottom: 0, // on the bottom we already have a new line for reserved for selection
        })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
