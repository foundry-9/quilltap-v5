//! The collapse-duplicate-avatar-rolls boot heal (v4 migration
//! `collapse-duplicate-avatar-rolls-v1`, `7fbf8a55b`).
//!
//! Brings existing avatars into the configuration cache: one image per character
//! per configuration, instead of a fresh provider call — and a fresh stored image
//! — every time a character put the same coat back on.
//!
//! Pre-cache rows cannot reproduce the full-fidelity key: the LoRAs and stored
//! profile options in force at generation time are recorded nowhere. They get the
//! v0 key derived from `(generationModel, generationPrompt)` instead, which is
//! exactly what this pass groups on. The `v` discriminator in the key format
//! means a v0 key can never collide with a v1 one. The derivation is
//! [`crate::services::avatar_cache::derive_legacy_avatar_cache_key`] — the ONE
//! home, never a second spelling.
//!
//! Per group:
//!   - survivor = newest `createdAt`; it receives the v0 `generationKey`;
//!   - every reference to a non-survivor is repointed to the survivor:
//!     `chats.characterAvatars`, `characters.avatarOverrides`, and
//!     `chat_messages.attachments`;
//!   - the Lantern announcement that attached a deleted roll quotes its uuid
//!     inline ("catalogued under uuid `…`"), so that uuid is substituted in
//!     `content` and `opaqueContent` too. A mechanical id swap, not a rewrite:
//!     without it the message names a file that no longer exists, to the reader
//!     and to any model that later reads the transcript;
//!   - non-survivors are then deleted — the `files` row, and in the mount-index
//!     partition the chunks, links, and the `doc_mount_files` /
//!     `doc_mount_blobs` / `doc_mount_documents` rows. Explicitly, not via
//!     `ON DELETE CASCADE`: tables generated from the Zod schema carry no foreign
//!     keys at all.
//!
//! A group of one keeps its row and gains a key — including rows no chat
//! references any more. An orphaned roll is a perfectly good cache entry for its
//! configuration, and keeping it means the next chat to wear that outfit costs
//! nothing.
//!
//! **This is destructive and visible.** The duplicates are not redundant bytes;
//! they are different seeds of one prompt. Chats that displayed an older roll now
//! display the survivor, irreversibly.
//!
//! ## NOT one transaction — resumable by design
//!
//! v4 says so in its own header ("Resumable rather than transactional") and its
//! `run()` carries no `db.transaction(...)`: grouping reads every avatar row
//! regardless of whether it is already keyed, so a re-run after an interrupted
//! pass picks the same survivor and finishes the work that remains. Wrapping the
//! pass in one transaction would be a behaviour change, not a tidy-up: a blob
//! that refuses to go currently costs one kept `files` row and the pass carries
//! on, where a rollback would undo every repoint already made.
//!
//! ## The once-only mechanism — v4's own ledger, no divergence
//!
//! v4's runner checks `isMigrationCompleted(state, id)` BEFORE it calls
//! `shouldRun()` (`migrations/index.ts:125-137`, measured at the pin), so an
//! applied id is skipped outright and `shouldRun` gates only the first run. And
//! unlike the P4.D152 heal's migration, this `shouldRun` tests for DRIFT — "an
//! unkeyed avatar row remains" — so there is no zero-affected stamp to diverge
//! about. v5 therefore matches v4 exactly in all three directions:
//!
//!   - a ledger row from EITHER app → run nothing, write nothing (the real
//!     Friday instance has already been collapsed BY v4, so this is the arm a
//!     dogfood pass will meet);
//!   - no row and nothing unkeyed → run nothing, write nothing;
//!   - no row and an unkeyed avatar remains → run, then write ONE row in v4's
//!     shape, which a later v4 boot honours.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use crate::db::DbError;
use crate::services::avatar_cache::derive_legacy_avatar_cache_key;

const MIGRATION_ID: &str = "collapse-duplicate-avatar-rolls-v1";

/// v4's `context` field on every line this pass logs.
const LOG_CONTEXT: &str = "migration.collapse-duplicate-avatar-rolls";

/// Matches what the avatar handler names its output: `avatar_<Name>_<ts>.webp`.
/// The `ESCAPE '\'` is load-bearing — without it `_` is a single-character
/// wildcard and `avatarXfoo` would qualify.
const AVATAR_FILENAME_PREDICATE: &str = r"originalFilename LIKE 'avatar\_%' ESCAPE '\'";

/// What one pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollapseOutcome {
    /// The ledger already carries the row (either app completed the pass).
    AlreadyCompleted,
    /// v4's `shouldRun() === false`: no `files` table, no `generationKey`
    /// column, or no unkeyed avatar row left. Nothing run, nothing stamped.
    NotApplicable,
    /// The pass ran. `victims_deleted == 0` is v4's "nothing to collapse" early
    /// return, which still keys the survivors and still stamps the ledger.
    Ran {
        avatar_rows: usize,
        configurations: usize,
        rows_keyed: usize,
        victims_deleted: usize,
        blobs_deleted: usize,
        chats_changed: usize,
        characters_changed: usize,
        messages_changed: usize,
    },
}

/// One `files` row in the avatar selection.
struct AvatarRow {
    id: String,
    generation_prompt: Option<String>,
    generation_model: Option<String>,
    storage_key: Option<String>,
    /// v4 sorts on `String(row.createdAt)`; a SQL NULL renders `"null"` there,
    /// not `""` — and since `"null"` sorts ABOVE every ISO timestamp, a
    /// null-dated row would WIN a descending sort. Rendering it the same way is
    /// free and keeps the survivor choice identical.
    created_at: String,
}

/// `mount-blob:<mountPointId>:<blobId>` → blobId, or `None` for any other shape.
/// v4 spells this out inline here rather than reusing the storage helper, and the
/// two agree: a leading `mount-blob:`, a non-empty mount point, a non-empty blob.
fn blob_id_from_storage_key(storage_key: Option<&str>) -> Option<String> {
    let rest = storage_key?.strip_prefix("mount-blob:")?;
    let sep = rest.find(':')?;
    if sep < 1 || sep == rest.len() - 1 {
        return None;
    }
    Some(rest[sep + 1..].to_string())
}

/// v4 `JSON.stringify(value)` for a JSON column. `serde_json` is built with
/// `preserve_order`, so a rewritten object keeps the key order it was read in —
/// which is what keeps a repointed row byte-identical to its original but for
/// the swapped id.
fn json_text(v: &Value) -> Result<String, DbError> {
    serde_json::to_string(v).map_err(|e| DbError::Internal(format!("json serialize: {e}")))
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        if r.get::<_, String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Blob ids that a character still names as its portrait (`defaultImageId` is a
/// vault *link* id, so this resolves link → file → blob).
///
/// The canonical `images/avatar.webp` portrait is a different path from the
/// history rolls collapsed here and is never ours to delete. A row backed by one
/// of these blobs is therefore excluded from the victim list OUTRIGHT rather than
/// skipped at deletion time — skipping late would either strand a `files` row
/// whose bytes survived, or leave it unkeyed and re-trigger this pass on every
/// startup. That is why this runs BEFORE the grouping.
fn protected_blob_ids(
    main: &Connection,
    mount: Option<&Connection>,
) -> Result<HashSet<String>, DbError> {
    let Some(mount) = mount else {
        return Ok(HashSet::new());
    };
    if !table_exists(main, "characters")? {
        return Ok(HashSet::new());
    }
    let link_ids: Vec<String> = {
        let mut stmt = main.prepare(
            "SELECT defaultImageId AS linkId FROM characters \
             WHERE defaultImageId IS NOT NULL AND defaultImageId != ''",
        )?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut protected = HashSet::new();
    if link_ids.is_empty() {
        return Ok(protected);
    }
    if !table_exists(mount, "doc_mount_file_links")? || !table_exists(mount, "doc_mount_blobs")? {
        return Ok(protected);
    }
    let mut stmt = mount.prepare(
        "SELECT b.id AS blobId \
           FROM doc_mount_file_links l \
           JOIN doc_mount_blobs b ON b.fileId = l.fileId \
          WHERE l.id = ?1",
    )?;
    for link_id in link_ids {
        let rows = stmt
            .query_map([&link_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for blob_id in rows {
            protected.insert(blob_id);
        }
    }
    Ok(protected)
}

/// Drop a victim's bytes from the mount-index partition, mirroring
/// `deleteMountBlob`: the storage key was the user-visible handle for "the file",
/// so every link to that file goes with it. Returns whether a blob was found.
fn delete_victim_blob(mount: &Connection, blob_id: &str) -> Result<bool, DbError> {
    let file_id: Option<String> = mount
        .query_row(
            "SELECT fileId FROM doc_mount_blobs WHERE id = ?1",
            [blob_id],
            |r| r.get(0),
        )
        .optional_row()?;
    let Some(file_id) = file_id else {
        return Ok(false);
    };

    let link_ids: Vec<String> = {
        let mut stmt = mount.prepare("SELECT id FROM doc_mount_file_links WHERE fileId = ?1")?;
        let rows = stmt
            .query_map([&file_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for link_id in &link_ids {
        mount.execute("DELETE FROM doc_mount_chunks WHERE linkId = ?1", [link_id])?;
    }
    mount.execute(
        "DELETE FROM doc_mount_file_links WHERE fileId = ?1",
        [&file_id],
    )?;
    mount.execute(
        "DELETE FROM doc_mount_documents WHERE fileId = ?1",
        [&file_id],
    )?;
    mount.execute("DELETE FROM doc_mount_blobs WHERE fileId = ?1", [&file_id])?;
    mount.execute("DELETE FROM doc_mount_files WHERE id = ?1", [&file_id])?;
    Ok(true)
}

/// `query_row` → `Option`, without the `QueryReturnedNoRows` special case
/// leaking into every call site.
trait OptionalRow<T> {
    fn optional_row(self) -> Result<Option<T>, DbError>;
}
impl<T> OptionalRow<T> for Result<T, rusqlite::Error> {
    fn optional_row(self) -> Result<Option<T>, DbError> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Rewrite every `victim → survivor` occurrence in a JSON column, writing back
/// only the rows that actually changed (v4 `repointJsonColumn`). A payload that
/// does not parse is left exactly as it is.
fn repoint_json_column(
    main: &Connection,
    table: &str,
    column: &str,
    remap: &HashMap<String, String>,
    rewrite: impl Fn(&Value, &HashMap<String, String>) -> (Value, bool),
) -> Result<usize, DbError> {
    if !table_exists(main, table)? {
        return Ok(0);
    }
    let rows: Vec<(String, String)> = {
        let mut stmt = main.prepare(&format!(
            "SELECT id, {column} AS payload FROM {table} WHERE {column} IS NOT NULL"
        ))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut changed_count = 0usize;
    for (id, payload) in rows {
        let Ok(parsed) = serde_json::from_str::<Value>(&payload) else {
            continue; // Malformed payloads are left exactly as they are.
        };
        let (value, changed) = rewrite(&parsed, remap);
        if changed {
            main.execute(
                &format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2"),
                rusqlite::params![json_text(&value)?, id],
            )?;
            changed_count += 1;
        }
    }
    Ok(changed_count)
}

/// `chats.characterAvatars`: `{ [characterId]: { imageId, ... } }`.
fn rewrite_character_avatars(parsed: &Value, remap: &HashMap<String, String>) -> (Value, bool) {
    let Some(obj) = parsed.as_object() else {
        return (parsed.clone(), false);
    };
    let mut changed = false;
    let mut next = serde_json::Map::new();
    for (character_id, entry) in obj {
        if let Some(record) = entry.as_object() {
            let survivor = record
                .get("imageId")
                .and_then(Value::as_str)
                .and_then(|id| remap.get(id));
            if let Some(survivor) = survivor {
                let mut copy = record.clone();
                copy.insert("imageId".into(), Value::String(survivor.clone()));
                next.insert(character_id.clone(), Value::Object(copy));
                changed = true;
                continue;
            }
        }
        next.insert(character_id.clone(), entry.clone());
    }
    (Value::Object(next), changed)
}

/// `characters.avatarOverrides`: `[{ chatId, imageId }]`.
fn rewrite_avatar_overrides(parsed: &Value, remap: &HashMap<String, String>) -> (Value, bool) {
    let Some(items) = parsed.as_array() else {
        return (parsed.clone(), false);
    };
    let mut changed = false;
    let next: Vec<Value> = items
        .iter()
        .map(|entry| {
            if let Some(record) = entry.as_object() {
                let survivor = record
                    .get("imageId")
                    .and_then(Value::as_str)
                    .and_then(|id| remap.get(id));
                if let Some(survivor) = survivor {
                    let mut copy = record.clone();
                    copy.insert("imageId".into(), Value::String(survivor.clone()));
                    changed = true;
                    return Value::Object(copy);
                }
            }
            entry.clone()
        })
        .collect();
    (Value::Array(next), changed)
}

/// Run the collapse once per instance, guarded by v4's own migration ledger.
/// `now_iso` stamps the ledger's `completedAt`/`lastChecked` (the caller passes
/// [`crate::clock::now_iso`]).
///
/// `mount` is the mount-index partition; `None` is v4's "no mount-index database
/// present" arm (`openMountIndexDbIfPresent` → null), in which case nothing is
/// protected and no blob is deleted — the `files` rows still collapse.
pub fn collapse_duplicate_avatar_rolls(
    main: &Connection,
    mount: Option<&Connection>,
    now_iso: &str,
) -> Result<CollapseOutcome, DbError> {
    // The completed check comes FIRST, exactly as v4's runner orders it
    // (`isMigrationCompleted` before `shouldRun`).
    if table_exists(main, "migrations_state")? {
        let mut stmt = main.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
        if stmt.exists([MIGRATION_ID])? {
            return Ok(CollapseOutcome::AlreadyCompleted);
        }
    }

    // v4 `shouldRun`: the table, the column, and at least one unkeyed avatar row.
    if !table_exists(main, "files")? || !column_exists(main, "files", "generationKey")? {
        return Ok(CollapseOutcome::NotApplicable);
    }
    {
        let mut stmt = main.prepare(&format!(
            "SELECT 1 FROM files \
              WHERE {AVATAR_FILENAME_PREDICATE} \
                AND category = 'IMAGE' \
                AND generationPrompt IS NOT NULL \
                AND generationPrompt != '' \
                AND generationKey IS NULL \
              LIMIT 1"
        ))?;
        if !stmt.exists([])? {
            return Ok(CollapseOutcome::NotApplicable);
        }
    }

    let outcome = run_pass(main, mount)?;

    // The ledger write — v4's `migrations/state.ts` shapes verbatim (the P4.D152
    // heal's shapes, unchanged). v4's runner records every successful pass,
    // including its "nothing to collapse" early return, so this is unconditional
    // once the pass has run.
    super::migrations_ledger::ensure_migrations_tables(main)?;
    let CollapseOutcome::Ran {
        avatar_rows,
        configurations,
        blobs_deleted,
        chats_changed,
        characters_changed,
        messages_changed,
        victims_deleted,
        ..
    } = outcome
    else {
        return Ok(outcome);
    };
    let message = if victims_deleted == 0 {
        format!(
            "Keyed {} avatar configurations; nothing to collapse",
            match outcome {
                CollapseOutcome::Ran { rows_keyed, .. } => rows_keyed,
                _ => 0,
            }
        )
    } else {
        format!(
            "Collapsed {avatar_rows} avatar rolls to {configurations} configurations \
             ({blobs_deleted} images freed; repointed {chats_changed} chats, \
             {characters_changed} characters, {messages_changed} messages)"
        )
    };
    main.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            now_iso,
            env!("CARGO_PKG_VERSION"),
            avatar_rows as i64,
            message
        ],
    )?;
    for (k, v) in [
        ("lastChecked", now_iso),
        ("quilltapVersion", env!("CARGO_PKG_VERSION")),
    ] {
        main.execute(
            "INSERT INTO migrations_metadata (key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![k, v],
        )?;
    }

    Ok(outcome)
}

/// v4's `run()` body. Separated from the gate so the gate reads as the gate.
fn run_pass(main: &Connection, mount: Option<&Connection>) -> Result<CollapseOutcome, DbError> {
    // Opened before grouping: which rolls are off-limits decides who can be a
    // victim, not merely what gets deleted.
    let protected = protected_blob_ids(main, mount)?;

    let rows: Vec<AvatarRow> = {
        let mut stmt = main.prepare(&format!(
            "SELECT id, generationPrompt, generationModel, storageKey, createdAt \
               FROM files \
              WHERE {AVATAR_FILENAME_PREDICATE} \
                AND category = 'IMAGE' \
                AND generationPrompt IS NOT NULL \
                AND generationPrompt != ''"
        ))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AvatarRow {
                    id: r.get(0)?,
                    generation_prompt: r.get(1)?,
                    generation_model: r.get(2)?,
                    storage_key: r.get(3)?,
                    created_at: r
                        .get::<_, Option<String>>(4)?
                        .unwrap_or_else(|| "null".to_string()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    // Group by the v0 key: the most faithful reconstruction available for a row
    // generated before the cache existed. Insertion order is preserved so the
    // survivor/victim walk matches v4's `Map` iteration.
    let mut group_order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let key = derive_legacy_avatar_cache_key(
            row.generation_model.as_deref(),
            row.generation_prompt.as_deref().unwrap_or(""),
        );
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                group_order.push(key.clone());
                Vec::new()
            })
            .push(i);
    }

    // Survivor per group, and the victim → survivor remap driving every repoint.
    let mut remap: HashMap<String, String> = HashMap::new();
    let mut survivors: Vec<(String, String)> = Vec::new();
    let mut victims: Vec<usize> = Vec::new();

    for key in &group_order {
        let bucket = &groups[key];
        let mut ordered: Vec<usize> = bucket.clone();
        // v4 `String(b.createdAt).localeCompare(String(a.createdAt))` — newest
        // first. These are ISO-8601 strings from one clock, so the collation
        // question does not arise: every comparison is digit against digit in the
        // same position. `sort_by` is stable, as V8's is.
        ordered.sort_by(|a, b| rows[*b].created_at.cmp(&rows[*a].created_at));
        let survivor = ordered[0];
        survivors.push((rows[survivor].id.clone(), key.clone()));
        for &victim in &ordered[1..] {
            let blob_id = blob_id_from_storage_key(rows[victim].storage_key.as_deref());
            if blob_id.as_ref().is_some_and(|b| protected.contains(b)) {
                // Still serving as a character's portrait. It keeps its row and
                // its bytes, and is keyed alongside the survivor — it is a
                // perfectly truthful image of this configuration, and the lookup
                // prefers the newest holder of a key anyway.
                survivors.push((rows[victim].id.clone(), key.clone()));
                tracing::info!(
                    target: "quilltap::migration",
                    context = LOG_CONTEXT,
                    fileId = %rows[victim].id,
                    "Keeping avatar roll still serving as a character portrait"
                );
                continue;
            }
            remap.insert(rows[victim].id.clone(), rows[survivor].id.clone());
            victims.push(victim);
        }
    }

    tracing::info!(
        target: "quilltap::migration",
        context = LOG_CONTEXT,
        avatarRows = rows.len(),
        configurations = groups.len(),
        victims = victims.len(),
        "Collapsing avatar rolls"
    );

    // ---- 1. Key the survivors ---------------------------------------------
    for (id, key) in &survivors {
        main.execute(
            "UPDATE files SET generationKey = ?1 WHERE id = ?2",
            rusqlite::params![key, id],
        )?;
    }

    if victims.is_empty() {
        // v4's early return: keyed, nothing to collapse, no repoints attempted.
        return Ok(CollapseOutcome::Ran {
            avatar_rows: rows.len(),
            configurations: groups.len(),
            rows_keyed: survivors.len(),
            victims_deleted: 0,
            blobs_deleted: 0,
            chats_changed: 0,
            characters_changed: 0,
            messages_changed: 0,
        });
    }

    // ---- 2. Repoint every reference to a victim ---------------------------
    let chats_changed = repoint_json_column(
        main,
        "chats",
        "characterAvatars",
        &remap,
        rewrite_character_avatars,
    )?;
    let characters_changed = repoint_json_column(
        main,
        "characters",
        "avatarOverrides",
        &remap,
        rewrite_avatar_overrides,
    )?;
    let messages_changed = repoint_messages(main, &remap)?;

    // ---- 3. Delete the victims --------------------------------------------
    let mut blobs_deleted = 0usize;
    let mut victims_deleted = 0usize;
    for &victim in &victims {
        let blob_id = blob_id_from_storage_key(rows[victim].storage_key.as_deref());
        if let (Some(mount), Some(blob_id)) = (mount, blob_id.as_deref()) {
            match delete_victim_blob(mount, blob_id) {
                Ok(true) => blobs_deleted += 1,
                Ok(false) => {}
                Err(error) => {
                    // A blob that refuses to go is not a reason to abandon the
                    // pass; the files row stays too, and the next run retries the
                    // pair. (This is why the pass is not one transaction.)
                    tracing::warn!(
                        target: "quilltap::migration",
                        context = LOG_CONTEXT,
                        fileId = %rows[victim].id,
                        blobId = %blob_id,
                        error = %error,
                        "Failed to delete avatar blob, leaving its file row in place"
                    );
                    continue;
                }
            }
        }
        main.execute("DELETE FROM files WHERE id = ?1", [&rows[victim].id])?;
        victims_deleted += 1;
    }

    let outcome = CollapseOutcome::Ran {
        avatar_rows: rows.len(),
        configurations: groups.len(),
        rows_keyed: survivors.len(),
        // v4 reports `victims.length` here, not the number actually deleted: a
        // blob failure leaves the row AND still counts. Reproduced rather than
        // corrected — the number is a log field and a ledger message, and
        // "corrected" would be a second spelling of v4's own count.
        victims_deleted: victims.len(),
        blobs_deleted,
        chats_changed,
        characters_changed,
        messages_changed,
    };
    let _ = victims_deleted;

    if let CollapseOutcome::Ran {
        configurations,
        rows_keyed,
        victims_deleted,
        blobs_deleted,
        chats_changed,
        characters_changed,
        messages_changed,
        ..
    } = &outcome
    {
        tracing::info!(
            target: "quilltap::migration",
            context = LOG_CONTEXT,
            configurations = *configurations,
            rowsKeyed = *rows_keyed,
            victimsDeleted = *victims_deleted,
            blobsDeleted = *blobs_deleted,
            chatsChanged = *chats_changed,
            charactersChanged = *characters_changed,
            messagesChanged = *messages_changed,
            "Collapsed duplicate avatar rolls"
        );
    }

    Ok(outcome)
}

/// `chat_messages.attachments`: `[fileId, ...]` — plus the uuid the Lantern
/// quoted inline, which must name the file actually attached.
fn repoint_messages(main: &Connection, remap: &HashMap<String, String>) -> Result<usize, DbError> {
    if !table_exists(main, "chat_messages")? {
        return Ok(0);
    }
    struct MessageRow {
        id: String,
        attachments: String,
        content: Option<String>,
        opaque_content: Option<String>,
    }
    let rows: Vec<MessageRow> = {
        let mut stmt = main.prepare(
            "SELECT id, attachments, content, opaqueContent \
               FROM chat_messages \
              WHERE attachments IS NOT NULL AND attachments != '[]'",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(MessageRow {
                    id: r.get(0)?,
                    attachments: r.get(1)?,
                    content: r.get(2)?,
                    opaque_content: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut changed = 0usize;
    for row in rows {
        let Ok(parsed) = serde_json::from_str::<Value>(&row.attachments) else {
            continue;
        };
        let Some(items) = parsed.as_array() else {
            continue;
        };

        let mut swapped: Vec<Value> = Vec::with_capacity(items.len());
        // Only the ids this message actually carried — substituting the whole
        // remap into every message would scan the full victim list per row.
        let mut swapped_here: Vec<(String, String)> = Vec::new();
        for entry in items {
            let survivor = entry.as_str().and_then(|s| remap.get(s));
            match survivor {
                Some(survivor) => {
                    swapped_here.push((
                        entry.as_str().unwrap_or_default().to_string(),
                        survivor.clone(),
                    ));
                    swapped.push(Value::String(survivor.clone()));
                }
                None => swapped.push(entry.clone()),
            }
        }

        if swapped_here.is_empty() {
            continue;
        }
        let mut content = row.content.clone();
        let mut opaque_content = row.opaque_content.clone();
        for (victim_id, survivor_id) in &swapped_here {
            if let Some(c) = content.as_ref() {
                if c.contains(victim_id.as_str()) {
                    content = Some(c.replace(victim_id.as_str(), survivor_id));
                }
            }
            if let Some(c) = opaque_content.as_ref() {
                if c.contains(victim_id.as_str()) {
                    opaque_content = Some(c.replace(victim_id.as_str(), survivor_id));
                }
            }
        }
        main.execute(
            "UPDATE chat_messages SET attachments = ?1, content = ?2, opaqueContent = ?3 \
             WHERE id = ?4",
            rusqlite::params![
                json_text(&Value::Array(swapped))?,
                content,
                opaque_content,
                row.id
            ],
        )?;
        changed += 1;
    }
    Ok(changed)
}
