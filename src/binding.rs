// ── Bindings ──────────────────────────────────────────────────────────────────
//
// Declarative representation of the key → action mapping for each application
// mode.  Used to render contextual hints and to document available shortcuts.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::Action;
use crate::state::{BlockMode, EdgeEnd, EdgeMode, Mode, Side};

// ── Key representation ────────────────────────────────────────────────────────

/// A physical key chord: a key code and zero or more modifier keys.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyBinding {
    pub key: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    /// Convenience constructor for a bare key (no modifiers).
    pub fn plain(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Constructor for a key with explicit modifiers.
    pub fn with_mods(key: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { key, modifiers }
    }
}

// ── A single key → action pairing ────────────────────────────────────────────

/// One concrete binding: a key chord mapped to an application action, with a
/// human-readable description used for hints and documentation.
#[derive(Clone, Debug)]
pub struct BindingInstance {
    pub key: KeyBinding,
    pub action: Action,
    pub description: &'static str,
}

impl BindingInstance {
    pub fn new(key: KeyCode, action: Action, description: &'static str) -> Self {
        // Uppercase characters are reported by terminals as Shift + lowercase,
        // so automatically attach the SHIFT modifier when an uppercase char is
        // used as the key code.
        let modifiers = match key {
            KeyCode::Char(c) if c.is_uppercase() => KeyModifiers::SHIFT,
            _ => KeyModifiers::NONE,
        };
        Self {
            key: KeyBinding::with_mods(key, modifiers),
            action,
            description,
        }
    }

    pub fn with_mods(
        key: KeyCode,
        modifiers: KeyModifiers,
        action: Action,
        description: &'static str,
    ) -> Self {
        Self {
            key: KeyBinding::with_mods(key, modifiers),
            action,
            description,
        }
    }
}

impl From<(KeyCode, Action, &'static str)> for BindingInstance {
    fn from((key, action, description): (KeyCode, Action, &'static str)) -> Self {
        Self::new(key, action, description)
    }
}

impl From<(KeyCode, KeyModifiers, Action, &'static str)> for BindingInstance {
    fn from(
        (key, modifiers, action, description): (KeyCode, KeyModifiers, Action, &'static str),
    ) -> Self {
        Self::with_mods(key, modifiers, action, description)
    }
}

// ── KeyListen ─────────────────────────────────────────────────────────────────

/// A catch-all binding that hands the raw key event to a closure and produces
/// an optional action.  Used for modes where input cannot be described as a
/// fixed key→action table, such as text editing or label-driven selection.
pub struct KeyListen {
    pub description: &'static str,
    pub handler: Box<dyn Fn(KeyEvent) -> Option<Action>>,
}

// ── Binding enum ──────────────────────────────────────────────────────────────

/// Either a single key binding, a named group of related bindings, or a
/// catch-all listener for modes driven by free-form input.
pub enum Binding {
    /// A standalone key → action mapping.
    Single(BindingInstance),
    /// A collection of thematically related bindings shown under a shared name.
    Group {
        name: String,
        bindings: Vec<BindingInstance>,
    },
    /// A free-form listener that inspects every key event and produces an
    /// optional action.  Useful for text editing and label-selection modes.
    Listen(KeyListen),
}

impl Binding {
    pub fn group<T: Into<BindingInstance>>(
        name: impl Into<String>,
        items: impl IntoIterator<Item = T>,
    ) -> Self {
        Binding::Group {
            name: name.into(),
            bindings: items.into_iter().map(Into::into).collect(),
        }
    }

    pub fn single(value: impl Into<BindingInstance>) -> Self {
        Binding::Single(value.into())
    }

    pub fn listen(
        description: &'static str,
        handler: impl Fn(KeyEvent) -> Option<Action> + 'static,
    ) -> Self {
        Binding::Listen(KeyListen {
            description,
            handler: Box::new(handler),
        })
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Return every binding that is active while the application is in `mode`.
///
/// Bindings that apply in all modes (e.g. quit) are always included.
pub fn bindings_for_mode(mode: &Mode) -> Vec<Binding> {
    use crate::geometry::Dir;
    use Action::*;

    let mut bindings: Vec<Binding> = match mode {
        // ── SelectedEdge / Selected ───────────────────────────────────────────
        Mode::SelectedEdge(_, EdgeMode::Selected) => vec![
            Binding::single((KeyCode::Char('d'), DeleteEdge, "delete edge")),
            Binding::single((KeyCode::Char('e'), StartTweakEdge, "tweak connectors")),
            Binding::single((KeyCode::Char('f'), StartSelecting, "jump")),
            Binding::single((KeyCode::Esc, Cancel, "deselect")),
        ],

        // ── SelectedEdge / TweakEndpoint ──────────────────────────────────────
        Mode::SelectedEdge(_, EdgeMode::TweakEndpoint) => vec![
            Binding::single((
                KeyCode::Char('s'),
                SelectEdgeEnd(EdgeEnd::From),
                "tweak start",
            )),
            Binding::single((KeyCode::Char('e'), SelectEdgeEnd(EdgeEnd::To), "tweak end")),
            Binding::single((KeyCode::Esc, Cancel, "back")),
        ],

        // ── SelectedEdge / TweakSide ──────────────────────────────────────────
        Mode::SelectedEdge(_, EdgeMode::TweakSide { .. }) => vec![
            Binding::group(
                "set side",
                [
                    (KeyCode::Char('h'), SetEdgeSide(Side::Left), "left"),
                    (KeyCode::Char('j'), SetEdgeSide(Side::Bottom), "down"),
                    (KeyCode::Char('k'), SetEdgeSide(Side::Top), "up"),
                    (KeyCode::Char('l'), SetEdgeSide(Side::Right), "right"),
                ],
            ),
            Binding::single((KeyCode::Esc, Cancel, "back")),
        ],

        // ── Normal ────────────────────────────────────────────────────────────
        Mode::Normal => vec![
            Binding::group(
                "pan",
                [
                    ('h', Dir::Left),
                    ('l', Dir::Right),
                    ('k', Dir::Up),
                    ('j', Dir::Down),
                ]
                .map(|(key, dir)| (KeyCode::Char(key), Pan(dir, 5), "pan")),
            ),
            Binding::group(
                "pan fast",
                [
                    ('H', Dir::Left),
                    ('L', Dir::Right),
                    ('K', Dir::Up),
                    ('J', Dir::Down),
                ]
                .map(|(key, dir)| (KeyCode::Char(key), KeyModifiers::SHIFT, Pan(dir, 10), "pan")),
            ),
            Binding::single((KeyCode::Char('f'), StartSelecting, "jump")),
            Binding::single((KeyCode::Char('c'), CreateNewNode, "create block")),
        ],

        // ── SelectedBlock / Selected ──────────────────────────────────────────
        Mode::SelectedBlock(_, BlockMode::Selected) => vec![
            Binding::group(
                "move",
                [
                    ('h', Dir::Left),
                    ('l', Dir::Right),
                    ('k', Dir::Up),
                    ('j', Dir::Down),
                ]
                .map(|(key, dir)| (KeyCode::Char(key), Move(dir, 1), "move")),
            ),
            Binding::group(
                "move fast",
                [
                    ('H', Dir::Left),
                    ('L', Dir::Right),
                    ('J', Dir::Down),
                    ('K', Dir::Up),
                ]
                .map(|(key, dir)| {
                    (
                        KeyCode::Char(key),
                        KeyModifiers::SHIFT,
                        Move(dir, 5),
                        "move fast",
                    )
                }),
            ),
            Binding::single((KeyCode::Char('r'), StartResizing, "resize mode")),
            Binding::single((KeyCode::Char('i'), StartEditing, "edit label")),
            Binding::single((KeyCode::Char('e'), StartConnectingEdge, "connect edge")),
            Binding::single((KeyCode::Char('d'), DeleteShape, "delete")),
            Binding::single((
                KeyCode::Char('c'),
                StartCreatingRelativeNode,
                "new relative node",
            )),
            Binding::single((KeyCode::Char('f'), StartSelecting, "jump")),
            Binding::single((KeyCode::Esc, Cancel, "deselect")),
        ],

        // ── SelectedBlock / CreatingRelativeNode ─────────────────────────────
        Mode::SelectedBlock(_, BlockMode::CreatingRelativeNode) => vec![
            Binding::group(
                "create node",
                [
                    ('h', Dir::Left),
                    ('l', Dir::Right),
                    ('k', Dir::Up),
                    ('j', Dir::Down),
                ]
                .map(|(key, dir)| (KeyCode::Char(key), CreateRelativeNode(dir), "new node")),
            ),
            Binding::single((KeyCode::Esc, Cancel, "cancel")),
        ],

        // ── SelectedBlock / Resizing ──────────────────────────────────────────
        Mode::SelectedBlock(_, BlockMode::Resizing) => vec![
            Binding::group(
                "Expand",
                [
                    (KeyCode::Char('h'), Expand(Dir::Left), "expand left"),
                    (KeyCode::Char('l'), Expand(Dir::Right), "expand right"),
                    (KeyCode::Char('k'), Expand(Dir::Up), "expand up"),
                    (KeyCode::Char('j'), Expand(Dir::Down), "expand down"),
                ],
            ),
            Binding::group(
                "shrink",
                [
                    ('H', Dir::Left),
                    ('L', Dir::Right),
                    ('K', Dir::Up),
                    ('J', Dir::Down),
                ]
                .map(|(key, dir)| {
                    (
                        KeyCode::Char(key),
                        KeyModifiers::SHIFT,
                        Shrink(dir),
                        "shrink",
                    )
                }),
            ),
            Binding::single((KeyCode::Esc, Cancel, "exit resize mode")),
        ],

        // ── SelectedBlock / Editing ───────────────────────────────────────────
        Mode::SelectedBlock(_, BlockMode::Editing { .. }) => {
            vec![Binding::listen("edit label text", |ev| match ev.code {
                KeyCode::Enter => Some(Confirm),
                KeyCode::Esc => Some(Cancel),
                KeyCode::Backspace => Some(DeleteChar),
                KeyCode::Left => Some(CursorLeft),
                KeyCode::Right => Some(CursorRight),
                KeyCode::Char(ch) => Some(InsertChar(ch)),
                _ => None,
            })]
        }

        // ── SelectedBlock / ConnectingEdge ────────────────────────────────────
        Mode::SelectedBlock(_, BlockMode::ConnectingEdge { .. }) => vec![
            Binding::single((KeyCode::Esc, Cancel, "cancel")),
            Binding::listen("type label to connect", |ev| match ev.code {
                KeyCode::Char(ch) => Some(SelectChar(ch)),
                _ => None,
            }),
        ],

        // ── Selecting (jump-to-node) ──────────────────────────────────────────
        Mode::Selecting { .. } => vec![
            Binding::single((KeyCode::Esc, Cancel, "cancel selection")),
            Binding::listen("type label to jump", |ev| match ev.code {
                KeyCode::Char(ch) => Some(SelectChar(ch)),
                _ => None,
            }),
        ],
    };

    // ── Global bindings (active in every mode except Editing) ─────────────────
    if !matches!(mode, Mode::SelectedBlock(_, BlockMode::Editing { .. })) {
        bindings.push(Binding::Single(BindingInstance::new(
            KeyCode::Char('q'),
            Quit,
            "quit",
        )));
    }

    bindings
}
