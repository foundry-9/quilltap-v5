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
//! This test is that census's tripwire: it lists every call site of either
//! variant under `quilltap-core/src` — one row per call, `(file, the nearest
//! preceding fn, variant)`, in source order — and fails when any row moves. A
//! new caller must CHOOSE: find its v4 counterpart, decide fallback or strict,
//! and record the choice here (and in the file's comment if it is not
//! obvious). A row going away is also a failure, so a repointed or deleted
//! site cannot drift silently.
//!
//! **Why per site, not per file** (P4.112 — the `00c290c9a` unification
//! review's nit): the census counted `(fallback, strict)` per FILE, so a SWAP
//! of two sites' variants inside one file (`find_event_value` strict ↔
//! `update_chat_metadata` fallback, say) left every count unchanged and
//! passed. A per-site row carries its `fn` anchor, so a swap reddens
//! (`a_same_file_swap_is_visible`, and the mutation in P4.112's lane record).
//! The anchor is a name, not a line number, so an unrelated edit above a site
//! moves nothing.
//!
//! **Scanner.** A plain per-line scan, not the shared census lexer: P4.110's
//! `tests/source_census/mod.rs::rust_sources` does not sort (the rows here are
//! ORDERED, so the walk must be), and its `production_zone` strips the test
//! modules this census deliberately counts. Lines whose first non-blank characters are `//`
//! (line and doc comments) are skipped; the definitions (`fn get_messages(`,
//! `fn get_messages_strict(`) are declarations, not calls. Test modules are
//! NOT stripped — their calls are counted too (a test helper is a caller like
//! any other; see `help_chat/orchestrator.rs`). The pattern `get_messages(`
//! cannot match `get_messages_strict(` (the `(` must follow `get_messages`
//! directly).
//!
//! Run: `cargo test -p quilltap-harness --test get_messages_caller_census`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Which variant a call site takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// `get_messages` — v4's fallback `safeQuery` (swallows, answers `[]`).
    F,
    /// `get_messages_strict` — rethrows.
    S,
}
use Variant::{F, S};

/// Every call site, in SOURCE ORDER per file (files sorted by path):
/// `(path under quilltap-core/src, the nearest preceding `fn` name, variant)`.
/// The verdict per site is in P4.109's lane record; the notes here name the
/// non-obvious ones. The `fn` column is what the scanner sees — the last `fn`
/// declaration above the call, closures and nesting ignored — so it is an
/// anchor, not a claim about Rust scoping.
const CENSUS: &[(&str, &str, Variant)] = &[
    ("api/brahma.rs", "message_count", F),
    ("api/brahma.rs", "brahma_console_messages", F),
    ("api/characters.rs", "character_chats", F),
    ("api/chat_media.rs", "message_save_image", F),
    ("api/chat_media.rs", "chat_files_list", F),
    ("api/chat_transcript.rs", "chat_message_events", F),
    ("api/files.rs", "dissociate_file_from_all", F),
    ("api/files.rs", "compute_associations", F),
    ("api/help_chats.rs", "message_count", F),
    ("api/help_chats.rs", "help_chat_messages", F),
    ("api/llm_logs.rs", "non_empty", F),
    ("api/memories.rs", "memory_by_message", F),
    ("api/memories.rs", "chat_queue_memories", F),
    // `message_edit` re-reads the edited row: v4's counterpart is
    // `updateMessage`'s own `findOne` inside a FALLBACK `safeQuery` (`null` →
    // 404), so the swallowing variant gives v4's 404 where strict would give a
    // 500.
    ("api/salon.rs", "turn_action", F),
    ("api/salon.rs", "resolve_message", F),
    ("api/salon.rs", "message_edit", F),
    ("api/transcript_projection.rs", "project_chat_transcript", F),
    // `update_message` no longer reads through `get_messages` at all (P4.113:
    // v4's `findOne` is ONE raw row — `chats_messages_read::find_event_raw`
    // — so its old strict `find_event_value` site LEFT the census).
    // `delete_bookkeeping` and `update_chat_metadata` read through v4's
    // `getMessages` (fallback).
    ("db/chats_messages.rs", "delete_bookkeeping", F),
    ("db/chats_messages.rs", "update_chat_metadata", F),
    // `get_messages`' own body calls its strict sibling; `get_message_count`
    // wraps v4's `getMessages`; the rest are unit tests (the `00c290c9a`
    // unification's unknown-type test added one of each).
    ("db/chats_messages_read.rs", "get_messages", S),
    ("db/chats_messages_read.rs", "get_message_count", F),
    (
        "db/chats_messages_read.rs",
        "is_silent_reads_integer_cells_from_a_migrated_instance",
        F,
    ),
    (
        "db/chats_messages_read.rs",
        "is_silent_still_reads_fresh_ddl_text_cells",
        F,
    ),
    (
        "db/chats_messages_read.rs",
        "a_read_that_throws_answers_empty_and_logs_once",
        F,
    ),
    (
        "db/chats_messages_read.rs",
        "a_read_that_throws_answers_empty_and_logs_once",
        S,
    ),
    (
        "db/chats_messages_read.rs",
        "a_healthy_read_does_not_log_the_safe_query_error",
        F,
    ),
    (
        "db/chats_messages_read.rs",
        "a_null_content_row_is_skipped_with_a_warn_not_fatal",
        S,
    ),
    (
        "db/chats_messages_read.rs",
        "a_null_content_row_is_skipped_with_a_warn_not_fatal",
        F,
    ),
    (
        "db/chats_messages_read.rs",
        "an_unknown_type_row_is_skipped_with_the_same_warn",
        S,
    ),
    (
        "db/chats_messages_read.rs",
        "an_unknown_type_row_is_skipped_with_the_same_warn",
        F,
    ),
    // count / find / replace — v4 reads through `getMessages`, which is
    // where their swallow actually happens.
    ("db/chats_search.rs", "count_messages_with_text", F),
    ("db/chats_search.rs", "find_messages_with_text", F),
    ("db/chats_search.rs", "replace_in_messages", F),
    ("enclave/step.rs", "step", F),
    ("generators/rename.rs", "run_character_rename", F),
    ("photos/chat_gallery.rs", "list_chat_gallery", F),
    ("services/announcer/in_scene_voiced.rs", "generate_inner", F),
    // v4's backup runs OUTSIDE the strict-failures scope (its only callers
    // are the importer's execute and preview), so a chat whose read fails is
    // written with `messages: []` and the backup completes.
    ("services/backup/collect.rs", "collect_user_data", F),
    (
        "services/brahma_console/orchestrator.rs",
        "process_brahma_response",
        F,
    ),
    (
        "services/brahma_console/orchestrator/tests.rs",
        "loop_bound_forces_a_final_answer_at_the_operator_cap",
        F,
    ),
    (
        "services/brahma_console/orchestrator/tests.rs",
        "stale_guard_forces_a_final_when_results_repeat",
        F,
    ),
    ("services/build_context.rs", "sweep_stale_whispers", F),
    ("services/build_context.rs", "load", F),
    (
        "services/build_context.rs",
        "collect_fold_whisper_conversation_ids",
        F,
    ),
    (
        "services/carina_memory_extraction.rs",
        "handle_carina_memory_extraction",
        F,
    ),
    ("services/carina_query.rs", "load_prior_carina_exchanges", F),
    (
        "services/cascade_delete.rs",
        "find_exclusive_images_for_chats",
        F,
    ),
    ("services/chat_admin.rs", "chat_bulk_reattribute", F),
    ("services/chat_admin.rs", "chat_regenerate_title", F),
    (
        "services/chat_continuation.rs",
        "apply_chat_continuation",
        F,
    ),
    ("services/chat_export.rs", "chat_export", F),
    (
        "services/commonplace_notifications.rs",
        "sweep_prior_relevant_conversation_whispers",
        F,
    ),
    ("services/context_summary.rs", "generate_inner", F),
    (
        "services/context_summary.rs",
        "sweep_prior_summary_whispers",
        F,
    ),
    (
        "services/context_summary.rs",
        "check_and_generate_summary_if_needed_with_seams",
        F,
    ),
    ("services/conversation_render_job.rs", "handle_inner", F),
    (
        "services/conversation_summaries_regen.rs",
        "handle_regenerate_conversation_summaries",
        F,
    ),
    (
        "services/cost_estimation.rs",
        "get_detailed_chat_cost_breakdown",
        F,
    ),
    (
        "services/courier_transport.rs",
        "build_courier_delta_events",
        F,
    ),
    ("services/courier_transport.rs", "resolve_external_turn", F),
    ("services/courier_transport.rs", "cancel_external_turn", F),
    (
        "services/dangerous_content/gatekeeper_job.rs",
        "handle_chat_danger_classification",
        F,
    ),
    // `message_count` is a test helper with no v4 counterpart → strict (a
    // broken fixture fails loudly); the production read is fallback.
    (
        "services/help_chat/orchestrator.rs",
        "process_help_response",
        F,
    ),
    ("services/help_chat/orchestrator.rs", "message_count", S),
    ("services/markdown_transcript.rs", "chat_export_markdown", F),
    (
        "services/memory_extraction_job.rs",
        "handle_memory_extraction",
        F,
    ),
    (
        "services/message_finalizer.rs",
        "finalize_message_response",
        F,
    ),
    (
        "services/message_finalizer.rs",
        "trigger_turn_memory_extraction",
        F,
    ),
    ("services/message_finalizer.rs", "calculate_next_speaker", F),
    ("services/message_reattribute.rs", "message_reattribute", F),
    ("services/message_reattribute.rs", "message_reattribute", F),
    ("services/off_scene.rs", "scan_off_scene_newcomers_inner", F),
    ("services/orchestrator.rs", "process_message", F),
    (
        "services/participant_resolver.rs",
        "resolve_responding_participant",
        F,
    ),
    ("services/qtap_export/records.rs", "stream_chats", F),
    // STRICT, both importer sites: v4 runs them inside
    // `withStrictRepositoryFailures` (`execute.ts:430`), where `safeQuery`
    // rethrows even in fallback mode; `mod.rs`'s `match` arm is v4's "Failed
    // to read chat while importing informs" warn, which the swallowing variant
    // would make unreachable. P4.109 recorded both (P4.110 owned the files);
    // repointed at the `00c290c9a` unification, pinned by `quilltap_import`'s
    // `a_failed_informs_message_read_warns_under_the_strict_scope`.
    ("services/quilltap_import/files.rs", "remap_linked_to", S),
    ("services/quilltap_import/mod.rs", "import_body", S),
    ("services/recall_replay.rs", "run_recall_replay", F),
    (
        "services/story_background_job.rs",
        "handle_story_background_generation",
        F,
    ),
    ("services/title_update_job.rs", "handle_title_update", F),
    ("services/turn_orchestrator.rs", "should_chain_next", F),
    ("services/turn_orchestrator.rs", "handle_turn_action", F),
    ("tools/generate_image.rs", "gather_db_context", F),
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

/// Every call in one source text, in order: `(nearest preceding fn, variant)`.
///
/// Per line (comment lines skipped): the `fn <name>` declarations and the
/// calls, merged by column, so a one-line `fn f() { get_messages(…) }` credits
/// `f`. A definition (`fn get_messages(`, `fn get_messages_strict(`) is a
/// declaration, never a call.
fn scan_calls(text: &str) -> Vec<(String, Variant)> {
    fn is_ident(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }
    let mut current = String::from("<none>");
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let b = line.as_bytes();
        // (column, Some(fn name) | None for a call, variant)
        let mut events: Vec<(usize, Option<String>, Variant)> = Vec::new();
        let mut i = 0;
        while let Some(off) = line[i..].find("fn ") {
            let at = i + off;
            i = at + 3;
            if at > 0 && is_ident(b[at - 1]) {
                continue;
            }
            let mut j = at + 3;
            while j < b.len() && b[j] == b' ' {
                j += 1;
            }
            let name_start = j;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            if j > name_start {
                events.push((name_start, Some(line[name_start..j].to_string()), F));
            }
        }
        for (pat, variant) in [("get_messages(", F), ("get_messages_strict(", S)] {
            for (at, _) in line.match_indices(pat) {
                let is_definition = line[..at].ends_with("fn ");
                if !is_definition {
                    events.push((at, None, variant));
                }
            }
        }
        events.sort_by_key(|e| e.0);
        for (_, name, variant) in events {
            match name {
                Some(n) => current = n,
                None => out.push((current.clone(), variant)),
            }
        }
    }
    out
}

/// A census row as Rust source, for the drift message (paste-ready).
fn row_source(path: &str, fn_name: &str, v: Variant) -> String {
    format!("    (\"{path}\", \"{fn_name}\", {v:?}),")
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

    // Calls in source order within each file; files ordered by their path
    // STRING below (the walker sorts per directory, so `orchestrator/` would
    // precede `orchestrator.rs`) — a STABLE sort, so each file keeps its
    // source order.
    let mut found: Vec<(String, String, Variant)> = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).expect("read source");
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (fn_name, v) in scan_calls(&text) {
            found.push((rel.clone(), fn_name, v));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let mut want: Vec<(String, String, Variant)> = CENSUS
        .iter()
        .map(|(p, f, v)| (p.to_string(), f.to_string(), *v))
        .collect();
    want.sort_by(|a, b| a.0.cmp(&b.0));

    if found != want {
        // Name every file whose ordered rows moved, then print the whole
        // found table — the census must be re-DECIDED site by site, never
        // pasted blind.
        let files_of = |rows: &[(String, String, Variant)]| {
            let mut m: BTreeMap<String, Vec<(String, Variant)>> = BTreeMap::new();
            for (p, f, v) in rows {
                m.entry(p.clone()).or_default().push((f.clone(), *v));
            }
            m
        };
        let (got_by, want_by) = (files_of(&found), files_of(&want));
        let mut drift = Vec::new();
        for path in got_by
            .keys()
            .chain(want_by.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            let (g, w) = (got_by.get(path), want_by.get(path));
            if g != w {
                drift.push(format!("{path}: census {w:?}\n      source {g:?}"));
            }
        }
        let table: Vec<String> = found.iter().map(|(p, f, v)| row_source(p, f, *v)).collect();
        panic!(
            "get_messages caller census drift — find each moved site's v4 counterpart, decide \
             fallback F (it reads through v4's getMessages) or strict S (it rethrows or reads \
             another way), and update CENSUS:\n  {}\n\nThe source as scanned now:\n{}",
            drift.join("\n  "),
            table.join("\n")
        );
    }

    let swallowing = found.iter().filter(|r| r.2 == F).count();
    let strict = found.iter().filter(|r| r.2 == S).count();
    let file_count = found
        .iter()
        .map(|r| r.0.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // 50 files at P4.109. The order's 76 sites, all `get_messages`, became:
    // swallowing 76 − 2 repointed (`find_event_value`, the help-chat test
    // helper) + 3 new unit-test calls = 77; strict 2 repointed + 1
    // (`get_messages`' own call of its sibling) + 2 new unit-test calls = 5.
    // The `00c290c9a` unification moved the two importer sites P4.109
    // recorded: swallowing 77 − 2 = 75; strict 5 + 2 = 7; and its
    // unknown-type unit test calls each variant once: 76 and 8. P4.112
    // re-keyed the rows per call site (file, fn, variant) — the SAME 84 sites
    // over the same 50 files, so the totals do not move. P4.113 retired
    // `update_message`'s strict `find_event_value` site (v4's raw `findOne`
    // is not a `getMessages` read): strict 8 − 1 = 7; the file keeps its two
    // fallback sites, so still 50 files.
    assert_eq!((swallowing, strict), (76, 7), "census totals");
    assert_eq!(file_count, 50, "census files");
}

/// The scanner itself: comments skipped, definitions are declarations not
/// calls, the strict name never counted as the swallowing one, and each call
/// credited to the nearest preceding `fn` (same-line declarations included).
#[test]
fn the_scanner_counts_what_it_claims() {
    let text = "\
        /// calls [`get_messages`] like get_messages(conn, id)\n\
        // get_messages(x)\n\
        pub fn get_messages(conn: &Connection) {\n\
            get_messages_strict(conn, id)\n\
        }\n\
        pub fn get_messages_strict(conn: &Connection) {}\n\
        fn caller() { let a = get_messages(c, &id)?; let b = x::get_messages(c, &id); }\n\
        fn other_fn() {}\n\
        let c = get_messages_strict(c, &id);\n";
    let got = scan_calls(text);
    let want: Vec<(String, Variant)> = vec![
        ("get_messages".into(), S),
        ("caller".into(), F),
        ("caller".into(), F),
        ("other_fn".into(), S),
    ];
    assert_eq!(got, want);
}

/// P4.112's reason for the reshape: a SWAP of two sites' variants inside one
/// file leaves the old per-file `(fallback, strict)` counts unchanged, so it
/// passed. Per-site rows see it.
#[test]
fn a_same_file_swap_is_visible() {
    let before = "fn a() { get_messages(c, x) }\nfn b() { get_messages_strict(c, x) }\n";
    let swapped = "fn a() { get_messages_strict(c, x) }\nfn b() { get_messages(c, x) }\n";
    let counts = |rows: &[(String, Variant)]| {
        (
            rows.iter().filter(|r| r.1 == F).count(),
            rows.iter().filter(|r| r.1 == S).count(),
        )
    };
    let (x, y) = (scan_calls(before), scan_calls(swapped));
    assert_eq!(
        counts(&x),
        counts(&y),
        "the old per-file counts cannot tell"
    );
    assert_ne!(x, y, "the per-site rows must");
}
