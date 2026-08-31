//! The `connection_profiles` fallback-chain columns boot ensure (v4
//! `65f5021c8` — migration `add-profile-fallback-fields-v1`).
//!
//! v4 adds `fallbackProfileId` + `allowTierFallback` through its migration
//! runner. v5's migration runner is a locked deferral, so — following the
//! P4.d7 / P4.D41 / P4.D63 / P4.D73 / P4.D77 / P4.D79 precedents — the column
//! add is re-homed as a **boot repair pass over the main partition**.
//!
//! ## The two v4 shapes AGREE here, unusually
//!
//! The standing generateDDL-vs-migration disagreement (P4.D77, P4.D79) does
//! not bite this pair. Measured at the `65f5021c8` re-dump:
//!
//!   - **generateDDL** emits `"fallbackProfileId" TEXT` and
//!     `"allowTierFallback" INTEGER DEFAULT 0`;
//!   - **the migration** emits `ADD COLUMN "fallbackProfileId" TEXT` and
//!     `ADD COLUMN "allowTierFallback" INTEGER DEFAULT 0`.
//!
//! Same declarations, same defaults. What the two shapes still disagree on is
//! column ORDER — generateDDL places the pair after `modelClass` (the Zod
//! declaration order), the migration appends them at the end of the table —
//! and v5 carries both, exactly as v4 does: `fresh_schema.json` holds the
//! generateDDL order for a fresh instance, this ensure appends for an existing
//! one, and the reads are column-name-addressed either way
//! (`db::connection_profiles::cp_select_columns`).
//!
//! ## The backfill
//!
//! v4's migration runs one UPDATE after the ALTERs —
//! `SET "allowTierFallback" = 0 WHERE "allowTierFallback" IS NULL` — a
//! belt-and-braces pass for "any row the column default may have missed". It
//! is reproduced verbatim. Unlike the prefill repair's backfill it cannot
//! clobber a user's choice even if it re-ran (it only touches NULLs, and the
//! ALTER's DEFAULT already made those impossible), but the guard is still at
//! the COLUMN level for the same reason: that is what v4's `shouldRun()` is.
//!
//! v4's `shouldRun()` is `!hasFallbackProfileId || !hasAllowTierFallback`, and
//! its `run()` re-checks each column separately — so a half-migrated table
//! (one column added, the process killed before the second) heals on the next
//! boot. Reproduced: each ALTER has its own guard.
//!
//! v4's migration pretty label ("Teaching each connection the name of its
//! understudy", `lib/startup/prettify.ts`) has no v5 analog — v5 surfaces no
//! migration labels anywhere. Recorded NO-PORT, the P4.D63 / P4.D73 / P4.D79
//! precedent.
//!
//! Idempotent; one PRAGMA on the happy path.

use rusqlite::Connection;

use super::DbError;

/// The understudy column, with v4's migration DDL verbatim.
const FALLBACK_PROFILE_ID_COLUMN: &str = "fallbackProfileId";
const FALLBACK_PROFILE_ID_DECL: &str = "TEXT";
/// The tier-pick opt-in, with v4's migration DDL verbatim.
const ALLOW_TIER_FALLBACK_COLUMN: &str = "allowTierFallback";
const ALLOW_TIER_FALLBACK_DECL: &str = "INTEGER DEFAULT 0";

/// Add the two `connection_profiles` fallback-chain columns when either is
/// absent, then run v4's NULL backfill.
///
/// A no-op when the table is absent (a partition that has never held profiles)
/// or both columns already exist.
pub fn ensure_connection_profiles_fallback_columns(main: &Connection) -> Result<(), DbError> {
    if !table_exists(main, "connection_profiles")? {
        return Ok(());
    }
    let columns = column_names(main, "connection_profiles")?;
    let has_fallback_profile_id = columns.iter().any(|c| c == FALLBACK_PROFILE_ID_COLUMN);
    let has_allow_tier_fallback = columns.iter().any(|c| c == ALLOW_TIER_FALLBACK_COLUMN);
    if has_fallback_profile_id && has_allow_tier_fallback {
        return Ok(());
    }

    if !has_fallback_profile_id {
        main.execute_batch(&format!(
            "ALTER TABLE \"connection_profiles\" ADD COLUMN \
             \"{FALLBACK_PROFILE_ID_COLUMN}\" {FALLBACK_PROFILE_ID_DECL}"
        ))?;
    }
    if !has_allow_tier_fallback {
        main.execute_batch(&format!(
            "ALTER TABLE \"connection_profiles\" ADD COLUMN \
             \"{ALLOW_TIER_FALLBACK_COLUMN}\" {ALLOW_TIER_FALLBACK_DECL}"
        ))?;
    }

    // v4's backfill: any row the column default may have missed.
    main.execute(
        "UPDATE \"connection_profiles\" SET \"allowTierFallback\" = 0 \
         WHERE \"allowTierFallback\" IS NULL",
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

    /// A pre-4.10 `connection_profiles` table: no fallback columns.
    fn legacy_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE \"connection_profiles\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \"provider\" TEXT, \
               \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .unwrap();
    }

    fn insert(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO connection_profiles (id, userId, provider, createdAt, updatedAt) \
             VALUES (?1, 'u1', 'OPENAI', 't', 't')",
            [id],
        )
        .unwrap();
    }

    fn columns(conn: &Connection) -> Vec<String> {
        column_names(conn, "connection_profiles").unwrap()
    }

    #[test]
    fn a_legacy_table_gains_both_columns_and_the_backfill() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "p1");

        ensure_connection_profiles_fallback_columns(&conn).unwrap();

        let cols = columns(&conn);
        assert!(cols.iter().any(|c| c == FALLBACK_PROFILE_ID_COLUMN));
        assert!(cols.iter().any(|c| c == ALLOW_TIER_FALLBACK_COLUMN));
        // v4 appends, so the pair lands at the END — NOT in the generateDDL slot.
        assert_eq!(
            &cols[cols.len() - 2..],
            &[
                FALLBACK_PROFILE_ID_COLUMN.to_string(),
                ALLOW_TIER_FALLBACK_COLUMN.to_string()
            ]
        );

        let (fallback, tier): (Option<String>, i64) = conn
            .query_row(
                "SELECT fallbackProfileId, allowTierFallback FROM connection_profiles \
                 WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fallback, None, "no understudy is the pre-column behaviour");
        assert_eq!(
            tier, 0,
            "the tier pick is opt-in, never turned on by an upgrade"
        );
    }

    #[test]
    fn a_half_migrated_table_heals_the_missing_column_only() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        conn.execute_batch(
            "ALTER TABLE \"connection_profiles\" ADD COLUMN \"fallbackProfileId\" TEXT",
        )
        .unwrap();
        insert(&conn, "p1");
        conn.execute(
            "UPDATE connection_profiles SET fallbackProfileId = 'p-understudy'",
            [],
        )
        .unwrap();

        ensure_connection_profiles_fallback_columns(&conn).unwrap();

        let (fallback, tier): (Option<String>, i64) = conn
            .query_row(
                "SELECT fallbackProfileId, allowTierFallback FROM connection_profiles \
                 WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            fallback.as_deref(),
            Some("p-understudy"),
            "the already-present column is left alone"
        );
        assert_eq!(tier, 0);
    }

    #[test]
    fn a_second_boot_never_reclobbers_an_explicit_choice() {
        let conn = Connection::open_in_memory().unwrap();
        legacy_table(&conn);
        insert(&conn, "p1");
        ensure_connection_profiles_fallback_columns(&conn).unwrap();

        conn.execute(
            "UPDATE connection_profiles SET allowTierFallback = 1, \
             fallbackProfileId = 'p-understudy' WHERE id = 'p1'",
            [],
        )
        .unwrap();

        ensure_connection_profiles_fallback_columns(&conn).unwrap();

        let (fallback, tier): (Option<String>, i64) = conn
            .query_row(
                "SELECT fallbackProfileId, allowTierFallback FROM connection_profiles \
                 WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fallback.as_deref(), Some("p-understudy"));
        assert_eq!(tier, 1, "an opted-in profile survives the next boot");
    }

    #[test]
    fn the_generatedll_shape_is_recognised_and_left_alone() {
        let conn = Connection::open_in_memory().unwrap();
        // A FRESH instance: the pair sits mid-table, in the Zod slot.
        conn.execute_batch(
            "CREATE TABLE \"connection_profiles\" (\
               \"id\" TEXT PRIMARY KEY, \"modelClass\" TEXT, \
               \"fallbackProfileId\" TEXT, \"allowTierFallback\" INTEGER DEFAULT 0, \
               \"maxContext\" REAL);",
        )
        .unwrap();

        ensure_connection_profiles_fallback_columns(&conn).unwrap();

        assert_eq!(
            columns(&conn),
            vec![
                "id",
                "modelClass",
                "fallbackProfileId",
                "allowTierFallback",
                "maxContext"
            ],
            "a fresh instance is untouched — no appended duplicates"
        );
    }

    #[test]
    fn a_partition_without_the_table_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_connection_profiles_fallback_columns(&conn).unwrap();
    }
}
