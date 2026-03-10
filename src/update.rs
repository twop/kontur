// ── Update ────────────────────────────────────────────────────────────────────
//
// Pure state-transition logic.  `update` takes the current `AppState` and a
// resolved `Action` and mutates the state accordingly.
//
// Returns `UpdateResult` so the caller knows whether to keep running.

use crate::actions::Action;
use crate::geometry::{CanvasRect, Dir, SPoint, SRect};
use crate::labels::LabelIter;
use crate::path;

use crate::state::{
    AppState, ArrowDecorations, BlockMode, Edge, EdgeEnd, EdgeId, EdgeMode, GraphId, Mode, Node,
    NodeId, Side, Viewport,
};
use crate::viewport::AnimationConfig;
use ratatui::layout::Size;

// ── Per-action animation configs ──────────────────────────────────────────────

/// ExpoOut tween for incremental pan (h/j/k/l): fast start, snappy arrival.
const PAN_ANIM: AnimationConfig = AnimationConfig::Tween { duration: 0.25 };

/// Damped spring for large camera jumps (FocusSelected): distance-independent
/// settle with a gentle overshoot that makes the movement legible.
const JUMP_ANIM: AnimationConfig = AnimationConfig::Spring {
    angular_freq: 6.0,
    damping_ratio: 0.95,
};

// ── Label pools ───────────────────────────────────────────────────────────────

static SINGLE_CHARS: &[char] = &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
static DOUBLE_CHARS: &[char] = &['e', 'r', 'u', 'i', 'o'];

// ── Jump label assignment ─────────────────────────────────────────────────────

/// Chebyshev distance between two canvas points.  Used to sort jump targets by
/// proximity to the viewport centre so the shortest labels go to nearby items.
fn chebyshev(a: SPoint, b: SPoint) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Assign jump labels to every visible node and edge, sorted by distance from
/// the viewport centre (closest first, so shortest labels land on nearby items).
///
/// * Nodes are sorted by distance from their centre to `vp.desired_center`.
/// * Edges are sorted by the distance of whichever connection point is closer
///   (from or to) to `vp.desired_center`.
///
/// Returns separate vecs for nodes and edges so callers can look up either kind.
fn assign_jump_labels(
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    frame_size: Size,
) -> (Vec<(NodeId, String)>, Vec<(EdgeId, String)>) {
    let looking_at = vp.animated_center();
    let viewport_rect = CanvasRect::from_center(looking_at, frame_size);

    // ── Collect visible nodes ─────────────────────────────────────────────────
    let mut items: Vec<(i32, GraphId)> = nodes
        .iter()
        .filter(|n| viewport_rect.contains(n.rect.center()))
        .map(|n| (chebyshev(n.rect.center(), looking_at), GraphId::Node(n.id)))
        .collect();

    // ── Collect visible edges ─────────────────────────────────────────────────
    for edge in edges {
        let from_node = nodes.iter().find(|n| n.id == edge.from_id);
        let to_node = nodes.iter().find(|n| n.id == edge.to_id);
        if let (Some(from), Some(to)) = (from_node, to_node) {
            let from_pt = path::connection_point(from, edge.from_side);
            let to_pt = path::connection_point(to, edge.to_side);
            if !viewport_rect.contains(from_pt) || !viewport_rect.contains(from_pt) {
                continue;
            }
            // Use the closer of the two connection points as the sort key.
            let dist = chebyshev(from_pt, looking_at).min(chebyshev(to_pt, looking_at));
            items.push((dist, GraphId::Edge(edge.id)));
        }
    }

    // ── Sort by distance, closest first ──────────────────────────────────────
    items.sort_by_key(|(d, _)| *d);

    // ── Assign labels from the shared pool ───────────────────────────────────
    let mut node_labels: Vec<(NodeId, String)> = Vec::new();
    let mut edge_labels: Vec<(EdgeId, String)> = Vec::new();

    for (label, (_, id)) in LabelIter::new(SINGLE_CHARS, DOUBLE_CHARS).zip(items) {
        match id {
            GraphId::Node(nid) => node_labels.push((nid, label)),
            GraphId::Edge(eid) => edge_labels.push((eid, label)),
        }
    }

    (node_labels, edge_labels)
}

/// Assign jump labels to every visible node **except** `exclude_id`.
///
/// Used by `BlockMode::ConnectingEdge` to show target labels while one node is
/// already selected as the source.  The same proximity ordering and label pools
/// as `assign_jump_labels` are used so the UX feels consistent.
fn assign_node_labels(
    nodes: &[Node],
    exclude_id: NodeId,
    vp: &Viewport,
    canvas_size: Size,
) -> Vec<(NodeId, String)> {
    let center = vp.center();
    let viewport_rect = CanvasRect::from_center(center, canvas_size);

    let mut items: Vec<(i32, NodeId)> = nodes
        .iter()
        .filter(|n| n.id != exclude_id)
        .filter(|n| viewport_rect.contains(n.rect.center()))
        .map(|n| (chebyshev(n.rect.center(), center), n.id))
        .collect();

    items.sort_by_key(|(d, _)| *d);

    LabelIter::new(SINGLE_CHARS, DOUBLE_CHARS)
        .zip(items)
        .map(|(label, (_, id))| (id, label))
        .collect()
}

// ── Result ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateResult {
    Continue,
    Quit,
    /// Schedule a sequence of actions to be dispatched immediately after the
    /// current one.  The caller processes them in order; each may itself return
    /// `Actions(…)`, enabling arbitrarily-deep chaining.
    Actions(Vec<Action>),
}

// ── Text-editing helpers ──────────────────────────────────────────────────────

fn clamp_cursor(input: &str, pos: usize) -> usize {
    pos.clamp(0, input.chars().count())
}

fn byte_index(input: &str, cursor: usize) -> usize {
    input
        .char_indices()
        .map(|(i, _)| i)
        .nth(cursor)
        .unwrap_or(input.len())
}

fn input_insert(input: &mut String, cursor: &mut usize, ch: char) {
    let idx = byte_index(input, *cursor);
    input.insert(idx, ch);
    *cursor = clamp_cursor(input, cursor.saturating_add(1));
}

fn input_delete(input: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let before = input.chars().take(*cursor - 1);
    let after = input.chars().skip(*cursor);
    *input = before.chain(after).collect();
    *cursor = clamp_cursor(input, cursor.saturating_sub(1));
}

// ── Edge side computation ─────────────────────────────────────────────────────

/// Determine which sides of `src` and `dst` an edge should exit/enter from.
///
/// The algorithm works in canvas cell coordinates with an aspect-ratio
/// correction: terminal cells are roughly twice as tall as they are wide, so
/// `dy` is multiplied by 0.5 before comparing magnitudes.  This makes the
/// 45 ° diagonal threshold correspond to a true diagonal *on screen*.
///
/// **Special case — nodes that are horizontally overlapping and vertically
/// touching/overlapping:**  A Top→Bottom (or Bottom→Top) route would produce a
/// hairpin that the router cannot draw cleanly.  Instead we emit a same-side
/// exit (Right→Right or Left→Left) so the router produces a smooth S-shape.
fn compute_sides(src: &Node, dst: &Node) -> (Side, Side) {
    let src_c = src.rect.center();
    let dst_c = dst.rect.center();

    let dx = dst_c.x - src_c.x;
    let dy = dst_c.y - src_c.y;

    // Are the two nodes horizontally overlapping (center separation < sum of half-widths)?
    let half_widths = (src.rect.size.width / 2 + dst.rect.size.width / 2) as i32;
    let horizontally_overlapping = dx.abs() < half_widths;

    // Vertical gap between the nearest borders of the two rects.
    let vertical_gap = if dy >= 0 {
        dst.rect.top() - src.rect.bottom()
    } else {
        src.rect.top() - dst.rect.bottom()
    };

    // Special case: centres overlap horizontally AND the rects are touching or
    // overlapping vertically → exit from the same horizontal side to avoid a
    // hairpin route.
    if horizontally_overlapping && vertical_gap <= 0 {
        return if dx >= 0 {
            (Side::Right, Side::Right)
        } else {
            (Side::Left, Side::Left)
        };
    }

    // General case: compare |dx| vs |dy * 0.5| (aspect-ratio-corrected).
    // Use integer arithmetic: |dx| * 2 >= |dy|  ⟺  |dx| >= |dy * 0.5|
    if dx.abs() * 2 >= dy.abs() {
        // Primarily horizontal.
        if dx >= 0 {
            (Side::Right, Side::Left)
        } else {
            (Side::Left, Side::Right)
        }
    } else {
        // Primarily vertical.
        if dy >= 0 {
            (Side::Bottom, Side::Top)
        } else {
            (Side::Top, Side::Bottom)
        }
    }
}

// ── Core update function ──────────────────────────────────────────────────────

/// Apply `action` to `state` and return whether the application should keep
/// running.  `canvas_size` is needed by `StartSelecting` (visible-node labels)
/// and `FocusSelected` (viewport centering).
pub fn update(state: &mut AppState, action: Action, canvas_size: Size) -> UpdateResult {
    match action {
        // ── Application ───────────────────────────────────────────────────────
        Action::Quit => return UpdateResult::Quit,

        // ── Viewport panning (Normal mode) ────────────────────────────────────
        Action::Pan(dir, amount) => {
            let mut t = state.vp.center();
            match dir {
                Dir::Left => t.x -= amount as i32,
                Dir::Right => t.x += amount as i32,
                Dir::Up => t.y -= amount as i32,
                Dir::Down => t.y += amount as i32,
            }
            state.vp.set_center(t, &PAN_ANIM);
        }

        Action::CreateNewNode => {
            // Default size for every new node.
            const NEW_W: u16 = 16;
            const NEW_H: u16 = 5;

            let rect = SRect::from_center(state.vp.center(), Size::new(NEW_W, NEW_H));

            let id = state.new_node_id();
            state.nodes.push(Node {
                id,
                rect,
                label: String::new(),
            });

            state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
            return UpdateResult::Actions(vec![Action::StartEditing, Action::FocusSelected]);
        }

        // ── Node movement ─────────────────────────────────────────────────────
        Action::Move(dir, amount) => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    match dir {
                        Dir::Left => node.rect.origin.x -= amount as i32,
                        Dir::Right => node.rect.origin.x += amount as i32,
                        Dir::Up => node.rect.origin.y -= amount as i32,
                        Dir::Down => node.rect.origin.y += amount as i32,
                    }
                }
                // Keep the viewport following the node if it drifts outside
                // the safe zone.  FocusSelected already applies the boundary
                // check, so this is a no-op when the node is well-centred.
                return UpdateResult::Actions(vec![Action::FocusSelected]);
            }
        }

        // ── Resize: expand ────────────────────────────────────────────────────
        Action::Expand(dir) => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    match dir {
                        Dir::Left => {
                            node.rect.origin.x -= 1;
                            node.rect.size.width += 1;
                        }
                        Dir::Right => node.rect.size.width += 1,
                        Dir::Up => {
                            node.rect.origin.y -= 1;
                            node.rect.size.height += 1;
                        }
                        Dir::Down => node.rect.size.height += 1,
                    }
                }
            }
        }

        // ── Resize: shrink ────────────────────────────────────────────────────
        Action::Shrink(dir) => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    match dir {
                        Dir::Left => {
                            if node.rect.size.width > 3 {
                                node.rect.origin.x += 1;
                                node.rect.size.width -= 1;
                            }
                        }
                        Dir::Right => {
                            if node.rect.size.width > 3 {
                                node.rect.size.width -= 1;
                            }
                        }
                        Dir::Up => {
                            if node.rect.size.height > 3 {
                                node.rect.origin.y += 1;
                                node.rect.size.height -= 1;
                            }
                        }
                        Dir::Down => {
                            if node.rect.size.height > 3 {
                                node.rect.size.height -= 1;
                            }
                        }
                    }
                }
            }
        }

        // ── Mode transitions ──────────────────────────────────────────────────
        Action::StartConnectingEdge => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                state.mode = Mode::SelectedBlock(
                    id,
                    BlockMode::ConnectingEdge {
                        node_labels: assign_node_labels(&state.nodes, id, &state.vp, canvas_size),
                        current: String::new(),
                    },
                );
            }
        }

        Action::ConnectNodes(from_id, to_id) => {
            let (from_side, to_side) = {
                let src = state.nodes.iter().find(|n| n.id == from_id);
                let dst = state.nodes.iter().find(|n| n.id == to_id);
                match (src, dst) {
                    (Some(s), Some(d)) => compute_sides(s, d),
                    _ => (Side::Right, Side::Left),
                }
            };
            let edge_id = state.new_edge_id();
            state.edges.push(Edge {
                id: edge_id,
                from_id,
                from_side,
                to_id,
                to_side,
                dir: ArrowDecorations::Forward,
            });
            // If we're in ConnectingEdge mode, return to Selected on the source node.
            if let Mode::SelectedBlock(src_id, BlockMode::ConnectingEdge { .. }) = state.mode {
                state.mode = Mode::SelectedBlock(src_id, BlockMode::Selected);
            }
        }

        Action::StartCreatingRelativeNode => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                state.mode = Mode::SelectedBlock(id, BlockMode::CreatingRelativeNode);
            }
        }

        Action::CreateRelativeNode(dir) => {
            if let Mode::SelectedBlock(src_id, BlockMode::CreatingRelativeNode) = state.mode {
                // Default size for every new node.
                const NEW_W: u16 = 16;
                const NEW_H: u16 = 5;
                // Gap in cells between the source node border and the new node border.
                const GAP: i32 = 2;

                let new_rect = if let Some(src) = state.nodes.iter().find(|n| n.id == src_id) {
                    let (nx, ny) = match dir {
                        Dir::Right => (src.rect.right() + 1 + GAP, src.rect.origin.y),
                        Dir::Left => (src.rect.left() - GAP - NEW_W as i32, src.rect.origin.y),
                        Dir::Down => (src.rect.origin.x, src.rect.bottom() + 1 + GAP),
                        Dir::Up => (src.rect.origin.x, src.rect.top() - GAP - NEW_H as i32),
                    };
                    Some(SRect::new(nx, ny, NEW_W, NEW_H))
                } else {
                    None
                };

                if let Some(rect) = new_rect {
                    let (from_side, to_side) = match dir {
                        Dir::Right => (Side::Right, Side::Left),
                        Dir::Left => (Side::Left, Side::Right),
                        Dir::Down => (Side::Bottom, Side::Top),
                        Dir::Up => (Side::Top, Side::Bottom),
                    };

                    let new_id = state.new_node_id();
                    let new_edge_id = state.new_edge_id();
                    state.nodes.push(Node {
                        id: new_id,
                        rect,
                        label: String::new(),
                    });
                    state.edges.push(Edge {
                        id: new_edge_id,
                        from_id: src_id,
                        from_side,
                        to_id: new_id,
                        to_side,
                        dir: ArrowDecorations::Forward,
                    });
                    // Immediately enter editing mode on the new node.
                    state.mode = Mode::SelectedBlock(
                        new_id,
                        BlockMode::Editing {
                            input: String::new(),
                            cursor: 0,
                        },
                    );
                    return UpdateResult::Actions(vec![Action::FocusSelected]);
                }
            }
        }

        Action::StartSelecting => {
            let (node_labels, edge_labels) =
                assign_jump_labels(&state.nodes, &state.edges, &state.vp, canvas_size);
            let prev = Box::new(state.mode.clone());
            state.mode = Mode::Selecting {
                node_labels,
                edge_labels,
                current: String::new(),
                prev,
            };
        }

        Action::StartResizing => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                state.mode = Mode::SelectedBlock(id, BlockMode::Resizing);
            }
        }

        Action::StartEditing => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                let current = state
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.label.clone())
                    .unwrap_or_default();
                let cursor = current.chars().count();
                state.mode = Mode::SelectedBlock(
                    id,
                    BlockMode::Editing {
                        input: current,
                        cursor,
                    },
                );
            }
        }

        Action::Confirm => {
            if let Mode::SelectedBlock(id, BlockMode::Editing { ref input, .. }) = state.mode {
                let id = id;
                let new_label = input.clone();
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    node.label = new_label;
                }
                state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
        }

        Action::Cancel => match &state.mode {
            Mode::SelectedBlock(id, BlockMode::Editing { .. }) => {
                state.mode = Mode::SelectedBlock(*id, BlockMode::Selected);
            }
            Mode::SelectedBlock(id, BlockMode::CreatingRelativeNode) => {
                state.mode = Mode::SelectedBlock(*id, BlockMode::Selected);
            }
            Mode::SelectedBlock(id, BlockMode::Resizing) => {
                state.mode = Mode::SelectedBlock(*id, BlockMode::Selected);
            }
            Mode::SelectedBlock(id, BlockMode::ConnectingEdge { .. }) => {
                state.mode = Mode::SelectedBlock(*id, BlockMode::Selected);
            }
            Mode::SelectedBlock(_, BlockMode::Selected) => {
                state.mode = Mode::Normal;
            }
            Mode::SelectedEdge(id, EdgeMode::TweakSide { .. }) => {
                let id = *id;
                state.mode = Mode::SelectedEdge(id, EdgeMode::TweakEndpoint);
            }
            Mode::SelectedEdge(id, EdgeMode::TweakEndpoint) => {
                state.mode = Mode::SelectedEdge(*id, EdgeMode::Selected);
            }
            Mode::SelectedEdge(_, EdgeMode::Selected) => {
                state.mode = Mode::Normal;
            }
            Mode::Selecting { prev, .. } => {
                state.mode = *prev.clone();
            }
            Mode::Normal => {}
        },

        // ── Text editing ──────────────────────────────────────────────────────
        Action::InsertChar(ch) => {
            if let Mode::SelectedBlock(
                _,
                BlockMode::Editing {
                    ref mut input,
                    ref mut cursor,
                },
            ) = state.mode
            {
                input_insert(input, cursor, ch);
            }
        }

        Action::DeleteChar => {
            if let Mode::SelectedBlock(
                _,
                BlockMode::Editing {
                    ref mut input,
                    ref mut cursor,
                },
            ) = state.mode
            {
                input_delete(input, cursor);
            }
        }

        Action::CursorLeft => {
            if let Mode::SelectedBlock(
                _,
                BlockMode::Editing {
                    ref input,
                    ref mut cursor,
                },
            ) = state.mode
            {
                *cursor = clamp_cursor(input, cursor.saturating_sub(1));
            }
        }

        Action::CursorRight => {
            if let Mode::SelectedBlock(
                _,
                BlockMode::Editing {
                    ref input,
                    ref mut cursor,
                },
            ) = state.mode
            {
                *cursor = clamp_cursor(input, cursor.saturating_add(1));
            }
        }

        // ── Shape deletion ────────────────────────────────────────────────────
        Action::DeleteShape => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                state.nodes.retain(|n| n.id != id);
                state.edges.retain(|e| e.from_id != id && e.to_id != id);
                state.mode = Mode::Normal;
            }
        }

        // ── Edge deletion ─────────────────────────────────────────────────────
        Action::DeleteEdge => {
            if let Mode::SelectedEdge(id, EdgeMode::Selected) = state.mode {
                state.edges.retain(|e| e.id != id);
                state.mode = Mode::Normal;
            }
        }

        // ── Edge selection ────────────────────────────────────────────────────
        Action::SelectEdge(id) => {
            state.mode = Mode::SelectedEdge(id, EdgeMode::Selected);
        }

        // ── Edge connector tweaking ───────────────────────────────────────────
        Action::StartTweakEdge => {
            if let Mode::SelectedEdge(id, EdgeMode::Selected) = state.mode {
                state.mode = Mode::SelectedEdge(id, EdgeMode::TweakEndpoint);
            }
        }

        Action::SelectEdgeEnd(geom_end) => {
            if let Mode::SelectedEdge(id, EdgeMode::TweakEndpoint) = state.mode {
                // Resolve the geometric "s"/"e" choice to the concrete NodeId
                // being tweaked, using the normalized endpoint order.
                let node_id = state
                    .edges
                    .iter()
                    .find(|e| e.id == id)
                    .and_then(|edge| path::edge_endpoints_ordered(&state.nodes, edge))
                    .map(|(left, right)| match geom_end {
                        EdgeEnd::From => left.0, // "s" → left/top node
                        EdgeEnd::To => right.0,  // "e" → right/bottom node
                    });
                if let Some(node_id) = node_id {
                    state.mode = Mode::SelectedEdge(id, EdgeMode::TweakSide { node_id });
                }
            }
        }

        Action::SetEdgeSide(side) => {
            if let Mode::SelectedEdge(id, EdgeMode::TweakSide { node_id }) = state.mode {
                if let Some(edge) = state.edges.iter_mut().find(|e| e.id == id) {
                    if node_id == edge.from_id {
                        edge.from_side = side;
                    } else if node_id == edge.to_id {
                        edge.to_side = side;
                    }
                }
                state.mode = Mode::SelectedEdge(id, EdgeMode::Selected);
            }
        }

        // ── Viewport focus ────────────────────────────────────────────────────
        Action::FocusSelected => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter().find(|n| n.id == id) {
                    let target = node.rect.center();

                    // Only jump when the focused node has drifted outside the
                    // inner 50 % "safe zone" of the viewport.  This avoids
                    // triggering a camera move every time the user selects a
                    // node that is already roughly centered.
                    let safe_w = ((canvas_size.width * 2) / 3).max(1);
                    let safe_h = ((canvas_size.height * 2) / 3).max(1);
                    let safe_zone = CanvasRect::from_center(
                        state.vp.animated_center(),
                        ratatui::layout::Size::new(safe_w, safe_h),
                    );

                    if !safe_zone.contains(target) {
                        state.vp.set_center(target, &JUMP_ANIM);
                    }
                }
            }
        }

        // ── Label selection ───────────────────────────────────────────────────
        Action::SelectChar(ch) => match state.mode {
            Mode::Selecting {
                ref mut node_labels,
                ref mut edge_labels,
                ref mut current,
                ref prev,
            } => {
                current.push(ch);
                let current_str = current.clone();
                let prev_mode = *prev.clone();
                if let Some((matched_id, _)) =
                    node_labels.iter().find(|(_, label)| *label == current_str)
                {
                    let matched_id = *matched_id;
                    state.mode = Mode::SelectedBlock(matched_id, BlockMode::Selected);
                    return UpdateResult::Actions(vec![Action::FocusSelected]);
                }
                if let Some((matched_id, _)) =
                    edge_labels.iter().find(|(_, label)| *label == current_str)
                {
                    let matched_id = *matched_id;
                    state.mode = Mode::SelectedEdge(matched_id, EdgeMode::Selected);
                    return UpdateResult::Continue;
                }
                let any_partial = node_labels
                    .iter()
                    .any(|(_, label)| label.starts_with(current_str.as_str()))
                    || edge_labels
                        .iter()
                        .any(|(_, label)| label.starts_with(current_str.as_str()));
                if !any_partial {
                    state.mode = prev_mode;
                }
            }
            Mode::SelectedBlock(
                src_id,
                BlockMode::ConnectingEdge {
                    ref node_labels,
                    ref mut current,
                },
            ) => {
                current.push(ch);
                let current_str = current.clone();
                if let Some((target_id, _)) =
                    node_labels.iter().find(|(_, label)| *label == current_str)
                {
                    let target_id: NodeId = *target_id;
                    return UpdateResult::Actions(vec![Action::ConnectNodes(src_id, target_id)]);
                }
                let any_partial = node_labels
                    .iter()
                    .any(|(_, label)| label.starts_with(current_str.as_str()));
                if !any_partial {
                    state.mode = Mode::SelectedBlock(src_id, BlockMode::Selected);
                }
            }
            _ => (),
        },
    }

    UpdateResult::Continue
}
