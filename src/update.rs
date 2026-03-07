// ── Update ────────────────────────────────────────────────────────────────────
//
// Pure state-transition logic.  `update` takes the current `AppState` and a
// resolved `Action` and mutates the state accordingly.
//
// Returns `UpdateResult` so the caller knows whether to keep running.

use crate::actions::Action;
use crate::geometry::{Dir, SPoint, SRect};
use crate::state::{AppState, ArrowDecorations, BlockMode, Edge, Mode, Node, NodeId, Side};
use crate::ui;
use ratatui::layout::Size;

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

// ── Core update function ──────────────────────────────────────────────────────

/// Apply `action` to `state` and return whether the application should keep
/// running.  `canvas_size` is needed by `StartSelecting` (visible-node labels)
/// and `FocusSelected` (viewport centering).
pub fn update(state: &mut AppState, action: Action, canvas_size: Size) -> UpdateResult {
    match action {
        // ── Application ───────────────────────────────────────────────────────
        Action::Quit => return UpdateResult::Quit,

        // ── Viewport panning (Normal mode) ────────────────────────────────────
        Action::Pan(dir) => {
            let mut t = state.vp.desired_center;
            match dir {
                Dir::Left => t.x -= 3,
                Dir::Right => t.x += 3,
                Dir::Up => t.y -= 3,
                Dir::Down => t.y += 3,
            }
            state.vp.set_center(t);
        }

        // ── Node movement ─────────────────────────────────────────────────────
        Action::Move(dir) => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    match dir {
                        Dir::Left => node.rect.origin.x -= 1,
                        Dir::Right => node.rect.origin.x += 1,
                        Dir::Up => node.rect.origin.y -= 1,
                        Dir::Down => node.rect.origin.y += 1,
                    }
                }
            }
        }

        Action::MoveFast(dir) => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter_mut().find(|n| n.id == id) {
                    match dir {
                        Dir::Left => node.rect.origin.x -= 5,
                        Dir::Right => node.rect.origin.x += 5,
                        Dir::Up => node.rect.origin.y -= 5,
                        Dir::Down => node.rect.origin.y += 5,
                    }
                }
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

                    let new_id = NodeId(state.nodes.len());
                    state.nodes.push(Node {
                        id: new_id,
                        rect,
                        label: String::new(),
                    });
                    state.edges.push(Edge {
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
            let labels = ui::assign_labels(
                &state.nodes,
                &state.vp,
                canvas_size.width as i32,
                canvas_size.height as i32,
            );
            let prev = Box::new(state.mode.clone());
            state.mode = Mode::Selecting {
                labels,
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
                let id = *id;
                state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
            Mode::SelectedBlock(id, BlockMode::CreatingRelativeNode) => {
                let id = *id;
                state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
            Mode::SelectedBlock(id, BlockMode::Resizing) => {
                let id = *id;
                state.mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
            Mode::SelectedBlock(_, BlockMode::Selected) => {
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

        // ── Viewport focus ────────────────────────────────────────────────────
        Action::FocusSelected => {
            if let Mode::SelectedBlock(id, _) = state.mode {
                if let Some(node) = state.nodes.iter().find(|n| n.id == id) {
                    let c = node.rect.center();
                    let target = SPoint::new(
                        c.x - canvas_size.width as i32 / 2,
                        c.y - canvas_size.height as i32 / 2,
                    );
                    state.vp.set_center(target);
                }
            }
        }

        // ── Label selection ───────────────────────────────────────────────────
        Action::SelectChar(ch) => {
            if let Mode::Selecting {
                ref mut labels,
                ref mut current,
                ref prev,
            } = state.mode
            {
                current.push(ch);
                let current_str = current.clone();
                let prev_mode = *prev.clone();

                if let Some((matched_id, _)) =
                    labels.iter().find(|(_, label)| *label == current_str)
                {
                    let matched_id = *matched_id;
                    state.mode = Mode::SelectedBlock(matched_id, BlockMode::Selected);
                    return UpdateResult::Actions(vec![Action::FocusSelected]);
                }

                let any_partial = labels
                    .iter()
                    .any(|(_, label)| label.starts_with(current_str.as_str()));

                if !any_partial {
                    state.mode = prev_mode;
                }
            }
        }
    }

    UpdateResult::Continue
}
