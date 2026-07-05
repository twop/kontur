use std::{ops::Range, path::PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;

pub const NATIVE_FILE_EXT: &str = "ktr";
const NATIVE_FILE_EXT_WITH_DOT: &str = ".ktr";

// ── Embedding patterns ────────────────────────────────────────────────────────

/// Opening sentinel line for Rust and Python diagram blocks.
/// Written inside a comment / docstring, e.g. `// ---` or `    ---`.
const EMBED_OPEN: &str = "---";

/// Closing sentinel line for Rust and Python diagram blocks.
/// Always written with a trailing source path: `// --- src: ./diagram.ktr`.
/// If no source path is available, no sentinels are emitted at all.
const EMBED_CLOSE: &str = "--- src:";

/// Source-backlink key embedded in Markdown fenced blocks.
/// e.g. the last content line before the closing fence: `src: ./diagram.ktr`
const EMBED_SOURCE_KEY: &str = "src:";

// ── File-type classification ───────────────────────────────────────────────────
/// All files we currently support.
///  - `Native` (.ktr) — the source file itself.
///  - `Embedded` — another file type that can carry a diagram backlink.
pub enum SupportedFileType {
    /// .ktr
    Native,
    /// .py | .rs | .md
    Embedded(SupportedEmbeddingType),
}

/// Embedding host languages / formats.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SupportedEmbeddingType {
    /// .py
    Python,
    /// .rs
    Rust,
    /// .md
    Markdown,
}

impl SupportedFileType {
    pub fn extension(&self) -> &'static str {
        match self {
            SupportedFileType::Native => NATIVE_FILE_EXT,
            SupportedFileType::Embedded(embedded) => match embedded {
                SupportedEmbeddingType::Python => "py",
                SupportedEmbeddingType::Rust => "rs",
                SupportedEmbeddingType::Markdown => "md",
            },
        }
    }

    pub fn try_parse(ext: &str) -> Option<SupportedFileType> {
        match ext {
            "py" => Some(Self::Embedded(SupportedEmbeddingType::Python)),
            "rs" => Some(Self::Embedded(SupportedEmbeddingType::Rust)),
            "md" => Some(Self::Embedded(SupportedEmbeddingType::Markdown)),
            ext if ext == NATIVE_FILE_EXT => Some(Self::Native),
            _ => None,
        }
    }
}

// ── Rust comment style ─────────────────────────────────────────────────────────

/// Which comment marker was used for a Rust/doc-comment embedding.
///
/// Captured during scanning so the style can be faithfully reproduced when the
/// diagram is re-written back into the source file.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RustCommentStyle {
    /// `//` — regular line comment
    Line,
    /// `///` — doc comment
    Doc,
}

impl RustCommentStyle {
    /// The literal comment prefix string (without trailing space).
    pub fn prefix(self) -> &'static str {
        match self {
            RustCommentStyle::Line => "//",
            RustCommentStyle::Doc => "///",
        }
    }
}

// ── DiagramEmbedding ──────────────────────────────────────────────────────────

/// A diagram block found inside a host file that carries a backlink to a `.ktr`
/// source file.
///
/// All returned embeddings have a non-empty `source` path:
/// - Markdown: `src: <path>` as the last line inside the fenced block.
/// - Python: `--- src: <path>` as the closing sentinel inside a `"""` block.
/// - Rust: `// --- src: <path>` (or `///`) as the closing sentinel line.
///
/// Source-less blocks (no `--- src:` / `src:` line) are silently skipped.
pub struct DiagramEmbedding {
    /// Path recorded in the backlink line, as written by the author.
    pub source: PathBuf,
    /// Byte range of the **entire** block (including delimiters / sentinels) in
    /// the original string passed to [`scan_for_embeddings`].
    pub buf_position: Range<usize>,
    /// Number of leading space columns before the opening delimiter / sentinel.
    /// Always `0` for Markdown.  Used when re-writing the block to preserve
    /// indentation.
    pub indent_cols: u16,
    /// For Rust embeddings: whether `//` or `///` was used.  `None` for Python
    /// and Markdown.
    pub rust_comment_style: Option<RustCommentStyle>,
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Resolve a raw filename string typed by the user into a concrete `PathBuf`.
///
/// Rules:
/// - Leading/trailing whitespace is stripped.
/// - A trailing `.ktr` extension is stripped before re-appending, so the user
///   can type either `foo` or `foo.ktr` without getting `foo.ktr.ktr`.
pub(crate) fn resolve_save_path(input: &str) -> PathBuf {
    let trimmed = input.trim();
    let stem = trimmed
        .strip_suffix(NATIVE_FILE_EXT_WITH_DOT)
        .unwrap_or(trimmed);
    let filename = format!("{}.{}", stem, NATIVE_FILE_EXT);
    let p = PathBuf::from(&filename);
    if p.is_absolute() {
        p
    } else {
        PathBuf::from(".").join(p)
    }
}

// ── Scanning ──────────────────────────────────────────────────────────────────

// Static regexes compiled once at first use.

/// Matches a Markdown fenced code block whose last content line is `src: <path>`.
///
/// The `` ``` `` fences act as natural block boundaries; no extra sentinels
/// are needed.
///
/// Capture groups:
///   1 — content lines before the `src:` line (may be absent)
///   2 — the source path string (trimmed by the caller)
static MARKDOWN_EMBED_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?s)```\n(.*?\n)?src: ([^\n]+{NATIVE_FILE_EXT_WITH_DOT})\n```"
    ))
    .unwrap()
});

/// Matches a kontur diagram block embedded inside a Python `"""` docstring.
///
/// Because a diagram may live *inside* a larger docstring, `"""` boundaries
/// alone are not sufficient.  Hence, the block is delimited by `---`
/// sentinels written on their own indented lines inside the docstring:
///
/// ```text
///     """
///     ---
///     ┌──────┐
///     │  A   │
///     └──────┘
///     --- src: ./diagram.ktr
///     """
/// ```
///
/// Capture groups:
///   1 — leading whitespace shared by all sentinel/content lines
///   2 — body lines between the sentinels
///   3 — source path on the closing `--- src:` line (trimmed by the caller)
static PYTHON_EMBED_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Matches: <indent>---\n <body> <indent>--- src: <path>\n
    // The indent is captured from the opening line and validated against
    // each body and closing line in scan_python.
    Regex::new(&format!(r"(?m)^([ \t]+)---\n((?:[ \t]+[^\n]*\n)*)[ \t]+--- src: ([^\n]+{NATIVE_FILE_EXT_WITH_DOT})\n")).unwrap()
});

/// Matches a Rust / doc-comment sentinel block.
///
/// Format (all lines share the same `<indent>` and `<marker>`):
///
/// ```text
///     // ---
///     // ┌──────┐
///     // │  A   │
///     // └──────┘
///     // --- src: ./diagram.ktr
/// ```
///
/// If no source path is available no sentinels are emitted, so every matched
/// block is guaranteed to have a backlink.
///
/// Capture groups:
///   1 — leading whitespace (indent)
///   2 — comment marker: `//` or `///`
///   3 — body lines (zero or more `<indent><marker> …` lines, each ending `\n`)
///   4 — source path on the closing `--- src:` line (trimmed by the caller)
static RUST_EMBED_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Header:  <indent>(// or ///) ---\n
    // Body:    zero or more comment lines
    // Footer:  <indent>(// or ///) --- src: <path>\n
    Regex::new(
        &format!(r"(?m)^([ \t]*)(///?) ---\n((?:[ \t]*(?:///?) [^\n]*\n)*)[ \t]*(?:///?) --- src: ([^\n]+{NATIVE_FILE_EXT_WITH_DOT})\n?"),
    )
    .unwrap()
});

/// Scans `file_content` for diagram embeddings of the given type.
///
/// Only embeddings that carry a `src:` backlink are included; source-less
/// blocks are silently skipped.  All matching is purely in-memory; no I/O is
/// performed.
pub fn scan_for_embeddings(
    file_content: &str,
    embedded: SupportedEmbeddingType,
) -> Vec<DiagramEmbedding> {
    match embedded {
        SupportedEmbeddingType::Markdown => scan_markdown(file_content),
        SupportedEmbeddingType::Python => scan_python(file_content),
        SupportedEmbeddingType::Rust => scan_rust(file_content),
    }
}

fn scan_markdown(content: &str) -> Vec<DiagramEmbedding> {
    MARKDOWN_EMBED_REGEX
        .captures_iter(content)
        .map(|cap| {
            let source = PathBuf::from(cap[2].trim());
            let m = cap.get(0).unwrap();
            DiagramEmbedding {
                source,
                buf_position: m.start()..m.end(),
                indent_cols: 0,
                rust_comment_style: None,
            }
        })
        .collect()
}

fn scan_python(content: &str) -> Vec<DiagramEmbedding> {
    let mut results = Vec::new();
    for cap in PYTHON_EMBED_REGEX.captures_iter(content) {
        // Groups: 1=indent string, 2=body lines, 3=source path.
        let indent = &cap[1];
        let body = &cap[2];
        let source = PathBuf::from(cap[3].trim());

        // Every body line must start with the same indent prefix.
        let body_ok = body
            .lines()
            .all(|line| line.starts_with(indent) || line.is_empty());
        if !body_ok {
            continue;
        }

        let m = cap.get(0).unwrap();
        results.push(DiagramEmbedding {
            source,
            buf_position: m.start()..m.end(),
            indent_cols: indent.len() as u16,
            rust_comment_style: None,
        });
    }
    results
}

fn scan_rust(content: &str) -> Vec<DiagramEmbedding> {
    let mut results = Vec::new();
    for cap in RUST_EMBED_REGEX.captures_iter(content) {
        // All four groups are always present (footer always has src:).
        let indent = &cap[1];
        let marker = &cap[2]; // "//" or "///"
        let body = &cap[3];
        let source_str = cap[4].trim();

        // Validate: every body line must start with <indent><marker> .
        let expected_prefix = format!("{}{} ", indent, marker);
        let body_ok = body.lines().all(|line| {
            line.starts_with(&expected_prefix) || line == format!("{}{}", indent, marker).as_str()
        });
        if !body_ok {
            continue;
        }

        let style = if marker == "///" {
            RustCommentStyle::Doc
        } else {
            RustCommentStyle::Line
        };

        let m = cap.get(0).unwrap();
        results.push(DiagramEmbedding {
            source: PathBuf::from(source_str),
            buf_position: m.start()..m.end(),
            indent_cols: indent.len() as u16,
            rust_comment_style: Some(style),
        });
    }
    results
}

// ── Formatting ────────────────────────────────────────────────────────────────

/// Wrap `text` (plain unicode art) in the comment/block syntax appropriate for
/// `fmt`, ready to be pasted into a host file.
///
/// - `fmt = None` returns `text` unchanged (plain clipboard copy).
/// - `source_path`: for Rust and Python, `None` means no sentinels are emitted
///   — the diagram is written as plain commented / indented lines only.
///   For Markdown a source path is always required (pass `Some`).
/// - `indent` is the number of leading space columns prepended to every output
///   line (0 for top-level, >0 for indented contexts).  Ignored for Markdown.
/// - `rust_comment_style` controls whether `//` or `///` is used for Rust
///   output; ignored for all other formats.
pub fn format_embedded_diagram(
    text: &str,
    fmt: Option<SupportedEmbeddingType>,
    source_path: Option<&PathBuf>,
    indent: u16,
    rust_comment_style: Option<RustCommentStyle>,
) -> String {
    let pad = " ".repeat(indent as usize);

    match fmt {
        None => text.to_string(),

        Some(SupportedEmbeddingType::Markdown) => {
            // Markdown: ``` fences are the natural boundary; no extra sentinels.
            format!(
                "```\n{}\n{} {}\n```\n",
                text,
                EMBED_SOURCE_KEY,
                source_path.unwrap().to_string_lossy()
            )
        }

        Some(SupportedEmbeddingType::Python) => {
            // Indent every content line.
            let indented_text = text
                .lines()
                .map(|l| format!("{}{}", pad, l))
                .collect::<Vec<_>>()
                .join("\n");

            match source_path {
                Some(p) => {
                    // Sentinels delimit the block inside a larger docstring.
                    format!(
                        "{pad}{EMBED_OPEN}\n{}\n{pad}{EMBED_CLOSE} {}\n",
                        indented_text,
                        p.to_string_lossy(),
                    )
                }
                None => {
                    // No source — emit plain indented lines, no sentinels.
                    indented_text
                }
            }
        }

        Some(SupportedEmbeddingType::Rust) => {
            let style = rust_comment_style.unwrap_or(RustCommentStyle::Line);
            let marker = style.prefix();

            // Prefix every diagram line with <indent><marker> <content>.
            let commented = text
                .lines()
                .map(|l| format!("{}{} {}", pad, marker, l))
                .collect::<Vec<_>>()
                .join("\n");

            match source_path {
                Some(p) => {
                    // Sentinels wrap the block; closing line carries the source.
                    let footer = format!("{}{} {EMBED_CLOSE} {}", pad, marker, p.to_string_lossy());
                    format!(
                        "{}{} {EMBED_OPEN}\n{}\n{}\n",
                        pad, marker, commented, footer
                    )
                }
                None => {
                    // No source — emit plain commented lines, no sentinels.
                    format!("{}\n", commented)
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    // ── resolve_save_path ──────────────────────────────────────────────────────

    #[test]
    fn resolve_strips_double_ext() {
        assert_eq!(resolve_save_path("foo.ktr"), PathBuf::from("./foo.ktr"));
    }

    #[test]
    fn resolve_appends_ext() {
        assert_eq!(resolve_save_path("foo"), PathBuf::from("./foo.ktr"));
    }

    #[test]
    fn resolve_trims_whitespace() {
        assert_eq!(resolve_save_path("  foo  "), PathBuf::from("./foo.ktr"));
    }

    #[test]
    fn resolve_absolute_path_unchanged() {
        assert_eq!(
            resolve_save_path("/tmp/diagram.ktr"),
            PathBuf::from("/tmp/diagram.ktr")
        );
    }

    // ── scan_for_embeddings — Markdown ─────────────────────────────────────────

    #[test]
    fn scan_markdown_finds_two_embeddings() {
        let content = include_str!("../tests/fixtures/sample.md");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Markdown);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].source, PathBuf::from("a.ktr"));
        assert_eq!(found[1].source, PathBuf::from("b.ktr"));
        assert_eq!(found[0].indent_cols, 0);
        assert!(found[0].rust_comment_style.is_none());
    }

    #[test]
    fn scan_markdown_plain_fence_not_matched() {
        let content = "```\njust code\n```\n";
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Markdown);
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn scan_markdown_buf_position_round_trips() {
        let content = include_str!("../tests/fixtures/sample.md");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Markdown);
        assert!(!found.is_empty());
        let slice = &content[found[0].buf_position.clone()];
        let re_parsed = scan_for_embeddings(slice, SupportedEmbeddingType::Markdown);
        assert_eq!(re_parsed.len(), 1);
        assert_eq!(re_parsed[0].source, found[0].source);
    }

    // ── scan_for_embeddings — Python ───────────────────────────────────────────

    #[test]
    fn scan_python_finds_two_embeddings() {
        let content = include_str!("../tests/fixtures/sample.py");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Python);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].source, PathBuf::from("a.ktr"));
        assert_eq!(found[1].source, PathBuf::from("b.ktr"));
        assert!(found[0].rust_comment_style.is_none());
    }

    #[test]
    fn scan_python_captures_indent_cols() {
        let content = include_str!("../tests/fixtures/sample.py");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Python);
        // First embedding: inside show_a() — 4 spaces
        assert_eq!(found[0].indent_cols, 4);
        // Second embedding: inside Foo.show_b() — 8 spaces
        assert_eq!(found[1].indent_cols, 8);
    }

    #[test]
    fn scan_python_plain_docstring_not_matched() {
        let content = "def f():\n    \"\"\"Normal docstring.\"\"\"\n    pass\n";
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Python);
        assert_eq!(found.len(), 0);
    }

    // ── scan_for_embeddings — Rust ─────────────────────────────────────────────

    #[test]
    fn scan_rust_finds_two_embeddings() {
        let content = include_str!("../tests/fixtures/sample.rs");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Rust);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].source, PathBuf::from("a.ktr"));
        assert_eq!(found[1].source, PathBuf::from("b.ktr"));
    }

    #[test]
    fn scan_rust_captures_comment_style() {
        let content = include_str!("../tests/fixtures/sample.rs");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Rust);
        assert_eq!(found[0].rust_comment_style, Some(RustCommentStyle::Line));
        assert_eq!(found[1].rust_comment_style, Some(RustCommentStyle::Doc));
    }

    #[test]
    fn scan_rust_captures_indent_cols() {
        let content = include_str!("../tests/fixtures/sample.rs");
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Rust);
        // render_a: 4 spaces indent
        assert_eq!(found[0].indent_cols, 4);
        // Foo::show_b: 4 spaces indent
        assert_eq!(found[1].indent_cols, 4);
    }

    #[test]
    fn scan_rust_plain_comments_not_matched() {
        let content = "// Plain comment\n// Another line\n";
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Rust);
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn scan_rust_no_source_no_match() {
        // A block with only an open sentinel and no --- src: footer is not matched.
        let content = "    // ---\n    // ┌──┐\n    // plain end\n";
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Rust);
        assert_eq!(found.len(), 0);
    }

    // ── format_embedded_diagram — round-trips ──────────────────────────────────

    #[test]
    fn format_then_scan_markdown_round_trips() {
        let diagram = "┌──┐\n│  │\n└──┘";
        let source = PathBuf::from("x.ktr");
        let formatted = format_embedded_diagram(
            diagram,
            Some(SupportedEmbeddingType::Markdown),
            Some(&source),
            0,
            None,
        );
        let found = scan_for_embeddings(&formatted, SupportedEmbeddingType::Markdown);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, source);
    }

    #[test]
    fn format_then_scan_python_round_trips_with_indent() {
        let diagram = "┌──┐\n│  │\n└──┘";
        let source = PathBuf::from("x.ktr");
        let formatted = format_embedded_diagram(
            diagram,
            Some(SupportedEmbeddingType::Python),
            Some(&source),
            4,
            None,
        );
        let found = scan_for_embeddings(&formatted, SupportedEmbeddingType::Python);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, source);
        assert_eq!(found[0].indent_cols, 4);
    }

    #[test]
    fn format_python_no_source_has_no_sentinels() {
        let diagram = "┌──┐\n│  │\n└──┘";
        let formatted =
            format_embedded_diagram(diagram, Some(SupportedEmbeddingType::Python), None, 4, None);
        assert!(!formatted.contains(EMBED_OPEN));
        assert!(!formatted.contains(EMBED_CLOSE));
        // Content lines are still indented.
        assert!(formatted.contains("    ┌──┐"));
    }

    #[test]
    fn format_then_scan_rust_line_comment_round_trips() {
        let diagram = "┌──┐\n│  │\n└──┘";
        let source = PathBuf::from("x.ktr");
        let formatted = format_embedded_diagram(
            diagram,
            Some(SupportedEmbeddingType::Rust),
            Some(&source),
            4,
            Some(RustCommentStyle::Line),
        );
        let found = scan_for_embeddings(&formatted, SupportedEmbeddingType::Rust);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, source);
        assert_eq!(found[0].indent_cols, 4);
        assert_eq!(found[0].rust_comment_style, Some(RustCommentStyle::Line));
    }

    #[test]
    fn format_then_scan_rust_doc_comment_round_trips() {
        let diagram = "┌──┐\n│  │\n└──┘";
        let source = PathBuf::from("x.ktr");
        let formatted = format_embedded_diagram(
            diagram,
            Some(SupportedEmbeddingType::Rust),
            Some(&source),
            4,
            Some(RustCommentStyle::Doc),
        );
        let found = scan_for_embeddings(&formatted, SupportedEmbeddingType::Rust);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, source);
        assert_eq!(found[0].rust_comment_style, Some(RustCommentStyle::Doc));
    }

    #[test]
    fn python_round_trip_preserves_indent_and_offset() {
        let content = indoc! {r#"
            class Foo:
                def show():
                    """
                    ---
                    ┌──┐
                    │  │
                    └──┘
                    --- src: ./diagram.ktr
                    """
        "#};

        // ── First scan ────────────────────────────────────────────────────
        let found = scan_for_embeddings(content, SupportedEmbeddingType::Python);
        assert_eq!(found.len(), 1);
        let emb = &found[0];
        assert_eq!(emb.source, PathBuf::from("./diagram.ktr"));
        assert_eq!(emb.indent_cols, 8);

        // ── Re-render ─────────────────────────────────────────────────────
        let new_diagram = indoc! {"
            ┌────┐
            │    │
            └────┘"
        };
        let rendered = format_embedded_diagram(
            new_diagram,
            Some(SupportedEmbeddingType::Python),
            Some(&emb.source),
            emb.indent_cols,
            None,
        );

        // ── Update diagram ───────────────────────────────────────────────
        let spliced = format!(
            "{}{}{}",
            &content[..emb.buf_position.start],
            rendered,
            &content[emb.buf_position.end..]
        );

        let expected_content = indoc! {r#"
            class Foo:
                def show():
                    """
                    ---
                    ┌────┐
                    │    │
                    └────┘
                    --- src: ./diagram.ktr
                    """
        "#};

        assert_eq!(expected_content, spliced);

        // ── Second scan ───────────────────────────────────────────────────
        let found2 = scan_for_embeddings(&spliced, SupportedEmbeddingType::Python);
        assert_eq!(found2.len(), 1);
        assert_eq!(found2[0].source, emb.source);
        assert_eq!(found2[0].indent_cols, emb.indent_cols);
        // Block starts at the same byte offset — surrounding context is unchanged.
        assert_eq!(found2[0].buf_position.start, emb.buf_position.start);
    }

    #[test]
    fn format_rust_no_source_has_no_sentinels() {
        let diagram = "┌──┐\n│  │\n└──┘";
        let formatted = format_embedded_diagram(
            diagram,
            Some(SupportedEmbeddingType::Rust),
            None,
            4,
            Some(RustCommentStyle::Line),
        );
        assert!(!formatted.contains(EMBED_OPEN));
        assert!(!formatted.contains(EMBED_CLOSE));
        // Content is still commented and indented.
        assert!(formatted.contains("    // ┌──┐"));
    }
}
