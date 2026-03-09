pub mod actions;
pub mod binding;
pub mod geometry;
pub mod labels;
pub mod path;
pub mod screen_space;
pub mod state;
pub mod ui;
pub mod update;
pub mod viewport;

use std::collections::VecDeque;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyModifiers};
use geometry::{SPoint, SRect};
use ratatui::layout::Size;
use state::{AppState, ArrowDecorations, BlockMode, Edge, Mode, Node, Side};
use update::{update, UpdateResult};
use viewport::Viewport;

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

    app.nodes.push(Node {
        id: alpha,
        rect: SRect::new(5, 5, 16, 5),
        label: "Alpha".to_string(),
    });
    app.nodes.push(Node {
        id: beta,
        rect: SRect::new(35, 5, 16, 5),
        label: "Beta".to_string(),
    });
    app.nodes.push(Node {
        id: gamma,
        rect: SRect::new(35, 18, 16, 5),
        label: "Gamma".to_string(),
    });
    app.nodes.push(Node {
        id: delta,
        rect: SRect::new(5, 18, 16, 5),
        label: "Delta".to_string(),
    });

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
    bootstrap_demo_graph(&mut app);
    // Select the first node (Alpha) by default
    if let Some(first) = app.nodes.first() {
        app.mode = Mode::SelectedBlock(first.id, BlockMode::Selected);
    }

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32().min(0.1);
        last_tick = now;
        app.vp.tick(dt);

        let bindings = binding::bindings_for_mode(&app.mode);

        terminal.draw(|frame| {
            ui::render_map(
                frame, &app.nodes, &app.edges, &app.vp, &app.mode, &bindings, &key_log,
            );
        })?;

        // Use a short timeout while an animation is running so the spring /
        // tween renders smoothly (~60 fps).  Fall back to 50 ms when idle to
        // avoid busy-looping when nothing is happening.
        let poll_ms = if app.vp.is_animating() { 16 } else { 50 };
        if crossterm::event::poll(std::time::Duration::from_millis(poll_ms))? {
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
