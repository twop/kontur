pub mod actions;
pub mod binding;
pub mod geometry;
pub mod labels;
pub mod path;
pub mod state;
pub mod ui;
pub mod update;

use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyModifiers};
use geometry::{SPoint, SRect};
use ratatui::layout::Size;
use state::{AppState, ArrowDecorations, BlockMode, Edge, Mode, Node, NodeId, Side, Viewport};
use update::{update, UpdateResult};

fn format_key(code: KeyCode, mods: KeyModifiers) -> String {
    let key = match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Backspace => "bksp".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Tab => "tab".to_string(),
        other => format!("{:?}", other),
    };
    if mods.is_empty() {
        key
    } else {
        format!("{:?}+{}", mods, key)
    }
}

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

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut key_log: Vec<String> = Vec::new();

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
        let bindings = binding::bindings_for_mode(&app.mode);

        terminal.draw(|frame| {
            ui::render_map(
                frame, &app.nodes, &app.edges, &app.vp, &app.mode, &bindings, &key_log,
            );
        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                key_log.insert(0, format_key(key.code, key.modifiers));

                let term_size = terminal.size()?;
                let canvas_size = Size {
                    width: term_size.width,
                    height: term_size.height - 1,
                };

                // Walk bindings in order; the first match wins.
                let mut action = None;
                for b in bindings.iter() {
                    match b {
                        binding::Binding::Single(inst) => {
                            if inst.key.key == key.code && inst.key.modifiers == key.modifiers {
                                action = Some(inst.action.clone());
                                break;
                            }
                        }
                        binding::Binding::Group {
                            bindings: members, ..
                        } => {
                            if let Some(inst) = members
                                .iter()
                                .find(|i| i.key.key == key.code && i.key.modifiers == key.modifiers)
                            {
                                action = Some(inst.action.clone());
                                break;
                            }
                        }
                        binding::Binding::Listen(listener) => {
                            if let Some(a) = (listener.handler)(key) {
                                action = Some(a);
                                break;
                            }
                        }
                    }
                }

                if let Some(a) = action {
                    let mut quit = false;
                    let mut queue = VecDeque::from([a]);
                    while let Some(next) = queue.pop_front() {
                        match update(&mut app, next, canvas_size) {
                            UpdateResult::Quit => {
                                quit = true;
                                break;
                            }
                            UpdateResult::Continue => {}
                            UpdateResult::Actions(follow_up) => queue.extend(follow_up),
                        }
                    }
                    if quit {
                        break;
                    }
                }
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
