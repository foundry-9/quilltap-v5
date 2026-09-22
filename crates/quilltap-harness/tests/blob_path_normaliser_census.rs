//! The blob-path-normaliser ONE-HOME census (P4.D209 — v4 `186eb09cb`).
//!
//! v4 has a SINGLE `normaliseBlobRelativePath`, in
//! `lib/mount-index/blob-transcode.ts`. v5 had **five** hand-written copies —
//! `services/mount_index/blob_transcode.rs`, `services/file_storage.rs`,
//! `services/image_job_storage.rs`, `tools/doc_edit/blob.rs` and
//! `tools/generate_image.rs` — each written independently against the same v4
//! source, and each free to drift.
//!
//! That stopped being a tidiness question at `186eb09cb`. Before it, the
//! function rewrote a path at two upload sites. After it, it is part of the
//! WRITE-SIDE CHOKEPOINT: `link_blob_content` normalizes image bytes and then
//! rewrites `relativePath` / `fileName` / `storedMimeType` to agree with what it
//! actually stored. A copy that disagrees by one edge — a dotted directory, an
//! extension-less leaf, an already-`.WEBP` path in the wrong case — writes a row
//! whose path claims one format while the bytes hold another, which is the exact
//! mismatch the function exists to prevent.
//!
//! So: exactly ONE definition in the production zone, and it is the canonical
//! one. Nothing else in the repo can see a sixth copy appearing; a differential
//! only compares the paths a corpus happens to exercise, and four of the five
//! copies were never reachable from any oracle at all.

use std::path::{Path, PathBuf};

/// The one home, as a path relative to `crates/quilltap-core/src`.
const CANONICAL_HOME: &str = "services/mount_index/blob_transcode.rs";

fn core_src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .join("crates/quilltap-core/src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn normalise_blob_relative_path_has_exactly_one_home() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();

    let mut definitions: Vec<(String, usize)> = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read: {e}"));
        // A DEFINITION, not a call and not an import: the `fn` keyword at the
        // start of a line (optionally `pub`), which is how every copy was
        // written and how a sixth would be.
        let count = text
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                (t.starts_with("fn normalise_blob_relative_path(")
                    || t.starts_with("pub fn normalise_blob_relative_path("))
                    && l.len() - t.len() == 0
            })
            .count();
        if count > 0 {
            let rel = file
                .strip_prefix(&root)
                .expect("under core src")
                .to_string_lossy()
                .replace('\\', "/");
            definitions.push((rel, count));
        }
    }

    assert_eq!(
        definitions,
        vec![(CANONICAL_HOME.to_string(), 1usize)],
        "`normalise_blob_relative_path` must have exactly one definition, in \
         {CANONICAL_HOME}. v4 has one; v5 had five until P4.D209 folded them. \
         Found: {definitions:?}. If you are adding a caller, IMPORT the \
         canonical one — a sixth copy is a row whose stored path can disagree \
         with its stored bytes."
    );
}
