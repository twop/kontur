// ── Update ────────────────────────────────────────────────────────────────────
//
// Pure state-transition logic.  `update` takes the current `AppState` and a
// resolved `Action` and mutates the state accordingly.
//
// Returns `UpdateResult` so the caller knows whether to keep running.

use crate::actions::Action;
use crate::geometry::Dir;
use crate::state::{AppState, BlockMode, Mode};
use crate::ui;

// ── Result ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResult {
    Continue,
    Quit,
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
/// running.  `frame_w` / `frame_h` are needed by `StartSelecting` to compute
/// which nodes are visible.
pub fn update(state: &mut AppState, action: Action, frame_w: i32, frame_h: i32) -> UpdateResult {
    match action {
        // ── Application ───────────────────────────────────────────────────────
        Action::Quit => return UpdateResult::Quit,

        // ── Viewport panning (Normal mode) ────────────────────────────────────
        Action::Pan(dir) => match dir {
            Dir::Left => state.vp.center.x -= 3,
            Dir::Right => state.vp.center.x += 3,
            Dir::Up => state.vp.center.y -= 3,
            Dir::Down => state.vp.center.y += 3,
        },

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
        Action::StartSelecting => {
            let labels = ui::assign_labels(&state.nodes, &state.vp, frame_w, frame_h);
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
                    return UpdateResult::Continue;
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
