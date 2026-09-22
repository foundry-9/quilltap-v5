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
//!     partition the victim's **own** link and its chunks, plus (when that link
//!     was the last one for the file) the `doc_mount_files` /
//!     `doc_mount_blobs` / `doc_mount_documents` rows. Explicitly, not via
//!     `ON DELETE CASCADE`: tables generated from the Zod schema carry no foreign
//!     keys at all. Only the roll's own link is ours — a plate the operator
//!     copied into a character's album is two links over one set of bytes, and
//!     the album's copy is theirs to keep. See [`drop_victim_roll_link`].
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

// `photos::photos_paths` is the ONE home, as v4 has one (`lib/photos/
// photos-paths.ts`, which is what its migration imports) — Node-faithful since
// P4.91 moved Node's own `dirname` loop into it, and driven by
// `photos_relative_path_equivalence` over v4's real function. The runtime roll
// rule (`avatar_rolls_service::classify_roll_links`, through
// `photo_link_summary`'s `isPhotoAlbum`) reads the same home, so the heal and
// the runtime agree by construction rather than by choice. (Until P4.91 there
// were two v5 copies disagreeing on trailing-slash runs, and this import named
// which one it wanted; there is nothing left to choose between.)
use crate::db::doc_mount_file_links::gc_orphaned_file_row;
use crate::db::DbError;
use crate::photos::photos_paths::is_photos_relative_path;
use crate::services::avatar_cache::derive_legacy_avatar_cache_key;
use crate::services::file_storage::parse_mount_blob_storage_key;

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
        /// Rows keyed alongside their survivor because they were still a
        /// character's portrait when the pass ran (v4 `protectedKept.length`,
        /// `23abc1ba1`). Drives the summary bag AND the ledger message's
        /// [`kept_clause`].
        protected_kept: usize,
        /// Victims whose own link went but whose bytes stayed, because the
        /// operator had kept that plate in a character's album.
        album_copies_kept: usize,
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

/// `mount-blob:<mountPointId>:<blobId>` → its two halves, or `None` for any
/// other shape — v4 `parseMountBlobKey` (`23abc1ba1`, which split it out of
/// `blobIdFromStorageKey` because the mount half is now load-bearing).
///
/// v4 spells the rule out inline in the migration rather than reusing the
/// storage helper; v5 goes the other way and reaches
/// [`parse_mount_blob_storage_key`], which is the same three conditions
/// character for character (a leading `mount-blob:`, a non-empty mount point, a
/// non-empty blob) and is already what [`crate::photos::avatar_rolls_service`]
/// reads. A second spelling is how the two come to disagree.
fn parse_mount_blob_key(storage_key: Option<&str>) -> Option<(String, String)> {
    parse_mount_blob_storage_key(storage_key?)
}

/// `mount-blob:<mountPointId>:<blobId>` → blobId, or `None` for any other shape
/// (v4 `blobIdFromStorageKey`, which delegates the same way since `23abc1ba1`).
fn blob_id_from_storage_key(storage_key: Option<&str>) -> Option<String> {
    parse_mount_blob_key(storage_key).map(|(_mount_point_id, blob_id)| blob_id)
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

/// What [`drop_victim_roll_link`] did — v4's `{ linkDropped, bytesFreed }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DroppedVictim {
    /// The roll's own link was found and taken.
    link_dropped: bool,
    /// …and it was the last consumer of the bytes, so the blob went too.
    bytes_freed: bool,
}

/// Drop a victim roll's **own link**, and its bytes only when that link was the
/// last consumer of them — v4 `dropVictimRollLink` (`23abc1ba1`, bug 145).
///
/// Deliberately *not* `deleteMountBlob`'s verb. A roll the operator has already
/// copied into a character's album is two links over one set of bytes — the
/// roll's own `character-avatars/…` (or `images/history/…`) link, and the
/// album's `photos/…` link — and only the first is ours to take. Deleting by
/// `fileId`, as this once did (and as v4 did until `23abc1ba1`), took the album
/// photo with the duplicate: the operator kept a plate they liked, and
/// collapsing an unrelated *roll* of the same configuration silently removed
/// the copy they kept. Nothing in the main DB records an album link, so the
/// loss left no trace to notice or undo.
///
/// This is the rule [`crate::photos::avatar_rolls_service`]'s `delete_avatar_roll`
/// follows at runtime — "only the roll's own link is ours" — reached here
/// through the same GC chokepoint ([`gc_orphaned_file_row`]) the link
/// repository's `delete_with_gc` uses, so all three collect content the same way.
///
/// A victim whose only surviving link is the album copy drops no link at all and
/// keeps its bytes; its `files` row still goes, exactly as `delete_avatar_roll`
/// does with a `None` roll link.
fn drop_victim_roll_link(
    mount: &Connection,
    blob_id: &str,
    roll_mount_point_id: Option<&str>,
) -> Result<DroppedVictim, DbError> {
    let file_id: Option<String> = mount
        .query_row(
            "SELECT fileId FROM doc_mount_blobs WHERE id = ?1",
            [blob_id],
            |r| r.get(0),
        )
        .optional_row()?;
    let Some(file_id) = file_id else {
        return Ok(DroppedVictim {
            link_dropped: false,
            bytes_freed: false,
        });
    };

    // No ORDER BY — v4 has none either, and its `Array.prototype.find` walks
    // the rows in whatever order the SELECT returns them (rowid order for a
    // plain table). Adding one here would pick a different link from v4's on a
    // content row with more than one candidate.
    let links: Vec<(String, String, Option<String>)> = {
        let mut stmt = mount.prepare(
            "SELECT id, mountPointId, relativePath FROM doc_mount_file_links WHERE fileId = ?1",
        )?;
        let rows = stmt
            .query_map([&file_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    // The roll's own link: never one in a `photos/` folder, and — when the
    // storage key names a mount — the one living in that mount.
    let roll_link = links.iter().find(|(_id, mount_point_id, relative_path)| {
        !is_photos_relative_path(relative_path.as_deref())
            // v4's `!rollMountPointId || l.mountPointId === rollMountPointId` is
            // a JS truthiness test, so an EMPTY mount point means "unconstrained"
            // there too. Unreachable from the pass — `parseMountBlobKey` rejects
            // a storage key whose mount half is empty — but part of the
            // function's contract, and pinned as such at unit level.
            && match roll_mount_point_id {
                None | Some("") => true,
                Some(mp) => mount_point_id == mp,
            }
    });

    if let Some((link_id, _, _)) = roll_link {
        mount.execute("DELETE FROM doc_mount_chunks WHERE linkId = ?1", [link_id])?;
        mount.execute("DELETE FROM doc_mount_file_links WHERE id = ?1", [link_id])?;
    }

    // Bytes go only once nothing links to them any more — a no-op while the
    // album still holds a copy. v4's `(collected?.blobs ?? 0) > 0`: a
    // document-only orphan collects a `files` row and no blob, and does NOT
    // count as bytes freed.
    let collected = gc_orphaned_file_row(mount, &file_id)?;
    Ok(DroppedVictim {
        link_dropped: roll_link.is_some(),
        bytes_freed: collected.is_some_and(|c| c.blobs > 0),
    })
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

/// Every `generationKey` this pass leaves on more than one row must be one it
/// doubled up on purpose — v4 `unexplainedDuplicateKeys` (`23abc1ba1`).
///
/// Run **in-pass**, with the run's own set of deliberate keeps, and not as an
/// after-the-fact census: the protection is `characters.defaultImageId`, which
/// the operator can repoint at any time, so reconstructing "was this keep
/// legitimate?" from a later snapshot answers a different question and reports
/// false positives for every portrait that has since moved or whose character
/// has gone. The only moment the invariant is checkable is the one in which the
/// decisions were made. (This is what closed v4's own bug 143 as not a defect:
/// built the way that filing proposed, the check would have manufactured
/// exactly the three false positives the filing could not explain.)
///
/// ⚠ The predicate is EVERY `files` row carrying a shared non-empty
/// `generationKey` — deliberately NOT the avatar predicate the pass groups on,
/// which is what lets a row outside the pass's reach (v4's `stowaway` test: a
/// `background_scene.webp` with no storage key) be named.
///
/// Reports rather than throws. A surprise here means the *next* pass has
/// something to look at, not that the collapse just done should be abandoned.
///
/// NO-PORT: v4 interleaves `reportProgress(…)` calls through both loops here
/// (and through the delete/repoint loops elsewhere in the pass). Those drive v4's
/// migration-runner progress bar, which v5 has no counterpart for — the pass runs
/// inside boot, not behind a runner UI — so they are deliberately absent rather
/// than stubbed.
fn unexplained_duplicate_keys(
    main: &Connection,
    kept_deliberately: &HashSet<String>,
) -> Result<Vec<String>, DbError> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = main.prepare(
            "SELECT id, generationKey AS key, createdAt FROM files \
              WHERE generationKey IS NOT NULL AND generationKey != '' \
                AND generationKey IN ( \
                      SELECT generationKey FROM files \
                       WHERE generationKey IS NOT NULL AND generationKey != '' \
                       GROUP BY generationKey HAVING COUNT(*) > 1)",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    // v4's `String(row.createdAt)` — a SQL NULL renders "null".
                    r.get::<_, Option<String>>(2)?
                        .unwrap_or_else(|| "null".to_string()),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    // v4 buckets into a `Map` in SELECT row order; the bucket walk is that
    // insertion order, and it decides the order of `fileIds`.
    //
    // ⚠ RECORDED, NOT FIXED (P4.91, escalated for the human's ruling). Neither
    // side's SELECT above carries an `ORDER BY`, so both engines take whatever
    // row order the scan hands back. On a fixture with no index on
    // `generationKey` that is stable and the two agree; the REAL instance has
    // `idx_files_generationKey` (itself recorded as a fresh-instance divergence
    // at P4.D192 — a fresh v4 never creates it), and SQLite may satisfy the
    // `IN`-subquery through the index and hand back a different order. So
    // `fileIds` is instance-dependent in production while the harness pins one
    // order. An `ORDER BY id` on BOTH sides is the fix AND a deliberate
    // divergence from v4's shipped SQL, which is a ruling this lane does not
    // get to make on its own — so it is written down here rather than added.
    let mut key_order: Vec<String> = Vec::new();
    let mut by_key: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (id, key, created_at) in rows {
        by_key
            .entry(key.clone())
            .or_insert_with(|| {
                key_order.push(key.clone());
                Vec::new()
            })
            .push((id, created_at));
    }

    let mut unexplained: Vec<String> = Vec::new();
    for key in &key_order {
        let mut bucket = by_key[key].clone();
        // v4 `String(b.createdAt).localeCompare(String(a.createdAt))` — newest
        // first, stable, and the same ISO-strings-from-one-clock rationale as
        // the survivor sort in `run_pass`.
        bucket.sort_by(|a, b| b.1.cmp(&a.1));
        // The newest holder is the survivor and needs no excuse; every other row
        // sharing its key must be one this pass chose to keep.
        for (id, _created_at) in bucket.iter().skip(1) {
            if !kept_deliberately.contains(id) {
                unexplained.push(id.clone());
            }
        }
    }
    Ok(unexplained)
}

/// v4 `reportCensus` — silent when the invariant holds, one warn when it does
/// not. Called on BOTH of the pass's exits.
fn report_census(main: &Connection, protected_kept: &[String]) -> Result<(), DbError> {
    let kept: HashSet<String> = protected_kept.iter().cloned().collect();
    let unexplained = unexplained_duplicate_keys(main, &kept)?;
    if unexplained.is_empty() {
        return Ok(());
    }
    let first_twenty: Vec<&str> = unexplained.iter().take(20).map(String::as_str).collect();
    let file_ids = serde_json::to_string(&first_twenty)
        .map_err(|e| DbError::Internal(format!("json serialize: {e}")))?;
    // `fileIdsJson`, not `fileIds`: v4 hands winston a raw `string[]` and the
    // record reads `"fileIds":["id1","id2"]`. `tracing` has no structured-value
    // channel here (no `valuable`; a `?`-formatted `Vec` would render Rust's
    // `Debug`), so the callsite serializes and the file layer's `…Json`
    // convention re-parses it back into an array under the unsuffixed name —
    // `quilltap_web::log_file::JSON_FIELD_SUFFIX`. Before P4.91 this field was
    // spelled `fileIds` and reached `combined.log` as a quoted JSON STRING,
    // which is the one place v5's record disagreed with v4's.
    tracing::warn!(
        target: "quilltap::migration",
        context = LOG_CONTEXT,
        unexplainedCount = unexplained.len(),
        fileIdsJson = file_ids.as_str(),
        "Avatar rolls share a generation key this pass did not choose to double up"
    );
    Ok(())
}

/// What the pass *kept*, said out loud — v4 `keptClause` (`23abc1ba1`). The
/// summary used to count only what it collapsed, so every deliberate keep was
/// invisible in the one record that outlives the logs, leaving a later reader to
/// rediscover the protected branch from the residue and mistake it for damage.
fn kept_clause(protected_kept_count: usize) -> String {
    match protected_kept_count {
        0 => String::new(),
        1 => "; kept 1 roll still serving as a character portrait".to_string(),
        n => format!("; kept {n} rolls still serving as character portraits"),
    }
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
        protected_kept,
        ..
    } = outcome
    else {
        return Ok(outcome);
    };
    // Both sentences carry v4's kept clause (`23abc1ba1`): the ledger row is the
    // one record that outlives the logs, so a pass that kept a portrait says so.
    let message = if victims_deleted == 0 {
        format!(
            "Keyed {} avatar configurations; nothing to collapse{}",
            match outcome {
                CollapseOutcome::Ran { rows_keyed, .. } => rows_keyed,
                _ => 0,
            },
            kept_clause(protected_kept)
        )
    } else {
        format!(
            "Collapsed {avatar_rows} avatar rolls to {configurations} configurations \
             ({blobs_deleted} images freed; repointed {chats_changed} chats, \
             {characters_changed} characters, {messages_changed} messages){}",
            kept_clause(protected_kept)
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
    // Rows deliberately keyed alongside their survivor because they were still a
    // character's portrait when this pass ran. Recorded here, at the moment of
    // the decision, because it cannot be reconstructed later:
    // `characters.defaultImageId` is mutable, so a portrait moved after the fact
    // makes a perfectly correct keep look like an unexplained duplicate.
    let mut protected_kept: Vec<String> = Vec::new();

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
                protected_kept.push(rows[victim].id.clone());
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
        // The census runs on THIS exit too (`23abc1ba1`) — and it is the exit
        // v4's own new census test takes, since a lone roll makes no victims.
        report_census(main, &protected_kept)?;
        return Ok(CollapseOutcome::Ran {
            avatar_rows: rows.len(),
            configurations: groups.len(),
            rows_keyed: survivors.len(),
            victims_deleted: 0,
            protected_kept: protected_kept.len(),
            album_copies_kept: 0,
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
    let mut album_copies_kept = 0usize;
    let mut victims_deleted = 0usize;
    for &victim in &victims {
        // v4 guards on `mountDb && parsed` (was `&& blobId`) — the same truth
        // table, since a parse that succeeds always yields a non-empty blob half.
        let parsed = parse_mount_blob_key(rows[victim].storage_key.as_deref());
        if let (Some(mount), Some((roll_mount_point_id, blob_id))) = (mount, parsed.as_ref()) {
            match drop_victim_roll_link(mount, blob_id, Some(roll_mount_point_id.as_str())) {
                Ok(DroppedVictim {
                    bytes_freed: true, ..
                }) => blobs_deleted += 1,
                Ok(DroppedVictim {
                    link_dropped: true, ..
                }) => {
                    // The bytes stayed because the operator had kept this plate
                    // in a character's album. That copy is theirs, not the
                    // cache's.
                    album_copies_kept += 1
                }
                Ok(_) => {}
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
        protected_kept: protected_kept.len(),
        album_copies_kept,
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
        protected_kept: protected_kept_count,
        album_copies_kept,
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
            protectedKept = *protected_kept_count,
            albumCopiesKept = *album_copies_kept,
            blobsDeleted = *blobs_deleted,
            chatsChanged = *chats_changed,
            charactersChanged = *characters_changed,
            messagesChanged = *messages_changed,
            "Collapsed duplicate avatar rolls"
        );
    }

    // v4 reports the census AFTER the summary line, on this exit too.
    report_census(main, &protected_kept)?;

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
        // content / opaqueContent are compressed-text columns: read through
        // qt_text() and write back through text_to_blob(), exactly as v4's
        // `collapse-duplicate-avatar-rolls-v1.ts:497-508` does. v4's migration
        // is ordered BEFORE compress-chat-message-text-v1, so on a real
        // instance it has already run against plaintext — the codec here is
        // what makes it safe to replay on a database that has since been
        // compressed. Before this wrap the `?` below aborted the BOOT pass:
        // this heal is the FIRST thing that broke on a migrated instance.
        let mut stmt = main.prepare(
            "SELECT id, attachments, qt_text(content) AS content, \
                    qt_text(opaqueContent) AS opaqueContent \
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

#[cfg(test)]
mod drop_victim_roll_link_tests {
    //! The arms of [`drop_victim_roll_link`] the tier-2 family cannot reach.
    //!
    //! The family drives the whole pass, and the pass always passes a mount
    //! point (v4's `parseMountBlobKey` rejects a storage key whose mount half is
    //! empty, so `parsed.mountPointId` is never falsy where v4 calls it). The
    //! unconstrained arm is part of the function's CONTRACT — it is the shape
    //! `classify_roll_links` already carries for a roll whose storage key names
    //! no mount — so it is pinned here instead (v4 `23abc1ba1`).

    use super::*;

    /// The five mount tables at the shape v4's own migration suite builds.
    fn mount_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            r#"
            CREATE TABLE "doc_mount_files" ("id" TEXT PRIMARY KEY);
            CREATE TABLE "doc_mount_blobs" ("id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL);
            CREATE TABLE "doc_mount_documents" ("id" TEXT PRIMARY KEY, "fileId" TEXT NOT NULL);
            CREATE TABLE "doc_mount_file_links" (
              "id" TEXT PRIMARY KEY,
              "fileId" TEXT NOT NULL,
              "mountPointId" TEXT NOT NULL,
              "relativePath" TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE "doc_mount_chunks" ("id" TEXT PRIMARY KEY, "linkId" TEXT NOT NULL);
            "#,
        )
        .unwrap();
        c.execute(
            r#"INSERT INTO "doc_mount_files" (id) VALUES ('content-1')"#,
            [],
        )
        .unwrap();
        c.execute(
            r#"INSERT INTO "doc_mount_blobs" (id, fileId) VALUES ('blob-1', 'content-1')"#,
            [],
        )
        .unwrap();
        c
    }

    fn link(c: &Connection, id: &str, mount_point_id: &str, relative_path: &str) {
        c.execute(
            r#"INSERT INTO "doc_mount_file_links" (id, fileId, mountPointId, relativePath)
               VALUES (?1, 'content-1', ?2, ?3)"#,
            rusqlite::params![id, mount_point_id, relative_path],
        )
        .unwrap();
    }

    fn link_ids(c: &Connection) -> Vec<String> {
        let mut stmt = c
            .prepare("SELECT id FROM doc_mount_file_links ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    }

    /// `!rollMountPointId` — an absent mount point constrains nothing, so the
    /// first non-`photos/` link wins whichever mount it lives in.
    #[test]
    fn an_absent_mount_point_takes_the_first_non_album_link() {
        let c = mount_db();
        link(&c, "album", "vault-1", "photos/kept.webp");
        link(&c, "roll", "somewhere-else", "images/history/a.webp");

        let got = drop_victim_roll_link(&c, "blob-1", None).unwrap();
        assert_eq!(
            got,
            DroppedVictim {
                link_dropped: true,
                bytes_freed: false
            }
        );
        assert_eq!(link_ids(&c), vec!["album".to_string()]);
    }

    /// v4's test is `!rollMountPointId`, a JS truthiness test — so an EMPTY
    /// string reads the same as absent. Unreachable from the pass; pinned so a
    /// later `Some("")` never silently starts constraining.
    #[test]
    fn an_empty_mount_point_reads_the_same_as_an_absent_one() {
        let c = mount_db();
        link(&c, "roll", "somewhere-else", "images/history/a.webp");

        assert_eq!(
            drop_victim_roll_link(&c, "blob-1", Some("")).unwrap(),
            DroppedVictim {
                link_dropped: true,
                bytes_freed: true
            }
        );
        assert!(link_ids(&c).is_empty());
    }

    /// An unknown blob is v4's `if (!blob) return { false, false }` — and it
    /// must not reach the GC at all.
    #[test]
    fn an_unknown_blob_drops_nothing() {
        let c = mount_db();
        link(&c, "roll", "mount-1", "images/history/a.webp");

        assert_eq!(
            drop_victim_roll_link(&c, "no-such-blob", Some("mount-1")).unwrap(),
            DroppedVictim {
                link_dropped: false,
                bytes_freed: false
            }
        );
        assert_eq!(link_ids(&c), vec!["roll".to_string()]);
    }

    /// A victim whose only surviving link is the album copy drops no link and
    /// keeps its bytes — the third of v4's new cases, at unit level.
    #[test]
    fn a_roll_with_only_an_album_link_left_drops_nothing_and_keeps_its_bytes() {
        let c = mount_db();
        link(&c, "album", "vault-1", "photos/kept.webp");

        assert_eq!(
            drop_victim_roll_link(&c, "blob-1", Some("mount-1")).unwrap(),
            DroppedVictim {
                link_dropped: false,
                bytes_freed: false
            }
        );
        assert_eq!(link_ids(&c), vec!["album".to_string()]);
    }
}

#[cfg(test)]
mod report_census_tests {
    //! The census warn's fields and its silence leg — P4.91.
    //!
    //! The order assumed a census capture test already existed; it did not.
    //! Nothing pinned the warn at all, which is how its `fileIds` could reach
    //! `combined.log` as a quoted JSON string for a whole round without a red.
    //!
    //! This is the CORE half of the pin: the field is named `fileIdsJson` and
    //! carries a parseable JSON array of at most 20 ids. The WEB half — that
    //! the layer turns that into `"fileIds":[…]` — is
    //! `quilltap_web::log_file::json_field_tests`.

    use super::*;
    use crate::test_support::captured;

    /// The `files` shape `unexplained_duplicate_keys` reads: id, generationKey,
    /// createdAt.
    fn main_db(rows: &[(&str, &str, &str)]) -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            r#"CREATE TABLE "files" (
                 "id" TEXT PRIMARY KEY,
                 "generationKey" TEXT,
                 "createdAt" TEXT
               );"#,
        )
        .unwrap();
        for (id, key, created) in rows {
            c.execute(
                r#"INSERT INTO "files" (id, generationKey, createdAt) VALUES (?1, ?2, ?3)"#,
                rusqlite::params![id, key, created],
            )
            .unwrap();
        }
        c
    }

    /// The silence leg: the invariant holding logs NOTHING. v4's `reportCensus`
    /// returns early on an empty list, and a census that narrates every clean
    /// pass is a census nobody reads.
    #[test]
    fn a_held_invariant_says_nothing() {
        let c = main_db(&[
            ("solo", "key-a", "2026-01-01T00:00:00.000Z"),
            ("other", "key-b", "2026-01-01T00:00:00.000Z"),
        ]);
        let lines = captured(|| {
            report_census(&c, &[]).unwrap();
        });
        assert!(
            lines.is_empty(),
            "nothing to report, nothing said: {lines:?}"
        );
    }

    /// The warn fires with v4's sentence, v4's count, and `fileIdsJson`
    /// carrying a parseable ARRAY — not a Rust `Debug` rendering, and not a
    /// value the file layer would have to quote.
    #[test]
    fn an_unexplained_duplicate_reports_its_ids_as_a_json_array() {
        // Two rows share `key-a`; the newest is the survivor and needs no
        // excuse, so `older` is the one the census cannot account for.
        let c = main_db(&[
            ("newer", "key-a", "2026-01-02T00:00:00.000Z"),
            ("older", "key-a", "2026-01-01T00:00:00.000Z"),
        ]);
        let lines = captured(|| {
            report_census(&c, &[]).unwrap();
        });

        assert_eq!(lines.len(), 1, "one warn, not one per row: {lines:?}");
        let line = &lines[0];
        assert!(
            line.starts_with("WARN quilltap::migration"),
            "v4 logs this at warn on the migration target: {line}"
        );
        assert!(
            line.contains(
                "Avatar rolls share a generation key this pass did not choose to double up"
            ),
            "v4's sentence, byte for byte: {line}"
        );
        assert!(line.contains("unexplainedCount=1"), "{line}");
        // NOT a `contains("fileIdsJson")` — the field NAME being present says
        // nothing about its payload, and an assertion that cannot fail is not
        // an assertion. The name is proven by `extract_field` panicking without
        // it; the payload is proven by parsing it.
        assert!(
            !line.contains("fileIds="),
            "the unsuffixed spelling is what reached combined.log as a quoted \
             string before P4.91; it must not come back: {line}"
        );

        // The payload is real JSON — which is the whole premise of the `…Json`
        // convention the file layer re-parses.
        let payload = extract_field(line, "fileIdsJson");
        let parsed: Value = serde_json::from_str(&payload)
            .unwrap_or_else(|e| panic!("fileIdsJson must be parseable JSON ({e}): {payload:?}"));
        assert_eq!(parsed, serde_json::json!(["older"]));
    }

    /// v4 slices to twenty. The array the file record carries is bounded, so a
    /// pathological instance cannot write an unbounded line.
    #[test]
    fn the_array_is_capped_at_twenty() {
        let mut rows: Vec<(String, String, String)> = Vec::new();
        // One shared key with 25 rows: the newest survives, 24 are unexplained.
        for i in 0..25 {
            rows.push((
                format!("f{i:02}"),
                "key-a".to_string(),
                format!("2026-01-{:02}T00:00:00.000Z", i + 1),
            ));
        }
        let borrowed: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();
        let c = main_db(&borrowed);

        let lines = captured(|| {
            report_census(&c, &[]).unwrap();
        });
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("unexplainedCount=24"), "{}", lines[0]);

        let payload = extract_field(&lines[0], "fileIdsJson");
        let parsed: Value = serde_json::from_str(&payload).expect("parseable");
        assert_eq!(
            parsed.as_array().unwrap().len(),
            20,
            "v4 slices to twenty; the count says 24: {payload}"
        );
    }

    /// Pull one field's value out of the capture layer's `name=value` rendering
    /// and undo the `Debug` quoting the rig applies to strings.
    fn extract_field(line: &str, name: &str) -> String {
        let needle = format!("{name}=");
        let start = line
            .find(&needle)
            .unwrap_or_else(|| panic!("{name} in {line}"))
            + needle.len();
        let rest = &line[start..];
        // The rig renders a `&str` field through `Debug`, so the value is a
        // quoted, escaped Rust string literal. Parse it back with serde.
        if rest.starts_with('"') {
            let end = {
                let bytes = rest.as_bytes();
                let mut i = 1;
                loop {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'"' => break i + 1,
                        _ => i += 1,
                    }
                }
            };
            serde_json::from_str::<String>(&rest[..end]).expect("a Debug-quoted string")
        } else {
            rest.split_whitespace().next().unwrap().to_string()
        }
    }
}
