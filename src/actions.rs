// ── Actions ───────────────────────────────────────────────────────────────────
//
// Every discrete user intent that can be produced by key-handling is
// represented here.  The update loop translates raw key events into one of
// these variants and then applies the resulting state change.

use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::geometry::Dir;
use crate::state::{EdgeEnd, EdgeId, EdgePropChange, NodeId, NodePropChange, Side};

// ── Copy-as format ────────────────────────────────────────────────────────────

/// The output format used when copying the whole scene via `CopyAs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyFormat {
    /// Plain unicode art — no wrapper, no breadcrumb.
    Plain,
    /// Wrapped in a fenced Markdown code block.
    Markdown,
    /// Wrapped in a Python triple-quoted string.
    Python,
    /// Every line prefixed with `// ` (Rust line comments).
    Rust,
}

#[derive(Clone, Debug)]
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
    /// Enter jump-to-node selection mode.
    StartSelecting,
    /// Enter resize sub-mode for the currently selected node.
    StartResizing,
    /// Enter inline label editing for the currently selected node.
    StartEditing,
    /// Enter the property editing panel for the currently selected node.
    StartPropEditing,
    /// Enter the property editing panel for the currently selected edge.
    StartEdgePropEditing,
    /// Confirm the current action (Enter inside editing, selection match).
    Confirm,
    /// Cancel / go back one mode level (Esc).
    Cancel,

    // ── Property panel navigation ─────────────────────────────────────────────
    /// Move the property panel focus up one section.
    PropNavUp,
    /// Move the property panel focus down one section.
    PropNavDown,
    /// Move the property panel focus left within the current section (wrapping).
    PropNavLeft,
    /// Move the property panel focus right within the current section (wrapping).
    PropNavRight,
    /// Dispatch the action embedded in the currently focused property item.
    ApplyCurrentPropItem,
    /// Apply a targeted change to the selected node's [`crate::state::NodeProperties`].
    /// Dispatched internally by property panel items when the user confirms.
    SetNodeProp(NodePropChange),
    /// Apply a targeted change to the selected edge's [`crate::state::ArrowDecorations`].
    /// Dispatched internally by edge property panel items when the user confirms.
    SetEdgeProp(EdgePropChange),

    // ── Editing ───────────────────────────────────────────────────────────────
    /// Pass a raw key event through to the active TextArea widget.
    TextAreaInput(KeyEvent),

    // ── Selection / jump ──────────────────────────────────────────────────────
    /// A printable character was typed while in Selecting mode.
    SelectChar(char),
    /// Enter the additive multi-select label overlay.
    /// Seeds the selection from the current mode:
    /// `Normal` → empty, `SelectedBlock(id)` → `[id]`, `MultiSelected` → existing ids.
    StartMultiSelecting,

    // ── Creating New Blocks──────────────────────────────────────────────────────
    CreateNewNode,

    /// Enter "create relative node" sub-mode.
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

    // ── Edge connection ───────────────────────────────────────────────────────
    /// Enter label-driven edge-connection mode.
    StartConnectingEdge,
    /// Create an edge between two explicitly identified nodes.
    /// Dispatched internally when a label sequence completes in ConnectingEdge mode,
    /// but may be used by any caller that knows both endpoints.
    ConnectNodes(NodeId, NodeId),

    // ── Edge selection ────────────────────────────────────────────────────────
    /// Transition to SelectedEdge mode for the given edge.
    SelectEdge(EdgeId),

    // ── Edge connector tweaking ───────────────────────────────────────────────
    /// Enter TweakEndpoint sub-mode from SelectedEdge.
    StartTweakEdge,
    /// Choose which endpoint to tweak: From or To.
    SelectEdgeEnd(EdgeEnd),
    /// Set the side for the chosen endpoint.
    SetEdgeSide(Side),

    // ── Selection ─────────────────────────────────────────────────────────────
    /// Select all nodes (transitions to MultiSelected with every node id).
    SelectAll,

    // ── Export ────────────────────────────────────────────────────────────────
    /// Render the current multi-selection to a plain unicode string and copy it
    /// to the system clipboard.  Only effective in `MultiSelected` mode.
    YankSelection,
    /// Open the "copy as" format-picker modal.  Operates on the whole scene
    /// (all nodes) regardless of the current selection state.
    StartCopyAs,
    /// Perform the actual copy-to-clipboard with the given format wrapper.
    /// Dispatched internally by the focused item in the `CopyAsModal` panel.
    CopyAs(CopyFormat),

    // ── Application ───────────────────────────────────────────────────────────
    Quit,

    // ── Scene persistence ─────────────────────────────────────────────────────
    /// Open the save-file modal.
    ///
    /// If `working_file` is already set on `AppState`, performs a quick save
    /// directly to that path without opening the modal.
    OpenSaveModal,
    /// Always open the save-file modal, pre-filling the input from
    /// `AppState::working_file` when it is set (Save As).
    OpenSaveAsModal,
    /// Confirm the filename typed in the save modal and write the file.
    SaveModalConfirm,
    /// Cancel the save modal and restore the previous mode without saving.
    SaveModalCancel,
    /// Internal: serialize the current scene to the given path.
    /// Produced by `SaveModalConfirm` and the quick-save path of `OpenSaveModal`.
    SaveSceneTo(PathBuf),
    /// Deserialize `scene.kontur` and replace the current scene.
    LoadScene,
}
