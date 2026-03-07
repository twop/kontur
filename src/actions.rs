// ── Actions ───────────────────────────────────────────────────────────────────
//
// Every discrete user intent that can be produced by key-handling is
// represented here.  The update loop translates raw key events into one of
// these variants and then applies the resulting state change.

use crate::geometry::Dir;
use crate::state::EdgeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    // ── Viewport panning (Normal mode) ────────────────────────────────────────
    Pan(Dir, u32),

    // ── Node movement (SelectedBlock / Selected) ──────────────────────────────
    /// Move the selected node by `amount` cells in the given direction.
    Move(Dir, u32),

    // ── Resize mode ───────────────────────────────────────────────────────────
    /// Expand the selected node in the given direction.
    Expand(Dir),
    /// Shrink the selected node from the given direction.
    Shrink(Dir),

    // ── Mode transitions ──────────────────────────────────────────────────────
    /// Enter jump-to-node selection mode (Enter key).
    StartSelecting,
    /// Enter resize sub-mode for the currently selected node ('r').
    StartResizing,
    /// Enter inline label editing for the currently selected node ('i').
    StartEditing,
    /// Confirm the current action (Enter inside editing, selection match).
    Confirm,
    /// Cancel / go back one mode level (Esc).
    Cancel,

    // ── Editing ───────────────────────────────────────────────────────────────
    InsertChar(char),
    DeleteChar,
    CursorLeft,
    CursorRight,

    // ── Selection / jump ──────────────────────────────────────────────────────
    /// A printable character was typed while in Selecting mode.
    SelectChar(char),

    // ── CreatingRelativeNode ──────────────────────────────────────────────────
    /// Enter "create relative node" sub-mode ('n' in Selected).
    StartCreatingRelativeNode,
    /// Spawn a new node adjacent to the selected node in the given direction.
    CreateRelativeNode(Dir),

    // ── Viewport ──────────────────────────────────────────────────────────────
    /// Center the viewport on the currently selected node.
    /// Not directly bindable; scheduled as a follow-up action by operations
    /// that move focus to a new node (label jump, relative-node creation).
    FocusSelected,

    // ── Shape deletion ────────────────────────────────────────────────────────
    /// Delete the currently selected node and all edges connected to it.
    DeleteShape,
    /// Delete the currently selected edge (only valid in SelectedEdge mode).
    DeleteEdge,

    // ── Edge selection ────────────────────────────────────────────────────────
    /// Transition to SelectedEdge mode for the given edge.
    SelectEdge(EdgeId),

    // ── Application ───────────────────────────────────────────────────────────
    Quit,
}
