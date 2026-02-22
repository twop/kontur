use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crossterm::event::KeyCode;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
struct NodeId(usize);

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum ArrowDir {
    Forward,  // arrowhead at destination only
    Backward, // arrowhead at source only
    Both,     // arrowheads at both ends
}

#[allow(dead_code)]
struct Node {
    id: NodeId,
    x: isize,    // canvas top-left column
    y: isize,    // canvas top-left row
    width: u16,  // total box width including borders
    height: u16, // total box height including borders
    label: String,
}

struct Edge {
    from_id: NodeId,
    from_side: Side,
    to_id: NodeId,
    to_side: Side,
    dir: ArrowDir,
}

#[derive(Clone, Copy)]
struct Point {
    x: isize,
    y: isize,
}

#[derive(Clone, Copy, PartialEq)]
enum SegDir {
    Right,
    Left,
    Up,
    Down,
}

struct Viewport {
    x: isize,
    y: isize,
}

// ── Application mode ──────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum BlockMode {
    Selected,
    Editing { input: String, cursor: usize },
}

#[derive(Clone, PartialEq)]
enum Mode {
    Normal,
    SelectedBlock(NodeId, BlockMode),
}

// ── Coordinate helpers ───────────────────────────────────────────────────────

fn to_screen(canvas_x: isize, canvas_y: isize, vp: &Viewport) -> (isize, isize) {
    (canvas_x - vp.x, canvas_y - vp.y)
}

fn in_frame(x: isize, y: isize, frame: &Frame) -> bool {
    let a = frame.area();
    x >= 0 && y >= 0 && x < a.width as isize && y < a.height as isize
}

// ── Clipping ─────────────────────────────────────────────────────────────────

/// Returns Some((clip_x, clip_y, clip_w, clip_h)) or None
fn clip_to_frame(
    nx: isize,
    ny: isize,
    nw: isize,
    nh: isize,
    fw: isize,
    fh: isize,
) -> Option<(isize, isize, isize, isize)> {
    if nx >= fw || nx + nw <= 0 || ny >= fh || ny + nh <= 0 {
        return None;
    }
    let x1 = nx.max(0);
    let x2 = (nx + nw).min(fw);
    let y1 = ny.max(0);
    let y2 = (ny + nh).min(fh);
    Some((x1, y1, x2 - x1, y2 - y1))
}

// ── Demo graph ───────────────────────────────────────────────────────────────

fn make_demo_graph() -> (Vec<Node>, Vec<Edge>) {
    let alpha = NodeId(0);
    let beta = NodeId(1);
    let gamma = NodeId(2);
    let delta = NodeId(3);

    let nodes = [
        Node {
            id: alpha,
            x: 5,
            y: 5,
            width: 16,
            height: 5,
            label: "Alpha".to_string(),
        },
        Node {
            id: beta,
            x: 35,
            y: 5,
            width: 16,
            height: 5,
            label: "Beta".to_string(),
        },
        Node {
            id: gamma,
            x: 35,
            y: 18,
            width: 16,
            height: 5,
            label: "Gamma".to_string(),
        },
        Node {
            id: delta,
            x: 5,
            y: 18,
            width: 16,
            height: 5,
            label: "Delta".to_string(),
        },
    ];

    let edges = vec![
        // Alpha.Right → Beta.Left   — horizontal S-shape
        Edge {
            from_id: alpha,
            from_side: Side::Right,
            to_id: beta,
            to_side: Side::Left,
            dir: ArrowDir::Forward,
        },
        // Beta.Bottom → Gamma.Top   — vertical drop, corner
        Edge {
            from_id: beta,
            from_side: Side::Bottom,
            to_id: gamma,
            to_side: Side::Top,
            dir: ArrowDir::Forward,
        },
        // Gamma.Left  → Delta.Right — reverse horizontal, S-shape
        Edge {
            from_id: gamma,
            from_side: Side::Left,
            to_id: delta,
            to_side: Side::Right,
            dir: ArrowDir::Both,
        },
        Edge {
            from_id: delta,
            from_side: Side::Top,
            to_id: beta,
            to_side: Side::Bottom,
            dir: ArrowDir::Backward,
        },
    ];

    (Vec::from(nodes), edges)
}

// ── Path routing ─────────────────────────────────────────────────────────────

fn connection_point(node: &Node, side: Side) -> Point {
    match side {
        Side::Right => Point {
            x: node.x + node.width as isize - 1,
            y: node.y + node.height as isize / 2,
        },
        Side::Left => Point {
            x: node.x,
            y: node.y + node.height as isize / 2,
        },
        Side::Top => Point {
            x: node.x + node.width as isize / 2,
            y: node.y,
        },
        Side::Bottom => Point {
            x: node.x + node.width as isize / 2,
            y: node.y + node.height as isize - 1,
        },
    }
}

fn offset_point(p: Point, side: Side) -> Point {
    match side {
        Side::Right => Point { x: p.x + 2, y: p.y },
        Side::Left => Point { x: p.x - 2, y: p.y },
        Side::Top => Point { x: p.x, y: p.y - 2 },
        Side::Bottom => Point { x: p.x, y: p.y + 2 },
    }
}

fn s_shape(start: Point, s_off: Point, end: Point, e_off: Point) -> Vec<Point> {
    let mid_x = s_off.x + (e_off.x - s_off.x) / 2;
    vec![
        start,
        s_off,
        Point {
            x: mid_x,
            y: s_off.y,
        },
        Point {
            x: mid_x,
            y: e_off.y,
        },
        e_off,
        end,
    ]
}

fn c_shape(start: Point, s_off: Point, end: Point, e_off: Point, from_side: Side) -> Vec<Point> {
    match from_side {
        Side::Right | Side::Left => {
            let far_x = if from_side == Side::Right {
                s_off.x.max(e_off.x)
            } else {
                s_off.x.min(e_off.x)
            };
            vec![
                start,
                s_off,
                Point {
                    x: far_x,
                    y: s_off.y,
                },
                Point {
                    x: far_x,
                    y: e_off.y,
                },
                e_off,
                end,
            ]
        }
        Side::Top | Side::Bottom => {
            let far_y = if from_side == Side::Bottom {
                s_off.y.max(e_off.y)
            } else {
                s_off.y.min(e_off.y)
            };
            vec![
                start,
                s_off,
                Point {
                    x: s_off.x,
                    y: far_y,
                },
                Point {
                    x: e_off.x,
                    y: far_y,
                },
                e_off,
                end,
            ]
        }
    }
}

fn corner(start: Point, s_off: Point, end: Point, e_off: Point) -> Vec<Point> {
    vec![
        start,
        s_off,
        Point {
            x: e_off.x,
            y: s_off.y,
        },
        e_off,
        end,
    ]
}

fn seg_dir(from: Point, to: Point) -> SegDir {
    if to.x > from.x {
        SegDir::Right
    } else if to.x < from.x {
        SegDir::Left
    } else if to.y > from.y {
        SegDir::Down
    } else {
        SegDir::Up
    }
}

fn calculate_path(nodes: &[Node], edge: &Edge) -> Vec<Point> {
    let from_node = nodes.iter().find(|n| n.id == edge.from_id).unwrap();
    let to_node = nodes.iter().find(|n| n.id == edge.to_id).unwrap();

    let start = connection_point(from_node, edge.from_side);
    let end = connection_point(to_node, edge.to_side);
    let start_off = offset_point(start, edge.from_side);
    let end_off = offset_point(end, edge.to_side);

    let dx = end.x - start.x;

    if edge.from_side == edge.to_side {
        c_shape(start, start_off, end, end_off, edge.from_side)
    } else if (edge.from_side == Side::Right && edge.to_side == Side::Left)
        || (edge.from_side == Side::Left && edge.to_side == Side::Right)
    {
        if dx.abs() >= 6 {
            s_shape(start, start_off, end, end_off)
        } else {
            corner(start, start_off, end, end_off)
        }
    } else {
        corner(start, start_off, end, end_off)
    }
}

// ── Node rendering ────────────────────────────────────────────────────────────

fn render_nodes(frame: &mut Frame, nodes: &[Node], vp: &Viewport, mode: &Mode) {
    let fw = frame.area().width as isize;
    let fh = frame.area().height as isize - 1; // reserve last row for hint bar

    for node in nodes {
        let (screen_x, screen_y) = to_screen(node.x, node.y, vp);
        let nw = node.width as isize;
        let nh = node.height as isize;

        let (cx, cy, cw, ch) = match clip_to_frame(screen_x, screen_y, nw, nh, fw, fh) {
            Some(r) => r,
            None => continue,
        };

        if cw <= 0 || ch <= 0 {
            continue;
        }

        let area = Rect::new(cx as u16, cy as u16, cw as u16, ch as u16);

        // Determine which border sides are visible (not clipped off)
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
        let path = calculate_path(nodes, edge);

        // Visibility check: skip if no waypoint is on screen
        let visible = path.iter().any(|p| {
            let (sx, sy) = to_screen(p.x, p.y, vp);
            in_frame(sx, sy, frame)
        });
        if !visible {
            continue;
        }

        // Compute segment directions
        let seg_dirs: Vec<SegDir> = path.windows(2).map(|pts| seg_dir(pts[0], pts[1])).collect();

        // Draw line segments
        for (i, pts) in path.windows(2).enumerate() {
            let dir = seg_dirs[i];
            let (x1, y1) = to_screen(pts[0].x, pts[0].y, vp);
            let (x2, y2) = to_screen(pts[1].x, pts[1].y, vp);

            match dir {
                SegDir::Right | SegDir::Left => {
                    let (start_x, end_x) = if x2 >= x1 { (x1, x2) } else { (x2, x1) };
                    for x in start_x..=end_x {
                        if in_frame(x, y1, frame) {
                            if let Some(cell) = frame.buffer_mut().cell_mut((x as u16, y1 as u16)) {
                                cell.set_symbol("─").set_fg(Color::White);
                            }
                        }
                    }
                }
                SegDir::Down | SegDir::Up => {
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

        // Draw corners at bends
        for (i, pts) in path.windows(3).enumerate() {
            let (px, py) = to_screen(pts[1].x, pts[1].y, vp);
            let incoming = seg_dirs[i];
            let outgoing = seg_dirs[i + 1];

            let glyph = match (incoming, outgoing) {
                (SegDir::Left, SegDir::Down) | (SegDir::Up, SegDir::Right) => "┌",
                (SegDir::Right, SegDir::Down) | (SegDir::Up, SegDir::Left) => "┐",
                (SegDir::Down, SegDir::Right) | (SegDir::Left, SegDir::Up) => "└",
                (SegDir::Down, SegDir::Left) | (SegDir::Right, SegDir::Up) => "┘",
                (SegDir::Left, SegDir::Left) | (SegDir::Right, SegDir::Right) => "─",
                (SegDir::Up, SegDir::Up) | (SegDir::Down, SegDir::Down) => "│",
                _ => continue,
            };

            if in_frame(px, py, frame) {
                if let Some(cell) = frame.buffer_mut().cell_mut((px as u16, py as u16)) {
                    cell.set_symbol(glyph).set_fg(Color::White);
                }
            }
        }

        // Draw arrowheads
        // Place at the offset point (second-to-last / second), not on the node
        // border itself which gets overdrawn by node rendering.
        let draw_arrowhead = |frame: &mut Frame, pt: Point, dir: SegDir| {
            let (sx, sy) = to_screen(pt.x, pt.y, vp);
            let ch = match dir {
                SegDir::Right => "→",
                SegDir::Left => "←",
                SegDir::Down => "↓",
                SegDir::Up => "↑",
            };
            if in_frame(sx, sy, frame) {
                if let Some(cell) = frame.buffer_mut().cell_mut((sx as u16, sy as u16)) {
                    cell.set_symbol(ch).set_fg(Color::Yellow);
                }
            }
        };

        let n = path.len();
        match edge.dir {
            ArrowDir::Forward => {
                // arrowhead at end_off (index n-2), pointing in direction of last segment
                if n >= 2 {
                    let dir = *seg_dirs.last().unwrap();
                    draw_arrowhead(frame, path[n - 2], dir);
                }
            }
            ArrowDir::Backward => {
                // arrowhead at start_off (index 1), pointing back toward source
                if n >= 2 {
                    let dir = match seg_dirs.first().unwrap() {
                        SegDir::Right => SegDir::Left,
                        SegDir::Left => SegDir::Right,
                        SegDir::Down => SegDir::Up,
                        SegDir::Up => SegDir::Down,
                    };
                    draw_arrowhead(frame, path[1], dir);
                }
            }
            ArrowDir::Both => {
                if n >= 2 {
                    let dir = *seg_dirs.last().unwrap();
                    draw_arrowhead(frame, path[n - 2], dir);
                    let dir = match seg_dirs.first().unwrap() {
                        SegDir::Right => SegDir::Left,
                        SegDir::Left => SegDir::Right,
                        SegDir::Down => SegDir::Up,
                        SegDir::Up => SegDir::Down,
                    };
                    draw_arrowhead(frame, path[1], dir);
                }
            }
        }
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
    use ratatui::{layout::Position, widgets::Paragraph};

    let area = popup_area(frame.area(), 50, 3);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Edit label ");

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let para = Paragraph::new(input);
    frame.render_widget(para, inner);

    // place the terminal cursor inside the popup at the right column
    #[allow(clippy::cast_possible_truncation)]
    frame.set_cursor_position(Position::new(inner.x + cursor as u16, inner.y));
}

// ── Hint bar ─────────────────────────────────────────────────────────────────

fn render_hint(frame: &mut Frame, mode: &Mode) {
    use ratatui::{layout::Rect, style::Style, widgets::Paragraph};
    let area = frame.area();
    let hint_area = Rect::new(0, area.height.saturating_sub(1), area.width, 1);
    let text = match mode {
        Mode::Normal => "  hjkl: pan   q: quit",
        Mode::SelectedBlock(_, BlockMode::Selected) => {
            "  hjkl: pan   i: edit   esc: deselect   q: quit"
        }
        Mode::SelectedBlock(_, BlockMode::Editing { .. }) => "  enter: confirm   esc: cancel",
    };
    let hint = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, hint_area);
}

// ── Cursor / input helpers ───────────────────────────────────────────────────

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

fn handle_key(code: KeyCode, vp: &mut Viewport, mode: &mut Mode, nodes: &mut Vec<Node>) {
    match mode {
        Mode::SelectedBlock(id, BlockMode::Editing { input, cursor }) => {
            match code {
                KeyCode::Char(ch) => input_enter_char(input, cursor, ch),
                KeyCode::Backspace => input_delete_char(input, cursor),
                KeyCode::Left => *cursor = clamp_cursor(input, cursor.saturating_sub(1)),
                KeyCode::Right => *cursor = clamp_cursor(input, cursor.saturating_add(1)),
                KeyCode::Enter => {
                    // commit the new label into the node
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
            }
        }
        Mode::SelectedBlock(id, BlockMode::Selected) => match code {
            KeyCode::Char('h') => vp.x -= 3,
            KeyCode::Char('l') => vp.x += 3,
            KeyCode::Char('k') => vp.y += 3,
            KeyCode::Char('j') => vp.y -= 3,
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
            KeyCode::Esc => *mode = Mode::Normal,
            _ => {}
        },
        Mode::Normal => match code {
            KeyCode::Char('h') => vp.x -= 3,
            KeyCode::Char('l') => vp.x += 3,
            KeyCode::Char('k') => vp.y += 3,
            KeyCode::Char('j') => vp.y -= 3,
            _ => {}
        },
    }
}

// ── Top-level render ──────────────────────────────────────────────────────────

fn render_map(frame: &mut Frame, nodes: &[Node], edges: &[Edge], vp: &Viewport, mode: &Mode) {
    frame.render_widget(ratatui::widgets::Clear, frame.area());
    render_connections(frame, nodes, edges, vp);
    render_nodes(frame, nodes, vp, mode);
    render_hint(frame, mode);
    if let Mode::SelectedBlock(_, BlockMode::Editing { input, cursor }) = mode {
        render_popup(frame, input, *cursor);
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

    let (mut nodes, edges) = make_demo_graph();
    let mut vp = Viewport { x: 0, y: 0 };
    let mut mode = Mode::SelectedBlock(NodeId(0), BlockMode::Selected);

    loop {
        terminal.draw(|frame| {
            render_map(frame, &nodes, &edges, &vp, &mode);
        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                // quit only when not typing inside an edit popup
                let editing = matches!(mode, Mode::SelectedBlock(_, BlockMode::Editing { .. }));
                if key.code == KeyCode::Char('q') && !editing {
                    break;
                }
                handle_key(key.code, &mut vp, &mut mode, &mut nodes);
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
