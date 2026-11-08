//! The `connection_profiles.maxImagesPerRequest` boot ensure — P4.D362, the
//! TWENTY-THIRD D23 move (v4 `d439f6dc9`, migration
//! `add-profile-max-images-per-request-v1`, `dependsOn:
//! ['add-profile-supports-image-upload-field-v1']`, `introducedInVersion
//! '4.12.0'`).
//!
//! v4's why, carried (the migration's docblock): `maxImagesPerRequest`
//! (INTEGER, DEFAULT 4) is the most images a vision-capable profile is sent in
//! one request. Some models refuse a request carrying more than four images
//! outright, and a Salon turn can gather more than that (uploads, unseen
//! uploads re-sent, Lantern images since the character last spoke). Read only
//! through `resolveMaxImagesPerRequest()` — v5's
//! [`crate::services::connection_profiles::resolve_max_images_per_request`].
//! Backfill: every existing row gets 4, the documented default.
//!
//! ## The two v4 shapes
//!
//! generateDDL places `"maxImagesPerRequest" INTEGER DEFAULT 4` between
//! `supportsImageUpload` and `tags` on a FRESH instance (`fresh_schema.json`);
//! the migration APPENDS the byte-equal clause on an existing one, then runs
//! its NULL → 4 backfill (a no-op after `ADD COLUMN … DEFAULT 4`, since SQLite
//! answers the default for every existing row — measured `backfilled` 0 on
//! every real add; the statement is v4's and is kept).
//!
//! UNSTAMPED (column-gated); FATAL on failure. v4's INFO `Added
//! connection_profiles.maxImagesPerRequest column`, its ERROR, its result
//! message and its prettify label are NO-PORT (§R.5); the host logs v5's ONE
//! boot INFO on an add.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, column_exists, in_savepoint, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-profile-max-images-per-request-v1";
/// The migration's column clause, verbatim.
const MAX_IMAGES_DECL: &str = "INTEGER DEFAULT 4";
/// v4's backfill statement, copied.
const BACKFILL_SQL: &str = r#"UPDATE "connection_profiles" SET "maxImagesPerRequest" = 4 WHERE "maxImagesPerRequest" IS NULL"#;

/// What the ensure did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaxImagesEnsureOutcome {
    /// No table, or the column is already there (v4's `shouldRun` false).
    NotNeeded,
    /// The column was appended and the backfill ran over `backfilled` rows.
    Added { backfilled: usize },
}

/// `add-profile-max-images-per-request-v1`: append the column and run v4's
/// backfill, in ONE savepoint.
pub fn ensure_connection_profiles_max_images(
    main: &Connection,
) -> Result<MaxImagesEnsureOutcome, DbError> {
    if !table_exists(main, "connection_profiles")?
        || column_exists(main, "connection_profiles", "maxImagesPerRequest")?
    {
        return Ok(MaxImagesEnsureOutcome::NotNeeded);
    }
    in_savepoint(main, "ensure_connection_profiles_max_images", |c| {
        add_column_if_missing(
            c,
            "connection_profiles",
            "maxImagesPerRequest",
            MAX_IMAGES_DECL,
        )?;
        let backfilled = c.execute(BACKFILL_SQL, [])?;
        Ok(MaxImagesEnsureOutcome::Added { backfilled })
    })
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    #[test]
    fn appends_backfills_and_is_idempotent() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(
            ensure_connection_profiles_max_images(&db).unwrap(),
            MaxImagesEnsureOutcome::NotNeeded
        );
        db.execute_batch(
            r#"CREATE TABLE "connection_profiles" ("id" TEXT PRIMARY KEY, "tags" TEXT);
               INSERT INTO connection_profiles (id) VALUES ('a'), ('b');"#,
        )
        .unwrap();
        assert_eq!(
            ensure_connection_profiles_max_images(&db).unwrap(),
            MaxImagesEnsureOutcome::Added { backfilled: 0 }
        );
        assert_eq!(
            column_names(&db, "connection_profiles")
                .unwrap()
                .last()
                .map(String::as_str),
            Some("maxImagesPerRequest")
        );
        let v: i64 = db
            .query_row(
                "SELECT maxImagesPerRequest FROM connection_profiles WHERE id = 'a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, 4);
        assert_eq!(
            ensure_connection_profiles_max_images(&db).unwrap(),
            MaxImagesEnsureOutcome::NotNeeded
        );
    }
}
