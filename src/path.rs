use crate::geometry::{Dir, SPoint, SRect};
use crate::state::{ArrowDecorations, Edge, Node, Side};

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
            PathSymbol::Horizontal => "─",
            PathSymbol::Vertical => "│",
            PathSymbol::CornerTopLeft => "┌",
            PathSymbol::CornerTopRight => "┐",
            PathSymbol::CornerBottomLeft => "└",
            PathSymbol::CornerBottomRight => "┘",
            PathSymbol::ArrowRight => "→",
            PathSymbol::ArrowLeft => "←",
            PathSymbol::ArrowDown => "↓",
            PathSymbol::ArrowUp => "↑",
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

/// Returns the border cell on `side` of `node` where a connection line starts.
fn connection_point(node: &Node, side: Side) -> SPoint {
    match side {
        Side::Right => node.rect.mid_right() + (1, 0),
        Side::Left => node.rect.mid_left() - (1, 0),
        Side::Top => node.rect.mid_top() - (0, 1),
        Side::Bottom => node.rect.mid_top() + (0, 1),
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
            let run1_dir = if mid_x >= start.x {
                Dir::Right
            } else {
                Dir::Left
            };
            let run2_dir = if end.y >= start.y { Dir::Down } else { Dir::Up };
            let run3_dir = if end.x >= mid_x {
                Dir::Right
            } else {
                Dir::Left
            };
            (
                start,
                vec![
                    (run1_dir, (mid_x - start.x).unsigned_abs() + 1),
                    (run2_dir, (end.y - start.y).unsigned_abs()),
                    (run3_dir, (end.x - mid_x).unsigned_abs()),
                ],
            )
        }
        Axis::Vertical => {
            // Same asymmetry: run1 needs +1, run3 does not.
            let mid_y = (start.y + end.y) / 2;
            let run1_dir = if mid_y >= start.y { Dir::Down } else { Dir::Up };
            let run2_dir = if end.x >= start.x {
                Dir::Right
            } else {
                Dir::Left
            };
            let run3_dir = if end.y >= mid_y { Dir::Down } else { Dir::Up };
            (
                start,
                vec![
                    (run1_dir, (mid_y - start.y).unsigned_abs() + 1),
                    (run2_dir, (end.x - start.x).unsigned_abs()),
                    (run3_dir, (end.y - mid_y).unsigned_abs()),
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
fn c_shape(start: SPoint, end: SPoint, dir: Dir, offset: u16) -> (SPoint, Vec<(Dir, u32)>) {
    let offset = offset as i32;
    match dir {
        Dir::Right => {
            let (so, eo) = match start.x - end.x {
                delta if delta < 0 => (offset - delta, offset),
                delta => (offset + delta, offset),
            };

            (
                start,
                Vec::from([
                    (Dir::Right, so as u32 + 1),
                    match start.y - end.y {
                        delta if delta < 0 => (Dir::Down, delta.unsigned_abs()),
                        delta => (Dir::Up, delta.unsigned_abs()),
                    },
                    (Dir::Left, eo as u32),
                ]),
            )
        }
        Dir::Left => {
            let (so, eo) = match start.x - end.x {
                delta if delta > 0 => (offset + delta, offset),
                delta => (offset, offset - delta),
            };

            (
                start,
                Vec::from([
                    (Dir::Left, so as u32 + 1),
                    match start.y - end.y {
                        delta if delta < 0 => (Dir::Down, delta.unsigned_abs()),
                        delta => (Dir::Up, delta.unsigned_abs()),
                    },
                    (Dir::Right, eo as u32),
                ]),
            )
        }
        Dir::Down => {
            let (so, eo) = match start.y - end.y {
                delta if delta < 0 => (offset - delta, offset),
                delta => (offset + delta, offset),
            };

            (
                start,
                Vec::from([
                    (Dir::Down, so as u32 + 1),
                    match start.x - end.x {
                        delta if delta < 0 => (Dir::Right, delta.unsigned_abs()),
                        delta => (Dir::Left, delta.unsigned_abs()),
                    },
                    (Dir::Up, eo as u32),
                ]),
            )
        }
        Dir::Up => {
            let (so, eo) = match start.y - end.y {
                delta if delta > 0 => (offset + delta, offset),
                delta => (offset, offset - delta),
            };

            (
                start,
                Vec::from([
                    (Dir::Up, so as u32 + 1),
                    match start.x - end.x {
                        delta if delta < 0 => (Dir::Right, delta.unsigned_abs()),
                        delta => (Dir::Left, delta.unsigned_abs()),
                    },
                    (Dir::Down, eo as u32),
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
    for &(dir, steps) in runs {
        pos = match dir {
            Dir::Right => SPoint::new(pos.x + steps as i32, pos.y),
            Dir::Left => SPoint::new(pos.x - steps as i32, pos.y),
            Dir::Down => SPoint::new(pos.x, pos.y + steps as i32),
            Dir::Up => SPoint::new(pos.x, pos.y - steps as i32),
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
}

impl PathIter {
    fn new(start: SPoint, runs: Vec<(Dir, u32)>, arrow: ArrowDecorations) -> Self {
        Self {
            runs,
            arrow,
            current: start,
            run_index: 0,
            completed_steps: 0,
            prev_dir: None,
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
        if self.prev_dir.is_none() {
            // Note that the very first step we don't progress the current position, because it is inclusive
            let dir = self.runs[0].0;
            self.prev_dir = Some(dir);
            let current = self.current;
            // self.current = self.current + dir;
            self.completed_steps = 1;

            // that means that we are at the very begining, so check if we need an arrow
            let symbol = match self.arrow {
                ArrowDecorations::Backward | ArrowDecorations::Both => {
                    dir_to_arrow(reverse_dir(dir))
                }
                _ => dir_to_symbol(dir),
            };

            return Some((current, symbol));
        }

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

        if steps == self.completed_steps + 1 {
            // that means that we on the last step in the run, hence we need to check how and if we change the direction
            self.completed_steps = 0;
            self.current = self.current + dir;
            // but it can also be the last step period
            let is_last_run = self.run_index == self.runs.len() - 1;
            let current_run_index = self.run_index;
            self.run_index += 1;

            if is_last_run {
                let symbol = match self.arrow {
                    ArrowDecorations::Forward | ArrowDecorations::Both => dir_to_arrow(dir),
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
        Some((self.current, dir_to_symbol(dir)))
    }
}

// ── Path calculation ──────────────────────────────────────────────────────────

/// Builds the path for `edge`, returning a lazy [`PathIter`] of
/// `(SPoint, PathSymbol)` pairs and the bounding [`SRect`] of the path.
pub fn calculate_path(nodes: &[Node], edge: &Edge) -> Option<(PathIter, SRect)> {
    let from_node = nodes.iter().find(|n| n.id == edge.from_id)?;
    let to_node = nodes.iter().find(|n| n.id == edge.to_id)?;

    let start = connection_point(from_node, edge.from_side);
    let end = connection_point(to_node, edge.to_side);

    let dx = end.x - start.x;

    let (start, runs) = if edge.from_side == edge.to_side {
        let (dir, offset) = match edge.from_side {
            Side::Right => (Dir::Right, (start.x.max(end.x) - start.x + 2) as u16),
            Side::Left => (Dir::Left, (start.x - start.x.min(end.x) + 2) as u16),
            Side::Bottom => (Dir::Down, (start.y.max(end.y) - start.y + 2) as u16),
            Side::Top => (Dir::Up, (start.y - start.y.min(end.y) + 2) as u16),
        };
        c_shape(start, end, dir, offset)
    } else if (edge.from_side == Side::Right && edge.to_side == Side::Left)
        || (edge.from_side == Side::Left && edge.to_side == Side::Right)
    {
        if dx.abs() >= 6 {
            s_shape(start, Axis::Horizontal, end)
        } else {
            corner(start, end, Axis::Horizontal)
        }
    } else {
        let start_axis = match edge.from_side {
            Side::Right | Side::Left => Axis::Horizontal,
            Side::Top | Side::Bottom => Axis::Vertical,
        };
        corner(start, end, start_axis)
    };

    let bounds = bounds_from_runs(start, &runs);
    let iter = PathIter::new(start, runs, edge.dir);
    Some((iter, bounds))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use ratatui::layout::Size;

    // ── Test renderer ─────────────────────────────────────────────────────────

    /// Render a list of `(SPoint, PathSymbol)` pairs onto a `width × height`
    /// grid, producing a multiline string where each row is wrapped in `x`
    /// delimiters for visibility.  Cells not covered by a symbol are spaces.
    ///
    /// Points outside the canvas are silently ignored.
    fn test_render(symbols: &[(SPoint, PathSymbol)], size: impl Into<Size>) -> String {
        let Size { width, height } = size.into();
        let mut grid = vec![vec![' '; width as usize]; height as usize];

        for (pt, seg) in symbols {
            let x = pt.x;
            let y = pt.y;
            if pt.x >= 0 && pt.y >= 0 && x < width as i32 && y < height as i32 {
                grid[y as usize][x as usize] = seg.to_ascii();
            }
        }

        let row_width = width as usize + 2;
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
        let segs: Vec<_> = PathIter::new(
            SPoint::new(0, 0),
            vec![(Dir::Right, 4)],
            ArrowDecorations::Forward,
        )
        .collect();
        let got = test_render(&segs, (5, 1));
        let expected = indoc! {"
            xxxxxxx
            x---> x
            xxxxxxx"};
        assert_eq!(got, expected);
    }

    #[test]
    fn vertical_line() {
        let segs: Vec<_> = PathIter::new(
            SPoint::new(0, 0),
            vec![(Dir::Down, 3)],
            ArrowDecorations::Forward,
        )
        .collect();
        let got = test_render(&segs, (1, 4));
        let expected = indoc! {"
            xxx
            x|x
            x|x
            xvx
            x x
            xxx"};
        assert_eq!(got, expected);
    }

    #[test]
    fn corner_right_then_down() {
        // Right 5 then Down 3: bend at (5,0)
        let segs: Vec<_> = PathIter::new(
            SPoint::new(0, 0),
            vec![(Dir::Right, 5), (Dir::Down, 3)],
            ArrowDecorations::Forward,
        )
        .collect();
        let got = test_render(&segs, (6, 4));
        // Note: corner overwrites run at (5,0); arrow at tip
        let expected = indoc! {"
            xxxxxxxx
            x----+ x
            x    | x
            x    | x
            x    v x
            xxxxxxxx"};
        assert_eq!(got, expected);
    }

    #[test]
    fn corner_down_then_right() {
        // Down 2 then Right 5: bend at (0,2)
        let segs: Vec<_> = PathIter::new(
            SPoint::new(0, 0),
            vec![(Dir::Down, 2), (Dir::Right, 5)],
            ArrowDecorations::Forward,
        )
        .collect();
        let got = test_render(&segs, (6, 3));
        let expected = indoc! {"
            xxxxxxxx
            x|     x
            x+---->x
            x      x
            xxxxxxxx"};
        assert_eq!(got, expected);
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (6, 4)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (5, 3)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (6, 4)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (5, 3)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (10, 3)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (10, 3)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (10, 3)),
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

    // Vertical stubs, end is right and below start.
    // mid_y = (0+8)/2 = 4
    // runs: Down 5 (+1 to land on row 4), Right 4, Down 4 (exact distance from row 4 to row 8)
    #[test]
    fn s_shape_vertical_down_right() {
        let res = s_shape(SPoint::new(0, 0), Axis::Vertical, SPoint::new(4, 4));
        let expected = (
            SPoint::new(0, 0),
            vec![(Dir::Down, 3), (Dir::Right, 4), (Dir::Down, 2)],
        );
        assert_eq!(res, expected);
        let (start, runs) = res;
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        // println!("{}", test_render(&segs, (5, 5)));
        assert_eq!(
            test_render(&segs, (5, 5)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (5, 9)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (3, 3)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (3, 3)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (5, 4)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (5, 4)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (2, 5)),
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
        let segs: Vec<_> = PathIter::new(start, runs, ArrowDecorations::Forward).collect();
        assert_eq!(
            test_render(&segs, (3, 3)),
            indoc! {"
                xxxxx
                x<-+x
                x  |x
                x--+x
                xxxxx"}
        );
    }

    // ── Backward arrowhead ────────────────────────────────────────────────────
    #[test]
    fn arrow_backward() {
        let segs: Vec<_> = PathIter::new(
            SPoint::new(0, 0),
            vec![(Dir::Right, 4)],
            ArrowDecorations::Backward,
        )
        .collect();
        let got = test_render(&segs, (5, 1));
        let expected = indoc! {"
            xxxxxxx
            x<--- x
            xxxxxxx"};
        assert_eq!(got, expected);
    }

    // ── Both arrowheads ───────────────────────────────────────────────────────
    #[test]
    fn arrow_both() {
        let segs: Vec<_> = PathIter::new(
            SPoint::new(0, 0),
            vec![(Dir::Right, 5)],
            ArrowDecorations::Both,
        )
        .collect();
        let got = test_render(&segs, (6, 1));
        let expected = indoc! {"
            xxxxxxxx
            x<---> x
            xxxxxxxx"};
        assert_eq!(got, expected);
    }
}
