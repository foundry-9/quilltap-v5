//! The `chats` scenario-source boot ensure — P4.D288 (C1-2), v4 `545e299bb`'s
//! migration `add-chat-scenario-source-v1` ("where each chat's scene was
//! found"; `dependsOn: ['add-chat-scenario-text-field-v1']`).
//!
//! v4 adds `chats.scenarioSource` (JSON `{mountPointId, path, bodySha256}`) and
//! `chats.scenarioNotes`, both `TEXT DEFAULT NULL` — the MIGRATION's text,
//! APPENDED (generateDDL writes bare `TEXT` after `scenarioText`). No backfill:
//! a null source means the in-chat control infers the preset by body, as it
//! always has. [`ensure_chats_scenario_source`] follows v4's `shouldRun`
//! (`chats` exists and either column is absent) and `run` (each column through
//! `addColumnIfMissing`, source first).
//!
//! Structure-gated, UNSTAMPED (a second run finds both columns present), FATAL
//! on failure (the caller `?`s it). v4's migration lines are NO-PORT; the host
//! logs v5's ONE boot INFO when a column was added.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, column_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-chat-scenario-source-v1";

/// The two columns in v4's add order, with the MIGRATION's DDL text.
pub const COLUMNS: [(&str, &str); 2] = [
    ("scenarioSource", "TEXT DEFAULT NULL"),
    ("scenarioNotes", "TEXT DEFAULT NULL"),
];

/// Add the two `chats` columns when v4's `shouldRun` would; answers the
/// columns added, in add order (empty — a no-op).
pub fn ensure_chats_scenario_source(main: &Connection) -> Result<Vec<&'static str>, DbError> {
    let mut added = Vec::new();
    if !table_exists(main, "chats")? {
        return Ok(added);
    }
    let mut should_run = false;
    for (name, _) in COLUMNS {
        should_run |= !column_exists(main, "chats", name)?;
    }
    if !should_run {
        return Ok(added);
    }
    for (name, decl) in COLUMNS {
        if add_column_if_missing(main, "chats", name, decl)? {
            added.push(name);
        }
    }
    Ok(added)
}
