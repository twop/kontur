// ── Property panel data structure ─────────────────────────────────────────────
//
// A generic, keyboard-navigable panel consisting of *sections* (rows) each
// holding a horizontal list of *items* (options).  The panel is agnostic about
// what kind of element it describes — it simply stores items and the action
// that should be dispatched when the user confirms the focused item.
//
// For nodes the panel is built by `node_prop_panel`.  Future element types
// (edges, etc.) add their own builder functions without touching this file.

use crate::actions::{Action, CopyFormat};
use crate::path::PathSymbol;
use crate::state::{
    ArrowDecorations, CornerStyle, EdgePropChange, NodeLayoutMode, NodePropChange, NodeProperties,
    PropPanelCoord, TextAlignH, TextAlignV,
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
    /// optional section title, e.g. `"layout"`.
    pub name: Option<&'static str>,
    /// The items in this section, displayed left-to-right.
    pub items: Vec<PropItem>,
}

/// The full property panel: an ordered list of [`PropSection`]s plus the
/// current keyboard cursor position.
#[derive(Clone, Debug)]
pub struct PropPanel {
    pub sections: Vec<PropSection>,
    pub focused: PropPanelCoord,
}

impl PropPanel {
    // ── Cursor navigation ─────────────────────────────────────────────────────

    /// Move focus up one section; clamp at 0; reset item cursor to 0.
    pub fn move_up(&mut self) {
        let section = self.focused.section.saturating_sub(1);
        let item = self
            .focused
            .item
            .min(self.current_section_len().saturating_sub(1));

        self.focused = PropPanelCoord { section, item }
    }

    /// Move focus down one section; clamp at last section; reset item cursor to 0.
    pub fn move_down(&mut self) {
        let section = self.focused.section.saturating_add(1);
        let item = self
            .focused
            .item
            .min(self.current_section_len().saturating_sub(1));

        self.focused = PropPanelCoord { section, item }
    }

    /// Move focus left within the current section, wrapping around.
    pub fn move_left(&mut self) {
        let n = self.current_section_len();
        if n == 0 {
            return;
        }
        self.focused.item = if self.focused.item == 0 {
            n - 1
        } else {
            self.focused.item - 1
        };
    }

    /// Move focus right within the current section, wrapping around.
    pub fn move_right(&mut self) {
        let n = self.current_section_len();
        if n == 0 {
            return;
        }
        self.focused.item = (self.focused.item + 1) % n;
    }

    /// Return the [`Action`] of the currently focused item, if the cursor is
    /// within bounds.
    pub fn current_action(&self) -> Option<Action> {
        let section = self.sections.get(self.focused.section)?;
        let item = section.items.get(self.focused.item)?;
        Some(item.action.clone())
    }

    // ── Cursor persistence ────────────────────────────────────────────────────

    /// Restore a previously saved cursor position, clamping to the panel's
    /// current bounds so stale coords can never cause an out-of-bounds access.
    pub fn apply_coord(self, coord: PropPanelCoord) -> Self {
        let Self { sections, .. } = self;
        let section = sections.len().saturating_sub(1).min(coord.section);
        let item = sections
            .get(section)
            .map(|s| s.items.len().saturating_sub(1).min(coord.item))
            .unwrap_or(0);
        let focused = PropPanelCoord { section, item };
        Self { sections, focused }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn current_section_len(&self) -> usize {
        self.sections
            .get(self.focused.section)
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
pub fn node_prop_panel(props: &NodeProperties, prev_coords: Option<PropPanelCoord>) -> PropPanel {
    let sections = vec![
        // ── Layout mode ───────────────────────────────────────────────────────
        PropSection {
            name: Some("Layout"),
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
            name: Some("Corners"),
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
            name: Some("Align horizontally"),
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
            name: Some("Align vertically"),
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

    let panel = PropPanel {
        sections,
        focused: PropPanelCoord {
            section: 0,
            item: 0,
        },
    };

    match prev_coords {
        Some(prev) => panel.apply_coord(prev),
        None => panel,
    }
}

// ── Copy-as format panel builder ──────────────────────────────────────────────

/// Build a [`PropPanel`] for the "copy as" format picker.
///
/// One section — "Copy as" — with four items: plain, markdown, python, rust.
/// The `selected` flag on the matching item reflects `last_format` so the
/// previously chosen format is visually highlighted when the modal re-opens.
pub fn copy_as_panel(prev_coords: Option<PropPanelCoord>) -> PropPanel {
    let sections = vec![PropSection {
        name: None,
        items: vec![
            PropItem {
                icon: "",
                label: "plain",
                selected: false,
                action: Action::CopyAs(CopyFormat::Plain),
            },
            PropItem {
                icon: "",
                label: "markdown",
                selected: false,
                action: Action::CopyAs(CopyFormat::Markdown),
            },
            PropItem {
                icon: "",
                label: "python",
                selected: false,
                action: Action::CopyAs(CopyFormat::Python),
            },
            PropItem {
                icon: "", //
                label: "rust",
                selected: false,
                action: Action::CopyAs(CopyFormat::Rust),
            },
        ],
    }];

    let panel = PropPanel {
        sections,
        focused: PropPanelCoord {
            section: 0,
            item: 0,
        },
    };

    match prev_coords {
        Some(prev) => panel.apply_coord(prev),
        None => panel,
    }
}
// ── Edge property panel builder ───────────────────────────────────────────────

/// Build a [`PropPanel`] from the current edge [`ArrowDecorations`].
///
/// One section — "Arrows" — with two independently-toggleable items:
/// start (◁) and end (▷).  The `selected` flag reflects whether each
/// arrowhead is currently active.
pub fn edge_prop_panel(dir: ArrowDecorations, prev_coords: Option<PropPanelCoord>) -> PropPanel {
    let sections = vec![PropSection {
        name: Some("Arrows"),
        items: vec![
            PropItem {
                icon: PathSymbol::ArrowLeft.to_symbol(),
                label: "start",
                selected: dir.has_start(),
                action: Action::SetEdgeProp(EdgePropChange::ToggleStart),
            },
            PropItem {
                icon: PathSymbol::ArrowRight.to_symbol(),
                label: "end",
                selected: dir.has_end(),
                action: Action::SetEdgeProp(EdgePropChange::ToggleEnd),
            },
        ],
    }];

    let panel = PropPanel {
        sections,
        focused: PropPanelCoord {
            section: 0,
            item: 0,
        },
    };

    match prev_coords {
        Some(prev) => panel.apply_coord(prev),
        None => panel,
    }
}
