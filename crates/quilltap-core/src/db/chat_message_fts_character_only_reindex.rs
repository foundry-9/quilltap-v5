//! The character-only re-index of message search (v4 migration
//! `reindex-chat-message-fts-character-only-v1`, `902c6efe9` — v4 #121;
//! P4.D363).
//!
//! `create-chat-message-fts-v1` indexed every `type='message'` row with role
//! `USER` or `ASSISTANT`. Staff messages — Commonplace Book recall whispers,
//! Host, Prospero and Librarian announcements — are written with role
//! `ASSISTANT`, so they were indexed too, and global search returned them as
//! though a character had said them. Announcement bubbles (`customAnnouncer`)
//! had the same problem.
//!
//! The eligibility rule in [`super::chat_message_fts`] now excludes both. That
//! module's [`ensure_chat_message_fts_schema`] replaces the insert trigger when
//! its stored definition no longer matches, and this pass then rebuilds the
//! index so the rows already in it are dropped. The startup reconcile
//! ([`super::chat_message_fts_reconcile`]) would reach the same end state on
//! its own; running this first puts the rebuild behind the loading screen (in
//! v4's words) — and, in v5, keeps the reconcile's "out of step" WARN from
//! firing on a pre-round instance's first post-round boot (round ruling R-6).
//!
//! ## v4's RUNNER semantics, reproduced
//!
//! v4's runner consults its ledger BEFORE `shouldRun`; a `shouldRun` of
//! `false` is skipped with NO ledger row; a run is recorded with
//! `recordCompletedMigration`. So:
//!
//! - a ledger row from EITHER app → [`ReindexOutcome::AlreadyCompleted`];
//! - no `chat_messages` table → [`ReindexOutcome::NotApplicable`] (v4's
//!   `sqliteTableExists` arm);
//! - nothing missing, nothing stale, counts equal →
//!   [`ReindexOutcome::NotNeeded`] — **writes nothing**, not even the ledger
//!   tables. A stamp on a skip would be a row v4's runner never writes;
//! - otherwise → [`ReindexOutcome::Ran`], v4's two INFO lines, and v4's row.
//!
//! v4's migration is not `resumable`, so a throw stops v4's boot ("Migrations
//! failed - cannot start server"); the host calls this with `?` for the same
//! reason (ruling L-1).
//!
//! ## `DEPENDENCY_PENDING` — the one arm v4's script does not spell
//!
//! v4's `run()` counts the map (`countIndexedChatMessages`) BEFORE it ensures
//! the schema, so on an instance with no `chat_messages_fts_map` it throws
//! `no such table: chat_messages_fts_map`. v4 never reaches that: the
//! migration `dependsOn` `create-chat-message-fts-v1`, which builds the index
//! (under the current rule) first, after which this one's `shouldRun` is
//! false and it is skipped unstamped. v5 has no twin of that migration ahead
//! of this pass — the PHASE 3.65 reconcile below does its work — so a fresh
//! v5 instance arrives here with no map at all. [`ReindexOutcome::
//! DependencyPending`] is that dependency gate: no map table → nothing runs,
//! nothing is stamped, nothing is logged, and the reconcile creates and
//! indexes under the current rule — the same end state as v4's runner. The
//! literal `should_run` / `run` pair is kept and differentially compared
//! (including v4's throw on that base); only the entry point gates.
//!
//! ## Stamp fidelity (`REINDEX_STAMP_IS_FIDELITY`)
//!
//! After a run, v4's `shouldRun` is false on the same instance whether or not
//! the row exists, so the stamp is not what keeps v4 from re-running: it is
//! there because v4's runner writes it after a run, byte for byte.
//!
//! ## Not ported
//!
//! `reportProgress` (runner UI), the `prettify.ts` label ("Striking the
//! Staff's private asides from the catalogue of things said…"), and the
//! runner metadata — `description: 'Rebuild the chat-message search index
//! without Staff messages or announcement bubbles'`, `introducedInVersion:
//! '4.12.0'`, `dependsOn: ['create-chat-message-fts-v1',
//! 'add-system-sender-field-v1', 'add-custom-announcer-field-v1']`. The two
//! column dependencies are inert for v5: `fresh_schema.json` carries both
//! columns and every v5 message read names them, so v5 cannot open an
//! instance that lacks them.

use rusqlite::Connection;

use super::chat_message_fts::{
    count_eligible_chat_messages, count_indexed_chat_messages, ensure_chat_message_fts_schema,
    missing_chat_message_fts_objects, rebuild_chat_message_fts_index,
    stale_chat_message_fts_triggers, CHAT_MESSAGE_FTS_MAP_TABLE,
};
use super::migrations_ledger::{ledger_has, stamp_completed};
use super::DbError;

/// v4's migration id — the ledger key both apps honour.
pub const MIGRATION_ID: &str = "reindex-chat-message-fts-character-only-v1";

/// v4's `context` on both INFO lines, VERBATIM: `migrations.` (plural) and the
/// id WITHOUT `-v1` — not the `migration.<id>` shape other v4 migrations use.
const LOG_CONTEXT: &str = "migrations.reindex-chat-message-fts-character-only";

/// What [`reindex_chat_message_fts_character_only`] found and did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReindexOutcome {
    /// The ledger already carries the row (either app ran the pass).
    AlreadyCompleted,
    /// No `chat_messages` table (v4's `sqliteTableExists` arm).
    NotApplicable,
    /// No `chat_messages_fts_map` — `create-chat-message-fts-v1`'s work is
    /// undone (see the module doc). Nothing ran, nothing stamped.
    DependencyPending,
    /// v4's `shouldRun()` was false. Nothing written.
    NotNeeded,
    /// The pass ran and stamped v4's row.
    Ran {
        replaced_triggers: Vec<String>,
        indexed_before: i64,
        scanned: i64,
        indexed: i64,
        total: i64,
        dropped: i64,
    },
}

/// v4's result message — its two sentences, with the singular / plural.
pub fn reindex_message(dropped: i64) -> String {
    if dropped > 0 {
        format!(
            "Removed {dropped} Staff message{} from the search index",
            if dropped == 1 { "" } else { "s" }
        )
    } else {
        "No Staff messages were in the search index".to_string()
    }
}

/// v4's `shouldRun()` past its backend / table guards: any object missing,
/// any trigger stale, or the counts disagree — short-circuiting in v4's
/// order.
pub fn should_run(conn: &Connection) -> Result<bool, DbError> {
    if !missing_chat_message_fts_objects(conn)?.is_empty() {
        return Ok(true);
    }
    if !stale_chat_message_fts_triggers(conn)?.is_empty() {
        return Ok(true);
    }
    Ok(count_eligible_chat_messages(conn)? != count_indexed_chat_messages(conn)?)
}

/// What [`run`] did — v4's `run()` body, its lines and its numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReindexRun {
    pub replaced_triggers: Vec<String>,
    pub indexed_before: i64,
    pub scanned: i64,
    pub indexed: i64,
    pub total: i64,
    pub dropped: i64,
}

/// v4's `run()`: `stale` and the indexed count captured FIRST, the ensure,
/// the opening INFO, the rebuild, the closing INFO. Writes no ledger row (the
/// runner does — [`reindex_chat_message_fts_character_only`]).
pub fn run(conn: &Connection) -> Result<ReindexRun, DbError> {
    let stale = stale_chat_message_fts_triggers(conn)?;
    let before = count_indexed_chat_messages(conn)?;
    ensure_chat_message_fts_schema(conn)?;

    tracing::info!(
        target: "quilltap::db",
        context = LOG_CONTEXT,
        // v4 logs an ARRAY; the `…Json` file-layer convention.
        replacedTriggersJson = serde_json::to_string(&stale)
            .unwrap_or_else(|_| "[]".to_string())
            .as_str(),
        indexedBefore = before,
        "Rebuilding chat message search index without Staff messages",
    );

    // v4 passes a `reportProgress` callback — runner UI, not ported.
    let rebuilt = rebuild_chat_message_fts_index(conn, None)?;
    let dropped = (before - rebuilt.indexed).max(0);

    // v4's closing line carries no `durationMs`.
    tracing::info!(
        target: "quilltap::db",
        context = LOG_CONTEXT,
        scanned = rebuilt.scanned,
        indexed = rebuilt.indexed,
        total = rebuilt.total,
        dropped,
        "Chat message search index rebuilt",
    );

    Ok(ReindexRun {
        replaced_triggers: stale,
        indexed_before: before,
        scanned: rebuilt.scanned,
        indexed: rebuilt.indexed,
        total: rebuilt.total,
        dropped,
    })
}

/// The boot ensure: v4's runner sequence over v4's migration, once per
/// instance (see the module doc for every arm). `now_iso` stamps the ledger
/// row's `completedAt` and `lastChecked`.
pub fn reindex_chat_message_fts_character_only(
    conn: &Connection,
    now_iso: &str,
) -> Result<ReindexOutcome, DbError> {
    // The ledger FIRST, as v4's runner orders it. `ledger_has` answers false
    // on an absent ledger table and creates nothing.
    if ledger_has(conn, MIGRATION_ID)? {
        return Ok(ReindexOutcome::AlreadyCompleted);
    }
    if !super::schema_probe::table_exists(conn, "chat_messages")? {
        return Ok(ReindexOutcome::NotApplicable);
    }
    if !super::schema_probe::table_exists(conn, CHAT_MESSAGE_FTS_MAP_TABLE)? {
        return Ok(ReindexOutcome::DependencyPending);
    }
    if !should_run(conn)? {
        return Ok(ReindexOutcome::NotNeeded);
    }
    let done = run(conn)?;
    stamp_completed(
        conn,
        MIGRATION_ID,
        now_iso,
        done.dropped,
        &reindex_message(done.dropped),
    )?;
    Ok(ReindexOutcome::Ran {
        replaced_triggers: done.replaced_triggers,
        indexed_before: done.indexed_before,
        scanned: done.scanned,
        indexed: done.indexed,
        total: done.total,
        dropped: done.dropped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::chat_message_fts::tests::LEGACY_INSERT_TRIGGER;
    use crate::db::text_compression::register_qt_text;
    use crate::test_support::captured_with;

    const NOW: &str = "2026-11-04T12:00:00.000Z";

    const REDUCED_DDL: &str = r#"CREATE TABLE "chat_messages" (
        "id" TEXT PRIMARY KEY, "chatId" TEXT NOT NULL, "type" TEXT DEFAULT 'message',
        "role" TEXT, "content" TEXT,
        "systemSender" TEXT DEFAULT NULL, "customAnnouncer" TEXT DEFAULT NULL,
        "createdAt" TEXT NOT NULL)"#;

    fn base() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        register_qt_text(&conn).unwrap();
        conn.execute_batch(REDUCED_DDL).unwrap();
        ensure_chat_message_fts_schema(&conn).unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, sender: Option<&str>) {
        conn.execute(
            r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","systemSender","createdAt")
               VALUES (?,'c1','message','ASSISTANT','a dirigible',?,'2026-01-01T00:00:00.000Z')"#,
            rusqlite::params![id, sender],
        )
        .unwrap();
    }

    fn legacy(conn: &Connection) {
        conn.execute_batch(r#"DROP TRIGGER "chat_messages_fts_ai""#)
            .unwrap();
        conn.execute_batch(LEGACY_INSERT_TRIGGER).unwrap();
    }

    fn tables(conn: &Connection) -> Vec<String> {
        conn.prepare("SELECT name FROM sqlite_master WHERE name LIKE 'migrations_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn infos(lines: &[String]) -> Vec<&String> {
        lines
            .iter()
            .filter(|l| l.contains("context=migrations.reindex-chat-message-fts-character-only"))
            .collect()
    }

    #[test]
    fn the_message_has_v4s_three_shapes() {
        assert_eq!(
            reindex_message(0),
            "No Staff messages were in the search index"
        );
        assert_eq!(
            reindex_message(1),
            "Removed 1 Staff message from the search index"
        );
        assert_eq!(
            reindex_message(2),
            "Removed 2 Staff messages from the search index"
        );
    }

    /// The ran arm: a pre-round instance (legacy trigger, two Staff rows
    /// indexed) — v4's two INFO lines with their fields and NO `durationMs`,
    /// the stamp, and a search index without the Staff.
    #[test]
    fn a_pre_round_instance_runs_logs_and_stamps() {
        let conn = base();
        legacy(&conn);
        insert(&conn, "a", Some("host"));
        insert(&conn, "b", Some("commonplaceBook"));
        insert(&conn, "c", None);
        let (out, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&conn, NOW).unwrap());
        assert_eq!(
            out,
            ReindexOutcome::Ran {
                replaced_triggers: vec!["chat_messages_fts_ai".into()],
                indexed_before: 3,
                scanned: 1,
                indexed: 1,
                total: 1,
                dropped: 2,
            }
        );
        let got = infos(&lines);
        assert_eq!(got.len(), 2, "{lines:?}");
        assert!(got[0].starts_with("INFO quilltap::db"), "{}", got[0]);
        assert!(got[0].contains("Rebuilding chat message search index without Staff messages"));
        assert!(
            got[0].contains(r#"replacedTriggersJson=["chat_messages_fts_ai"]"#),
            "{}",
            got[0]
        );
        assert!(got[0].contains("indexedBefore=3"), "{}", got[0]);
        assert!(got[1].starts_with("INFO quilltap::db"), "{}", got[1]);
        assert!(got[1].contains("Chat message search index rebuilt"));
        for f in ["scanned=1", "indexed=1", "total=1", "dropped=2"] {
            assert!(got[1].contains(f), "{f}: {}", got[1]);
        }
        assert!(!got[1].contains("durationMs"), "{}", got[1]);
        let row: (i64, String) = conn
            .query_row(
                "SELECT itemsAffected, message FROM migrations_state WHERE id = ?",
                [MIGRATION_ID],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (2, "Removed 2 Staff messages from the search index".into())
        );
        // A second pass is AlreadyCompleted and silent.
        let (again, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&conn, NOW).unwrap());
        assert_eq!(again, ReindexOutcome::AlreadyCompleted);
        assert!(infos(&lines).is_empty());
    }

    /// Each `should_run` arm alone.
    #[test]
    fn should_run_has_three_arms() {
        let conn = base();
        assert!(!should_run(&conn).unwrap());
        conn.execute_batch(r#"DROP TRIGGER "chat_messages_fts_ad""#)
            .unwrap();
        assert!(should_run(&conn).unwrap(), "missing");
        ensure_chat_message_fts_schema(&conn).unwrap();
        legacy(&conn);
        assert!(should_run(&conn).unwrap(), "stale");
        ensure_chat_message_fts_schema(&conn).unwrap();
        assert!(!should_run(&conn).unwrap());
        insert(&conn, "p", None);
        conn.execute_batch(r#"DELETE FROM "chat_messages_fts_map""#)
            .unwrap();
        assert!(should_run(&conn).unwrap(), "counts");
    }

    /// The silence legs: AlreadyCompleted / NotApplicable / DependencyPending
    /// / NotNeeded log neither INFO and write no ledger table or row.
    #[test]
    fn the_skip_arms_are_silent_and_write_nothing() {
        // NotNeeded — current triggers, counts equal.
        let conn = base();
        insert(&conn, "p", None);
        let (out, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&conn, NOW).unwrap());
        assert_eq!(out, ReindexOutcome::NotNeeded);
        assert!(infos(&lines).is_empty());
        assert!(tables(&conn).is_empty());

        // NotApplicable — no chat_messages.
        let bare = Connection::open_in_memory().unwrap();
        let (out, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&bare, NOW).unwrap());
        assert_eq!(out, ReindexOutcome::NotApplicable);
        assert!(infos(&lines).is_empty());
        assert!(tables(&bare).is_empty());

        // DependencyPending — chat_messages but no FTS objects (fresh v5).
        let fresh = Connection::open_in_memory().unwrap();
        register_qt_text(&fresh).unwrap();
        fresh.execute_batch(REDUCED_DDL).unwrap();
        let (out, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&fresh, NOW).unwrap());
        assert_eq!(out, ReindexOutcome::DependencyPending);
        assert!(infos(&lines).is_empty());
        assert!(tables(&fresh).is_empty());
        // … and v4's literal `run` on that base throws, which is why the gate
        // exists.
        let err = run(&fresh).unwrap_err().to_string();
        assert!(
            err.contains("no such table: chat_messages_fts_map"),
            "{err}"
        );

        // AlreadyCompleted — a legacy trigger STAYS legacy.
        let done = base();
        legacy(&done);
        stamp_completed(&done, MIGRATION_ID, NOW, 0, "x").unwrap();
        let (out, lines) =
            captured_with(|| reindex_chat_message_fts_character_only(&done, NOW).unwrap());
        assert_eq!(out, ReindexOutcome::AlreadyCompleted);
        assert!(infos(&lines).is_empty());
        assert_eq!(
            stale_chat_message_fts_triggers(&done).unwrap(),
            vec!["chat_messages_fts_ai".to_string()]
        );
    }

    /// Ran with nothing to drop: a counts-only drift runs, dropped 0, the zero
    /// message.
    #[test]
    fn a_counts_only_drift_runs_with_the_zero_message() {
        let conn = base();
        insert(&conn, "p", None);
        conn.execute_batch(r#"DELETE FROM "chat_messages_fts_map""#)
            .unwrap();
        let out = reindex_chat_message_fts_character_only(&conn, NOW).unwrap();
        assert!(matches!(
            out,
            ReindexOutcome::Ran {
                dropped: 0,
                indexed_before: 0,
                indexed: 1,
                ..
            }
        ));
        let msg: String = conn
            .query_row("SELECT message FROM migrations_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(msg, "No Staff messages were in the search index");
    }
}
