//! The embedded-help-tree guard (P4.9I2A, Tier 1 item 1d).
//!
//! `quilltap-host`'s `build.rs` embeds `<repo>/help/**/*.md` into the binary as
//! `help_content::EMBEDDED_HELP`, and `files_store::embedded_help_source_files`
//! is the ONE source both the boot-time `ensure_help_docs_synced` and the
//! `EMBEDDING_REINDEX_ALL` handler read. This test holds that table equal to
//! the on-disk tree walked by the production filesystem walker
//! (`load_help_source_files`, v4's `findMarkdownFiles` order): the same path
//! SET, the same ORDER, the same BYTES. A build whose embedded table went stale
//! against the checkout (an edited or added help file the build did not see)
//! cannot pass a gate.
//!
//! It also pins the tree's SIZE against the vendored v4 count (120 files at
//! v4 `d883a5ee1`): a checkout without `help/` embeds an EMPTY table rather
//! than failing the build, and this is the assertion that refuses it.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test help_tree_embed_guard

use std::path::PathBuf;

use quilltap_host::files_store::{embedded_help_source_files, load_help_source_files};
use quilltap_host::help_content::EMBEDDED_HELP;

/// The shipped tree at the vendored pin (v4 `d883a5ee1`: 120 files, 1.6 MB).
const VENDORED_FILE_COUNT: usize = 120;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

#[test]
fn embedded_table_equals_the_on_disk_help_tree() {
    let embedded = embedded_help_source_files();
    let on_disk = load_help_source_files(&repo_root());

    assert_eq!(
        embedded.len(),
        VENDORED_FILE_COUNT,
        "the embedded help table must carry the vendored tree ({VENDORED_FILE_COUNT} files) — \
         an empty or partial table means the build did not see <repo>/help/"
    );
    assert_eq!(
        on_disk.len(),
        VENDORED_FILE_COUNT,
        "the on-disk help/ tree must carry the vendored {VENDORED_FILE_COUNT} files"
    );

    // Path set + ORDER (the walker's raw readdir order, mirrored in build.rs).
    let embedded_paths: Vec<&str> = embedded.iter().map(|f| f.rel_path.as_str()).collect();
    let disk_paths: Vec<&str> = on_disk.iter().map(|f| f.rel_path.as_str()).collect();
    assert_eq!(
        embedded_paths, disk_paths,
        "the embedded table's path list (set AND order) must equal the production walker's"
    );

    // Bytes, file by file.
    for (e, d) in embedded.iter().zip(on_disk.iter()) {
        assert_eq!(e.rel_path, d.rel_path);
        assert!(
            e.raw_content == d.raw_content,
            "embedded bytes for {} differ from disk — a stale build",
            e.rel_path
        );
    }

    // Every rel_path is v4's `relative(process.cwd(), filePath)` shape.
    for (rel, _) in EMBEDDED_HELP {
        assert!(
            rel.starts_with("help/") && rel.ends_with(".md"),
            "unexpected embedded rel_path {rel:?}"
        );
    }
}
