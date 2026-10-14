//! The memory-maintenance settings move — P4.D288 (C1-2), v4 `e0ac7493a`'s
//! migration `move-memory-maintenance-settings-v1` (Phase A — "every memory
//! knob now lives in `instance_settings` behind one façade").
//!
//! The last knob on `chat_settings` was `autoHousekeepingSettings`; v4 copies
//! the active user's value into `instance_settings.memoryMaintenance` through
//! the legacy translator ([`legacy_housekeeping_to_maintenance`] — `enabled`,
//! the clamped `perCharacterCap`, `perCharacterCapOverrides`; the retired merge
//! keys dropped; NO `phases`), never overwriting an existing key (`ON
//! CONFLICT("key") DO NOTHING`), then removes what is dead: the
//! `memoryExtractionConcurrency` instance key and each present of
//! `chat_settings.autoHousekeepingSettings` / `memoryExtractionLimits` /
//! `memoryExtractionConcurrency` (`ALTER TABLE … DROP COLUMN`).
//!
//! [`ensure_memory_maintenance_settings_move`] follows v4's `shouldRun` (both
//! tables exist, and a dead column is present OR the dead key is) and `run`:
//! the source row is the single user's (`ffffffff-…`) — when that row exists
//! its value is used even when NULL (v4's `??` falls back only on a MISSING
//! row) — else the latest by `updatedAt`. **ATOMIC (v5):** one savepoint
//! around the copy, the key delete and the drops, where v4 runs them bare; a
//! failure leaves the instance as it was. Structure-gated, UNSTAMPED (a second
//! run meets no dead column and no dead key), FATAL on failure. v4's DEBUG /
//! INFO lines are NO-PORT; the host logs v5's ONE boot INFO when it ran.

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use super::instance_settings::{
    legacy_housekeeping_to_maintenance, DEAD_INSTANCE_SETTING_KEYS, KEY_MEMORY_MAINTENANCE,
};
use super::schema_probe::{column_exists, in_savepoint, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "move-memory-maintenance-settings-v1";

/// v4's `DEAD_COLUMNS`, in its drop order.
pub const DEAD_COLUMNS: [&str; 3] = [
    "autoHousekeepingSettings",
    "memoryExtractionLimits",
    "memoryExtractionConcurrency",
];

const SINGLE_USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

/// What one run did; the default is a no-op.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaintenanceMoveOutcome {
    /// `memoryMaintenance` was written (v4's `copied` — `changes > 0`).
    pub copied: bool,
    /// The `chat_settings` columns dropped, in drop order.
    pub dropped_columns: Vec<&'static str>,
    /// The `memoryExtractionConcurrency` instance key was deleted.
    pub dropped_instance_key: bool,
    /// v4's `shouldRun` was true (the ensure ran, whatever it found).
    pub ran: bool,
}

fn dead_columns_present(main: &Connection) -> Result<Vec<&'static str>, DbError> {
    let mut present = Vec::new();
    for column in DEAD_COLUMNS {
        if column_exists(main, "chat_settings", column)? {
            present.push(column);
        }
    }
    Ok(present)
}

/// A SQLite cell as the JS value better-sqlite3 hands v4.
fn cell_value(cell: ValueRef<'_>) -> Value {
    match cell {
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(r) => serde_json::Number::from_f64(r).map_or(Value::Null, Value::Number),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
    }
}

/// The source row's `autoHousekeepingSettings` (v4's `.get(SINGLE_USER_ID) ??
/// …ORDER BY "updatedAt" DESC LIMIT 1`); `None` — no row at all.
fn source_value(main: &Connection) -> Result<Option<Value>, DbError> {
    let read = |sql: &str, user: Option<&str>| -> Result<Option<Value>, DbError> {
        let mut stmt = main.prepare(sql)?;
        let row = match user {
            Some(u) => stmt.query_row(params![u], |r| Ok(cell_value(r.get_ref(0)?))),
            None => stmt.query_row([], |r| Ok(cell_value(r.get_ref(0)?))),
        };
        Ok(row.optional()?)
    };
    if let Some(v) = read(
        r#"SELECT "autoHousekeepingSettings" AS value FROM "chat_settings" WHERE "userId" = ?"#,
        Some(SINGLE_USER_ID),
    )? {
        return Ok(Some(v));
    }
    read(
        r#"SELECT "autoHousekeepingSettings" AS value FROM "chat_settings" ORDER BY "updatedAt" DESC LIMIT 1"#,
        None,
    )
}

/// Run v4's `move-memory-maintenance-settings-v1` when its `shouldRun` would.
pub fn ensure_memory_maintenance_settings_move(
    main: &Connection,
) -> Result<MaintenanceMoveOutcome, DbError> {
    let mut outcome = MaintenanceMoveOutcome::default();
    if !table_exists(main, "instance_settings")? || !table_exists(main, "chat_settings")? {
        return Ok(outcome);
    }
    let dead_key_present = |conn: &Connection| -> Result<bool, DbError> {
        let mut stmt = conn.prepare(r#"SELECT 1 FROM "instance_settings" WHERE "key" = ?"#)?;
        Ok(stmt.exists([DEAD_INSTANCE_SETTING_KEYS[0]])?)
    };
    if dead_columns_present(main)?.is_empty() && !dead_key_present(main)? {
        return Ok(outcome);
    }
    in_savepoint(main, "memory_maintenance_settings_move", |conn| {
        outcome.ran = true;
        if column_exists(conn, "chat_settings", "autoHousekeepingSettings")? {
            let maintenance = source_value(conn)?
                .as_ref()
                .and_then(legacy_housekeeping_to_maintenance);
            if let Some(maintenance) = maintenance {
                let text = serde_json::to_string(&maintenance)
                    .map_err(|e| DbError::Internal(e.to_string()))?;
                let changes = conn.execute(
                    r#"INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?)
               ON CONFLICT("key") DO NOTHING"#,
                    params![KEY_MEMORY_MAINTENANCE, text],
                )?;
                outcome.copied = changes > 0;
            }
        }
        outcome.dropped_instance_key = conn.execute(
            r#"DELETE FROM "instance_settings" WHERE "key" = ?"#,
            params![DEAD_INSTANCE_SETTING_KEYS[0]],
        )? > 0;
        for column in dead_columns_present(conn)? {
            conn.execute_batch(&format!(
                r#"ALTER TABLE "chat_settings" DROP COLUMN "{column}""#
            ))?;
            outcome.dropped_columns.push(column);
        }
        Ok(())
    })?;
    Ok(outcome)
}
