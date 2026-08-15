//! The `connection_profiles.multiCharacterPrefill` column boot ensure (v4
//! `23af7146` — migration `add-profile-multi-character-prefill-field-v1`).
//!
//! v4 adds the column through its migration runner. v5's migration runner is a
//! locked deferral, so — following the P4.d7 (NOCASE namespace), P4.D41
//! (`linkGroupId`), P4.D63 (`characters` archive columns), P4.D73
//! (`chat_settings` composer columns) and P4.D77 (`help_doc_chunks`)
//! precedents — the column add is re-homed as a **boot repair pass over the
//! main partition**.
//!
//! ## The two v4 shapes are NOT the same here
//!
//! Measured at the `aa464abf` re-dump (the P4.D77 lesson repeating):
//!
//!   - **generateDDL** (`ConnectionProfileSchema` →
//!     `z.boolean().nullable().optional()`, no `.default()`) emits
//!     `"multiCharacterPrefill" INTEGER` — **no DEFAULT clause**. That is the
//!     shape a fresh instance gets, from the D23 re-dump. It is harmless there
//!     because v4's create route always resolves and STORES a boolean, so a
//!     fresh instance never leans on a column default.
//!   - **The migration** (and `sqlite-initial-schema.ts`) emits
//!     `INTEGER DEFAULT 1`, then backfills. That is the shape an EXISTING
//!     instance gets, and it is what this ensure reproduces — exactly as v4's
//!     own migration would, so an instance v5 repairs and v4 later opens sees
//!     the same defaults for rows written before the add.
//!
//! ## ⚠ The backfill runs ONCE, when the column is added
//!
//! v4's migration `shouldRun()` is "the column is absent", so its two UPDATEs
//! fire exactly once in an instance's life. A boot ensure that re-ran the
//! Anthropic UPDATE every boot would clobber a user's explicit `true` on an
//! Anthropic profile — which the profile editor deliberately permits (ticking
//! it on Anthropic is allowed and merely warned about). So the guard here is
//! all-or-nothing at the COLUMN level, not per-statement: column absent → add
//! + backfill; column present → no-op, whatever the rows hold.
//!
//! The column is tri-state on purpose (NULL = "never chosen"), so a NULL
//! surviving here is not damage — [`crate::services::multi_character_prefill`]
//! resolves it to the provider default. The backfill exists only to preserve
//! the pre-migration BEHAVIOUR byte-for-byte at the moment of the add.
//!
//! v4's migration pretty label ("Deciding who is announced at the door") has no
//! v5 analog — v5 surfaces no migration labels anywhere. Recorded NO-PORT, the
//! P4.D63 / P4.D73 precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The column, with v4's migration DDL verbatim (`ALTER TABLE
/// "connection_profiles" ADD COLUMN "multiCharacterPrefill" INTEGER DEFAULT 1`).
const PREFILL_COLUMN: &str = "multiCharacterPrefill";
const PREFILL_COLUMN_DECL: &str = "INTEGER DEFAULT 1";

/// Add `connection_profiles.multiCharacterPrefill` and run v4's backfill when
/// the column is absent.
///
/// A no-op when the table is absent (a partition that has never held profiles)
/// or the column already exists — see the module header on why the guard is at
/// the column level and not per-statement.
pub fn ensure_connection_profiles_prefill_column(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "connection_profiles")? {
        return Ok(());
    }
    if column_names(main, "connection_profiles")?
        .iter()
        .any(|c| c == PREFILL_COLUMN)
    {
        return Ok(());
    }

    main.execute_batch(&format!(
        "ALTER TABLE \"connection_profiles\" ADD COLUMN \"{PREFILL_COLUMN}\" {PREFILL_COLUMN_DECL}"
    ))?;

    // v4's first UPDATE: preserve the pre-migration behaviour exactly — Anthropic
    // was the sole provider that never received the prefill, because it 400s on a
    // request that ends with an assistant message.
    main.execute(
        "UPDATE \"connection_profiles\" SET \"multiCharacterPrefill\" = 0 \
         WHERE upper(\"provider\") = 'ANTHROPIC'",
        [],
    )?;
    // v4's second UPDATE: backfill any rows the column default may have missed.
    main.execute(
        "UPDATE \"connection_profiles\" SET \"multiCharacterPrefill\" = 1 \
         WHERE \"multiCharacterPrefill\" IS NULL",
        [],
    )?;

    Ok(())
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

    /// The pre-4.9 shape: a `connection_profiles` table without the column.
    fn old_schema_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"connection_profiles\" (\"id\" TEXT PRIMARY KEY, \
             \"userId\" TEXT NOT NULL, \"provider\" TEXT NOT NULL, \
             \"pseudoToolMode\" TEXT DEFAULT 'auto', \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .expect("ddl");
        db
    }

    fn seed(db: &Connection, id: &str, provider: &str) {
        db.execute(
            "INSERT INTO connection_profiles (id, userId, provider, createdAt, updatedAt) \
             VALUES (?1, 'u1', ?2, '2026-08-15T00:00:00.000Z', '2026-08-15T00:00:00.000Z')",
            rusqlite::params![id, provider],
        )
        .expect("seed row");
    }

    fn prefill(db: &Connection, id: &str) -> Option<i64> {
        db.query_row(
            "SELECT multiCharacterPrefill FROM connection_profiles WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, Option<i64>>(0),
        )
        .expect("read back")
    }

    fn has_column(db: &Connection) -> bool {
        column_names(db, "connection_profiles")
            .expect("pragma")
            .iter()
            .any(|c| c == PREFILL_COLUMN)
    }

    /// v4's backfill, reproduced: Anthropic off, everything else on — and the
    /// `upper()` comparison is case-insensitive on the stored provider, as v4's
    /// `upper("provider") = 'ANTHROPIC'` is.
    #[test]
    fn adds_the_column_and_backfills_v4s_split() {
        let db = old_schema_db();
        seed(&db, "p-anthropic", "ANTHROPIC");
        seed(&db, "p-anthropic-lower", "anthropic");
        seed(&db, "p-openai", "OPENAI");
        seed(&db, "p-ollama", "OLLAMA");
        assert!(!has_column(&db), "column absent before the ensure");

        ensure_connection_profiles_prefill_column(&db).expect("ensure");

        assert!(has_column(&db));
        assert_eq!(prefill(&db, "p-anthropic"), Some(0));
        assert_eq!(prefill(&db, "p-anthropic-lower"), Some(0));
        assert_eq!(prefill(&db, "p-openai"), Some(1));
        assert_eq!(prefill(&db, "p-ollama"), Some(1));
    }

    /// The DEFAULT clause is the migration's own: a row inserted after the add
    /// without naming the column reads back 1, not NULL.
    #[test]
    fn the_default_clause_is_v4s() {
        let db = old_schema_db();
        ensure_connection_profiles_prefill_column(&db).expect("ensure");
        seed(&db, "p-new", "OPENAI");
        assert_eq!(prefill(&db, "p-new"), Some(1));
    }

    /// ⚠ The property the whole column-level guard exists for: a user who
    /// deliberately ticked the prefill ON for an Anthropic profile must not have
    /// it clobbered back to 0 by the next boot. (A per-statement guard, or an
    /// unconditional re-run of the backfill, would fail this.)
    #[test]
    fn a_second_boot_never_reclobbers_an_explicit_choice() {
        let db = old_schema_db();
        seed(&db, "p-anthropic", "ANTHROPIC");
        seed(&db, "p-openai", "OPENAI");

        ensure_connection_profiles_prefill_column(&db).expect("first boot");
        assert_eq!(prefill(&db, "p-anthropic"), Some(0));

        // The user ticks it on for Anthropic (permitted — warned about, not
        // forbidden) and off for the OpenAI profile.
        db.execute(
            "UPDATE connection_profiles SET multiCharacterPrefill = 1 WHERE id = 'p-anthropic'",
            [],
        )
        .expect("user choice");
        db.execute(
            "UPDATE connection_profiles SET multiCharacterPrefill = 0 WHERE id = 'p-openai'",
            [],
        )
        .expect("user choice");

        ensure_connection_profiles_prefill_column(&db).expect("second boot");

        assert_eq!(
            prefill(&db, "p-anthropic"),
            Some(1),
            "an explicit true on an Anthropic profile survives the next boot"
        );
        assert_eq!(
            prefill(&db, "p-openai"),
            Some(0),
            "an explicit false on a non-Anthropic profile survives the next boot"
        );
    }

    /// A NULL that arrives AFTER the add (an import from a pre-4.9 bundle) stays
    /// NULL — the column is tri-state on purpose and the resolver, not the
    /// ensure, decides what it means.
    #[test]
    fn a_null_written_after_the_add_is_left_alone() {
        let db = old_schema_db();
        ensure_connection_profiles_prefill_column(&db).expect("first boot");
        db.execute(
            "INSERT INTO connection_profiles \
               (id, userId, provider, multiCharacterPrefill, createdAt, updatedAt) \
             VALUES ('p-imported', 'u1', 'ANTHROPIC', NULL, '2026-08-15T00:00:00.000Z', \
                     '2026-08-15T00:00:00.000Z')",
            [],
        )
        .expect("import a pre-4.9 profile");

        ensure_connection_profiles_prefill_column(&db).expect("second boot");

        assert_eq!(prefill(&db, "p-imported"), None);
    }

    /// A fresh instance carries the generateDDL shape (`INTEGER`, no DEFAULT)
    /// from the D23 re-dump; the ensure must recognise it as present and leave
    /// it — including the rows, which the create route has already resolved.
    #[test]
    fn the_generatedll_shape_is_recognised_and_left_alone() {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"connection_profiles\" (\"id\" TEXT PRIMARY KEY, \
             \"userId\" TEXT NOT NULL, \"provider\" TEXT NOT NULL, \
             \"multiCharacterPrefill\" INTEGER, \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .expect("ddl");
        db.execute(
            "INSERT INTO connection_profiles \
               (id, userId, provider, multiCharacterPrefill, createdAt, updatedAt) \
             VALUES ('p1', 'u1', 'ANTHROPIC', 1, '2026-08-15T00:00:00.000Z', \
                     '2026-08-15T00:00:00.000Z')",
            [],
        )
        .expect("seed");

        ensure_connection_profiles_prefill_column(&db).expect("ensure");

        assert_eq!(prefill(&db, "p1"), Some(1));
    }

    #[test]
    fn no_connection_profiles_table_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        ensure_connection_profiles_prefill_column(&db).expect("no-op on a table-less partition");
    }
}
