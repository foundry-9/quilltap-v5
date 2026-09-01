//! The recompute-chat-last-message-at boot heal (v4 migration
//! `recompute-chat-last-message-at-v1`, `735d9408c` — bug 112's data pass).
//!
//! `lastMessageAt` is the timestamp every chat list, sort and card reads. It was
//! written by `addMessage`/`addMessages` on *any* `type: 'message'` row, and the
//! Staff (Lantern, Aurora, Librarian, Concierge, Prospero, Host, Commonplace
//! Book, Ariel, Carina, Suparṇā, Pascal) persist their announcements as message
//! rows too. A story background finishing its render, a summary being folded, a
//! Concierge notice — each stamped the chat as freshly active and floated a
//! months-dead conversation to the top of the list. Raw `TOOL` result rows and
//! user announcement bubbles did the same.
//!
//! The write path now bumps `lastMessageAt` only for character-authored content
//! ([`crate::chat_activity`]). This recomputes the column for every existing chat
//! under the same rule, so history reads the way new activity will. Chats with no
//! character-authored message at all are set to NULL, where readers fall back to
//! `createdAt` (`chat_activity_at`) rather than to the drifting `updatedAt`.
//!
//! `updatedAt` is deliberately left alone — it keeps its meaning of "anything
//! about this row changed", it is simply no longer what the reader is shown.
//!
//! ## The once-only mechanism is v4's OWN migration ledger — not a column
//!
//! Like the P4.D97 retire-prefill heal (`thinking_prefill_retire_heal`, the
//! template for this module), the pass is DATA-only with no schema delta to key
//! off, and re-running it would be harmless but pointless. The only guard that
//! survives BOTH apps opening the same instance is v4's `migrations_state`
//! ledger: v4's runner skips any migration whose ledger row exists, and this heal
//! skips when the row exists.
//!
//! **A clean boot writes NO ledger row** — v4's `shouldRun()` is false when
//! nothing has drifted, so its runner never records the migration and simply
//! retries it (cheaply: one correlated-subquery scan) on the next boot. v5 must
//! match that exactly: a stamp on a clean boot would make a LATER v4 boot skip a
//! migration it believes it has already run.
//!
//! The prettify label v4 adds alongside the migration
//! (`'Re-reading each conversation to find when a soul last actually spoke in
//! it…'`) is v4-runner UI with no v5 counterpart — a deliberate non-port.

use rusqlite::Connection;

use super::DbError;

/// v4's migration id — the ledger key both apps honour.
const MIGRATION_ID: &str = "recompute-chat-last-message-at-v1";

/// Columns the pass reads. Absent any of them, there is nothing to recompute
/// from (v4 `REQUIRED_MESSAGE_COLUMNS`).
const REQUIRED_MESSAGE_COLUMNS: &[&str] = &[
    "chatId",
    "type",
    "role",
    "systemSender",
    "customAnnouncer",
    "createdAt",
];

/// v4's `run()` sentence for the no-drift branch. v4's runner never reaches it
/// (`shouldRun()` gates `run()`), and neither does v5's boot path — it is carried,
/// and pinned by the differential, so the two implementations describe the same
/// world.
pub const NO_DRIFT_MESSAGE: &str =
    "All chat last-activity timestamps already reflect character-authored messages";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecomputeOutcome {
    /// The ledger already carries the row (either app completed the pass).
    AlreadyCompleted,
    /// A table or column is not there yet — retried next boot, nothing stamped
    /// (v4's `shouldRun() === false` arm).
    NotApplicable,
    /// Nothing has drifted. v4's runner skips and records NOTHING, so neither
    /// does this — the pass is simply re-checked on the next boot.
    NoDrift,
    /// The pass ran: `updated` chats rewritten, `cleared` of them to NULL, and
    /// the ledger row written.
    Ran { updated: usize, cleared: usize },
}

/// One drifted chat: its stored value and the recomputed one.
struct DriftRow {
    id: String,
    correct: Option<String>,
}

/// Every chat whose stored `lastMessageAt` disagrees with the recomputed one.
///
/// v4 uses `IS NOT` rather than `<>` so a NULL on either side counts as a
/// difference — the Staff-only chats that must be CLEARED are exactly the rows
/// going to NULL, and `<>` would never see them.
fn find_drift(conn: &Connection) -> Result<Vec<DriftRow>, DbError> {
    let correct = format!(
        "(SELECT MAX(m.\"createdAt\") FROM \"chat_messages\" m \
          WHERE m.\"chatId\" = c.\"id\" AND {})",
        crate::chat_activity::CHARACTER_AUTHORED_MESSAGE_FILTER
    );
    let sql = format!(
        "SELECT c.\"id\" AS \"id\", {correct} AS \"correct\" \
           FROM \"chats\" c \
          WHERE c.\"lastMessageAt\" IS NOT {correct}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DriftRow {
                id: r.get(0)?,
                correct: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Run the recompute once per instance, guarded by v4's own migration ledger.
/// `now_iso` stamps `completedAt`/`lastChecked` (the caller passes
/// [`crate::clock::now_iso`]).
pub fn recompute_chat_last_message_at(
    main: &Connection,
    now_iso: &str,
) -> Result<RecomputeOutcome, DbError> {
    // The completed check comes FIRST, exactly as v4's runner orders it
    // (`isMigrationCompleted` before `shouldRun`).
    if table_exists(main, "migrations_state")? {
        let mut stmt = main.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
        if stmt.exists([MIGRATION_ID])? {
            return Ok(RecomputeOutcome::AlreadyCompleted);
        }
    }

    // v4 `shouldRun`: both tables, the `chats` column, and every message column
    // this reads — else skip WITHOUT stamping.
    if !table_exists(main, "chats")? || !table_exists(main, "chat_messages")? {
        return Ok(RecomputeOutcome::NotApplicable);
    }
    if !column_names(main, "chats")?
        .iter()
        .any(|c| c == "lastMessageAt")
    {
        return Ok(RecomputeOutcome::NotApplicable);
    }
    let message_columns = column_names(main, "chat_messages")?;
    if !REQUIRED_MESSAGE_COLUMNS
        .iter()
        .all(|want| message_columns.iter().any(|c| c == want))
    {
        return Ok(RecomputeOutcome::NotApplicable);
    }

    let drifted = find_drift(main)?;
    if drifted.is_empty() {
        // v4's `shouldRun()` is false here: no run, NO ledger row, retried next
        // boot. See the module header — a stamp would poison v4's own skip.
        return Ok(RecomputeOutcome::NoDrift);
    }

    let cleared = drifted.iter().filter(|r| r.correct.is_none()).count();

    // One transaction, as v4's `db.transaction(...)` is.
    let tx = main.unchecked_transaction()?;
    {
        let mut stmt =
            tx.prepare("UPDATE \"chats\" SET \"lastMessageAt\" = ?1 WHERE \"id\" = ?2")?;
        for row in &drifted {
            stmt.execute(rusqlite::params![row.correct, row.id])?;
        }
    }
    tx.commit()?;

    // The ledger write — v4's `migrations/state.ts` shapes verbatim (the P4.D97
    // heal's shapes, unchanged): both tables created lazily under the
    // migrations_state absence check, the row appended, the two metadata keys
    // upserted.
    if !table_exists(main, "migrations_state")? {
        main.execute_batch(
            "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n        \"id\" TEXT PRIMARY KEY,\n        \"completedAt\" TEXT NOT NULL,\n        \"quilltapVersion\" TEXT NOT NULL,\n        \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n        \"message\" TEXT\n      );\n      CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n        \"key\" TEXT PRIMARY KEY,\n        \"value\" TEXT NOT NULL\n      );",
        )?;
    }
    let message = format!(
        "Recomputed last-activity for {} chat{} ({} with no character-authored messages)",
        drifted.len(),
        if drifted.len() == 1 { "" } else { "s" },
        cleared
    );
    main.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            now_iso,
            env!("CARGO_PKG_VERSION"),
            drifted.len() as i64,
            message
        ],
    )?;
    for (k, v) in [
        ("lastChecked", now_iso),
        ("quilltapVersion", env!("CARGO_PKG_VERSION")),
    ] {
        main.execute(
            "INSERT INTO migrations_metadata (key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![k, v],
        )?;
    }

    Ok(RecomputeOutcome::Ran {
        updated: drifted.len(),
        cleared,
    })
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's own integration-test schema: the migration-vintage reduced tables.
    fn vintage_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE chats (\n               id TEXT PRIMARY KEY,\n               lastMessageAt TEXT,\n               createdAt TEXT NOT NULL,\n               updatedAt TEXT NOT NULL\n             );\n             CREATE TABLE chat_messages (\n               id TEXT PRIMARY KEY,\n               chatId TEXT NOT NULL,\n               type TEXT DEFAULT 'message',\n               role TEXT,\n               systemSender TEXT DEFAULT NULL,\n               customAnnouncer TEXT DEFAULT NULL,\n               createdAt TEXT NOT NULL\n             );",
        )
        .expect("ddl");
        db
    }

    fn add_chat(db: &Connection, id: &str, last: Option<&str>) {
        db.execute(
            "INSERT INTO chats (id, lastMessageAt, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                id,
                last,
                "2026-01-01T00:00:00.000Z",
                "2026-12-31T00:00:00.000Z"
            ],
        )
        .expect("chat");
    }

    fn add_message(db: &Connection, id: &str, chat: &str, created: &str, sender: Option<&str>) {
        db.execute(
            "INSERT INTO chat_messages (id, chatId, type, role, systemSender, createdAt) \
             VALUES (?1, ?2, 'message', 'ASSISTANT', ?3, ?4)",
            rusqlite::params![id, chat, sender, created],
        )
        .expect("message");
    }

    #[test]
    fn a_clean_instance_stamps_nothing_so_a_later_v4_boot_still_runs_it() {
        let db = vintage_db();
        add_chat(&db, "correct", Some("2026-07-07T00:00:00.000Z"));
        add_message(&db, "m1", "correct", "2026-07-07T00:00:00.000Z", None);

        let out = recompute_chat_last_message_at(&db, "2026-09-01T00:00:00.000Z").expect("heal");
        assert_eq!(out, RecomputeOutcome::NoDrift);
        assert!(!table_exists(&db, "migrations_state").unwrap());
    }

    #[test]
    fn the_ledger_row_makes_the_pass_once_only() {
        let db = vintage_db();
        add_chat(&db, "drifted", Some("2026-08-30T00:00:00.000Z"));
        add_message(&db, "m1", "drifted", "2026-08-01T00:00:00.000Z", None);
        add_message(
            &db,
            "m2",
            "drifted",
            "2026-08-30T00:00:00.000Z",
            Some("pascal"),
        );

        let out = recompute_chat_last_message_at(&db, "2026-09-01T00:00:00.000Z").expect("heal");
        assert_eq!(
            out,
            RecomputeOutcome::Ran {
                updated: 1,
                cleared: 0
            }
        );
        let again =
            recompute_chat_last_message_at(&db, "2026-09-02T00:00:00.000Z").expect("heal again");
        assert_eq!(again, RecomputeOutcome::AlreadyCompleted);
    }

    #[test]
    fn a_missing_table_is_not_applicable_and_stamps_nothing() {
        let db = Connection::open_in_memory().expect("open");
        let out = recompute_chat_last_message_at(&db, "2026-09-01T00:00:00.000Z").expect("heal");
        assert_eq!(out, RecomputeOutcome::NotApplicable);
        assert!(!table_exists(&db, "migrations_state").unwrap());
    }
}
