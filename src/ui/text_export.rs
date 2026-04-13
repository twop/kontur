// ── Text export ───────────────────────────────────────────────────────────────
//
// Renders a multi-selection of nodes (and the edges between them) into a plain
// unicode string suitable for pasting into any text editor.
//
// The rendering reuses the existing `render_nodes` / `render_connections`
// functions from the parent `ui` module by routing them through ratatui's
// `TestBackend` — an in-memory buffer that accepts the same widget calls as a
// real terminal but never emits escape sequences.  The resulting buffer cells
// are read back as plain unicode symbols, with trailing whitespace stripped per
// row and trailing blank rows removed.

use ratatui::{backend::TestBackend, Terminal};
use smallvec::SmallVec;

use crate::{
    path,
    state::{Edge, Mode, Node, NodeId},
    viewport::Viewport,
};

use super::{render_connections, render_nodes};

// ── Export options ────────────────────────────────────────────────────────────

/// Controls the whitespace padding added around the bounding box of the
/// selected nodes in the exported text.
///
/// Defaults to one cell of horizontal padding (left and right) and no vertical
/// padding.  Additional padding variants can be added here without breaking
/// callers that rely on `Default`.
pub struct ExportOptions {
    /// Extra blank columns to the left of the leftmost selected node.
    pub padding_left: u16,
    /// Extra blank columns to the right of the rightmost selected node.
    pub padding_right: u16,
    /// Extra blank rows above the topmost selected node.
    pub padding_top: u16,
    /// Extra blank rows below the bottommost selected node.
    pub padding_bottom: u16,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            padding_left: 1,
            padding_right: 1,
            padding_top: 0,
            padding_bottom: 0,
        }
    }
}

// ── Core rendering function ───────────────────────────────────────────────────

/// Render the nodes identified by `selected_ids` — and all edges whose both
/// endpoints are in that set — into a plain unicode string.
///
/// Returns `None` when `selected_ids` is empty or contains no nodes that exist
/// in `nodes`.
///
/// The output contains no ANSI escape sequences or colour information; every
/// cell is represented only by its unicode symbol (border glyphs, arrow tips,
/// spaces, and label text).  Trailing whitespace is stripped from each row and
/// trailing blank rows are removed from the bottom of the string.
pub fn render_selection_to_string(
    nodes: &[Node],
    edges: &[Edge],
    selected_ids: &[NodeId],
    opts: &ExportOptions,
) -> Option<String> {
    // ── 1. Resolve selected nodes ─────────────────────────────────────────────
    let selected_nodes: Vec<&Node> = nodes
        .iter()
        .filter(|n| selected_ids.contains(&n.id))
        .collect();

    if selected_nodes.is_empty() {
        return None;
    }

    // ── 2. Filter edges: only those fully inside the selection ───────────────
    //
    // Done before the bbox computation so edge path bounds can be folded in.
    let selected_id_set: SmallVec<[NodeId; 8]> = selected_ids.iter().copied().collect();
    let visible_edges_owned: Vec<Edge> = edges
        .iter()
        .filter(|e| selected_id_set.contains(&e.from_id) && selected_id_set.contains(&e.to_id))
        .cloned()
        .collect();

    // ── 3. Compute bounding box: node rects ∪ edge path bounds ───────────────
    let bbox = {
        // Start from the union of selected node rects.
        let mut left = selected_nodes.iter().map(|n| n.rect.left()).min().unwrap();
        let mut top = selected_nodes.iter().map(|n| n.rect.top()).min().unwrap();
        let mut right = selected_nodes.iter().map(|n| n.rect.right()).max().unwrap();
        let mut bottom = selected_nodes
            .iter()
            .map(|n| n.rect.bottom())
            .max()
            .unwrap();

        // Expand to include the bounding box of every edge path.
        // Edges can route outside the union of node rects (e.g. C-shapes, S-shapes),
        // so their path bounds must be included to avoid clipping connectors.
        for edge in &visible_edges_owned {
            if let Ok((_, edge_bounds)) = path::calculate_path(nodes, edge) {
                left = left.min(edge_bounds.left());
                top = top.min(edge_bounds.top());
                right = right.max(edge_bounds.right());
                bottom = bottom.max(edge_bounds.bottom());
            }
        }

        // Width/height in cells; both far edges are inclusive.
        let w = (right - left + 1).max(1) as u16;
        let h = (bottom - top + 1).max(1) as u16;
        (left, top, w, h)
    };

    let (bbox_left, bbox_top, bbox_w, bbox_h) = bbox;

    // ── 4. Canvas size including padding ─────────────────────────────────────
    let canvas_w = bbox_w + opts.padding_left + opts.padding_right;
    let canvas_h = bbox_h + opts.padding_top + opts.padding_bottom;

    if canvas_w == 0 || canvas_h == 0 {
        return None;
    }

    // ── 5. Build a viewport whose animated_center maps bbox onto the frame ────
    //
    // `render_nodes` / `render_connections` project canvas coords to screen via
    //
    //   screen_x = canvas_x - vp.animated_center().x + frame_width / 2
    //   screen_y = canvas_y - vp.animated_center().y + frame_height / 2
    //
    // We want the top-left corner of the bounding box (accounting for padding)
    // to land at screen position (0, 0).  Setting the viewport center to
    //
    //   cx = bbox_left - padding_left + canvas_w / 2
    //   cy = bbox_top  - padding_top  + canvas_h / 2
    //
    // achieves that (integer division matches ratatui's frame_size / 2).
    let center_x = bbox_left - opts.padding_left as i32 + (canvas_w / 2) as i32;
    let center_y = bbox_top - opts.padding_top as i32 + (canvas_h / 2) as i32;

    let vp = Viewport::new(crate::geometry::SPoint::new(center_x, center_y));

    // ── 6. Render into TestBackend ────────────────────────────────────────────
    let backend = TestBackend::new(canvas_w, canvas_h);
    let mut terminal = Terminal::new(backend).ok()?;

    // Use Normal mode so no nodes are highlighted yellow — clean unicode output.
    let mode = Mode::Normal;

    terminal
        .draw(|frame| {
            // Edges first (they are drawn directly into buffer cells; nodes
            // then overdraw their interiors and borders on top).
            render_connections(frame, nodes, &visible_edges_owned, &vp, &mode);
            // Render only the selected nodes (pass the full slice; nodes not
            // in the viewport will be skipped by clip logic, but passing the
            // subset is cleaner).
            let selected_nodes_owned: Vec<Node> =
                selected_nodes.iter().map(|n| (*n).clone()).collect();
            render_nodes(frame, &selected_nodes_owned, &vp, &mode);
        })
        .ok()?;

    // ── 7. Extract plain unicode text from the buffer ─────────────────────────
    let buffer = terminal.backend().buffer().clone();
    let width = canvas_w as usize;
    let height = canvas_h as usize;

    let mut rows: Vec<String> = Vec::with_capacity(height);
    for row in 0..height {
        let mut line = String::new();
        for col in 0..width {
            let cell = buffer.cell(ratatui::layout::Position {
                x: col as u16,
                y: row as u16,
            });
            let sym = cell.map(|c| c.symbol()).unwrap_or(" ");
            line.push_str(sym);
        }
        // Strip trailing whitespace.
        rows.push(line.trim_end().to_string());
    }

    // Strip trailing blank rows.
    while rows.last().map(|r: &String| r.is_empty()).unwrap_or(false) {
        rows.pop();
    }

    if rows.is_empty() {
        return None;
    }

    Some(rows.join("\n"))
}
