//! The `chat_messages` full-text index — DDL, triggers and rebuild. A whole
//! port of v4 `lib/database/backends/sqlite/chat-message-fts.ts`
//! (`f45a517a9`).
//!
//! This module is the SINGLE SOURCE OF TRUTH for the FTS5 schema behind global
//! message search. The boot reconciler that heals it
//! (`quilltap_host::host::seed_built_ins`) and the search itself
//! ([`super::chats_search`]) share these statements; nothing else may spell
//! this DDL, and nothing outside this module may write to `chat_messages_fts`
//! or `chat_messages_fts_map` — the triggers own them, and
//! [`rebuild_chat_message_fts_index`] is the only bulk writer.
//!
//! ## Why an index at all
//!
//! Global message search used to be `content LIKE '%…%'` with no index that
//! could serve it: every search read the whole column (355 MB on v4's
//! reference instance, 55 ms). That full scan was also the one thing blocking
//! `chat_messages.content` from being stored brotli-compressed, because a
//! compressed BLOB cannot be `LIKE`-matched. Replacing the scan with an index
//! makes search ~250× faster AND unblocks the compression.
//!
//! ## Why contentless (`content=''`)
//!
//! An EXTERNAL-content FTS5 table reads the source column to tokenize it. Once
//! `content` holds a brotli BLOB, that would index the compressed bytes. A
//! CONTENTLESS table stores only the inverted index and never looks at the
//! base table, so it is correct regardless of how `content` is encoded. It
//! also halves the cost: 18.6 MB per 20,000 rows.
//!
//! Two consequences fall out of that choice:
//!
//! - `snippet()` / `highlight()` are unavailable — snippets are built in Rust
//!   by the search route ([`crate::api::ui_search`]'s `create_snippet`).
//! - `INSERT INTO chat_messages_fts(chat_messages_fts) VALUES('rebuild')` is
//!   REFUSED on a contentless table, which is why
//!   [`rebuild_chat_message_fts_index`] deletes and re-inserts by hand.
//!
//! ## Why the id-mapping table
//!
//! `chat_messages` is declared `"id" TEXT PRIMARY KEY`, so its rowid is
//! IMPLICIT. FTS5 keys every index entry on an integer rowid, and a
//! contentless table can hand back nothing else. SQLite reserves the right to
//! renumber the implicit rowids of such a table during `VACUUM` (which
//! `quilltap db optimize` runs), and any table rebuild —
//! `CREATE new … INSERT … SELECT … DROP … RENAME` — reassigns every rowid AND
//! silently drops the triggers with the old table.
//!
//! `chat_messages_fts_map` gives the index a stable integer identity that
//! `VACUUM` never renumbers (an explicit `INTEGER PRIMARY KEY`), and its
//! `UNIQUE` message id makes the per-row trigger lookups O(log n). The boot
//! reconciler catches the dropped-trigger case.
//!
//! ## Why triggers rather than repository hooks
//!
//! `chat_messages` is written from the writer task, from restore, from
//! migrations and from several repository methods. A trigger cannot be
//! bypassed by a new write path; an application-layer hook can, and silently.
//!
//! The triggers call `qt_text()` ([`super::text_compression::register_qt_text`]),
//! so a connection that opens without it fails any write to `chat_messages`
//! with "no such function: qt_text". **That is deliberate on v4's side and
//! must stay deliberate here: a loud, immediate failure beats silent index
//! drift.** It is also the wall the drift ledger measured — a v5 binary
//! without the codec substrate can DELETE messages from a v4-4.10 instance but
//! not create them, because SQLite resolves a trigger's functions when it
//! COMPILES the trigger program, before any row is tested.
//!
//! ## The DDL is RECORDED, not transcribed
//!
//! [`CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS`] is byte-identical to v4's array,
//! generated from a recording of v4's REAL module into
//! `harness/oracle/fixtures/chat-message-fts-ddl.json` and pinned against it
//! by `chat_message_fts_equivalence` — which ALSO compares the resulting
//! `sqlite_master.sql` text from a v4-built database against v5's, so the
//! on-disk objects are compared rather than the source literals. The
//! continuation lines below therefore sit at column 0 on purpose: the indents
//! inside each statement are v4's bytes, not Rust formatting.

use rusqlite::Connection;

use super::DbError;

/// The `context` field on every line this module logs. v4 reaches these three
/// through the bare `@/lib/logger`, which carries no service name; v5's file
/// layer wants one, and the module's own name is the honest answer.
const LOG_CONTEXT: &str = "db.chat-message-fts";

/// The contentless FTS5 index (v4 `CHAT_MESSAGE_FTS_TABLE`).
pub const CHAT_MESSAGE_FTS_TABLE: &str = "chat_messages_fts";

/// Stable integer identity for index entries; see the module doc (v4
/// `CHAT_MESSAGE_FTS_MAP_TABLE`).
pub const CHAT_MESSAGE_FTS_MAP_TABLE: &str = "chat_messages_fts_map";

/// The three sync triggers (v4 `CHAT_MESSAGE_FTS_TRIGGERS`).
pub const CHAT_MESSAGE_FTS_TRIGGERS: [&str; 3] = [
    "chat_messages_fts_ai",
    "chat_messages_fts_ad",
    "chat_messages_fts_au",
];

/// Rows re-indexed per transaction during a rebuild (v4 `REBUILD_BATCH_SIZE`).
const REBUILD_BATCH_SIZE: usize = 500;

/// The eligibility filter, as a SQL fragment (v4
/// `chatMessageFtsEligibilitySql`).
///
/// Only `type='message'` rows with `role IN ('USER','ASSISTANT')` and non-null
/// `content` are indexed — exactly the filter global search has always
/// applied, so the index is as small as it can be while answering every
/// question the search bar asks. System events, informs and Staff messages
/// stay unsearchable, as they always have been.
///
/// Lives here so the triggers, the counts and the rebuild cannot drift apart.
///
/// `alias` qualifies the columns with a table alias (`"m"` → `m."type"`), or
/// `""` for an unqualified reference.
pub fn chat_message_fts_eligibility_sql(alias: &str) -> String {
    let p = if alias.is_empty() {
        String::new()
    } else {
        format!("{alias}.")
    };
    format!(
        "{p}\"type\" = 'message' AND {p}\"role\" IN ('USER','ASSISTANT') AND {p}\"content\" IS NOT NULL"
    )
}

/// Every DDL statement, in dependency order (v4
/// `CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS`). All `IF NOT EXISTS`, so replaying
/// them on a healthy instance is a no-op.
pub const CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS: [&str; 5] = [
    r#"CREATE TABLE IF NOT EXISTS "chat_messages_fts_map" (
  "ftsId"     INTEGER PRIMARY KEY,
  "messageId" TEXT NOT NULL UNIQUE
)"#,
    r#"CREATE VIRTUAL TABLE IF NOT EXISTS "chat_messages_fts" USING fts5(
  content,
  content='',
  contentless_delete=1,
  tokenize='unicode61 remove_diacritics 2'
)"#,
    r#"CREATE TRIGGER IF NOT EXISTS "chat_messages_fts_ai" AFTER INSERT ON "chat_messages"
  WHEN new."type" = 'message' AND new."role" IN ('USER','ASSISTANT') AND new."content" IS NOT NULL
BEGIN
  INSERT INTO "chat_messages_fts_map"("messageId") VALUES (new."id");
  INSERT INTO "chat_messages_fts"(rowid, content)
    VALUES ((SELECT "ftsId" FROM "chat_messages_fts_map" WHERE "messageId" = new."id"),
            qt_text(new."content"));
END"#,
    r#"CREATE TRIGGER IF NOT EXISTS "chat_messages_fts_ad" AFTER DELETE ON "chat_messages"
  WHEN EXISTS (SELECT 1 FROM "chat_messages_fts_map" WHERE "messageId" = old."id")
BEGIN
  DELETE FROM "chat_messages_fts"
    WHERE rowid = (SELECT "ftsId" FROM "chat_messages_fts_map" WHERE "messageId" = old."id");
  DELETE FROM "chat_messages_fts_map" WHERE "messageId" = old."id";
END"#,
    r#"CREATE TRIGGER IF NOT EXISTS "chat_messages_fts_au" AFTER UPDATE OF "content" ON "chat_messages"
  WHEN qt_text(new."content") IS NOT qt_text(old."content")
   AND EXISTS (SELECT 1 FROM "chat_messages_fts_map" WHERE "messageId" = old."id")
BEGIN
  DELETE FROM "chat_messages_fts"
    WHERE rowid = (SELECT "ftsId" FROM "chat_messages_fts_map" WHERE "messageId" = old."id");
  INSERT INTO "chat_messages_fts"(rowid, content)
    VALUES ((SELECT "ftsId" FROM "chat_messages_fts_map" WHERE "messageId" = old."id"),
            qt_text(new."content"));
END"#,
];

/// Create the index, the map and the triggers if they are missing (v4
/// `ensureChatMessageFtsSchema`).
///
/// Every statement is `IF NOT EXISTS`, so this is a cheap no-op on a healthy
/// instance and is safe to call from every boot.
pub fn ensure_chat_message_fts_schema(conn: &Connection) -> Result<(), DbError> {
    for sql in CHAT_MESSAGE_FTS_SCHEMA_STATEMENTS {
        conn.execute_batch(sql)?;
    }
    Ok(())
}

/// Names of every schema object this module owns, for `sqlite_master` checks
/// (v4 `chatMessageFtsObjectNames`).
pub fn chat_message_fts_object_names() -> Vec<&'static str> {
    let mut names = vec![CHAT_MESSAGE_FTS_MAP_TABLE, CHAT_MESSAGE_FTS_TABLE];
    names.extend(CHAT_MESSAGE_FTS_TRIGGERS);
    names
}

/// Report which of this module's schema objects are absent from `sqlite_master`
/// (v4 `missingChatMessageFtsObjects`).
///
/// A table rebuild of `chat_messages` drops the triggers with the old table and
/// says nothing about it, so "which are missing" is the question the boot
/// reconciler needs answered.
pub fn missing_chat_message_fts_objects(conn: &Connection) -> Result<Vec<String>, DbError> {
    let names = chat_message_fts_object_names();
    let placeholders = (0..names.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT name FROM sqlite_master\n        WHERE type IN ('table','trigger') AND name IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let present: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(names.iter()), |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names
        .into_iter()
        .filter(|n| !present.iter().any(|p| p == n))
        .map(str::to_string)
        .collect())
}

/// How many `chat_messages` rows the index is supposed to hold (v4
/// `countEligibleChatMessages`).
pub fn count_eligible_chat_messages(conn: &Connection) -> Result<i64, DbError> {
    let sql = format!(
        "SELECT COUNT(*) AS n FROM \"chat_messages\" WHERE {}",
        chat_message_fts_eligibility_sql("")
    );
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

/// How many it actually holds. Counted on the map, which is a plain table (v4
/// `countIndexedChatMessages`).
pub fn count_indexed_chat_messages(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row(
        &format!("SELECT COUNT(*) AS n FROM \"{CHAT_MESSAGE_FTS_MAP_TABLE}\""),
        [],
        |row| row.get(0),
    )?)
}

/// What [`rebuild_chat_message_fts_index`] did (v4
/// `ChatMessageFtsRebuildResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatMessageFtsRebuildResult {
    /// Rows read from `chat_messages`.
    pub scanned: i64,
    /// Rows written into the index.
    pub indexed: i64,
    /// Eligible rows counted up front (what `scanned` should reach).
    pub total: i64,
    pub duration_ms: u128,
}

/// Drop and re-populate the index from `chat_messages` (v4
/// `rebuildChatMessageFtsIndex`).
///
/// FTS5 refuses `'rebuild'` on a contentless table, so this empties both tables
/// and walks the base table itself, keyset-paginated by `rowid`. A cursor on
/// the implicit rowid is fine WITHIN one run — the identity problem the mapping
/// table solves is about rowids changing across time, not during a walk.
///
/// The text is read through `qt_text()`, so this is correct whether or not the
/// compression backfill has run.
///
/// `on_progress` is called once per batch with `(scanned, total)`.
pub fn rebuild_chat_message_fts_index(
    conn: &Connection,
    mut on_progress: Option<&mut dyn FnMut(i64, i64)>,
) -> Result<ChatMessageFtsRebuildResult, DbError> {
    let started_at = std::time::Instant::now();
    let total = count_eligible_chat_messages(conn)?;

    tracing::debug!(
        target: "quilltap::db",
        context = LOG_CONTEXT,
        total,
        "Rebuilding chat message FTS index",
    );

    conn.execute_batch(&format!("DELETE FROM \"{CHAT_MESSAGE_FTS_TABLE}\""))?;
    conn.execute_batch(&format!("DELETE FROM \"{CHAT_MESSAGE_FTS_MAP_TABLE}\""))?;

    let select_sql = format!(
        "SELECT rowid AS rid, \"id\" AS id, qt_text(\"content\") AS text\n       FROM \"chat_messages\"\n      WHERE rowid > ? AND {}\n      ORDER BY rowid\n      LIMIT {REBUILD_BATCH_SIZE}",
        chat_message_fts_eligibility_sql("")
    );

    let mut scanned = 0i64;
    let mut indexed = 0i64;
    let mut last_rowid = 0i64;
    loop {
        let batch: Vec<(i64, String, Option<String>)> = {
            let mut stmt = conn.prepare(&select_sql)?;
            let rows = stmt
                .query_map([last_rowid], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<Result<_, _>>()?;
            rows
        };
        if batch.is_empty() {
            break;
        }
        last_rowid = batch[batch.len() - 1].0;

        // v4 wraps each batch in `db.transaction(...)`.
        let tx = conn.unchecked_transaction()?;
        let mut written = 0i64;
        {
            let mut insert_map = tx.prepare(&format!(
                "INSERT INTO \"{CHAT_MESSAGE_FTS_MAP_TABLE}\"(\"messageId\") VALUES (?)"
            ))?;
            let mut insert_fts = tx.prepare(&format!(
                "INSERT INTO \"{CHAT_MESSAGE_FTS_TABLE}\"(rowid, content) VALUES (?, ?)"
            ))?;
            for (_, id, text) in &batch {
                insert_map.execute([id])?;
                let fts_id = tx.last_insert_rowid();
                // v4's `row.text ?? ''`: the eligibility filter excludes NULL
                // `content`, but `qt_text` of a corrupt cell can still be NULL.
                insert_fts.execute(rusqlite::params![fts_id, text.as_deref().unwrap_or("")])?;
                written += 1;
            }
        }
        tx.commit()?;

        indexed += written;
        scanned += batch.len() as i64;
        if let Some(cb) = on_progress.as_deref_mut() {
            cb(scanned, total);
        }
        tracing::debug!(
            target: "quilltap::db",
            context = LOG_CONTEXT,
            scanned,
            total,
            "Chat message FTS rebuild batch",
        );
    }

    let duration_ms = started_at.elapsed().as_millis();
    tracing::debug!(
        target: "quilltap::db",
        context = LOG_CONTEXT,
        scanned,
        indexed,
        total,
        // v4 logs this key as `durationMs`
        // (`lib/database/backends/sqlite/chat-message-fts.ts:293`), as every
        // other v5 site does (`chat_message_fts_reconcile.rs:158`).
        durationMs = duration_ms,
        "Chat message FTS index rebuilt",
    );
    Ok(ChatMessageFtsRebuildResult {
        scanned,
        indexed,
        total,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::text_compression::register_qt_text;
    use crate::test_support::captured_with;

    /// The reduced `chat_messages` the rebuild's SELECT and the triggers name —
    /// the same columns `chat-message-fts.json` carries, so these log pins need
    /// no fixture.
    const REDUCED_DDL: &str = r#"CREATE TABLE "chat_messages" (
        "id" TEXT PRIMARY KEY, "chatId" TEXT NOT NULL, "type" TEXT DEFAULT 'message',
        "role" TEXT, "content" TEXT, "createdAt" TEXT NOT NULL)"#;

    /// An in-memory db with `qt_text` registered and the five FTS objects
    /// present, holding `eligible` indexable messages.
    fn db(eligible: usize) -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        register_qt_text(&conn).expect("register qt_text");
        conn.execute_batch(REDUCED_DDL).expect("ddl");
        ensure_chat_message_fts_schema(&conn).expect("ensure fts");
        for i in 0..eligible {
            conn.execute(
                r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","createdAt")
                   VALUES (?,?,'message','USER',?,?)"#,
                rusqlite::params![
                    format!("m{i}"),
                    "c1",
                    format!("the lantern hissed {i}"),
                    format!("2026-01-01T00:00:0{i}.000Z")
                ],
            )
            .expect("seed");
        }
        conn
    }

    fn find<'a>(lines: &'a [String], needle: &str) -> Vec<&'a String> {
        lines.iter().filter(|l| l.contains(needle)).collect()
    }

    /// v4 logs three debug lines per rebuild — the opening `total`, one per
    /// batch, and the closing tally
    /// (`lib/database/backends/sqlite/chat-message-fts.ts:246,288,293`). Pin
    /// all three, their level/target, v5's `context` field, and — on the
    /// closing line only, as v4 has it — the `durationMs` key.
    #[test]
    fn a_rebuild_logs_v4s_three_debug_lines() {
        let conn = db(2);
        let (result, lines) =
            captured_with(|| rebuild_chat_message_fts_index(&conn, None).expect("rebuild"));
        assert_eq!((result.scanned, result.indexed, result.total), (2, 2, 2));

        let opening = find(&lines, "Rebuilding chat message FTS index");
        assert_eq!(opening.len(), 1, "one opening line: {lines:?}");
        assert!(
            opening[0].starts_with("DEBUG quilltap::db"),
            "level/target: {}",
            opening[0]
        );
        assert!(
            opening[0].contains("context=db.chat-message-fts"),
            "{}",
            opening[0]
        );
        assert!(opening[0].contains("total=2"), "{}", opening[0]);
        // v4's opening line carries `total` and nothing else.
        assert!(
            !opening[0].contains("durationMs"),
            "v4's opening line has no duration: {}",
            opening[0]
        );

        // One batch line per batch — two rows fit in one REBUILD_BATCH_SIZE.
        let batch = find(&lines, "Chat message FTS rebuild batch");
        assert_eq!(batch.len(), 1, "one batch line: {lines:?}");
        assert!(
            batch[0].starts_with("DEBUG quilltap::db"),
            "level/target: {}",
            batch[0]
        );
        assert!(
            batch[0].contains("context=db.chat-message-fts"),
            "{}",
            batch[0]
        );
        assert!(batch[0].contains("scanned=2"), "{}", batch[0]);
        assert!(batch[0].contains("total=2"), "{}", batch[0]);
        assert!(
            !batch[0].contains("durationMs"),
            "v4's batch line has no duration: {}",
            batch[0]
        );

        let done = find(&lines, "Chat message FTS index rebuilt");
        assert_eq!(done.len(), 1, "one closing line: {lines:?}");
        assert!(
            done[0].starts_with("DEBUG quilltap::db"),
            "level/target: {}",
            done[0]
        );
        assert!(
            done[0].contains("context=db.chat-message-fts"),
            "{}",
            done[0]
        );
        assert!(done[0].contains("scanned=2"), "{}", done[0]);
        assert!(done[0].contains("indexed=2"), "{}", done[0]);
        assert!(done[0].contains("total=2"), "{}", done[0]);
        // The key is v4's camelCase `durationMs`, not `duration_ms`.
        assert!(done[0].contains("durationMs="), "{}", done[0]);
        assert!(
            !done[0].contains("duration_ms"),
            "snake_case would diverge from v4 and from every other v5 site: {}",
            done[0]
        );
    }

    /// An EMPTY base table still opens and closes the rebuild — v4 logs both
    /// unconditionally — but the per-batch line never fires, because the first
    /// `select.all()` returns nothing and the loop breaks before it.
    #[test]
    fn an_empty_rebuild_logs_the_two_unconditional_lines_and_no_batch() {
        let conn = db(0);
        let (result, lines) =
            captured_with(|| rebuild_chat_message_fts_index(&conn, None).expect("rebuild"));
        assert_eq!((result.scanned, result.indexed, result.total), (0, 0, 0));
        assert_eq!(find(&lines, "Rebuilding chat message FTS index").len(), 1);
        assert_eq!(find(&lines, "Chat message FTS index rebuilt").len(), 1);
        assert!(
            find(&lines, "Chat message FTS rebuild batch").is_empty(),
            "no batch ran, so no batch line: {lines:?}"
        );
    }

    /// The silence leg: standing the schema up and writing messages through the
    /// triggers is not a rebuild, and says NONE of the three.
    #[test]
    fn writing_messages_without_a_rebuild_says_none_of_the_three() {
        let (_, lines) = captured_with(|| {
            let conn = db(3);
            // The triggers really did index them — the silence is not the
            // silence of nothing happening.
            let n: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM \"{CHAT_MESSAGE_FTS_MAP_TABLE}\""),
                    [],
                    |r| r.get(0),
                )
                .expect("count");
            assert_eq!(n, 3);
        });
        for needle in [
            "Rebuilding chat message FTS index",
            "Chat message FTS rebuild batch",
            "Chat message FTS index rebuilt",
        ] {
            assert!(
                find(&lines, needle).is_empty(),
                "no rebuild ran, so {needle:?} must not fire: {lines:?}"
            );
        }
    }
}
