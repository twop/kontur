use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

use crate::startup_selection::io_scan_diagram_files::DiagramScanItem;

#[derive(Clone)]
pub struct FuzzyMatchingFiles {
    pattern: String,
    items: Vec<DiagramScanItem>,
    candidates: Vec<DiagramScanItem>,
}

#[derive(Clone)]
pub enum FileSelectionState {
    ScanningFiles,
    FuzzyMatching(FuzzyMatchingFiles),
}

/// Returns the string that will be matched against the fuzzy pattern.
///
/// - `NativeSource` → full path string.
/// - `Backlink`     → "file_path source_path" (space-separated) so the user
///   can narrow by either the host file or the embedded diagram name.
fn match_key(item: &DiagramScanItem) -> String {
    match item {
        DiagramScanItem::NativeSource(path) => path.to_string_lossy().into_owned(),
        DiagramScanItem::Backlink(bl) => format!(
            "{} {}",
            bl.file_path.to_string_lossy(),
            bl.source_path.to_string_lossy()
        ),
    }
}

/// Numeric variant rank used as a secondary sort key.
///   Backlinks are more useful here, because it is likely that is what we would want to edit
fn variant_rank(item: &DiagramScanItem) -> i64 {
    match item {
        DiagramScanItem::NativeSource(_) => 0,
        DiagramScanItem::Backlink(_) => 1,
    }
}

/// Number of path components (depth) of the primary path for an item.
/// Fewer components → shallower → ranked earlier on ties.
fn path_depth(item: &DiagramScanItem) -> usize {
    match item {
        DiagramScanItem::NativeSource(p) => p.components().count(),
        DiagramScanItem::Backlink(bl) => bl.file_path.components().count(),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fuzzy-matches `items` against `pattern` and returns a ranked list of
/// `(item_clone, score)` pairs.
///
/// Ranking is determined by a composite key (descending):
///   1. Skim fuzzy score          — primary signal
///   2. Variant rank              — NativeSource beats Backlink on equal score
///   3. Path depth (negated)      — shallower paths rank higher on ties
///
/// When `pattern` is empty every item is returned with score `0` in its
/// original order.
///
/// The function is pure: it does not perform IO, mutate state, or rely on
/// any global mutable state.
pub fn fuzzy_match_files(pattern: &str, items: &[DiagramScanItem]) -> Vec<(DiagramScanItem, i64)> {
    if pattern.is_empty() {
        return items.iter().map(|item| (item.clone(), 0)).collect();
    }

    let matcher = SkimMatcherV2::default();

    let mut scored: Vec<(DiagramScanItem, i64)> = items
        .iter()
        .filter_map(|item| {
            let key = match_key(item);
            matcher
                .fuzzy_match(&key, pattern)
                .map(|score| (item.clone(), score))
        })
        .collect();

    // Sort descending: highest score first, then prefer NativeSource, then
    // shallower paths.
    scored.sort_by(|(a, score_a), (b, score_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| variant_rank(b).cmp(&variant_rank(a)))
            .then_with(|| path_depth(a).cmp(&path_depth(b)))
    });

    scored
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::export_import_content::SupportedEmbeddingType;
    use crate::startup_selection::io_scan_diagram_files::{Backlink, DiagramScanItem};

    use super::fuzzy_match_files;

    fn native(path: &str) -> DiagramScanItem {
        DiagramScanItem::NativeSource(PathBuf::from(path))
    }

    fn backlink(file_path: &str, source_path: &str) -> DiagramScanItem {
        DiagramScanItem::Backlink(Backlink {
            file_path: PathBuf::from(file_path),
            source_path: PathBuf::from(source_path),
            embedding_type: SupportedEmbeddingType::Markdown,
            buf_position: 0..0,
        })
    }

    // 1. Empty pattern returns all items in original order with score 0.
    #[test]
    fn empty_pattern_returns_all() {
        let items = vec![
            native("/a/foo.ktr"),
            native("/b/bar.ktr"),
            backlink("/notes/readme.md", "/diagrams/arch.ktr"),
        ];
        let result = fuzzy_match_files("", &items);
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|(_, score)| *score == 0));
    }

    // 2. A file whose name contains a contiguous match for the pattern ranks
    //    above a file where the same characters are only loosely scattered.
    //    We use "auth" as the pattern: "auth.ktr" has a contiguous match while
    //    "/zzz/zzz/zzz/zzz/auth-noise.ktr" has it buried deeper.
    #[test]
    fn exact_filename_ranks_first() {
        let items = vec![
            // Deep path — "auth" is still in the filename but many directories
            // precede it, making the path score lower than a shallow match.
            native("/deeply/nested/directory/structure/auth-noise.ktr"),
            // Shallow path — "auth" appears directly as the filename stem.
            native("/projects/auth.ktr"),
        ];
        let result = fuzzy_match_files("auth", &items);
        assert!(!result.is_empty());
        let (first_item, _) = &result[0];
        match first_item {
            DiagramScanItem::NativeSource(p) => {
                assert!(
                    p.to_string_lossy().contains("auth.ktr"),
                    "expected auth.ktr to rank first, got {:?}",
                    p
                )
            }
            _ => panic!("expected NativeSource"),
        }
    }

    // 3. Partial / subsequence match still surfaces the item.
    #[test]
    fn partial_match_works() {
        let items = vec![
            native("/projects/authentication.ktr"),
            native("/projects/unrelated.ktr"),
        ];
        let result = fuzzy_match_files("auth", &items);
        // "authentication" contains the subsequence "auth"; "unrelated" does not.
        assert_eq!(result.len(), 1);
        match &result[0].0 {
            DiagramScanItem::NativeSource(p) => {
                assert!(p.to_string_lossy().contains("authentication"))
            }
            _ => panic!("expected NativeSource"),
        }
    }

    // 4. Items that don't match are excluded entirely.
    #[test]
    fn non_matching_item_excluded() {
        let items = vec![native("/projects/foo.ktr"), native("/projects/bar.ktr")];
        let result = fuzzy_match_files("zzzzz", &items);
        assert!(result.is_empty());
    }

    // 5. NativeSource beats Backlink when fuzzy scores are equal.
    //    We force equal scores by matching on a term that appears verbatim in
    //    both match keys at the same relative position.
    #[test]
    fn native_beats_backlink_on_tie() {
        // Both contain "diagram" in the same position so scores should be equal.
        let items = vec![
            backlink("/notes/notes.md", "/projects/diagram.ktr"),
            native("/projects/diagram.ktr"),
        ];
        let result = fuzzy_match_files("diagram", &items);
        assert_eq!(result.len(), 2);
        match &result[0].0 {
            DiagramScanItem::NativeSource(_) => {} // correct
            DiagramScanItem::Backlink(_) => panic!("NativeSource should rank first"),
        }
    }

    // 6. Matching is case-insensitive.
    #[test]
    fn case_insensitive_match() {
        let items = vec![native("/projects/MyDiagram.ktr")];
        let result = fuzzy_match_files("mydiagram", &items);
        assert_eq!(result.len(), 1);
    }

    // 7. A nonsense pattern yields an empty result.
    #[test]
    fn no_match_returns_empty() {
        let items = vec![
            native("/a/foo.ktr"),
            backlink("/notes/readme.md", "/diagrams/arch.ktr"),
        ];
        let result = fuzzy_match_files("xqzxqzxqz", &items);
        assert!(result.is_empty());
    }

    // 8. Backlink items can be matched via the host file path.
    #[test]
    fn backlink_matches_on_file_path() {
        let items = vec![backlink("/notes/architecture.md", "/diagrams/flow.ktr")];
        let result = fuzzy_match_files("architecture", &items);
        assert_eq!(result.len(), 1);
    }

    // 9. Backlink items can also be matched via the source (diagram) path.
    #[test]
    fn backlink_matches_on_source_path() {
        let items = vec![backlink("/notes/readme.md", "/diagrams/sequence.ktr")];
        let result = fuzzy_match_files("sequence", &items);
        assert_eq!(result.len(), 1);
    }
}
