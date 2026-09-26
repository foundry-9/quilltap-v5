//! P4.D226 (v4 `4d370a90f`, #75) — the Concierge state's writers, by census.
//!
//! v4 makes `conciergeMode` / `conciergeModeSetBy` / `conciergeModeReason`
//! PATCH-ONLY on `chats` (`ChatsRepository.patchOnlyFields`): a whole-row
//! `update` leaves them out of its `$set` unless the patch names them, and the
//! one sanctioned writer is `applyConciergeFlip` through `setConciergeMode`.
//! v5's `ChatUpdate` writes only the columns it names, so the rewind hazard
//! the hook closes cannot arise — the RULE is kept instead, and pinned here:
//!
//!   1. no production SQL `UPDATE`s the trio but `set_concierge_mode`'s;
//!   2. `ChatUpdate` has no field for any of the three (so no generic patch can
//!      ever carry one — the order's M2);
//!   3. nothing `UPDATE`s `conciergeOverride` any more (the legacy column is
//!      read-only from `4d370a90f`; `ChatCreate` may still carry a bundle's
//!      value, as v4's Zod create does at this pin);
//!   4. no production code outside the writer EMITS a retired manual kind
//!      (`manual-flagged` … `auto-flagged-refusals`): they stay DECODABLE for
//!      old transcripts, never written again (the order's M9).
//!
//! Standing on the shared source-census lexer (`source_census`), which keeps
//! string literals — the SQL IS a literal.

mod source_census;

use std::path::PathBuf;

use source_census::{
    code_only, contains_word, core_src_root, production_zone, rust_sources, string_literals,
};

const TRIO: [&str; 3] = ["conciergeMode", "conciergeModeSetBy", "conciergeModeReason"];

const RETIRED_VARIANTS: [&str; 6] = [
    "ManualFlagged",
    "ManualSafe",
    "ManualVouched",
    "ManualResumed",
    "ManualUncensored",
    "AutoFlaggedRefusals",
];

const RETIRED_WIRE: [&str; 6] = [
    "manual-flagged",
    "manual-safe",
    "manual-vouched",
    "manual-resumed",
    "manual-uncensored",
    "auto-flagged-refusals",
];

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

/// A literal that writes a column: an `UPDATE … SET` naming it, word-bounded.
fn updates(literal: &str, column: &str) -> bool {
    contains_word(literal, "UPDATE") && literal.contains(column)
}

#[test]
fn only_set_concierge_mode_updates_the_trio() {
    let mut writers: Vec<String> = Vec::new();
    for (file, zone) in production_files() {
        for lit in string_literals(&zone) {
            if TRIO.iter().any(|c| updates(lit, c)) {
                writers.push(format!(
                    "{file}: {}",
                    lit.split_whitespace().collect::<Vec<_>>().join(" ")
                ));
            }
        }
    }
    // `set_concierge_mode`'s ONE statement (its compare-and-set arms extend it
    // with `format!`, not with another literal).
    assert_eq!(
        writers.len(),
        1,
        "the Concierge state must have ONE writer (v4 `setConciergeMode`); found:\n{}",
        writers.join("\n")
    );
    assert!(
        writers[0].starts_with("quilltap-core/src/db/chats.rs:"),
        "the writer must be ChatsRepository::set_concierge_mode: {}",
        writers[0]
    );
}

#[test]
fn chat_update_names_none_of_the_trio() {
    let src = std::fs::read_to_string(core_src_root().join("db/chats.rs")).expect("chats.rs");
    let start = src
        .find("pub struct ChatUpdate {")
        .expect("ChatUpdate is declared in db/chats.rs");
    let body = &src[start..start + src[start..].find("\n}\n").expect("ChatUpdate closes")];
    for field in [
        "concierge_mode",
        "concierge_mode_set_by",
        "concierge_mode_reason",
    ] {
        assert!(
            !body.contains(&format!("pub {field}:")),
            "ChatUpdate carries `{field}` — v4's `patchOnlyFields` rule says only \
             set_concierge_mode may name the Concierge state"
        );
    }
    // The comparand must bite: the struct is the one the census thinks it is.
    assert!(
        body.contains("pub title: Option<String>"),
        "ChatUpdate shape moved"
    );
}

#[test]
fn nothing_updates_the_legacy_concierge_override() {
    let mut writers: Vec<String> = Vec::new();
    for (file, zone) in production_files() {
        for lit in string_literals(&zone) {
            if updates(lit, "conciergeOverride") {
                writers.push(format!("{file}: {lit}"));
            }
        }
    }
    assert!(
        writers.is_empty(),
        "`conciergeOverride` is no longer written (v4 `4d370a90f`): {writers:#?}"
    );
}

#[test]
fn no_production_code_emits_a_retired_manual_kind() {
    let mut emitters: Vec<String> = Vec::new();
    let mut writer_seen = false;
    for (file, zone) in production_files() {
        // The writer DECLARES and DECODES the retired kinds (old transcripts);
        // that is the one home allowed to name them.
        if file.ends_with("services/concierge_notifications.rs") {
            writer_seen = true;
            continue;
        }
        let code = code_only(&zone);
        for v in RETIRED_VARIANTS {
            if contains_word(&code, &format!("ConciergeManualKind::{v}")) {
                emitters.push(format!("{file}: ConciergeManualKind::{v}"));
            }
        }
        for lit in string_literals(&zone) {
            for w in RETIRED_WIRE {
                if lit.trim_matches('"') == w {
                    emitters.push(format!("{file}: \"{w}\""));
                }
            }
        }
    }
    assert!(
        writer_seen,
        "the census never reached the writer — the walk moved"
    );
    assert!(
        emitters.is_empty(),
        "a retired Concierge manual kind is emitted again (v4 `4d370a90f` retired six): {emitters:#?}"
    );
}
