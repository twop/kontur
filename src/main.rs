pub mod actions;
pub mod binding;
pub mod geometry;
pub mod labels;
pub mod path;
pub mod prop_panel;
pub mod scene_save;
pub mod screen_space;
pub mod state;
pub mod ui;
pub mod update;
pub mod viewport;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};
use geometry::{SPoint, SRect};
use ratatui::layout::Size;
use state::{AppState, ArrowDecorations, BlockMode, Edge, Mode, Node, Side};
use update::{UpdateResult, update};
use viewport::Viewport;

use crate::binding::{Binding, KeyBinding, bindings_for_mode};

// ── Save state ────────────────────────────────────────────────────────────────

/// Tracks whether the in-memory scene matches the last file written to disk.
///
/// `iteration` is incremented after every `update()` call.
/// `last_saved_iteration` is set to `iteration` after a successful write or
/// load.  The scene is dirty when the two values differ.
struct SaveState {
    iteration: u64,
    last_saved_iteration: u64,
}

impl SaveState {
    fn new() -> Self {
        Self {
            iteration: 0,
            last_saved_iteration: 0,
        }
    }

    fn is_dirty(&self) -> bool {
        self.iteration != self.last_saved_iteration
    }

    fn mark_saved(&mut self) {
        self.last_saved_iteration = self.iteration;
    }

    fn tick(&mut self) {
        self.iteration += 1;
    }
}

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

fn bootstrap_demo_graph(app: &mut AppState) {
    let alpha = app.new_node_id();
    let beta = app.new_node_id();
    let gamma = app.new_node_id();
    let delta = app.new_node_id();

    app.nodes
        .push(Node::content_layout(alpha, SPoint::new(5, 5), "Alpha"));
    app.nodes
        .push(Node::content_layout(beta, SPoint::new(35, 5), "Beta"));
    app.nodes
        .push(Node::content_layout(gamma, SPoint::new(35, 18), "Gamma"));
    app.nodes
        .push(Node::content_layout(delta, SPoint::new(5, 18), "Delta"));

    let e0 = app.new_edge_id();
    let e1 = app.new_edge_id();
    let e2 = app.new_edge_id();
    let e3 = app.new_edge_id();
    app.edges.push(Edge {
        id: e0,
        from_id: alpha,
        from_side: Side::Right,
        to_id: beta,
        to_side: Side::Left,
        dir: ArrowDecorations::Forward,
    });
    app.edges.push(Edge {
        id: e1,
        from_id: beta,
        from_side: Side::Bottom,
        to_id: gamma,
        to_side: Side::Top,
        dir: ArrowDecorations::Forward,
    });
    app.edges.push(Edge {
        id: e2,
        from_id: gamma,
        from_side: Side::Left,
        to_id: delta,
        to_side: Side::Right,
        dir: ArrowDecorations::Both,
    });
    app.edges.push(Edge {
        id: e3,
        from_id: delta,
        from_side: Side::Top,
        to_id: beta,
        to_side: Side::Bottom,
        dir: ArrowDecorations::Backward,
    });
}

fn bootstrap_small_demo_graph(app: &mut AppState) {
    let alpha = app.new_node_id();
    let beta = app.new_node_id();

    app.nodes.push(Node::manual_layout(
        alpha,
        SRect::new(-5, -3, 10, 3),
        "alpha",
    ));
    app.nodes
        .push(Node::manual_layout(beta, SRect::new(-5, 2, 10, 3), "beta"));

    let a_to_b = app.new_edge_id();
    app.edges.push(Edge {
        id: a_to_b,
        from_id: alpha,
        from_side: Side::Right,
        to_id: beta,
        to_side: Side::Left,
        dir: ArrowDecorations::Forward,
    });
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

    let mut app = AppState::new(Viewport::new(SPoint::new(0, 0)), Mode::Normal);
    bootstrap_small_demo_graph(&mut app);
    // bootstrap_demo_graph(&mut app);
    // Select the first node (Alpha) by default
    if let Some(first) = app.nodes.first() {
        app.mode = Mode::SelectedBlock(first.id, BlockMode::Selected);
    }

    let cwd = std::env::current_dir().unwrap_or_default();

    let mut last_tick = Instant::now();
    let mut menu_keys_sequence: Option<Vec<KeyBinding>> = None;
    let mut save_state = SaveState::new();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32().min(0.1);
        last_tick = now;
        app.vp.tick(dt);

        let mode_bindings = bindings_for_mode(&app.mode);

        let (bindings, hints_header): (&[Binding], &str) = if let Some(typed_keys) =
            &menu_keys_sequence
            && let Some((menu_items, menu_name)) = resolve_menu(&mode_bindings, typed_keys)
        {
            (menu_items, menu_name)
        } else {
            (mode_bindings.as_slice(), mode_name(&app.mode))
        };

        let is_dirty = save_state.is_dirty();
        terminal.draw(|frame| {
            ui::render_app(
                frame,
                &app.nodes,
                &app.edges,
                &app.vp,
                &app.mode,
                &bindings,
                hints_header,
                &key_log,
                app.working_file.as_deref(),
                is_dirty,
                &cwd,
            );
        })?;

        // Use a short timeout while an animation is running so the spring /
        // tween renders smoothly (~60 fps).  Fall back to 50 ms when idle to
        // avoid busy-looping when nothing is happening.
        let poll_ms = if app.vp.is_animating() { 33 } else { 60 };

        if crossterm::event::poll(std::time::Duration::from_millis(poll_ms))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                key_log.insert(0, format_key(key.code, key.modifiers));

                let term_size = terminal.size()?;
                let canvas_size = Size {
                    width: term_size.width,
                    height: term_size.height - 1,
                };

                // Walk bindings in order; the first match wins.
                let mut matched_actions: Option<smallvec::SmallVec<[crate::actions::Action; 1]>> =
                    None;
                let mut clear_menu = true;

                for b in bindings.iter() {
                    match b {
                        Binding::Single(inst) => {
                            if inst.key.matches(key.code, key.modifiers) {
                                matched_actions = Some(inst.actions.clone());
                                break;
                            }
                        }
                        Binding::Group {
                            bindings: members, ..
                        } => {
                            if let Some(inst) = members
                                .iter()
                                .find(|inst| inst.key.matches(key.code, key.modifiers))
                            {
                                matched_actions = Some(inst.actions.clone());
                                break;
                            }
                        }
                        Binding::Menu { key: menu_key, .. } => {
                            if menu_key.matches(key.code, key.modifiers) {
                                clear_menu = false;
                                // println!("hitting menu {name}");
                                if let Some(pressed) = &mut menu_keys_sequence {
                                    pressed.push(menu_key.clone());
                                } else {
                                    menu_keys_sequence = Some(vec![menu_key.clone()]);
                                }
                                break;
                            }
                        }
                        Binding::Listen(listener) => {
                            if let Some(a) = (listener.handler)(key) {
                                matched_actions = Some(smallvec::smallvec![a]);
                                break;
                            }
                        }
                    }
                }

                if clear_menu {
                    menu_keys_sequence = None;
                }

                if let Some(actions) = matched_actions {
                    let mut quit = false;
                    let mut queue = VecDeque::from_iter(actions);
                    while let Some(next) = queue.pop_front() {
                        save_state.tick();
                        match update(&mut app, next, canvas_size) {
                            UpdateResult::Quit => {
                                quit = true;
                                break;
                            }
                            UpdateResult::Continue => {}
                            UpdateResult::Actions(follow_up) => queue.extend(follow_up),
                            UpdateResult::Effect(effect) => match effect {
                                update::Effect::SaveSceneTo(ref path) => {
                                    let snapshot =
                                        scene_save::to_scene_save(&app.nodes, &app.edges, &app.vp);
                                    if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
                                        // Create parent directories if needed.
                                        if let Some(parent) = path.parent() {
                                            let _ = std::fs::create_dir_all(parent);
                                        }
                                        if std::fs::write(path, json).is_ok() {
                                            app.working_file = Some(path.clone());
                                            save_state.mark_saved();
                                        }
                                    }
                                }
                                update::Effect::LoadScene => {
                                    if let Ok(json) = std::fs::read_to_string("scene.kontur") {
                                        if let Ok(snapshot) =
                                            serde_json::from_str::<scene_save::SceneSave>(&json)
                                        {
                                            let (vp, nodes, edges) =
                                                scene_save::from_scene_save(snapshot);
                                            app = AppState::from_parts(nodes, edges, vp);
                                            app.working_file =
                                                Some(PathBuf::from("scene.kontur"));
                                            save_state.mark_saved();
                                        }
                                    }
                                }
                                update::Effect::CopyToClipboard(text) => {
                                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                        let _ = clipboard.set_text(text);
                                    }
                                }
                            },
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

/// Walk `bindings` following `prefix[0]` into a `Binding::Menu`, then recurse
/// with the remaining prefix keys.  Returns the resolved sub-menu items and the
/// name of the deepest matched menu when the full prefix is consumed, or `None`
/// if any step fails to find a matching `Menu` entry.
fn resolve_menu<'b>(
    bindings: &'b [Binding],
    prefix: &[KeyBinding],
) -> Option<(&'b [Binding], &'static str)> {
    let pressed_key = prefix.first()?;
    bindings.iter().find_map(|item| match item {
        Binding::Menu { key, name, items } if key == pressed_key => {
            if prefix.len() == 1 {
                Some((items.as_slice(), *name))
            } else {
                resolve_menu(items, &prefix[1..])
            }
        }
        _ => None,
    })
}

/// Return a short lowercase human-readable name for the current mode.
fn mode_name(mode: &Mode) -> &'static str {
    use state::{BlockMode, EdgeMode};
    match mode {
        Mode::Normal => "normal",
        Mode::SaveModal { .. } => "save",
        Mode::SelectedBlock(_, BlockMode::Selected) => "block",
        Mode::SelectedBlock(_, BlockMode::CreatingRelativeNode) => "new node",
        Mode::SelectedBlock(_, BlockMode::Resizing) => "resize",
        Mode::SelectedBlock(_, BlockMode::Editing { .. }) => "edit",
        Mode::SelectedBlock(_, BlockMode::ConnectingEdge { .. }) => "connect",
        Mode::SelectedBlock(_, BlockMode::PropEditing { .. }) => "props",
        Mode::SelectedEdge(_, EdgeMode::Selected) => "edge",
        Mode::SelectedEdge(_, EdgeMode::TweakEndpoint) => "tweak endpoint",
        Mode::SelectedEdge(_, EdgeMode::TweakSide { .. }) => "tweak side",
        Mode::SelectedEdge(_, EdgeMode::PropEditing { .. }) => "edge props",
        Mode::Selecting { .. } => "select",
        Mode::MultiSelecting { .. } => "multi-select",
        Mode::MultiSelected { .. } => "multi-selected",
        Mode::CopyAsModal { .. } => "copy as",
    }
}
