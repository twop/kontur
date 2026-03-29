# kontur — Agent Index

Keep this file in sync with the codebase. When a file is added, renamed, or deleted under `src/`, update the tree and the file index below accordingly. Descriptions are two sentences max; list key identifiers but do not expand on them.

---

## Project

A terminal whiteboard/diagram editor written in Rust. Renders using `ratatui` + `crossterm`, keyboard-driven (Vim-style), and follows the **ELM (Model-View-Update)** architecture.

**ELM layer map:**
- Model → `src/state.rs`
- Messages → `src/actions.rs`
- Update → `src/update.rs`
- View → `src/ui/mod.rs`

---

## File Tree

```
kontur/
├── Cargo.toml
├── Cargo.lock
└── src/
    ├── main.rs
    ├── actions.rs
    ├── binding.rs
    ├── geometry.rs
    ├── labels.rs
    ├── path.rs
    ├── scene_save.rs
    ├── screen_space.rs
    ├── state.rs
    ├── update.rs
    ├── viewport.rs
    └── ui/
        └── mod.rs
```

---

## File Index

- **Cargo.toml** — Package manifest, edition, and all dependencies (`ratatui`, `crossterm`, `serde`, `damped-springs`, `tween`, etc.).

- **src/main.rs** — Entry point: terminal setup/teardown, main event loop, and side-effect dispatch (scene save/load). Key functions: `bootstrap_small_demo_graph`, `resolve_menu`.

- **src/state.rs** — Central data model; the ELM Model. Key types: `AppState`, `Node`, `Edge`, `Mode`, `BlockMode`, `EdgeMode`, `NodeId`, `EdgeId`, `GraphId`.

- **src/actions.rs** — All user action variants; the ELM Message type. Key type: `Action`.

- **src/update.rs** — Pure state-transition function; the ELM Update. Key functions/types: `update`, `UpdateResult`, `Effect`.

- **src/ui/mod.rs** — All ratatui rendering; the ELM View. Key functions: `render_app`, `render_nodes`, `render_connections`, `render_selection_labels`, `render_hints_panel`.

- **src/binding.rs** — Declarative key→action binding system; also drives the hints panel display. Key types/functions: `Binding`, `BindingInstance`, `KeyListen`, `bindings_for_mode`.

- **src/geometry.rs** — Phantom-typed 2D geometry primitives for canvas coordinate space. Key types: `SPoint`, `SRect`, `Dir`, `Padding`, `CanvasPoint`, `CanvasRect`.

- **src/viewport.rs** — Animated camera supporting spring physics and ExpoOut tween strategies. Key types: `Viewport`, `AnimationConfig`.

- **src/path.rs** — Orthogonal edge routing: shape classification, path iteration, and symbol rendering. Key types/functions: `PathIter`, `ConnectorShape`, `PathSymbol`, `calculate_path`, `classify_shape_ordered`.

- **src/screen_space.rs** — Type-safe canvas→screen coordinate projection via a private marker type. Key types: `Screen`, `SPoint<Screen>`.

- **src/labels.rs** — Vimium-style jump label iterator with prefix-safe 1- and 2-char sequences. Key type: `LabelIter`.

- **src/scene_save.rs** — JSON serialization/deserialization of scenes; all serde annotations are isolated here. Key types/functions: `SceneSave`, `NodeSave`, `EdgeSave`, `to_scene_save`, `from_scene_save`.
