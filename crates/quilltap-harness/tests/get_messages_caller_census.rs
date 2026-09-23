//! P4.109 — the `get_messages` caller census, as a guard.
//!
//! v4's `getMessages` (`chats-messages.ops.ts:315-366`) is a FALLBACK
//! `safeQuery`: a read that throws logs `Failed to get messages for chat` and
//! answers `[]`. Since P4.109, v5's `chats_messages_read::get_messages` does the
//! same, and `get_messages_strict` keeps the old rethrowing body for the call
//! sites whose v4 counterpart does NOT read through `getMessages` (it rethrows,
//! or reads the rows some other way). Which variant a site takes was decided
//! SITE BY SITE against v4 — the 76-site census in P4.109's lane record
//! (`status-log.md`) — never by blanket rule.
//!
//! This test is that census's tripwire: it counts, per file under
//! `quilltap-core/src`, the call sites of each variant, and fails when a count
//! moves. A new caller must CHOOSE: find its v4 counterpart, decide fallback or
//! strict, and record the choice here (and in the file's comment if it is not
//! obvious). A count going DOWN is also a failure, so a repointed or deleted
//! site cannot drift silently.
//!
//! **Scanner.** A plain per-line scan, not the shared census lexer: P4.110's
//! `tests/source_census/mod.rs` is not on this lane's base. Lines whose first
//! non-blank characters are `//` (line and doc comments) are skipped; the
//! definitions (`fn get_messages(`, `fn get_messages_strict(`) are subtracted.
//! Test modules are NOT stripped — their calls are counted too (a test helper
//! is a caller like any other; see `help_chat/orchestrator.rs`). The pattern
//! `get_messages(` cannot match `get_messages_strict(` (the `(` must follow
//! `get_messages` directly).
//!
//! Run: `cargo test -p quilltap-harness --test get_messages_caller_census`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// `(path under quilltap-core/src, swallowing get_messages calls,
/// get_messages_strict calls)`. The verdict per site is in P4.109's lane
/// record; the notes here name the non-obvious ones.
const CENSUS: &[(&str, usize, usize)] = &[
    ("api/brahma.rs", 2, 0),
    ("api/characters.rs", 1, 0),
    ("api/chat_media.rs", 2, 0),
    ("api/chat_transcript.rs", 1, 0),
    ("api/files.rs", 2, 0),
    ("api/help_chats.rs", 2, 0),
    ("api/llm_logs.rs", 1, 0),
    ("api/memories.rs", 2, 0),
    // `:1090` re-reads the edited row: v4's counterpart is `updateMessage`'s
    // own `findOne` inside a FALLBACK `safeQuery` (`null` → 404), so the
    // swallowing variant gives v4's 404 where strict would give a 500.
    ("api/salon.rs", 3, 0),
    ("api/transcript_projection.rs", 1, 0),
    // `update_message`'s `find_event_value` is STRICT: v4's `findOne` sits
    // inside `updateMessage`'s own `safeQuery`, so a failed read must log
    // `Failed to update message in chat`. `delete_bookkeeping` and
    // `update_chat_metadata` read through v4's `getMessages` (fallback).
    ("db/chats_messages.rs", 2, 1),
    // `get_message_count` (v4 wraps `getMessages`) + five unit-test calls;
    // strict: `get_messages`' own body + two unit-test calls.
    ("db/chats_messages_read.rs", 6, 3),
    // count / find / replace — v4 reads through `getMessages`, which is
    // where their swallow actually happens.
    ("db/chats_search.rs", 3, 0),
    ("enclave/step.rs", 1, 0),
    ("generators/rename.rs", 1, 0),
    ("photos/chat_gallery.rs", 1, 0),
    ("services/announcer/in_scene_voiced.rs", 1, 0),
    // v4's backup runs OUTSIDE the strict-failures scope (its only callers
    // are the importer's execute and preview), so a chat whose read fails is
    // written with `messages: []` and the backup completes.
    ("services/backup/collect.rs", 1, 0),
    ("services/brahma_console/orchestrator.rs", 1, 0),
    ("services/brahma_console/orchestrator/tests.rs", 2, 0),
    ("services/build_context.rs", 3, 0),
    ("services/carina_memory_extraction.rs", 1, 0),
    ("services/carina_query.rs", 1, 0),
    ("services/cascade_delete.rs", 1, 0),
    ("services/chat_admin.rs", 2, 0),
    ("services/chat_continuation.rs", 1, 0),
    ("services/chat_export.rs", 1, 0),
    ("services/commonplace_notifications.rs", 1, 0),
    ("services/context_summary.rs", 3, 0),
    ("services/conversation_render_job.rs", 1, 0),
    ("services/conversation_summaries_regen.rs", 1, 0),
    ("services/cost_estimation.rs", 1, 0),
    ("services/courier_transport.rs", 3, 0),
    ("services/dangerous_content/gatekeeper_job.rs", 1, 0),
    // A test helper with no v4 counterpart → strict (a broken fixture fails
    // loudly); the production read is fallback.
    ("services/help_chat/orchestrator.rs", 1, 1),
    ("services/markdown_transcript.rs", 1, 0),
    ("services/memory_extraction_job.rs", 1, 0),
    ("services/message_finalizer.rs", 3, 0),
    ("services/message_reattribute.rs", 2, 0),
    ("services/off_scene.rs", 1, 0),
    ("services/orchestrator.rs", 1, 0),
    ("services/participant_resolver.rs", 1, 0),
    ("services/qtap_export/records.rs", 1, 0),
    // STRICT, both: v4 runs them inside `withStrictRepositoryFailures`
    // (`execute.ts:430`), where `safeQuery` rethrows even in fallback mode;
    // `mod.rs`'s `match` arm is v4's "Failed to read chat while importing
    // informs" warn, which the swallowing variant would make unreachable.
    // P4.109 recorded both (P4.110 owned the files); repointed at the
    // `00c290c9a` unification, pinned by `quilltap_import`'s
    // `a_failed_informs_message_read_warns_under_the_strict_scope`.
    ("services/quilltap_import/files.rs", 0, 1),
    ("services/quilltap_import/mod.rs", 0, 1),
    ("services/recall_replay.rs", 1, 0),
    ("services/story_background_job.rs", 1, 0),
    ("services/title_update_job.rs", 1, 0),
    ("services/turn_orchestrator.rs", 2, 0),
    ("tools/generate_image.rs", 1, 0),
];

fn core_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rust_sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// `(swallowing, strict)` call counts in one source text.
fn count_calls(text: &str) -> (usize, usize) {
    let mut swallowing = 0;
    let mut strict = 0;
    for line in text.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        swallowing += line.matches("get_messages(").count();
        swallowing -= line.matches("fn get_messages(").count();
        strict += line.matches("get_messages_strict(").count();
        strict -= line.matches("fn get_messages_strict(").count();
    }
    (swallowing, strict)
}

#[test]
fn every_get_messages_call_site_has_chosen_its_variant() {
    let root = core_src();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    assert!(
        files.len() > 100,
        "the scan found {} files — wrong root?",
        files.len()
    );

    let mut found: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for f in &files {
        let text = std::fs::read_to_string(f).expect("read source");
        let counts = count_calls(&text);
        if counts != (0, 0) {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            found.insert(rel, counts);
        }
    }
    let want: BTreeMap<String, (usize, usize)> = CENSUS
        .iter()
        .map(|(p, a, b)| (p.to_string(), (*a, *b)))
        .collect();

    let mut drift = Vec::new();
    for (path, got) in &found {
        match want.get(path) {
            None => drift.push(format!("NEW caller file {path}: {got:?}")),
            Some(w) if w != got => drift.push(format!("{path}: census {w:?}, source {got:?}")),
            Some(_) => {}
        }
    }
    for path in want.keys().filter(|p| !found.contains_key(*p)) {
        drift.push(format!("{path}: in the census, no call left in the source"));
    }
    assert!(
        drift.is_empty(),
        "get_messages caller census drift — find each moved site's v4 counterpart, decide \
         fallback (it reads through v4's getMessages) or strict (it rethrows or reads another \
         way), and update CENSUS (counts are (get_messages, get_messages_strict)):\n  {}",
        drift.join("\n  ")
    );

    let (swallowing, strict): (usize, usize) =
        found.values().fold((0, 0), |(a, b), (x, y)| (a + x, b + y));
    // 50 files at P4.109. The order's 76 sites, all `get_messages`, became:
    // swallowing 76 − 2 repointed (`find_event_value`, the help-chat test
    // helper) + 3 new unit-test calls = 77; strict 2 repointed + 1
    // (`get_messages`' own call of its sibling) + 2 new unit-test calls = 5.
    // The `00c290c9a` unification moved the two importer sites P4.109
    // recorded: swallowing 77 − 2 = 75; strict 5 + 2 = 7.
    assert_eq!((swallowing, strict), (75, 7), "census totals");
}

/// The scanner itself: comments skipped, definitions subtracted, and the
/// strict name never counted as the swallowing one.
#[test]
fn the_scanner_counts_what_it_claims() {
    let text = "\
        /// calls [`get_messages`] like get_messages(conn, id)\n\
        // get_messages(x)\n\
        pub fn get_messages(conn: &Connection) {\n\
            get_messages_strict(conn, id)\n\
        }\n\
        pub fn get_messages_strict(conn: &Connection) {}\n\
        let a = get_messages(c, &id)?; let b = x::get_messages(c, &id);\n";
    assert_eq!(count_calls(text), (2, 1));
}
