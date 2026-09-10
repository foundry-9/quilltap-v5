//! The shared `migrations_state` / `migrations_metadata` ledger tables (v4
//! `migrations/state.ts`'s `ensureSQLiteMigrationsTable`).
//!
//! Four boot heals stamp v4's ledger so a v4 boot on the same instance does not
//! re-run work v5 already did (and vice versa — a row from EITHER app is
//! honoured). Every one of them needs the two tables to exist before it writes,
//! and until P4.88 every one carried its own copy of the DDL under its own
//! `if !table_exists(main, "migrations_state")` guard — v4's shape, transcribed
//! four times.
//!
//! **The hardening (P4.88, the fragility P4.D175's boot-wiring test surfaced).**
//! That guard reads ONE table and then the caller upserts into the OTHER, so a
//! partition carrying `migrations_state` WITHOUT `migrations_metadata` fails the
//! whole BOOT with `no such table: migrations_metadata` — not just the pass.
//! v4's own `ensureSQLiteMigrationsTable` has the identical guard, so the shape
//! is v4-faithful and unreachable FROM v4 (v4 creates both together or neither);
//! it is reachable from a hand-built partition, a partial restore, or a fixture.
//! Dropping the guard and letting `CREATE TABLE IF NOT EXISTS` do its own job
//! costs nothing, changes nothing on any partition v4 can produce, and cannot
//! fail that way. The DDL text is byte-identical to what the four heals carried.

use rusqlite::Connection;

use super::DbError;

/// v4 `migrations/state.ts:44` — create both ledger tables if they are missing.
///
/// Idempotent and unguarded: `CREATE TABLE IF NOT EXISTS` is the guard. Call it
/// before any `migrations_state` INSERT or `migrations_metadata` upsert.
pub(crate) fn ensure_migrations_tables(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(LEDGER_DDL)?;
    Ok(())
}

/// The two `CREATE TABLE IF NOT EXISTS` statements, verbatim from v4's
/// `ensureSQLiteMigrationsTable` (and from the four copies this replaced).
const LEDGER_DDL: &str = "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n        \"id\" TEXT PRIMARY KEY,\n        \"completedAt\" TEXT NOT NULL,\n        \"quilltapVersion\" TEXT NOT NULL,\n        \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n        \"message\" TEXT\n      );\n      CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n        \"key\" TEXT PRIMARY KEY,\n        \"value\" TEXT NOT NULL\n      );";

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap).collect()
    }

    #[test]
    fn both_tables_land_on_a_bare_partition() {
        let db = Connection::open_in_memory().unwrap();
        ensure_migrations_tables(&db).unwrap();
        assert_eq!(
            tables(&db),
            vec![
                "migrations_metadata".to_string(),
                "migrations_state".to_string()
            ]
        );
    }

    /// The shape the guard could not heal: `migrations_state` alone. This is what
    /// used to fail the BOOT with `no such table: migrations_metadata`.
    #[test]
    fn the_missing_sibling_is_created_when_only_state_exists() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE \"migrations_state\" (\"id\" TEXT PRIMARY KEY);")
            .unwrap();
        ensure_migrations_tables(&db).unwrap();
        assert!(tables(&db).contains(&"migrations_metadata".to_string()));
    }

    /// …and the mirror image.
    #[test]
    fn the_missing_sibling_is_created_when_only_metadata_exists() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE \"migrations_metadata\" (\"key\" TEXT PRIMARY KEY);")
            .unwrap();
        ensure_migrations_tables(&db).unwrap();
        assert!(tables(&db).contains(&"migrations_state".to_string()));
    }

    /// The Friday shape: both already present, from v4's own migration runner.
    /// Nothing is created and nothing is dropped — an existing table keeps its
    /// own columns and its own rows (§R.12: the hardening must stay a no-op on a
    /// partition where v4 already created both).
    #[test]
    fn a_partition_carrying_both_is_untouched() {
        let db = Connection::open_in_memory().unwrap();
        ensure_migrations_tables(&db).unwrap();
        db.execute(
            "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message) \
             VALUES ('v4-written', '2026-01-01T00:00:00.000Z', '4.9.0', 3, 'from v4')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO \"migrations_metadata\" (key, value) VALUES ('lastChecked', '2026-01-01T00:00:00.000Z')",
            [],
        )
        .unwrap();
        ensure_migrations_tables(&db).unwrap();
        let rows: i64 = db
            .query_row("SELECT COUNT(*) FROM \"migrations_state\"", [], |r| {
                r.get(0)
            })
            .unwrap();
        let meta: String = db
            .query_row(
                "SELECT value FROM \"migrations_metadata\" WHERE key = 'lastChecked'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1, "the v4-written row survives");
        assert_eq!(meta, "2026-01-01T00:00:00.000Z");
    }
}
