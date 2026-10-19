//! The chat wardrobe history's WRITE fence (P4.D315 Tier 2 item 21 — the v5
//! twin of v4's `__tests__/unit/lib/wardrobe/outfit-history-fence.test.ts`
//! `:42`, `:50`, `:62` at the `e8e17b2d9` pin).
//!
//! v4 holds three source-grep fences over the outfit history:
//!
//! - **`:42` — no code outside the DDL module creates `chat_wardrobe_history`.**
//!   v5: the DDL text (`CREATE TABLE IF NOT EXISTS "chat_wardrobe_history"`,
//!   raw or escaped) appears in exactly ONE production file,
//!   `db/chat_wardrobe_history.rs` (the boot ensure and the test-support plant
//!   run its constant).
//! - **`:50` — no code outside the repository, the seed migration and the DDL
//!   module inserts history rows by hand.** v5: the INSERT text
//!   (`INSERT INTO "chat_wardrobe_history"`) appears only in
//!   `db/chat_wardrobe_history.rs`, and the files that spell the constant
//!   `CHAT_WARDROBE_HISTORY_INSERT_SQL` in CODE are exactly the repository and
//!   the seed ensure (`db/chat_wardrobe_history_repair.rs`).
//! - **`:62` — `wardrobeHistory.upsertRows` is called only from restore and
//!   import.** v5: every production file outside the repository that calls
//!   `upsert_raw_rows(` — or names `ChatWardrobeHistoryRepository` and calls
//!   `.upsert_rows(` — lives under `services/backup/restore/` or
//!   `services/quilltap_import/` (path PREFIXES, so P4.D317's future sites
//!   fall inside without an edit). Tests and `test_support` are exempt (the
//!   shared `source_census` lexer strips `#[cfg(test)]` items).
//!
//! v4's `:70` fence (no turn creates the announcement set by hand) is
//! P4.D316's; `:78` (`settle` is a buffered write in the job child) is NO-PORT
//! (v5 has no job child).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test chat_wardrobe_history_writer_census_guard

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// The DDL module — the only home of the CREATE and INSERT text.
const DDL_MODULE: &str = "db/chat_wardrobe_history.rs";
/// The seed ensure — the one other user of the shared INSERT.
const SEED_MODULE: &str = "db/chat_wardrobe_history_repair.rs";
/// Where `upsert_rows` / `upsert_raw_rows` may be called from.
const UPSERT_CALLER_PREFIXES: [&str; 2] = ["services/backup/restore/", "services/quilltap_import/"];

/// `(path under crates/quilltap-core/src, production zone, code-only zone)`.
fn sources() -> Vec<(String, String, String)> {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let src = std::fs::read_to_string(&path).unwrap();
            let zone = production_zone(&src);
            let code = code_only(&zone);
            (rel, zone, code)
        })
        .collect()
}

/// Files whose production zone (strings included) spells `text`, raw or as an
/// escaped Rust string.
fn spelling(sources: &[(String, String, String)], text: &str) -> Vec<String> {
    let escaped = text.replace('"', "\\\"");
    sources
        .iter()
        .filter(|(_, zone, _)| zone.contains(text) || zone.contains(&escaped))
        .map(|(rel, _, _)| rel.clone())
        .collect()
}

/// Is this code an upsert caller of the history repository?
fn calls_history_upsert(code: &str) -> bool {
    code.contains("upsert_raw_rows(")
        || (code.contains("ChatWardrobeHistoryRepository") && code.contains(".upsert_rows("))
}

#[test]
fn only_the_ddl_module_creates_the_table() {
    let s = sources();
    assert_eq!(
        spelling(&s, "CREATE TABLE IF NOT EXISTS \"chat_wardrobe_history\""),
        vec![DDL_MODULE.to_string()],
        "the history table's DDL is spelled outside its module — run \
         `CHAT_WARDROBE_HISTORY_DDL` (v4 `outfit-history-fence.test.ts:42`)"
    );
}

#[test]
fn only_the_repository_and_the_seed_insert_rows() {
    let s = sources();
    assert_eq!(
        spelling(&s, "INSERT INTO \"chat_wardrobe_history\""),
        vec![DDL_MODULE.to_string()],
        "a hand-written history INSERT outside the repository (v4 \
         `outfit-history-fence.test.ts:50`)"
    );
    let users: Vec<String> = s
        .iter()
        .filter(|(_, _, code)| code.contains("CHAT_WARDROBE_HISTORY_INSERT_SQL"))
        .map(|(rel, _, _)| rel.clone())
        .collect();
    assert_eq!(
        users,
        vec![DDL_MODULE.to_string(), SEED_MODULE.to_string()],
        "the shared INSERT is used outside the repository and the seed ensure"
    );
}

#[test]
fn upserts_come_only_from_restore_and_import() {
    let offenders: Vec<String> = sources()
        .into_iter()
        .filter(|(rel, _, code)| {
            rel != DDL_MODULE
                && calls_history_upsert(code)
                && !UPSERT_CALLER_PREFIXES.iter().any(|p| rel.starts_with(p))
        })
        .map(|(rel, _, _)| rel)
        .collect();
    assert!(
        offenders.is_empty(),
        "history upserts outside restore / import (v4 `outfit-history-fence.test.ts:62` — \
         `settle` is the one runtime writer): {offenders:?}"
    );
}

/// The detectors see what they should: code calls count, comments / strings /
/// `#[cfg(test)]` items do not; another repository's `upsert_rows` is not ours.
#[test]
fn the_detectors_see_only_code() {
    let ours = r#"
        fn site(conn: &C) { ChatWardrobeHistoryRepository::new(conn).upsert_rows(&rows); }
    "#;
    let raw = "fn site(r: &R) { r.upsert_raw_rows(&rows); }";
    let theirs = "fn site(r: &WardrobeWearStatsRepository) { r.upsert_rows(&rows); }";
    let hidden = r#"
        // ChatWardrobeHistoryRepository::new(c).upsert_rows(&rows) in a comment
        const S: &str = "upsert_raw_rows(";
        #[cfg(test)]
        mod tests { fn t() { ChatWardrobeHistoryRepository::new(c).upsert_rows(&r); } }
    "#;
    let code = |src: &str| code_only(&production_zone(src));
    assert!(calls_history_upsert(&code(ours)));
    assert!(calls_history_upsert(&code(raw)));
    assert!(!calls_history_upsert(&code(theirs)));
    assert!(!calls_history_upsert(&code(hidden)));
}
