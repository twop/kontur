pub mod props;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Offset, Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table},
    Frame,
};

use crate::geometry::{CanvasRect, SPoint, SRect};
use crate::path::{self, PathError};
use crate::screen_space::Screen;
use crate::state::{
    BlockMode, CornerStyle, Edge, EdgeId, EdgeMode, Mode, Node, NodeId, Side, TextAlignH, Viewport,
};

// ── Editing node ID helper ────────────────────────────────────────────────────

/// If the mode is `SelectedBlock(id, Editing { .. })`, return the node id and a
/// reference to the textarea.  Used in `render_nodes` to decide whether to
/// render the inline editor instead of a paragraph.
fn editing_state(mode: &Mode) -> Option<(NodeId, &ratatui_textarea::TextArea<'static>)> {
    if let Mode::SelectedBlock(id, BlockMode::Editing { textarea, .. }) = mode {
        Some((*id, textarea))
    } else {
        None
    }
}

// ── Node rendering ────────────────────────────────────────────────────────────

fn render_nodes(frame: &mut Frame, nodes: &[Node], vp: &Viewport, mode: &Mode) {
    let frame_canvas_rect = CanvasRect::from_center(vp.animated_center(), frame.area().as_size());
    let editing = editing_state(mode);

    for node in nodes {
        let node_rect = node.rect;
        let clipped = match node_rect.clip_by(frame_canvas_rect) {
            Some(r) => r,
            None => continue,
        };

        if clipped.size.width == 0 || clipped.size.height == 0 {
            continue;
        }

        let mut borders = Borders::NONE;

        if node_rect.left() == clipped.left() {
            borders |= Borders::LEFT;
        }
        if node_rect.right() == clipped.right() {
            borders |= Borders::RIGHT;
        }
        if node_rect.top() == clipped.top() {
            borders |= Borders::TOP;
        }
        if node_rect.bottom() == clipped.bottom() {
            borders |= Borders::BOTTOM;
        }

        let is_selected = matches!(mode, Mode::SelectedBlock(id, _) if *id == node.id);
        let is_editing = editing.as_ref().is_some_and(|(id, _)| *id == node.id);

        let area = Screen::to_ratatui_rect(Screen::rect(vp, clipped), frame_canvas_rect.size);

        frame.render_widget(Clear, area);

        // Normal (unselected) border type reflects the node's corner_style property.
        let normal_border_type = match node.props.corner_style {
            CornerStyle::Sharp => BorderType::Plain,
            CornerStyle::Rounded => BorderType::Rounded,
        };

        // While editing: yellow double border, no title (cursor is in the textarea).
        // While selected: yellow double border with label as title.
        // Normal: plain/rounded border based on corner_style.
        let block = if is_editing {
            Block::default()
                .borders(borders)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow))
        } else if is_selected {
            Block::default()
                .borders(borders)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow))
        } else {
            Block::default()
                .borders(borders)
                .border_type(normal_border_type)
        }
        .padding(node.padding.to_ratatui());

        let content_area = block.inner(area);
        frame.render_widget(block, area);

        if content_area.width > 0 && content_area.height > 0 {
            if is_editing {
                // Render the TextArea widget inline — it owns cursor rendering.
                if let Some((_, textarea)) = &editing {
                    // note that the weird offset thing is for accomodating the cursor
                    frame.render_widget(
                        *textarea,
                        content_area.union(content_area.offset(Offset::new(1, 0))),
                    );
                }
            } else {
                if content_area.width > 0 && content_area.height > 0 {
                    use ratatui::text::Text;
                    let text = Text::from_iter(node.lines.iter().map(|l| Line::from(l.as_str())));
                    let text_align = match node.props.text_align_h {
                        TextAlignH::Left => Alignment::Left,
                        TextAlignH::Center => Alignment::Center,
                        TextAlignH::Right => Alignment::Right,
                    };
                    let para = Paragraph::new(text).alignment(text_align);
                    frame.render_widget(para, content_area);
                }
            }
        }
    }
}

// ── Connection rendering ──────────────────────────────────────────────────────

/// Draw all edges and return one error string per unimplemented route so the
/// caller can surface them in the error bar.
///
/// The selected edge (if any is identified by `mode`) is rendered in yellow to
/// match the selection colour used for nodes.
fn render_connections(
    frame: &mut Frame,
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    mode: &Mode,
) -> Vec<String> {
    use crate::path::PathSymbol;

    let selected_edge_id = if let Mode::SelectedEdge(id, _) = mode {
        Some(*id)
    } else {
        None
    };

    let mut errors: Vec<String> = Vec::new();
    let frame_rect = SRect::from_center(vp.animated_center(), frame.area().as_size());

    for edge in edges {
        let is_selected = selected_edge_id == Some(edge.id);

        match path::calculate_path(nodes, edge) {
            Ok((path_iter, _bounds)) => {
                for (pt, sym) in path_iter.take(100) {
                    if !frame_rect.contains(pt) {
                        continue;
                    }

                    let color = if is_selected {
                        // Selected edge: everything in yellow (matches node
                        // selection colour).
                        Color::Yellow
                    } else {
                        // Normal edge: arrowheads yellow, line segments white.
                        match sym {
                            PathSymbol::ArrowRight
                            | PathSymbol::ArrowLeft
                            | PathSymbol::ArrowDown
                            | PathSymbol::ArrowUp => Color::Yellow,
                            _ => Color::White,
                        }
                    };

                    let ratatui_pos =
                        Screen::to_ratatui_point(Screen::point(vp, pt), frame.area().as_size());

                    if let Some(cell) = frame.buffer_mut().cell_mut(ratatui_pos) {
                        cell.set_symbol(sym.to_symbol()).set_fg(color);
                    }
                }
            }
            Err(PathError::NodeNotFound) => {
                // One or both nodes are missing — nothing to draw.
            }
            Err(PathError::NotImplemented(details)) => {
                errors.push(format!(
                    "unimplemented route: {:?} → {:?}",
                    details.from_side, details.to_side
                ));
            }
        }
    }

    errors
}

fn get_connection_center(nodes: &[Node], edge: &Edge) -> Option<SPoint> {
    let (path, _) = path::calculate_path(nodes, edge).ok()?;
    let mut second_iter = path.clone();
    let count = path.count();
    let (mid_point, _) = second_iter.nth(count / 2)?;
    Some(mid_point)
}

// ── Error bar ─────────────────────────────────────────────────────────────────

/// Render routing errors as a single line at the very top of the frame.
///
/// Multiple errors are joined with `  |  `.  The bar is only drawn when there
/// is at least one error.
fn render_error_bar(frame: &mut Frame, errors: &[String]) {
    if errors.is_empty() {
        return;
    }
    let fa = frame.area();
    let area = Rect::new(0, 0, fa.width, 1);
    let text = errors.join("  |  ");
    frame.render_widget(
        Paragraph::new(text.as_str()).style(Style::default().fg(Color::Red)),
        area,
    );
}

// ── Selection label overlay ───────────────────────────────────────────────────

/// Render a single jump label at the given screen position.
///
/// The matched prefix (already typed) is shown in DarkGray; the remaining
/// suffix is shown as bold black text on a cyan background.
fn render_jump_label(frame: &mut Frame, label: &str, current: &str, pos: Position) {
    use ratatui::style::Modifier;
    let Position { x, y } = pos;
    if x >= frame.area().width || y >= frame.area().height {
        return;
    }

    let matched_len = current.len();
    let matched = &label[..matched_len];
    let rest = &label[matched_len..];

    let matched_style = Style::default().fg(Color::DarkGray);
    let hint_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let spans = vec![
        Span::styled(matched, matched_style),
        Span::styled(rest, hint_style),
    ];

    let label_w = label.chars().count() as u16;
    let available_w = frame.area().width.saturating_sub(x);
    let render_w = label_w.min(available_w);
    if render_w == 0 {
        return;
    }

    let area = Rect::new(x, y, render_w, 1);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_selection_labels(
    frame: &mut Frame,
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    node_labels: &[(NodeId, String)],
    edge_labels: &[(EdgeId, String)],
    current: &str,
) {
    let viewport_rect = CanvasRect::from_center(vp.animated_center(), frame.area().as_size());
    // ── Node labels ───────────────────────────────────────────────────────────
    for (id, label) in node_labels {
        if !label.starts_with(current) {
            continue;
        }

        let node = match nodes.iter().find(|n| n.id == *id) {
            Some(n) if viewport_rect.contains(n.rect.center()) => n,
            _ => continue,
        };

        render_jump_label(
            frame,
            label,
            current,
            Screen::to_ratatui_point(Screen::point(vp, node.rect.center()), viewport_rect.size),
        );
    }

    // ── Edge labels ───────────────────────────────────────────────────────────
    for (id, label) in edge_labels {
        if !label.starts_with(current) {
            continue;
        }

        let edge = match edges.iter().find(|e| e.id == *id) {
            Some(e) => e,
            None => continue,
        };

        let center = match get_connection_center(nodes, edge) {
            Some(center) if viewport_rect.contains(center) => center,
            _ => continue,
        };

        render_jump_label(
            frame,
            label,
            current,
            Screen::to_ratatui_point(Screen::point(vp, center), viewport_rect.size),
        );
    }
}

// ── Connect-edge label overlay ────────────────────────────────────────────────

/// Render jump labels for all target nodes while in `BlockMode::ConnectingEdge`.
///
/// The source node (`source_id`) is skipped; all other visible nodes that still
/// match the typed `current` prefix are labelled using the shared
/// `render_jump_label` primitive.
fn render_connect_labels(
    frame: &mut Frame,
    nodes: &[Node],
    vp: &Viewport,
    source_id: NodeId,
    node_labels: &[(NodeId, String)],
    current: &str,
) {
    let viewport_rect = CanvasRect::from_center(vp.animated_center(), frame.area().as_size());

    for (id, label) in node_labels {
        if *id == source_id {
            continue;
        }
        if !label.starts_with(current) {
            continue;
        }

        let node = match nodes.iter().find(|n| n.id == *id) {
            Some(n) if viewport_rect.contains(n.rect.center()) => n,
            _ => continue,
        };

        render_jump_label(
            frame,
            label,
            current,
            Screen::to_ratatui_point(Screen::point(vp, node.rect.center()), viewport_rect.size),
        );
    }
}

// ── Edge tweak label overlays ─────────────────────────────────────────────────

/// Render 's' / 'e' labels at the geometrically ordered connection points of
/// the selected edge while in `EdgeMode::TweakEndpoint`.
///
/// 's' is placed at the left-most (or top-most) connection point; 'e' at the
/// other.  Uses `path::edge_endpoints_ordered` for the ordering so update and
/// render agree on which endpoint is which.
fn render_tweak_endpoint_labels(
    frame: &mut Frame,
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    edge_id: EdgeId,
) {
    let edge = match edges.iter().find(|e| e.id == edge_id) {
        Some(e) => e,
        None => return,
    };
    let ((left_node_id, left_side), (right_node_id, right_side)) =
        match crate::path::edge_endpoints_ordered(nodes, edge) {
            Some(pair) => pair,
            None => return,
        };

    let viewport_rect = CanvasRect::from_center(vp.animated_center(), frame.area().as_size());

    let left_node = nodes.iter().find(|n| n.id == left_node_id);
    let right_node = nodes.iter().find(|n| n.id == right_node_id);

    if let Some(node) = left_node {
        let pt = crate::path::connection_point(node, left_side);
        if viewport_rect.contains(pt) {
            render_jump_label(
                frame,
                "s",
                "",
                Screen::to_ratatui_point(Screen::point(vp, pt), viewport_rect.size),
            );
        }
    }
    if let Some(node) = right_node {
        let pt = crate::path::connection_point(node, right_side);
        if viewport_rect.contains(pt) {
            render_jump_label(
                frame,
                "e",
                "",
                Screen::to_ratatui_point(Screen::point(vp, pt), viewport_rect.size),
            );
        }
    }
}

/// Render h/j/k/l labels at the four connection points of the node whose side
/// is being chosen, while in `EdgeMode::TweakSide`.
fn render_tweak_side_labels(frame: &mut Frame, nodes: &[Node], vp: &Viewport, node_id: NodeId) {
    let node = match nodes.iter().find(|n| n.id == node_id) {
        Some(n) => n,
        None => return,
    };

    let viewport_rect = CanvasRect::from_center(vp.animated_center(), frame.area().as_size());

    let side_labels: &[(&str, Side)] = &[
        ("h", Side::Left),
        ("j", Side::Bottom),
        ("k", Side::Top),
        ("l", Side::Right),
    ];

    for (label, side) in side_labels {
        let pt = crate::path::connection_point(node, *side);
        if viewport_rect.contains(pt) {
            render_jump_label(
                frame,
                label,
                "",
                Screen::to_ratatui_point(Screen::point(vp, pt), viewport_rect.size),
            );
        }
    }
}

// ── Hints panel ───────────────────────────────────────────────────────────────

/// Format a `KeyCode` as a short human-readable string.
fn key_label(code: &KeyCode) -> String {
    match code {
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
    }
}

/// Format a full key binding (modifiers + key code) as a human-readable string.
///
/// The `ALT`/`META` modifier is rendered as the nerd-font option symbol `⌥`.
/// The `SHIFT` modifier is omitted when it is already implied by an uppercase
/// character key (e.g. `SHIFT+h` is shown as `H`).
fn binding_label(key: &crate::binding::KeyBinding) -> String {
    use crossterm::event::KeyModifiers;

    let base = key_label(&key.key);

    // SHIFT implied by an uppercase char — don't repeat it.
    let show_shift = key.modifiers.contains(KeyModifiers::SHIFT)
        && !matches!(key.key, KeyCode::Char(c) if c.is_uppercase());

    let mut prefix = String::new();
    if key.modifiers.contains(KeyModifiers::ALT) {
        prefix.push_str("⌥+");
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        prefix.push_str("ctrl+");
    }
    if show_shift {
        prefix.push_str("shift+");
    }

    format!("{}{}", prefix, base)
}

/// Build `(key_string, description)` pairs for a set of bindings.
///
/// Each `Binding::Single` becomes one row with its key and description.
/// Each `Binding::Group` becomes one row with all member keys joined and the group name.
/// Each `Binding::Listen` becomes one row with `…` as the key and the description.
fn hint_table_data(bindings: &[crate::binding::Binding]) -> Vec<(String, String)> {
    bindings
        .iter()
        .map(|b| match b {
            crate::binding::Binding::Single(inst) => {
                let key = binding_label(&inst.key);
                let desc = inst.description.to_string();
                (key, desc)
            }
            crate::binding::Binding::Group {
                name,
                bindings: members,
            } => {
                let keys: String = members
                    .iter()
                    .map(|i| key_label(&i.key.key))
                    .collect::<Vec<_>>()
                    .join("");
                (keys, name.clone())
            }
            crate::binding::Binding::Listen(listener) => {
                let desc = listener.description.to_string();
                ("…".to_string(), desc)
            }
            crate::binding::Binding::Menu { key, name, .. } => {
                let k = binding_label(key);
                (k, format!("{} >", name))
            }
        })
        .collect()
}

/// Render the hints panel anchored to the bottom-right corner, framed and
/// aligned using a `Table` widget.
///
/// `header` is displayed as the block title (e.g. the menu name or the current
/// mode name).
fn render_hints_panel(frame: &mut Frame, bindings: &[crate::binding::Binding], header: &str) {
    let data = hint_table_data(bindings);
    if data.is_empty() {
        return;
    }

    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default().fg(Color::Gray);

    // Compute column widths from actual content.
    let key_col_w = data
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(1) as u16;
    let desc_col_w = data
        .iter()
        .map(|(_, d)| d.chars().count())
        .max()
        .unwrap_or(1) as u16;

    // Build table rows.
    let rows: Vec<Row> = data
        .into_iter()
        .map(|(key, desc)| {
            Row::new(vec![
                Cell::from(key).style(key_style),
                Cell::from(desc).style(desc_style),
            ])
        })
        .collect();

    let row_count = rows.len() as u16;

    let fa = frame.area();

    // panel_w = border(1) + key_col + gap(1) + desc_col + border(1)
    let panel_w = (1 + key_col_w + 1 + desc_col_w + 1).min(fa.width);
    // panel_h = border(1) + rows + border(1)
    let panel_h = (row_count + 2).min(fa.height);

    // Anchor: bottom-right.
    let x = fa.width.saturating_sub(panel_w);
    let y = fa.height.saturating_sub(panel_h);

    let area = Rect::new(x, y, panel_w, panel_h);

    let title = format!(" {} ", header);
    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let table = Table::new(
        rows,
        [
            Constraint::Length(key_col_w),
            Constraint::Length(desc_col_w),
        ],
    )
    .block(block)
    .column_spacing(1);

    frame.render_widget(Clear, area);
    frame.render_widget(table, area);
}

// ── Generic debug panel ───────────────────────────────────────────────────────

/// Render a titled, bordered panel anchored to the **bottom-left** corner of the
/// frame.
///
/// `content` is a pre-built [`Paragraph`] (without a block attached).
/// `content_w` / `content_h` are the inner dimensions of the content — the
/// panel will add 2 cells on each axis for the border.
fn render_debug_panel(
    frame: &mut Frame,
    title: &str,
    content: Paragraph,
    content_w: u16,
    content_h: u16,
) {
    let fa = frame.area();
    let panel_w = (content_w + 2).min(fa.width);
    let panel_h = (content_h + 2).min(fa.height);
    let x = 0;
    let y = fa.height.saturating_sub(panel_h);
    let area = Rect::new(x, y, panel_w, panel_h);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(Clear, area);
    frame.render_widget(content.block(block), area);
}

// ── Edge shape debug panel ────────────────────────────────────────────────────

/// When an edge is selected (`EdgeMode::Selected`), render a top-right debug
/// panel showing the [`ConnectorShape`] classification as a pretty-printed Rust
/// value (`{:#?}`).
fn render_edge_shape_panel(frame: &mut Frame, nodes: &[Node], edges: &[Edge], mode: &Mode) {
    let Mode::SelectedEdge(edge_id, crate::state::EdgeMode::Selected) = mode else {
        return;
    };
    let Some(edge) = edges.iter().find(|e| e.id == *edge_id) else {
        return;
    };
    let Some((shape, ordered_endpoints)) = crate::path::classify_path(nodes, edge) else {
        return;
    };

    let (start, runs) = shape.clone().into_runs();
    let runs_lines: String = runs
        .iter()
        .map(|(dir, steps)| format!("  {:?}", (dir, steps)))
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!(
        "{shape:#?}\n\nstart: {start:?}\nruns:\n{runs_lines}\nrelations:{:?}",
        ordered_endpoints.relation
    );
    let content_w = text.lines().map(|l| l.len()).max().unwrap_or(0) as u16;
    let content_h = text.lines().count() as u16;

    let para = Paragraph::new(text).style(Style::default().fg(Color::Gray));
    render_debug_panel(frame, " shape ", para, content_w, content_h);
}

// ── Key log bar ───────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn render_key_log(frame: &mut Frame, key_log: &[String]) {
    let fa = frame.area();
    let area = Rect::new(0, fa.height.saturating_sub(1), fa.width, 1);
    let text = key_log.join("  ");
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

// ── Top-level render ──────────────────────────────────────────────────────────

pub fn render_app(
    frame: &mut Frame,
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    mode: &Mode,
    bindings: &[crate::binding::Binding],
    hints_header: &str,
    _key_log: &[String],
) {
    let fa = frame.area();
    frame.render_widget(ratatui::widgets::Clear, fa);
    let path_errors = render_connections(frame, nodes, edges, vp, mode);
    render_nodes(frame, nodes, vp, mode);
    render_error_bar(frame, &path_errors);
    if let Mode::Selecting {
        node_labels,
        edge_labels,
        current,
        ..
    } = mode
    {
        render_selection_labels(frame, nodes, edges, vp, node_labels, edge_labels, current);
    }
    if let Mode::SelectedBlock(
        source_id,
        BlockMode::ConnectingEdge {
            node_labels,
            current,
        },
    ) = mode
    {
        render_connect_labels(frame, nodes, vp, *source_id, node_labels, current);
    }
    if let Mode::SelectedEdge(edge_id, EdgeMode::TweakEndpoint) = mode {
        render_tweak_endpoint_labels(frame, nodes, edges, vp, *edge_id);
    }
    if let Mode::SelectedEdge(_, EdgeMode::TweakSide { node_id }) = mode {
        render_tweak_side_labels(frame, nodes, vp, *node_id);
    }
    render_hints_panel(frame, bindings, hints_header);
    render_edge_shape_panel(frame, nodes, edges, mode);
    if let Mode::SelectedBlock(_, BlockMode::PropEditing { panel }) = mode {
        props::render_props_panel(frame, panel);
    }
    // render_key_log(frame, _key_log);
}
