// ── Property panel data structure ─────────────────────────────────────────────
//
// A generic, keyboard-navigable panel consisting of *sections* (rows) each
// holding a horizontal list of *items* (options).  The panel is agnostic about
// what kind of element it describes — it simply stores items and the action
// that should be dispatched when the user confirms the focused item.
//
// For nodes the panel is built by `node_prop_panel`.  Future element types
// (edges, etc.) add their own builder functions without touching this file.

use crate::actions::Action;
use crate::state::{
    CornerStyle, NodeLayoutMode, NodePropChange, NodeProperties, TextAlignH, TextAlignV,
};

// ── Nerd Font icon constants ──────────────────────────────────────────────────

// Layout mode
const ICON_MANUAL: &str = "󰙖"; // nf-md-move_resize_variant
const ICON_CONTENT: &str = "󱗝"; // nf-md-circle_box_outline

// Corner style
const ICON_ROUNDED: &str = ""; // nf-fa-square_o
const ICON_SHARP: &str = ""; // nf-fa-square_full

// Horizontal text alignment
const ICON_ALIGN_LEFT: &str = ""; // nf-fa-arrow_left
const ICON_ALIGN_H_CENTER: &str = ""; // nf-fa-arrows_h
const ICON_ALIGN_RIGHT: &str = ""; // nf-fa-arrow_right

// Vertical text alignment
const ICON_ALIGN_TOP: &str = ""; // nf-fa-arrow_up
const ICON_ALIGN_V_CENTER: &str = ""; // nf-fa-arrows_v
const ICON_ALIGN_BOTTOM: &str = ""; // nf-fa-arrow_down

// ── Data types ────────────────────────────────────────────────────────────────

/// One selectable option within a [`PropSection`].
#[derive(Clone, Debug)]
pub struct PropItem {
    /// Nerd Font glyph displayed before the label.
    pub icon: &'static str,
    /// Short human-readable name, e.g. `"manual"`.
    pub label: &'static str,
    /// Whether this item represents the current value of the property.
    pub selected: bool,
    /// The action dispatched when the user presses Space on this item.
    pub action: Action,
}

/// A named horizontal group of [`PropItem`]s.
#[derive(Clone, Debug)]
pub struct PropSection {
    /// Section title, e.g. `"layout"`.
    pub name: &'static str,
    /// The items in this section, displayed left-to-right.
    pub items: Vec<PropItem>,
}

/// The full property panel: an ordered list of [`PropSection`]s plus the
/// current keyboard cursor position.
#[derive(Clone, Debug)]
pub struct PropPanel {
    pub sections: Vec<PropSection>,
    /// Index of the currently focused section (row).
    pub focused_section: usize,
    /// Index of the currently focused item within the focused section.
    pub focused_item: usize,
}

impl PropPanel {
    // ── Cursor navigation ─────────────────────────────────────────────────────

    /// Move focus up one section; clamp at 0; reset item cursor to 0.
    pub fn move_up(&mut self) {
        self.focused_section = self.focused_section.saturating_sub(1);
        self.focused_item = 0;
    }

    /// Move focus down one section; clamp at last section; reset item cursor to 0.
    pub fn move_down(&mut self) {
        let max = self.sections.len().saturating_sub(1);
        self.focused_section = (self.focused_section + 1).min(max);
        self.focused_item = 0;
    }

    /// Move focus left within the current section, wrapping around.
    pub fn move_left(&mut self) {
        let n = self.current_section_len();
        if n == 0 {
            return;
        }
        self.focused_item = if self.focused_item == 0 {
            n - 1
        } else {
            self.focused_item - 1
        };
    }

    /// Move focus right within the current section, wrapping around.
    pub fn move_right(&mut self) {
        let n = self.current_section_len();
        if n == 0 {
            return;
        }
        self.focused_item = (self.focused_item + 1) % n;
    }

    /// Return the [`Action`] of the currently focused item, if the cursor is
    /// within bounds.
    pub fn current_action(&self) -> Option<Action> {
        let section = self.sections.get(self.focused_section)?;
        let item = section.items.get(self.focused_item)?;
        Some(item.action.clone())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn current_section_len(&self) -> usize {
        self.sections
            .get(self.focused_section)
            .map(|s| s.items.len())
            .unwrap_or(0)
    }
}

// ── Node property panel builder ───────────────────────────────────────────────

/// Build a [`PropPanel`] from the current [`NodeProperties`].
///
/// The `selected` flag on each item is derived from `props` at build time.
/// When a property changes, call this function again with the updated `props`
/// and restore the previous cursor position (the caller in `update.rs` does
/// this automatically).
pub fn node_prop_panel(props: &NodeProperties) -> PropPanel {
    let sections = vec![
        // ── Layout mode ───────────────────────────────────────────────────────
        PropSection {
            name: "Layout",
            items: vec![
                PropItem {
                    icon: ICON_MANUAL,
                    label: "manual",
                    selected: props.layout_mode == NodeLayoutMode::Manual,
                    action: Action::SetNodeProp(NodePropChange::LayoutMode(NodeLayoutMode::Manual)),
                },
                PropItem {
                    icon: ICON_CONTENT,
                    label: "content",
                    selected: props.layout_mode == NodeLayoutMode::WrapContent,
                    action: Action::SetNodeProp(NodePropChange::LayoutMode(
                        NodeLayoutMode::WrapContent,
                    )),
                },
            ],
        },
        // ── Corner style ──────────────────────────────────────────────────────
        PropSection {
            name: "Corners",
            items: vec![
                PropItem {
                    icon: ICON_ROUNDED,
                    label: "rounded",
                    selected: props.corner_style == CornerStyle::Rounded,
                    action: Action::SetNodeProp(NodePropChange::CornerStyle(CornerStyle::Rounded)),
                },
                PropItem {
                    icon: ICON_SHARP,
                    label: "sharp",
                    selected: props.corner_style == CornerStyle::Sharp,
                    action: Action::SetNodeProp(NodePropChange::CornerStyle(CornerStyle::Sharp)),
                },
            ],
        },
        // ── Horizontal text alignment ─────────────────────────────────────────
        PropSection {
            name: "Align horizontally",
            items: vec![
                PropItem {
                    icon: ICON_ALIGN_LEFT,
                    label: "left",
                    selected: props.text_align_h == TextAlignH::Left,
                    action: Action::SetNodeProp(NodePropChange::TextAlignH(TextAlignH::Left)),
                },
                PropItem {
                    icon: ICON_ALIGN_H_CENTER,
                    label: "center",
                    selected: props.text_align_h == TextAlignH::Center,
                    action: Action::SetNodeProp(NodePropChange::TextAlignH(TextAlignH::Center)),
                },
                PropItem {
                    icon: ICON_ALIGN_RIGHT,
                    label: "right",
                    selected: props.text_align_h == TextAlignH::Right,
                    action: Action::SetNodeProp(NodePropChange::TextAlignH(TextAlignH::Right)),
                },
            ],
        },
        // ── Vertical text alignment ───────────────────────────────────────────
        PropSection {
            name: "Align vertically",
            items: vec![
                PropItem {
                    icon: ICON_ALIGN_TOP,
                    label: "top",
                    selected: props.text_align_v == TextAlignV::Top,
                    action: Action::SetNodeProp(NodePropChange::TextAlignV(TextAlignV::Top)),
                },
                PropItem {
                    icon: ICON_ALIGN_V_CENTER,
                    label: "middle",
                    selected: props.text_align_v == TextAlignV::Center,
                    action: Action::SetNodeProp(NodePropChange::TextAlignV(TextAlignV::Center)),
                },
                PropItem {
                    icon: ICON_ALIGN_BOTTOM,
                    label: "bottom",
                    selected: props.text_align_v == TextAlignV::Bottom,
                    action: Action::SetNodeProp(NodePropChange::TextAlignV(TextAlignV::Bottom)),
                },
            ],
        },
    ];

    PropPanel {
        sections,
        focused_section: 0,
        focused_item: 0,
    }
}
