// Plain comment with no sentinel — must NOT match.
// Another plain line.

fn render_a() {
    // ---
    // ┌──────┐
    // │  A   │
    // └──────┘
    // --- src: a.ktr
}

/// Doc comment with no sentinel — must NOT match.
/// Another doc line.
fn regular() {}

impl Foo {
    /// ---
    /// ┌──────┐
    /// │  B   │
    /// └──────┘
    /// --- src: b.ktr
    fn show_b(&self) {}
}
