// ── Types ────────────────────────────────────────────────────────────────────

use crate::geometry::SRect;
pub use crate::viewport::{AnimationConfig, Viewport};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NodeId(usize);

#[cfg(test)]
impl NodeId {
    pub fn hacky(_0: usize) -> Self {
        Self(_0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EdgeId(usize);

/// Identifies either a node or an edge — used by the jump-label system to
/// enumerate both kinds of graph elements in a single sorted list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GraphId {
    Node(NodeId),
    Edge(EdgeId),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, PartialEq, Eq, Copy)]
pub enum ArrowDecorations {
    Forward,  // arrowhead at destination only
    Backward, // arrowhead at source only
    Both,     // arrowheads at both ends
}

#[allow(dead_code)]
pub struct Node {
    pub id: NodeId,
    pub rect: SRect,
    pub label: String,
}

pub struct Edge {
    pub id: EdgeId,
    pub from_id: NodeId,
    pub from_side: Side,
    pub to_id: NodeId,
    pub to_side: Side,
    pub dir: ArrowDecorations,
}

// ── Application mode ──────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
pub enum BlockMode {
    Selected,
    // Moving,
    CreatingRelativeNode,
    Resizing,
    Editing { input: String, cursor: usize },
}

#[derive(Clone, PartialEq)]
pub enum Mode {
    Normal,
    SelectedBlock(NodeId, BlockMode),
    SelectedEdge(EdgeId),
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
}
