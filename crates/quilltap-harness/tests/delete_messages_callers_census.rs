//! P4.163 (Tier 1 item 4) — every production caller of
//! `ChatMessagesRepository::delete_messages_by_ids` answers through
//! `db::fallback::messages_deleted_or_zero`, v4's fallback shape.
//!
//! v4's `deleteMessagesByIds` (`chats-messages.ops.ts:633-686`) is a
//! STANDALONE 4-argument `safeQuery(…, 'Failed to delete messages from chat',
//! { chatId, count }, 0)`: outside the strict scope it never throws, so every
//! one of its six callers receives `0` on a database failure and carries on —
//! measured at `94fbb1ae3` caller by caller (the lane record's table):
//!
//! | v5 site | v4 site | v4 after a failed delete |
//! |---|---|---|
//! | `api/chat_informs.rs` (cancel) | `chats/[id]/actions/inform.ts:228` | the line, `record_deleted = false` (P4.156) |
//! | `api/salon.rs` (message DELETE) | `messages/[id]/route.ts:196` | the line, touch + invalidate, 200 |
//! | `services/commonplace_notifications.rs` | `relevant-conversations-refresh.ts:167` | the line; its WARN unreachable |
//! | `services/context_summary.rs` | `context-summary.ts:490` | the line, INFO `removed: 0`, the fresh whisper posts |
//! | `services/courier_transport.rs` (cancel) | `chats/[id]/messages/[messageId]/route.ts:271` | the line, unpause, `{cancelled: true}` |
//! | `services/build_context.rs` (whisper sweeps) | `context-manager.ts:2276, 2566` | the line; both sweep catches unreachable |
//!
//! So R-A's "convert only where v4 falls back" converts all six. Before P4.163
//! five bypassed the home: the Salon DELETE answered 500, the courier cancel
//! failed whole, and three swallowed the failure with no line at all.
//!
//! Two arms: (1) a SOURCE census — each production call sits beside a
//! `messages_deleted_or_zero` in the same file window, and the site set is
//! pinned by file (a new caller must be classified); (2) a BEHAVIOURAL arm for
//! the courier cancel (the Salon DELETE's is `salon_reads`'
//! `delete_message_main_plant`, against v4's real route): over a planted BEFORE
//! DELETE trigger the cancel completes, unpauses the chat, and logs v4's line.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test delete_messages_callers_census

mod source_census;

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::courier_transport::{cancel_external_turn, CancelExternalTurnOutcome};
use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// Every file with a production `delete_messages_by_ids(` call, and how many.
const SITES: &[(&str, usize)] = &[
    ("api/chat_informs.rs", 1),
    ("api/salon.rs", 1),
    ("services/build_context.rs", 1),
    ("services/commonplace_notifications.rs", 1),
    ("services/context_summary.rs", 1),
    ("services/courier_transport.rs", 1),
];

/// How far (in lines, either side) the home's call may sit from the delete: the
/// async shape awaits the write and hands its result to the home a few lines
/// later; the in-closure shape wraps the call directly.
const WINDOW: usize = 12;

#[test]
fn every_delete_messages_caller_answers_through_the_fallback_home() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut bypassers: Vec<String> = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "db/chats_messages.rs" {
            continue; // the definition
        }
        let src = std::fs::read_to_string(f).unwrap();
        let code = code_only(&production_zone(&src));
        let lines: Vec<&str> = code.lines().collect();
        let mut n = 0usize;
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("delete_messages_by_ids(") {
                continue;
            }
            n += 1;
            let lo = i.saturating_sub(WINDOW);
            let hi = (i + WINDOW + 1).min(lines.len());
            if !lines[lo..hi]
                .iter()
                .any(|l| l.contains("messages_deleted_or_zero("))
            {
                bypassers.push(format!("{rel}:{}", i + 1));
            }
        }
        if n > 0 {
            seen.push((rel, n));
        }
    }
    let want: Vec<(String, usize)> = SITES.iter().map(|(f, n)| (f.to_string(), *n)).collect();
    assert_eq!(
        seen, want,
        "the production `delete_messages_by_ids` callers moved — measure the new site's v4 \
         twin (R-A) and classify it here"
    );
    assert!(
        bypassers.is_empty(),
        "callers bypassing `db::fallback::messages_deleted_or_zero` (v4's `deleteMessagesByIds` \
         is a fallback — line + 0, never a throw):\n  {}",
        bypassers.join("\n  ")
    );
}

const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";
const SOLO: &str = "c1000000-0000-4000-8000-000000000001";
const MESSAGE: &str = "d1000000-0000-4000-8000-000000000002";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[test]
fn the_courier_cancel_completes_through_a_failed_delete() {
    let spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/salon.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec["testPepperBase64"]
        .as_str()
        .unwrap_or(PEPPER)
        .to_string();
    let scratch = tempfile::Builder::new()
        .prefix("qt-delete-callers-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("salon-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("salon-mount.db"), &mount).unwrap();
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &pepper).unwrap();
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
        quilltap_core::test_support::ensure_p4d182_columns(w.connection());
        // A courier placeholder awaiting its external reply, on a paused chat.
        w.connection()
            .execute_batch(&format!(
                "UPDATE chat_messages SET pendingExternalPrompt = 'paste me' WHERE id = '{MESSAGE}';
                 UPDATE chats SET isPaused = 1 WHERE id = '{SOLO}';
                 CREATE TRIGGER qt_plant_no_message_delete BEFORE DELETE ON chat_messages
                 BEGIN SELECT RAISE(ABORT, 'planted message delete failure'); END;"
            ))
            .unwrap();
    }
    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: Some(mount),
            llm_logs: None,
        },
        &pepper,
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (outcome, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(cancel_external_turn(&db, SOLO, MESSAGE, 1_790_000_000_000))
    });
    let outcome = outcome.expect("v4's cancel never fails on a failed delete");
    assert!(
        matches!(outcome, CancelExternalTurnOutcome::Cancelled { ref message_id } if message_id == MESSAGE),
        "{outcome:?}"
    );
    let errors: Vec<&String> = lines.iter().filter(|l| l.starts_with("ERROR ")).collect();
    assert_eq!(
        errors,
        [&format!(
            "ERROR quilltap::db Failed to delete messages from chat chatId={SOLO} count=1 \
             error=planted message delete failure"
        )],
        "{lines:?}"
    );
    let paused: Option<bool> = db
        .read_main(|c| {
            c.query_row("SELECT isPaused FROM chats WHERE id = ?1", [SOLO], |r| {
                r.get(0)
            })
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(paused, Some(false), "the cancel still unpauses the chat");
}
