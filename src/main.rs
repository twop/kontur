pub mod actions;
pub mod binding;
pub mod geometry;
pub mod labels;
pub mod path;
pub mod state;
pub mod ui;
pub mod update;

use crossterm::event::KeyCode;
use geometry::{SPoint, SRect};
use state::{AppState, ArrowDecorations, BlockMode, Edge, Mode, Node, NodeId, Side, Viewport};

// ── Demo graph ────────────────────────────────────────────────────────────────

fn make_demo_graph() -> (Vec<Node>, Vec<Edge>) {
    let alpha = NodeId(0);
    let beta = NodeId(1);
    let gamma = NodeId(2);
    let delta = NodeId(3);

    let nodes = [
        Node {
            id: alpha,
            rect: SRect::new(5, 5, 16, 5),
            label: "Alpha".to_string(),
        },
        Node {
            id: beta,
            rect: SRect::new(35, 5, 16, 5),
            label: "Beta".to_string(),
        },
        Node {
            id: gamma,
            rect: SRect::new(35, 18, 16, 5),
            label: "Gamma".to_string(),
        },
        Node {
            id: delta,
            rect: SRect::new(5, 18, 16, 5),
            label: "Delta".to_string(),
        },
    ];

    let edges = vec![
        Edge {
            from_id: alpha,
            from_side: Side::Right,
            to_id: beta,
            to_side: Side::Left,
            dir: ArrowDecorations::Forward,
        },
        Edge {
            from_id: beta,
            from_side: Side::Bottom,
            to_id: gamma,
            to_side: Side::Top,
            dir: ArrowDecorations::Forward,
        },
        Edge {
            from_id: gamma,
            from_side: Side::Left,
            to_id: delta,
            to_side: Side::Right,
            dir: ArrowDecorations::Both,
        },
        Edge {
            from_id: delta,
            from_side: Side::Top,
            to_id: beta,
            to_side: Side::Bottom,
            dir: ArrowDecorations::Backward,
        },
    ];

    (Vec::from(nodes), edges)
}

// ── Cursor / input helpers ────────────────────────────────────────────────────

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

fn input_enter_char(input: &mut String, cursor: &mut usize, ch: char) {
    let idx = byte_index(input, *cursor);
    input.insert(idx, ch);
    *cursor = clamp_cursor(input, cursor.saturating_add(1));
}

fn input_delete_char(input: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let before = input.chars().take(*cursor - 1);
    let after = input.chars().skip(*cursor);
    *input = before.chain(after).collect();
    *cursor = clamp_cursor(input, cursor.saturating_sub(1));
}

// ── Input handling ────────────────────────────────────────────────────────────

fn handle_key(
    code: KeyCode,
    vp: &mut Viewport,
    mode: &mut Mode,
    nodes: &mut Vec<Node>,
    frame_w: i32,
    frame_h: i32,
) {
    match mode {
        Mode::SelectedBlock(id, BlockMode::Editing { input, cursor }) => match code {
            KeyCode::Char(ch) => input_enter_char(input, cursor, ch),
            KeyCode::Backspace => input_delete_char(input, cursor),
            KeyCode::Left => *cursor = clamp_cursor(input, cursor.saturating_sub(1)),
            KeyCode::Right => *cursor = clamp_cursor(input, cursor.saturating_add(1)),
            KeyCode::Enter => {
                let id = *id;
                let new_label = input.clone();
                if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                    node.label = new_label;
                }
                *mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
            KeyCode::Esc => {
                let id = *id;
                *mode = Mode::SelectedBlock(id, BlockMode::Selected);
            }
            _ => {}
        },

        Mode::SelectedBlock(id, BlockMode::Resizing) => {
            let id = *id;
            match code {
                KeyCode::Char('h') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        node.rect.origin.x -= 1;
                        node.rect.size.width += 1;
                    }
                }
                KeyCode::Char('l') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        node.rect.size.width += 1;
                    }
                }
                KeyCode::Char('k') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        node.rect.origin.y -= 1;
                        node.rect.size.height += 1;
                    }
                }
                KeyCode::Char('j') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        node.rect.size.height += 1;
                    }
                }
                KeyCode::Char('H') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        if node.rect.size.width > 3 {
                            node.rect.origin.x += 1;
                            node.rect.size.width -= 1;
                        }
                    }
                }
                KeyCode::Char('L') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        if node.rect.size.width > 3 {
                            node.rect.size.width -= 1;
                        }
                    }
                }
                KeyCode::Char('K') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        if node.rect.size.height > 3 {
                            node.rect.origin.y += 1;
                            node.rect.size.height -= 1;
                        }
                    }
                }
                KeyCode::Char('J') => {
                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                        if node.rect.size.height > 3 {
                            node.rect.size.height -= 1;
                        }
                    }
                }
                KeyCode::Esc => *mode = Mode::SelectedBlock(id, BlockMode::Selected),
                _ => {}
            }
        }

        Mode::SelectedBlock(id, BlockMode::Selected) => match code {
            KeyCode::Char('h') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.x -= 1;
                }
            }
            KeyCode::Char('H') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.x -= 5;
                }
            }
            KeyCode::Char('l') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.x += 1;
                }
            }
            KeyCode::Char('L') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.x += 5;
                }
            }
            KeyCode::Char('k') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.y -= 1;
                }
            }
            KeyCode::Char('K') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.y -= 5;
                }
            }
            KeyCode::Char('j') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.y += 1;
                }
            }
            KeyCode::Char('J') => {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == *id) {
                    node.rect.origin.y += 5;
                }
            }
            KeyCode::Char('r') => {
                let id = *id;
                *mode = Mode::SelectedBlock(id, BlockMode::Resizing);
            }
            KeyCode::Char('i') => {
                let id = *id;
                let current = nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.label.clone())
                    .unwrap_or_default();
                let cursor = current.chars().count();
                *mode = Mode::SelectedBlock(
                    id,
                    BlockMode::Editing {
                        input: current,
                        cursor,
                    },
                );
            }
            KeyCode::Enter => {
                let labels = ui::assign_labels(nodes, vp, frame_w, frame_h);
                let prev = Box::new(mode.clone());
                *mode = Mode::Selecting {
                    labels,
                    current: String::new(),
                    prev,
                };
            }
            KeyCode::Esc => *mode = Mode::Normal,
            _ => {}
        },

        Mode::Normal => match code {
            KeyCode::Char('h') => vp.center.x -= 3,
            KeyCode::Char('l') => vp.center.x += 3,
            KeyCode::Char('k') => vp.center.y += 3,
            KeyCode::Char('j') => vp.center.y -= 3,
            KeyCode::Enter => {
                let labels = ui::assign_labels(nodes, vp, frame_w, frame_h);
                let prev = Box::new(mode.clone());
                *mode = Mode::Selecting {
                    labels,
                    current: String::new(),
                    prev,
                };
            }
            _ => {}
        },

        Mode::Selecting {
            labels,
            current,
            prev,
        } => match code {
            KeyCode::Esc => {
                *mode = *prev.clone();
            }
            KeyCode::Char(ch) => {
                current.push(ch);
                let current_str = current.clone();
                let prev_mode = *prev.clone();

                if let Some((matched_id, _)) =
                    labels.iter().find(|(_, label)| *label == current_str)
                {
                    let matched_id = *matched_id;
                    *mode = Mode::SelectedBlock(matched_id, BlockMode::Selected);
                    return;
                }

                let any_partial = labels
                    .iter()
                    .any(|(_, label)| label.starts_with(current_str.as_str()));

                if !any_partial {
                    *mode = prev_mode;
                }
            }
            _ => {}
        },
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let (nodes, edges) = make_demo_graph();
    let mut app = AppState {
        nodes,
        edges,
        vp: Viewport {
            center: SPoint::new(0, 0),
        },
        mode: Mode::SelectedBlock(NodeId(0), BlockMode::Selected),
    };

    loop {
        terminal.draw(|frame| {
            ui::render_map(frame, &app.nodes, &app.edges, &app.vp, &app.mode);
        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                let editing = matches!(app.mode, Mode::SelectedBlock(_, BlockMode::Editing { .. }));
                if key.code == KeyCode::Char('q') && !editing {
                    break;
                }
                let size = terminal.size()?;
                let fw = size.width as i32;
                let fh = size.height as i32 - 1;
                handle_key(key.code, &mut app.vp, &mut app.mode, &mut app.nodes, fw, fh);
            }
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    Ok(())
}
