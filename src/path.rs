use crate::actions::Dir;
use crate::geometry::SPoint;
use crate::state::{Edge, Node, Side};

// ── Connection / offset helpers ───────────────────────────────────────────────

/// Returns the border cell on `side` of `node` where a connection line starts.
pub fn connection_point(node: &Node, side: Side) -> SPoint {
    match side {
        Side::Right => node.rect.mid_right() + (1, 0),
        Side::Left => node.rect.mid_left() - (1, 0),
        Side::Top => node.rect.mid_top() - (0, 1),
        Side::Bottom => node.rect.mid_top() + (0, 1),
    }
}

// ── Shape builders ────────────────────────────────────────────────────────────

/// S-shaped route: two horizontal runs joined by a vertical jog in the middle.
fn s_shape(start: SPoint, s_off: SPoint, end: SPoint, e_off: SPoint) -> Vec<SPoint> {
    let mid_x = s_off.x + (e_off.x - s_off.x) / 2;
    vec![
        start,
        s_off,
        SPoint {
            x: mid_x,
            y: s_off.y,
        },
        SPoint {
            x: mid_x,
            y: e_off.y,
        },
        e_off,
        end,
    ]
}

/// C-shaped route: both stubs go in the same direction then wrap around.
fn c_shape(
    start: SPoint,
    s_off: SPoint,
    end: SPoint,
    e_off: SPoint,
    from_side: Side,
) -> Vec<SPoint> {
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
                SPoint {
                    x: far_x,
                    y: s_off.y,
                },
                SPoint {
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
                SPoint {
                    x: s_off.x,
                    y: far_y,
                },
                SPoint {
                    x: e_off.x,
                    y: far_y,
                },
                e_off,
                end,
            ]
        }
    }
}

/// Corner route: one horizontal and one vertical segment meeting at a bend.
fn corner(start: SPoint, s_off: SPoint, end: SPoint, e_off: SPoint) -> Vec<SPoint> {
    vec![
        start,
        s_off,
        SPoint {
            x: e_off.x,
            y: s_off.y,
        },
        e_off,
        end,
    ]
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

// ── Path calculation ──────────────────────────────────────────────────────────

/// Builds the list of waypoints for `edge`, choosing an appropriate shape.
pub fn calculate_path(nodes: &[Node], edge: &Edge) -> Vec<SPoint> {
    let from_node = nodes.iter().find(|n| n.id == edge.from_id).unwrap();
    let to_node = nodes.iter().find(|n| n.id == edge.to_id).unwrap();

    let start = connection_point(from_node, edge.from_side);
    let end = connection_point(to_node, edge.to_side);

    let dx = end.x - start.x;

    if edge.from_side == edge.to_side {
        c_shape(start, start, end, end, edge.from_side)
    } else if (edge.from_side == Side::Right && edge.to_side == Side::Left)
        || (edge.from_side == Side::Left && edge.to_side == Side::Right)
    {
        if dx.abs() >= 6 {
            s_shape(start, start, end, end)
        } else {
            corner(start, start, end, end)
        }
    } else {
        corner(start, start, end, end)
    }
}
