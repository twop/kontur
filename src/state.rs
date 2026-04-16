// ── Types ────────────────────────────────────────────────────────────────────

use std::path::PathBuf;

use ratatui::layout::Size;
use smallvec::SmallVec;

use crate::geometry::{Padding, SPoint, SRect};
pub use crate::viewport::{AnimationConfig, Viewport};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NodeId(pub(crate) usize);

#[cfg(test)]
impl NodeId {
    pub fn hacky(_0: usize) -> Self {
        Self(_0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EdgeId(pub(crate) usize);

/// Identifies either a node or an edge — used by the jump-label system to
/// enumerate both kinds of graph elements in a single sorted list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GraphId {
    Node(NodeId),
    Edge(EdgeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum ArrowDecorations {
    None,     // no arrowheads (plain connector)
    Forward,  // arrowhead at destination only
    Backward, // arrowhead at source only
    Both,     // arrowheads at both ends
}

impl ArrowDecorations {
    /// True when the *start* end (from-node side) has an arrowhead.
    pub fn has_start(self) -> bool {
        matches!(self, Self::Backward | Self::Both)
    }

    /// True when the *end* end (to-node side) has an arrowhead.
    pub fn has_end(self) -> bool {
        matches!(self, Self::Forward | Self::Both)
    }

    /// Toggle the start arrowhead on or off.
    pub fn toggle_start(self) -> Self {
        match (self.has_start(), self.has_end()) {
            (false, false) => Self::Backward,
            (false, true) => Self::Both,
            (true, false) => Self::None,
            (true, true) => Self::Forward,
        }
    }

    /// Toggle the end arrowhead on or off.
    pub fn toggle_end(self) -> Self {
        match (self.has_start(), self.has_end()) {
            (false, false) => Self::Forward,
            (false, true) => Self::None,
            (true, false) => Self::Both,
            (true, true) => Self::Backward,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NodeLayoutMode {
    #[default]
    Manual,
    WrapContent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CornerStyle {
    #[default]
    Sharp,
    Rounded,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextAlignH {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextAlignV {
    #[default]
    Top,
    Center,
    Bottom,
}

/// All visual/layout properties of a node that are editable via the properties
/// panel.  Kept as a sub-struct so it can be passed as a unit to
/// [`crate::prop_panel::node_prop_panel`] and mutated atomically by a
/// [`NodePropChange`].
#[derive(Clone, Debug, Default)]
pub struct NodeProperties {
    pub layout_mode: NodeLayoutMode,
    pub corner_style: CornerStyle,
    pub text_align_h: TextAlignH,
    pub text_align_v: TextAlignV,
}

/// A single targeted mutation to a [`NodeProperties`] value.
///
/// Carried as the payload of [`crate::actions::Action::SetNodeProp`]; this
/// keeps `PropItem` actions concrete while centralising all mutation logic
/// inside `update.rs`.
#[derive(Clone, Debug)]
pub enum NodePropChange {
    LayoutMode(NodeLayoutMode),
    CornerStyle(CornerStyle),
    TextAlignH(TextAlignH),
    TextAlignV(TextAlignV),
}

pub type LinesVec = SmallVec<[String; 1]>;

#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub rect: SRect,
    pub lines: LinesVec,
    pub padding: Padding,
    pub props: NodeProperties,
}

impl Node {
    /// Create a node with an explicitly supplied rectangle (manual layout).
    pub fn manual_layout(id: NodeId, rect: SRect, label: impl Into<String>) -> Self {
        Self {
            id,
            rect,
            lines: LinesVec::from_iter(label.into().split('\n').map(|l| l.to_string())),
            padding: Padding::default(),
            props: NodeProperties {
                layout_mode: NodeLayoutMode::Manual,
                ..NodeProperties::default()
            },
        }
    }

    /// Create a node whose size is calculated from the label text.
    ///
    /// Width  = longest line length + left padding + right padding + 2 border columns.
    /// Height = number of lines     + top padding  + bottom padding + 2 border rows.
    ///
    /// Both dimensions are clamped to a minimum of 3 so the rest of the
    /// codebase's "nodes must be at least 3×3" invariant is always satisfied.
    pub fn content_layout(id: NodeId, origin: SPoint, label: impl Into<String>) -> Self {
        Self::content_layout_with_padding(id, origin, label, Padding::default())
    }

    /// Create a node whose size is calculated from the label text, with custom padding around the node.
    ///
    /// Width  = longest line length + left padding + right padding + 2 border columns.
    /// Height = number of lines     + top padding  + bottom padding + 2 border rows.
    ///
    /// Both dimensions are clamped to a minimum of 3 so the rest of the
    /// codebase's "nodes must be at least 3×3" invariant is always satisfied.
    pub fn content_layout_with_padding(
        id: NodeId,
        origin: SPoint,
        label: impl Into<String>,
        padding: Padding,
    ) -> Self {
        let label: String = label.into();
        let max_chars = label
            .split('\n')
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as u16;
        let line_count = label.split('\n').count() as u16;

        let rect = create_node_rect_with_padding(
            origin,
            padding,
            Size::new(max_chars.max(1), line_count.max(1)),
        );

        Self {
            id,
            rect,
            lines: LinesVec::from_iter(label.split('\n').map(|l| l.to_string())),
            padding,
            props: NodeProperties {
                layout_mode: NodeLayoutMode::WrapContent,
                ..NodeProperties::default()
            },
        }
    }
}

pub fn create_node_rect_with_padding(
    origin: SPoint,
    padding: Padding,
    size: impl Into<Size>,
) -> SRect {
    let size = size.into();
    let size = Size::new(size.width.max(1), size.height.max(1));

    // border takes 1 on each side, hence "+2"
    let width = size.width + padding.left as u16 + padding.right as u16 + 2;
    let height = size.height + padding.top as u16 + padding.bottom as u16 + 2;
    let rect = SRect::from_origin(origin, Size::new(width, height));
    rect
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub id: EdgeId,
    pub from_id: NodeId,
    pub from_side: Side,
    pub to_id: NodeId,
    pub to_side: Side,
    pub dir: ArrowDecorations,
}

// ── Edge tweaking types ───────────────────────────────────────────────────────

/// Which endpoint of an edge is being tweaked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeEnd {
    From,
    To,
}

/// A single targeted mutation to an edge's [`ArrowDecorations`].
///
/// Carried as the payload of [`crate::actions::Action::SetEdgeProp`].
#[derive(Clone, Debug)]
pub enum EdgePropChange {
    ToggleStart,
    ToggleEnd,
}

/// Sub-mode for a selected edge.
#[derive(Clone)]
pub enum EdgeMode {
    /// Edge is selected; normal edge actions available.
    Selected,
    /// Choosing which endpoint to tweak: press 's' for the geometrically
    /// left/top endpoint, 'e' for the right/bottom endpoint.
    TweakEndpoint,
    /// Choosing which side for a specific node endpoint: h/j/k/l.
    TweakSide { node_id: NodeId },
    /// Keyboard-navigable property editor for the selected edge.
    PropEditing { panel: crate::prop_panel::PropPanel },
}

// ── Application mode ──────────────────────────────────────────────────────────
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BlockMode {
    Selected,
    // Moving,
    CreatingRelativeNode,
    Resizing,
    Editing {
        textarea: ratatui_textarea::TextArea<'static>,
        /// Label the node had when editing started — restored on Esc.
        original_label: LinesVec,
        /// Rect the node had when editing started — restored on Esc.
        original_rect: SRect,
    },
    /// Label-driven edge creation: shows jump labels on all other visible nodes;
    /// typing a label creates an edge from the source node to the chosen target.
    ConnectingEdge {
        node_labels: Vec<(NodeId, String)>,
        current: String,
    },
    /// Keyboard-navigable property editor for the selected node.
    ///
    /// The [`crate::prop_panel::PropPanel`] holds both the panel data (sections
    /// and items) and the current cursor position (focused section + item).
    /// It is built from [`NodeProperties`] when entering this mode and rebuilt
    /// in-place (preserving the cursor) after each [`NodePropChange`] is applied.
    PropEditing {
        panel: crate::prop_panel::PropPanel,
    },
}

#[derive(Clone)]
pub enum Mode {
    Normal,
    /// Single-line filename input for saving the scene.
    ///
    /// `textarea` holds the filename stem the user is typing.
    /// `prev` is the mode to restore on Cancel (Esc).
    SaveModal {
        textarea: ratatui_textarea::TextArea<'static>,
        prev: Box<Mode>,
    },
    /// Format-picker modal for copying the whole scene as formatted text.
    ///
    /// `panel` drives the option-selection UI (reuses [`crate::prop_panel::PropPanel`]).
    /// `prev` is the mode to restore on Cancel (Esc) or after a successful copy.
    CopyAsModal {
        panel: crate::prop_panel::PropPanel,
        prev: Box<Mode>,
    },
    SelectedBlock(NodeId, BlockMode),
    SelectedEdge(EdgeId, EdgeMode),
    /// Jump-to-node/edge selection mode (inspired by vimium/hop.nvim).
    ///
    /// `node_labels` — label assigned to every visible node.
    /// `edge_labels` — label assigned to every visible edge.
    /// `current`     — characters typed so far in this mode.
    /// `prev`        — mode to return to on Esc or a dead sequence.
    Selecting {
        node_labels: Vec<(NodeId, String)>,
        edge_labels: Vec<(EdgeId, String)>,
        current: String,
        prev: Box<Mode>,
    },
    /// Additive node-multi-select label overlay.
    ///
    /// Shows jump labels on every visible node. Typing a label toggles that
    /// node in/out of `selected`. Stays in this mode after each toggle so the
    /// user can keep adding/removing nodes without re-triggering.
    ///
    /// `Esc` → `MultiSelected { ids: selected }` if non-empty, else `Normal`.
    MultiSelecting {
        node_labels: Vec<(NodeId, String)>,
        current: String,
        /// Nodes toggled into the selection so far.
        selected: SmallVec<[NodeId; 4]>,
    },
    /// Two or more nodes selected simultaneously.
    ///
    /// `h/j/k/l` moves all selected nodes as a rigid group.
    /// `s` re-enters `MultiSelecting` with these ids pre-filled.
    /// `Esc` → `Normal`.
    MultiSelected {
        ids: SmallVec<[NodeId; 4]>,
    },
}

// ── Prop panel cursor coordinate ──────────────────────────────────────────────

/// Remembered cursor position inside a [`crate::prop_panel::PropPanel`].
///
/// Stored in [`AppState`] alongside the last-used properties so that reopening
/// the same panel type restores the cursor to where the user left it, enabling
/// quick iterative editing without having to re-navigate every time.
#[derive(Clone, Copy, Debug)]
pub struct PropPanelCoord {
    pub section: usize,
    pub item: usize,
}

// ── Application state ─────────────────────────────────────────────────────────

pub struct AppState {
    ids: (NodeId, EdgeId),
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub vp: Viewport,
    pub mode: Mode,
    /// The file path most recently saved to / loaded from.  `None` means the
    /// scene has never been persisted.  Not serialized — managed by `main.rs`.
    pub working_file: Option<PathBuf>,
    /// Properties (and panel cursor) from the last node prop-panel session.
    /// Applied automatically when creating new nodes.
    pub last_node_props: Option<(PropPanelCoord, NodeProperties)>,
    /// Properties (and panel cursor) from the last edge prop-panel session.
    /// Applied automatically when creating new edges.
    pub last_edge_props: Option<(PropPanelCoord, ArrowDecorations)>,
    /// Format (and panel cursor) from the last "copy as" session.
    /// Restored when the modal is re-opened so the cursor remembers the last
    /// format the user picked.
    pub last_copy_as: Option<PropPanelCoord>,
}

impl AppState {
    pub fn new(vp: Viewport, mode: Mode) -> Self {
        Self {
            ids: (NodeId(0), EdgeId(0)),
            nodes: Vec::new(),
            edges: Vec::new(),
            vp,
            mode,
            working_file: None,
            last_node_props: None,
            last_edge_props: None,
            last_copy_as: None,
        }
    }

    pub fn new_node_id(&mut self) -> NodeId {
        let (node_id, edge_id) = self.ids;
        self.ids = (NodeId(node_id.0 + 1), edge_id);
        node_id
    }
    pub fn new_edge_id(&mut self) -> EdgeId {
        let (node_id, edge_id) = self.ids;
        self.ids = (node_id, EdgeId(edge_id.0 + 1));
        edge_id
    }

    /// Construct an `AppState` from an existing set of nodes, edges, and a
    /// viewport.  The ID counters are derived from the maximum IDs present in
    /// the supplied collections so that subsequent allocations never collide
    /// with existing shapes.  The mode starts as `Normal`.
    pub fn from_parts(nodes: Vec<Node>, edges: Vec<Edge>, vp: Viewport) -> Self {
        let next_node = nodes.iter().map(|n| n.id.0).max().unwrap_or(0) + 1;
        let next_edge = edges.iter().map(|e| e.id.0).max().unwrap_or(0) + 1;
        Self {
            ids: (NodeId(next_node), EdgeId(next_edge)),
            nodes,
            edges,
            vp,
            mode: Mode::Normal,
            working_file: None,
            last_node_props: None,
            last_edge_props: None,
            last_copy_as: None,
        }
    }
}
