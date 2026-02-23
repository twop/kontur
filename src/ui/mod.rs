use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::labels::LabelIter;
use crate::path::{self};
use crate::state::{ArrowDecorations, BlockMode, Edge, Mode, Node, NodeId, Viewport};
use crate::{actions::Dir, geometry::SPoint};

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

fn render_connections(frame: &mut Frame, nodes: &[Node], edges: &[Edge], vp: &Viewport) {
    for edge in edges {
        let path = path::calculate_path(nodes, edge);

        let visible = path.iter().any(|p| {
            let (sx, sy) = to_screen(p.x, p.y, vp);
            in_frame(sx, sy, frame)
        });
        if !visible {
            continue;
        }

        let seg_dirs: Vec<Dir> = path
            .windows(2)
            .map(|pts| path::seg_dir(pts[0], pts[1]))
            .collect();

        for (i, pts) in path.windows(2).enumerate() {
            let dir = seg_dirs[i];
            let (x1, y1) = to_screen(pts[0].x, pts[0].y, vp);
            let (x2, y2) = to_screen(pts[1].x, pts[1].y, vp);

            match dir {
                Dir::Right | Dir::Left => {
                    let (start_x, end_x) = if x2 >= x1 { (x1, x2) } else { (x2, x1) };
                    for x in start_x..=end_x {
                        if in_frame(x, y1, frame) {
                            if let Some(cell) = frame.buffer_mut().cell_mut((x as u16, y1 as u16)) {
                                cell.set_symbol("─").set_fg(Color::White);
                            }
                        }
                    }
                }
                Dir::Down | Dir::Up => {
                    let (start_y, end_y) = if y2 >= y1 { (y1, y2) } else { (y2, y1) };
                    for y in start_y..=end_y {
                        if in_frame(x1, y, frame) {
                            if let Some(cell) = frame.buffer_mut().cell_mut((x1 as u16, y as u16)) {
                                cell.set_symbol("│").set_fg(Color::White);
                            }
                        }
                    }
                }
            }
        }

        for (i, pts) in path.windows(3).enumerate() {
            let (px, py) = to_screen(pts[1].x, pts[1].y, vp);
            let incoming = seg_dirs[i];
            let outgoing = seg_dirs[i + 1];

            let glyph = match (incoming, outgoing) {
                (Dir::Left, Dir::Down) | (Dir::Up, Dir::Right) => "┌",
                (Dir::Right, Dir::Down) | (Dir::Up, Dir::Left) => "┐",
                (Dir::Down, Dir::Right) | (Dir::Left, Dir::Up) => "└",
                (Dir::Down, Dir::Left) | (Dir::Right, Dir::Up) => "┘",
                (Dir::Left, Dir::Left) | (Dir::Right, Dir::Right) => "─",
                (Dir::Up, Dir::Up) | (Dir::Down, Dir::Down) => "│",
                _ => continue,
            };

            if in_frame(px, py, frame) {
                if let Some(cell) = frame.buffer_mut().cell_mut((px as u16, py as u16)) {
                    cell.set_symbol(glyph).set_fg(Color::White);
                }
            }
        }

        let draw_arrowhead = |frame: &mut Frame, pt: SPoint, dir: Dir| {
            let (sx, sy) = to_screen(pt.x, pt.y, vp);
            let ch = match dir {
                Dir::Right => "→",
                Dir::Left => "←",
                Dir::Down => "↓",
                Dir::Up => "↑",
            };
            if in_frame(sx, sy, frame) {
                if let Some(cell) = frame.buffer_mut().cell_mut((sx as u16, sy as u16)) {
                    cell.set_symbol(ch).set_fg(Color::Yellow);
                }
            }
        };

        let n = path.len();
        match edge.dir {
            ArrowDecorations::Forward => {
                if n >= 2 {
                    let dir = *seg_dirs.last().unwrap();
                    draw_arrowhead(frame, path[n - 2], dir);
                }
            }
            ArrowDecorations::Backward => {
                if n >= 2 {
                    let dir = match seg_dirs.first().unwrap() {
                        Dir::Right => Dir::Left,
                        Dir::Left => Dir::Right,
                        Dir::Down => Dir::Up,
                        Dir::Up => Dir::Down,
                    };
                    draw_arrowhead(frame, path[1], dir);
                }
            }
            ArrowDecorations::Both => {
                if n >= 2 {
                    let dir = *seg_dirs.last().unwrap();
                    draw_arrowhead(frame, path[n - 2], dir);
                    let dir = match seg_dirs.first().unwrap() {
                        Dir::Right => Dir::Left,
                        Dir::Left => Dir::Right,
                        Dir::Down => Dir::Up,
                        Dir::Up => Dir::Down,
                    };
                    draw_arrowhead(frame, path[1], dir);
                }
            }
        }
    }
}

// ── Selection label overlay ───────────────────────────────────────────────────

fn render_selection_labels(
    frame: &mut Frame,
    nodes: &[Node],
    vp: &Viewport,
    labels: &[(NodeId, String)],
    current: &str,
) {
    use ratatui::{
        style::Modifier,
        text::{Line, Span},
    };

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

// ── Hint bar ─────────────────────────────────────────────────────────────────

fn render_hint(frame: &mut Frame, mode: &Mode) {
    let area = frame.area();
    let hint_area = Rect::new(0, area.height.saturating_sub(1), area.width, 1);
    let text = match mode {
        Mode::Normal => "  [normal]  hjkl: pan   enter: select node   q: quit",
        Mode::SelectedBlock(_, BlockMode::Selected) => {
            "  [selected]   hjkl: move ×1   HJKL: move ×5   r: resize   i: edit   enter: select node   esc: deselect   q: quit"
        }
        Mode::SelectedBlock(_, BlockMode::Resizing) => {
            "  [resize]  hjkl: expand in direction   HJKL: shrink from direction   esc: back   q: quit"
        }
        Mode::SelectedBlock(_, BlockMode::Editing { .. }) => {
            "  [editing]  enter: confirm   esc: cancel"
        }
        Mode::Selecting { .. } => "  [select node]  type label to jump   esc: cancel",
    };
    let hint = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, hint_area);
}

// ── Top-level render ──────────────────────────────────────────────────────────

pub fn render_map(frame: &mut Frame, nodes: &[Node], edges: &[Edge], vp: &Viewport, mode: &Mode) {
    frame.render_widget(ratatui::widgets::Clear, frame.area());
    render_connections(frame, nodes, edges, vp);
    render_nodes(frame, nodes, vp, mode);
    if let Mode::Selecting {
        labels, current, ..
    } = mode
    {
        render_selection_labels(frame, nodes, vp, labels, current);
    }
    render_hint(frame, mode);
    if let Mode::SelectedBlock(_, BlockMode::Editing { input, cursor }) = mode {
        render_popup(frame, input, *cursor);
    }
}
