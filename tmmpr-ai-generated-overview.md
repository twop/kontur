# Building a Terminal Diagramming Tool with Ratatui

A practical guide to rendering nodes at arbitrary canvas positions, drawing orthogonal connectors, and managing a scrollable infinite canvas in a terminal UI.

This guide is derived from studying **[tmmpr](https://github.com/tanciaku/tmmpr)** — a terminal mind-mapping and diagramming tool written in Rust using [ratatui](https://ratatui.rs). All code examples are taken directly from that codebase or are minimal adaptations of its patterns.

---

## Table of Contents

1. [The Infinite Canvas Model](#1-the-infinite-canvas-model)
2. [Rendering Widgets at Arbitrary Positions](#2-rendering-widgets-at-arbitrary-positions)
3. [Viewport Culling](#3-viewport-culling)
4. [Drawing Connectors and Arrows](#4-drawing-connectors-and-arrows)
5. [Z-Ordering and Layering](#5-z-ordering-and-layering)
6. [Alternative Approaches and Crates](#6-alternative-approaches-and-crates)
7. [Key Patterns to Take Away](#7-key-patterns-to-take-away)

---

## 1. The Infinite Canvas Model

The first design decision for any diagramming tool is the coordinate system. tmmpr separates two distinct spaces:

- **Canvas space** — the infinite logical plane where nodes live. Uses `usize` (always non-negative). The top-left corner of the canvas is `(0, 0)`. Nodes can be placed anywhere on this plane.
- **Screen space** — the terminal window, a fixed-size rectangle. Uses `isize` because a node's screen position can temporarily be negative while it is partially off-screen to the left or top.

### The Viewport

The viewport is a rectangular window into the canvas. It is defined entirely by its top-left corner (`view_pos`) and the current terminal dimensions. There is no zoom — one canvas cell equals one terminal cell.

```rust
// src/states/map/viewport.rs
pub struct ViewPos {
    pub x: usize,
    pub y: usize,
}

pub struct ViewportState {
    pub view_pos: ViewPos,
    pub screen_width: usize,   // updated every frame from frame.area()
    pub screen_height: usize,  // updated every frame from frame.area()
}

impl ViewportState {
    /// World → screen: subtract the camera offset.
    /// Returns negative values when the point is above or left of the screen edge.
    pub fn to_screen_coords(&self, p_x: isize, p_y: isize) -> (isize, isize) {
        let p_x = p_x - self.view_pos.x as isize;
        let p_y = p_y - self.view_pos.y as isize;
        (p_x, p_y)
    }
}
```

`to_screen_coords` is the single function that bridges canvas space and screen space. Every rendering function calls it before writing anything to the buffer.

The terminal dimensions are read from the frame every render cycle:

```rust
// src/ui/map/screen.rs
pub fn render_map(frame: &mut Frame, map_state: &mut MapState) {
    // Keep viewport aware of actual terminal size
    map_state.viewport.screen_width  = frame.area().width  as usize;
    map_state.viewport.screen_height = frame.area().height as usize;
    // ...
}
```

### Node positions

Each node stores its top-left canvas coordinate as `(usize, usize)`:

```rust
// src/states/map/note.rs
pub struct Note {
    pub x: usize,     // canvas column of the top-left corner of the box
    pub y: usize,     // canvas row
    pub content: String,
    pub color: Color,
}
```

Moving a node is a simple arithmetic operation on `x` and `y` — there is no physics, no matrix transform, no layout engine.

---

## 2. Rendering Widgets at Arbitrary Positions

### Option A: The ratatui `Canvas` widget

Ratatui ships a `Canvas` widget in `ratatui::widgets::canvas`. It renders vector-style shapes (lines, circles, rectangles, scatter points) onto a grid of braille, half-block, or dot characters. The coordinate system is floating-point and is mapped to terminal cells via `x_bounds` and `y_bounds`:

```rust
use ratatui::widgets::canvas::{Canvas, Line, Rectangle};
use ratatui::symbols::Marker;

let canvas = Canvas::default()
    .marker(Marker::Braille)          // 2×4 dots per cell — fine-grained but single color per cell
    .x_bounds([0.0, 200.0])           // canvas x range mapped to terminal width
    .y_bounds([0.0, 50.0])            // canvas y range mapped to terminal height
    .paint(|ctx| {
        ctx.draw(&Rectangle {
            x: 10.0, y: 10.0,
            width: 20.0, height: 8.0,
            color: Color::White,
        });
        ctx.draw(&Line {
            x1: 30.0, y1: 14.0,
            x2: 60.0, y2: 14.0,
            color: Color::Cyan,
        });
    });

frame.render_widget(canvas, frame.area());
```

You can also implement the `Shape` trait to create custom shapes:

```rust
use ratatui::widgets::canvas::{Painter, Shape};

struct Arrow {
    x1: f64, y1: f64,
    x2: f64, y2: f64,
    color: Color,
}

impl Shape for Arrow {
    fn draw(&self, painter: &mut Painter) {
        // painter.get_point maps float coords to (col, row) cells
        // painter.paint marks that cell with the color
        let steps = ((self.x2 - self.x1).abs() + (self.y2 - self.y1).abs()) as usize * 4;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = self.x1 + t * (self.x2 - self.x1);
            let y = self.y1 + t * (self.y2 - self.y1);
            if let Some((col, row)) = painter.get_point(x, y) {
                painter.paint(col, row, self.color);
            }
        }
    }
}
```

**Limitation of `Canvas`:** Braille dots look great for smooth curves, but each terminal cell can only hold one unicode character. If you need to render a bordered text box (using box-drawing characters like `┌─┐│└─┘`) at an arbitrary position on the canvas, the `Canvas` widget cannot do it — it only renders single-color dot patterns, not full unicode strings. For text nodes and box-drawing connector lines, you need the approach below.

### Option B: Direct buffer mutation (what tmmpr uses)

Ratatui exposes the raw terminal buffer through `frame.buffer_mut()`. You can write any unicode string into any cell at any `(column, row)` position. This is the mechanism behind all of tmmpr's rendering.

```rust
// Write a single box-drawing character directly into the buffer
if let Some(cell) = frame.buffer_mut().cell_mut((x as u16, y as u16)) {
    cell.set_symbol("─")
        .set_fg(Color::White);
}
```

This bypasses layout entirely and treats the terminal as a grid of individually addressable cells.

### Rendering a bordered text box at a canvas position

For nodes with text content, tmmpr uses ratatui's standard `Paragraph` and `Block` widgets — but passes them a manually computed `Rect` based on the viewport transform, rather than using `Layout`:

```rust
// src/ui/map/notes.rs (simplified)
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, BorderType, Clear, Paragraph},
};

fn render_note(frame: &mut Frame, note: &Note, viewport: &ViewportState) {
    let (note_width, note_height) = note.get_dimensions();

    // Step 1: transform canvas position to screen position
    let (screen_x, screen_y) = viewport.to_screen_coords(note.x as isize, note.y as isize);

    // Step 2: create a Rect (must be non-negative — handle clipping first, see section 3)
    let area = Rect::new(
        screen_x as u16,
        screen_y as u16,
        note_width,
        note_height,
    );

    // Step 3: clear the area (prevents lower z-index notes showing through)
    frame.render_widget(Clear, area);

    // Step 4: render content with a border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(note.color);

    let paragraph = Paragraph::new(note.content.as_str()).block(block);
    frame.render_widget(paragraph, area);
}
```

`Note::get_dimensions()` calculates the box size from content:

```rust
// src/states/map/note.rs
pub fn get_dimensions(&self) -> (u16, u16) {
    // Count newlines rather than using .lines() to preserve trailing blank lines
    let height = (1 + self.content.matches('\n').count()) as u16;
    let width = self.content.lines()
        .map(|line| line.width())   // unicode-width: accounts for wide CJK chars
        .max()
        .unwrap_or(0) as u16;

    // Apply minimums and add space for borders + cursor column
    let width  = (width  + 2).max(20) + 1;
    let height = (height + 2).max(4);
    (width, height)
}
```

### Placing the text cursor

In text-editing mode, the hardware cursor must be positioned at the byte offset within the note's content. This requires converting a byte offset to a display column (accounting for multi-byte unicode):

```rust
// src/ui/map/notes.rs
let text_before_cursor = &note.content[..cursor_byte_offset];

let cursor_row = text_before_cursor.matches('\n').count();
let cursor_col = match text_before_cursor.rfind('\n') {
    Some(nl) => text_before_cursor[nl + 1..].width(),  // width after last newline
    None     => text_before_cursor.width(),             // single-line
};

let screen_cursor_x = note_area.x as usize + 1 + cursor_col;  // +1 for left border
let screen_cursor_y = note_area.y as usize + 1 + cursor_row;  // +1 for top border

frame.set_cursor_position(Position::new(
    screen_cursor_x as u16,
    screen_cursor_y as u16,
));
```

---

## 3. Viewport Culling

Before rendering, every node should be checked against the viewport bounds. Nodes completely outside the visible area are skipped entirely. Nodes partially visible need to be clipped: only the visible rectangle is drawn, and the text content is scrolled to match.

### SignedRect intersection

Standard ratatui `Rect` uses `u16` and will panic (or wrap) on negative values. tmmpr defines its own `SignedRect` with `isize` fields for the intermediate calculation:

```rust
// src/states/map/geometry.rs
pub struct SignedRect {
    pub x: isize, pub y: isize,
    pub width: isize, pub height: isize,
}

impl SignedRect {
    /// Returns the overlapping region, or None if the rectangles don't intersect.
    pub fn intersection(&self, view: &SignedRect) -> Option<SignedRect> {
        // Early-out: no overlap
        if self.x >= view.x + view.width
           || self.x + self.width  <= view.x
           || self.y >= view.y + view.height
           || self.y + self.height <= view.y
        {
            return None;
        }

        let x1 = self.x.max(view.x);
        let x2 = (self.x + self.width).min(view.x + view.width);
        let y1 = self.y.max(view.y);
        let y2 = (self.y + self.height).min(view.y + view.height);

        Some(SignedRect { x: x1, y: y1, width: x2 - x1, height: y2 - y1 })
    }
}
```

### Full culling + clipping loop

```rust
// src/ui/map/notes.rs (simplified)
for &note_id in map_state.notes_state.render_order() {
    let note = &notes[note_id];
    let (note_width, note_height) = note.get_dimensions();

    let (screen_x, screen_y) = viewport.to_screen_coords(note.x as isize, note.y as isize);

    let note_rect  = SignedRect { x: screen_x, y: screen_y,
                                   width: note_width as isize, height: note_height as isize };
    let frame_rect = SignedRect { x: 0, y: 0,
                                   width: frame.area().width as isize,
                                   height: frame.area().height as isize };

    // Skip completely off-screen notes
    if let Some(visible) = note_rect.intersection(&frame_rect) {

        // visible is always non-negative here — safe to cast to u16
        let area = Rect::new(
            visible.x as u16, visible.y as u16,
            visible.width as u16, visible.height as u16,
        );

        // Scroll the Paragraph to show only the visible slice of content
        let h_scroll = (visible.x - note_rect.x) as u16;
        let v_scroll = (visible.y - note_rect.y) as u16;

        // Only draw borders for sides that are not clipped off
        let mut borders = Borders::NONE;
        if note_rect.x == visible.x                         { borders |= Borders::LEFT;   }
        if note_rect.x + note_rect.width  == visible.x + visible.width  { borders |= Borders::RIGHT;  }
        if note_rect.y == visible.y                         { borders |= Borders::TOP;    }
        if note_rect.y + note_rect.height == visible.y + visible.height { borders |= Borders::BOTTOM; }

        let block  = Block::default().borders(borders).border_style(note.color);
        let widget = Paragraph::new(note.content.as_str())
            .scroll((v_scroll, h_scroll))
            .block(block);

        frame.render_widget(Clear,  area);
        frame.render_widget(widget, area);
    }
}
```

The selective border rendering (`Borders::LEFT | Borders::TOP` etc.) is a subtle but important detail: if a note is clipped on the right, drawing a right border at the clipped edge would look wrong.

---

## 4. Drawing Connectors and Arrows

This is the most algorithmically involved part. The pipeline has two stages: **path routing** and **path rendering**.

### 4.1 Path routing

Rather than running A* or a full graph-layout algorithm on every frame, tmmpr uses a hand-crafted routing function that selects a path shape based on the spatial relationship between the two endpoint notes.

Each note exposes a *connection point* — the midpoint of a specified side in canvas coordinates:

```rust
// src/states/map/note.rs
pub fn get_connection_point(&self, side: Side) -> (usize, usize) {
    let (w, h) = self.get_dimensions();
    match side {
        Side::Right  => (self.x + w as usize - 1,    self.y + (h / 2) as usize),
        Side::Left   => (self.x,                      self.y + (h / 2) as usize),
        Side::Top    => (self.x + (w / 2) as usize,   self.y),
        Side::Bottom => (self.x + (w / 2) as usize,   self.y + h as usize - 1),
    }
}
```

Connections exit the note perpendicularly with an *offset point* 2 cells away from the edge, to visually clear the border:

```rust
// src/utils/geometry.rs
pub fn get_offset_point(p: Point, side: Side) -> Point {
    let offset = 2;
    match side {
        Side::Right  => Point { x: p.x + offset, y: p.y },
        Side::Left   => Point { x: p.x - offset, y: p.y },
        Side::Top    => Point { x: p.x,           y: p.y - offset },
        Side::Bottom => Point { x: p.x,           y: p.y + offset },
    }
}
```

`calculate_path` returns a small `Vec<Point>` of waypoints. The path is a sequence of axis-aligned segments — no diagonal lines. The shape is selected by a match on `(start_side, end_side, h_placement, v_placement)`:

```rust
// src/utils/geometry.rs
pub fn calculate_path(
    start_note: &Note, start_side: Side,
    end_note:   &Note, end_side:   Side,
) -> Vec<Point> {
    let start     = start_note.get_connection_point(start_side);
    let end       = end_note  .get_connection_point(end_side);
    let start     = Point { x: start.0 as isize, y: start.1 as isize };
    let end       = Point { x: end.0   as isize, y: end.1   as isize };
    let start_off = get_offset_point(start, start_side);
    let end_off   = get_offset_point(end,   end_side);

    let dx = end.x - start.x;   // positive = end is to the right
    let dy = end.y - start.y;   // positive = end is below

    // Classify the spatial relationship into 3 zones each axis
    // ±4 cells is the threshold; within that range is "Level"
    let h = match dx { 4.. => HPlacement::Right, ..=-4 => HPlacement::Left, _ => HPlacement::Level };
    let v = match dy { 4.. => VPlacement::Below, ..=-4 => VPlacement::Above, _ => VPlacement::Level };

    match (start_side, end_side) {
        (Side::Right, Side::Left) => match (h, v) {
            (HPlacement::Right, _) =>
                s_shapes(start, start_off, end, end_off, dx),
            _ =>
                sideways_s_shapes_y(start, start_off, end, end_off, dy),
        },
        (Side::Right, Side::Right) | (Side::Left, Side::Left) =>
            reverse_c_shape(start, start_off, end, end_off),
        // ... 16 total (start_side, end_side) combinations
        _ => vec![],
    }
}
```

The available path shapes, each returning 5–6 waypoints:

| Shape | Description | Canvas appearance |
|---|---|---|
| `s_shapes` | Horizontal S / reverse-S | `──┐` `│` `└──` |
| `sideways_s_shapes_y` | Vertical S using Y midpoint | vertical variant of above |
| `sideways_s_shapes_x` | Vertical S using X midpoint | crosses a horizontal centerline |
| `corner_shapes_1` | L-bend at `(end_off.x, start_off.y)` | right-angle turn |
| `corner_shapes_2` | L-bend at `(start_off.x, end_off.y)` | right-angle turn (other orientation) |
| `c_shape` | C opening left | used when both notes exit leftward |
| `reverse_c_shape` | C opening right | used when both notes exit rightward |
| `u_shapes` | U opening down | both notes exit bottom side |
| `upside_down_u_shapes` | U opening up | both notes exit top side |

Example — the S-shape: split the horizontal gap at its midpoint and route through it:

```rust
fn s_shapes(
    start: Point, start_off: Point,
    end:   Point, end_off:   Point,
    dx: isize,
) -> Vec<Point> {
    let mid_x = start.x + dx / 2;
    vec![
        start,
        start_off,
        Point { x: mid_x, y: start_off.y },  // horizontal segment from start side
        Point { x: mid_x, y: end_off.y },    // vertical segment through midpoint
        end_off,
        end,                                  // horizontal segment to end side
    ]
}
```

### 4.2 Path rendering

Once the waypoints are calculated, they are rasterised cell-by-cell directly into the frame buffer.

**Step 1 — draw the line segments** (pairs of consecutive waypoints):

```rust
// src/ui/map/connections.rs
for points in path.windows(2) {
    let (x1, y1) = viewport.to_screen_coords(points[0].x, points[0].y);

    if points[0].x != points[1].x {
        // Horizontal segment
        let len = (points[1].x - points[0].x).abs();
        for offset in 0..len {
            let x = if points[1].x > points[0].x { x1 + offset } else { x1 - offset };
            if in_bounds(x, y1, frame) {
                if let Some(cell) = frame.buffer_mut().cell_mut((x as u16, y1 as u16)) {
                    cell.set_symbol("─").set_fg(color);
                }
            }
        }
    } else {
        // Vertical segment
        let len = (points[1].y - points[0].y).abs();
        for offset in 0..len {
            let (x2, y2) = viewport.to_screen_coords(points[0].x, points[0].y);
            let y = if points[1].y > points[0].y { y2 + offset } else { y2 - offset };
            if in_bounds(x2, y, frame) {
                if let Some(cell) = frame.buffer_mut().cell_mut((x2 as u16, y as u16)) {
                    cell.set_symbol("│").set_fg(color);
                }
            }
        }
    }
}
```

**Step 2 — draw corner characters at bends** (triples of consecutive waypoints):

First, classify each segment's direction:

```rust
#[derive(Copy, Clone)]
pub enum SegDir { Right, Left, Up, Down }

let mut seg_dirs: Vec<SegDir> = path.windows(2).map(|pts| {
    if pts[1].x > pts[0].x      { SegDir::Right }
    else if pts[1].x < pts[0].x { SegDir::Left  }
    else if pts[1].y > pts[0].y { SegDir::Down  }
    else                         { SegDir::Up    }
}).collect();
```

Then pick the correct corner glyph at each bend:

```rust
for (i, pts) in path.windows(3).enumerate() {
    let (px, py) = viewport.to_screen_coords(pts[1].x, pts[1].y);
    let incoming = seg_dirs[i];
    let outgoing = seg_dirs[i + 1];

    let glyph = match (incoming, outgoing) {
        (SegDir::Left,  SegDir::Down) | (SegDir::Up,   SegDir::Right) => "┌",
        (SegDir::Right, SegDir::Down) | (SegDir::Up,   SegDir::Left)  => "┐",
        (SegDir::Down,  SegDir::Right)| (SegDir::Left, SegDir::Up)    => "└",
        (SegDir::Down,  SegDir::Left) | (SegDir::Right,SegDir::Up)    => "┘",
        (SegDir::Left | SegDir::Right, SegDir::Left | SegDir::Right)   => "─",
        (SegDir::Up   | SegDir::Down,  SegDir::Up   | SegDir::Down)    => "│",
        _ => continue,
    };

    if in_bounds(px, py, frame) {
        if let Some(cell) = frame.buffer_mut().cell_mut((px as u16, py as u16)) {
            cell.set_symbol(glyph).set_fg(color);
        }
    }
}
```

### 4.3 Character sets

tmmpr keeps the connector glyphs in simple arrays indexed by purpose:

```rust
// src/ui/constants.rs
//                              horiz  vert   ┌     ┐     └     ┘
pub const NORMAL_CHARSET:      [&str; 6] = ["─", "│", "┌", "┐", "└", "┘"];
pub const IN_PROGRESS_CHARSET: [&str; 6] = ["━", "┃", "┏", "┓", "┗", "┛"];

// Junction chars where connectors meet node borders
//                              top    bottom left   right
pub const PLAIN_JUNCTIONS:  [&str; 4] = ["┴", "┬", "┤", "├"];
pub const THICK_JUNCTIONS:  [&str; 4] = ["┻", "┳", "┫", "┣"];
pub const DOUBLE_JUNCTIONS: [&str; 4] = ["╩", "╦", "╣", "╠"];
```

After each node is drawn, junction characters are stamped on top of the node border wherever a connector attaches:

```rust
// src/ui/map/connections.rs
pub fn draw_connecting_character(
    note: &Note, side: Side, color: Color,
    frame: &mut Frame, viewport: &ViewportState,
) {
    let junctions = &PLAIN_JUNCTIONS; // or THICK_JUNCTIONS / DOUBLE_JUNCTIONS per mode

    let glyph = match side {
        Side::Top    => junctions[0],  // ┴
        Side::Bottom => junctions[1],  // ┬
        Side::Left   => junctions[2],  // ┤
        Side::Right  => junctions[3],  // ├
    };

    let (cx, cy) = note.get_connection_point(side);
    let (sx, sy) = viewport.to_screen_coords(cx as isize, cy as isize);

    if in_bounds(sx, sy, frame) {
        if let Some(cell) = frame.buffer_mut().cell_mut((sx as u16, sy as u16)) {
            cell.set_symbol(glyph).set_fg(color);
        }
    }
}
```

### 4.4 Off-screen optimisation

Computing the path for a connector is cheap, but iterating every cell of a long connector is not. Connections are skipped entirely if none of their waypoints fall within the terminal bounds:

```rust
let is_visible = path.iter().any(|pt| {
    let (sx, sy) = viewport.to_screen_coords(pt.x, pt.y);
    sx >= 0 && sx < frame.area().width  as isize &&
    sy >= 0 && sy < frame.area().height as isize
});

if !is_visible { continue; }
```

---

## 5. Z-Ordering and Layering

The render order in a canvas tool determines what appears "on top" of what.

tmmpr's render order:

1. **Connections** — drawn first, so node boxes cover connector lines cleanly at the endpoints
2. **Nodes** — drawn in z-order (a `Vec<usize>` of node IDs; last element = topmost). Selecting a node moves its ID to the end of this Vec
3. **Junction characters** — drawn after each node so they appear on top of the node border
4. **Status bar** — drawn last, always on top of everything

```rust
// src/ui/map/screen.rs
pub fn render_map(frame: &mut Frame, map_state: &mut MapState) {
    frame.render_widget(Clear, frame.area());   // full clear each frame
    render_connections(frame, map_state);        // layer 1: wires
    render_notes(frame, map_state);              // layer 2: nodes (includes junction chars)
    render_bar(frame, map_state);                // layer 3: chrome
}
```

The full-screen `Clear` before each frame is important. Without it, stale content from the previous frame bleeds through wherever nothing is rendered this frame (e.g., after a node is moved).

Each node is also cleared individually with `frame.render_widget(Clear, note_area)` before its `Paragraph` is drawn. This prevents lower z-index nodes from showing through the background of a higher-index one.

The z-index Vec inside `NotesState`:

```rust
pub struct NotesState {
    notes: HashMap<usize, Note>,  // O(1) lookup by ID
    render_order: Vec<usize>,     // draw sequence: render_order[0] is bottom-most
}

// Bring a node to the top (called on selection)
fn bring_to_front(&mut self, id: usize) {
    self.render_order.retain(|&x| x != id);
    self.render_order.push(id);  // last = drawn last = on top
}
```

---

## 6. Alternative Approaches and Crates

The approach above is hand-rolled and highly tailored for character-cell terminals. Depending on your requirements you may want to build on existing libraries instead.

### ratatui `Canvas` + custom `Shape`

If your nodes do not need text content — only coloured dots or simple outlines — the built-in `Canvas` widget avoids all the manual buffer manipulation. Implement the `Shape` trait and call `ctx.draw(&your_shape)` inside the `paint` closure. The main limitation is that each terminal cell can only hold one braille pattern with one foreground colour; mixing unicode box-drawing characters and braille in the same cell is not possible.

### `petgraph` for graph topology

[petgraph](https://crates.io/crates/petgraph) is the standard Rust graph data structure library. It provides:

- `Graph`, `DiGraph`, `UnGraph` — directed/undirected adjacency representations
- BFS, DFS, topological sort, shortest path algorithms (Dijkstra, Bellman-Ford)
- Cycle detection

tmmpr manages its own simple `HashMap` + index, which is appropriate for a small interactive canvas. If your application needs more complex graph algorithms — e.g., automatic layout, finding the spanning tree, or routing based on graph distance — pull in `petgraph` for the data structure and algorithms, and keep the rendering layer separate.

### `tui-nodes`

[tui-nodes](https://crates.io/crates/tui-nodes) is a ratatui widget crate specifically for node graph editing. It provides ready-made node boxes with connectors and some layout assistance. It is worth evaluating if you want to skip the from-scratch work, though it may not offer the same degree of control as the approach shown here.

### Orthogonal edge routing algorithms

The routing logic in tmmpr is a finite decision table — effective but hand-tuned. For a tool where arbitrary graph topologies are common (many crossing edges, nodes in varied arrangements), a more systematic approach may be warranted:

**Grid-based A\*** — discretise the canvas into a grid, treat each cell as a node, mark cells occupied by note boxes as obstacles, and run A* to find a shortest obstacle-avoiding path. The path will naturally be orthogonal if you only allow horizontal and vertical moves. This produces optimal non-overlapping routes but is expensive to run for large canvases.

**Sugiyama framework** — a multi-pass algorithm for layered graph drawing. Works well for directed acyclic graphs (flowcharts, dependency diagrams). It assigns layers, orders nodes to minimise crossings, and calculates spline or polyline connector routes. The `layout` crate on crates.io provides a partial Rust implementation.

**Rectilinear Steiner routing** — given a set of terminal points, find the shortest orthogonal tree connecting them. Used in PCB autorouting. More complex to implement but produces very clean diagrams.

For most interactive mind-map use cases, the decision-table approach in tmmpr is the right trade-off: it is O(1) per connection, produces aesthetically consistent results for the common cases, and is trivially debuggable.

---

## 7. Key Patterns to Take Away

**Separate canvas space from screen space.** Store all data in canvas coordinates (`usize`). Translate to screen coordinates (`isize`) only at render time with a single `to_screen_coords` function. Never store screen coordinates in your data model.

**Cull before you render.** Intersect each node's screen-space rect with the frame rect before constructing any widget. Skip nodes entirely if the intersection is `None`. Use `Paragraph::scroll()` to display only the visible slice of partially-clipped content.

**Use `SignedRect` for clip math.** Ratatui's `Rect` is `u16` and will overflow on negative values. Keep a signed rectangle type for intermediate calculations; only cast to `u16` after clipping guarantees non-negative values.

**Connections render before nodes.** Draw connector lines first so node boxes naturally cover the line endpoints. Then stamp junction characters on top of node borders after each node is drawn.

**Direct buffer writes for character-level graphics.** `frame.buffer_mut().cell_mut((x, y))` is the escape hatch for anything that cannot be expressed as a layout-positioned widget. Connector lines, corner glyphs, and junction characters all use this. It is idiomatic in ratatui — not a hack.

**Maintain a z-index Vec.** Keep node IDs in a `Vec` in render order. Moving an ID to the end is O(n) but n is small for interactive canvases. The simplicity is worth it.

**Use box-drawing unicode instead of ASCII.** The full box-drawing block (U+2500–U+257F) includes single, double, and thick variants for horizontals, verticals, and all four corners. The junction characters (T-shapes and crosses: `┤ ├ ┴ ┬ ┼`) handle connector attachment points cleanly. See the [Unicode box-drawing chart](https://en.wikipedia.org/wiki/Box-drawing_characters) for the complete set.

**On-demand rendering reduces CPU to near zero.** Only call `terminal.draw()` when state has changed. A `needs_redraw: bool` flag toggled by input handlers is sufficient. Poll events with a short timeout (50 ms) to remain responsive. tmmpr's idle CPU usage is effectively 0%.

---

*Based on [tmmpr](https://github.com/tanciaku/tmmpr) — a terminal mind-mapping tool by [tanciaku](https://github.com/tanciaku). MIT License.*
