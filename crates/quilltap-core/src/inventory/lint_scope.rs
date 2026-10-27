//! The title lint's "already linted" scope, and which paths the
//! document-write layer lints (v4 `lib/inventory/lint-scope.ts`, `7263105c6`
//! at `322a5488e`; P4.D340).
//!
//! The Inventory mutations (`item_mutations`) lint a write with the whole
//! context in hand — the chat's project tier, the record before and after —
//! and then project the folder through `db::inventory_writes`, which reaches
//! the document store like any other writer. The document-write layer
//! (`title_lint_documents`) lints writes made by ordinary document tools; it
//! must not lint the mutation layer's projection a second time.
//!
//! v4 keeps the scope in an `AsyncLocalStorage` the store asks. v5 passes it
//! as an EXPLICIT argument instead (T9; the round's ruling — no thread-local
//! twin): every one of the four document-store hooks takes a
//! [`TitleLintScope`] LAST, named `linted`, and because the lint reads the
//! MAIN database (the character behind a vault, the General mount id, the
//! pool's owners) while the hooks hold only the mount index, the unlinted arm
//! carries the main connection (P4.D340 R-1).

use rusqlite::Connection;

/// What a document-store write knows about the title lint (v4's
/// `inventoryWritesAreLinted()` plus the connection the lint needs).
#[derive(Clone, Copy)]
pub enum TitleLintScope<'a> {
    /// Inside `with_inventory_writes_linted`: the Inventory mutations already
    /// linted this write (v4 `scope.run(true, fn)`).
    Linted,
    /// An ordinary writer: the store lints any claimed path, reading `main`.
    Unlinted { main: &'a Connection },
    /// A writer whose path is fixed and never claimed by
    /// [`is_title_lint_path`] (the mailbox, subprompts, scenarios, state
    /// files…) and that holds no main connection (P4.D340 R-2). v4 evaluates
    /// relevance for every write and returns at once on an unclaimed path;
    /// so does v5. A claimed path under this arm is a programming error: a
    /// `debug_assert!` in a debug build, a fail-closed refusal in a release
    /// build (the census test proves every site unclaimed).
    NeverClaimed,
}

impl TitleLintScope<'_> {
    /// v4 `inventoryWritesAreLinted()`.
    pub fn is_linted(&self) -> bool {
        matches!(self, TitleLintScope::Linted)
    }
}

impl std::fmt::Debug for TitleLintScope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TitleLintScope::Linted => "Linted",
            TitleLintScope::Unlinted { .. } => "Unlinted",
            TitleLintScope::NeverClaimed => "NeverClaimed",
        })
    }
}

// ---------------------------------------------------------------------------
// Which paths the document-write layer lints. Plain string tests, so the store
// can decide without loading anything; the Inventory's own predicates
// (`is_inventory_item_document_path`) refine them once loaded.
// ---------------------------------------------------------------------------

/// `s.replace(/^\/+/, '')`.
fn strip_leading_slashes(s: &str) -> &str {
    s.trim_start_matches('/')
}

/// A root-level `Tools/*.tool.json` custom-tool definition (v4
/// `/^tools\/[^/]+\.tool\.json$/i` over the path with LEADING slashes
/// stripped). The `i` flag without `u` folds ASCII letters only — no Unicode
/// case folding reaches an ASCII letter in a non-unicode JS regex.
pub fn is_tool_definition_path(relative_path: &str) -> bool {
    let p = strip_leading_slashes(relative_path).as_bytes();
    const HEAD: &[u8] = b"tools/";
    const TAIL: &[u8] = b".tool.json";
    if p.len() < HEAD.len() + 1 + TAIL.len() {
        return false;
    }
    let (head, rest) = p.split_at(HEAD.len());
    let (stem, tail) = rest.split_at(rest.len() - TAIL.len());
    head.eq_ignore_ascii_case(HEAD)
        && tail.eq_ignore_ascii_case(TAIL)
        && !stem.is_empty()
        && !stem.contains(&b'/')
}

/// `Inventory/` itself or anything inside it (v4: both ends' slashes stripped,
/// then `toLowerCase` — the UNICODE lower, so `İnventory` is not `inventory`).
pub fn is_title_lint_folder(relative_path: &str) -> bool {
    let lower = relative_path.trim_matches('/').to_lowercase();
    lower == "inventory" || lower.starts_with("inventory/")
}

/// A path whose write, delete or move the title lint must see (v4
/// `isTitleLintPath`: a tool definition, or a `.json` — ASCII `i` — under
/// `Inventory/`).
pub fn is_title_lint_path(relative_path: &str) -> bool {
    is_tool_definition_path(relative_path)
        || (is_title_lint_folder(relative_path) && ends_with_ascii_ci(relative_path, ".json"))
}

fn ends_with_ascii_ci(s: &str, suffix: &str) -> bool {
    let (s, suffix) = (s.as_bytes(), suffix.as_bytes());
    s.len() >= suffix.len() && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_title_lint_documents_test_rows() {
        // title-lint-documents.test.ts:64-73.
        assert!(is_title_lint_path("Inventory/tape measure.json"));
        assert!(is_title_lint_path("inventory/Belt/tape.json"));
        assert!(is_title_lint_path("Tools/measure_twice.tool.json"));
        assert!(!is_title_lint_path("Tools/archive/old.tool.json"));
        assert!(!is_title_lint_path("metadata.json"));
        assert!(!is_tool_definition_path("Inventory/x.json"));
        assert!(is_title_lint_folder("Inventory"));
        assert!(!is_title_lint_folder("Inventory Notes"));
    }

    #[test]
    fn slashes_cases_and_extensions() {
        assert!(is_tool_definition_path("/Tools/x.tool.json"));
        assert!(is_tool_definition_path("//TOOLS/X.TOOL.JSON"));
        assert!(!is_tool_definition_path("Tools/x.tool.json/"));
        assert!(!is_tool_definition_path("Tools/.tool.json"));
        assert!(is_title_lint_folder("/Inventory/"));
        assert!(is_title_lint_folder("//inventory//"));
        assert!(!is_title_lint_folder("\u{130}nventory/x.json"));
        assert!(!is_title_lint_path("\u{130}nventory/x.json"));
        assert!(!is_title_lint_path("Inventory.json"));
        assert!(is_title_lint_path("Inventory/x.JSON"));
        assert!(is_title_lint_path("/Inventory/x.json"));
        assert!(!is_title_lint_path("Inventory/x.jsonl"));
    }

    /// R-2's census: every `TitleLintScope::NeverClaimed` writer, by file, with
    /// a representative of the path shape it writes — each must be one the
    /// lint can never claim (a folder move: neither side a lint folder).
    #[test]
    fn title_lint_scope_never_claimed_census() {
        /// `(file, NeverClaimed count, written paths, folder moves)`.
        type Site = (
            &'static str,
            usize,
            &'static [&'static str],
            &'static [(&'static str, &'static str)],
        );
        const SITES: &[Site] = &[
            (
                "post_office/mailbox.rs",
                3,
                &["Mail/2026-10-26-from-friday.md"],
                &[],
            ),
            ("db/scenarios.rs", 2, &["Scenarios/the-ball.md"], &[]),
            (
                "scenarios/location.rs",
                4,
                &["Scenarios/the-ball.md", "Scenarios/the-dance.md"],
                &[],
            ),
            (
                "services/aesthetics.rs",
                2,
                &["lantern-aesthetics.md", "aurora-aesthetics.md"],
                &[],
            ),
            (
                "db/vault_wardrobe_public.rs",
                1,
                &[],
                &[("Wardrobe/Hats", "Wardrobe/Caps")],
            ),
        ];
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found: Vec<(String, usize)> = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&src)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                // The scope's own home, the lint that answers it, and the store
                // that passes it on are not writers.
                if [
                    "inventory/lint_scope.rs",
                    "inventory/title_lint_documents.rs",
                    "db/database_store.rs",
                ]
                .contains(&rel.as_str())
                {
                    continue;
                }
                let n = std::fs::read_to_string(&path)
                    .unwrap()
                    .matches("TitleLintScope::NeverClaimed")
                    .count();
                if n > 0 {
                    found.push((rel, n));
                }
            }
        }
        found.sort();
        let mut want: Vec<(String, usize)> = SITES
            .iter()
            .map(|(f, n, _, _)| (f.to_string(), *n))
            .collect();
        want.sort();
        assert_eq!(
            found, want,
            "a NeverClaimed writer was added or removed: measure its paths"
        );
        for (file, _, paths, moves) in SITES {
            for p in *paths {
                assert!(!is_title_lint_path(p), "{file}: {p} is claimed");
            }
            for (from, to) in *moves {
                assert!(
                    !is_title_lint_folder(from) && !is_title_lint_folder(to),
                    "{file}: {from} -> {to}"
                );
            }
        }
    }

    #[test]
    fn scope_is_linted() {
        assert!(TitleLintScope::Linted.is_linted());
        assert!(!TitleLintScope::NeverClaimed.is_linted());
        let main = Connection::open_in_memory().unwrap();
        assert!(!TitleLintScope::Unlinted { main: &main }.is_linted());
    }
}
