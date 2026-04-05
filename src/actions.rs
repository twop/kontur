// ── Actions ───────────────────────────────────────────────────────────────────
//
// Every discrete user intent that can be produced by key-handling is
// represented here.  The update loop translates raw key events into one of
// these variants and then applies the resulting state change.

use crossterm::event::KeyEvent;

use crate::geometry::Dir;
use crate::state::{EdgeEnd, EdgeId, EdgePropChange, NodeId, NodePropChange, Side};

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

    // ── Application ───────────────────────────────────────────────────────────
    Quit,

    // ── Scene persistence ─────────────────────────────────────────────────────
    /// Serialize the current scene to `scene.kontur`.
    SaveScene,
    /// Deserialize `scene.kontur` and replace the current scene.
    LoadScene,
}
