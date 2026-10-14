//! P4.D265 (v4 `d58548051`) — the fold-grain OTHER pass's watermark,
//! `chats.otherExtractionWatermarkMessageId`, has ONE writer, by census.
//!
//! v4 makes the column PATCH-ONLY on `chats` (`ChatsRepository.patchOnlyFields`,
//! `chats.repository.ts:560-564`, beside the Concierge trio): a whole-row
//! `update` leaves it out of its `$set` unless the patch names it, so a stale
//! snapshot cannot rewind the watermark. v5's `ChatUpdate` writes only the
//! columns it names, so the rewind hazard cannot arise — the RULE is kept
//! instead (the Concierge precedent, `concierge_state_writers_census`), and
//! pinned here:
//!
//!   1. no production SQL `UPDATE`s the column but
//!      `ChatsRepository::set_other_extraction_watermark`'s;
//!   2. `ChatUpdate` has no field for it.
//!
//! `ChatCreate` may carry it (a restore / import create), as v4's Zod create
//! does. Standing on the shared source-census lexer (`source_census`), which
//! keeps string literals — the SQL IS a literal.
//!
//! **P4.D288 (v4 `add-memory-extraction-cursors-v1`, `83334b9a5`):** the
//! column is DROPPED — the OTHER pass's position lives in
//! `memory_extraction_cursors` (pass `other-fold`) and
//! `set_other_extraction_watermark` is deleted, so the ONE writer became
//! ZERO. The census stands (mechanically restated) until P4.D296 retires it
//! (the order's §R.13(i)): no production SQL `UPDATE`s the column, and
//! `ChatUpdate` has no field for it.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test other_extraction_watermark_writers_census

mod source_census;

use std::path::PathBuf;

use source_census::{contains_word, core_src_root, production_zone, rust_sources, string_literals};

const COLUMN: &str = "otherExtractionWatermarkMessageId";

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
fn only_set_other_extraction_watermark_updates_the_column() {
    let mut writers: Vec<String> = Vec::new();
    for (file, zone) in production_files() {
        for lit in string_literals(&zone) {
            if contains_word(lit, "UPDATE") && lit.contains(COLUMN) {
                writers.push(format!(
                    "{file}: {}",
                    lit.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
            }
        }
    }
    // P4.D288: the column is retired — ZERO writers (the cursor table holds
    // the position).
    assert!(
        writers.is_empty(),
        "the retired OTHER-pass watermark column gained a writer (P4.D288 moved it \
         into memory_extraction_cursors); found:\n{}",
        writers.join("\n")
    );
}

#[test]
fn chat_update_has_no_watermark_field() {
    let src = std::fs::read_to_string(core_src_root().join("db/chats.rs")).expect("chats.rs");
    let start = src
        .find("pub struct ChatUpdate {")
        .expect("ChatUpdate is declared in db/chats.rs");
    let body = &src[start..start + src[start..].find("\n}\n").expect("ChatUpdate closes")];
    assert!(
        !body.contains("other_extraction_watermark"),
        "ChatUpdate carries the watermark — v4's `patchOnlyFields` rule says only \
         set_other_extraction_watermark may write it"
    );
    // The comparand must bite: the struct is the one the census thinks it is.
    assert!(
        body.contains("pub title: Option<String>"),
        "ChatUpdate shape moved"
    );
    // P4.D288: …and the retired writer stays gone.
    assert!(!src.contains("pub fn set_other_extraction_watermark("));
}
