//! The `instance_settings` key/value store (v4 `lib/instance-settings/index.ts`).
//!
//! `instance_settings` is a tiny per-instance key/value table in the **main** db
//! (`"key" TEXT PRIMARY KEY, "value" TEXT NOT NULL` — see v4's
//! `lib/startup/version-guard.ts`, which `CREATE TABLE IF NOT EXISTS`es it). The
//! port needs only the one reader the wardrobe archetype tier depends on:
//! `getGeneralMountPointId`, the id of the singleton "Quilltap General" document
//! store that houses shared wardrobe/scenario archetypes.
//!
//! v4's `readSetting` wraps the `SELECT` in a try/catch and returns `null` on any
//! error (a freshly-cloned instance may not have the table yet — the provisioning
//! migration writes the key on first boot). We reproduce that: a query error
//! (including `no such table`) resolves to `None`, never propagating.

use rusqlite::{params, Connection};

use super::DbError;

/// The `instance_settings` key that stores the Quilltap General mount-point id.
const KEY_GENERAL_MOUNT_POINT_ID: &str = "generalMountPointId";

/// v4 `KEY_MAX_CONCURRENT_JOBS` — the per-instance background-job concurrency cap.
const KEY_MAX_CONCURRENT_JOBS: &str = "maxConcurrentJobs";

/// v4 `DEFAULT_MAX_CONCURRENT_JOBS`.
const DEFAULT_MAX_CONCURRENT_JOBS: i64 = 4;

/// v4 `KEY_LAST_MAINTENANCE_SWEEP_AT` — the last daily-maintenance-pass timestamp
/// (ISO 8601), used by the host driver to short-circuit the dev-restart startup
/// tick.
const KEY_LAST_MAINTENANCE_SWEEP_AT: &str = "lastMaintenanceSweepAt";

/// The `instance_settings` key that stores the singleton "Lantern Backgrounds"
/// mount-point id (v4 `getLanternBackgroundsMountPointId`), provisioned by
/// `provision-lantern-backgrounds-mount-v1`. Houses generated story backgrounds
/// (`generated/`) + ad-hoc `generate_image` tool output (`tool/`).
const KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID: &str = "lanternBackgroundsMountPointId";

/// v4 `KEY_MEMORY_RECALL` — the per-instance Commonplace-Book recall settings.
const KEY_MEMORY_RECALL: &str = "memoryRecall";

/// v4 `readSetting(key)` — read one `instance_settings` value, or `None`.
///
/// Faithful to v4: the whole read is fallible-tolerant — a missing table or any
/// other SQLite error resolves to `None` (v4 logs a warning and returns null).
fn read_setting(main: &Connection, key: &str) -> Option<String> {
    main.query_row(
        "SELECT \"value\" FROM \"instance_settings\" WHERE \"key\" = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// v4 `writeSetting(key, value)` — upsert one `instance_settings` value.
fn write_setting(main: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    main.execute(
        "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2) \
         ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
        params![key, value],
    )?;
    Ok(())
}

/// v4 `getGeneralMountPointId()` — the Quilltap General mount-point id, or `None`
/// when the General store has not been provisioned (or the table is absent).
pub fn get_general_mount_point_id(main: &Connection) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_GENERAL_MOUNT_POINT_ID))
}

/// v4 `getLanternBackgroundsMountPointId()` — the Lantern Backgrounds mount-point
/// id, or `None` when the store has not been provisioned (or the table is absent).
pub fn get_lantern_backgrounds_mount_point_id(
    main: &Connection,
) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID))
}

/// v4 `getMemoryRecallSettings()` — the per-instance Commonplace-Book recall
/// settings, as `(scope_policy, expand_related)`. Returns the documented default
/// (`down-weight`, `false`) when the setting is unwritten OR fails to parse (v4's
/// Zod `safeParse` → `catch` → default). The Zod schema has `scopePolicy` enum
/// `['down-weight','exclude']` (`.default('down-weight')`) and `expandRelated`
/// boolean (`.default(false)`); an out-of-enum / non-bool value fails the parse
/// (both `.default`-carrying keys mean a bad *value* still fails, not defaults —
/// faithful to `.parse` throwing on a present-but-wrong value).
pub fn get_memory_recall_settings(main: &Connection) -> Result<(String, bool), DbError> {
    const DEFAULT: (&str, bool) = ("down-weight", false);
    let Some(raw) = read_setting(main, KEY_MEMORY_RECALL) else {
        return Ok((DEFAULT.0.to_string(), DEFAULT.1));
    };
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&raw);
    let Ok(obj) = parsed else {
        return Ok((DEFAULT.0.to_string(), DEFAULT.1));
    };
    // scopePolicy: enum, `.default('down-weight')` (absent → default; present but
    // out-of-enum → parse fails → whole-object default).
    let scope_policy = match obj.get("scopePolicy") {
        None => DEFAULT.0.to_string(),
        Some(serde_json::Value::String(s)) if s == "down-weight" || s == "exclude" => s.clone(),
        Some(_) => return Ok((DEFAULT.0.to_string(), DEFAULT.1)),
    };
    // expandRelated: boolean, `.default(false)`.
    let expand_related = match obj.get("expandRelated") {
        None => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(_) => return Ok((DEFAULT.0.to_string(), DEFAULT.1)),
    };
    Ok((scope_policy, expand_related))
}

/// v4 `getMaxConcurrentJobs()` — the per-instance background-job concurrency cap.
/// Returns the documented default (4) when unset OR when the stored value is not a
/// finite integer `>= 1` (v4: `Math.floor(Number(raw))`, reject `!Number.isFinite`
/// or `< 1`); otherwise the parsed value clamped to `[1, 32]`. A missing table
/// reads as `None` → default, matching v4's `readSetting` try/catch.
pub fn get_max_concurrent_jobs(main: &Connection) -> Result<i64, DbError> {
    let Some(raw) = read_setting(main, KEY_MAX_CONCURRENT_JOBS) else {
        return Ok(DEFAULT_MAX_CONCURRENT_JOBS);
    };
    // v4: `Math.floor(Number(raw))`; `Number("")`/non-numeric → NaN → default.
    let parsed = raw.trim().parse::<f64>();
    match parsed {
        Ok(n) if n.is_finite() && n >= 1.0 => Ok((n.floor() as i64).clamp(1, 32)),
        _ => Ok(DEFAULT_MAX_CONCURRENT_JOBS),
    }
}

/// v4 `getLastMaintenanceSweepAt()` — the last daily-maintenance-pass instant as
/// Unix milliseconds, or `None` when never recorded / unparseable (v4 returns a
/// `Date` or null; the host driver only needs the instant for the recent-run
/// window). Parsed via [`crate::clock::iso_to_ms`] (the `Date.parse` inverse).
pub fn get_last_maintenance_sweep_at(main: &Connection) -> Result<Option<i64>, DbError> {
    Ok(read_setting(main, KEY_LAST_MAINTENANCE_SWEEP_AT)
        .as_deref()
        .and_then(crate::clock::iso_to_ms))
}

/// v4 `setLastMaintenanceSweepAt(when)` — record the timestamp of a completed
/// maintenance pass (ISO 8601). Defaults to now. Written at the end of a pass
/// regardless of per-sweep failures ("last attempted pass").
pub fn set_last_maintenance_sweep_at(main: &Connection, when_iso: &str) -> Result<(), DbError> {
    write_setting(main, KEY_LAST_MAINTENANCE_SWEEP_AT, when_iso)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn missing_table_yields_none() {
        // v4's readSetting try/catch returns null when the table doesn't exist.
        let c = conn();
        assert_eq!(get_general_mount_point_id(&c).unwrap(), None);
    }

    #[test]
    fn absent_key_yields_none() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
        )
        .unwrap();
        assert_eq!(get_general_mount_point_id(&c).unwrap(), None);
    }

    #[test]
    fn present_key_returns_value() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);\
             INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('generalMountPointId', 'mp-general-1');",
        )
        .unwrap();
        assert_eq!(
            get_general_mount_point_id(&c).unwrap(),
            Some("mp-general-1".to_string())
        );
    }

    fn table() -> Connection {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    #[test]
    fn max_concurrent_jobs_default_and_clamp() {
        // Missing table → default (v4 readSetting try/catch → null → default).
        assert_eq!(get_max_concurrent_jobs(&conn()).unwrap(), 4);
        // Unset key → default.
        let c = table();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 4);
        // In-range passes through.
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "8").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 8);
        // Above 32 → clamp to 32.
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "100").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 32);
        // < 1 → default (v4 rejects `< 1` before the clamp).
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "0").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 4);
        // Non-numeric → default.
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "banana").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 4);
        // Floored (v4 Math.floor).
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "5.9").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 5);
    }

    #[test]
    fn maintenance_sweep_at_roundtrip() {
        let c = table();
        assert_eq!(get_last_maintenance_sweep_at(&c).unwrap(), None);
        set_last_maintenance_sweep_at(&c, "2026-07-06T12:00:00.000Z").unwrap();
        assert_eq!(
            get_last_maintenance_sweep_at(&c).unwrap(),
            crate::clock::iso_to_ms("2026-07-06T12:00:00.000Z")
        );
    }
}

// ---------------------------------------------------------------------------
// P4.1b appends (append-only region — lane b; lane d also appends below).
// ---------------------------------------------------------------------------

/// The `instance_settings` key that stores the singleton "Quilltap Uploads"
/// mount-point id (v4 `KEY_USER_UPLOADS_MOUNT_POINT_ID`), provisioned by
/// `provision-user-uploads-mount-v1`. Home for project-less file uploads,
/// image pastes, capabilities reports, and restored project-less backup files.
const KEY_USER_UPLOADS_MOUNT_POINT_ID: &str = "userUploadsMountPointId";

/// v4 `getUserUploadsMountPointId()` — the Quilltap Uploads mount-point id, or
/// `None` when the store has not been provisioned (or the table is absent).
pub fn get_user_uploads_mount_point_id(main: &Connection) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_USER_UPLOADS_MOUNT_POINT_ID))
}

// ---------------------------------------------------------------------------
// P4.4u3 appends (lane d) — the mount-point-pointer setters. The built-in mount
// provisioner ([`crate::services::builtin_mounts`]) writes each store's id here
// after minting it (v4's `INSERT ... ON CONFLICT(key) DO UPDATE`).
// ---------------------------------------------------------------------------

/// v4 `setGeneralMountPointId` — persist the Quilltap General store's id.
pub fn set_general_mount_point_id(main: &Connection, id: &str) -> Result<(), DbError> {
    write_setting(main, KEY_GENERAL_MOUNT_POINT_ID, id)
}

/// v4 `setUserUploadsMountPointId` — persist the Quilltap Uploads store's id.
pub fn set_user_uploads_mount_point_id(main: &Connection, id: &str) -> Result<(), DbError> {
    write_setting(main, KEY_USER_UPLOADS_MOUNT_POINT_ID, id)
}

/// v4 `setLanternBackgroundsMountPointId` — persist the Lantern Backgrounds id.
pub fn set_lantern_backgrounds_mount_point_id(main: &Connection, id: &str) -> Result<(), DbError> {
    write_setting(main, KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID, id)
}

#[cfg(test)]
mod p41b_tests {
    use super::*;

    #[test]
    fn user_uploads_mount_point_id_reads() {
        let c = Connection::open_in_memory().unwrap();
        assert_eq!(get_user_uploads_mount_point_id(&c).unwrap(), None);
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);\
             INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('userUploadsMountPointId', 'mp-uploads-1');",
        )
        .unwrap();
        assert_eq!(
            get_user_uploads_mount_point_id(&c).unwrap(),
            Some("mp-uploads-1".to_string())
        );
    }
}
