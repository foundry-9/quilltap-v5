//! The blob-column registry guard (P4.D129 — the `dcab791c2`-round rider on
//! v4's `487ae57fe`).
//!
//! v4 carries a runtime registry: `manager.ts` calls
//! `registerBlobColumns(<table>, ['embedding'])` for five tables, and the
//! document mapper consults it to decide whether a value is binary. Forgetting
//! to re-assert that registration on a fresh backend is not a cosmetic slip —
//! the write path then persists an index-keyed JSON object (`{"0":…}`) where a
//! BLOB belongs, which is how v4's "legacy JSON-text embeddings" were minted in
//! the first place. `487ae57fe` pins the trap v4-side with a new
//! `help-doc-chunks.repository` test asserting the registration is re-asserted
//! on EVERY collection access rather than remembered on the instance.
//!
//! **v5 cannot reproduce the bug, and this test is why that stays true.** The
//! port abandoned v4's generic document mapper: there is no registry to
//! populate, no cached instance flag, and no `JSON.stringify` fallback a
//! Float32 vector could land in. Every embedding write encodes explicitly at
//! the binding site through [`quilltap_core::embedding_blob::float32_to_blob`].
//! `db/help_docs.rs` and `db/help_doc_chunks.rs` both record that finding at
//! length, and both end with the same instruction: **do not add a registration
//! mechanism in order to have something to register.**
//!
//! A prose instruction is not a pin, so this test makes the two halves of the
//! claim executable:
//!
//! 1. **No registry may grow back.** Nothing under `crates/` may name a
//!    blob-column registration mechanism *in code* — anywhere, with no
//!    exceptions — and the only place allowed to name it in PROSE is the
//!    censused line of `db/help_docs.rs` that quotes this very grep.
//! 2. **Every embedding-bearing module still encodes at the binding site.**
//!    Each module owning a table with an `embedding` column references
//!    `float32_to_blob(` — the needle carries its paren so that a module
//!    switching to `float32_to_blob_raw` (the headerless LEGACY encoder, kept
//!    only for round-trips) cannot satisfy it by substring. Drop or downgrade
//!    the encode in any of them and this reddens.
//! 3. **One encoder, not several.** `float32_to_blob` is defined exactly once,
//!    so "the single source of truth" is a fact rather than a comment.
//!
//! ⚠ [P4.63] Both censuses used to be plain substring counts over the whole
//! file, which made each of them looser than it read: the registry arm skipped
//! `help_docs.rs` ENTIRELY (so a real registration mechanism could have grown
//! inside the one module that explains why none may exist), and the encode arm
//! counted a mention in a doc comment as a call site (so an encode could be
//! deleted while a comment kept the census green). Both are now split by
//! [`count_hits`] into CODE and COMMENT hits, and the file-wide skip is a
//! per-site census with exact counts.
//!
//! Sibling pins: `embedding_blob`'s own round-trip tests (the byte format),
//! `legacy_embedding_equivalence` (read-side recovery for genuinely old v4
//! rows, which MUST stay — v5 cannot mint that shape, only inherit it).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test embedding_blob_binding_guard

use std::path::{Path, PathBuf};

/// The registry mechanism, in every spelling worth refusing. v4's own name is
/// `registerBlobColumns`; the snake_case and SCREAMING forms are what a Rust
/// port of it would be called.
const REGISTRY_NEEDLES: &[&str] = &["register_blob", "blob_columns", "BLOB_COLUMNS"];

/// `(repo-relative path, needle, expected COMMENT occurrences, why)`.
///
/// ⚠ [P4.63] This was a whole-FILE exemption (`help_docs.rs` skipped entirely),
/// which excused far more than the site it exists for: a real
/// `register_blob_columns()` mechanism could have grown inside that very file —
/// the one module whose header explains why no such mechanism may exist —
/// and this guard would have said nothing. The allowance is now per SITE, in
/// the `db_error_key_guard` idiom: an exact count, on an exact needle, in an
/// exact file, and only ever in PROSE.
///
/// The rule that needs no census: **a CODE hit is refused everywhere**,
/// help_docs.rs included. A comment may quote the grep; nothing may call it.
const REGISTRY_ALLOWED: &[(&str, &str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/db/help_docs.rs",
        "register_blob",
        1,
        "the module header quotes this very grep as part of recording why v4's \
         unregistered-blob-column bug has no v5 analog",
    ),
    (
        "crates/quilltap-core/src/db/help_docs.rs",
        "blob_columns",
        1,
        "same header line — the grep names all three spellings at once",
    ),
    (
        "crates/quilltap-core/src/db/help_docs.rs",
        "BLOB_COLUMNS",
        1,
        "same header line — the grep names all three spellings at once",
    ),
];

/// `(module, minimum `float32_to_blob` references, which table it owns)`.
///
/// The first five are exactly the tables v4's `manager.ts` registers; the sixth
/// is v5's own (mount-index document chunks), which v4 reaches by another door.
const ENCODE_CENSUS: &[(&str, usize, &str)] = &[
    ("crates/quilltap-core/src/db/memories.rs", 1, "memories"),
    (
        "crates/quilltap-core/src/db/vector_indices.rs",
        1,
        "vector_entries",
    ),
    (
        "crates/quilltap-core/src/db/conversation_chunks.rs",
        1,
        "conversation_chunks",
    ),
    ("crates/quilltap-core/src/db/help_docs.rs", 1, "help_docs"),
    (
        "crates/quilltap-core/src/db/help_doc_chunks.rs",
        1,
        "help_doc_chunks",
    ),
    (
        "crates/quilltap-core/src/db/doc_mount_chunks.rs",
        1,
        "doc_mount_chunks",
    ),
];

/// This file names every needle in its prose and its censuses; it is the guard,
/// not a call site.
const SELF: &str = "crates/quilltap-harness/tests/embedding_blob_binding_guard.rs";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

/// Every `.rs` file under `crates/`, skipping build output and the vendored
/// SQLite3MC amalgamation (12 MB of C, no Rust of ours).
fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if path.is_dir() {
            if name == "target" || name == "vendor" {
                continue;
            }
            rust_sources(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// A needle's occurrences, split by where they sit.
#[derive(Default, PartialEq, Eq, Debug)]
struct Hits {
    code: usize,
    comment: usize,
}

/// Count `needle` in `text`, separating CODE hits from COMMENT hits.
///
/// ⚠ [P4.63] The censuses used to be a bare `text.matches(needle).count()`, so
/// a mention in a doc comment satisfied them — an encode site could be deleted
/// and the census stay green as long as some comment still said the words. That
/// is the vacuity this exists to close: `ENCODE_CENSUS` now counts CODE hits
/// only, and `REGISTRY_ALLOWED` allows COMMENT hits only.
///
/// A hit is a COMMENT hit when it sits inside a `/* … */` block (nesting
/// counted, as Rust allows) or when its own line carries a `//` at any earlier
/// column. That second rule is deliberately syntactic rather than a full lexer:
/// the one way it can be wrong is a `//` inside a string literal *before* the
/// needle on the same line, which would demote a code hit to a comment hit —
/// i.e. it under-counts CODE, which REDDENS the encode census rather than
/// silently passing it. Failing loud is the direction this guard wants.
fn count_hits(text: &str, needle: &str) -> Hits {
    let mut hits = Hits::default();
    let mut block_depth = 0usize;
    for line in text.lines() {
        let mut line_comment = false;
        let mut i = 0usize;
        while i < line.len() {
            let rest = &line[i..];
            if block_depth > 0 {
                if rest.starts_with("*/") {
                    block_depth -= 1;
                    i += 2;
                    continue;
                }
                if rest.starts_with("/*") {
                    block_depth += 1;
                    i += 2;
                    continue;
                }
            } else if !line_comment {
                if rest.starts_with("/*") {
                    block_depth += 1;
                    i += 2;
                    continue;
                }
                if rest.starts_with("//") {
                    line_comment = true;
                    i += 2;
                    continue;
                }
            }
            if rest.starts_with(needle) {
                if block_depth > 0 || line_comment {
                    hits.comment += 1;
                } else {
                    hits.code += 1;
                }
                i += needle.len();
                continue;
            }
            i += 1;
            while i < line.len() && !line.is_char_boundary(i) {
                i += 1;
            }
        }
    }
    hits
}

fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("under the repo root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn all_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files — it is not reaching the tree",
        files.len()
    );
    files
}

#[test]
fn no_blob_column_registry_grows_back() {
    let root = repo_root();
    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<(String, String, usize)> = Vec::new();

    for path in all_sources(&root) {
        let rel = rel_of(&root, &path);
        if rel == SELF {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        for needle in REGISTRY_NEEDLES {
            let hits = count_hits(&text, needle);
            // A CALL SITE is refused everywhere — there is no censused file for
            // this arm, help_docs.rs included.
            if hits.code > 0 {
                failures.push(format!(
                    "{rel}: {} CODE site(s) naming `{needle}`. v5 has no \
                     blob-column registry and must not grow one — an embedding \
                     is encoded at its binding site via `float32_to_blob`, \
                     which is why v4's JSON-text-embedding bug (pinned v4-side \
                     by `487ae57fe`) has no analog here. Do not add a \
                     registration mechanism in order to have something to \
                     register.",
                    hits.code
                ));
            }
            if hits.comment == 0 {
                continue;
            }
            seen.push((rel.clone(), (*needle).to_string(), hits.comment));
            match REGISTRY_ALLOWED
                .iter()
                .find(|(p, n, ..)| *p == rel && *n == *needle)
            {
                None => failures.push(format!(
                    "{rel}: names `{needle}` in prose ({} hit(s)) outside the \
                     census. Explaining the mechanism is allowed only where the \
                     absence of it is being recorded; add the site here with its \
                     reason, or drop the mention.",
                    hits.comment
                )),
                Some((_, _, expected, why)) if hits.comment != *expected => failures.push(format!(
                    "{rel}: {} prose mention(s) of `{needle}`, census says \
                         {expected}. The allowance is per site, not per file \
                         ({why}) — re-count it or drop the new mention.",
                    hits.comment
                )),
                Some(_) => {}
            }
        }
    }

    for (rel, needle, expected, why) in REGISTRY_ALLOWED {
        if !seen.iter().any(|(p, n, _)| p == rel && n == needle) {
            failures.push(format!(
                "{rel}: census expects {expected} prose mention(s) of \
                 `{needle}`, found none. If the explanation moved, move the \
                 census with it ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "a blob-column registry is growing back:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn every_embedding_table_encodes_at_the_binding_site() {
    let root = repo_root();
    let mut failures: Vec<String> = Vec::new();

    for (rel, minimum, table) in ENCODE_CENSUS {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "read {rel}: {e} — if the `{table}` module moved, move the \
                 census with it"
            )
        });
        // CODE hits only: a doc comment saying `float32_to_blob(` is not an
        // encode, and before P4.63 it counted as one.
        let count = count_hits(&text, "float32_to_blob(").code;
        if count < *minimum {
            failures.push(format!(
                "{rel}: {count} `float32_to_blob(` CALL site(s), census expects at \
                 least {minimum}. `{table}` carries an `embedding` column; a \
                 write that does not encode through `float32_to_blob` is the \
                 shape v4's unregistered blob column used to persist."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "an embedding column lost its binding-site encode:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn float32_to_blob_has_exactly_one_definition() {
    let root = repo_root();
    let mut sites: Vec<String> = Vec::new();

    for path in all_sources(&root) {
        let rel = rel_of(&root, &path);
        if rel == SELF {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        // `float32_to_blob_raw` is a distinct, deliberately-kept encoder (the
        // headerless legacy format); only the canonical name is counted — and
        // only where it is DECLARED, never where a comment names it.
        for _ in 0..count_hits(&text, "fn float32_to_blob(").code {
            sites.push(rel.clone());
        }
    }

    assert_eq!(
        sites,
        vec!["crates/quilltap-core/src/embedding_blob.rs".to_string()],
        "`float32_to_blob` must have exactly one definition — it is the single \
         source of truth for the on-disk embedding encoding"
    );
}

/// The classifier is load-bearing for both censuses, so it is pinned rather
/// than trusted: every shape the walk has to get right, in one place.
#[test]
fn the_hit_classifier_separates_code_from_comment() {
    let cases: &[(&str, Hits)] = &[
        (
            "let x = float32_to_blob(v);",
            Hits {
                code: 1,
                comment: 0,
            },
        ),
        (
            "// float32_to_blob(v) used to live here",
            Hits {
                code: 0,
                comment: 1,
            },
        ),
        (
            "//! see float32_to_blob(",
            Hits {
                code: 0,
                comment: 1,
            },
        ),
        (
            "let x = float32_to_blob(v); // float32_to_blob( again",
            Hits {
                code: 1,
                comment: 1,
            },
        ),
        (
            "/* float32_to_blob(\n   float32_to_blob( */ float32_to_blob(v)",
            Hits {
                code: 1,
                comment: 2,
            },
        ),
        (
            "/* outer /* inner float32_to_blob( */ still comment float32_to_blob( */ float32_to_blob(v)",
            Hits {
                code: 1,
                comment: 2,
            },
        ),
        // Multi-byte text on the line must not desync the byte walk.
        (
            "// ⚠ float32_to_blob( — banked\nlet y = float32_to_blob(w);",
            Hits {
                code: 1,
                comment: 1,
            },
        ),
    ];
    for (text, want) in cases {
        assert_eq!(
            &count_hits(text, "float32_to_blob("),
            want,
            "input: {text:?}"
        );
    }
}
