// ── Scene persistence ─────────────────────────────────────────────────────────
//
// Serializable representations of the scene state (nodes, edges, viewport).
// serde annotations are intentionally confined to this file — the canonical
// state types in state.rs remain free of serialization concerns.
//
// These functions are pure: no I/O is performed here.  The caller (main.rs)
// is responsible for reading/writing the file and for applying the result to
// the application state.

use serde::{Deserialize, Serialize};

use crate::geometry::{SPoint, SRect};
use crate::state::{
    ArrowDecorations, Edge, EdgeId, NODE_PADDING, Node, NodeId, NodeLayoutMode, Side, Viewport,
};

// ── Mirror enums ──────────────────────────────────────────────────────────────

/// Serializable mirror of [`Side`].
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum SideSave {
    Top,
    Bottom,
    Left,
    Right,
}

impl From<Side> for SideSave {
    fn from(s: Side) -> Self {
        match s {
            Side::Top => SideSave::Top,
            Side::Bottom => SideSave::Bottom,
            Side::Left => SideSave::Left,
            Side::Right => SideSave::Right,
        }
    }
}

impl From<SideSave> for Side {
    fn from(s: SideSave) -> Self {
        match s {
            SideSave::Top => Side::Top,
            SideSave::Bottom => Side::Bottom,
            SideSave::Left => Side::Left,
            SideSave::Right => Side::Right,
        }
    }
}

/// Serializable mirror of [`ArrowDecorations`].
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ArrowSave {
    Forward,
    Backward,
    Both,
}

impl From<ArrowDecorations> for ArrowSave {
    fn from(a: ArrowDecorations) -> Self {
        match a {
            ArrowDecorations::Forward => ArrowSave::Forward,
            ArrowDecorations::Backward => ArrowSave::Backward,
            ArrowDecorations::Both => ArrowSave::Both,
        }
    }
}

impl From<ArrowSave> for ArrowDecorations {
    fn from(a: ArrowSave) -> Self {
        match a {
            ArrowSave::Forward => ArrowDecorations::Forward,
            ArrowSave::Backward => ArrowDecorations::Backward,
            ArrowSave::Both => ArrowDecorations::Both,
        }
    }
}

// ── Serializable node / edge ──────────────────────────────────────────────────

/// Serializable mirror of [`NodeLayoutMode`].
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum LayoutModeSave {
    Manual,
    WrapContent { padding: u8 },
}

impl From<&NodeLayoutMode> for LayoutModeSave {
    fn from(m: &NodeLayoutMode) -> Self {
        match m {
            NodeLayoutMode::Manual => LayoutModeSave::Manual,
            NodeLayoutMode::WrapContent { padding } => {
                LayoutModeSave::WrapContent { padding: *padding }
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct NodeSave {
    id: usize,
    x: i32,
    y: i32,
    width: u16,
    height: u16,
    label: String,
    /// Absent in older saves — defaults to [`Auto`](LayoutModeSave::Auto).
    #[serde(default)]
    layout_mode: Option<LayoutModeSave>,
}

#[derive(Serialize, Deserialize)]
pub struct EdgeSave {
    id: usize,
    from_id: usize,
    from_side: SideSave,
    to_id: usize,
    to_side: SideSave,
    dir: ArrowSave,
}

// ── Top-level scene snapshot ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct SceneSave {
    pub nodes: Vec<NodeSave>,
    pub edges: Vec<EdgeSave>,
    pub viewport_x: i32,
    pub viewport_y: i32,
}

// ── Pure conversion functions ─────────────────────────────────────────────────

/// Build a [`SceneSave`] from the individual scene components.
///
/// The caller (main.rs) is responsible for serializing and writing the result.
pub fn to_scene_save(nodes: &[Node], edges: &[Edge], vp: &Viewport) -> SceneSave {
    let nodes = nodes
        .iter()
        .map(|n| NodeSave {
            id: n.id.0,
            x: n.rect.origin.x,
            y: n.rect.origin.y,
            width: n.rect.size.width,
            height: n.rect.size.height,
            label: n.label.clone(),
            layout_mode: Some(LayoutModeSave::from(&n.layout_mode)),
        })
        .collect();

    let edges = edges
        .iter()
        .map(|e| EdgeSave {
            id: e.id.0,
            from_id: e.from_id.0,
            from_side: e.from_side.into(),
            to_id: e.to_id.0,
            to_side: e.to_side.into(),
            dir: e.dir.into(),
        })
        .collect();

    let center = vp.center();

    SceneSave {
        nodes,
        edges,
        viewport_x: center.x,
        viewport_y: center.y,
    }
}

/// Unpack a [`SceneSave`] into its constituent parts.
///
/// Returns `(viewport, nodes, edges)`.  The caller passes these directly to
/// [`AppState::from_parts`], which derives the next ID counters itself.
pub fn from_scene_save(save: SceneSave) -> (Viewport, Vec<Node>, Vec<Edge>) {
    let nodes: Vec<Node> = save
        .nodes
        .iter()
        .map(|n| {
            match n.layout_mode.unwrap_or(LayoutModeSave::WrapContent {
                padding: NODE_PADDING,
            }) {
                LayoutModeSave::Manual => Node::manual_layout(
                    NodeId(n.id),
                    SRect::new(n.x, n.y, n.width, n.height),
                    n.label.clone(),
                ),
                LayoutModeSave::WrapContent { padding } => {
                    Node::content_layout(NodeId(n.id), SPoint::new(n.x, n.y), n.label.clone())
                }
            }
        })
        .collect();

    let edges: Vec<Edge> = save
        .edges
        .iter()
        .map(|e| Edge {
            id: EdgeId(e.id),
            from_id: NodeId(e.from_id),
            from_side: e.from_side.into(),
            to_id: NodeId(e.to_id),
            to_side: e.to_side.into(),
            dir: e.dir.into(),
        })
        .collect();

    let viewport = Viewport::new(SPoint::new(save.viewport_x, save.viewport_y));

    (viewport, nodes, edges)
}
