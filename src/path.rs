use crate::geometry::{Dir, SPoint, SRect};
use crate::state::{ArrowDecorations, Edge, Node, NodeId, Side};
use ratatui::symbols::line as ratatui_line;

const DEFAULT_CSHAPE_STICKOUT: u16 = 1;

// ── PathSegment ───────────────────────────────────────────────────────────────

/// A graphical symbol that can appear at a single cell on a rendered path.
///
/// This is the intermediate representation used by both:
/// - `ui/mod.rs` (maps to Unicode box-drawing / arrow characters)
/// - tests (`test_render` maps to ASCII approximations for golden-string tests)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSymbol {
    /// Horizontal run  ─
    Horizontal,
    /// Vertical run  │
    Vertical,
    /// Corner: arriving from Left or departing Down  ┌
    CornerTopLeft,
    /// Corner: arriving from Right or departing Down  ┐
    CornerTopRight,
    /// Corner: arriving from Left or departing Up  └
    CornerBottomLeft,
    /// Corner: arriving from Right or departing Up  ┘
    CornerBottomRight,
    /// Arrowhead pointing right  →
    ArrowRight,
    /// Arrowhead pointing left  ←
    ArrowLeft,
    /// Arrowhead pointing down  ↓
    ArrowDown,
    /// Arrowhead pointing up  ↑
    ArrowUp,
}

impl PathSymbol {
    /// Map the segment to its Unicode box-drawing / arrow symbol used in the UI.
    pub fn to_symbol(self) -> &'static str {
        match self {
            PathSymbol::Horizontal => ratatui_line::HORIZONTAL,
            PathSymbol::Vertical => ratatui_line::VERTICAL,
            PathSymbol::CornerTopLeft => ratatui_line::ROUNDED_TOP_LEFT,
            PathSymbol::CornerTopRight => ratatui_line::ROUNDED_TOP_RIGHT,
            PathSymbol::CornerBottomLeft => ratatui_line::ROUNDED_BOTTOM_LEFT,
            PathSymbol::CornerBottomRight => ratatui_line::ROUNDED_BOTTOM_RIGHT,
            PathSymbol::ArrowRight => "▷", // alternatives: ▶ ▷ ▹ ► ❯ > ▸
            PathSymbol::ArrowLeft => "◁",  // alternatives: ◀ ◁ ◃ ◄ ❮ < ◂
            PathSymbol::ArrowDown => "▽",  // alternatives: ▼ ▽ ▿ v ▾
            PathSymbol::ArrowUp => "△",    // alternatives: ▲ △ ▵ ^ ▴
        }
    }

    /// Map the segment to a plain ASCII character used in test golden strings.
    pub fn to_ascii(self) -> char {
        match self {
            PathSymbol::Horizontal => '-',
            PathSymbol::Vertical => '|',
            PathSymbol::CornerTopLeft => '+',
            PathSymbol::CornerTopRight => '+',
            PathSymbol::CornerBottomLeft => '+',
            PathSymbol::CornerBottomRight => '+',
            PathSymbol::ArrowRight => '>',
            PathSymbol::ArrowLeft => '<',
            PathSymbol::ArrowDown => 'v',
            PathSymbol::ArrowUp => '^',
        }
    }
}

// ── Axis ──────────────────────────────────────────────────────────────────────

/// The axis along which the stubs leave their connectors in an S-shaped route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Stubs exit horizontally (left or right); the middle jog is vertical.
    Horizontal,
    /// Stubs exit vertically (up or down); the middle jog is horizontal.
    Vertical,
}

// ── Connection / offset helpers ───────────────────────────────────────────────

/// Returns the border cell on `side` of `rect` where a connection line starts.
pub(crate) fn rect_connection_point(rect: SRect, side: Side) -> SPoint {
    match side {
        Side::Right => rect.mid_right() + (1, 0),
        Side::Left => rect.mid_left() - (1, 0),
        Side::Top => rect.mid_top() - (0, 1),
        Side::Bottom => rect.mid_bottom() + (0, 1),
    }
}

/// Returns the border cell on `side` of `node` where a connection line starts.
pub(crate) fn connection_point(node: &Node, side: Side) -> SPoint {
    rect_connection_point(node.rect, side)
}

/// Returns the two endpoints of `edge` ordered geometrically:
/// left-most connection point first; if X is equal, top-most first.
/// Each endpoint is a `(NodeId, Side)` pair.
///
/// Returns `None` if either node cannot be found in `nodes`.
pub(crate) fn edge_endpoints_ordered(
    nodes: &[Node],
    edge: &Edge,
) -> Option<((NodeId, Side), (NodeId, Side))> {
    let from_node = nodes.iter().find(|n| n.id == edge.from_id)?;
    let to_node = nodes.iter().find(|n| n.id == edge.to_id)?;

    let mut points = [(from_node, edge.from_side), (to_node, edge.to_side)]
        .map(|(node, side)| (node.id, side, connection_point(node, side)));

    points.sort_by_key(|(_, _, point)| (point.y, point.x));

    let [start, end] = points.map(|(id, side, _)| (id, side));

    Some((start, end))
}

// ── Endpoint ordering ─────────────────────────────────────────────────────────

/// Spatial relationship of `start` relative to `end` after the two connection
/// points have been sorted top-first, then left-first.
///
/// Because sorting guarantees `start.y <= end.y` (and `start.x <= end.x` when
/// the y-coordinates are equal), only five cases are possible:
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointRelation {
    /// `start == end`.
    Coincident,
    /// `start.x == end.x`, `start.y < end.y` — directly above, same column.
    Above,
    /// `start.y == end.y`, `start.x < end.x` — directly left, same row.
    LeftOf,
    /// `start.x < end.x && start.y < end.y` — above and to the left.
    AboveLeft,
    /// `start.x > end.x && start.y < end.y` — above and to the right.
    AboveRight,
}

/// The result of [`sort_endpoints`]: the two connection points in a canonical
/// top-first / left-first order, together with their exit directions, the
/// (possibly flipped) arrow decoration, and the spatial relationship.
#[derive(Debug)]
pub struct OrderedEndpoints {
    /// Topmost (then leftmost) connection point — always the rendering start.
    pub start: (Side, SRect),
    /// The other connection point.
    pub end: (Side, SRect),

    /// Arrow decoration, flipped if the original `from`/`to` order was swapped
    /// (`Forward` ↔ `Backward`; `Both` is unchanged).
    pub arrow: ArrowDecorations,

    /// Spatial relationship of `start` relative to `end`.
    pub relation: PointRelation,
}

/// Maps a [`Side`] to the [`Dir`] a connector exits in.
fn side_to_dir(side: Side) -> Dir {
    match side {
        Side::Right => Dir::Right,
        Side::Left => Dir::Left,
        Side::Top => Dir::Up,
        Side::Bottom => Dir::Down,
    }
}

/// Flip `Forward` ↔ `Backward`; `Both` is symmetric so it stays unchanged.
fn flip_arrow(arrow: ArrowDecorations) -> ArrowDecorations {
    match arrow {
        ArrowDecorations::Forward => ArrowDecorations::Backward,
        ArrowDecorations::Backward => ArrowDecorations::Forward,
        ArrowDecorations::Both => ArrowDecorations::Both,
    }
}

/// Sort two node endpoints into a canonical order (topmost first, then
/// leftmost) and return an [`OrderedEndpoints`] describing the result.
///
/// The sort key is `(point.y, point.x)` — the same convention used by
/// [`edge_endpoints_ordered`].  When the two points are swapped the arrow
/// decoration is flipped so that the arrowhead(s) still point in the
/// semantically correct direction.
pub(crate) fn sort_endpoints(
    from_node: &Node,
    from_side: Side,
    to_node: &Node,
    to_side: Side,
    arrow: ArrowDecorations,
) -> OrderedEndpoints {
    let from_pt = connection_point(from_node, from_side);
    let to_pt = connection_point(to_node, to_side);

    let swapped = (from_pt.y, from_pt.x) > (to_pt.y, to_pt.x);

    let (start, end, arrow) = if swapped {
        (
            (to_side, to_node.rect),
            (from_side, from_node.rect),
            flip_arrow(arrow),
        )
    } else {
        ((from_side, from_node.rect), (to_side, to_node.rect), arrow)
    };

    let start_pt = rect_connection_point(start.1, start.0);
    let end_pt = rect_connection_point(end.1, end.0);

    let relation = if start_pt == end_pt {
        PointRelation::Coincident
    } else if start_pt.x == end_pt.x {
        PointRelation::Above
    } else if start_pt.y == end_pt.y {
        PointRelation::LeftOf
    } else if start_pt.x < end_pt.x {
        PointRelation::AboveLeft
    } else {
        // start.x > end.x && start.y < end.y — guaranteed by the sort
        PointRelation::AboveRight
    };

    OrderedEndpoints {
        start,
        end,
        arrow,
        relation,
    }
}

// ── Shape builders ────────────────────────────────────────────────────────────
//
// Each builder returns the starting point plus a Vec of (Dir, steps) runs.
// Steps are the number of cells to advance in that direction.

/// S-shaped route: two parallel stubs joined by a perpendicular jog in the middle.
///
/// * `start` — the inclusive start cell of the path (exit point of the first connector).
/// * `dir`   — the axis along which both stubs leave their connectors:
///   - [`Axis::Horizontal`]: stubs go left/right; the middle jog is vertical.
///     The jog is placed at `mid_x = (start.x + end.x) / 2`.
///   - [`Axis::Vertical`]: stubs go up/down; the middle jog is horizontal.
///     The jog is placed at `mid_y = (start.y + end.y) / 2`.
/// * `end`   — the inclusive end cell of the path (exit point of the second connector).
///
/// Zero-length runs are emitted as-is when `start` and `end` share the same
/// x- (horizontal) or y- (vertical) coordinate.
fn s_shape(start: SPoint, dir: Axis, end: SPoint) -> (SPoint, Vec<(Dir, u32)>) {
    match dir {
        Axis::Horizontal => {
            // mid_x is the absolute x-coordinate of the vertical jog.
            //
            // PathIter emits `start` as the very first cell without advancing (inclusive
            // start), so run1 needs `distance + 1` steps to land *on* mid_x — matching
            // the `so + 1` convention used in c_shape.
            //
            // run2 is an exact delta: PathIter resumes advancing *from* the bend cell
            // (which run1 already placed the corner on), so no adjustment needed.
            //
            // run3 also starts advancing from its bend cell (mid_x, end.y), so
            // `distance` steps land exactly on end.x — no `+1`.
            let mid_x = (start.x + end.x) / 2;
            let hor_dir = if end.x >= start.x {
                Dir::Right
            } else {
                Dir::Left
            };
            let vert_dir = if end.y >= start.y { Dir::Down } else { Dir::Up };
            (
                start,
                (vec![
                    (hor_dir, (mid_x - start.x).unsigned_abs() + 1),
                    (vert_dir, (end.y - start.y).unsigned_abs()),
                    (hor_dir, (end.x - mid_x).unsigned_abs()),
                ]),
            )
        }
        Axis::Vertical => {
            // Same asymmetry: run1 needs +1, run3 does not.
            let mid_y = (start.y + end.y) / 2;
            let vert_dir = if end.y >= start.y { Dir::Down } else { Dir::Up };
            let hor_dir = if end.x >= start.x {
                Dir::Right
            } else {
                Dir::Left
            };
            (
                start,
                vec![
                    (vert_dir, (mid_y - start.y).unsigned_abs() + 1),
                    (hor_dir, (end.x - start.x).unsigned_abs()),
                    (vert_dir, (end.y - mid_y).unsigned_abs()),
                ],
            )
        }
    }
}

/// C-shaped route: both stubs go in the same direction then wrap around.
///
/// * `start` / `end` — the two inclusive endpoint cells of the path.
/// * `dir`   — the direction both stubs point out from their respective nodes.
/// * `offset` — how many cells to travel in `dir` before turning; this sets
///              the position of the "far" column (horizontal dirs) or row
///              (vertical dirs) at which the two stubs meet.
///              note that, for offset=2, for Dir=Right from 0,0 the end point of
///              the starting line will be 0,3 (2 for the connector and 1 for the line itself)
fn c_shape(start: SPoint, end: SPoint, dir: Dir, offset: u32) -> (SPoint, Vec<(Dir, u32)>) {
    match dir {
        Dir::Right => {
            let (start_step_out, end_step_out) = match end.x - start.x {
                delta if delta > 0 => (delta.abs(), 0),
                delta => (0, delta.abs()),
            };

            (
                start,
                Vec::from([
                    (Dir::Right, (start_step_out) as u32 + 1 + offset),
                    match start.y - end.y {
                        delta if delta < 0 => (Dir::Down, delta.unsigned_abs()),
                        delta => (Dir::Up, delta.unsigned_abs()),
                    },
                    (Dir::Left, (end_step_out) as u32 + offset),
                ]),
            )
        }
        Dir::Left => {
            let (start_step_out, end_step_out) = match end.x - start.x {
                delta if delta > 0 => (0, delta.abs()),
                delta => (delta.abs(), 0),
            };

            (
                start,
                Vec::from([
                    (Dir::Left, start_step_out as u32 + 1 + offset),
                    match start.y - end.y {
                        delta if delta < 0 => (Dir::Down, delta.unsigned_abs()),
                        delta => (Dir::Up, delta.unsigned_abs()),
                    },
                    (Dir::Right, end_step_out as u32 + offset),
                ]),
            )
        }
        Dir::Down => {
            let (start_step_out, end_step_out) = match end.y - start.y {
                delta if delta > 0 => (delta.abs(), 0),
                delta => (0, delta.abs()),
            };

            (
                start,
                Vec::from([
                    (Dir::Down, start_step_out as u32 + 1 + offset),
                    match start.x - end.x {
                        delta if delta < 0 => (Dir::Right, delta.unsigned_abs()),
                        delta => (Dir::Left, delta.unsigned_abs()),
                    },
                    (Dir::Up, end_step_out as u32 + offset),
                ]),
            )
        }
        Dir::Up => {
            let (start_step_out, end_step_out) = match end.y - start.y {
                delta if delta > 0 => (0, delta.abs()),
                delta => (delta.abs(), 0),
            };

            (
                start,
                Vec::from([
                    (Dir::Up, start_step_out as u32 + 1 + offset),
                    match start.x - end.x {
                        delta if delta < 0 => (Dir::Right, delta.unsigned_abs()),
                        delta => (Dir::Left, delta.unsigned_abs()),
                    },
                    (Dir::Down, end_step_out as u32 + offset),
                ]),
            )
        }
    }
}

/// Corner route: one horizontal and one vertical segment meeting at a single bend.
///
/// * `start`      — the inclusive start cell of the path.
/// * `end`        — the inclusive end cell of the path.
/// * `start_axis` — which axis the first run travels along:
///   - [`Axis::Horizontal`]: go horizontally to `end.x`, then vertically to `end.y`.
///   - [`Axis::Vertical`]:   go vertically to `end.y`, then horizontally to `end.x`.
///
/// The `+1` on run1 follows the same inclusive-start convention as `c_shape` and
/// `s_shape`: PathIter emits `start` without advancing, so `distance + 1` steps
/// are needed to land on the bend column/row.  Run2 starts advancing from the
/// bend cell, so no `+1` is needed.
fn corner(start: SPoint, end: SPoint, start_axis: Axis) -> (SPoint, Vec<(Dir, u32)>) {
    match start_axis {
        Axis::Horizontal => {
            let run1_dir = if end.x >= start.x {
                Dir::Right
            } else {
                Dir::Left
            };
            let run2_dir = if end.y >= start.y { Dir::Down } else { Dir::Up };
            (
                start,
                vec![
                    (run1_dir, (end.x - start.x).unsigned_abs() + 1),
                    (run2_dir, (end.y - start.y).unsigned_abs()),
                ],
            )
        }
        Axis::Vertical => {
            let run1_dir = if end.y >= start.y { Dir::Down } else { Dir::Up };
            let run2_dir = if end.x >= start.x {
                Dir::Right
            } else {
                Dir::Left
            };
            (
                start,
                vec![
                    (run1_dir, (end.y - start.y).unsigned_abs() + 1),
                    (run2_dir, (end.x - start.x).unsigned_abs()),
                ],
            )
        }
    }
}

// ── Segment direction ─────────────────────────────────────────────────────────

/// Classifies the direction of a single straight segment.
pub fn seg_dir(from: SPoint, to: SPoint) -> Dir {
    if to.x > from.x {
        Dir::Right
    } else if to.x < from.x {
        Dir::Left
    } else if to.y > from.y {
        Dir::Down
    } else {
        Dir::Up
    }
}

fn compute_corner_symbold(prev: Dir, next: Dir) -> PathSymbol {
    match (prev, next) {
        (Dir::Left, Dir::Down) | (Dir::Up, Dir::Right) => PathSymbol::CornerTopLeft,
        (Dir::Right, Dir::Down) | (Dir::Up, Dir::Left) => PathSymbol::CornerTopRight,
        (Dir::Down, Dir::Right) | (Dir::Left, Dir::Up) => PathSymbol::CornerBottomLeft,
        (Dir::Down, Dir::Left) | (Dir::Right, Dir::Up) => PathSymbol::CornerBottomRight,
        _ => dir_to_symbol(next),
    }
}

fn dir_to_arrow(dir: Dir) -> PathSymbol {
    match dir {
        Dir::Right => PathSymbol::ArrowRight,
        Dir::Left => PathSymbol::ArrowLeft,
        Dir::Down => PathSymbol::ArrowDown,
        Dir::Up => PathSymbol::ArrowUp,
    }
}

fn dir_to_symbol(dir: Dir) -> PathSymbol {
    match dir {
        Dir::Right | Dir::Left => PathSymbol::Horizontal,
        Dir::Down | Dir::Up => PathSymbol::Vertical,
    }
}

fn reverse_dir(dir: Dir) -> Dir {
    match dir {
        Dir::Right => Dir::Left,
        Dir::Left => Dir::Right,
        Dir::Down => Dir::Up,
        Dir::Up => Dir::Down,
    }
}

// ── Bounds ────────────────────────────────────────────────────────────────────

/// Compute the bounding `SRect` of a path by walking the run endpoints.
///
/// Only the waypoint at the end of each run is visited (O(runs), not O(cells)),
/// which is sufficient because axis-aligned segments cannot extend beyond their
/// own endpoints.
fn bounds_from_runs(start: SPoint, runs: &[(Dir, u32)]) -> SRect {
    let mut bounds = SRect::new(start.x, start.y, 1, 1);
    let mut pos = start;
    for (i, &(dir, steps)) in runs.iter().enumerate() {
        // Run 0 has an inclusive start (PathIter emits `start` without
        // advancing), so its net displacement is `steps - 1`.
        // Subsequent runs each start at the previous corner cell and advance
        // `steps` more positions, so their displacement is `steps`.
        let dist = if i == 0 {
            steps as i32 - 1
        } else {
            steps as i32
        };
        pos = match dir {
            Dir::Right => SPoint::new(pos.x + dist, pos.y),
            Dir::Left => SPoint::new(pos.x - dist, pos.y),
            Dir::Down => SPoint::new(pos.x, pos.y + dist),
            Dir::Up => SPoint::new(pos.x, pos.y - dist),
        };
        bounds = bounds.extend_to(pos);
    }
    bounds
}

// ── PathIter ──────────────────────────────────────────────────────────────────

/// Lazy iterator yielding `(SPoint, PathSymbol)` pairs for a rendered path.
///
/// Constructed by [`calculate_path`].  Owns all required state so no lifetime
/// is needed.  Corners are emitted *instead of* the straight-run cell at the
/// bend point (no duplicate positions), and arrowheads are yielded last.
#[derive(Clone)]
pub struct PathIter {
    // ── fixed inputs ──────────────────────────────────────────────────────────
    runs: Vec<(Dir, u32)>,
    arrow: ArrowDecorations,
    // /// Direction of the first run — determines backward arrowhead orientation.
    // first_dir: Dir,
    // /// Direction of the last run — determines forward arrowhead orientation.
    // last_dir: Dir,
    // /// Second cell of the path (one step after `start`) — backward arrowhead pos.
    // second_cell: SPoint,
    // /// Second-to-last cell of the path — forward arrowhead pos.
    // tip_prev: SPoint,

    // ── mutable iteration state ───────────────────────────────────────────────
    current: SPoint,
    /// Index into `runs`.
    run_index: usize,
    /// Steps already taken within the current run.
    completed_steps: u32,
    prev_dir: Option<Dir>,

    /// Note that those can be different from runs due to empty runs (edge cases)
    ///   so it makes sense to explicitly capture those
    start_end_dir: (Dir, Dir),
}

impl PathIter {
    fn new(start: SPoint, mut runs: Vec<(Dir, u32)>, arrow: ArrowDecorations) -> Self {
        let start_end_dir = (
            runs.first().map(|(dir, _)| *dir).unwrap_or(Dir::Left),
            runs.last().map(|(dir, _)| *dir).unwrap_or(Dir::Left),
        );
        runs.retain(|&(_, steps)| steps > 0);
        Self {
            runs,
            arrow,
            current: start,
            run_index: 0,
            completed_steps: 0,
            prev_dir: None,
            start_end_dir,
        }
    }
}

impl Iterator for PathIter {
    type Item = (SPoint, PathSymbol);

    fn next(&mut self) -> Option<Self::Item> {
        if self.runs.is_empty() {
            return None;
        }

        // println!("here");

        if self.run_index >= self.runs.len() {
            // that means that we completed all runs
            // so nothing left to do
            return None;
        }

        // now we are not in the beginning, hence
        //   we need get to the current run
        //   then position within
        //   and then
        let (dir, steps) = self.runs[self.run_index];
        debug_assert!(steps > 0, "zero-step run passed to PathIter");

        let symbol = if self.prev_dir.is_none() {
            self.prev_dir = Some(dir);
            // Note that the very first step we don't progress the current position, because it is inclusive
            self.current = self.current + reverse_dir(dir);

            let symbol_dir = self.start_end_dir.0;
            match self.arrow {
                // that means that we are at the very begining, so check if we need an arrow
                ArrowDecorations::Backward | ArrowDecorations::Both => {
                    dir_to_arrow(reverse_dir(symbol_dir))
                }
                _ => dir_to_symbol(dir),
            }
        } else {
            dir_to_symbol(dir)
        };

        // println!(
        //     "--run: {dir:?}, steps={steps}, completed={}",
        //     self.completed_steps
        // );

        if steps <= self.completed_steps + 1 {
            // that means that we on the last step in the run, hence we need to check how and if we change the direction
            self.completed_steps = 0;
            self.current = self.current + dir;
            // but it can also be the last step period
            let is_last_run = self.run_index == self.runs.len() - 1;
            let current_run_index = self.run_index;
            self.run_index += 1;

            // println!(
            //     "--end of run: {dir:?}, steps={steps}, is_last_run={}",
            //     is_last_run
            // );

            let symbol_dir = self.start_end_dir.1;
            if is_last_run {
                let symbol = match self.arrow {
                    ArrowDecorations::Forward | ArrowDecorations::Both => dir_to_arrow(symbol_dir),
                    _ => dir_to_symbol(dir),
                };
                return Some((self.current, symbol));
            }

            // that means that we are about to change direction
            // for that we need to get it first
            let (next_dir, _) = self.runs[current_run_index + 1];
            return Some((self.current, compute_corner_symbold(dir, next_dir)));
        }

        // here we know that we can just safely make another step, we checked the corner cases above
        self.current = self.current + dir;
        self.completed_steps += 1;
        Some((self.current, symbol))
    }
}

// ── ConnectorShape ────────────────────────────────────────────────────────────

/// Contextual details captured when a connector shape is not yet implemented.
///
/// Stored inside [`ConnectorShape::NotImplemented`] and also carried as the
/// payload of [`PathError::NotImplemented`] so that callers can inspect or
/// display the missing configuration.
#[derive(Debug, PartialEq, Clone)]
pub struct NotImplementedDetails {
    pub from_rect: SRect,
    pub from_side: Side,
    pub to_rect: SRect,
    pub to_side: Side,
    pub start: SPoint,
    pub end: SPoint,
}

/// Error returned by [`calculate_path`] when the path cannot be computed.
#[derive(Debug, PartialEq)]
pub enum PathError {
    /// One or both node IDs referenced by the edge were not found in `nodes`.
    NodeNotFound,
    /// The connector shape for this combination of sides / positions is not
    /// yet implemented.  The inner value describes the failing configuration.
    NotImplemented(NotImplementedDetails),
}

/// Describes which shape function to invoke and with what arguments to render
/// a connector between two nodes.
///
/// This is the output of [`classify_shape`] and the input consumed by
/// [`calculate_path`].  Separating classification from rendering makes each
/// half independently testable.
#[derive(Debug, PartialEq, Clone)]
pub enum ConnectorShape {
    /// C-shaped route: both stubs leave in the same direction, then wrap around.
    CShape {
        start: SPoint,
        end: SPoint,
        step_out_dir: Dir,
        // offset: u16,
    },
    /// S-shaped route: stubs leave in opposite directions along one axis,
    /// connected by a perpendicular jog in the middle.
    /// note that for edge case of vertical or horizontal lines (delta in x or y is 0) use LineWithPointedEnds
    SShape {
        start: SPoint,
        axis: Axis,
        end: SPoint,
    },
    /// L-shaped corner route: one horizontal + one vertical run.
    Corner {
        start: SPoint,
        end: SPoint,
        start_axis: Axis,
    },
    /// A straight single-segment line.
    ///
    /// * `start` — inclusive start cell.
    /// * `dir`   — direction of travel.
    /// * `len`   — number of additional steps after `start` (so the total
    ///             number of cells occupied is `len + 1`).
    Line { start: SPoint, dir: Dir, len: u16 },

    /// Same as Line but has ability to specify turns at the ends (useful for edgecases)
    LineWithPointedEnds {
        start: SPoint,
        dir: Dir,
        len: u16,
        pointed_ends: (Dir, Dir),
    },
    /// Placeholder for a connector configuration that has not yet been
    /// implemented.  Carries all the context needed to route the connection so
    /// that it can be displayed as a diagnostic or used to implement the shape
    /// later.
    NotImplemented(NotImplementedDetails),
    /// A composite of multiple primitive shapes chained end-to-end.
    ///
    /// The end point of each shape must equal the start point of the next.
    /// When converted to runs the shared junction point is emitted only once:
    /// the inclusive start of each subsequent shape is dropped by decrementing
    /// its first run's step count by one.
    Composite(Vec<ConnectorShape>),
}

/// Walk a `(start, runs)` pair and return the final position reached.
///
/// PathIter uses an inclusive start: the very first cell of run 0 is `start`
/// itself (no advance), so run 0 displaces by `steps - 1`.  Each subsequent
/// run begins at the corner emitted by the previous run and advances `steps`
/// more cells, so runs 1+ displace by `steps`.
fn end_point_from_runs(start: SPoint, runs: &[(Dir, u32)]) -> SPoint {
    let mut pos = start;
    for (i, &(dir, steps)) in runs.iter().enumerate() {
        let dist = if i == 0 {
            steps as i32 - 1
        } else {
            steps as i32
        };
        pos = match dir {
            Dir::Right => SPoint::new(pos.x + dist, pos.y),
            Dir::Left => SPoint::new(pos.x - dist, pos.y),
            Dir::Down => SPoint::new(pos.x, pos.y + dist),
            Dir::Up => SPoint::new(pos.x, pos.y - dist),
        };
    }
    pos
}

impl ConnectorShape {
    /// Return the inclusive start point of this shape.
    pub fn start(&self) -> SPoint {
        match self {
            ConnectorShape::CShape { start, .. } => *start,
            ConnectorShape::SShape { start, .. } => *start,
            ConnectorShape::Corner { start, .. } => *start,
            ConnectorShape::Line { start, .. } => *start,
            ConnectorShape::LineWithPointedEnds { start, .. } => *start,
            ConnectorShape::NotImplemented(d) => d.start,
            ConnectorShape::Composite(shapes) => shapes[0].start(),
        }
    }

    /// Convert this shape into a `(start, runs)` pair by calling the
    /// appropriate low-level shape builder.
    ///
    /// This is the single place that maps `ConnectorShape` variants to their
    /// builder functions, keeping both [`calculate_path`] and tests DRY.
    pub fn into_runs(self) -> (SPoint, Vec<(Dir, u32)>) {
        match self {
            ConnectorShape::CShape {
                start,
                end,
                step_out_dir: dir,
            } => c_shape(start, end, dir, DEFAULT_CSHAPE_STICKOUT as u32),
            ConnectorShape::SShape { start, axis, end } => s_shape(start, axis, end),
            ConnectorShape::Corner {
                start,
                end,
                start_axis,
            } => corner(start, end, start_axis),
            ConnectorShape::Line { start, dir, len } => (start, vec![(dir, len as u32)]),
            ConnectorShape::LineWithPointedEnds {
                start,
                dir,
                len,
                pointed_ends,
            } => (
                start,
                vec![(pointed_ends.0, 0), (dir, len as u32), (pointed_ends.1, 0)],
            ),
            ConnectorShape::NotImplemented(d) => (d.start, vec![]),
            ConnectorShape::Composite(shapes) => {
                assert!(
                    !shapes.is_empty(),
                    "Composite must contain at least one shape"
                );

                let overall_start = shapes[0].start();
                let mut merged: Vec<(Dir, u32)> = Vec::new();

                for (i, shape) in shapes.into_iter().enumerate() {
                    let (shape_start, mut runs) = shape.into_runs();

                    if i == 0 {
                        merged.extend(runs);
                    } else {
                        // The start of this shape is the same cell as the end of
                        // the previous shape (PathIter uses inclusive starts), so
                        // we must not emit it again.  Decrement the first run's
                        // step count to skip that shared junction cell.
                        debug_assert!(!runs.is_empty(), "Composite sub-shape {i} produced no runs");
                        debug_assert_eq!(
                            {
                                // end of the previous merged path so far
                                end_point_from_runs(overall_start, &merged)
                            },
                            shape_start,
                            "Composite shape {i} start does not align with the end of the previous shape"
                        );
                        runs[0].1 -= 1;
                        merged.extend(runs);
                    }
                }

                (overall_start, merged)
            }
        }
    }
}

// ── Path classification (ordered) ────────────────────────────────────────────

/// Decide which [`ConnectorShape`] to use given a pre-sorted [`OrderedEndpoints`].
///
/// The outer `match` dispatches on the spatial [`PointRelation`] between the
/// two connection points.  The inner `match` dispatches on the pair of exit
/// sides.  Because [`sort_endpoints`] has already established the canonical
/// ordering (start is always top-most then left-most), many of the geometric
/// guard conditions present in [`classify_shape`] are now structural.
///
/// Every `(relation, start_side, end_side)` triple must be covered; gaps that
/// are not yet routed panic with `unreachable!` to make them visible immediately
/// during development.  Only `Coincident` returns [`ConnectorShape::NotImplemented`].
pub fn classify_shape_ordered(ep: &OrderedEndpoints) -> ConnectorShape {
    let (start_side, start_rect) = ep.start;
    let (end_side, end_rect) = ep.end;
    let start = rect_connection_point(start_rect, start_side);
    let end = rect_connection_point(end_rect, end_side);
    let start_dir = side_to_dir(start_side);

    use Side::*;
    match ep.relation {
        // ── Coincident ────────────────────────────────────────────────────
        PointRelation::Coincident => ConnectorShape::Line {
            start,
            dir: side_to_dir(start_side),
            len: 1,
        },

        // ── Above ─────────────────────────────────────────────────────────
        // start.x == end.x, start.y < end.y  (start directly above end)
        PointRelation::Above => classify_above(start_side, end_side, start, end, start_dir),

        // ── LeftOf ────────────────────────────────────────────────────────
        // start.y == end.y, start.x < end.x  (start directly left of end)
        PointRelation::LeftOf => classify_left_of((start_rect, start_side), (end_rect, end_side)),

        // ── AboveLeft ─────────────────────────────────────────────────────
        // start.x < end.x && start.y < end.y  (start above and to the left)
        PointRelation::AboveLeft => {
            classify_above_left((start_rect, start_side), (end_rect, end_side))
        }

        // ── AboveRight ────────────────────────────────────────────────────
        // start.x > end.x && start.y < end.y  (start above and to the right)
        PointRelation::AboveRight => {
            classify_above_right((start_rect, start_side), (end_rect, end_side))
        }
    }
}

fn classify_above_right(
    (start_rect, start_side): (SRect, Side),
    (end_rect, end_side): (SRect, Side),
) -> ConnectorShape {
    let start = rect_connection_point(start_rect, start_side);
    let end = rect_connection_point(end_rect, end_side);
    use ConnectorShape as CS;
    use Side::*;
    match (start_side, end_side) {
        // Same side → C-shape.
        (Top, Top) | (Right, Right) | (Bottom, Bottom) | (Left, Left) => CS::CShape {
            start,
            end,
            step_out_dir: side_to_dir(start_side),
        },

        // Opposing horizontal stubs → S-shape horizontal.
        (Side::Left, Side::Right) => CS::SShape {
            start,
            axis: Axis::Horizontal,
            end,
        },

        // Opposing vertical stubs → S-shape vertical.
        (Side::Bottom, Side::Top) => CS::SShape {
            start,
            axis: Axis::Vertical,
            end,
        },

        (Side::Right, Side::Left) => classify_horizontal_connectors_sticking_out(
            (start_rect, start_side),
            (end_rect, end_side),
        ),

        // Stubs face away vertically → C-shape.
        (Side::Top, Side::Bottom) => CS::CShape {
            start,
            end,
            step_out_dir: Dir::Up,
        },

        // Right → Top with start to the right of end: Composite (Line + CShape).
        //
        //   +---------------+
        //   |               |
        //  xxx              |
        //  xxx              |
        //  xxx              |
        //               xxx |
        //               xSx-+
        //               xxx
        //
        (Side::Right, Side::Top) => CS::Composite(Vec::from([
            CS::Line {
                start,
                dir: Dir::Right,
                len: DEFAULT_CSHAPE_STICKOUT,
            },
            CS::CShape {
                start: start + (DEFAULT_CSHAPE_STICKOUT as i32, 0),
                end,
                step_out_dir: Dir::Up,
            },
        ])),

        // Horizontal-first corners.
        (Side::Right, Side::Bottom) | (Side::Left, Side::Top) | (Side::Left, Side::Bottom) => {
            CS::Corner {
                start,
                end,
                start_axis: Axis::Horizontal,
            }
        }

        // Vertical-first corners.
        (Side::Top, Side::Right)
        | (Side::Top, Side::Left)
        | (Side::Bottom, Side::Right)
        | (Side::Bottom, Side::Left) => CS::Corner {
            start,
            end,
            start_axis: Axis::Vertical,
        },
    }
}

fn classify_horizontal_connectors_sticking_out(
    (start_rect, start_side): (SRect, Side),
    (end_rect, end_side): (SRect, Side),
) -> ConnectorShape {
    use ConnectorShape as CS;

    let start = rect_connection_point(start_rect, start_side);
    let end = rect_connection_point(end_rect, end_side);

    let start_rect_bottom = rect_connection_point(start_rect, Side::Bottom).y;
    let end_rect_top = rect_connection_point(end_rect, Side::Top).y;

    let enough_space_between = start_rect_bottom <= end_rect_top;

    let meeting_point: SPoint = match enough_space_between {
        true => SPoint::new(end.x, end_rect_top),

        // we are going to wrap around the smaller one
        //   which in that case is the starting rect
        //   hence, wrap it on top
        false if end_rect.size.height >= start_rect.size.height => {
            SPoint::new(end.x, rect_connection_point(start_rect, Side::Top).y)
        }

        //  and in that case is the end rect is smaller, hence wrap it on the bottom
        false => SPoint::new(end.x, rect_connection_point(end_rect, Side::Bottom).y),
    };

    CS::Composite(Vec::from([
        CS::CShape {
            start,
            end: meeting_point,
            step_out_dir: side_to_dir(start_side),
        },
        CS::CShape {
            start: meeting_point,
            end,
            step_out_dir: side_to_dir(end_side),
        },
    ]))
}

fn classify_above_left(
    (start_rect, start_side): (SRect, Side),
    (end_rect, end_side): (SRect, Side),
) -> ConnectorShape {
    let start = rect_connection_point(start_rect, start_side);
    let end = rect_connection_point(end_rect, end_side);

    use Side::*;
    match (start_side, end_side) {
        // Same side → C-shape.
        (Top, Top) | (Right, Right) | (Bottom, Bottom) | (Left, Left) => ConnectorShape::CShape {
            start,
            end,
            step_out_dir: side_to_dir(start_side),
        },

        // Opposing horizontal stubs → S-shape horizontal.
        (Side::Right, Side::Left) => ConnectorShape::SShape {
            start,
            axis: Axis::Horizontal,
            end,
        },

        // Opposing vertical stubs → S-shape vertical.
        (Side::Bottom, Side::Top) => ConnectorShape::SShape {
            start,
            axis: Axis::Vertical,
            end,
        },

        (Side::Left, Side::Right) => classify_horizontal_connectors_sticking_out(
            (start_rect, start_side),
            (end_rect, end_side),
        ),

        // Stubs face away vertically → C-shape.
        (Side::Top, Side::Bottom) => ConnectorShape::CShape {
            start,
            end,
            step_out_dir: Dir::Up,
        },

        // Horizontal-first corners.
        (Side::Right, Side::Top)
        | (Side::Right, Side::Bottom)
        | (Side::Left, Side::Top)
        | (Side::Left, Side::Bottom) => ConnectorShape::Corner {
            start,
            end,
            start_axis: Axis::Horizontal,
        },

        // Vertical-first corners.
        (Side::Top, Side::Right)
        | (Side::Top, Side::Left)
        | (Side::Bottom, Side::Right)
        | (Side::Bottom, Side::Left) => ConnectorShape::Corner {
            start,
            end,
            start_axis: Axis::Vertical,
        },
    }
}

fn classify_left_of(
    (start_rect, start_side): (SRect, Side),
    (end_rect, end_side): (SRect, Side),
) -> ConnectorShape {
    let start = rect_connection_point(start_rect, start_side);
    let end = rect_connection_point(end_rect, end_side);

    use Side::*;
    match (start_side, end_side) {
        // Same side → C-shape.
        (Top, Top) | (Right, Right) | (Bottom, Bottom) | (Left, Left) => ConnectorShape::CShape {
            start,
            end,
            step_out_dir: side_to_dir(start_side),
        },

        // Stubs face each other horizontally, same row → straight line.
        // pointed_ends: start exits Right; end travel direction is Right (for forward arrow).
        (Side::Right, Side::Left) => ConnectorShape::LineWithPointedEnds {
            start,
            dir: Dir::Right,
            len: (end.x - start.x) as u16,
            pointed_ends: (Dir::Right, Dir::Right),
        },

        (Side::Left, Side::Right) => classify_horizontal_connectors_sticking_out(
            (start_rect, start_side),
            (end_rect, end_side),
        ),

        // Vertical stubs with horizontal separation → L-shaped corner.
        (Side::Top, Side::Bottom)
        | (Side::Bottom, Side::Top)
        | (Side::Top, Side::Right)
        | (Side::Top, Side::Left)
        | (Side::Bottom, Side::Right)
        | (Side::Bottom, Side::Left) => ConnectorShape::Corner {
            start,
            end,
            start_axis: Axis::Vertical,
        },

        // Horizontal stubs with horizontal separation → L-shaped corner.
        (Side::Right, Side::Top)
        | (Side::Right, Side::Bottom)
        | (Side::Left, Side::Top)
        | (Side::Left, Side::Bottom) => ConnectorShape::Corner {
            start,
            end,
            start_axis: Axis::Horizontal,
        },
    }
}

fn classify_above(
    start_side: Side,
    end_side: Side,
    start: SPoint,
    end: SPoint,
    start_dir: Dir,
) -> ConnectorShape {
    use Side::*;
    match (start_side, end_side) {
        // Same side → C-shape in the shared exit direction.
        (Top, Top) | (Right, Right) | (Bottom, Bottom) | (Left, Left) => ConnectorShape::CShape {
            start,
            end,
            step_out_dir: start_dir,
        },

        // Stubs face each other vertically, same column → straight vertical line.
        // pointed_ends: start exits Down; end travel direction is Down (for forward arrow).
        (Side::Bottom, Side::Top) => ConnectorShape::LineWithPointedEnds {
            start,
            dir: Dir::Down,
            len: (end.y - start.y) as u16,
            pointed_ends: (Dir::Down, Dir::Down),
        },

        // Stubs face away vertically → C-shape upward from start.
        (Side::Top, Side::Bottom) => ConnectorShape::CShape {
            start,
            end,
            step_out_dir: Dir::Up,
        },

        // Horizontal stubs in same column → straight vertical line.
        // pointed_ends: start exits Right/Left (for backward arrow); end travel dir is Down.
        (Side::Right, Side::Left) => ConnectorShape::LineWithPointedEnds {
            start,
            dir: Dir::Down,
            len: (end.y - start.y) as u16,
            pointed_ends: (Dir::Right, Dir::Down),
        },
        (Side::Left, Side::Right) => ConnectorShape::LineWithPointedEnds {
            start,
            dir: Dir::Down,
            len: (end.y - start.y) as u16,
            pointed_ends: (Dir::Left, Dir::Down),
        },

        (Side::Right, Side::Top)
        | (Side::Right, Side::Bottom)
        | (Side::Left, Side::Top)
        | (Side::Left, Side::Bottom) => ConnectorShape::Corner {
            start,
            end,
            start_axis: Axis::Horizontal,
        },

        // Vertical stubs with vertical separation → L-shaped corner.
        (Side::Top, Side::Right)
        | (Side::Top, Side::Left)
        | (Side::Bottom, Side::Right)
        | (Side::Bottom, Side::Left) => ConnectorShape::Corner {
            start,
            end,
            start_axis: Axis::Vertical,
        },
        // _ => unreachable!("Above: same-side covered by the first arm"),
    }
}

// ── Path calculation ──────────────────────────────────────────────────────────

/// Builds the path for `edge`, returning a lazy [`PathIter`] of
/// `(SPoint, PathSymbol)` pairs and the bounding [`SRect`] of the path.
///
/// # Errors
///
/// Returns [`PathError::NodeNotFound`] when either node referenced by the edge
/// does not exist in `nodes`, or [`PathError::NotImplemented`] when the
/// routing for this combination of sides / positions is not yet implemented.
pub fn calculate_path(nodes: &[Node], edge: &Edge) -> Result<(PathIter, SRect), PathError> {
    let from_node = nodes
        .iter()
        .find(|n| n.id == edge.from_id)
        .ok_or(PathError::NodeNotFound)?;
    let to_node = nodes
        .iter()
        .find(|n| n.id == edge.to_id)
        .ok_or(PathError::NodeNotFound)?;

    let ordered_endpoints =
        sort_endpoints(from_node, edge.from_side, to_node, edge.to_side, edge.dir);

    let shape = classify_shape_ordered(&ordered_endpoints);
    // println!("calculate_path, {ordered_endpoints:#?}\n{shape:#?}");

    // Surface NotImplemented before trying to build runs.
    if let ConnectorShape::NotImplemented(details) = shape {
        return Err(PathError::NotImplemented(details));
    }

    let (start, runs) = shape.into_runs();
    let bounds = bounds_from_runs(start, &runs);
    let iter = PathIter::new(start, runs, ordered_endpoints.arrow);
    Ok((iter, bounds))
}

/// Classify the [`ConnectorShape`] for `edge` without converting it to runs.
///
/// This is the same classification pipeline used by [`calculate_path`], exposed
/// separately so callers (e.g. the debug UI) can inspect the shape without
/// paying the cost of building the full run list.
///
/// Returns `None` if either node referenced by the edge is not found in `nodes`.
pub fn classify_path(nodes: &[Node], edge: &Edge) -> Option<(ConnectorShape, OrderedEndpoints)> {
    let from_node = nodes.iter().find(|n| n.id == edge.from_id)?;
    let to_node = nodes.iter().find(|n| n.id == edge.to_id)?;
    let ep = sort_endpoints(from_node, edge.from_side, to_node, edge.to_side, edge.dir);
    Some((classify_shape_ordered(&ep), ep))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::state::NodeId;

    use super::*;
    use indoc::indoc;

    // ── Test renderer ─────────────────────────────────────────────────────────

    /// Render `(start, runs)` into a grid string.
    ///
    /// The bounding box is computed from the runs themselves and the grid is
    /// automatically sized and translated so the top-left of the path lands at
    /// (0, 0).  Each row is wrapped in `x` delimiters; the whole block is
    /// bordered top and bottom with `x` rows.
    fn render_path_arrow(start: SPoint, runs: Vec<(Dir, u32)>, arrow: ArrowDecorations) -> String {
        let bounds = bounds_from_runs(start, &runs);
        let ox = bounds.origin.x;
        let oy = bounds.origin.y;
        let w = bounds.size.width as usize;
        let h = bounds.size.height as usize;

        let mut grid = vec![vec![' '; w]; h];

        for (i, (pt, sym)) in PathIter::new(start, runs, arrow).enumerate() {
            // dbg!((pt, sym));
            assert!(
                i < 100,
                "PathIter exceeded 100 items — likely an infinite loop"
            );
            dbg!((i, pt, sym));
            let px = (pt.x - ox) as usize;
            let py = (pt.y - oy) as usize;
            if pt.x >= ox && pt.y >= oy && px < w && py < h {
                grid[py][px] = sym.to_ascii();
            }
        }

        let row_width = w + 2;
        let border = "x".repeat(row_width);
        let rows: Vec<String> = grid
            .iter()
            .map(|row| {
                let inner: String = row.iter().collect();
                format!("x{inner}x")
            })
            .collect();
        format!("{border}\n{}\n{border}", rows.join("\n"))
    }

    fn render_path(start: SPoint, runs: Vec<(Dir, u32)>) -> String {
        render_path_arrow(start, runs, ArrowDecorations::Forward)
    }

    /// Render a [`ConnectorShape`] together with the two nodes it connects.
    ///
    /// The bounding box is the union of all node rects and the path bounds,
    /// translated so that its top-left lands at (0, 0).  Nodes are drawn as
    /// box-drawing borders with the node id digit in the centre cell; the
    /// connector path is drawn on top (path symbols overwrite node borders
    /// only where the stub exits the border cell, which is intentional).
    ///
    /// Nodes must be at least 3×3.  The node label is used as the id char;
    /// `make_node` sets it to the stringified `id` argument.
    fn render_scene(nodes: &[&Node], shape: ConnectorShape) -> String {
        let (start, runs) = shape.into_runs();
        let path_bounds = bounds_from_runs(start, &runs);

        // Union of all node rects and the path bounds.
        let scene_bounds = nodes.iter().fold(path_bounds, |acc, n| {
            acc.extend_to(n.rect.top_left())
                .extend_to(n.rect.bottom_right())
        });
        let ox = scene_bounds.origin.x;
        let oy = scene_bounds.origin.y;
        let w = scene_bounds.size.width as usize;
        let h = scene_bounds.size.height as usize;

        let mut grid = vec![vec![' '; w]; h];

        // Draw nodes first (path symbols are painted on top afterwards).
        for node in nodes {
            let id_char = node.label.chars().next().unwrap_or('?');
            let nx = (node.rect.origin.x - ox) as usize;
            let ny = (node.rect.origin.y - oy) as usize;
            let nw = node.rect.size.width as usize;
            let nh = node.rect.size.height as usize;

            for row in 0..nh {
                for col in 0..nw {
                    let ch = match (row, col) {
                        // corners
                        (r, c) if r == 0 && c == 0 => '+',
                        (r, c) if r == 0 && c == nw - 1 => '+',
                        (r, c) if r == nh - 1 && c == 0 => '+',
                        (r, c) if r == nh - 1 && c == nw - 1 => '+',
                        // top / bottom edges
                        (r, _) if r == 0 || r == nh - 1 => '-',
                        // left / right edges
                        (_, c) if c == 0 || c == nw - 1 => '|',
                        // centre of a 3×3 cell — id digit
                        (r, c) if r == nh / 2 && c == nw / 2 => id_char,
                        _ => ' ',
                    };
                    grid[ny + row][nx + col] = ch;
                }
            }
        }

        // Paint the path on top.
        for (i, (pt, sym)) in PathIter::new(start, runs, ArrowDecorations::Forward).enumerate() {
            assert!(
                i < 100,
                "PathIter exceeded 100 items — likely an infinite loop"
            );
            let px = (pt.x - ox) as usize;
            let py = (pt.y - oy) as usize;
            if pt.x >= ox && pt.y >= oy && px < w && py < h {
                grid[py][px] = sym.to_ascii();
            }
        }

        let row_width = w + 2;
        let border = "x".repeat(row_width);
        let rows: Vec<String> = grid
            .iter()
            .map(|row| {
                let inner: String = row.iter().collect();
                format!("x{inner}x")
            })
            .collect();
        format!("{border}\n{}\n{border}", rows.join("\n"))
    }

    // ── Straight lines ────────────────────────────────────────────────────────
    #[test]
    fn horizontal_line() {
        assert_eq!(
            render_path(SPoint::new(0, 0), vec![(Dir::Right, 4)]),
            indoc! {"
                xxxxxx
                x--->x
                xxxxxx"}
        );
    }

    #[test]
    fn horizontal_short_composite_line() {
        assert_eq!(
            render_path(SPoint::new(0, 0), vec![(Dir::Right, 1), (Dir::Right, 1)]),
            indoc! {"
                xxxx
                x->x
                xxxx"}
        );
    }

    #[test]
    fn horizontal_short_one_symbol_path() {
        assert_eq!(
            render_path(SPoint::new(0, 0), vec![(Dir::Right, 1)]),
            indoc! {"
                xxx
                x>x
                xxx"}
        );
    }

    #[test]
    fn vertical_line() {
        assert_eq!(
            render_path(SPoint::new(0, 0), vec![(Dir::Down, 3)]),
            indoc! {"
                xxx
                x|x
                x|x
                xvx
                xxx"}
        );
    }

    #[test]
    fn corner_right_then_down() {
        // Right 5 then Down 3: bend at (5,0)
        assert_eq!(
            render_path(SPoint::new(0, 0), vec![(Dir::Right, 5), (Dir::Down, 3)]),
            indoc! {"
                xxxxxxx
                x----+x
                x    |x
                x    |x
                x    vx
                xxxxxxx"}
        );
    }

    #[test]
    fn corner_down_then_right() {
        // Down 2 then Right 5: bend at (0,2)
        assert_eq!(
            render_path(SPoint::new(0, 0), vec![(Dir::Down, 2), (Dir::Right, 5)]),
            indoc! {"
                xxxxxxxx
                x|     x
                x+---->x
                xxxxxxxx"}
        );
    }

    // ── Corner (function) ─────────────────────────────────────────────────────

    // Horizontal first, end is right and below: Right then Down.
    // run1: Right 5+1=6 steps to land on col 5, run2: Down 3 steps.
    #[test]
    fn corner_horizontal_right_down() {
        let res = corner(SPoint::new(0, 0), SPoint::new(5, 3), Axis::Horizontal);
        let expected = (SPoint::new(0, 0), vec![(Dir::Right, 6), (Dir::Down, 3)]);
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxxx
                x-----+x
                x     |x
                x     |x
                x     vx
                xxxxxxxx"}
        );
    }

    // Horizontal first, end is left and above: Left then Up.
    // run1: Left 4+1=5 to land on col 0, run2: Up 2.
    #[test]
    fn corner_horizontal_left_up() {
        let res = corner(SPoint::new(4, 2), SPoint::new(0, 0), Axis::Horizontal);
        let expected = (SPoint::new(4, 2), vec![(Dir::Left, 5), (Dir::Up, 2)]);
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxx
                x^    x
                x|    x
                x+----x
                xxxxxxx"}
        );
    }

    // Vertical first, end is right and below: Down then Right.
    // run1: Down 3+1=4 to land on row 3, run2: Right 5.
    #[test]
    fn corner_vertical_down_right() {
        let res = corner(SPoint::new(0, 0), SPoint::new(5, 3), Axis::Vertical);
        let expected = (SPoint::new(0, 0), vec![(Dir::Down, 4), (Dir::Right, 5)]);
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxxx
                x|     x
                x|     x
                x|     x
                x+---->x
                xxxxxxxx"}
        );
    }

    // Vertical first, end is left and above: Up then Left.
    // run1: Up 2+1=3 to land on row 0, run2: Left 4.
    #[test]
    fn corner_vertical_up_left() {
        let res = corner(SPoint::new(4, 2), SPoint::new(0, 0), Axis::Vertical);
        let expected = (SPoint::new(4, 2), vec![(Dir::Up, 3), (Dir::Left, 4)]);
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxx
                x<---+x
                x    |x
                x    |x
                xxxxxxx"}
        );
    }

    // ── S-shape ───────────────────────────────────────────────────────────────

    // Horizontal stubs, end is right and below start.
    // mid_x = (0+9)/2 = 4
    // runs: Right 5 (+1 to land on col 4), Down 2, Right 5 (exact distance from col 4 to col 9)
    #[test]
    fn s_shape_horizontal_right_down() {
        let res = s_shape(SPoint::new(0, 0), Axis::Horizontal, SPoint::new(9, 2));
        let expected = (
            SPoint::new(0, 0),
            vec![(Dir::Right, 5), (Dir::Down, 2), (Dir::Right, 5)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxxxxxxx
                x----+     x
                x    |     x
                x    +---->x
                xxxxxxxxxxxx"}
        );
    }

    // Horizontal stubs, end is right and above start.
    // mid_x = (0+9)/2 = 4
    // runs: Right 5 (+1), Up 2, Right 5 (exact)
    #[test]
    fn s_shape_horizontal_right_up() {
        let res = s_shape(SPoint::new(0, 2), Axis::Horizontal, SPoint::new(9, 0));
        let expected = (
            SPoint::new(0, 2),
            vec![(Dir::Right, 5), (Dir::Up, 2), (Dir::Right, 5)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxxxxxxx
                x    +---->x
                x    |     x
                x----+     x
                xxxxxxxxxxxx"}
        );
    }

    // Horizontal stubs going leftward, end is left and below start.
    // mid_x = (9+0)/2 = 4
    // runs: Left 6 (+1 to land on col 4), Down 2, Left 4 (exact distance from col 4 to col 0)
    #[test]
    fn s_shape_horizontal_left_down() {
        let res = s_shape(SPoint::new(9, 0), Axis::Horizontal, SPoint::new(0, 2));
        let expected = (
            SPoint::new(9, 0),
            vec![(Dir::Left, 6), (Dir::Down, 2), (Dir::Left, 4)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxxxxxxx
                x    +-----x
                x    |     x
                x<---+     x
                xxxxxxxxxxxx"}
        );
    }

    // Horizontal stubs, degenerate: same x — mid_x == start.x == end.x.
    // mid_x = (5+5)/2 = 5
    // runs: Right 1 (+1, distance=0), Down 4, Right 0 (exact, distance=0)
    #[test]
    fn s_shape_horizontal_degenerate_same_x() {
        let res = s_shape(SPoint::new(5, 0), Axis::Horizontal, SPoint::new(5, 4));
        let expected = (
            SPoint::new(5, 0),
            vec![(Dir::Right, 1), (Dir::Down, 4), (Dir::Right, 0)],
        );
        assert_eq!(res, expected);
    }

    // Horizontal stubs, degenerate: same y — start and end on the same
    // horizontal line.  The S-shape collapses to a straight horizontal line
    // because the middle vertical jog has 0 steps.
    //
    // mid_x = (0+8)/2 = 4
    // runs: Right 5 (+1 to land on col 4), Down 0, Right 4 (exact from col 4 to col 8)
    //
    // We only assert the runs here — PathIter::new strips zero-step runs and
    // fires a debug_assert for each one, so calling render_path in a debug
    // build would panic.  The rendering behaviour (a plain horizontal line)
    // is a consequence of the zero-step run being filtered out.
    #[test]
    fn s_shape_horizontal_degenerate_same_y() {
        let res = s_shape(SPoint::new(0, 0), Axis::Horizontal, SPoint::new(8, 0));
        assert_eq!(
            res,
            (
                SPoint::new(0, 0),
                vec![(Dir::Right, 5), (Dir::Down, 0), (Dir::Right, 4)],
            )
        );
    }

    // Vertical stubs, end is right and below start.
    // mid_y = (0+4)/2 = 2
    // runs: Down 3 (+1 to land on row 2), Right 4, Down 2 (exact distance from row 2 to row 4)
    #[test]
    fn s_shape_vertical_down_right() {
        let res = s_shape(SPoint::new(0, 0), Axis::Vertical, SPoint::new(4, 4));
        let expected = (
            SPoint::new(0, 0),
            vec![(Dir::Down, 3), (Dir::Right, 4), (Dir::Down, 2)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxx
                x|    x
                x|    x
                x+---+x
                x    |x
                x    vx
                xxxxxxx"}
        );
    }

    // Vertical stubs, end is left and below start.
    // mid_y = (0+8)/2 = 4
    // runs: Down 5 (+1), Left 4, Down 4 (exact)
    #[test]
    fn s_shape_vertical_down_left() {
        let res = s_shape(SPoint::new(4, 0), Axis::Vertical, SPoint::new(0, 8));
        let expected = (
            SPoint::new(4, 0),
            vec![(Dir::Down, 5), (Dir::Left, 4), (Dir::Down, 4)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxx
                x    |x
                x    |x
                x    |x
                x    |x
                x+---+x
                x|    x
                x|    x
                x|    x
                xv    x
                xxxxxxx"}
        );
    }

    // ── C-shape ───────────────────────────────────────────────────────────────

    #[test]
    fn c_shape_right_basic() {
        let res = c_shape(SPoint::new(0, 0), SPoint::new(0, 2), Dir::Right, 2);
        let expected = (
            SPoint::new(0, 0),
            vec![(Dir::Right, 3), (Dir::Down, 2), (Dir::Left, 2)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxx
                x--+x
                x  |x
                x<-+x
                xxxxx"}
        );
    }

    #[test]
    fn c_shape_left_basic() {
        // start shifted to (2,0) so the leftward path stays on canvas
        let res = c_shape(SPoint::new(2, 0), SPoint::new(2, 2), Dir::Left, 2);
        let expected = (
            SPoint::new(2, 0),
            vec![(Dir::Left, 3), (Dir::Down, 2), (Dir::Right, 2)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxx
                x+--x
                x|  x
                x+->x
                xxxxx"}
        );
    }

    #[test]
    fn c_shape_down_basic() {
        let res = c_shape(SPoint::new(0, 0), SPoint::new(4, 0), Dir::Down, 3);
        let expected = (
            SPoint::new(0, 0),
            vec![(Dir::Down, 4), (Dir::Right, 4), (Dir::Up, 3)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxx
                x|   ^x
                x|   |x
                x|   |x
                x+---+x
                xxxxxxx"}
        );
    }

    #[test]
    fn c_shape_up_basic() {
        // start shifted to (0,3) so the upward path stays on canvas
        let res = c_shape(SPoint::new(0, 3), SPoint::new(4, 3), Dir::Up, 3);
        let expected = (
            SPoint::new(0, 3),
            vec![(Dir::Up, 4), (Dir::Right, 4), (Dir::Down, 3)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxxxx
                x+---+x
                x|   |x
                x|   |x
                x|   vx
                xxxxxxx"}
        );
    }

    #[test]
    fn c_shape_right_minimal_offset() {
        let res = c_shape(SPoint::new(0, 0), SPoint::new(0, 4), Dir::Right, 1);
        let expected = (
            SPoint::new(0, 0),
            vec![(Dir::Right, 2), (Dir::Down, 4), (Dir::Left, 1)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxx
                x-+x
                x |x
                x |x
                x |x
                x<+x
                xxxx"}
        );
    }

    #[test]
    fn c_shape_right_end_above_start() {
        let res = c_shape(SPoint::new(0, 2), SPoint::new(0, 0), Dir::Right, 2);
        let expected = (
            SPoint::new(0, 2),
            vec![(Dir::Right, 3), (Dir::Up, 2), (Dir::Left, 2)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        assert_eq!(
            render_path(start, runs),
            indoc! {"
                xxxxx
                x<-+x
                x  |x
                x--+x
                xxxxx"}
        );
    }

    // ── Composite ─────────────────────────────────────────────────────────────

    #[test]
    fn composite_two_c_shapes() {
        use Dir::*;
        let shape = ConnectorShape::Composite(vec![
            ConnectorShape::CShape {
                start: SPoint::new(0, 0),
                end: SPoint::new(0, 2),
                step_out_dir: Dir::Right,
            },
            ConnectorShape::CShape {
                start: SPoint::new(0, 2),
                end: SPoint::new(-2, 4),
                step_out_dir: Dir::Left,
            },
        ]);

        let (start, runs) = shape.into_runs();

        assert_eq!(
            (start, runs.clone()),
            (
                SPoint::new(0, 0),
                vec![
                    (Right, 2),
                    (Down, 2),
                    (Left, 1),
                    (Left, 3),
                    (Down, 2),
                    (Right, 1)
                ]
            )
        );

        println!("{}", render_path(start, runs.clone()));
        assert_eq!(
            dbg!(render_path(start, runs)),
            indoc! {"
                xxxxxxxxx
                x    --+x
                x      |x
                x+-----+x
                x|      x
                x+->    x
                xxxxxxxxx"}
        );
    }

    // ── classify_shape ────────────────────────────────────────────────────────
    //
    // Helper: build a node for routing tests.  `id` must be a single digit
    // (0-9); it is stored as the label and rendered in the centre of the box.
    // Width and height must be ≥ 3.

    /// Test-only wrapper: delegates to [`sort_endpoints`] + [`classify_shape_ordered`]
    /// so that test bodies that pass raw `(from, from_side, to, to_side)` still compile
    /// unchanged.
    fn classify_shape(from: &Node, from_side: Side, to: &Node, to_side: Side) -> ConnectorShape {
        let ep = sort_endpoints(from, from_side, to, to_side, ArrowDecorations::Forward);
        classify_shape_ordered(&ep)
    }

    fn make_node(id: usize, (x, y): (i32, i32), (w, h): (u16, u16)) -> Node {
        assert!(w >= 3 && h >= 3, "nodes must be at least 3×3");
        Node {
            id: NodeId::hacky(id),
            rect: SRect::new(x, y, w, h),
            label: id.to_string(),
        }
    }

    // Both nodes expose their Right side — A is above B, same column.
    //
    // Layout (absolute coords, node size 3×3):
    //
    //   col: 0 1 2 3 4 5
    //
    //   row0  +-+
    //   row1  |0|--+
    //   row2  +-+  |
    //   row3       |
    //   row4  +-+  |
    //   row5  |1|<-+
    //   row6  +-+
    //
    // A=(0,0,3,3): mid_right=(2,1)+1 → start=(3,1)
    // B=(0,4,3,3): mid_right=(2,5)+1 → end=(3,5)
    // offset = max(3,3)-3+2 = 2
    // c_shape(Right,offset=2): so=2, eo=2 → runs: Right 3, Down 4, Left 2
    #[test]
    fn classify_same_side_right() {
        let a = make_node(0, (0, 0), (3, 3));
        let b = make_node(1, (0, 4), (3, 3));
        let shape = classify_shape(&a, Side::Right, &b, Side::Right);
        assert_eq!(
            shape,
            ConnectorShape::CShape {
                start: SPoint::new(3, 1),
                end: SPoint::new(3, 5),
                step_out_dir: Dir::Right,
            }
        );
        assert_eq!(
            render_scene(&[&a, &b], shape),
            indoc! {"
                xxxxxxx
                x+-+  x
                x|0|-+x
                x+-+ |x
                x    |x
                x+-+ |x
                x|1|<+x
                x+-+  x
                xxxxxxx"}
        );
    }

    #[test]
    fn classify_short_right_to_left() {
        let a = make_node(0, (0, 0), (3, 3));
        let b = make_node(1, (5, 0), (3, 3));
        let shape = classify_shape(&a, Side::Right, &b, Side::Left);
        assert_eq!(
            shape,
            ConnectorShape::LineWithPointedEnds {
                start: SPoint::new(3, 1),
                dir: Dir::Right,
                len: 1,
                pointed_ends: (Dir::Right, Dir::Left),
            }
        );
        let rendered = render_scene(&[&a, &b], shape);
        println!("{rendered}");
        assert_eq!(
            rendered,
            indoc! {"
                 xxxxxxxxxx
                 x+-+  +-+x
                 x|0|->|1|x
                 x+-+  +-+x
                 xxxxxxxxxx"
            }
        );
    }

    // Both nodes expose their Left side — A is above B, same column.
    //
    // Layout (absolute coords, cols shifted so path fits left of nodes):
    //
    //   col: 0 1 2 3 4 5 6
    //
    //   row0      +-+
    //   row1  +---|0|
    //   row2  |   +-+
    //   row3  |
    //   row4  |   +-+
    //   row5  +-->|1|
    //   row6      +-+
    //
    // A=(4,0,3,3): mid_left=(4,1)-1 → start=(3,1)
    // B=(4,4,3,3): mid_left=(4,5)-1 → end=(3,5)
    // offset = 3-min(3,3)+2 = 2
    // c_shape(Left,offset=2): delta=0, so=2, eo=2 → runs: Left 3, Down 4, Right 2
    // path extends to x=0, scene origin shifts to include it → 7-wide canvas
    #[test]
    fn classify_same_side_left() {
        let a = make_node(0, (4, 0), (3, 3));
        let b = make_node(1, (4, 4), (3, 3));
        let shape = classify_shape(&a, Side::Left, &b, Side::Left);
        assert_eq!(
            shape,
            ConnectorShape::CShape {
                start: SPoint::new(3, 1),
                end: SPoint::new(3, 5),
                step_out_dir: Dir::Left,
            }
        );
        assert_eq!(
            render_scene(&[&a, &b], shape),
            indoc! {"
                xxxxxxx
                x  +-+x
                x+-|0|x
                x| +-+x
                x|    x
                x| +-+x
                x+>|1|x
                x  +-+x
                xxxxxxx"}
        );
    }

    // Right → Left, nodes far apart: S-shape (dx ≥ 6).
    //
    // Layout (absolute coords):
    //
    //   col: 0 1 2 3 4 5 6 7 8 9 10 11 12
    //
    //   row0  +-+                    +-+
    //   row1  |0|---+           +---|1|
    //   row2  +-+   |           |   +-+
    //   row3         +-----------+
    //
    // A=(0,0,3,3): start=(3,1)
    // B=(10,0,3,3): mid_left=(10,1)-1 → end=(9,1)... wait that's same row.
    //
    // Let me use B=(10,2,3,3) so they are on different rows:
    // B mid_left=(10,3)-1 → end=(9,3), dx=|9-3|=6 ≥ 6 → SShape
    // mid_x=(3+9)/2=6
    // runs: Right (6-3)+1=4, Down 2, Right (9-6)=3
    #[test]
    fn classify_right_to_left_far_s_shape() {
        let a = make_node(0, (0, 0), (3, 3));
        let b = make_node(1, (10, 2), (3, 3));
        let shape = classify_shape(&a, Side::Right, &b, Side::Left);
        assert_eq!(
            shape,
            ConnectorShape::SShape {
                start: SPoint::new(3, 1),
                axis: Axis::Horizontal,
                end: SPoint::new(9, 3),
            }
        );
        assert_eq!(
            render_scene(&[&a, &b], shape),
            indoc! {"
                xxxxxxxxxxxxxxx
                x+-+          x
                x|0|---+      x
                x+-+   |   +-+x
                x      +-->|1|x
                x          +-+x
                xxxxxxxxxxxxxxx"}
        );
    }

    // Right → Left, nodes close: L-corner (dx < 6).
    //
    // The path exits A's Right side, jogs Left past A's left boundary, then
    // turns Down and arrives at B's Left connection point.  Because the
    // horizontal run passes through the column of both nodes the stub clips
    // the top border of A on the way out and enters B's left border going
    // down — this is expected for a short right-to-left corner route.
    //
    //   col: 0 1 2 3 4 5 6
    //
    //   row0    +-+
    //   row1  +----   ← path exits A right, turns left past A's left side
    //   row2  | +-+
    //   row3  |
    //   row4  |
    //   row5  | +-+
    //   row6  v |1|   ← path ends at B's Left connection point going down
    //   row7    +-+
    //
    // A=(2,0,3,3): start=(5,1)
    // B=(2,5,3,3): mid_left=(2,6)-1 → end=(1,6), dx=|1-5|=4 < 6 → Corner(Horizontal)
    // runs: Left (5-1)+1=5, Down (6-1)=5
    #[test]
    fn classify_right_to_left_close_corner() {
        let a = make_node(0, (0, 0), (3, 3));
        let b = make_node(1, (0, 5), (3, 3));
        let shape = classify_shape(&a, Side::Right, &b, Side::Left);
        let cshape_meeting_point = SPoint::new(3, (3 + 4) / 2);
        assert_eq!(
            shape,
            ConnectorShape::Composite(Vec::from([
                ConnectorShape::CShape {
                    start: SPoint::new(3, 1),
                    end: cshape_meeting_point,
                    step_out_dir: Dir::Right,
                },
                ConnectorShape::CShape {
                    start: cshape_meeting_point,
                    end: SPoint::new(-1, 6),
                    step_out_dir: Dir::Left,
                },
            ]))
        );
        let rendered = render_scene(&[&a, &b], shape);
        println!("{rendered}");
        assert_eq!(
            rendered,
            indoc! {"
                xxxxxxxxx
                x  +-+  x
                x  |0|-+x
                x  +-+ |x
                x+-----+x
                x|      x
                x| +-+  x
                x+>|1|  x
                x  +-+  x
                xxxxxxxxx"
            }
        );
    }

    // Right → Bottom: mixed sides, L-corner, horizontal run first.
    //
    // The path exits A's Right side, goes right to align with B's bottom
    // connection point column, then descends through B to reach the stub one
    // cell below B's bottom border.  The vertical segment passes through B's
    // interior — the centre column of the node box shows the path symbol
    // overwriting the interior space.
    //
    //   col: 0 1 2 3 4 5 6 7
    //
    //   row0  +-+
    //   row1  |0|--+
    //   row2  +-+  |
    //   row3       |
    //   row4       |
    //   row5    +-+|   ← vertical leg enters B from above
    //   row6    |1||
    //   row7    +-+|
    //   row8       v   ← Bottom connection point, one below B's bottom border
    //
    // A=(0,0,3,3): start=(3,1)
    // B=(4,5,3,3): mid_bottom=(5,7)+1 → end=(5,8)
    // Corner(Horizontal): run1=Right (5-3)+1=3, run2=Down (8-1)=7
    #[test]
    fn classify_right_to_bottom_corner() {
        let a = make_node(0, (0, 0), (3, 3));
        let b = make_node(1, (4, 5), (3, 3));
        let shape = classify_shape(&a, Side::Right, &b, Side::Bottom);
        assert_eq!(
            shape,
            ConnectorShape::Corner {
                start: SPoint::new(3, 1),
                end: SPoint::new(5, 8),
                start_axis: Axis::Horizontal,
            }
        );
        assert_eq!(
            render_scene(&[&a, &b], shape),
            indoc! {"
                xxxxxxxxx
                x+-+    x
                x|0|--+ x
                x+-+  | x
                x     | x
                x     | x
                x    +|+x
                x    |||x
                x    +|+x
                x     v x
                xxxxxxxxx"}
        );
    }

    // Bottom → Bottom: same side, C-shape wrapping down.
    //
    // Layout (absolute coords):
    //
    //   col: 0 1 2 3 4 5 6 7
    //
    //   row0  +-+    +-+
    //   row1  |0|    |1|
    //   row2  +-+    +-+
    //   row3   |      ^
    //   row4   |      |
    //   row5   +------+
    //
    // A=(0,0,3,3): mid_bottom=(1,2)+1 → start=(1,3)
    // B=(5,0,3,3): mid_bottom=(6,2)+1 → end=(6,3)
    // offset = max(3,3)-3+2 = 2
    // c_shape(Down,offset=2): delta=0 → so=2, eo=2
    // runs: Down 3, Right 5, Up 2
    #[test]
    fn classify_same_side_bottom() {
        let a = make_node(0, (0, 0), (3, 3));
        let b = make_node(1, (5, 0), (3, 3));
        let shape = classify_shape(&a, Side::Bottom, &b, Side::Bottom);
        assert_eq!(
            shape,
            ConnectorShape::CShape {
                start: SPoint::new(1, 3),
                end: SPoint::new(6, 3),
                step_out_dir: Dir::Down,
            }
        );
        assert_eq!(
            render_scene(&[&a, &b], shape),
            indoc! {"
                xxxxxxxxxx
                x+-+  +-+x
                x|0|  |1|x
                x+-+  +-+x
                x |    ^ x
                x +----+ x
                xxxxxxxxxx"}
        );
    }

    // Top → Right: mixed sides, L-corner, vertical run first.
    //
    // The path exits A's Top side, goes up to align with B's right connection
    // row, then turns Right and travels to B's Right connection point.  The
    // horizontal run crosses through B's interior — node B's middle row shows
    // path dashes overwriting the id cell, while top and bottom borders remain
    // intact above and below.
    //
    //   col: 0 1 2 3 4 5 6 7 8
    //
    //   row0      +-+
    //   row1  +------>   ← horizontal leg exits B's right border at >
    //   row2  |   +-+
    //   row3  |
    //   row4  +-+
    //   row5  |0|
    //   row6  +-+
    //
    // A=(0,4,3,3): mid_top=(1,4)-1 → start=(1,3)
    // B=(4,0,3,3): mid_right=(6,1)+1 → end=(7,1)
    // Corner(Vertical): run1=Up (3-1)+1=3, run2=Right (7-1)=6
    #[test]
    fn classify_top_to_right_corner() {
        let a = make_node(0, (0, 4), (3, 3));
        let b = make_node(1, (4, 0), (3, 3));
        let shape = classify_shape(&a, Side::Top, &b, Side::Right);
        assert_eq!(
            shape,
            ConnectorShape::Corner {
                start: SPoint::new(1, 3),
                end: SPoint::new(7, 1),
                start_axis: Axis::Vertical,
            }
        );
        assert_eq!(
            render_scene(&[&a, &b], shape),
            indoc! {"
                xxxxxxxxxx
                x    +-+ x
                x +----->x
                x |  +-+ x
                x |      x
                x+-+     x
                x|0|     x
                x+-+     x
                xxxxxxxxxx"}
        );
    }

    // ── Backward arrowhead ────────────────────────────────────────────────────
    #[test]
    fn arrow_backward() {
        assert_eq!(
            render_path_arrow(
                SPoint::new(0, 0),
                vec![(Dir::Right, 4)],
                ArrowDecorations::Backward,
            ),
            indoc! {"
                xxxxxx
                x<---x
                xxxxxx"}
        );
    }

    // ── Both arrowheads ───────────────────────────────────────────────────────
    #[test]
    fn arrow_both() {
        assert_eq!(
            render_path_arrow(
                SPoint::new(0, 0),
                vec![(Dir::Right, 5)],
                ArrowDecorations::Both,
            ),
            indoc! {"
                xxxxxxx
                x<--->x
                xxxxxxx"}
        );
    }
}
