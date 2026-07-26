//! Fictional-clock anchor backfill — the port of v4's migration
//! `anchor-fictional-clock-base-v1` (`migrations/scripts/
//! anchor-fictional-clock-base.ts`, the `e3a9654f` fictional-story-clock fix).
//!
//! Chats using fictional time advance their story clock 1:1 with the wall clock,
//! measured from `timestampConfig.fictionalBaseRealTime`. Nothing ever wrote
//! that field — its only writer was an uncalled helper — so
//! [`crate::chat_timestamp::calculate_current_timestamp`] fell back to "now",
//! measured zero elapsed time, and re-reported the configured base instant on
//! every turn. The Host announced the same moment forever and the story clock
//! never moved.
//!
//! The write path now stamps the anchor at chat creation
//! ([`crate::services::chat_create`]). This backfills chats created before that,
//! using each chat's own `createdAt` — the instant its base timestamp was
//! chosen, and so the anchor it would have carried had the field ever been
//! written. Story time therefore resumes exactly where 1:1 tracking from chat
//! creation would have put it, rather than lurching.
//!
//! # Why a boot pass and not a migration
//!
//! v5's migration runner is deferred, so v4's migration has no home. The
//! precedent is [`crate::db::mount_index_case_repair`] (v4 `0a0419f5`), which is
//! likewise a v4 migration re-homed as a once-per-startup repair invoked from
//! the host's boot seed. This follows it exactly, on the **main** partition.
//!
//! Idempotent by construction — it only touches rows that have no anchor — so no
//! marker row is needed. v4's own `needsAnchor` guard is the marker, which is
//! also why re-running is free: a second pass finds nothing.

use rusqlite::{params, Connection};
use serde_json::Value;

use super::DbError;
use crate::chat_timestamp::ensure_fictional_base_real_time;

/// v4's candidate pre-filter: a cheap SQL narrowing on the JSON *text*, refined
/// by real parsing below. Reproduced byte-for-byte, including the fact that the
/// `LIKE` pattern only matches the compact `"useFictionalTime":true` spelling —
/// a blob serialized with a space after the colon is invisible to it, on both
/// sides. `fictionalBaseRealTime` may legitimately appear as an explicit null,
/// so its presence in the text is not on its own proof of an anchor.
const CANDIDATE_SQL: &str = r#"SELECT "id", "createdAt", "timestampConfig"
         FROM "chats"
        WHERE "timestampConfig" IS NOT NULL
          AND "timestampConfig" LIKE '%"useFictionalTime":true%'"#;

/// Parse a stored blob, tolerating the malformed and the empty — v4's
/// `parseConfig`. A chat whose config we cannot read is a chat we must not
/// rewrite, so arrays, scalars and unparseable text all yield `None`.
fn parse_config(raw: Option<&str>) -> Option<Value> {
    let raw = raw?;
    if raw.trim().is_empty() {
        return None;
    }
    let parsed: Value = serde_json::from_str(raw).ok()?;
    parsed.is_object().then_some(parsed)
}

/// Whether `chats` exists and carries both columns the pass reads — v4's
/// `shouldRun` guard (`sqliteTableExists` + `getSQLiteTableColumns`).
fn columns_present(db: &Connection) -> Result<bool, DbError> {
    let table_exists: bool = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'chats'",
        [],
        |r| r.get::<_, i64>(0).map(|n| n > 0),
    )?;
    if !table_exists {
        return Ok(false);
    }
    let mut stmt = db.prepare("PRAGMA table_info(\"chats\")")?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<_, _>>()?;
    Ok(names.iter().any(|n| n == "timestampConfig") && names.iter().any(|n| n == "createdAt"))
}

/// Backfill `timestampConfig.fictionalBaseRealTime` from each chat's own
/// `createdAt` — v4's migration `run()`. Returns the number of rows anchored.
///
/// `now_ms` is v4's `new Date()`, used only for the two fallback arms: a row
/// with no `createdAt`, or one whose `createdAt` JS cannot parse. An anchor of
/// now still beats a clock frozen forever.
///
/// The stamp itself goes through [`ensure_fictional_base_real_time`], so the
/// backfill and the create path cannot drift apart: v4 re-implements the same
/// three truthiness guards inline in the migration, and a config that needs no
/// anchor comes back unchanged and is skipped.
pub fn anchor_fictional_clock_bases(db: &Connection, now_ms: i64) -> Result<u64, DbError> {
    if !columns_present(db)? {
        return Ok(0);
    }

    struct Candidate {
        id: String,
        created_at: Option<String>,
        timestamp_config: Option<String>,
    }
    let candidates: Vec<Candidate> = {
        let mut stmt = db.prepare(CANDIDATE_SQL)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    created_at: r.get(1)?,
                    timestamp_config: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        rows
    };

    let mut updates: Vec<(String, String)> = Vec::new();
    for row in &candidates {
        let Some(config) = parse_config(row.timestamp_config.as_deref()) else {
            continue;
        };
        // createdAt is the instant the base was chosen. `new Date(null)` and an
        // unparseable string both land on "now" in v4 (the explicit ternary and
        // the NaN re-check respectively).
        //
        // The column is written by us as `toISOString()`, so it is always
        // zone-bearing; a hypothetical zone-less value would be host-local in v4
        // and UTC here, which no v4 or v5 write path can produce.
        let anchor_ms = row
            .created_at
            .as_deref()
            .and_then(crate::episodic::js_date_parse_ms)
            .unwrap_or(now_ms);

        let stamped = ensure_fictional_base_real_time(&config, anchor_ms);
        if stamped == config {
            continue;
        }
        // `Value`'s Display IS compact JSON, and with `preserve_order` it emits
        // insertion order — v4's `JSON.stringify` on the spread result.
        updates.push((row.id.clone(), stamped.to_string()));
    }

    if !updates.is_empty() {
        // v4 wraps the writes in one better-sqlite3 transaction. Here the caller
        // already holds the writer's transaction, so the loop is the whole
        // apply.
        let mut stmt = db.prepare(r#"UPDATE "chats" SET "timestampConfig" = ? WHERE "id" = ?"#)?;
        for (id, config) in &updates {
            stmt.execute(params![config, id])?;
        }
    }

    let anchored = updates.len() as u64;
    if anchored > 0 {
        tracing::info!(
            target: "quilltap::boot",
            scanned = candidates.len(),
            anchored,
            "Anchored fictional story clocks to real time",
        );
    }
    Ok(anchored)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_782_995_696_789; // 2026-07-02T12:34:56.789Z

    fn db_with(rows: &[(&str, Option<&str>, Option<&str>)]) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            r#"CREATE TABLE "chats" ("id" TEXT PRIMARY KEY, "createdAt" TEXT,
                 "timestampConfig" TEXT)"#,
        )
        .unwrap();
        for (id, created_at, config) in rows {
            db.execute(
                r#"INSERT INTO "chats" ("id","createdAt","timestampConfig") VALUES (?,?,?)"#,
                params![id, created_at, config],
            )
            .unwrap();
        }
        db
    }

    fn config_of(db: &Connection, id: &str) -> Option<String> {
        db.query_row(
            r#"SELECT "timestampConfig" FROM "chats" WHERE "id" = ?"#,
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn anchors_only_what_needs_it() {
        let db = db_with(&[
            (
                "unanchored",
                Some("2024-03-04T05:06:07.000Z"),
                Some(r#"{"useFictionalTime":true,"fictionalBaseTimestamp":"1550-07-25T10:15"}"#),
            ),
            (
                "anchored",
                Some("2024-03-04T05:06:07.000Z"),
                Some(
                    r#"{"useFictionalTime":true,"fictionalBaseTimestamp":"1550-07-25T10:15","fictionalBaseRealTime":"2020-01-01T00:00:00.000Z"}"#,
                ),
            ),
            (
                "real-time",
                Some("2024-03-04T05:06:07.000Z"),
                Some(r#"{"useFictionalTime":false}"#),
            ),
            ("null-config", Some("2024-03-04T05:06:07.000Z"), None),
            (
                "malformed",
                Some("2024-03-04T05:06:07.000Z"),
                Some(r#"{"useFictionalTime":true,"#),
            ),
            (
                "no-createdat",
                None,
                Some(r#"{"useFictionalTime":true,"fictionalBaseTimestamp":"1550-07-25T10:15"}"#),
            ),
        ]);

        assert_eq!(anchor_fictional_clock_bases(&db, NOW).unwrap(), 2);

        // The anchor is the chat's own createdAt, not now.
        assert_eq!(
            config_of(&db, "unanchored").unwrap(),
            r#"{"useFictionalTime":true,"fictionalBaseTimestamp":"1550-07-25T10:15","fictionalBaseRealTime":"2024-03-04T05:06:07.000Z"}"#
        );
        // A row with no createdAt falls back to now.
        assert!(config_of(&db, "no-createdat")
            .unwrap()
            .contains("2026-07-02T12:34:56.789Z"));
        // Everything else is untouched, byte for byte.
        assert!(config_of(&db, "anchored")
            .unwrap()
            .contains("2020-01-01T00:00:00.000Z"));
        assert_eq!(
            config_of(&db, "real-time").unwrap(),
            r#"{"useFictionalTime":false}"#
        );
        assert_eq!(config_of(&db, "null-config"), None);
        assert_eq!(
            config_of(&db, "malformed").unwrap(),
            r#"{"useFictionalTime":true,"#
        );

        // Idempotent: a second pass finds nothing.
        assert_eq!(anchor_fictional_clock_bases(&db, NOW).unwrap(), 0);
    }

    #[test]
    fn missing_table_is_a_no_op() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(anchor_fictional_clock_bases(&db, NOW).unwrap(), 0);
    }
}
