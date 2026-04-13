// ── Bindings ──────────────────────────────────────────────────────────────────
//
// Declarative representation of the key → action mapping for each application
// mode.  Used to render contextual hints and to document available shortcuts.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use smallvec::{smallvec, SmallVec};

use crate::actions::Action;
use crate::state::{BlockMode, EdgeEnd, EdgeMode, Mode, Side};

// ── Key representation ────────────────────────────────────────────────────────

/// A physical key chord: a key code and zero or more modifier keys.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Return true if this binding matches the given key event.
    pub fn matches(&self, code: KeyCode, mods: KeyModifiers) -> bool {
        self.key == code && self.modifiers == mods
    }
}

// ── A single key → action pairing ────────────────────────────────────────────

/// One concrete binding: a key chord mapped to one or more application actions,
/// with a human-readable description used for hints and documentation.
///
/// `actions` is a `SmallVec<[Action; 1]>` so the common single-action case
/// never heap-allocates; multi-action bindings (e.g. apply + close) spill to
/// the heap only when needed.
#[derive(Clone, Debug)]
pub struct BindingInstance {
    pub key: KeyBinding,
    pub actions: SmallVec<[Action; 1]>,
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
            actions: smallvec![action],
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
            actions: smallvec![action],
            description,
        }
    }

    /// Construct a binding that dispatches multiple actions in sequence when
    /// the key is pressed.  Actions are applied left-to-right.
    pub fn multi(
        key: KeyCode,
        actions: impl IntoIterator<Item = Action>,
        description: &'static str,
    ) -> Self {
        let modifiers = match key {
            KeyCode::Char(c) if c.is_uppercase() => KeyModifiers::SHIFT,
            _ => KeyModifiers::NONE,
        };
        Self {
            key: KeyBinding::with_mods(key, modifiers),
            actions: actions.into_iter().collect(),
            description,
        }
    }
}

impl From<(KeyCode, Action, &'static str)> for BindingInstance {
    fn from((key, action, description): (KeyCode, Action, &'static str)) -> Self {
        Self::new(key, action, description)
    }
}

impl From<(char, Action, &'static str)> for BindingInstance {
    fn from((key, action, description): (char, Action, &'static str)) -> Self {
        Self::new(KeyCode::Char(key), action, description)
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

/// Either a single key binding, a named group of related bindings, a
/// catch-all listener for modes driven by free-form input, or a menu that
/// opens a nested set of bindings when its trigger key is pressed.
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
    /// A leader-key menu: pressing `key` pushes it onto the menu prefix and
    /// reveals `items` as the next level of bindings.  Items may themselves
    /// contain nested `Menu` entries, enabling arbitrary nesting depth.
    Menu {
        key: KeyBinding,
        name: &'static str,
        items: Vec<Binding>,
    },
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

    /// Construct a menu binding with the given trigger key, display name, and
    /// list of child bindings.
    pub fn menu(
        key: KeyCode,
        name: &'static str,
        items: impl IntoIterator<Item = Binding>,
    ) -> Self {
        Binding::Menu {
            key: KeyBinding::plain(key),
            name,
            items: items.into_iter().collect(),
        }
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Items shared by every space-leader menu (Normal and SelectedBlock).
fn space_menu_items() -> impl IntoIterator<Item = Binding> {
    [
        Binding::single((KeyCode::Char('q'), Action::Quit, "quit")),
        Binding::single((KeyCode::Char('s'), Action::SaveScene, "save scene")),
        Binding::single((KeyCode::Char('l'), Action::LoadScene, "load scene")),
    ]
}

/// Return the bindings that are currently active given the application `mode`
/// and the already-pressed `menu_prefix` key sequence.
pub fn bindings_for_mode(mode: &Mode) -> Vec<Binding> {
    use crate::geometry::Dir;
    use Action::*;

    // ── Build the top-level binding list for the current mode ─────────────────

    let bindings: Vec<Binding> = match mode {
        // ── SelectedEdge / Selected ───────────────────────────────────────────
        Mode::SelectedEdge(_, EdgeMode::Selected) => vec![
            Binding::single((KeyCode::Char('d'), DeleteEdge, "delete edge")),
            Binding::single((KeyCode::Char('e'), StartTweakEdge, "tweak connectors")),
            Binding::single((KeyCode::Char('p'), StartEdgePropEditing, "properties")),
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
                    ('h', SetEdgeSide(Side::Left), "left"),
                    ('j', SetEdgeSide(Side::Bottom), "down"),
                    ('k', SetEdgeSide(Side::Top), "up"),
                    ('l', SetEdgeSide(Side::Right), "right"),
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
            Binding::single(('f', StartSelecting, "jump")),
            Binding::single(('c', CreateNewNode, "create block")),
            // ── Space leader menu ─────────────────────────────────────────────
            Binding::menu(KeyCode::Char(' '), "menu", space_menu_items()),
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
            Binding::single(('r', StartResizing, "resize mode")),
            Binding::single(('i', StartEditing, "edit label")),
            Binding::single(('e', StartConnectingEdge, "connect edge")),
            Binding::single(('p', StartPropEditing, "properties")),
            Binding::single(('d', DeleteShape, "delete")),
            Binding::single(('c', StartCreatingRelativeNode, "new relative node")),
            Binding::single(('f', StartSelecting, "jump")),
            Binding::single((KeyCode::Esc, Cancel, "deselect")),
            Binding::menu(KeyCode::Char(' '), "menu", space_menu_items()),
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
                "expand",
                [
                    ('h', Expand(Dir::Left), "expand left"),
                    ('l', Expand(Dir::Right), "expand right"),
                    ('k', Expand(Dir::Up), "expand up"),
                    ('j', Expand(Dir::Down), "expand down"),
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
            vec![
                Binding::single((KeyCode::Esc, Confirm, "apply and exit")),
                Binding::listen("edit label text", |ev| match ev.code {
                    // Esc is handled by the explicit binding above.
                    KeyCode::Esc => None,
                    // Everything else (including Enter) is forwarded verbatim to the TextArea widget.
                    _ => Some(TextAreaInput(ev)),
                }),
            ]
        }

        // ── SelectedBlock / ConnectingEdge ────────────────────────────────────
        Mode::SelectedBlock(_, BlockMode::ConnectingEdge { .. }) => vec![
            Binding::single((KeyCode::Esc, Cancel, "cancel")),
            Binding::listen("type label to connect", |ev| match ev.code {
                KeyCode::Char(ch) => Some(SelectChar(ch)),
                _ => None,
            }),
        ],

        // ── SelectedBlock / PropEditing ───────────────────────────────────────
        // ── SelectedEdge / PropEditing ────────────────────────────────────────
        Mode::SelectedBlock(_, BlockMode::PropEditing { .. })
        | Mode::SelectedEdge(_, EdgeMode::PropEditing { .. }) => vec![
            Binding::group(
                "move focus",
                [
                    ('h', PropNavLeft, "prev option"),
                    ('j', PropNavDown, "next section"),
                    ('k', PropNavUp, "prev section"),
                    ('l', PropNavRight, "next option"),
                ],
            ),
            Binding::single((KeyCode::Char(' '), ApplyCurrentPropItem, "apply")),
            Binding::Single(BindingInstance::multi(
                KeyCode::Enter,
                [ApplyCurrentPropItem, Cancel],
                "apply and close",
            )),
            Binding::single((KeyCode::Esc, Cancel, "close")),
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

    bindings
}
