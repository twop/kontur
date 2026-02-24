// ── Types ────────────────────────────────────────────────────────────────────

use crate::geometry::{SPoint, SRect};

#[derive(Clone, Copy, PartialEq)]
pub struct NodeId(pub usize);

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
    pub from_id: NodeId,
    pub from_side: Side,
    pub to_id: NodeId,
    pub to_side: Side,
    pub dir: ArrowDecorations,
}

pub struct Viewport {
    pub center: SPoint,
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
    /// Jump-to-node selection mode (inspired by vimium/hop.nvim).
    ///
    /// `labels`  — assignment of a label string to every visible node.
    /// `current` — characters typed so far in this mode.
    /// `prev`    — mode to return to on Esc or a dead sequence.
    Selecting {
        labels: Vec<(NodeId, String)>,
        current: String,
        prev: Box<Mode>,
    },
}

// ── Application state ─────────────────────────────────────────────────────────

pub struct AppState {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub vp: Viewport,
    pub mode: Mode,
}
