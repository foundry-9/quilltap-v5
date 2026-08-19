//! The `DbError::Key` regrowth guard (P4.50 — dogfood finding #96).
//!
//! `DbError::Key`'s `Display` prepends `"key derivation failed: "`, which is a
//! claim about the *cause*. Before P4.50 the variant was also the crate's
//! general-purpose message carrier: 246 construction sites workspace-wide, of
//! which exactly **two** derived a key. Every other one printed a
//! cipher-flavoured lie in front of its real sentence — which stopped being
//! cosmetic when P4.49 made `combined.log` the place an operator looks after a
//! failed turn, and the first line they found there read
//! `key derivation failed: primary stream failed: HTTP 500 …`.
//!
//! The split (`DbError::Internal(String)`, whose `Display` is the bare message)
//! is only durable if the catch-all cannot silently regrow, so this test walks
//! `crates/**/*.rs` and holds every `DbError::Key(` occurrence against the
//! census below. **The allow-list IS the census** — the two genuine derivation
//! sites plus the `Display` arm that gives them their prefix.
//!
//! Adding a `DbError::Key(` anywhere else fails this test. If a new site really
//! does derive a key from the pepper, add it here with its one-line
//! justification; if it does not, it wants `DbError::Internal`.
//!
//! Sibling pins: `db::db_error_display_tests` in `quilltap-core` (both
//! variants' `Display` bytes).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test db_error_key_guard

use std::path::{Path, PathBuf};

/// `(repo-relative path, expected occurrences, why this file may say `Key`)`.
const CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/db/mod.rs",
        3,
        "the `Display` arm that owns the prefix; `Writer::open_writable`'s \
         `dbkey::pepper_b64_to_key_hex` wrap (a real derivation failure); and \
         `db_error_display_tests`' pin on the surviving prefix bytes",
    ),
    (
        "crates/quilltap-core/src/db/runtime.rs",
        1,
        "`Db::open`'s `dbkey::pepper_b64_to_key_hex` wrap — a real derivation \
         failure",
    ),
];

const NEEDLE: &str = "DbError::Key(";

/// This file names the needle in its prose and its census, so it would flag
/// itself; it is the guard, not a call site.
const SELF: &str = "crates/quilltap-harness/tests/db_error_key_guard.rs";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

/// Every `.rs` file under `crates/`, skipping build output and the vendored
/// SQLite3MC amalgamation's crate (12 MB of C, no Rust of ours).
fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
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

#[test]
fn db_error_key_variant_is_key_only() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files — it is not reaching the tree",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let count = text.matches(NEEDLE).count();
        if count == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == SELF {
            continue;
        }
        seen.push((rel.clone(), count));

        match CENSUS.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {count} `{NEEDLE}` site(s) outside the census. \
                 `Key`'s Display claims a key-derivation cause; unless this \
                 really derives a key from the pepper it wants \
                 `DbError::Internal` (P4.50)."
            )),
            Some((_, expected, _)) if count != *expected => failures.push(format!(
                "{rel}: {count} `{NEEDLE}` site(s), census says {expected}. \
                 A new one in a key-genuine file is still suspect — justify it \
                 in the census or move it to `DbError::Internal`."
            )),
            Some(_) => {}
        }
    }

    for (rel, expected, why) in CENSUS {
        if !seen.iter().any(|(p, _)| p == rel) {
            failures.push(format!(
                "{rel}: census expects {expected} `{NEEDLE}` site(s), found none. \
                 If the key path moved, move the census with it ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the `DbError::Key` catch-all is regrowing:\n  {}",
        failures.join("\n  ")
    );
}
