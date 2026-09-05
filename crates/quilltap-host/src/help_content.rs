//! The shipped help tree, embedded into the binary at compile time (P4.9I2A).
//!
//! v4 captures `HELP_DIR = join(process.cwd(), 'help')` at module load and walks
//! it on every sync; a native binary carries the tree instead (the
//! `seed_assets` precedent — "a native binary carries them"). `build.rs`
//! generates [`EMBEDDED_HELP`] from `<repo>/help/**/*.md` in the runtime
//! walker's exact order; `files_store::embedded_help_source_files` turns it
//! into the core sync's input list, and that ONE table feeds BOTH the boot-time
//! `ensure_help_docs_synced` and the `EMBEDDING_REINDEX_ALL` handler, so the two
//! can never disagree about what the help tree contains.
//!
//! The harness guard `help_tree_embed_guard` holds this table equal to the
//! on-disk `help/` (path set, order, bytes); a stale build cannot pass a gate.

include!(concat!(env!("OUT_DIR"), "/help_embedded.rs"));

/// The number of embedded help documents (the shipped tree is 120 files at v4
/// `d883a5ee1`; the guard test pins the exact set against disk).
pub fn embedded_help_count() -> usize {
    EMBEDDED_HELP.len()
}
