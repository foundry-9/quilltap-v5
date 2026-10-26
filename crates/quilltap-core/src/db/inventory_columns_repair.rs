//! The two Inventory column boot ensures — P4.D324, the THIRTEENTH +
//! FOURTEENTH D23 moves:
//!
//!   - v4 `4fbdad186` ("The Inventory — objects a character carries") —
//!     migration `add-chat-equipped-inventory-v1`, `dependsOn:
//!     ['add-equipped-outfit-field-v1']`, `introducedInVersion '4.11.0'`:
//!     `chats.equippedInventory`, JSON `{ [characterId]: string[] }` of the
//!     Inventory objects each character carries in the chat. Written only by
//!     `ChatsRepository.setEquippedInventory` (a patch-only column). No
//!     backfill: a chat that predates the Inventory carries nothing until the
//!     operator picks something up or uses *Reset to defaults* — arrival
//!     defaults apply only when a character arrives.
//!   - v4 `e1872c94f` (the Inventory tools) — migration
//!     `add-character-inventory-flag-v1`, `dependsOn:
//!     ['add-character-wardrobe-flags-v1']`: `characters.canManageInventory`,
//!     whether the character is offered the `inventory_*` tools. NULL reads as
//!     on, as `canDressThemselves` does. No backfill.
//!
//! Both are registered at `migrations/scripts/index.ts:924-926` AFTER
//! `seedChatWardrobeHistoryMigration` and BEFORE
//! `addTerminalSessionGeometryMigration` — chats first, then characters; each
//! `dependsOn` is earlier in the array, so registration order IS run order.
//! [`run_inventory_columns_ensures`] is the ONE home for that order — the
//! host's `seed_built_ins` (after the P4.D315 wardrobe-history chain, before
//! the P4.160 index-family backfill), the vintage mirror-boot and the
//! zero-rerun differential all call it, so the three cannot drift.
//!
//! ## TWO shapes per column (the P4.D249 class, NOT the P4.D304 byte-equal one)
//!
//! A FRESH instance renders generateDDL's `"equippedInventory" TEXT` (cid 66,
//! between `equippedOutfit` and `characterAvatars` — `JsonSchema.nullable().
//! optional()`, no DEFAULT) and `"canManageInventory" INTEGER` (between
//! `canCreateOutfits` and `canChooseOutfit` — `z.boolean().nullable().
//! optional()`, no DEFAULT); those are the committed `fresh_schema.json`'s
//! bytes. An EXISTING instance gets the migrations' appended clauses
//! `TEXT DEFAULT NULL` / `INTEGER DEFAULT NULL`. Both are legal: a `DEFAULT
//! NULL` is semantically no default, so a row written by an app that does not
//! bind the column reads NULL either way.
//!
//! v4's `shouldRun` (`isSQLiteBackend() ∧ sqliteTableExists(<table>) ∧
//! !sqliteColumnExists(<table>, <col>)`) is the table gate here plus
//! `add_column_if_missing`'s own column gate, whose ALTER text is v4
//! `database-utils.ts`'s `addColumnIfMissing` verbatim (`ALTER TABLE "<t>" ADD
//! COLUMN "<c>" <ddl>`).
//!
//! ## No stamp
//!
//! A `shouldRun` false is "skipped and NOT recorded" by v4's runner, re-asked
//! every boot, and nothing later drops either column — so after v5's ALTER
//! v4's runner never re-runs the migration (no ping-pong) and nothing is
//! written to `migrations_state` (the ruled `UNSTAMPED_COLUMN_ENSURES`
//! asymmetry: v4's runner stamps each id when IT adds the column; v5 never
//! does). FATAL on failure: neither migration is `resumable`, so a failure
//! stops v4's boot (the host `?`s the chain).
//!
//! ## NO-PORT
//!
//! v4's migration lines — INFO `Added equippedInventory column to chats` /
//! `Added canManageInventory column to characters` (inside the `if (added)`),
//! the two ERRORs (`Failed to add … column to …`), `database-utils`' DEBUG
//! `Adding column`, the `migrations_state` rows and `lib/startup/prettify.ts`'s
//! two labels — belong to the deferred migration runner. The chain logs v5's
//! ONE INFO per column it added (`target: "quilltap::boot"`, `migrationId`,
//! `columnAdded`) and is silent on a no-op boot.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, table_exists};
use super::DbError;

/// v4's migration id for the chat's carried-objects ledger.
pub const MIGRATION_ID_EQUIPPED_INVENTORY: &str = "add-chat-equipped-inventory-v1";
/// v4's migration id for the character's inventory-tools flag.
pub const MIGRATION_ID_INVENTORY_FLAG: &str = "add-character-inventory-flag-v1";

/// The carried-objects column, with v4's migration clause verbatim.
pub const EQUIPPED_INVENTORY_COLUMN: &str = "equippedInventory";
pub const EQUIPPED_INVENTORY_DECL: &str = "TEXT DEFAULT NULL";
/// The inventory-tools flag column, with v4's migration clause verbatim.
pub const INVENTORY_FLAG_COLUMN: &str = "canManageInventory";
pub const INVENTORY_FLAG_DECL: &str = "INTEGER DEFAULT NULL";

fn ensure_column(
    main: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<bool, DbError> {
    if !table_exists(main, table)? {
        return Ok(false);
    }
    add_column_if_missing(main, table, column, decl)
}

/// `add-chat-equipped-inventory-v1`: add `chats.equippedInventory` when the
/// table exists and the column does not. Answers whether it added.
pub fn ensure_chats_equipped_inventory(main: &Connection) -> Result<bool, DbError> {
    ensure_column(
        main,
        "chats",
        EQUIPPED_INVENTORY_COLUMN,
        EQUIPPED_INVENTORY_DECL,
    )
}

/// `add-character-inventory-flag-v1`: add `characters.canManageInventory` when
/// the table exists and the column does not. Answers whether it added.
pub fn ensure_characters_can_manage_inventory(main: &Connection) -> Result<bool, DbError> {
    ensure_column(
        main,
        "characters",
        INVENTORY_FLAG_COLUMN,
        INVENTORY_FLAG_DECL,
    )
}

/// Run both ensures in v4's registration order (chats, then characters),
/// logging v5's ONE boot INFO per column added. The first `Err` returns.
///
/// The lines are emitted here, on whatever thread runs the chain — the host
/// runs it inside `Db::write`, so a capture that must see them uses the
/// process-wide rig (memory note `capture-rig-writer-thread`).
pub fn run_inventory_columns_ensures(main: &Connection) -> Result<(), DbError> {
    if ensure_chats_equipped_inventory(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = MIGRATION_ID_EQUIPPED_INVENTORY,
            columnAdded = true,
            "Fitted each chat with a ledger of what everyone is carrying"
        );
    }
    if ensure_characters_can_manage_inventory(main)? {
        tracing::info!(
            target: "quilltap::boot",
            migrationId = MIGRATION_ID_INVENTORY_FLAG,
            columnAdded = true,
            "Granted characters leave to rummage through their own bags"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    /// The pre-Inventory shape: neither column, around the neighbours.
    fn old_schema_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE \"chats\" (\"id\" TEXT PRIMARY KEY, \"equippedOutfit\" TEXT, \
             \"characterAvatars\" TEXT, \"createdAt\" TEXT, \"updatedAt\" TEXT);\
             CREATE TABLE \"characters\" (\"id\" TEXT PRIMARY KEY, \"canCreateOutfits\" INTEGER, \
             \"canChooseOutfit\" INTEGER DEFAULT 0, \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .expect("ddl");
        db
    }

    fn sql_of(db: &Connection, table: &str) -> String {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [table],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn last_column(db: &Connection, table: &str) -> (String, String, Option<String>) {
        let mut stmt = db
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .unwrap();
        let cols: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(4)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        cols.last().unwrap().clone()
    }

    /// Both appended at the END with the migrations' clauses.
    #[test]
    fn appends_both_with_the_migration_clauses() {
        let db = old_schema_db();
        run_inventory_columns_ensures(&db).expect("chain");
        assert_eq!(
            last_column(&db, "chats"),
            (
                "equippedInventory".into(),
                "TEXT".into(),
                Some("NULL".into())
            )
        );
        assert_eq!(
            last_column(&db, "characters"),
            (
                "canManageInventory".into(),
                "INTEGER".into(),
                Some("NULL".into())
            )
        );
        let chats = sql_of(&db, "chats");
        assert!(
            chats.ends_with(", \"equippedInventory\" TEXT DEFAULT NULL)"),
            "{chats}"
        );
        let characters = sql_of(&db, "characters");
        assert!(
            characters.ends_with(", \"canManageInventory\" INTEGER DEFAULT NULL)"),
            "{characters}"
        );
    }

    /// No back-fill: a row that predates the add reads NULL for both.
    #[test]
    fn a_preexisting_row_reads_null() {
        let db = old_schema_db();
        db.execute("INSERT INTO chats (id) VALUES ('c1')", [])
            .unwrap();
        db.execute("INSERT INTO characters (id) VALUES ('k1')", [])
            .unwrap();
        run_inventory_columns_ensures(&db).expect("chain");
        let inv: Option<String> = db
            .query_row("SELECT equippedInventory FROM chats", [], |r| r.get(0))
            .unwrap();
        let flag: Option<i64> = db
            .query_row("SELECT canManageInventory FROM characters", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!((inv, flag), (None, None));
    }

    /// Half-migrated (chats present, characters absent): only the flag is
    /// added, and the answers say so; a second run adds nothing.
    #[test]
    fn half_migrated_adds_one_and_is_idempotent() {
        let db = old_schema_db();
        assert!(ensure_chats_equipped_inventory(&db).unwrap());
        assert!(!ensure_chats_equipped_inventory(&db).unwrap());
        assert!(ensure_characters_can_manage_inventory(&db).unwrap());
        assert!(!ensure_characters_can_manage_inventory(&db).unwrap());
        let before = (sql_of(&db, "chats"), sql_of(&db, "characters"));
        run_inventory_columns_ensures(&db).expect("re-run is a no-op");
        assert_eq!((sql_of(&db, "chats"), sql_of(&db, "characters")), before);
    }

    #[test]
    fn no_tables_is_a_no_op() {
        let db = Connection::open_in_memory().expect("open");
        run_inventory_columns_ensures(&db).expect("no-op");
        assert!(column_names(&db, "chats").unwrap().is_empty());
        assert!(column_names(&db, "characters").unwrap().is_empty());
    }

    /// TWO shapes: the committed generateDDL dump carries each column ONCE in
    /// its rendered form (no DEFAULT) at its Zod position, and a fresh table
    /// is a no-op for both ensures.
    #[test]
    fn the_committed_dump_carries_the_rendered_shapes() {
        let fresh: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let ddl_of = |table: &str| -> String {
            let head = format!("CREATE TABLE \"{table}\"");
            fresh["main"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|s| s.as_str().filter(|s| s.starts_with(&head)))
                .unwrap()
                .to_string()
        };
        let chats = ddl_of("chats");
        assert_eq!(
            chats
                .matches("  \"equippedOutfit\" TEXT,\n  \"equippedInventory\" TEXT,\n  \"characterAvatars\" TEXT,\n")
                .count(),
            1,
            "{chats}"
        );
        let characters = ddl_of("characters");
        assert_eq!(
            characters
                .matches("  \"canCreateOutfits\" INTEGER,\n  \"canManageInventory\" INTEGER,\n  \"canChooseOutfit\" INTEGER DEFAULT 0,\n")
                .count(),
            1,
            "{characters}"
        );
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&chats).unwrap();
        db.execute_batch(&characters).unwrap();
        let names = column_names(&db, "chats").unwrap();
        assert_eq!(names[66], EQUIPPED_INVENTORY_COLUMN);
        assert!(!ensure_chats_equipped_inventory(&db).unwrap());
        assert!(!ensure_characters_can_manage_inventory(&db).unwrap());
    }

    /// The round's ONE re-dump carries what the siblings read through
    /// `fresh_ddl_for`: `log_entries` and its three indexes (P4.D331), and
    /// `terminal_sessions`' geometry (P4.D337).
    #[test]
    fn the_redump_serves_log_entries_and_terminal_geometry() {
        use crate::db::table_shape::{fresh_ddl_for, Partition};
        let log = fresh_ddl_for(Partition::LlmLogs, "log_entries");
        assert_eq!(log.len(), 4, "{log:#?}");
        assert!(log[0].starts_with("CREATE TABLE \"log_entries\" ("));
        let terminal = fresh_ddl_for(Partition::Main, "terminal_sessions");
        assert!(terminal[0].contains("\"cols\" REAL") && terminal[0].contains("\"rows\" REAL"));
    }
}
