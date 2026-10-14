//! The memory-row DELETE census (P4.D288, C1-7 — v4 `af48f6cc2`, memory
//! refactor Phase B: "the one deletion path").
//!
//! **Every memory row is deleted through ONE path.** v4's `retireMemories`
//! (`lib/memory/retire.ts`) scrubs every link that names a row, evicts its
//! vector and announces the retirement; a bare row delete leaves all of that
//! behind (bugs 200, 225). v4 fences it with a test that greps for any caller
//! of `MemoriesRepository.bulkDeleteUnsafe` other than `retireMemories`, and
//! refuses the bare `delete` (ported as its REMOVAL, R-F).
//!
//! v5's fence holds the same intent over `crates/quilltap-core/src`'s
//! production zone (comments and `#[cfg(test)]` items stripped by the shared
//! `source_census` lexer), two ways:
//!
//! 1. the files that spell `bulk_delete_unsafe(` — `db/memories.rs` (the
//!    definition, no call) and `services/memory_retire.rs` (one call);
//! 2. the string literals that spell `DELETE FROM memories` — exactly one,
//!    inside the definition.
//!
//! A new site is a red here, by name: route it through
//! `services::memory_retire::retire_memories` instead.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test memory_retire_sole_deleter_guard

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources, string_literals};

const IDENT: &str = "bulk_delete_unsafe(";

/// `(path under crates/quilltap-core/src, definitions, calls, DELETE literals, why)`.
const CENSUS: &[(&str, usize, usize, usize, &str)] = &[
    (
        "db/memories.rs",
        1,
        0,
        1,
        "the row delete's definition (v4 `MemoriesRepository.bulkDeleteUnsafe`) — the \
         one `DELETE FROM memories` statement; its own module never calls it.",
    ),
    (
        "services/memory_retire.rs",
        0,
        1,
        0,
        "the ONE retire path (v4 `retireMemories`): the sanctioned caller.",
    ),
];

/// `(definitions, calls)` of [`IDENT`] in a code-only zone (a match must not
/// continue an identifier on its left; one preceded by `fn` is the definition).
fn count(code: &str) -> (usize, usize) {
    let (mut defs, mut calls) = (0usize, 0usize);
    let mut from = 0usize;
    while let Some(at) = code[from..].find(IDENT) {
        let abs = from + at;
        let prev_ok = abs == 0
            || !code.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && code.as_bytes()[abs - 1] != b'_';
        if prev_ok {
            if code[..abs].trim_end().ends_with("fn") {
                defs += 1;
            } else {
                calls += 1;
            }
        }
        from = abs + IDENT.len();
    }
    (defs, calls)
}

/// String literals spelling a `DELETE FROM memories` (either quoting).
fn delete_literals(zone: &str) -> usize {
    string_literals(zone)
        .iter()
        .filter(|lit| {
            let flat = lit.replace("\\\"", "\"").replace('"', "");
            flat.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_uppercase()
                .contains("DELETE FROM MEMORIES ")
        })
        .count()
}

fn census_view() -> Vec<(String, usize, usize, usize)> {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(&path).unwrap();
        let zone = production_zone(&src);
        let (defs, calls) = count(&code_only(&zone));
        let deletes = delete_literals(&zone);
        if defs + calls + deletes > 0 {
            out.push((rel, defs, calls, deletes));
        }
    }
    out
}

#[test]
fn only_the_retire_path_deletes_memory_rows() {
    let view = census_view();
    let got: Vec<(&str, usize, usize, usize)> = view
        .iter()
        .map(|(p, d, c, x)| (p.as_str(), *d, *c, *x))
        .collect();
    let want: Vec<(&str, usize, usize, usize)> = CENSUS
        .iter()
        .map(|(p, d, c, x, _)| (*p, *d, *c, *x))
        .collect();
    assert_eq!(
        got, want,
        "the production files that delete memory rows moved. Every deletion goes through \
         `services::memory_retire::retire_memories` (v4 `af48f6cc2`'s one path), which \
         scrubs the links, evicts the vectors and is announced after the commit — route \
         the new site through it rather than adding a row here."
    );
}

/// The counters see code only: a comment, a `#[cfg(test)]` module and a
/// longer identifier do not count; a call, a definition and a DELETE literal
/// (either quoting, any spacing) do.
#[test]
fn the_counters_see_only_code() {
    let src = r#"
        // repo.bulk_delete_unsafe(a, b) in a comment; DELETE FROM memories
        pub fn bulk_delete_unsafe(&self) { let s = "DELETE FROM \"memories\"  WHERE id = ?"; }
        fn not_bulk_delete_unsafe(x: u8) {}
        fn site() { repo.bulk_delete_unsafe(&a, &b); }
        #[cfg(test)]
        mod tests { fn t() { repo.bulk_delete_unsafe(&a, &b); let q = "DELETE FROM memories WHERE 1"; } }
    "#;
    let zone = production_zone(src);
    assert_eq!(count(&code_only(&zone)), (1, 1));
    assert_eq!(delete_literals(&zone), 1);
}
