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

/// v4 `getGeneralMountPointId()` — the Quilltap General mount-point id, or `None`
/// when the General store has not been provisioned (or the table is absent).
pub fn get_general_mount_point_id(main: &Connection) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_GENERAL_MOUNT_POINT_ID))
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
}
