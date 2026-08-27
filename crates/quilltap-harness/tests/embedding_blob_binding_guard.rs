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
//!    blob-column registration mechanism (the sole allowed hit is the doc
//!    comment in `db/help_docs.rs` that quotes this very grep).
//! 2. **Every embedding-bearing module still encodes at the binding site.**
//!    Each module owning a table with an `embedding` column references
//!    `float32_to_blob(` — the needle carries its paren so that a module
//!    switching to `float32_to_blob_raw` (the headerless LEGACY encoder, kept
//!    only for round-trips) cannot satisfy it by substring. Drop or downgrade
//!    the encode in any of them and this reddens.
//! 3. **One encoder, not several.** `float32_to_blob` is defined exactly once,
//!    so "the single source of truth" is a fact rather than a comment.
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

/// The one file allowed to name the mechanism: its header quotes the grep as
/// part of recording why the bug has no v5 analog.
const REGISTRY_ALLOWED: &str = "crates/quilltap-core/src/db/help_docs.rs";

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

    for path in all_sources(&root) {
        let rel = rel_of(&root, &path);
        if rel == SELF || rel == REGISTRY_ALLOWED {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        for needle in REGISTRY_NEEDLES {
            if text.contains(needle) {
                failures.push(format!(
                    "{rel}: names `{needle}`. v5 has no blob-column registry and \
                     must not grow one — an embedding is encoded at its binding \
                     site via `float32_to_blob`, which is why v4's \
                     JSON-text-embedding bug (pinned v4-side by `487ae57fe`) has \
                     no analog here. Do not add a registration mechanism in \
                     order to have something to register."
                ));
            }
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
        let count = text.matches("float32_to_blob(").count();
        if count < *minimum {
            failures.push(format!(
                "{rel}: {count} `float32_to_blob(` call site(s), census expects at \
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
        // headerless legacy format); only the canonical name is counted.
        for line in text.lines() {
            if line.contains("fn float32_to_blob(") {
                sites.push(rel.clone());
            }
        }
    }

    assert_eq!(
        sites,
        vec!["crates/quilltap-core/src/embedding_blob.rs".to_string()],
        "`float32_to_blob` must have exactly one definition — it is the single \
         source of truth for the on-disk embedding encoding"
    );
}
