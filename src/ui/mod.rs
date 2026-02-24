use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table},
};

use crate::binding::Binding;
use crate::labels::LabelIter;
use crate::path::{self, PathError};
use crate::state::{BlockMode, Edge, Mode, Node, NodeId, Viewport};

// ── Label pools ───────────────────────────────────────────────────────────────

static SINGLE_CHARS: &[char] = &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
static DOUBLE_CHARS: &[char] = &['e', 'r', 'u', 'i', 'o'];

/// Assign labels to every node that is currently visible on screen.
pub fn assign_labels(
    nodes: &[Node],
    vp: &Viewport,
    frame_w: i32,
    frame_h: i32,
) -> Vec<(NodeId, String)> {
    let visible: Vec<NodeId> = nodes
        .iter()
        .filter(|n| {
            let (sx, sy) = to_screen(n.rect.origin.x, n.rect.origin.y, vp);
            clip_to_frame(
                sx,
                sy,
                n.rect.size.width as i32,
                n.rect.size.height as i32,
                frame_w,
                frame_h,
            )
            .is_some()
        })
        .map(|n| n.id)
        .collect();

    LabelIter::new(SINGLE_CHARS, DOUBLE_CHARS)
        .zip(visible)
        .map(|(label, id)| (id, label))
        .collect()
}

// ── Coordinate helpers ────────────────────────────────────────────────────────

fn to_screen(canvas_x: i32, canvas_y: i32, vp: &Viewport) -> (i32, i32) {
    (canvas_x - vp.center.x, canvas_y - vp.center.y)
}

fn in_frame(x: i32, y: i32, frame: &Frame) -> bool {
    let a = frame.area();
    x >= 0 && y >= 0 && x < a.width as i32 && y < a.height as i32
}

// ── Clipping ──────────────────────────────────────────────────────────────────

fn clip_to_frame(
    nx: i32,
    ny: i32,
    nw: i32,
    nh: i32,
    fw: i32,
    fh: i32,
) -> Option<(i32, i32, i32, i32)> {
    if nx >= fw || nx + nw <= 0 || ny >= fh || ny + nh <= 0 {
        return None;
    }
    let x1 = nx.max(0);
    let x2 = (nx + nw).min(fw);
    let y1 = ny.max(0);
    let y2 = (ny + nh).min(fh);
    Some((x1, y1, x2 - x1, y2 - y1))
}

// ── Node rendering ────────────────────────────────────────────────────────────

fn render_nodes(frame: &mut Frame, nodes: &[Node], vp: &Viewport, mode: &Mode) {
    let fw = frame.area().width as i32;
    let fh = frame.area().height as i32 - 1; // reserve last row for hint bar

    for node in nodes {
        let (screen_x, screen_y) = to_screen(node.rect.origin.x, node.rect.origin.y, vp);
        let nw = node.rect.size.width as i32;
        let nh = node.rect.size.height as i32;

        let (cx, cy, cw, ch) = match clip_to_frame(screen_x, screen_y, nw, nh, fw, fh) {
            Some(r) => r,
            None => continue,
        };

        if cw <= 0 || ch <= 0 {
            continue;
        }

        let area = Rect::new(cx as u16, cy as u16, cw as u16, ch as u16);

        let mut borders = Borders::NONE;
        if screen_x == cx {
            borders |= Borders::LEFT;
        }
        if screen_x + nw == cx + cw {
            borders |= Borders::RIGHT;
        }
        if screen_y == cy {
            borders |= Borders::TOP;
        }
        if screen_y + nh == cy + ch {
            borders |= Borders::BOTTOM;
        }

        let is_selected = matches!(mode, Mode::SelectedBlock(id, _) if *id == node.id);

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
fn render_connections(
    frame: &mut Frame,
    nodes: &[Node],
    edges: &[Edge],
    vp: &Viewport,
) -> Vec<String> {
    use crate::path::PathSymbol;

    let mut errors: Vec<String> = Vec::new();

    for edge in edges {
        match path::calculate_path(nodes, edge) {
            Ok((path_iter, _bounds)) => {
                for (pt, sym) in path_iter.take(100) {
                    let (sx, sy) = to_screen(pt.x, pt.y, vp);
                    if !in_frame(sx, sy, frame) {
                        continue;
                    }

                    // Arrowheads are drawn in yellow; line segments in white.
                    let color = match sym {
                        PathSymbol::ArrowRight
                        | PathSymbol::ArrowLeft
                        | PathSymbol::ArrowDown
                        | PathSymbol::ArrowUp => Color::Yellow,
                        _ => Color::White,
                    };

                    if let Some(cell) = frame.buffer_mut().cell_mut((sx as u16, sy as u16)) {
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

fn render_selection_labels(
    frame: &mut Frame,
    nodes: &[Node],
    vp: &Viewport,
    labels: &[(NodeId, String)],
    current: &str,
) {
    use ratatui::style::Modifier;

    let fw = frame.area().width as i32;
    let fh = frame.area().height as i32 - 1;

    for (id, label) in labels {
        if !label.starts_with(current) {
            continue;
        }

        let node = match nodes.iter().find(|n| n.id == *id) {
            Some(n) => n,
            None => continue,
        };

        let (sx, sy) = to_screen(node.rect.origin.x, node.rect.origin.y, vp);
        let (cx, cy, cw, ch) = match clip_to_frame(
            sx,
            sy,
            node.rect.size.width as i32,
            node.rect.size.height as i32,
            fw,
            fh,
        ) {
            Some(r) => r,
            None => continue,
        };

        if cw < 1 || ch < 1 {
            continue;
        }
        if cx != sx || cy != sy {
            continue;
        }

        let label_x = (sx + 1) as u16;
        let label_y = (sy + 1) as u16;

        if label_x >= frame.area().width || label_y >= frame.area().height {
            continue;
        }

        let matched_len = current.len();
        let matched = &label[..matched_len];
        let rest = &label[matched_len..];

        let matched_style = Style::default().fg(Color::DarkGray);
        let hint_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD);

        let spans = vec![
            Span::styled(matched, matched_style),
            Span::styled(rest, hint_style),
        ];

        let label_w = label.chars().count() as u16;
        let available_w = frame.area().width.saturating_sub(label_x);
        let render_w = label_w.min(available_w);
        if render_w == 0 {
            continue;
        }

        let area = Rect::new(label_x, label_y, render_w, 1);
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
fn hint_table_data(bindings: &[Binding]) -> Vec<(String, String)> {
    bindings
        .iter()
        .map(|b| match b {
            Binding::Single(inst) => {
                let key = key_label(&inst.key.key);
                let desc = inst.description.to_string();
                (key, desc)
            }
            Binding::Group {
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
            Binding::Listen(listener) => {
                let desc = listener.description.to_string();
                ("…".to_string(), desc)
            }
        })
        .collect()
}

/// Render the hints panel anchored to the bottom-right corner, framed and
/// aligned using a `Table` widget.
fn render_hints_panel(frame: &mut Frame, bindings: &[Binding]) {
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
    bindings: &[Binding],
    _key_log: &[String],
) {
    frame.render_widget(ratatui::widgets::Clear, frame.area());
    let path_errors = render_connections(frame, nodes, edges, vp);
    render_nodes(frame, nodes, vp, mode);
    render_error_bar(frame, &path_errors);
    if let Mode::Selecting {
        labels, current, ..
    } = mode
    {
        render_selection_labels(frame, nodes, vp, labels, current);
    }
    render_hints_panel(frame, bindings);
    // render_key_log(frame, key_log);
    if let Mode::SelectedBlock(_, BlockMode::Editing { input, cursor }) = mode {
        render_popup(frame, input, *cursor);
    }
}
