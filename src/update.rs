// ── Update ────────────────────────────────────────────────────────────────────
//
// Pure state-transition logic.  `update` takes the current `AppState` and a
// resolved `Action` and mutates the state accordingly.
//
// Returns `UpdateResult` so the caller knows whether to keep running.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Size;
use ratatui_textarea::{CursorMove, Input, Key, TextArea};

use crate::actions::Action;
use crate::geometry::{CanvasRect, Dir, SPoint};
use crate::labels::LabelIter;
use crate::path;
use crate::prop_panel::{edge_prop_panel, node_prop_panel};

use crate::state::{
    AppState, ArrowDecorations, BlockMode, Edge, EdgeEnd, EdgeId, EdgeMode, EdgePropChange,
    GraphId, LinesVec, Mode, Node, NodeId, NodeLayoutMode, NodePropChange, Side, TextAlignH,
    Viewport, create_node_rect_with_padding,
};
use crate::viewport::AnimationConfig;

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

/// Side effects that the pure `update` function cannot perform itself (e.g.
/// I/O).  The caller in `main.rs` is responsible for executing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Serialize the current scene to `scene.kontur`.
    SaveScene,
    /// Deserialize `scene.kontur` and replace the current scene.
    LoadScene,
}

#[derive(Debug, Clone)]
pub enum UpdateResult {
    Continue,
    Quit,
    /// Schedule a sequence of actions to be dispatched immediately after the
    /// current one.  The caller processes them in order; each may itself return
    /// `Actions(…)`, enabling arbitrarily-deep chaining.
    Actions(Vec<Action>),
    /// A side effect that must be handled by the caller.
    Effect(Effect),
}

// ── TextArea helpers ──────────────────────────────────────────────────────────

/// Build a `TextArea` pre-filled with `text`, cursor at the end.
///
/// `align_h` is applied via `TextArea::set_alignment` so horizontal text
/// alignment in editing mode matches the node's property.
fn make_textarea(text: &[String], align_h: ratatui::layout::Alignment) -> TextArea<'static> {
    let mut ta = TextArea::new(Vec::from(text));
    ta.set_cursor_line_style(ratatui::style::Style::default());
    ta.remove_line_number();
    ta.set_alignment(align_h);
    ta.move_cursor(CursorMove::End);
    ta
}

/// Convert a `crossterm::event::KeyEvent` into a `ratatui_textarea::Input`.
///
/// `ratatui_textarea` uses `ratatui`'s re-exported crossterm types, which may
/// differ from the direct `crossterm` crate types.  We bridge the gap by
/// constructing `Input` from its public fields.
fn key_event_to_input(ev: crossterm::event::KeyEvent) -> Input {
    use crossterm::event::KeyEventKind;

    // Ignore key-release events (same as ratatui-textarea does internally).
    if ev.kind == KeyEventKind::Release {
        return Input::default();
    }

    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    let alt = ev.modifiers.contains(KeyModifiers::ALT);
    let shift = ev.modifiers.contains(KeyModifiers::SHIFT);

    let key = match ev.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Esc => Key::Esc,
        KeyCode::F(n) => Key::F(n),
        _ => Key::Null,
    };

    Input {
        key,
        ctrl,
        alt,
        shift,
    }
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

        // ── Scene persistence ─────────────────────────────────────────────────
        Action::SaveScene => return UpdateResult::Effect(Effect::SaveScene),
        Action::LoadScene => return UpdateResult::Effect(Effect::LoadScene),

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
            let id = state.new_node_id();
            let mut node = Node::content_layout(id, state.vp.center(), String::new());
            // Inherit last-used node properties.
            if let Some((_, props)) = state.last_node_props.clone() {
                node.props = props;
            }
            state.nodes.push(node);

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
                    node.props.layout_mode = NodeLayoutMode::Manual;
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
                    node.props.layout_mode = NodeLayoutMode::Manual;
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
            let edge_dir = state
                .last_edge_props
                .map(|(_, d)| d)
                .unwrap_or(ArrowDecorations::Forward);
            state.edges.push(Edge {
                id: edge_id,
                from_id,
                from_side,
                to_id,
                to_side,
                dir: edge_dir,
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
                let new_id = state.new_node_id();
                let mut new_node = Node::content_layout(new_id, SPoint::new(0, 0), String::new());
                let node_size = new_node.rect.size;

                // Gap in cells between the source node border and the new node border.
                const GAP: i32 = 2;

                let new_origin = if let Some(src) = state.nodes.iter().find(|n| n.id == src_id) {
                    let origin = match dir {
                        Dir::Right => (src.rect.right() + 1 + GAP, src.rect.origin.y),
                        Dir::Left => (
                            src.rect.left() - GAP - node_size.width as i32,
                            src.rect.origin.y,
                        ),
                        Dir::Down => (src.rect.origin.x, src.rect.bottom() + 1 + GAP),
                        Dir::Up => (
                            src.rect.origin.x,
                            src.rect.top() - GAP - node_size.width as i32,
                        ),
                    };
                    Some(origin)
                } else {
                    None
                };

                if let Some((x, y)) = new_origin {
                    new_node.rect.origin = SPoint::new(x, y);
                    // Inherit last-used node properties.
                    if let Some((_, props)) = state.last_node_props.clone() {
                        new_node.props = props;
                    }

                    let (from_side, to_side) = match dir {
                        Dir::Right => (Side::Right, Side::Left),
                        Dir::Left => (Side::Left, Side::Right),
                        Dir::Down => (Side::Bottom, Side::Top),
                        Dir::Up => (Side::Top, Side::Bottom),
                    };

                    let new_edge_id = state.new_edge_id();
                    let edge_dir = state
                        .last_edge_props
                        .map(|(_, d)| d)
                        .unwrap_or(ArrowDecorations::Forward);
                    state.nodes.push(new_node);
                    state.edges.push(Edge {
                        id: new_edge_id,
                        from_id: src_id,
                        from_side,
                        to_id: new_id,
                        to_side,
                        dir: edge_dir,
                    });
                    // Immediately enter editing mode on the new node.
                    state.mode = Mode::SelectedBlock(new_id, BlockMode::Selected);
                    return UpdateResult::Actions(vec![
                        Action::StartEditing,
                        Action::FocusSelected,
                    ]);
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
                if let Some(node) = state.nodes.iter().find(|n| n.id == id) {
                    let original_label = node.lines.clone();
                    let original_rect = node.rect;
                    let align_h = match node.props.text_align_h {
                        TextAlignH::Left => ratatui::layout::Alignment::Left,
                        TextAlignH::Center => ratatui::layout::Alignment::Center,
                        TextAlignH::Right => ratatui::layout::Alignment::Right,
                    };
                    let textarea = make_textarea(&original_label, align_h);
                    state.mode = Mode::SelectedBlock(
                        id,
                        BlockMode::Editing {
                            textarea,
                            original_label,
                            original_rect,
                        },
                    );
                }
            }
        }

        Action::StartPropEditing => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter().find(|n| n.id == id) {
                    let prev_coords = state
                        .last_node_props
                        .as_ref()
                        .map(|(coords, _)| coords.clone());

                    state.mode = Mode::SelectedBlock(
                        id,
                        BlockMode::PropEditing {
                            panel: node_prop_panel(&node.props, prev_coords),
                        },
                    );
                }
            }
        }

        Action::StartEdgePropEditing => {
            if let Mode::SelectedEdge(id, _) = state.mode {
                if let Some(edge) = state.edges.iter().find(|e| e.id == id) {
                    let panel =
                        edge_prop_panel(edge.dir, state.last_edge_props.map(|(coords, _)| coords));
                    state.mode = Mode::SelectedEdge(id, EdgeMode::PropEditing { panel });
                }
            }
        }

        // ── Property panel navigation ─────────────────────────────────────────
        Action::PropNavUp => match state.mode {
            Mode::SelectedBlock(_, BlockMode::PropEditing { ref mut panel })
            | Mode::SelectedEdge(_, EdgeMode::PropEditing { ref mut panel }) => {
                panel.move_up();
            }
            _ => {}
        },

        Action::PropNavDown => match state.mode {
            Mode::SelectedBlock(_, BlockMode::PropEditing { ref mut panel })
            | Mode::SelectedEdge(_, EdgeMode::PropEditing { ref mut panel }) => {
                panel.move_down();
            }
            _ => {}
        },

        Action::PropNavLeft => match state.mode {
            Mode::SelectedBlock(_, BlockMode::PropEditing { ref mut panel })
            | Mode::SelectedEdge(_, EdgeMode::PropEditing { ref mut panel }) => {
                panel.move_left();
            }
            _ => {}
        },

        Action::PropNavRight => match state.mode {
            Mode::SelectedBlock(_, BlockMode::PropEditing { ref mut panel })
            | Mode::SelectedEdge(_, EdgeMode::PropEditing { ref mut panel }) => {
                panel.move_right();
            }
            _ => {}
        },

        Action::ApplyCurrentPropItem => {
            let action = match state.mode {
                Mode::SelectedBlock(_, BlockMode::PropEditing { ref panel })
                | Mode::SelectedEdge(_, EdgeMode::PropEditing { ref panel }) => {
                    panel.current_action()
                }
                _ => None,
            };
            if let Some(action) = action {
                return UpdateResult::Actions(vec![action]);
            }
        }

        Action::SetNodeProp(change) => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    match change {
                        NodePropChange::LayoutMode(m) => {
                            let prev_mode = node.props.layout_mode;
                            node.props.layout_mode = m;
                            if m == NodeLayoutMode::WrapContent
                                && prev_mode != NodeLayoutMode::WrapContent
                            {
                                let max_chars = node
                                    .lines
                                    .iter()
                                    .map(|l| l.chars().count())
                                    .max()
                                    .unwrap_or(0) as u16;
                                let line_count = node.lines.len() as u16;
                                node.rect = create_node_rect_with_padding(
                                    node.rect.origin,
                                    node.padding,
                                    (max_chars, line_count),
                                );
                            }
                        }
                        NodePropChange::CornerStyle(c) => node.props.corner_style = c,
                        NodePropChange::TextAlignH(a) => node.props.text_align_h = a,
                        NodePropChange::TextAlignV(a) => node.props.text_align_v = a,
                    };
                }

                // Rebuild panel preserving cursor position.
                if let Mode::SelectedBlock(id, BlockMode::PropEditing { ref mut panel }) =
                    state.mode
                {
                    if let Some(node) = state.nodes.iter().find(|n| n.id == id) {
                        let new_panel = node_prop_panel(&node.props, Some(panel.focused));
                        *panel = new_panel;
                    }
                }
            }
        }

        Action::SetEdgeProp(change) => {
            if let Mode::SelectedEdge(id, _) = state.mode {
                if let Some(edge) = state.edges.iter_mut().find(|e| e.id == id) {
                    edge.dir = match change {
                        EdgePropChange::ToggleStart => edge.dir.toggle_start(),
                        EdgePropChange::ToggleEnd => edge.dir.toggle_end(),
                    };
                }
                // Rebuild panel preserving cursor position.
                if let Mode::SelectedEdge(id, EdgeMode::PropEditing { ref mut panel }) = state.mode
                {
                    if let Some(edge) = state.edges.iter().find(|e| e.id == id) {
                        let new_panel = edge_prop_panel(edge.dir, Some(panel.focused));
                        *panel = new_panel;
                    }
                }
            }
        }

        Action::Confirm => {
            if let Mode::SelectedBlock(id, BlockMode::Editing { ref textarea, .. }) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    node.lines = LinesVec::from(textarea.lines());
                }
                state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
        }

        Action::Cancel => {
            // Each arm extracts what it needs from a read borrow of state.mode,
            // then drops the borrow before writing back.  This two-step pattern
            // avoids holding a shared &state reference across a mutable write.
            match state.mode {
                Mode::SelectedBlock(
                    id,
                    BlockMode::Editing {
                        ref original_label,
                        original_rect,
                        ..
                    },
                ) => {
                    if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                        node.lines = original_label.clone();
                        node.rect = original_rect;
                    }
                    state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
                }
                Mode::SelectedBlock(id, BlockMode::CreatingRelativeNode) => {
                    state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
                }
                Mode::SelectedBlock(id, BlockMode::Resizing) => {
                    state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
                }
                Mode::SelectedBlock(id, BlockMode::ConnectingEdge { .. }) => {
                    state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
                }
                Mode::SelectedBlock(id, BlockMode::PropEditing { ref panel }) => {
                    let props = state
                        .nodes
                        .iter()
                        .find(|n| n.id == id)
                        .map(|n| n.props.clone());

                    if let Some(props) = props {
                        state.last_node_props = Some((panel.focused, props));
                    }

                    state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
                }
                Mode::SelectedBlock(_, BlockMode::Selected) => {
                    state.mode = Mode::Normal;
                }
                Mode::SelectedEdge(id, EdgeMode::PropEditing { ref panel }) => {
                    if let Some(dir) = state.edges.iter().find(|e| e.id == id).map(|e| e.dir) {
                        state.last_edge_props = Some((panel.focused, dir));
                    }
                    state.mode = Mode::SelectedEdge(id, EdgeMode::Selected);
                }
                Mode::SelectedEdge(id, EdgeMode::TweakSide { .. }) => {
                    state.mode = Mode::SelectedEdge(id, EdgeMode::TweakEndpoint);
                }
                Mode::SelectedEdge(id, EdgeMode::TweakEndpoint) => {
                    state.mode = Mode::SelectedEdge(id, EdgeMode::Selected);
                }
                Mode::SelectedEdge(_, EdgeMode::Selected) => {
                    state.mode = Mode::Normal;
                }
                Mode::Selecting { ref prev, .. } => {
                    state.mode = *prev.clone();
                }
                _ => (),
            }
        }

        // ── Text editing ──────────────────────────────────────────────────────
        Action::TextAreaInput(key_event) => {
            if let Mode::SelectedBlock(
                id,
                BlockMode::Editing {
                    ref mut textarea, ..
                },
            ) = state.mode
                && let Some(ref mut node) = state.nodes.iter_mut().find(|n| n.id == id)
            {
                textarea.input(key_event_to_input(key_event));
                let max_chars = textarea
                    .lines()
                    .iter()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0) as u16;
                let line_count = textarea.lines().len() as u16;

                let desired_wrapped_rect = create_node_rect_with_padding(
                    node.rect.origin,
                    node.padding,
                    (max_chars, line_count),
                );

                match node.props.layout_mode {
                    NodeLayoutMode::Manual => {
                        // that basically means that we expand the rect if we exceed size
                        node.rect = node.rect.extend_to(desired_wrapped_rect.bottom_right());
                    }

                    NodeLayoutMode::WrapContent => {
                        node.rect = desired_wrapped_rect;
                    }
                }
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
