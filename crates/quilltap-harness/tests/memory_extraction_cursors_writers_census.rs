//! P4.D296 (v4 `ac6348c5f`, pin `83334b9a5`) — who writes
//! `memory_extraction_cursors`, by census. Replaces
//! `other_extraction_watermark_writers_census` (the column it guarded was
//! DROPPED by P4.D288; the position it held lives in this table).
//!
//! v4 writes the table through ONE repository (`MemoryExtractionCursorsRepository
//! .setCursor` / `.clearForChat`, `memory-extraction-cursors.repository.ts`), the
//! migration that created it (`add-memory-extraction-cursors-v1`'s backfill), and
//! the chat's `ON DELETE CASCADE`. The extraction pipeline's cursor store
//! (`cursors.ts` `liveCursorStore`) is the only per-turn / per-fold caller; the
//! carriers (backup / restore / `.qtap` import, `cursor-bundle.ts`) restore
//! rows. The rule pinned here, so a new writer must be classified:
//!
//!   1. the only production SQL literals naming the table beside an INSERT /
//!      UPDATE / DELETE are the repository's own (its upsert, its clears, and
//!      its DDL's `ON DELETE CASCADE`) and the boot ensure's backfill
//!      (`db/memory_extraction_cursors_repair.rs`, v4's migration re-homed);
//!   2. outside the repository, production code names its table constant in
//!      exactly three places: that ensure, and two `table_exists` READ probes —
//!      `db/memory_episode_watermark_repair.rs` (the one state v4's chain
//!      reaches only when the table is absent) and the carriers'
//!      `services/carriers/cursor_bundle.rs` (`load_bundle_cursors` reading an
//!      instance whose table is absent as no cursors);
//!   3. the only production callers of the repository's write verbs
//!      (`set_cursor`, `set_cursor_raw`, `clear_for_chat`) outside the
//!      repository are the pipeline's `CursorStore`
//!      (`services/extraction/cursors.rs`), the carriers
//!      (`services/carriers/cursor_bundle.rs`) and the chat regenerate's wipe
//!      (`services/memory_regenerate_job.rs`, v4 `memory-regenerate-chat.ts`'s
//!      `clearForChat`). Standing on the shared
//! source-census lexer (`source_census`).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test memory_extraction_cursors_writers_census

mod source_census;

use std::path::PathBuf;

use source_census::{
    code_only, contains_word, core_src_root, production_zone, rust_sources, string_literals,
};

const TABLE: &str = "memory_extraction_cursors";

/// Every production file of the three crates that hold server code, with its
/// production zone (test items stripped, literals kept).
fn production_files() -> Vec<(String, String)> {
    let crates = core_src_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/")
        .to_path_buf();
    let mut out = Vec::new();
    for krate in ["quilltap-core", "quilltap-web", "quilltap-host"] {
        let mut files: Vec<PathBuf> = Vec::new();
        rust_sources(&crates.join(krate).join("src"), &mut files);
        files.sort();
        for f in files {
            let src = std::fs::read_to_string(&f).expect("read source");
            let rel = f
                .strip_prefix(&crates)
                .expect("under crates/")
                .display()
                .to_string();
            out.push((rel, production_zone(&src)));
        }
    }
    out
}

#[test]
fn only_the_boot_ensure_writes_the_table_by_name() {
    let mut writers: Vec<String> = Vec::new();
    for (file, zone) in production_files() {
        for lit in string_literals(&zone) {
            let writes = ["INSERT", "UPDATE", "DELETE"]
                .iter()
                .any(|verb| contains_word(lit, verb));
            if writes && lit.contains(TABLE) {
                writers.push(file.clone());
            }
        }
    }
    writers.dedup();
    assert_eq!(
        writers,
        vec![
            "quilltap-core/src/db/memory_extraction_cursors.rs".to_string(),
            "quilltap-core/src/db/memory_extraction_cursors_repair.rs".to_string(),
        ],
        "a production SQL literal writes memory_extraction_cursors outside the boot \
         ensure — route it through the repository's verbs"
    );
}

#[test]
fn outside_the_repository_three_files_name_the_table_constant() {
    let mut users: Vec<String> = Vec::new();
    for (file, zone) in production_files() {
        if file.ends_with("db/memory_extraction_cursors.rs") {
            continue;
        }
        let code = code_only(&zone);
        let imports_it = code.contains("memory_extraction_cursors::{") && {
            let at = code.find("memory_extraction_cursors::{").unwrap();
            let end = at + code[at..].find('}').unwrap_or(0);
            contains_word(&code[at..end], "TABLE")
        };
        if code.contains("memory_extraction_cursors::TABLE") || imports_it {
            users.push(file);
        }
    }
    assert_eq!(
        users,
        vec![
            "quilltap-core/src/db/memory_episode_watermark_repair.rs".to_string(),
            "quilltap-core/src/db/memory_extraction_cursors_repair.rs".to_string(),
            "quilltap-core/src/services/carriers/cursor_bundle.rs".to_string(),
        ],
        "a new production file names the cursor table's constant — classify it \
         (a read probe, or a writer that belongs in the repository)"
    );
}

#[test]
fn only_the_cursor_store_the_carriers_and_the_regenerate_call_the_write_verbs() {
    let mut callers: Vec<String> = Vec::new();
    for (file, zone) in production_files() {
        if file.ends_with("db/memory_extraction_cursors.rs") {
            continue;
        }
        let code = code_only(&zone);
        for verb in [".set_cursor(", ".set_cursor_raw(", ".clear_for_chat("] {
            for _ in code.matches(verb) {
                callers.push(format!("{file} {verb}"));
            }
        }
    }
    assert_eq!(
        callers,
        vec![
            "quilltap-core/src/services/carriers/cursor_bundle.rs .set_cursor(".to_string(),
            "quilltap-core/src/services/extraction/cursors.rs .set_cursor_raw(".to_string(),
            "quilltap-core/src/services/memory_regenerate_job.rs .clear_for_chat(".to_string(),
        ],
        "a new production caller writes extraction cursors — classify it (v4's \
         `setCursor` / `clearForChat` callers are the cursor store, the carriers \
         and the regenerate handlers) and update this census"
    );
}

/// The census must bite: the repository still declares the verbs it counts.
#[test]
fn the_counted_verbs_exist() {
    let src = std::fs::read_to_string(core_src_root().join("db/memory_extraction_cursors.rs"))
        .expect("memory_extraction_cursors.rs");
    for verb in [
        "pub fn set_cursor(",
        "pub fn set_cursor_raw(",
        "pub fn clear_for_chat(",
    ] {
        assert!(
            src.contains(verb),
            "{verb} moved — the census counts nothing"
        );
    }
    assert!(src.contains(&format!("pub const TABLE: &str = \"{TABLE}\";")));
}
