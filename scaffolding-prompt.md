# Prompt: Scaffold a Minimal Terminal Canvas Diagram Viewer

You have access to `GUIDE.md` which documents the architecture of a terminal diagramming tool built with ratatui. This task is a minimal implementation of the patterns described there. Read the guide carefully before writing any code — every design decision below is derived from it.

---

## Task

Produce a single Rust source file `src/main.rs` for a new project created with `cargo new diagram-sketch`. The program renders a hardcoded directed graph on a scrollable infinite canvas in the terminal. There is no editing, no mouse input, and no persistence — just rendering and viewport panning.

The program must compile and run with `cargo run` without modification.

---

## Cargo.toml dependencies

Add exactly these three dependencies. Use recent stable versions:

```toml
[dependencies]
ratatui   = "0.30"
crossterm = "0.28"
color-eyre = "0.6"
```

No other crates.

---

## Coordinate system

This is the most important section. Read it twice.

**Canvas space** uses `isize` for both axes. The origin is `(0, 0)`. Coordinates can be negative. Nodes can be placed at any `(isize, isize)` position. There is no clamping.

**Screen space** is also `isize` during calculations. A screen coordinate is obtained by subtracting the viewport offset from the canvas coordinate:

```
screen_x = canvas_x - viewport_x
screen_y = canvas_y - viewport_y
```

Implement this as a free function:

```rust
fn to_screen(canvas_x: isize, canvas_y: isize, vp: &Viewport) -> (isize, isize) {
    (canvas_x - vp.x, canvas_y - vp.y)
}
```

**Viewport** is the top-left corner of the visible window on the canvas. It is a plain struct:

```rust
struct Viewport {
    x: isize,
    y: isize,
}
```

`h/j/k/l` keys pan the viewport. Each keypress moves it by **3 cells**:
- `h` → `vp.x -= 3` (pan left: canvas shifts right on screen)
- `l` → `vp.x += 3`
- `k` → `vp.y -= 3`
- `j` → `vp.y += 3`

There is no clamping — the viewport can scroll into negative canvas territory.

---

## Data model

Define these types exactly. Put them near the top of the file before any functions.

```rust
#[derive(Clone, Copy, PartialEq)]
enum Side { Top, Bottom, Left, Right }

#[derive(Clone, Copy)]
enum ArrowDir {
    Forward,   // arrowhead at destination only
    Backward,  // arrowhead at source only
    Both,      // arrowheads at both ends
}

struct Node {
    id:     usize,
    x:      isize,  // canvas top-left column
    y:      isize,  // canvas top-left row
    width:  u16,    // total box width  including borders
    height: u16,    // total box height including borders
    label:  &'static str,
}

struct Edge {
    from_id:   usize,
    from_side: Side,
    to_id:     usize,
    to_side:   Side,
    dir:       ArrowDir,
}

#[derive(Clone, Copy)]
struct Point {
    x: isize,
    y: isize,
}
```

---

## Demo graph

Implement this function exactly. It returns the nodes and edges that will be rendered. The positions and dimensions are chosen to exercise all three path shapes (S-shape, C-shape, L-corner):

```rust
fn make_demo_graph() -> (Vec<Node>, Vec<Edge>) {
    let nodes = vec![
        Node { id: 0, x: 5,  y: 5,  width: 16, height: 5, label: "Alpha"   },
        Node { id: 1, x: 35, y: 5,  width: 16, height: 5, label: "Beta"    },
        Node { id: 2, x: 35, y: 18, width: 16, height: 5, label: "Gamma"   },
        Node { id: 3, x: 5,  y: 18, width: 16, height: 5, label: "Delta"   },
    ];

    let edges = vec![
        // Alpha.Right → Beta.Left   — horizontal S-shape (nodes side by side)
        Edge { from_id: 0, from_side: Side::Right, to_id: 1, to_side: Side::Left,   dir: ArrowDir::Forward  },
        // Beta.Bottom → Gamma.Top   — vertical drop, same X column, use corner
        Edge { from_id: 1, from_side: Side::Bottom, to_id: 2, to_side: Side::Top,   dir: ArrowDir::Forward  },
        // Gamma.Left  → Delta.Right — reverse horizontal, use S-shape
        Edge { from_id: 2, from_side: Side::Left,  to_id: 3, to_side: Side::Right,  dir: ArrowDir::Both     },
        // Delta.Top   → Alpha.Bottom — same-side exits (both top/bottom), use C-shape
        Edge { from_id: 3, from_side: Side::Top,   to_id: 0, to_side: Side::Bottom, dir: ArrowDir::Backward },
    ];

    (nodes, edges)
}
```

---

## Node rendering

Implement `fn render_nodes(frame: &mut Frame, nodes: &[Node], vp: &Viewport)`.

For each node:

1. Compute `(screen_x, screen_y)` via `to_screen(node.x, node.y, vp)`.
2. Clip: compute the intersection of the node's screen rect with the frame rect using signed arithmetic (see the `SignedRect` pattern in the guide). If there is no intersection, skip the node entirely.
3. On the clipped `Rect`: call `frame.render_widget(Clear, area)` first, then render a `Block::bordered()` with the node label as the title, and a centred `Paragraph` with the label inside the block's inner area.
4. Conditional border sides: only draw the border sides that are not clipped. If the left edge of the node is off-screen, omit `Borders::LEFT`, and so on for all four sides. (See the guide section on viewport culling.)

Use `ratatui::widgets::{Block, Borders, Clear, Paragraph}` and `ratatui::layout::Rect`.

Implement the signed rect intersection inline — no need for a named struct, a helper function is fine:

```rust
// Returns Some((clip_x, clip_y, clip_w, clip_h)) or None
fn clip_to_frame(
    nx: isize, ny: isize, nw: isize, nh: isize,
    fw: isize, fh: isize,
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
```

---

## Path routing

Implement `fn calculate_path(nodes: &[Node], edge: &Edge) -> Vec<Point>`.

Look up `from_node` and `to_node` by `edge.from_id` / `edge.to_id` (index into the slice — IDs are 0-based indices). Then:

**Step 1 — connection points.** The connection point is the midpoint of the specified side of the node box:

```rust
fn connection_point(node: &Node, side: Side) -> Point {
    match side {
        Side::Right  => Point { x: node.x + node.width  as isize - 1,
                                y: node.y + node.height as isize / 2 },
        Side::Left   => Point { x: node.x,
                                y: node.y + node.height as isize / 2 },
        Side::Top    => Point { x: node.x + node.width  as isize / 2,
                                y: node.y },
        Side::Bottom => Point { x: node.x + node.width  as isize / 2,
                                y: node.y + node.height as isize - 1 },
    }
}
```

**Step 2 — offset points.** Push 2 cells away from the node edge perpendicularly so the connector visually clears the border:

```rust
fn offset_point(p: Point, side: Side) -> Point {
    match side {
        Side::Right  => Point { x: p.x + 2, y: p.y },
        Side::Left   => Point { x: p.x - 2, y: p.y },
        Side::Top    => Point { x: p.x,     y: p.y - 2 },
        Side::Bottom => Point { x: p.x,     y: p.y + 2 },
    }
}
```

**Step 3 — choose a path shape.** Use exactly these three shapes and this dispatch logic:

```
dx = to_connection_point.x - from_connection_point.x
dy = to_connection_point.y - from_connection_point.y

if from_side == to_side:
    → c_shape

elif (from_side == Right && to_side == Left) || (from_side == Left && to_side == Right):
    if |dx| >= 6:
        → s_shape (split at horizontal midpoint)
    else:
        → corner (bend at end_off.x, start_off.y)

else:   // mixed horizontal/vertical sides (Top/Bottom mixed with Left/Right, or Top↔Bottom)
    → corner (bend at end_off.x, start_off.y)
```

Implement the three shape functions:

```rust
// S-shape: exits horizontally, turns at the midpoint column, arrives horizontally.
// 6 waypoints: start → start_off → mid_top → mid_bot → end_off → end
fn s_shape(start: Point, s_off: Point, end: Point, e_off: Point) -> Vec<Point> {
    let mid_x = s_off.x + (e_off.x - s_off.x) / 2;
    vec![
        start,
        s_off,
        Point { x: mid_x, y: s_off.y },
        Point { x: mid_x, y: e_off.y },
        e_off,
        end,
    ]
}

// C-shape: both exits on the same side; loops around to the furthest point.
// For Left/Right: loop further in x. For Top/Bottom: loop further in y.
fn c_shape(start: Point, s_off: Point, end: Point, e_off: Point, from_side: Side) -> Vec<Point> {
    match from_side {
        Side::Right | Side::Left => {
            let far_x = if from_side == Side::Right {
                s_off.x.max(e_off.x)   // loop rightward
            } else {
                s_off.x.min(e_off.x)   // loop leftward
            };
            vec![
                start,
                s_off,
                Point { x: far_x, y: s_off.y },
                Point { x: far_x, y: e_off.y },
                e_off,
                end,
            ]
        }
        Side::Top | Side::Bottom => {
            let far_y = if from_side == Side::Bottom {
                s_off.y.max(e_off.y)   // loop downward
            } else {
                s_off.y.min(e_off.y)   // loop upward
            };
            vec![
                start,
                s_off,
                Point { x: s_off.x, y: far_y },
                Point { x: e_off.x, y: far_y },
                e_off,
                end,
            ]
        }
    }
}

// L-corner: one right-angle bend at the intersection of the offset rows/columns.
// 5 waypoints: start → start_off → corner → end_off → end
fn corner(start: Point, s_off: Point, end: Point, e_off: Point) -> Vec<Point> {
    vec![
        start,
        s_off,
        Point { x: e_off.x, y: s_off.y },  // bend here
        e_off,
        end,
    ]
}
```

---

## Connector rendering

Implement `fn render_connections(frame: &mut Frame, nodes: &[Node], edges: &[Edge], vp: &Viewport)`.

For each edge:

1. Call `calculate_path(nodes, edge)` to get the waypoint list.
2. **Visibility check** — skip the entire edge if none of its waypoints have a screen coordinate inside the frame bounds. Use `.iter().any(|p| { let (sx,sy) = to_screen(...); in_frame(sx,sy,frame) })`.
3. **Draw segments** — iterate `path.windows(2)`. For each pair, determine if the segment is horizontal (same y) or vertical (same x), then write the appropriate character into every cell along the segment using `frame.buffer_mut().cell_mut((x as u16, y as u16))`. Skip cells outside frame bounds. Characters: `"─"` for horizontal, `"│"` for vertical. Use `Color::White` for all connectors.
4. **Draw corners** — iterate `path.windows(3)`. At the middle point of each triple, determine the incoming and outgoing `SegDir` and write the correct box-drawing corner character. Use this exact match table:

```
(Left→Down) | (Up→Right)   =>  "┌"
(Right→Down)| (Up→Left)    =>  "┐"
(Down→Right)| (Left→Up)    =>  "└"
(Down→Left) | (Right→Up)   =>  "┘"
straight (same axis)       =>  keep the line char ("─" or "│")
```

5. **Draw arrowheads** — after all segments and corners are drawn, place the arrowhead character(s) according to `edge.dir`:
   - `ArrowDir::Forward`  → arrowhead at `path.last()` (destination end)
   - `ArrowDir::Backward` → arrowhead at `path.first()` (source end)
   - `ArrowDir::Both`     → arrowheads at both ends

   The arrowhead character depends on the direction of arrival at that endpoint. Determine it from the last segment direction for the destination, and the reverse of the first segment direction for the source:

   ```
   arriving from the left  (last seg was Right) → "▶"
   arriving from the right (last seg was Left)  → "◀"
   arriving from above     (last seg was Down)  → "▼"
   arriving from below     (last seg was Up)    → "▲"
   ```

   Write the arrowhead into the buffer at the endpoint's screen coordinate using `Color::Yellow`. This overwrites whatever segment or corner character was placed there — that is intentional.

---

## Event loop

Use the standard minimal ratatui/crossterm setup. The structure must be:

```rust
fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    // Setup
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend  = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // State
    let (nodes, edges) = make_demo_graph();
    let mut vp = Viewport { x: 0, y: 0 };

    // Loop
    loop {
        terminal.draw(|frame| {
            render_map(frame, &nodes, &edges, &vp);
        })?;

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                use crossterm::event::KeyCode;
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('h') => vp.x -= 3,
                    KeyCode::Char('l') => vp.x += 3,
                    KeyCode::Char('k') => vp.y -= 3,
                    KeyCode::Char('j') => vp.y += 3,
                    _ => {}
                }
            }
        }
    }

    // Restore
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    Ok(())
}
```

Implement `fn render_map(frame: &mut Frame, nodes: &[Node], edges: &[Edge], vp: &Viewport)` as the top-level render function:

```rust
fn render_map(frame: &mut Frame, nodes: &[Node], edges: &[Edge], vp: &Viewport) {
    // 1. Clear entire frame
    frame.render_widget(ratatui::widgets::Clear, frame.area());

    // 2. Connections drawn first (under nodes)
    render_connections(frame, nodes, edges, vp);

    // 3. Nodes drawn on top
    render_nodes(frame, nodes, vp);

    // 4. Hint bar at the very bottom row
    render_hint(frame);
}
```

Implement `fn render_hint(frame: &mut Frame)` to render a dim line at the bottom of the screen:

```rust
fn render_hint(frame: &mut Frame) {
    use ratatui::{layout::Rect, style::{Color, Style}, widgets::Paragraph};
    let area = frame.area();
    let hint_area = Rect::new(0, area.height.saturating_sub(1), area.width, 1);
    let hint = Paragraph::new("  hjkl: pan   q: quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, hint_area);
}
```

---

## Helper: in-frame bounds check

Use this helper throughout to guard all buffer writes:

```rust
fn in_frame(x: isize, y: isize, frame: &ratatui::Frame) -> bool {
    let a = frame.area();
    x >= 0 && y >= 0 && x < a.width as isize && y < a.height as isize
}
```

---

## SegDir type

Define this near the top of the file alongside the other types:

```rust
#[derive(Clone, Copy, PartialEq)]
enum SegDir { Right, Left, Up, Down }

fn seg_dir(from: Point, to: Point) -> SegDir {
    if      to.x > from.x { SegDir::Right }
    else if to.x < from.x { SegDir::Left  }
    else if to.y > from.y { SegDir::Down  }
    else                   { SegDir::Up   }
}
```

---

## Constraints and quality requirements

- **Single file.** Everything in `src/main.rs`. No `mod` declarations, no subdirectories.
- **No additional crates.** Only `ratatui`, `crossterm`, `color-eyre`.
- **Must compile** with `cargo build` without warnings (use `#[allow(dead_code)]` if needed for unused variants, but prefer to use all defined types).
- **No panics on small terminals.** All buffer writes must be guarded by bounds checks. `Rect` construction must only happen with non-negative, non-zero dimensions — guard with early returns.
- **No unwrap on buffer cell access.** Use `if let Some(cell) = frame.buffer_mut().cell_mut(...)` for all direct buffer writes.
- **Render order:** connections are drawn before nodes. Nodes are drawn in `id` order (id 0 first). Junction and arrowhead characters are drawn after all nodes, as part of the connection pass — they will be overwritten by nodes if a node sits on top of them, which is the correct behaviour.
- **The hint bar must not overlap with nodes.** The hint bar occupies only the last row. Do not render nodes into the last row — subtract 1 from `fh` in `clip_to_frame` to exclude it.

---

## Expected visual result

When the program starts with `vp = (0, 0)`, the terminal should show something like:

```
┌── Alpha ──────┐               ┌── Beta ────────┐
│               ├──────────────▶│                │
│               │               │                │
└───────────────┘               └───────┬────────┘
                                        │
        ┌───────────────────────────────┘
        ▼
┌── Delta ──────┐               ┌── Gamma ───────┐
│               │◀══════════════│                │
│               │               │                │
└───────┬───────┘               └────────────────┘
        │
        └── (loops back up to Alpha bottom)
```

The exact ASCII art is illustrative, not a pixel-perfect specification. The key correctness criteria are:
- All four nodes are visible at startup
- Connectors exit from the correct sides of each node
- Arrowheads appear at the correct endpoint(s) in yellow
- `hjkl` panning moves the entire diagram smoothly
- No rendering artifacts or panics at any terminal size

---

*Reference: [GUIDE.md](GUIDE.md) — patterns derived from [tmmpr](https://github.com/tanciaku/tmmpr)*
