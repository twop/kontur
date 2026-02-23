// ── Actions ───────────────────────────────────────────────────────────────────
//
// Every discrete user intent that can be produced by key-handling is
// represented here.  The update loop translates raw key events into one of
// these variants and then applies the resulting state change.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    // ── Viewport panning (Normal mode) ────────────────────────────────────────
    Pan(Dir),

    // ── Node movement (SelectedBlock / Selected) ──────────────────────────────
    Move(Dir),
    /// Move by a larger step (×5).
    MoveFast(Dir),

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

    // ── Application ───────────────────────────────────────────────────────────
    Quit,
}
