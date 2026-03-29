// ── Types ────────────────────────────────────────────────────────────────────

use ratatui::{layout::Size, widgets::StatefulWidget};

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
    Forward,  // arrowhead at destination only
    Backward, // arrowhead at source only
    Both,     // arrowheads at both ends
}

pub enum NodeLayoutMode {
    Manual,
    WrapContent,
}

pub struct Node {
    pub id: NodeId,
    pub rect: SRect,
    pub label: String,
    pub padding: Padding,
    pub layout_mode: NodeLayoutMode,
}

impl Node {
    /// Create a node with an explicitly supplied rectangle (manual layout).
    pub fn manual_layout(id: NodeId, rect: SRect, label: impl Into<String>) -> Self {
        Self {
            id,
            rect,
            label: label.into(),
            padding: Padding::default(),
            layout_mode: NodeLayoutMode::Manual,
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
        let label = label.into();
        let max_chars = label
            .split('\n')
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as u16;
        let line_count = label.split('\n').count() as u16;

        let rect = create_node_rect_with_padding(origin, padding, Size::new(max_chars, line_count));

        Self {
            id,
            rect,
            label,
            padding,
            layout_mode: NodeLayoutMode::WrapContent,
        }
    }
}

pub fn create_node_rect_with_padding(origin: SPoint, padding: Padding, size: Size) -> SRect {
    let width = (size.width + padding.left as u16 + padding.right as u16 + 2).max(3);
    let height = (size.height + padding.top as u16 + padding.bottom as u16 + 2).max(3);
    let rect = SRect::from_origin(origin, Size::new(width, height));
    rect
}

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

/// Sub-mode for a selected edge.
#[derive(Clone, PartialEq)]
pub enum EdgeMode {
    /// Edge is selected; normal edge actions available.
    Selected,
    /// Choosing which endpoint to tweak: press 's' for the geometrically
    /// left/top endpoint, 'e' for the right/bottom endpoint.
    TweakEndpoint,
    /// Choosing which side for a specific node endpoint: h/j/k/l.
    TweakSide { node_id: NodeId },
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
        original_label: String,
        /// Rect the node had when editing started — restored on Esc.
        original_rect: SRect,
    },
    /// Label-driven edge creation: shows jump labels on all other visible nodes;
    /// typing a label creates an edge from the source node to the chosen target.
    ConnectingEdge {
        node_labels: Vec<(NodeId, String)>,
        current: String,
    },
}

#[derive(Clone)]
pub enum Mode {
    Normal,
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
}

// ── Application state ─────────────────────────────────────────────────────────

pub struct AppState {
    ids: (NodeId, EdgeId),
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub vp: Viewport,
    pub mode: Mode,
}

impl AppState {
    pub fn new(vp: Viewport, mode: Mode) -> Self {
        Self {
            ids: (NodeId(0), EdgeId(0)),
            nodes: Vec::new(),
            edges: Vec::new(),
            vp,
            mode,
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
        }
    }
}
