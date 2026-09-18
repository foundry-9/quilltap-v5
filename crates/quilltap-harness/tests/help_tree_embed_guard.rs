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
//! It also pins the tree's SIZE against the vendored v4 count (124 files at
//! v4 `5f0a57dc4`): a checkout without `help/` embeds an EMPTY table rather
//! than failing the build, and this is the assertion that refuses it.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test help_tree_embed_guard

use std::path::PathBuf;

use quilltap_host::files_store::{embedded_help_source_files, load_help_source_files};
use quilltap_host::help_content::EMBEDDED_HELP;

/// The shipped tree at the vendored pin (v4 `89fcc3c0d`: still 124 — the
/// P4.D200 re-vendor MODIFIED `help/character-system-transparency.md` alone
/// (bugs 152 + 153, `1065a1f53` + `89fcc3c0d` — the vault-covenant paragraph's
/// "Vaults, and nothing besides" and "Nor are they so much as named" additions)
/// and added none; v4 `bcd7e4852`: still 124 — the
/// P4.D199 re-vendor MODIFIED `help/connection-profiles.md` alone (bug 151,
/// `bcd7e4852` — the "A travelling portrait packs light" section on the
/// transport shrink and the per-turn image byte budget) and added none; v4
/// `5f0a57dc4`: still 124 — the
/// P4.D197 re-vendor MODIFIED `help/image-generation-profiles.md` and
/// `help/provider-recommendations.md` for `d8d2890ee` (PR #62, GPT Image 2.5
/// and the full OpenAI image parameter set) and added none; `53294163f` and
/// `5f0a57dc4` touch no `help/` file at all; v4 `1fefadb9a`: still 124 — the
/// P4.D195 re-vendor MODIFIED `help/chat-turn-manager.md` alone (bug 147,
/// `1fefadb9a` — one sentence on the sidebar reading the same stored running
/// order as the banner) and added none; v4 `2075242f9`: still 124 — the
/// P4.D194 re-vendor MODIFIED `help/database-protection.md` (bug 144,
/// `23abc1ba1`) and `help/chat-turn-manager.md` (bug 146, `2075242f9`) and added
/// none; v4 `ffb6b3119`: still 124 — the P4.D191 re-vendor MODIFIED
/// `help/chats.md` and `help/connection-profiles.md` for bug 141 (`f90144ac4`)
/// and added none; v4 `31436bae4`: still 124 files —
/// that round modified EIGHT and added none; v4 `f4ad2c8d1`: 124 files — the
/// P4.D179 re-vendor added `help/impersonation-voice.md` and re-took four
/// edited files across `686954937` + `f4ad2c8d1`; 123 at `78b381a96` after
/// P4.D175's `help/chat-gallery.md`; 122 at `25f534c0b` after P4.D168's
/// `help/character-progressions.md`; 121 at `2f4254b42` after P4.D163's
/// `help/character-subprompts.md`; 120 at `d883a5ee1`).
const VENDORED_FILE_COUNT: usize = 124;

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
