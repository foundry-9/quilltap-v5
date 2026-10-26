//! The store-name reconcile — v4's `lib/mount-index/reconcile-store-names.ts`
//! (`ed8b15b50`, bug 186): bring every document store's name into line with
//! the naming rules in [`crate::store_names`] — one case-insensitive
//! namespace, each live character vault named after its character, and every
//! vault no character points at retired to `"<Name> Version <timestamp>
//! Store"`.
//!
//! Runs at boot, at the end of a backup restore and a `.qtap` import, and
//! whenever a character is created, renamed or deleted. A no-op when the
//! instance already conforms: two reads and a pure plan.
//!
//! The renames are applied in ONE transaction on the mount-index database, in
//! two phases (every renamed row to a placeholder, then to its final name) so a
//! swap of names can never trip the unique name index halfway through. v5's
//! callers run inside the writer's closures with no implicit transaction of
//! their own, so the one transaction is a SAVEPOINT (the P4.D248 precedent) —
//! correct whether or not an outer transaction is open.
//!
//! ## The reads (P4.D274 R-A / R-C)
//!
//! v4 reads `docMountPoints.findAll()` and `characters.findAllRaw()`, both
//! `_findAll` (`base.repository.ts:263-277`):
//!
//! - **A bad ROW is dropped, as v4 drops it** — the two lines (ERROR `Data
//!   validation failed`, WARN `Safe validation failed`, each `{collection,
//!   error}`) and the rest answered. Faithful by necessity: a row v4 drops but
//!   v5 keeps would be planned by v5 alone, which IS the ping-pong on a shared
//!   instance. The store predicate is v4's `DocMountPointSchema` WHOLE; the
//!   character predicate is the plan's subset of `CharacterSchema` (`id`,
//!   `userId`, `name`, `characterDocumentMountPointId`, `createdAt`,
//!   `updatedAt`) plus the BLOB-cell class on every other column — the rest of
//!   `CharacterSchema` is the recorded divergence
//!   `RECONCILE_CHARACTER_ROW_SUBSET` (measured equal drop counts on the
//!   Friday copy and every committed fixture pair this lane boots).
//! - **A WHOLE-QUERY failure stays STRICT** (`RECONCILE_READ_FAILURE_STRICT`,
//!   the 2026-08-03 write-path-reads-strict rule). v4's failed `findAllRaw()`
//!   answers `[]`, so its plan sees every vault as unlinked and RETIRES every
//!   vault-shaped `character` store. v5 logs v4's same ERROR line
//!   (`strictFailures=true` inside the strict scope) and SKIPS: a v5-only WARN,
//!   no write, no hint, `skipped_reason = ReadFailed`. A whole-query failure is
//!   transient or structural, not a stable data shape, so skipping makes v5
//!   lag v4; it never undoes v4's names.
//!
//! ## NO-PORT (R-J)
//!
//! v4's `QUILLTAP_JOB_CHILD === '1'` arm (`:35-38`) has no v5 analog — v5 has
//! no job child — and v4's character triggers never reach it (the job-path
//! character writes never touch `name`, measured by the planning survey). So
//! [`StoreNameSkipReason`] carries no job-child reason.

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::fallback;
use super::DbError;
use crate::api::zod_issues::{key, zod_error_message, zod_iso_datetime_ok, zod_uuid_ok, ZodIssue};
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::store_names::{plan_store_names, StoreNameRow, StoreRename, VaultOwnerRow};

/// v4's triggers (`reconcileStoreNames(trigger)` call sites).
pub const TRIGGER_BOOT: &str = "boot";
pub const TRIGGER_RESTORE: &str = "restore";
pub const TRIGGER_QTAP_IMPORT: &str = "qtap-import";
pub const TRIGGER_CHARACTER_CREATED: &str = "character-created";
pub const TRIGGER_CHARACTER_RENAMED: &str = "character-renamed";
pub const TRIGGER_CHARACTER_DELETED: &str = "character-deleted";

/// v4's child-logger module, carried as a field on every reconcile line.
const MODULE: &str = "mount-index:reconcile-store-names";

/// Why a reconcile wrote nothing without planning (v4 `skippedReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreNameSkipReason {
    /// v4 `'mount-index-unavailable'`: the mount index is degraded or absent.
    MountIndexUnavailable,
    /// v5-only (`RECONCILE_READ_FAILURE_STRICT`): a whole-query read failed.
    ReadFailed,
}

impl StoreNameSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StoreNameSkipReason::MountIndexUnavailable => "mount-index-unavailable",
            StoreNameSkipReason::ReadFailed => "read-failed",
        }
    }
}

/// v4 `StoreNameReconcileResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreNameReconcileResult {
    pub renamed: Vec<StoreRename>,
    pub skipped_reason: Option<StoreNameSkipReason>,
}

impl StoreNameReconcileResult {
    fn skipped(reason: StoreNameSkipReason) -> Self {
        Self {
            renamed: Vec::new(),
            skipped_reason: Some(reason),
        }
    }
}

/// Reconcile every store's name — v4 `reconcileStoreNames(trigger)`. `mount`
/// is `None` when the mount index is degraded or absent (v4
/// `isMountIndexDegraded() ? null : getRawMountIndexDatabase()`): v4's skip
/// WARN, nothing read or written. An `Err` is a phase-2 failure (only
/// reachable through a Zod-dropped row that still holds a name under the
/// index): the savepoint is rolled back, and no INFO and no hint go out.
pub fn reconcile_store_names(
    main: &Connection,
    mount: Option<&Connection>,
    trigger: &str,
) -> Result<StoreNameReconcileResult, DbError> {
    let Some(mount) = mount else {
        tracing::warn!(
            target: "quilltap::mount_index",
            module = MODULE,
            trigger = trigger,
            "Store-name reconcile skipped: mount-index database unavailable"
        );
        return Ok(StoreNameReconcileResult::skipped(
            StoreNameSkipReason::MountIndexUnavailable,
        ));
    };

    // v4's `Promise.all` runs both reads whatever the other answers; so does
    // v5, so each read's own lines go out before the skip is decided.
    let stores =
        fallback::find_all_logged_strict("doc_mount_points", || read_store_name_rows(mount));
    let characters = fallback::find_all_logged_strict("characters", || read_vault_owner_rows(main));
    let (stores, characters) = match (stores, characters) {
        (Ok(s), Ok(c)) => (s, c),
        (Err(e), _) => return Ok(read_failed(trigger, "doc_mount_points", &e)),
        (_, Err(e)) => return Ok(read_failed(trigger, "characters", &e)),
    };

    let renames = plan_store_names(&stores, &characters);
    tracing::debug!(
        target: "quilltap::mount_index",
        module = MODULE,
        trigger = trigger,
        storeCount = stores.len(),
        characterCount = characters.len(),
        renameCount = renames.len(),
        "Store-name reconcile planned"
    );
    if renames.is_empty() {
        return Ok(StoreNameReconcileResult {
            renamed: Vec::new(),
            skipped_reason: None,
        });
    }

    apply_renames(mount, &renames)?;

    for rename in &renames {
        tracing::info!(
            target: "quilltap::mount_index",
            module = MODULE,
            trigger = trigger,
            id = %rename.id,
            from = %rename.from,
            to = %rename.to,
            reason = rename.reason.as_str(),
            "Renamed document store to its rightful name"
        );
    }
    publish_realtime(RealtimeTopic::MountPoints, None);
    Ok(StoreNameReconcileResult {
        renamed: renames,
        skipped_reason: None,
    })
}

/// R-A arm 2's skip: the v5-only WARN, nothing written.
fn read_failed(trigger: &str, collection: &str, error: &DbError) -> StoreNameReconcileResult {
    tracing::warn!(
        target: "quilltap::mount_index",
        module = MODULE,
        trigger = trigger,
        collection = collection,
        error = %fallback::error_text(error),
        "Store-name reconcile skipped: a store or character read failed"
    );
    StoreNameReconcileResult::skipped(StoreNameSkipReason::ReadFailed)
}

/// v4's ONE `db.transaction`: one `now`, every rename to `__renaming__ <id>`,
/// then each to its final name, both phases in rename-list order.
fn apply_renames(mount: &Connection, renames: &[StoreRename]) -> Result<(), DbError> {
    let now = crate::clock::now_iso();
    mount.execute_batch("SAVEPOINT qt_store_name_reconcile")?;
    let applied = (|| -> Result<(), rusqlite::Error> {
        let mut set_name = mount.prepare(
            "UPDATE \"doc_mount_points\" SET \"name\" = ?1, \"updatedAt\" = ?2 WHERE \"id\" = ?3",
        )?;
        for r in renames {
            set_name.execute(params![format!("__renaming__ {}", r.id), now, r.id])?;
        }
        for r in renames {
            set_name.execute(params![r.to, now, r.id])?;
        }
        Ok(())
    })();
    match applied {
        Ok(()) => {
            mount.execute_batch("RELEASE qt_store_name_reconcile")?;
            Ok(())
        }
        Err(e) => {
            let _ = mount.execute_batch(
                "ROLLBACK TO qt_store_name_reconcile; RELEASE qt_store_name_reconcile",
            );
            Err(e.into())
        }
    }
}

/// v4 `reconcileVaultNames` (`characters.repository.ts:67-78`): the reconcile
/// after a character create / rename / delete, which NEVER fails the caller's
/// write — any `Err` becomes WARN `Store-name reconcile after a character
/// change failed` `{trigger, characterId, error}` (root logger; `error` the
/// bare message) and a naming miss is repaired at the next boot.
pub fn reconcile_vault_names_or_warn(
    main: &Connection,
    mount: Option<&Connection>,
    trigger: &str,
    character_id: &str,
) {
    if let Err(error) = reconcile_store_names(main, mount, trigger) {
        tracing::warn!(
            target: "quilltap::db",
            trigger = trigger,
            characterId = character_id,
            error = %fallback::error_text(&error),
            "Store-name reconcile after a character change failed"
        );
    }
}

/// v4's wipe-time cadence (`delete-service.ts:171-189`, P4.D274 R-F): N
/// `characters.delete` calls inside ONE `Promise.all`, each awaiting its own
/// `reconcileVaultNames('character-deleted', id)`. MEASURED at the pin (five
/// identical runs): all N reconciles read BEFORE any of them writes, so each
/// plans the SAME renames from the post-delete state and each logs the whole
/// list — `planned`, then (when the plan is non-empty) the two-phase apply,
/// the INFO lines and a hint, N times over; a failing apply logs the wrapper's
/// WARN per character. v5 reads once and replays that plan per deleted
/// character, in delete order. Re-applying a rename onto a store already
/// carrying its target is state-identical (both phases re-run), as v4's later
/// writes are. A whole-query read failure or an unavailable mount index runs
/// each reconcile in full instead, so each logs its own lines, as v4's do.
pub fn reconcile_vault_names_after_concurrent_deletes(
    main: &Connection,
    mount: Option<&Connection>,
    character_ids: &[String],
) {
    let trigger = TRIGGER_CHARACTER_DELETED;
    let Some(mount_conn) = mount else {
        for id in character_ids {
            reconcile_vault_names_or_warn(main, mount, trigger, id);
        }
        return;
    };
    let stores =
        fallback::find_all_logged_strict("doc_mount_points", || read_store_name_rows(mount_conn));
    let characters = fallback::find_all_logged_strict("characters", || read_vault_owner_rows(main));
    let (stores, characters) = match (stores, characters) {
        (Ok(s), Ok(c)) => (s, c),
        (Err(e), _) | (_, Err(e)) => {
            // The first reconcile's read lines are the ones just logged (its
            // skip WARN names the first failed read); the rest re-read (and
            // re-fail) on their own, as v4's each do.
            let collection = if read_store_name_rows(mount_conn).is_err() {
                "doc_mount_points"
            } else {
                "characters"
            };
            for (i, id) in character_ids.iter().enumerate() {
                if i == 0 {
                    read_failed(trigger, collection, &e);
                } else {
                    reconcile_vault_names_or_warn(main, mount, trigger, id);
                }
            }
            return;
        }
    };
    let renames = plan_store_names(&stores, &characters);
    for id in character_ids {
        tracing::debug!(
            target: "quilltap::mount_index",
            module = MODULE,
            trigger = trigger,
            storeCount = stores.len(),
            characterCount = characters.len(),
            renameCount = renames.len(),
            "Store-name reconcile planned"
        );
        if renames.is_empty() {
            continue;
        }
        if let Err(error) = apply_renames(mount_conn, &renames) {
            tracing::warn!(
                target: "quilltap::db",
                trigger = trigger,
                characterId = %id,
                error = %fallback::error_text(&error),
                "Store-name reconcile after a character change failed"
            );
            continue;
        }
        for rename in &renames {
            tracing::info!(
                target: "quilltap::mount_index",
                module = MODULE,
                trigger = trigger,
                id = %rename.id,
                from = %rename.from,
                to = %rename.to,
                reason = rename.reason.as_str(),
                "Renamed document store to its rightful name"
            );
        }
        publish_realtime(RealtimeTopic::MountPoints, None);
    }
}

// ============================================================================
// The reads
// ============================================================================

/// A cell as v4's `hydrateRow` hands it to Zod (`backend.ts:363-470`), for the
/// columns these predicates read. `None` = `undefined` (a NULL cell, or a JSON
/// column whose text does not parse).
fn hydrate(cell: ValueRef<'_>, json_column: bool, boolean_column: bool) -> Option<Value> {
    match cell {
        ValueRef::Null => None,
        ValueRef::Blob(b) => {
            if boolean_column {
                // `Boolean(buffer)` — a Buffer is truthy.
                Some(Value::Bool(true))
            } else {
                // `blobToEmbedding`: a Float32 array (its values never reach a
                // message — only its type does).
                Some(Value::Array(
                    b.chunks_exact(4)
                        .map(|c| json!(f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64))
                        .collect(),
                ))
            }
        }
        ValueRef::Integer(i) => Some(if boolean_column {
            Value::Bool(i == 1)
        } else {
            json!(i)
        }),
        ValueRef::Real(f) => Some(if boolean_column {
            Value::Bool(f == 1.0)
        } else {
            json!(f)
        }),
        ValueRef::Text(t) => {
            let s = String::from_utf8_lossy(t).into_owned();
            if json_column {
                // `fromJsonSafe`: '' / unparseable / `null` → undefined.
                match serde_json::from_str::<Value>(&s) {
                    Ok(Value::Null) | Err(_) => None,
                    Ok(v) => Some(v),
                }
            } else if boolean_column {
                // `Boolean(str)`.
                Some(Value::Bool(!s.is_empty()))
            } else {
                Some(Value::String(s))
            }
        }
    }
}

/// `SELECT *` from `table`, each row hydrated into v4's column-keyed bag.
fn read_bags(
    conn: &Connection,
    table: &str,
    filter: &str,
    json_columns: &[&str],
    boolean_column: impl Fn(&str) -> bool,
) -> Result<Vec<Map<String, Value>>, DbError> {
    let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"{filter}"))?;
    let names: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut bag = Map::new();
        for (i, name) in names.iter().enumerate() {
            let cell = row.get_ref(i)?;
            if let Some(v) = hydrate(
                cell,
                json_columns.contains(&name.as_str()),
                boolean_column(name),
            ) {
                bag.insert(name.clone(), v);
            }
        }
        out.push(bag);
    }
    Ok(out)
}

/// v4's two drop lines for a row its schema refuses.
fn log_dropped_row(collection: &str, error: &str) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        error = %error,
        "Data validation failed"
    );
    tracing::warn!(
        target: "quilltap::db",
        collection = collection,
        error = %error,
        "Safe validation failed"
    );
}

// ---- Zod twins (v4's issue order = schema key order) ----------------------

fn uuid_issues(bag: &Map<String, Value>, k: &str, nullable: bool, issues: &mut Vec<ZodIssue>) {
    match bag.get(k) {
        None | Some(Value::Null) if nullable => {}
        Some(Value::String(v)) => {
            if !zod_uuid_ok(v) {
                issues.push(ZodIssue::invalid_uuid(vec![key(k)]));
            }
        }
        got => issues.push(ZodIssue::invalid_type("string", vec![key(k)], got)),
    }
}

fn timestamp_issues(bag: &Map<String, Value>, k: &str, nullable: bool, issues: &mut Vec<ZodIssue>) {
    match bag.get(k) {
        None | Some(Value::Null) if nullable => {}
        Some(Value::String(s)) => {
            if !zod_iso_datetime_ok(s) {
                issues.push(ZodIssue::invalid_datetime(vec![key(k)]));
            }
        }
        got => issues.push(ZodIssue::invalid_union(
            vec![
                vec![ZodIssue::invalid_type("string", vec![], got)],
                vec![ZodIssue::invalid_type("date", vec![], got)],
            ],
            vec![key(k)],
        )),
    }
}

/// `z.string()` (required) / `.default(..)` (absent passes) /
/// `.nullable().optional()` — the type gate, then `.min(1)` when `min1`.
fn string_issues(
    bag: &Map<String, Value>,
    k: &str,
    required: bool,
    min1: bool,
    issues: &mut Vec<ZodIssue>,
) {
    match bag.get(k) {
        None if !required => {}
        Some(Value::String(s)) => {
            if min1 && crate::jsstr::utf16_len(s) < 1 {
                issues.push(ZodIssue::too_small_string(json!(1), vec![key(k)]));
            }
        }
        got => issues.push(ZodIssue::invalid_type("string", vec![key(k)], got)),
    }
}

fn enum_issues(bag: &Map<String, Value>, k: &str, values: &[&str], issues: &mut Vec<ZodIssue>) {
    match bag.get(k) {
        None => {}
        Some(Value::String(v)) if values.contains(&v.as_str()) => {}
        Some(_) => issues.push(ZodIssue::invalid_value(values, vec![key(k)])),
    }
}

fn string_array_issues(bag: &Map<String, Value>, k: &str, issues: &mut Vec<ZodIssue>) {
    match bag.get(k) {
        None => {}
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                if !item.is_string() {
                    issues.push(ZodIssue::invalid_type(
                        "string",
                        vec![key(k), json!(i)],
                        Some(item),
                    ));
                }
            }
        }
        got => issues.push(ZodIssue::invalid_type("array", vec![key(k)], got)),
    }
}

/// `z.boolean().default(..)` — after `hydrateRow` a present cell is always a
/// boolean, so only a hand-built bag could fail it.
fn boolean_issues(bag: &Map<String, Value>, k: &str, issues: &mut Vec<ZodIssue>) {
    match bag.get(k) {
        None | Some(Value::Bool(_)) => {}
        got => issues.push(ZodIssue::invalid_type("boolean", vec![key(k)], got)),
    }
}

/// `z.number().int().default(0)`: the number gate, then the integer gate,
/// then the safe-integer bounds.
fn int_issues(bag: &Map<String, Value>, k: &str, issues: &mut Vec<ZodIssue>) {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    let Some(v) = bag.get(k) else {
        return;
    };
    let at = || vec![key(k)];
    match v.as_f64() {
        None => issues.push(ZodIssue::invalid_type("number", at(), Some(v))),
        Some(n) if n.fract() != 0.0 => issues.push(ZodIssue::invalid_int_type(at(), Some(v))),
        Some(n) if n > MAX_SAFE_INTEGER => issues.push(ZodIssue::too_big_int(at())),
        Some(n) if n < -MAX_SAFE_INTEGER => issues.push(ZodIssue::too_small_int(at())),
        Some(_) => {}
    }
}

/// v4 `DocMountPointSchema` (`lib/schemas/mount-index.types.ts:20-47`) WHOLE,
/// in schema key order (R-C).
fn doc_mount_point_issues(bag: &Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    uuid_issues(bag, "id", false, &mut issues);
    string_issues(bag, "name", true, true, &mut issues);
    string_issues(bag, "basePath", false, false, &mut issues);
    enum_issues(
        bag,
        "mountType",
        &["filesystem", "obsidian", "database"],
        &mut issues,
    );
    enum_issues(bag, "storeType", &["documents", "character"], &mut issues);
    string_array_issues(bag, "includePatterns", &mut issues);
    string_array_issues(bag, "excludePatterns", &mut issues);
    boolean_issues(bag, "enabled", &mut issues);
    timestamp_issues(bag, "lastScannedAt", true, &mut issues);
    enum_issues(
        bag,
        "scanStatus",
        &["idle", "scanning", "error"],
        &mut issues,
    );
    string_issues(bag, "lastScanError", false, false, &mut issues);
    enum_issues(
        bag,
        "conversionStatus",
        &["idle", "converting", "deconverting", "error"],
        &mut issues,
    );
    string_issues(bag, "conversionError", false, false, &mut issues);
    int_issues(bag, "fileCount", &mut issues);
    int_issues(bag, "chunkCount", &mut issues);
    int_issues(bag, "totalSizeBytes", &mut issues);
    timestamp_issues(bag, "createdAt", false, &mut issues);
    timestamp_issues(bag, "updatedAt", false, &mut issues);
    issues
}

/// `docMountPoints.findAll()` as the store-name readers see it: every row that
/// passes `DocMountPointSchema`, in row order, reduced to what naming needs;
/// a refused row DROPPED with v4's two lines. A whole-query failure is the
/// `Err` (the caller decides the fallback).
pub fn read_store_name_rows(mount: &Connection) -> Result<Vec<StoreNameRow>, DbError> {
    read_store_rows_where(mount, "")
}

/// v4 `docMountPoints.findEnabled()` (`findByFilter({enabled: true})`) as the
/// vault provisioning's adopt lookup sees it: the ENABLED rows that pass
/// `DocMountPointSchema`, in row order; a refused row DROPPED with v4's two
/// lines.
pub fn read_enabled_store_name_rows(mount: &Connection) -> Result<Vec<StoreNameRow>, DbError> {
    read_store_rows_where(mount, " WHERE \"enabled\" = 1")
}

/// A table v4's `getCollection()` would CREATE on first access
/// (`ensureCollection`) — so its read answers `[]`, never a failure. v5 does
/// not create it; it reads it as empty (measured on five committed fixtures
/// with no `characters` table: the plan is identical — no renames).
fn table_absent(conn: &Connection, table: &str) -> Result<bool, DbError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_none())
}

fn read_store_rows_where(mount: &Connection, filter: &str) -> Result<Vec<StoreNameRow>, DbError> {
    if table_absent(mount, "doc_mount_points")? {
        return Ok(Vec::new());
    }
    let bags = read_bags(
        mount,
        "doc_mount_points",
        filter,
        &["includePatterns", "excludePatterns"],
        |c| c == "enabled" || c.starts_with("is"),
    )?;
    let mut out = Vec::new();
    for bag in bags {
        let issues = doc_mount_point_issues(&bag);
        if !issues.is_empty() {
            log_dropped_row("doc_mount_points", &zod_error_message(&issues));
            continue;
        }
        let text = |k: &str| bag.get(k).and_then(Value::as_str).map(str::to_string);
        out.push(StoreNameRow {
            id: text("id").unwrap_or_default(),
            name: text("name").unwrap_or_default(),
            // `.default('documents')`.
            store_type: Some(text("storeType").unwrap_or_else(|| "documents".into())),
            created_at: text("createdAt").unwrap_or_default(),
        });
    }
    Ok(out)
}

/// `CharacterSchema`'s boolean columns (`character.types.ts`), plus
/// `hydrateRow`'s naming convention (`is*`, `enabled`, `active`, `npc`).
const CHARACTER_BOOLEAN_COLUMNS: &[&str] = &[
    "isFavorite",
    "npc",
    "defaultAgentModeEnabled",
    "defaultHelpToolsEnabled",
    "canDressThemselves",
    "canCreateOutfits",
    // P4.D324 (v4 `e1872c94f`): a Zod boolean in v4; without the name a `1`
    // cell would hydrate as a NUMBER.
    "canManageInventory",
    "canChooseOutfit",
    "systemTransparency",
    "coreWhisperEnabled",
    "canBeCarina",
    "enabled",
    "active",
];

/// `CharacterSchema`'s `.default([])` array columns: an EMPTY Buffer hydrates
/// to `[]`, which they accept.
const CHARACTER_DEFAULT_ARRAY_COLUMNS: &[&str] = &[
    "scenarios",
    "systemPrompts",
    "partnerLinks",
    "aliases",
    "tags",
    "avatarOverrides",
];

/// The plan's subset of `CharacterSchema` (R-C), in schema key order; a BLOB
/// in any other (non-boolean) column refuses the row too.
fn vault_owner_issues(bag: &Map<String, Value>) -> Vec<ZodIssue> {
    let mut issues = Vec::new();
    uuid_issues(bag, "id", false, &mut issues);
    uuid_issues(bag, "userId", false, &mut issues);
    string_issues(bag, "name", true, false, &mut issues);
    uuid_issues(bag, "characterDocumentMountPointId", true, &mut issues);
    timestamp_issues(bag, "createdAt", false, &mut issues);
    timestamp_issues(bag, "updatedAt", false, &mut issues);
    issues
}

/// `characters.findAllRaw()` as the store-name readers see it: every row that
/// passes R-C's predicate, in row order; a refused row DROPPED with v4's two
/// lines. A whole-query failure is the `Err`.
pub fn read_vault_owner_rows(main: &Connection) -> Result<Vec<VaultOwnerRow>, DbError> {
    if table_absent(main, "characters")? {
        return Ok(Vec::new());
    }
    let mut stmt = main.prepare("SELECT * FROM \"characters\"")?;
    let names: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut bag = Map::new();
        let mut blob_cell: Option<(usize, String)> = None;
        for (i, name) in names.iter().enumerate() {
            let cell = row.get_ref(i)?;
            let boolean =
                CHARACTER_BOOLEAN_COLUMNS.contains(&name.as_str()) || name.starts_with("is");
            if let ValueRef::Blob(b) = cell {
                let empty_array_ok =
                    b.is_empty() && CHARACTER_DEFAULT_ARRAY_COLUMNS.contains(&name.as_str());
                if !boolean && !empty_array_ok && blob_cell.is_none() {
                    blob_cell = Some((i, name.clone()));
                }
            }
            if let Some(v) = hydrate(cell, false, boolean) {
                bag.insert(name.clone(), v);
            }
        }
        let issues = vault_owner_issues(&bag);
        if !issues.is_empty() {
            log_dropped_row("characters", &zod_error_message(&issues));
            continue;
        }
        if let Some((i, name)) = blob_cell {
            // `RECONCILE_CHARACTER_ROW_SUBSET`: the subset passed, but v4's
            // whole schema refuses a hydrated Buffer here; v4's text would be
            // the ZodError over the full schema, which v5 does not port — the
            // rusqlite sentence stands in (the P4.149 precedent).
            let error = rusqlite::Error::InvalidColumnType(i, name, rusqlite::types::Type::Blob)
                .to_string();
            log_dropped_row("characters", &error);
            continue;
        }
        let text = |k: &str| bag.get(k).and_then(Value::as_str).map(str::to_string);
        out.push(VaultOwnerRow {
            id: text("id").unwrap_or_default(),
            name: text("name").unwrap_or_default(),
            character_document_mount_point_id: text("characterDocumentMountPointId"),
            created_at: text("createdAt").unwrap_or_default(),
        });
    }
    Ok(out)
}

/// The mount-points PATCH's owner check (v4 `[id]/route.ts:104-120`): the
/// FIRST character, in row order, whose vault pointer is `mount_point_id` —
/// archived characters included. The same read the reconcile plans from, so
/// a whole-query failure is the `Err` (R-A arm 3).
pub fn find_vault_owner(
    main: &Connection,
    mount_point_id: &str,
) -> Result<Option<VaultOwnerRow>, DbError> {
    let owners = fallback::find_all_logged_strict("characters", || read_vault_owner_rows(main))?;
    Ok(owners
        .into_iter()
        .find(|c| c.character_document_mount_point_id.as_deref() == Some(mount_point_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "2026-01-01T00:00:00.000Z";
    const USER: &str = "7e000000-0000-4000-8000-000000000001";
    const CHAR: &str = "6c000000-0000-4000-8000-000000000001";
    const ORPHAN: &str = "5a000000-0000-4000-8000-000000000001";
    const LIVE: &str = "5a000000-0000-4000-8000-000000000002";

    /// The plan's columns only; every other `DocMountPointSchema` field defaults.
    fn pair() -> (Connection, Connection) {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE characters (id TEXT PRIMARY KEY, userId TEXT NOT NULL, name TEXT NOT NULL, \
             characterDocumentMountPointId TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL)",
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();
        mount
            .execute_batch(
                "CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY, name TEXT NOT NULL, \
                 storeType TEXT DEFAULT 'documents', createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
                 CREATE UNIQUE INDEX \"idx_doc_mount_points_name_nocase\" ON \"doc_mount_points\" (\"name\" COLLATE NOCASE);",
            )
            .unwrap();
        (main, mount)
    }

    fn store(mount: &Connection, id: &str, name: &str, store_type: &str) {
        mount
            .execute(
                "INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, name, store_type, T],
            )
            .unwrap();
    }

    fn name_of(mount: &Connection, id: &str) -> String {
        mount
            .query_row(
                "SELECT name FROM doc_mount_points WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// The jest swap (`reconcile-store-names.test.ts:55-66`) under the real
    /// index, and R-H: ONE `mountPoints` hint for a renaming run, SILENCE for
    /// the no-op second run.
    #[tokio::test]
    async fn a_renaming_run_hints_mount_points_once_and_a_no_op_is_silent() {
        let (main, mount) = pair();
        store(&mount, ORPHAN, "Tester Character Vault", "character");
        store(&mount, LIVE, "Tester Character Vault (2)", "character");
        main.execute(
            "INSERT INTO characters VALUES (?1, ?2, 'Tester', ?3, ?4, ?4)",
            params![CHAR, USER, LIVE, T],
        )
        .unwrap();

        let mut cap = crate::realtime::publish_sites::HintCapture::start();
        let first = reconcile_store_names(&main, Some(&mount), TRIGGER_BOOT).unwrap();
        assert_eq!(first.renamed.len(), 2);
        assert_eq!(name_of(&mount, LIVE), "Tester Character Vault");
        assert_eq!(
            name_of(&mount, ORPHAN),
            "Tester Version 2026-01-01T000000Z Store"
        );
        assert_eq!(
            cap.drain_expecting(1).await,
            vec![("mountPoints".to_string(), None)]
        );

        let again = reconcile_store_names(&main, Some(&mount), TRIGGER_BOOT).unwrap();
        assert!(again.renamed.is_empty() && again.skipped_reason.is_none());
        assert!(
            cap.drain().await.is_empty(),
            "a no-op run publishes nothing"
        );
    }

    /// A phase-2 UNIQUE failure (a Zod-dropped row holding the wanted name)
    /// rolls the savepoint back — no name moves, no hint — and is the `Err`;
    /// the swallowing wrapper turns it into v4's WARN.
    #[tokio::test]
    async fn a_phase_two_failure_rolls_back_and_the_hook_wrapper_warns() {
        let (main, mount) = pair();
        store(&mount, "not-a-uuid", "Tester Character Vault", "documents");
        store(&mount, LIVE, "Tester Character Vault (2)", "character");
        main.execute(
            "INSERT INTO characters VALUES (?1, ?2, 'Tester', ?3, ?4, ?4)",
            params![CHAR, USER, LIVE, T],
        )
        .unwrap();

        let mut cap = crate::realtime::publish_sites::HintCapture::start();
        let err = reconcile_store_names(&main, Some(&mount), TRIGGER_BOOT).unwrap_err();
        assert_eq!(
            fallback::error_text(&err),
            "UNIQUE constraint failed: doc_mount_points.name"
        );
        assert_eq!(name_of(&mount, LIVE), "Tester Character Vault (2)");
        assert!(
            main.is_autocommit() && mount.is_autocommit(),
            "savepoint released"
        );
        assert!(cap.drain().await.is_empty());

        let ((), lines) = crate::test_support::captured_with(|| {
            reconcile_vault_names_or_warn(&main, Some(&mount), TRIGGER_CHARACTER_CREATED, CHAR)
        });
        assert_eq!(
            lines.last().map(String::as_str),
            Some(
                "WARN quilltap::db Store-name reconcile after a character change failed \
                 trigger=character-created characterId=6c000000-0000-4000-8000-000000000001 \
                 error=UNIQUE constraint failed: doc_mount_points.name"
            )
        );
    }

    /// P4.D324: `canManageInventory` is one of `CharacterSchema`'s booleans, so
    /// a BLOB there hydrates as `Boolean(buffer)` = `true` and the row STAYS
    /// (v4's Zod boolean accepts it); without the name the cell would be a
    /// stray Buffer and the row would be dropped as a refused one.
    #[test]
    fn a_blob_in_can_manage_inventory_hydrates_as_a_boolean_and_keeps_the_row() {
        let (main, _) = pair();
        main.execute_batch("ALTER TABLE characters ADD COLUMN canManageInventory INTEGER")
            .unwrap();
        main.execute(
            "INSERT INTO characters (id, userId, name, characterDocumentMountPointId, createdAt, \
             updatedAt, canManageInventory) VALUES (?1, ?2, 'Abigail', ?3, ?4, ?4, X'00')",
            rusqlite::params![CHAR, USER, LIVE, T],
        )
        .unwrap();
        let (rows, lines) =
            crate::test_support::captured_with(|| read_vault_owner_rows(&main).unwrap());
        assert_eq!(rows.len(), 1, "{lines:#?}");
        assert_eq!(rows[0].id, CHAR);
        assert!(lines.is_empty(), "{lines:#?}");
    }

    /// No mount index: v4's skip WARN, nothing read.
    #[test]
    fn an_unavailable_mount_index_skips_with_v4s_warn() {
        let (main, _) = pair();
        let (result, lines) = crate::test_support::captured_with(|| {
            reconcile_store_names(&main, None, TRIGGER_BOOT).unwrap()
        });
        assert_eq!(
            result.skipped_reason,
            Some(StoreNameSkipReason::MountIndexUnavailable)
        );
        assert_eq!(
            lines,
            vec!["WARN quilltap::mount_index Store-name reconcile skipped: mount-index database unavailable module=mount-index:reconcile-store-names trigger=boot"]
        );
    }
}
