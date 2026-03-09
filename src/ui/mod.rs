use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table},
};

use crate::screen_space::Screen;
use crate::state::{BlockMode, Edge, EdgeId, Mode, Node, NodeId, Viewport};
use crate::{geometry::CanvasRect, labels::LabelIter};
use crate::{
    geometry::{SPoint, SRect},
    screen_space::ViewportRect,
};
use crate::{
    path::{self, PathError},
    screen_space::ViewportPoint,
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
pub fn assign_jump_labels(
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    frame: ViewportRect,
) -> (Vec<(NodeId, String)>, Vec<(EdgeId, String)>) {
    use crate::state::GraphId;

    let center = vp.desired_center;

    // ── Collect visible nodes ─────────────────────────────────────────────────
    let mut items: Vec<(i32, GraphId)> = nodes
        .iter()
        .filter(|n| Screen::rect(vp, n.rect).clip_by(frame).is_some())
        .map(|n| (chebyshev(n.rect.center(), center), GraphId::Node(n.id)))
        .collect();

    // ── Collect visible edges ─────────────────────────────────────────────────
    for edge in edges {
        let from_node = nodes.iter().find(|n| n.id == edge.from_id);
        let to_node = nodes.iter().find(|n| n.id == edge.to_id);
        if let (Some(from), Some(to)) = (from_node, to_node) {
            let from_pt = path::connection_point(from, edge.from_side);
            let to_pt = path::connection_point(to, edge.to_side);
            // Use the closer of the two connection points as the sort key.
            let dist = chebyshev(from_pt, center).min(chebyshev(to_pt, center));
            // Only include the edge if at least one connection point is on screen.
            let from_on = frame.contains(Screen::point(vp, from_pt));
            let to_on = frame.contains(Screen::point(vp, to_pt));
            if from_on || to_on {
                items.push((dist, GraphId::Edge(edge.id)));
            }
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

// ── Node rendering ────────────────────────────────────────────────────────────

fn render_nodes(frame: &mut Frame, nodes: &[Node], vp: &Viewport, mode: &Mode) {
    let frame_canvas_rect = CanvasRect::from_center(vp.looking_at(), frame.area().as_size());

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

        let area = Screen::to_ratatui_rect(Screen::rect(vp, clipped), frame_canvas_rect.size);

        frame.render_widget(Clear, area);

        let block = if is_selected {
            Block::default()
                .borders(borders)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow))
                .title(node.label.as_str())
        } else {
            Block::default().borders(borders).title(node.label.as_str())
        };

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width > 0 && inner.height > 0 {
            let para = Paragraph::new(node.label.as_str()).alignment(Alignment::Center);
            frame.render_widget(para, inner);
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

    let selected_edge_id = if let Mode::SelectedEdge(id) = mode {
        Some(*id)
    } else {
        None
    };

    let mut errors: Vec<String> = Vec::new();
    let frame_rect = SRect::from_center(vp.looking_at(), frame.area().as_size());

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
    let viewport_rect = CanvasRect::from_center(vp.looking_at(), frame.area().as_size());
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

// ── Edit popup ────────────────────────────────────────────────────────────────

fn popup_area(area: Rect, percent_x: u16, rows: u16) -> Rect {
    use ratatui::layout::{Constraint, Flex, Layout};
    let vertical = Layout::vertical([Constraint::Length(rows)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

fn render_popup(frame: &mut Frame, input: &str, cursor: usize) {
    use ratatui::layout::Position;

    let area = popup_area(frame.area(), 50, 3);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Edit label ");

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(input), inner);

    #[allow(clippy::cast_possible_truncation)]
    frame.set_cursor_position(Position::new(inner.x + cursor as u16, inner.y));
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
                let key = key_label(&inst.key.key);
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
        })
        .collect()
}

/// Render the hints panel anchored to the bottom-right corner, framed and
/// aligned using a `Table` widget.
fn render_hints_panel(frame: &mut Frame, bindings: &[crate::binding::Binding]) {
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

    let block = Block::default()
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

pub fn render_map(
    frame: &mut Frame,
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
    mode: &Mode,
    bindings: &[crate::binding::Binding],
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
    render_hints_panel(frame, bindings);
    // render_key_log(frame, _key_log);
    if let Mode::SelectedBlock(_, BlockMode::Editing { input, cursor }) = mode {
        render_popup(frame, input, *cursor);
    }
}
