//! The `/api/v1/images` COLLECTION surface (P4.73) — v4
//! `app/api/v1/images/route.ts` (508 lines) plus the `[id]` DELETE arm.
//!
//! v5 shipped only `GET /api/v1/images/{id}` (P4.9a2, `api/photos.rs`); the
//! collection endpoint — the list, the upload / import-from-URL POST, the
//! `?action=generate` synchronous generate, and the orphan-aware DELETE — had
//! no counterpart at all. Three SPA files named the hole by name
//! (`images/image-gallery.ts`, `screens/profile/avatar-picker.ts`,
//! `chat/cast/create-npc-dialog.ts`) and the DELETE answered a loud refusal.
//!
//! ## What v4 does that is easy to get wrong
//!
//! * **The POST is the FIRST dispatch shape.** `route.ts:161-171` reads
//!   `getActionParam` and runs the generate leg only on the literal
//!   `'generate'`; *every other value — unknown, empty, absent — falls through
//!   to the upload/import leg.* There is no `withActionDispatch`, so there is
//!   no unknown-action envelope. `?action=bogus` UPLOADS.
//! * **The list's `tagType` is derived, never stored.** A tag id that is a
//!   character id reads `CHARACTER`, everything else `THEME` — so this route
//!   can never emit `CHAT`, even though the type union contains it and the
//!   `[id]` add-tag action only accepts `CHAT`/`THEME`. Carried as a quirk.
//! * **The list drops rows that fail `FileEntrySchema`.** v4's
//!   `findByCategory` → `findByFilter` validates each row and `.filter`s the
//!   failures out (`base.repository.ts:277-285`). `sha256` is
//!   `z.string().length(64)` and BOTH `linkedTo` and `tags` are
//!   `z.array(z.uuid())` — so a row whose tag array carries the raw non-string
//!   `tagId` v4's schemaless upload leg writes (the P4.62(a) raw carry) becomes
//!   invisible to its own list. Reproduced in [`file_entry_row_is_valid`].
//! * **The generate leg is its OWN implementation**, not a call into the
//!   Salon's `generate_image` tool: its Concierge gate is `scanImagePrompts`
//!   with no chat, its reroute picks the first `isDangerousCompatible` profile
//!   rather than consulting the Concierge desk, and it resolves NO orientation
//!   (`params_builder`'s `orientation: None` arm exists for this caller).
//! * **The route-level timestamps are the route's own.** The upload / import
//!   receipts stamp `new Date().toISOString()` at `route.ts:443-444` /
//!   `:494-495`, NOT the row's — so they are minted values, normalized in the
//!   differential.
//!
//! ## The pixel codec
//!
//! v4 transcodes on ingest through sharp, twice: `createFile`'s explicit
//! `convertToWebP` (quality 90) and then the uploads bridge's own
//! `transcodeToWebP` (quality 85), the second a no-op on the already-WebP
//! bytes of the first. v5 threads the HOST codec
//! ([`crate::api::engine::Engine::qtap_pixel_codec`]) into both, so these arms
//! transcode for real — D19 policy parity, not byte parity with sharp.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::db::files::FilesRepository;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::file_storage::{PixelCodec, StorageBackend};

use super::types::{ErrorKind, Response};

// ===========================================================================
// Shared vocabulary
// ===========================================================================

/// v4's `source` → the legacy wire word (`route.ts:117-119`). Anything the
/// enum does not name reads `'upload'`, v4's own final `: 'upload'`.
pub(crate) fn legacy_source(source: &str) -> &'static str {
    match source {
        "UPLOADED" => "upload",
        "IMPORTED" => "import",
        "GENERATED" => "generated",
        _ => "upload",
    }
}

/// The Zod `uuid()` predicate — ONE home, `services::file_storage::is_zod_uuid`
/// (a fourth transcription lived here until the follow-ups-round-2 §3 review
/// folded it; the `api/settings.rs` copy is the remaining consolidation
/// candidate, recorded in the lane record).
use crate::services::file_storage::is_zod_uuid;

/// A stored JSON array column read back as raw elements (v4 hydrates the cell
/// with `JSON.parse`, so a number stays a number).
fn raw_json_array(cell: Option<&str>) -> Vec<Value> {
    cell.filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
        .unwrap_or_default()
}

/// v4's `FileEntrySchema` acceptance, restricted to the arms a `files` row can
/// actually fail on disk: `sha256: z.string().length(64)` plus
/// `linkedTo`/`tags: z.array(z.uuid()).default([])`. A row that fails reads as
/// ABSENT out of `findByFilter`, so the list silently omits it
/// (`base.repository.ts:277-285`). The columns the schema types as plain
/// strings/enums cannot fail from a row this port writes.
fn file_entry_row_is_valid(sha256: Option<&str>, linked_to: &[Value], tags: &[Value]) -> bool {
    if !matches!(sha256, Some(s) if s.len() == 64) {
        return false;
    }
    let all_uuid = |arr: &[Value]| arr.iter().all(|v| v.as_str().is_some_and(is_zod_uuid));
    all_uuid(linked_to) && all_uuid(tags)
}

// ===========================================================================
// GET /api/v1/images — the tagged list
// ===========================================================================

/// v4 `GET /api/v1/images` (`route.ts:69-153`).
///
/// `repos.files.findByCategory('IMAGE')` filtered to the user, optionally
/// filtered to a `tagId` MEMBERSHIP (`img.tags.includes(tagId)`), sorted
/// `createdAt` DESC, then projected into the legacy shape. The key order of
/// each element is the comparand (memory note `json-column-key-order`).
///
/// The outer catch is `serverError('Failed to fetch images')` after an
/// `[Images v1] Error fetching images` log.
pub fn images_list(db: &Db, user_id: &str, tag_id: Option<&str>) -> Response {
    let read = db.read_main(|main| db.read_mount_index(|mount| list(main, mount, user_id, tag_id)));
    match read {
        Ok(resp) => resp,
        Err(_) => Response::error(ErrorKind::Internal, "Failed to fetch images"),
    }
}

/// One `files` row as the list route reads it.
struct ListRow {
    id: String,
    sha256: Option<String>,
    original_filename: String,
    mime_type: String,
    size: Value,
    width: Value,
    height: Value,
    source: String,
    generation_prompt: Option<String>,
    generation_model: Option<String>,
    description: Option<String>,
    linked_to: Vec<Value>,
    tags: Vec<Value>,
    storage_key: Option<String>,
    created_at: String,
    updated_at: String,
}

/// A SQLite numeric cell → the JSON number JS would serialize (`9.0` → `9`).
/// `size`/`width`/`height` carry REAL affinity, so an integer-valued cell is a
/// float on disk and better-sqlite3 hands v4 a JS `Number`.
fn numeric_cell(row: &rusqlite::Row<'_>, idx: usize) -> Result<Value, rusqlite::Error> {
    Ok(match row.get_ref(idx)? {
        rusqlite::types::ValueRef::Null => Value::Null,
        rusqlite::types::ValueRef::Integer(i) => Value::from(i),
        rusqlite::types::ValueRef::Real(f) => crate::db::js_number_to_json(f),
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                idx,
                other.data_type(),
                "numeric column expected".into(),
            ))
        }
    })
}

fn list(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    tag_id: Option<&str>,
) -> Result<Response, DbError> {
    // v4 `findByCategory('IMAGE')` then `.filter(img => img.userId === user.id)`.
    // The two collapse into one predicate; v5 reads are single-user-scoped
    // anyway (`db/files.rs:707`'s note).
    let mut stmt = main.prepare(
        "SELECT id, sha256, originalFilename, mimeType, size, width, height, source, \
                generationPrompt, generationModel, description, linkedTo, tags, storageKey, \
                createdAt, updatedAt \
         FROM files WHERE userId = ?1 AND category = 'IMAGE'",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![user_id], |row| {
            let linked_to_cell: Option<String> = row.get(11)?;
            let tags_cell: Option<String> = row.get(12)?;
            Ok(ListRow {
                id: row.get(0)?,
                sha256: row.get(1)?,
                original_filename: row.get(2)?,
                mime_type: row.get(3)?,
                size: numeric_cell(row, 4)?,
                width: numeric_cell(row, 5)?,
                height: numeric_cell(row, 6)?,
                source: row.get(7)?,
                generation_prompt: row.get(8)?,
                generation_model: row.get(9)?,
                description: row.get(10)?,
                linked_to: raw_json_array(linked_to_cell.as_deref()),
                tags: raw_json_array(tags_cell.as_deref()),
                storage_key: row.get(13)?,
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // The repository's row-level schema drop, BEFORE any route filtering.
    let mut images: Vec<ListRow> = rows
        .into_iter()
        .filter(|r| file_entry_row_is_valid(r.sha256.as_deref(), &r.linked_to, &r.tags))
        .collect();

    // v4 `if (tagId) images = images.filter(img => img.tags.includes(tagId))`
    // — a JS `Array.includes` over the hydrated tag array, so it is a strict
    // equality against each element; a numeric element never matches a string
    // parameter.
    if let Some(want) = tag_id.filter(|s| !s.is_empty()) {
        images.retain(|r| r.tags.iter().any(|t| t.as_str() == Some(want)));
    }

    // v4 `images.sort((a, b) => new Date(b.createdAt).getTime() -
    // new Date(a.createdAt).getTime())` — a stable DESC sort by `createdAt`
    // (uniform ISO-8601 → lexical == chronological; `Array.prototype.sort` is
    // stable in V8, so ties keep the repository's rowid order). The same
    // reduction `api/files.rs:275-278` carries.
    images.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let characters = crate::db::characters_read::find_by_user_id(main, mount, user_id)?;
    let character_ids: std::collections::HashSet<&str> =
        characters.iter().filter_map(|c| c["id"].as_str()).collect();

    let data: Vec<Value> = images
        .iter()
        .map(|img| {
            // v4 `route.ts:92-94` / `:97-105` — the two usage counts.
            let characters_using_as_default = characters
                .iter()
                .filter(|c| c["defaultImageId"].as_str() == Some(img.id.as_str()))
                .count();
            let chat_avatar_overrides: usize = characters
                .iter()
                .map(|c| {
                    c["avatarOverrides"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter(|o| o["imageId"].as_str() == Some(img.id.as_str()))
                                .count()
                        })
                        .unwrap_or(0)
                })
                .sum();

            // v4 `route.ts:108-114` — CHARACTER when the tag id is a character
            // id, else THEME. Never CHAT: the quirk is carried.
            let tags: Vec<Value> = img
                .tags
                .iter()
                .map(|t| {
                    let is_character = t.as_str().is_some_and(|s| character_ids.contains(s));
                    json!({
                        "tagId": t,
                        "tagType": if is_character { "CHARACTER" } else { "THEME" },
                    })
                })
                .collect();

            // v4 `route.ts:122` — the API path when the bytes are stored,
            // otherwise the bare original filename (a legacy row).
            let filepath = match img.storage_key.as_deref().filter(|s| !s.is_empty()) {
                Some(_) => format!("/api/v1/files/{}", img.id),
                None => img.original_filename.clone(),
            };

            // v4 `route.ts:128` — the IMPORTED row's `description` doubles as
            // the source URL ("Imported from <url>"); every other source is null.
            let url = if img.source == "IMPORTED" {
                // A NULL `description` on an IMPORTED row makes v4's ternary
                // yield `undefined`, which `JSON.stringify` DROPS. Every row the
                // import leg writes carries the sentence, so this is the legacy
                // arm; it is spelled out rather than collapsed to null.
                match &img.description {
                    Some(d) => Value::String(d.clone()),
                    None => Value::Null,
                }
            } else {
                Value::Null
            };

            // KEY ORDER is the comparand. The nullable-optional columns are
            // OMITTED when NULL, not emitted as null: v4's repository hydrates a
            // NULL cell to `undefined` and `JSON.stringify` drops the key (the
            // P4.6p reads-omit-null rule; MEASURED against v4's own output for
            // `width`/`height` on the storage-key-less row and for the two
            // generation columns on every non-GENERATED row). `url` is the one
            // deliberate explicit null — it comes from the route's ternary, not
            // from a column.
            let mut m = Map::new();
            m.insert("id".into(), json!(img.id));
            m.insert("userId".into(), json!(user_id));
            m.insert("filename".into(), json!(img.original_filename));
            m.insert("filepath".into(), json!(filepath));
            m.insert("url".into(), url);
            m.insert("mimeType".into(), json!(img.mime_type));
            m.insert("size".into(), img.size.clone());
            if !img.width.is_null() {
                m.insert("width".into(), img.width.clone());
            }
            if !img.height.is_null() {
                m.insert("height".into(), img.height.clone());
            }
            m.insert("source".into(), json!(legacy_source(&img.source)));
            if let Some(p) = &img.generation_prompt {
                m.insert("generationPrompt".into(), json!(p));
            }
            if let Some(gm) = &img.generation_model {
                m.insert("generationModel".into(), json!(gm));
            }
            m.insert("createdAt".into(), json!(img.created_at));
            m.insert("updatedAt".into(), json!(img.updated_at));
            m.insert("tags".into(), Value::Array(tags));
            m.insert(
                "_count".into(),
                json!({
                    "charactersUsingAsDefault": characters_using_as_default,
                    "chatAvatarOverrides": chat_avatar_overrides,
                }),
            );
            Value::Object(m)
        })
        .collect();

    Ok(Response::Images(json!({ "data": data })))
}

// ===========================================================================
// Shared: the addTag loop both ingest legs run
// ===========================================================================

/// v4 `for (const tag of tags) await repos.files.addTag(imageData.id, tag.tagId)`
/// (`route.ts:421-425` / `:477-481`). Runs AFTER the ingest, so a tag id the
/// ingest already inherited is a no-op.
///
/// v4's `addTag` is `this.update(id, { tags: [...tags, tagId] })`
/// (`base.repository.ts:579-596`), and `_update` re-validates the row against
/// `FileEntrySchema` — so a non-string or non-UUID id THROWS out of the loop
/// (the middleware's `Validation error` 400). Every id reaching this loop has
/// already passed `create_file_conns`' own refusal (the create arm after the
/// bridge write, the dedup arm before any write), so the refusal here is a
/// guard that names the invariant rather than a live arm. It replaces a raw
/// `UPDATE files SET tags = …` that pushed the value as-is — which was a claim
/// about v4 that was false (the §3 review of the follow-ups round 2).
pub(crate) fn add_raw_tags(
    main: &rusqlite::Connection,
    file_id: &str,
    tag_ids: &[Value],
) -> Result<(), DbError> {
    for tag in tag_ids {
        let Some(s) = tag.as_str().filter(|s| is_zod_uuid(s)) else {
            return Err(DbError::Internal(format!(
                "files.addTag: `{tag}` is not a UUID"
            )));
        };
        FilesRepository::new(main).add_tag(file_id, s)?;
    }
    Ok(())
}

// ===========================================================================
// The ingest legs' shared tail (both answer the same receipt shape)
// ===========================================================================

/// v4's upload / import receipt (`route.ts:428-442` / `:486-500`). `createdAt`
/// / `updatedAt` are the ROUTE's `new Date().toISOString()`, not the row's.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ingest_receipt(
    user_id: &str,
    entry: &crate::db::files::FileEntry,
    url: Value,
    source: &str,
    tags: Option<&[Value]>,
    now_iso: &str,
) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), json!(entry.id));
    m.insert("userId".into(), json!(user_id));
    m.insert("filename".into(), json!(entry.original_filename));
    m.insert(
        "filepath".into(),
        json!(format!("/api/v1/files/{}", entry.id)),
    );
    m.insert("url".into(), url);
    m.insert("mimeType".into(), json!(entry.mime_type));
    m.insert("size".into(), json!(entry.size));
    // v4's `ImageUploadResult` maps `width: fileEntry.width || undefined`, and
    // `JSON.stringify` drops an undefined key — so a NULL/zero dimension is
    // ABSENT from the receipt, not null.
    if let Some(w) = entry.width.filter(|w| *w != 0) {
        m.insert("width".into(), json!(w));
    }
    if let Some(h) = entry.height.filter(|h| *h != 0) {
        m.insert("height".into(), json!(h));
    }
    m.insert("source".into(), json!(source));
    m.insert("createdAt".into(), json!(now_iso));
    m.insert("updatedAt".into(), json!(now_iso));
    m.insert(
        "tags".into(),
        tags.map(|t| Value::Array(t.to_vec()))
            .unwrap_or_else(|| Value::Array(vec![])),
    );
    Value::Object(m)
}

/// The ingest arms' shared dependencies — the host codec and the byte store.
pub struct IngestDeps {
    pub codec: Arc<dyn PixelCodec>,
    pub backend: Arc<dyn StorageBackend>,
}

/// v4's `tags.map(t => t.tagId)` with NO schema (`route.ts:415`/`:471`) — the
/// raw property value, whatever its type, and `undefined` (a missing key)
/// serialized as `null`. P4.62(a): a `Vec<String>` here would silently drop
/// `[{"tagId": 5}]` and `[{}]`, which v4 carries into both `linkedTo` and the
/// `tags` column.
pub(crate) fn raw_tag_ids(tags: Option<&[Value]>) -> Vec<Value> {
    tags.map(|arr| {
        arr.iter()
            .map(|t| t.get("tagId").cloned().unwrap_or(Value::Null))
            .collect()
    })
    .unwrap_or_default()
}

// ===========================================================================
// DELETE /api/v1/images/{id} — the orphan-aware, in-use-refusing delete
// ===========================================================================

/// v4 `DELETE /api/v1/images/[id]` (`[id]/route.ts:134-237`). Replaces the
/// P4.9a2 loud refusal (`photos_routes::image_delete_not_available`).
///
/// The shape that matters is the ORDER of its two gates. v4 probes storage
/// FIRST and counts usages second, then:
///
/// * bytes GONE **and** in use → the references are cleaned up and the row is
///   deleted anyway (an image nobody can display is not worth protecting);
/// * bytes PRESENT and in use → `badRequest('Image is in use', …)`;
/// * otherwise → delete.
///
/// ⚠ `associations.chatAvatarOverrides` in the refusal body counts **characters
/// that have any override**, because v4 reduces to one entry per character
/// (`[id]/route.ts:162-165`). The LIST route's `_count.chatAvatarOverrides`
/// counts the individual OVERRIDES. Two different numbers under the same name;
/// the fixture's `CHAR_OVR` carries two overrides so the differential can tell
/// them apart (1 vs 2).
pub async fn image_delete(
    db: &Db,
    backend: Arc<dyn StorageBackend>,
    user_id: &str,
    id: &str,
) -> Response {
    let user_id = user_id.to_string();
    let id = id.to_string();
    match db
        .write(move |ws| {
            let main = ws.main().connection();
            let mount = ws
                .mount_index()
                .ok_or_else(|| {
                    DbError::Internal("image delete requires the mount-index database".to_string())
                })?
                .connection();
            delete_conns(main, mount, backend.as_ref(), &user_id, &id)
        })
        .await
    {
        Ok(resp) => resp,
        // v4's outer catch: `serverError('Failed to delete image')`.
        Err(_) => Response::error(ErrorKind::Internal, "Failed to delete image"),
    }
}

/// v4 `validateCharacterArchivePatch` (`characters.repository.ts:42`), at the
/// only site this family reaches it: the orphan cleanup's two
/// `repos.characters.update` loops.
///
/// P4.76 (the P4.73 unification review's item (a)). The cleanup used to push
/// raw `UPDATE characters SET …` where v4 goes through the repository, whose
/// guard THROWS `CharacterArchivedError` for any patch to an archived character
/// other than the single-key unarchive — and `update` takes `safeQuery`'s
/// NO-fallback overload, so the throw reaches the route's outer catch and the
/// request answers 500 `Failed to delete image`.
///
/// ⚠ The updates already committed STAY committed on both sides: v4's
/// better-sqlite3 autocommits per statement, and `Db::write` opens no
/// transaction of its own. So the peer character's cleared `defaultImageId`
/// survives the refusal while its `avatarOverrides` — the loop that never
/// ran — still point at the image. The fixture's `CHAR_ARCH_PEER` /
/// `CHAR_ARCHIVED` pair exists to make exactly that visible.
fn refuse_if_archived(main: &rusqlite::Connection, character_id: &str) -> Result<(), DbError> {
    let archived_at: Option<String> = main
        .query_row(
            "SELECT archivedAt FROM characters WHERE id = ?1",
            rusqlite::params![character_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    // v4 `if (!existing?.archivedAt) return` — JS-falsy, so an empty string is
    // not archived either.
    if archived_at.is_some_and(|s| !s.is_empty()) {
        return Err(DbError::Internal(format!(
            "Character {character_id} is archived and cannot be modified"
        )));
    }
    Ok(())
}

fn delete_conns(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    backend: &dyn StorageBackend,
    user_id: &str,
    id: &str,
) -> Result<Response, DbError> {
    let files = FilesRepository::new(main);

    // v4 `if (!image) return notFound('Image')` (`:139-141`).
    let Some(image) = files.find_by_id(id)? else {
        return Ok(Response::error(ErrorKind::NotFound, "Image not found"));
    };

    // v4 `if (category !== 'IMAGE' && category !== 'AVATAR')` (`:144-146`).
    if image.category != "IMAGE" && image.category != "AVATAR" {
        return Ok(Response::error(ErrorKind::NotFound, "Image not found"));
    }

    // v4 `:149-156` — the storage-existence probe, guarded on `storageKey` and
    // wrapped in a try whose catch leaves `fileExists` FALSE. `file_exists_conn`
    // already folds every error to false, and a key-less row never probes.
    let file_exists = match image.storage_key.as_deref().filter(|s| !s.is_empty()) {
        Some(_) => crate::services::file_storage::file_exists_conn(mount, backend, &image),
        None => false,
    };

    // v4 `:159-165` — the two usage sets. `chatAvatarOverrides` is a list of
    // CHARACTERS (one entry each), not of overrides.
    let default_chars = crate::db::characters_read::find_by_default_image_id(main, mount, id)?;
    let override_chars =
        crate::db::characters_read::find_by_avatar_override_image_id(main, mount, id)?;
    let is_in_use = !default_chars.is_empty() || !override_chars.is_empty();

    if !file_exists && is_in_use {
        // v4 `:173-192` — the bytes are gone, so clear the references and let
        // the delete proceed.
        tracing::info!(
            imageId = id,
            charactersUsingAsDefault = default_chars.len(),
            chatAvatarOverrides = override_chars.len(),
            "[Images v1] Cleaning up references to orphaned image"
        );
        for c in &default_chars {
            if let Some(cid) = c.get("id").and_then(Value::as_str) {
                refuse_if_archived(main, cid)?;
                main.execute(
                    "UPDATE characters SET defaultImageId = NULL, updatedAt = ?1 WHERE id = ?2",
                    rusqlite::params![crate::clock::now_iso(), cid],
                )?;
            }
        }
        for c in &override_chars {
            let Some(cid) = c.get("id").and_then(Value::as_str) else {
                continue;
            };
            refuse_if_archived(main, cid)?;
            // Read the RAW stored JSON so the rewrite preserves the exact shape
            // (the `api/files.rs:702-720` dissociate precedent).
            let raw: Option<String> = main
                .query_row(
                    "SELECT avatarOverrides FROM characters WHERE id = ?1",
                    rusqlite::params![cid],
                    |r| r.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten();
            let mut overrides: Vec<Value> = raw
                .as_deref()
                .and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
                .unwrap_or_default();
            overrides.retain(|o| o.get("imageId").and_then(Value::as_str) != Some(id));
            let json_text = serde_json::to_string(&overrides)
                .map_err(|e| DbError::Internal(format!("avatarOverrides serialize: {e}")))?;
            main.execute(
                "UPDATE characters SET avatarOverrides = ?1, updatedAt = ?2 WHERE id = ?3",
                rusqlite::params![json_text, crate::clock::now_iso(), cid],
            )?;
        }
    } else if is_in_use {
        // v4 `:193-203` — the bytes are there and something uses them.
        return Ok(Response::bad_request_with_details(
            "Image is in use",
            json!({
                "message": "This image is currently being used as an avatar or in chat overrides. \
                            Please remove all usages before deleting.",
                "code": "IMAGE_IN_USE",
                "associations": {
                    "charactersUsingAsDefault": default_chars.len(),
                    "chatAvatarOverrides": override_chars.len(),
                },
            }),
        ));
    }

    // v4 `:206-217` — a storage-delete failure is WARNED and swallowed; the
    // metadata delete proceeds regardless.
    if let Some(key) = image.storage_key.as_deref().filter(|s| !s.is_empty()) {
        if let Err(e) = crate::services::file_storage::delete_file_conn(mount, backend, &image) {
            tracing::warn!(
                imageId = id,
                storageKey = key,
                error = %e,
                "[Images v1] Failed to delete from storage"
            );
        }
    }

    // v4 `:220-225` — a repository delete that reports no row is a 500.
    if !files.delete(id)? {
        tracing::warn!(imageId = id, "[Images v1] Failed to delete file metadata");
        return Ok(Response::error(
            ErrorKind::Internal,
            "Failed to delete image",
        ));
    }

    tracing::info!(
        imageId = id,
        filename = image.original_filename,
        userId = user_id,
        "[Images v1] Image deleted successfully"
    );
    Ok(Response::Images(json!({ "success": true })))
}

// ===========================================================================
// The import-from-URL fetch seam
// ===========================================================================

/// What v4's `fetch(url)` gives `importImageFromUrl` (`images-v2.ts:269-282`):
/// the status, its `statusText` (which the `!ok` throw interpolates), the
/// `content-type` header, and the body BYTES. Core has no HTTP, so this rides
/// the host — the P4.D138 HuggingFace precedent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFetchResponse {
    pub status: u16,
    /// v4 `response.statusText`.
    pub status_text: String,
    /// v4 `response.headers.get('content-type') || ''`.
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// The one outbound call the import leg makes. `Err` is v4's THROWN fetch (a
/// network failure), which its route lets escape to the middleware's 500;
/// every HTTP status — including a 404 — is an `Ok` the caller gates on.
pub trait ImageImportFetch: Send + Sync {
    fn get(
        &self,
        url: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ImageFetchResponse, String>> + Send + '_>,
    >;
}

/// A type-erased [`ImageImportFetch`] (the `ErasedLoraMetadata` shape).
#[derive(Clone)]
pub struct ErasedImageImportFetch(Arc<dyn ImageImportFetch>);

impl ErasedImageImportFetch {
    pub fn new<T: ImageImportFetch + 'static>(inner: T) -> Self {
        Self(Arc::new(inner))
    }

    pub async fn get(&self, url: &str) -> Result<ImageFetchResponse, String> {
        self.0.get(url).await
    }
}

// ===========================================================================
// POST /api/v1/images — upload (multipart) and import (JSON)
// ===========================================================================

/// v4 `ALLOWED_IMAGE_TYPES.join(', ')` — the exact interpolation both refusal
/// sentences carry.
fn allowed_types_joined() -> String {
    crate::services::file_storage::ALLOWED_IMAGE_TYPES.join(", ")
}

/// v4 `validateImageFile` (`images-v2.ts:218-226`). Both arms THROW, and
/// neither route leg wraps them, so the sentences reach the middleware's catch
/// and the client sees a flat `500 {"error": "Internal server error"}` — they
/// are pinned by [`validate_image_file_tests`] here, not by a corpus row,
/// because the wire cannot show them (v4 logs them as the unhandled error).
fn validate_image_file(content_type: &str, size: usize) -> Result<(), String> {
    if !crate::services::file_storage::ALLOWED_IMAGE_TYPES.contains(&content_type) {
        return Err(format!(
            "Invalid file type. Allowed types: {}",
            allowed_types_joined()
        ));
    }
    if size as i64 > crate::services::file_storage::MAX_IMAGE_FILE_SIZE {
        return Err(format!(
            "File size exceeds maximum allowed size of {} MB",
            crate::services::file_storage::MAX_IMAGE_FILE_SIZE / 1024 / 1024
        ));
    }
    Ok(())
}

/// v4's `handleUploadOrImport` multipart leg (`route.ts:449-506`).
///
/// `uploadImage` → `createFile` (already ported as
/// [`crate::services::file_storage::create_file_conns`], differential-proven by
/// `image_ingest_tier2_equivalence`), then the per-tag `addTag` loop, then the
/// route's own receipt whose `createdAt`/`updatedAt` are minted AT THE ROUTE.
pub async fn image_upload(
    db: &Db,
    deps: IngestDeps,
    user_id: &str,
    filename: &str,
    content_type: &str,
    bytes: Vec<u8>,
    tags: Option<Vec<Value>>,
) -> Response {
    ingest(
        db,
        deps,
        user_id,
        IngestRequest {
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            bytes,
            source: "UPLOADED",
            description: None,
            url: Value::Null,
            wire_source: "upload",
            // v4 validates the FILE before reading its bytes; the import leg
            // gates on the response instead (different sentences, same 500).
            validate: true,
        },
        tags,
    )
    .await
}

/// v4's `handleUploadOrImport` JSON leg (`route.ts:410-447`) →
/// `importImageFromUrl` (`images-v2.ts:267-321`).
/// v4 `importFromUrlSchema` (`route.ts:33-42`) — `url: z.url()` plus the
/// optional `tags` array of `{tagType: enum, tagId: string}`. Unlike the
/// multipart leg, this one IS schema-checked, so a wrong-typed `tagId` refuses
/// HERE rather than reaching the row write. That asymmetry between two legs of
/// one route is v4's, measured against its real handler.
///
/// Validated in the HANDLER, not at the web edge, so both transports answer the
/// same bytes from one place (the `ChatCreate` trio's lesson).
fn parse_import_body(
    url: Option<&Value>,
    tags: Option<&Value>,
) -> Result<(String, Option<Vec<Value>>), Response> {
    let bad = || Response::error(ErrorKind::BadRequest, "Validation error");
    let url = url
        .and_then(Value::as_str)
        .filter(|s| zod_url_ok(s))
        .ok_or_else(bad)?;
    let tags = match tags {
        None | Some(Value::Null) if tags.is_none() => None,
        None => None,
        Some(v) => {
            // `.optional()` is not `.nullable()`: an explicit null REFUSES.
            let arr = v.as_array().ok_or_else(bad)?;
            for t in arr {
                let o = t.as_object().ok_or_else(bad)?;
                match o.get("tagType").and_then(Value::as_str) {
                    Some("CHARACTER" | "CHAT" | "THEME") => {}
                    _ => return Err(bad()),
                }
                if o.get("tagId").and_then(Value::as_str).is_none() {
                    return Err(bad());
                }
            }
            // v4 echoes the PARSED array (`route.ts:445` `tags: tags || []` is
            // the `importFromUrlSchema.parse` result): unknown keys are
            // stripped and each object is rebuilt in schema key order,
            // `tagType` then `tagId`. Echoing the raw objects would diverge on
            // key order and content the moment a client sent either (the §3
            // review of the follow-ups round 2; `import_tags_extra_keys`).
            Some(
                arr.iter()
                    .map(|t| {
                        json!({
                            "tagType": t["tagType"].clone(),
                            "tagId": t["tagId"].clone(),
                        })
                    })
                    .collect(),
            )
        }
    };
    Ok((url.to_string(), tags))
}

/// WHATWG's six special schemes. They are the ones that REQUIRE a host (all but
/// `file`, which permits an empty one); every other scheme may carry an opaque
/// path instead of an authority.
const SPECIAL_SCHEMES: &[&str] = &["http", "https", "ws", "wss", "ftp", "file"];

/// A scheme, lowercased, and the rest of the input — or `None` when the string
/// has no scheme at all (WHATWG: `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
/// then `:`), which is what makes `new URL('not-a-url')` throw.
fn split_scheme(s: &str) -> Option<(String, &str)> {
    let colon = s.find(':')?;
    let scheme = &s[..colon];
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), &s[colon + 1..]))
}

/// Zod 4's bare `z.url()` — which, MEASURED against the installed zod
/// (`zod/v4/core/schemas.cjs:$ZodURL`), is exactly "`new URL(value.trim())` does
/// not throw". The `://` guard two lines above it in that file applies ONLY when
/// the schema carries the `httpProtocol` constraint, which
/// `importFromUrlSchema`'s `z.url()` does not.
///
/// **This used to require `://` and a non-empty authority, and that was wrong**:
/// v4 accepts `mailto:someone@example.invalid` and
/// `data:image/png;base64,…` and proceeds to fetch them (P4.76, the P4.73
/// unification review's item (b) — measured, then pinned by the two
/// `import_*_scheme` corpus rows).
///
/// **Scope, said out loud.** Reproduced: the scheme grammar; the special-scheme
/// host requirement (`file` excepted, which permits an empty host); the
/// non-special opaque-path form, which needs no authority at all; and the
/// slash-tolerance that lets a special scheme reach its authority through any
/// run of `/` or `\`. NOT reproduced: IDNA, percent-decoding of the host, the
/// forbidden-code-point set, and port validity — all of which can only make v4
/// THROW where this accepts, on inputs no client sends and the corpus does not
/// carry. The narrower half of that gap is the one that mattered and is closed.
fn zod_url_ok(s: &str) -> bool {
    // `payload.value.trim()` — JS trim, which is the whitespace set `js_trim`
    // already carries.
    let s = crate::jsstr::js_trim(s);
    let Some((scheme, rest)) = split_scheme(s) else {
        return false;
    };
    if !SPECIAL_SCHEMES.contains(&scheme.as_str()) {
        // A non-special scheme parses with an opaque path (`mailto:x`), with an
        // authority (`foo://h/p`) or with an absolute path (`foo:/p`) — every
        // form the constructor accepts.
        return true;
    }
    // A special scheme reaches its authority through ANY run of `/` or `\`,
    // including none at all (`http:example.com` parses, host `example.com`).
    let after = rest.trim_start_matches(['/', '\\']);
    let host_end = after.find(['/', '\\', '?', '#']).unwrap_or(after.len());
    // `file:` is the one special scheme whose host may be empty.
    scheme == "file" || !after[..host_end].is_empty()
}

pub async fn image_import_from_url(
    db: &Db,
    deps: IngestDeps,
    fetch: &ErasedImageImportFetch,
    user_id: &str,
    url_raw: Option<&Value>,
    tags_raw: Option<&Value>,
) -> Response {
    let (url, tags) = match parse_import_body(url_raw, tags_raw) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let url = url.as_str();
    // v4 `const response = await fetch(url)` — a THROWN fetch escapes to the
    // middleware's catch, exactly as every other sentence in this leg does.
    let resp = match fetch.get(url).await {
        Ok(r) => r,
        Err(_) => return internal_error(),
    };
    // v4 `if (!response.ok)` — `Response.ok` is `status` in [200, 300).
    if !(200..300).contains(&resp.status) {
        // `Failed to fetch image from URL: ${response.statusText}` — thrown,
        // so the wire shows the middleware's flat 500.
        return internal_error();
    }
    if !crate::services::file_storage::ALLOWED_IMAGE_TYPES.contains(&resp.content_type.as_str()) {
        // `Invalid image type from URL. Allowed types: …` — thrown.
        return internal_error();
    }
    if resp.bytes.len() as i64 > crate::services::file_storage::MAX_IMAGE_FILE_SIZE {
        // `Image size exceeds maximum allowed size of 10 MB` — thrown.
        return internal_error();
    }

    // v4 `images-v2.ts:292-295` — the filename comes from the URL's PATH (not
    // its query), last segment, with the mime's subtype appended only when the
    // segment carries no dot at all.
    let filename = import_filename(url, &resp.content_type);

    ingest(
        db,
        deps,
        user_id,
        IngestRequest {
            filename,
            content_type: resp.content_type.clone(),
            bytes: resp.bytes,
            source: "IMPORTED",
            description: Some(format!("Imported from {url}")),
            // v4 echoes the REQUEST url, not the row's description.
            url: Value::String(url.to_string()),
            wire_source: "import",
            validate: false,
        },
        tags,
    )
    .await
}

/// v4 `images-v2.ts:292-295`:
/// ```text
/// const urlPath = new URL(url).pathname;
/// const urlFilename = urlPath.split('/').pop() || 'imported-image';
/// const ext = contentType.split('/')[1] || 'jpg';
/// const originalFilename = urlFilename.includes('.') ? urlFilename : `${urlFilename}.${ext}`;
/// ```
/// `pathname` of a WHATWG URL always begins `/` for a special scheme, so the
/// last segment is `''` for a bare origin — falsy, hence `imported-image`.
fn import_filename(url: &str, content_type: &str) -> String {
    let path = whatwg_pathname(url);
    let last = path.rsplit('/').next().unwrap_or("");
    let base = if last.is_empty() {
        "imported-image"
    } else {
        last
    };
    if base.contains('.') {
        return base.to_string();
    }
    // `'image/png'.split('/')[1]` → `png`; an empty subtype is falsy → `jpg`.
    let ext = content_type.split('/').nth(1).filter(|s| !s.is_empty());
    format!("{}.{}", base, ext.unwrap_or("jpg"))
}

/// The `pathname` of a WHATWG URL. This only ever sees a URL that already
/// passed [`zod_url_ok`], so the scheme is well formed.
///
/// Two shapes, because [`zod_url_ok`] now admits both (P4.76 item (b)):
///
/// * **an authority form** (`scheme://host/p?q`) — everything from the first
///   path slash to `?`/`#`;
/// * **an OPAQUE path** (`mailto:someone@example.invalid`,
///   `data:image/png;base64,…`) — a non-special scheme with no `//`, whose
///   whole remainder up to `?`/`#` IS the pathname, with no leading slash. That
///   is what makes v4 store `someone@example.webp` and
///   `png;base64,iVBORw0KGgo=.webp`, both pinned by corpus rows.
fn whatwg_pathname(url: &str) -> String {
    let url = crate::jsstr::js_trim(url);
    let Some((scheme, rest)) = split_scheme(url) else {
        return String::new();
    };
    let special = SPECIAL_SCHEMES.contains(&scheme.as_str());
    // A special scheme ALWAYS reaches an authority (v4: `http:example.com` parses
    // with the host `example.com`); a non-special one only when it opens `//`.
    let after_authority = if special || rest.starts_with("//") {
        // Skip the slash run, then the authority.
        let after = rest.trim_start_matches(['/', '\\']);
        match after.find(['/', '\\']) {
            Some(i) => &after[i..],
            // A special scheme's `pathname` is never empty: `new URL("https://h")`
            // reports `/` (measured). The downstream last-segment read is `''`
            // either way, but returning v4's value keeps this honest.
            None if special => "/",
            None => "",
        }
    } else {
        // The opaque path: everything after the colon.
        rest
    };
    let end = after_authority
        .find(['?', '#'])
        .unwrap_or(after_authority.len());
    after_authority[..end].to_string()
}

#[cfg(test)]
mod url_shape_tests {
    //! P4.76 item (b) — the two functions above, over the shapes the corpus
    //! rows carry plus the neighbours a corpus cannot reach.
    use super::{import_filename, whatwg_pathname, zod_url_ok};

    #[test]
    fn zod_url_accepts_what_new_url_accepts() {
        // The corpus rows.
        assert!(zod_url_ok("https://example.invalid/pics/photo.png"));
        assert!(zod_url_ok("mailto:someone@example.invalid"));
        assert!(zod_url_ok("data:image/png;base64,iVBORw0KGgo="));
        assert!(!zod_url_ok("not-a-url"));
        // The neighbours: a scheme is required, and it must START with a letter.
        assert!(!zod_url_ok(""));
        assert!(!zod_url_ok("://example.invalid"));
        assert!(!zod_url_ok("1http://example.invalid"));
        assert!(!zod_url_ok("ht tp://example.invalid"));
        // A special scheme REQUIRES a host…
        assert!(!zod_url_ok("https://"));
        // …but reaches it through ANY slash run, in either direction, or none:
        // MEASURED on Node 24, `new URL("https:///path")` parses with the host
        // `path`, and `new URL("http:/x")` with the host `x`.
        assert!(zod_url_ok("https:///path"));
        assert!(zod_url_ok("http:/x"));
        assert!(zod_url_ok("http:example.invalid"));
        assert!(zod_url_ok(r"https:\/\/h/p"));
        // …and `file:` is the exception that permits an empty one.
        assert!(zod_url_ok("file:///etc/hosts"));
        // A non-special scheme needs no authority at all.
        assert!(zod_url_ok("foo:bar"));
        assert!(zod_url_ok("foo://"));
        // `.trim()` runs first.
        assert!(zod_url_ok("  https://example.invalid/a.png  "));
    }

    #[test]
    fn pathname_covers_both_forms() {
        assert_eq!(
            whatwg_pathname("https://h/pics/photo.png"),
            "/pics/photo.png"
        );
        assert_eq!(whatwg_pathname("https://h/p?q=1#f"), "/p");
        assert_eq!(whatwg_pathname("https://h"), "/");
        assert_eq!(whatwg_pathname("https:///path"), "/");
        assert_eq!(whatwg_pathname("foo://"), "");
        assert_eq!(whatwg_pathname("foo:bar"), "bar");
        assert_eq!(
            whatwg_pathname("mailto:someone@example.invalid"),
            "someone@example.invalid"
        );
        assert_eq!(
            whatwg_pathname("data:image/png;base64,iVBORw0KGgo="),
            "image/png;base64,iVBORw0KGgo="
        );
    }

    #[test]
    fn the_two_scheme_rows_derive_v4s_filenames() {
        // v4's stored names, before `createFile`'s `.webp` rename: the mailto
        // path's last segment carries a dot so it is used as-is, while the data
        // URI's does not, so the mime subtype is appended.
        assert_eq!(
            import_filename("mailto:someone@example.invalid", "image/png"),
            "someone@example.invalid"
        );
        assert_eq!(
            import_filename("data:image/png;base64,iVBORw0KGgo=", "image/png"),
            "png;base64,iVBORw0KGgo=.png"
        );
    }
}

/// The middleware's catch for every thrown sentence in this family
/// (`context.ts:147-148` → `serverError('Internal server error')`). The
/// sentences themselves never reach the wire, which is why they are pinned by
/// unit tests rather than corpus rows.
fn internal_error() -> Response {
    Response::error(ErrorKind::Internal, "Internal server error")
}

struct IngestRequest {
    filename: String,
    content_type: String,
    bytes: Vec<u8>,
    source: &'static str,
    description: Option<String>,
    url: Value,
    wire_source: &'static str,
    validate: bool,
}

/// The shared tail both legs run: `createFile` → the per-tag `addTag` loop →
/// the 201 receipt.
async fn ingest(
    db: &Db,
    deps: IngestDeps,
    user_id: &str,
    req: IngestRequest,
    tags: Option<Vec<Value>>,
) -> Response {
    if req.validate && validate_image_file(&req.content_type, req.bytes.len()).is_err() {
        return internal_error();
    }

    // v4 `tags.map(t => t.tagId)` with NO schema — the RAW values, whatever
    // their type.
    let raw_tags = raw_tag_ids(tags.as_deref());

    // MEASURED against v4's real route, and it refutes the ordering premise
    // this port was written from: v4 does NOT silently carry a non-string
    // `tagId`. `repos.files.create` re-validates the row against
    // `FileEntrySchema`, whose `linkedTo` is `z.array(z.uuid())`, so
    // `[{"tagId": 5}]` throws a ZodError out of `createFile` and the route
    // answers `Validation error` 400 — with the bytes ALREADY written, because
    // the throw lands after the bridge write. `create_file_conns` reproduces
    // that order; this flag is only how the caller knows to answer 400 rather
    // than 500.
    let ids_valid = raw_tags.iter().all(|v| {
        v.as_str()
            .is_some_and(crate::services::file_storage::is_zod_uuid)
    });
    // A non-string raw id crosses as its JSON text (`5` → `"5"`), which is not
    // a UUID either, so the validation fails identically on BOTH of
    // `create_file_conns`' arms — the create arm (after the bridge write, an
    // orphaned blob) and the dedup-with-growth arm (before any write). It can
    // never be written, so the coerced spelling is unobservable; the
    // alternative is widening `IngestParams.linked_to` to `Vec<Value>` for a
    // path that always refuses.
    let linked_to: Vec<String> = raw_tags
        .iter()
        .map(|v| match v.as_str() {
            Some(s) => s.to_string(),
            None => v.to_string(),
        })
        .collect();

    let user = user_id.to_string();
    let tags_for_loop = raw_tags.clone();
    let written = db
        .write(move |ws| {
            let main = ws.main().connection();
            let mount = ws
                .mount_index()
                .ok_or_else(|| {
                    DbError::Internal("image ingest requires the mount-index database".to_string())
                })?
                .connection();
            let dims = deps.codec.measure(&req.bytes);
            let params = crate::services::file_storage::IngestParams {
                buffer: req.bytes,
                original_filename: req.filename,
                mime_type: req.content_type,
                user_id: user.clone(),
                linked_to,
                tags: vec![],
                source: req.source.to_string(),
                description: req.description,
            };
            let (entry, _outcome) = crate::services::file_storage::create_file_conns(
                main,
                mount,
                deps.codec.as_ref(),
                deps.backend.as_ref(),
                &params,
                "IMAGE",
                dims,
            )?;
            // v4 runs `addTag` AFTER the ingest, so a tag the ingest already
            // inherited is a no-op; an id that is not a UUID never reaches
            // this loop (see `add_raw_tags`).
            add_raw_tags(main, &entry.id, &tags_for_loop)?;
            let refreshed = crate::db::files::FilesRepository::new(main)
                .find_by_id(&entry.id)?
                .unwrap_or(entry);
            Ok((refreshed, user))
        })
        .await;

    match written {
        Ok((entry, user)) => {
            // v4's two receipt lines (`route.ts:429` / `:483`) — P4.76, the
            // P4.73 unification review's item (c). TWO literal sites, exactly
            // as v4 has two `logger.info` calls: an interpolated single site
            // would carry the same bytes but would be invisible to
            // `handler-logging-inventory.md`'s sentence survey, which requires
            // the literal within reach of the macro. No differential can see a
            // log-only fix (`differential-blind-to-a-log-only-fix`), so the
            // capture tests below are the proof.
            if req.wire_source == "import" {
                tracing::info!(
                    imageId = %entry.id,
                    userId = %user,
                    "[Images v1] Image imported from URL"
                );
            } else {
                tracing::info!(
                    imageId = %entry.id,
                    userId = %user,
                    "[Images v1] Image uploaded"
                );
            }
            // v4 stamps `new Date().toISOString()` at the ROUTE, not the row's.
            let now = crate::clock::now_iso();
            Response::Images(json!({
                "data": ingest_receipt(
                    &user,
                    &entry,
                    req.url,
                    req.wire_source,
                    tags.as_deref(),
                    &now,
                ),
            }))
        }
        // v4's ZodError from `repos.files.create` is the middleware's
        // `validationError` 400; anything else is its generic 500.
        Err(_) if !ids_valid => Response::error(ErrorKind::BadRequest, "Validation error"),
        Err(_) => internal_error(),
    }
}

#[cfg(test)]
mod validate_image_file_tests {
    //! The two `validateImageFile` sentences (`images-v2.ts:218-226`) — the
    //! wire collapses both to the middleware's 500, so the bytes are pinned
    //! here (promised by the doc comment above; landed at the follow-ups-
    //! round-2 unification after the §3 review found the promise empty).
    use super::validate_image_file;

    #[test]
    fn a_disallowed_type_names_the_allow_list() {
        let err = validate_image_file("text/plain", 1).unwrap_err();
        assert_eq!(
            err,
            "Invalid file type. Allowed types: image/jpeg, image/jpg, image/png, image/gif, \
             image/webp, image/avif, image/svg+xml"
        );
    }

    #[test]
    fn an_oversize_file_names_the_limit_in_mb() {
        let err = validate_image_file("image/png", 10 * 1024 * 1024 + 1).unwrap_err();
        assert_eq!(err, "File size exceeds maximum allowed size of 10 MB");
        assert!(validate_image_file("image/png", 10 * 1024 * 1024).is_ok());
    }
}

// ===========================================================================
// P4.76 — POST /api/v1/images?action=generate
// ===========================================================================
//
// v4 `handleGenerateImage` (`app/api/v1/images/route.ts:177-408`). Its own
// route-level implementation, NOT a call into the Salon's `generate_image`
// tool, and the differences are the whole point:
//
// * the Concierge gate is `scanImagePrompts` with **no chat** — the settings
//   are the user's global bag, resolved with `chat: None`;
// * the AUTO_ROUTE reroute picks the FIRST `isDangerousCompatible` profile
//   other than the current one, rather than consulting the Concierge desk's
//   `uncensoredImageProfileId` the way the tool's `reroute_image_profile` does;
// * NO orientation is resolved (`params_builder`'s `orientation: None` arm
//   exists for exactly this caller — "this route's caller passes an explicit
//   size and means it");
// * the whole Concierge block sits in one try/catch that FAILS SAFE — "never
//   block on the Concierge errors" — so a classification failure continues
//   with the ORIGINAL profile;
// * the written file lands through `write_lantern_background_to_mount_store`
//   under `tool/` with `get_inherited_tags`, and the `files` row records the
//   STORE's `storedMimeType`/`sizeBytes`, not the transcode's.
//
// ⚠ 💸 LIVE MONEY: one image-provider call per request, plus (when the
// Concierge is armed) one cheap-LLM classification.

use crate::db::chat_settings::DangerousContentSettings;
use crate::model::image::{ErasedImageGenerate, GeneratedImageData};
use crate::services::dangerous_content::gatekeeper::DangerClassificationResult;

/// The classification call v4's route makes, with its two provider generics
/// erased — `classify_content(db, moderation, completion, …)` is object-safe in
/// every argument BUT `M` and `C`, which only a composing host can construct
/// (the [`ErasedImageImportFetch`] precedent, same file, same reason).
///
/// The trait is the seam; `classify_content`'s own fail-safe contract is
/// unchanged (it never returns an error — a provider failure is
/// [`DangerClassificationResult::safe_fallback`]).
pub trait ImagePromptClassifier: Send + Sync {
    fn classify<'a>(
        &'a self,
        db: &'a crate::db::runtime::Db,
        content: &'a str,
        selection: &'a crate::cheap_llm::CheapLlmSelection,
        user_id: &'a str,
        settings: &'a DangerousContentSettings,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DangerClassificationResult> + Send + 'a>>;
}

/// A type-erased [`ImagePromptClassifier`].
#[derive(Clone)]
pub struct ErasedImagePromptClassifier(Arc<dyn ImagePromptClassifier>);

impl ErasedImagePromptClassifier {
    pub fn new<T: ImagePromptClassifier + 'static>(inner: T) -> Self {
        Self(Arc::new(inner))
    }

    pub async fn classify(
        &self,
        db: &crate::db::runtime::Db,
        content: &str,
        selection: &crate::cheap_llm::CheapLlmSelection,
        user_id: &str,
        settings: &DangerousContentSettings,
    ) -> DangerClassificationResult {
        self.0
            .classify(db, content, selection, user_id, settings)
            .await
    }
}

/// Everything `?action=generate` needs from the composing host, carried as ONE
/// `Option` in the engine assembly so the two seams can never be half-wired:
/// the arm's NAMED refusal gates on the pair, and a `None` classifier can
/// therefore never silently skip v4's Concierge block.
#[derive(Clone)]
pub struct ImagesGenerateSeams {
    /// One `createImageProvider(...).generateImage(params, key)` call.
    pub provider: ErasedImageGenerate,
    /// One `classifyContent(prompt, selection, userId, settings)` call.
    pub classifier: ErasedImagePromptClassifier,
    /// The WebP transcode + measure seam (v4 `convertToWebP`, quality 90).
    pub codec: Arc<dyn PixelCodec>,
}

/// v4's `generateImageSchema` (`route.ts:44-64`), parsed.
struct GenerateBody {
    prompt: String,
    profile_id: String,
    /// The PARSED tags — used for `linkedTo` and echoed on each receipt entry.
    tags: Option<Vec<Value>>,
    /// The parsed `options` bag, already narrowed to the five override keys.
    overrides: crate::image_gen::params_builder::ImageGenOverrides,
}

/// v4 `generateImageSchema.parse(body)`. A failure is a ZodError out of the
/// handler → the middleware's `validationError` (400 `Validation error`), the
/// `details` array being the standing project-wide omission.
///
/// Every arm is Zod's, spelled the way the rest of this port spells Zod 4.5:
/// `z.string().min(1).max(4000)` measures CODE POINTS
/// ([`crate::jsstr::zod_len_min_ok`]), `z.uuid()` is
/// [`is_zod_uuid`], `.optional()` is not `.nullable()` (an explicit `null`
/// REFUSES), and `z.int()` accepts only a safe integer.
fn parse_generate_body(
    prompt: Option<&Value>,
    profile_id: Option<&Value>,
    tags: Option<&Value>,
    options: Option<&Value>,
) -> Result<GenerateBody, Response> {
    let bad = || Response::error(ErrorKind::BadRequest, "Validation error");

    let prompt = prompt
        .and_then(Value::as_str)
        .filter(|s| crate::jsstr::zod_len_min_ok(s, 1) && crate::jsstr::zod_len_max_ok(s, 4000))
        .ok_or_else(bad)?
        .to_string();
    let profile_id = profile_id
        .and_then(Value::as_str)
        .filter(|s| is_zod_uuid(s))
        .ok_or_else(bad)?
        .to_string();

    // The same `{tagType: enum, tagId: string}` array `importFromUrlSchema`
    // carries — and, like it, `.optional()` so an explicit null refuses. The
    // echo is the PARSED shape: unknown keys stripped, `tagType` then `tagId`.
    let tags = match tags {
        None => None,
        Some(v) => {
            let arr = v.as_array().ok_or_else(bad)?;
            let mut out = Vec::with_capacity(arr.len());
            for t in arr {
                let o = t.as_object().ok_or_else(bad)?;
                match o.get("tagType").and_then(Value::as_str) {
                    Some("CHARACTER" | "CHAT" | "THEME") => {}
                    _ => return Err(bad()),
                }
                if o.get("tagId").and_then(Value::as_str).is_none() {
                    return Err(bad());
                }
                out.push(json!({ "tagType": t["tagType"].clone(), "tagId": t["tagId"].clone() }));
            }
            Some(out)
        }
    };

    // `options` is `.optional()`; the handler's `= {}` default only applies to
    // an ABSENT key. A present non-object (null included) is a ZodError.
    let mut overrides = crate::image_gen::params_builder::ImageGenOverrides::default();
    if let Some(v) = options {
        let o = v.as_object().ok_or_else(bad)?;
        // `n: z.int().min(1).max(10).optional()`.
        if let Some(n) = o.get("n") {
            let n = zod_int(n)
                .filter(|n| (1..=10).contains(n))
                .ok_or_else(bad)?;
            overrides.n = Some(n as f64);
        }
        // `size` / `aspectRatio`: `z.string().optional()` — any string.
        for (key, slot) in [
            ("size", &mut overrides.size),
            ("aspectRatio", &mut overrides.aspect_ratio),
        ] {
            if let Some(v) = o.get(key) {
                *slot = Some(v.as_str().ok_or_else(bad)?.to_string());
            }
        }
        // `quality: z.enum(['standard','hd'])`, `style: z.enum(['vivid','natural'])`.
        if let Some(v) = o.get("quality") {
            match v.as_str() {
                Some(s @ ("standard" | "hd")) => overrides.quality = Some(s.to_string()),
                _ => return Err(bad()),
            }
        }
        if let Some(v) = o.get("style") {
            match v.as_str() {
                Some(s @ ("vivid" | "natural")) => overrides.style = Some(s.to_string()),
                _ => return Err(bad()),
            }
        }
    }

    Ok(GenerateBody {
        prompt,
        profile_id,
        tags,
        overrides,
    })
}

/// Zod 4 `z.int()`: a JSON number that is a SAFE integer. `1.5` and `"3"` are
/// both refusals, and so is `1e21` (outside `Number.MAX_SAFE_INTEGER`).
fn zod_int(v: &Value) -> Option<i64> {
    let f = v.as_f64()?;
    if !f.is_finite() || f.fract() != 0.0 || f.abs() > 9_007_199_254_740_991.0 {
        return None;
    }
    Some(f as i64)
}

/// The DB reads v4's Concierge block makes, gathered in one pass. v4 issues
/// them inside its try/catch, so a failure here is that catch's `continue
/// normally`, not a 500.
struct ConciergeInputs {
    settings: DangerousContentSettings,
    all_profiles: Vec<Value>,
    cheap_settings: Option<Value>,
}

fn read_concierge_inputs(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<ConciergeInputs, DbError> {
    // v4 `repos.chatSettings.findByUserId(user.id)` →
    // `resolveDangerousContentSettings(chatSettings ?? null)` with NO chat: this
    // route has none, so the exempt / vouched / uncensored chat arms can never
    // fire and the global bag is the whole answer.
    let chat_settings = crate::db::chat_settings::find_by_user_id(conn, user_id)?;
    let global = chat_settings
        .as_ref()
        .and_then(|cs| cs.get("dangerousContentSettings"))
        .and_then(|d| serde_json::from_value::<DangerousContentSettings>(d.clone()).ok());
    let resolved = crate::services::dangerous_content::resolver::resolve_dangerous_content_settings(
        global, None,
    );
    Ok(ConciergeInputs {
        settings: resolved.settings,
        all_profiles: crate::db::connection_profiles::find_by_user_id(conn, user_id)?,
        cheap_settings: chat_settings
            .as_ref()
            .and_then(|cs| cs.get("cheapLLMSettings"))
            .cloned(),
    })
}

/// v4 `handleGenerateImage` (`route.ts:177-408`), whole.
///
/// The activity span is v4's, at v4's altitude: the POST handler wraps the whole
/// call in `trackActivity('image', …)` (`route.ts:165`) because "generation is
/// synchronous here rather than queued, so it registers with the activity
/// registry to keep the Img chip honest (the Concierge check inside counts
/// under Dgr on its own)". Held by `activity_span_sites_guard` site 9 — which
/// until this unit asserted the site ABSENT.
#[allow(clippy::too_many_arguments)]
pub async fn images_generate(
    db: &Db,
    seams: &ImagesGenerateSeams,
    user_id: &str,
    now_ms: i64,
    prompt_raw: Option<&Value>,
    profile_id_raw: Option<&Value>,
    tags_raw: Option<&Value>,
    options_raw: Option<&Value>,
) -> Response {
    crate::services::activity_registry::track_activity(
        crate::services::activity_kinds::ActivityKind::Image,
        run_images_generate(
            db,
            seams,
            user_id,
            now_ms,
            prompt_raw,
            profile_id_raw,
            tags_raw,
            options_raw,
        ),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_images_generate(
    db: &Db,
    seams: &ImagesGenerateSeams,
    user_id: &str,
    now_ms: i64,
    prompt_raw: Option<&Value>,
    profile_id_raw: Option<&Value>,
    tags_raw: Option<&Value>,
    options_raw: Option<&Value>,
) -> Response {
    let body = match parse_generate_body(prompt_raw, profile_id_raw, tags_raw, options_raw) {
        Ok(b) => b,
        Err(r) => return r,
    };

    // v4 `repos.connections.findById(profileId)`; a miss is `badRequest`, NOT a
    // 404 — this route answers 400 where the image-profiles route answers 404.
    let mut profile = match db
        .read_main(|c| crate::db::connection_profiles::find_by_id(c, &body.profile_id))
    {
        Ok(Some(p)) => p,
        Ok(None) => return Response::error(ErrorKind::BadRequest, "Connection profile not found"),
        Err(e) => return super::types::db_error_response(e),
    };
    let original_profile_id = body.profile_id.clone();

    // ── the Concierge integration (route.ts:190-257) ────────────────────────
    // ONE try/catch around the whole block: "Fail safe — never block on the
    // Concierge errors". Every `?`-shaped failure below therefore lands in
    // `concierge_failed` and continues with the ORIGINAL profile.
    let inputs = db.read_main(|c| read_concierge_inputs(c, user_id));
    match inputs {
        Err(e) => {
            tracing::error!(
                context = "Images v1",
                user_id = %user_id,
                error = %e,
                "[Images v1] the Concierge classification failed, continuing normally"
            );
        }
        Ok(inputs) => {
            if inputs.settings.mode != "OFF" && inputs.settings.scan_image_prompts {
                // v4 builds the selection from the DEFAULT profile (or the
                // first) — `build_cheap_llm_selection` is `None` only when the
                // user has no profiles at all, which is v4's `if (defaultProfile)`.
                let selection = crate::services::image_job_common::build_cheap_llm_selection(
                    &inputs.all_profiles,
                    inputs.cheap_settings.as_ref(),
                );
                if let Some(selection) = selection {
                    let classification = seams
                        .classifier
                        .classify(db, &body.prompt, &selection, user_id, &inputs.settings)
                        .await;
                    if classification.is_dangerous {
                        tracing::info!(
                            context = "Images v1",
                            user_id = %user_id,
                            score = classification.score,
                            categories = ?classification
                                .categories
                                .iter()
                                .map(|c| c.category.clone())
                                .collect::<Vec<_>>(),
                            mode = %inputs.settings.mode,
                            "[Images v1] Front page image prompt classified as dangerous"
                        );
                        if inputs.settings.mode == "AUTO_ROUTE" {
                            // v4 `allProfiles.find(p => p.isDangerousCompatible
                            // === true && p.id !== profile.id)` — the FIRST
                            // compatible profile in `findByUserId` order, NOT
                            // the Concierge desk's `uncensoredImageProfileId`
                            // (which is what the tool's reroute reads). The
                            // comparison is against the CURRENT `profile.id`,
                            // which at this point is still the requested one.
                            let uncensored = inputs.all_profiles.iter().find(|p| {
                                p.get("isDangerousCompatible").and_then(Value::as_bool)
                                    == Some(true)
                                    && p.get("id").and_then(Value::as_str)
                                        != profile.get("id").and_then(Value::as_str)
                            });
                            match uncensored {
                                Some(p) => {
                                    // `Value::as_str` inside a `tracing::` macro
                                    // resolves against the macro's own `Value`
                                    // trait, not `serde_json`'s — read the two
                                    // fields OUT here (memory note
                                    // `tracing-macro-shadows-serde-json-value`).
                                    let new_id = p
                                        .get("id")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or_default();
                                    let new_name = p
                                        .get("name")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or_default();
                                    tracing::info!(
                                        context = "Images v1",
                                        user_id = %user_id,
                                        original_profile_id = %original_profile_id,
                                        uncensored_profile_id = %new_id,
                                        uncensored_profile_name = %new_name,
                                        "[Images v1] Rerouted to uncensored connection profile"
                                    );
                                    profile = p.clone();
                                }
                                None => tracing::warn!(
                                    context = "Images v1",
                                    user_id = %user_id,
                                    "[Images v1] No uncensored connection profile available, using original"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    // ── the API key (route.ts:259-266) ──────────────────────────────────────
    // v4 `repos.connections.findApiKeyById` — UN-scoped by user, and a dangling
    // id is simply an empty key (no refusal), which the provider then rejects.
    let api_key_id = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let decrypted_key = match api_key_id {
        Some(id) => match db.read_main(|c| crate::db::api_keys::find_by_id(c, &id)) {
            Ok(Some(k)) => k.key_value,
            Ok(None) => String::new(),
            Err(e) => return super::types::db_error_response(e),
        },
        None => String::new(),
    };

    // ── createImageProvider (route.ts:268-275) ──────────────────────────────
    // v4 `providerRegistry.createImageProvider` throws when the provider is not
    // registered, declares no `imageGeneration` capability, or ships no factory;
    // the route's bare `catch` collapses all three into ONE sentence that names
    // the profile's ORIGINAL provider string (never the GOOGLE_IMAGEN→GOOGLE
    // mapping the factory applies first).
    let provider_name = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mapped = if provider_name.to_uppercase() == "GOOGLE_IMAGEN" {
        "GOOGLE"
    } else {
        provider_name.as_str()
    };
    if !crate::provider_manifest::Registry::built_in().supports_capability(
        mapped,
        crate::provider_manifest::Capability::ImageGeneration,
    ) {
        return Response::error(
            ErrorKind::BadRequest,
            format!("{provider_name} provider does not support image generation"),
        );
    }

    // ── the shared params builder, with NO orientation (route.ts:277-296) ────
    let model_name = profile.get("modelName").and_then(Value::as_str);
    let built = crate::image_gen::params_builder::build_image_gen_params(
        crate::image_gen::params_builder::ImageProfileLike {
            provider: &provider_name,
            model_name,
            parameters: profile.get("parameters"),
        },
        &body.prompt,
        &body.overrides,
        // "No orientation is resolved: this route's caller passes an explicit
        // size and means it."
        None,
        "dall-e-3",
        &crate::image_gen_data::image_declarations_for(&provider_name),
        &crate::image_gen::params_builder::ImageParamsLogContext {
            context: "api.v1.images.generate",
            profile_id: profile
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            ..Default::default()
        },
    );

    // ── the provider call (route.ts:298) ────────────────────────────────────
    // v4 does not wrap it: a throw escapes to the middleware's flat 500.
    let response = match seams
        .provider
        .generate_image(&provider_name, &decrypted_key, &built.params)
        .await
    {
        Ok(r) => r,
        Err(_) => return internal_error(),
    };

    // ── store each image (route.ts:300-388) ─────────────────────────────────
    let linked_to: Vec<String> = body
        .tags
        .as_ref()
        .map(|ts| {
            ts.iter()
                .map(|t| {
                    t.get("tagId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    let mut prepared: Vec<PreparedGeneratedImage> = Vec::with_capacity(response.images.len());
    for (index, image) in response.images.iter().enumerate() {
        match prepare_generated_image(seams.codec.as_ref(), image, index, now_ms) {
            Some(p) => prepared.push(p),
            // v4 `throw new Error('Generated image has no data')` inside the
            // `Promise.all` — the middleware's flat 500.
            None => return internal_error(),
        }
    }

    let model_for_row = model_name.map(str::to_string);
    let prompt_for_row = body.prompt.clone();
    let user = user_id.to_string();
    let written = db
        .write(move |ws| {
            let main = ws.main().connection();
            let mount = ws
                .mount_index()
                .ok_or_else(|| {
                    DbError::Internal(
                        "image generation requires the mount-index database".to_string(),
                    )
                })?
                .connection();
            let mut out = Vec::with_capacity(prepared.len());
            for p in &prepared {
                // v4 checks `getLanternBackgroundsStore()` first and throws its
                // own sentence; `write_lantern_background_to_mount_store`
                // resolves the same mount and errors identically when it is
                // unprovisioned. Either way the wire shows the flat 500.
                let stored =
                    crate::services::image_job_storage::write_lantern_background_to_mount_store(
                        main,
                        mount,
                        &p.filename,
                        &p.bytes,
                        &p.mime_type,
                        "tool",
                        None,
                    )?;
                let inherited = crate::services::file_storage::get_inherited_tags(
                    main, mount, &linked_to, &user,
                );
                let file_id = uuid::Uuid::new_v4().to_string();
                let now = crate::clock::now_iso();
                crate::db::files::FilesRepository::new(main).create(
                    &crate::db::files::FileCreate {
                        user_id: user.clone(),
                        sha256: p.sha256.clone(),
                        original_filename: p.filename.clone(),
                        // The STORE's mime/size, not the transcode's — "the
                        // Lantern bridge transcodes bitmaps to WebP; record the
                        // stored mime/size so vision providers don't reject
                        // 'media_type X but bytes are Y' mismatches."
                        mime_type: stored.stored_mime_type.clone(),
                        size: stored.size_bytes as f64,
                        // v4's route passes NO width/height (unlike the tool's
                        // `saveGeneratedImage`) — the columns take their
                        // schema defaults.
                        width: None,
                        height: None,
                        is_plain_text: None,
                        linked_to: linked_to.clone(),
                        source: "GENERATED".to_string(),
                        category: "IMAGE".to_string(),
                        generation_prompt: Some(prompt_for_row.clone()),
                        generation_model: model_for_row.clone(),
                        generation_revised_prompt: p.revised_prompt.clone(),
                        description: None,
                        tags: inherited,
                        project_id: None,
                        folder_path: None,
                        storage_key: Some(stored.storage_key.clone()),
                        file_status: "ok".to_string(),
                    },
                    &crate::db::files::CreateOptions {
                        id: file_id.clone(),
                        created_at: now.clone(),
                        updated_at: now,
                    },
                )?;
                let entry = crate::db::files::FilesRepository::new(main).find_by_id(&file_id)?;
                out.push((file_id, entry, p.revised_prompt.clone()));
            }
            Ok(out)
        })
        .await;

    let saved = match written {
        Ok(v) => v,
        Err(_) => return internal_error(),
    };

    let echo_tags = body
        .tags
        .clone()
        .map(Value::Array)
        .unwrap_or_else(|| Value::Array(vec![]));
    let data: Vec<Value> = saved
        .iter()
        .map(|(id, entry, revised)| {
            let filepath = format!("/api/v1/files/{id}");
            let mut m = Map::new();
            m.insert("id".into(), json!(id));
            m.insert(
                "filename".into(),
                json!(entry.as_ref().map(|e| e.original_filename.clone())),
            );
            m.insert("filepath".into(), json!(filepath));
            m.insert("url".into(), json!(filepath));
            m.insert(
                "mimeType".into(),
                json!(entry.as_ref().map(|e| e.mime_type.clone())),
            );
            m.insert("size".into(), json!(entry.as_ref().map(|e| e.size)));
            // `revisedPrompt: generatedImage.revisedPrompt` — a JS `undefined`
            // drops out of `JSON.stringify`, so an absent revision is an ABSENT
            // key, never null.
            if let Some(r) = revised {
                m.insert("revisedPrompt".into(), json!(r));
            }
            m.insert("tags".into(), echo_tags.clone());
            Value::Object(m)
        })
        .collect();

    tracing::info!(
        context = "Images v1",
        user_id = %user_id,
        generated_count = data.len(),
        "[Images v1] Image generation complete"
    );

    let count = data.len();
    Response::Images(json!({
        "data": data,
        "metadata": {
            "prompt": body.prompt,
            "provider": provider_name,
            "model": model_name,
            "count": count,
        },
    }))
}

/// One image's pre-write work — the base64 decode + the WebP transcode + the
/// two filenames — all of it OFF the writer thread (the codec seam never
/// crosses into the write closure).
struct PreparedGeneratedImage {
    bytes: Vec<u8>,
    mime_type: String,
    filename: String,
    sha256: String,
    revised_prompt: Option<String>,
}

fn prepare_generated_image(
    codec: &dyn PixelCodec,
    image: &GeneratedImageData,
    index: usize,
    now_ms: i64,
) -> Option<PreparedGeneratedImage> {
    // v4 `generatedImage.data || generatedImage.b64Json` — v5's decoder folds
    // both vendor spellings into `data`, so the `||` has one operand here.
    let raw = image.data.as_deref().filter(|s| !s.is_empty())?;
    let raw_buffer = crate::services::image_job_common::decode_base64_node(raw);

    // `const providerMime = generatedImage.mimeType || 'image/png'` then
    // `mimeTypeParts[1] === 'jpeg' ? 'jpg' : mimeTypeParts[1] || 'png'`.
    let provider_mime = image
        .mime_type
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("image/png");
    let subtype = provider_mime.split('/').nth(1).unwrap_or("");
    let ext = match subtype {
        "jpeg" => "jpg",
        "" => "png",
        other => other,
    };
    let provider_filename = format!("generated_{now_ms}_{index}.{ext}");

    let converted = crate::services::file_storage::convert_to_webp(
        codec,
        &raw_buffer,
        provider_mime,
        &provider_filename,
    );
    let sha256 = crate::photos::keep_image_markdown::sha256_of_buffer(&converted.buffer);
    let short_hash = &sha256[..8];
    // ⚠ ALWAYS `.webp`, even when `convertToWebP` passed the bytes through
    // (an SVG, or an already-WebP provider answer) — v4's literal.
    let filename = format!("generated_{now_ms}_{index}_{short_hash}.webp");

    Some(PreparedGeneratedImage {
        bytes: converted.buffer,
        mime_type: converted.mime_type,
        filename,
        sha256,
        revised_prompt: image.revised_prompt.clone(),
    })
}

#[cfg(test)]
mod log_context_tests {
    //! P4.76 (the P4.73 unification review's item (c)) — v4's two per-leg
    //! receipt lines, `[Images v1] Image uploaded` (`route.ts:485`) and
    //! `[Images v1] Image imported from URL` (`route.ts:429`).
    //!
    //! **A differential cannot see a log-only fix**
    //! (`differential-blind-to-a-log-only-fix`): both legs answer the same
    //! receipt and write the same rows whether the line is emitted or not. The
    //! capture layer is the proof, and the two-sentences-DIFFER arm is half of
    //! it — a port that emitted the upload sentence on the import leg would
    //! satisfy a presence-only test.

    use std::sync::{Arc, Mutex};

    struct CaptureLayer(Arc<Mutex<Vec<String>>>);

    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0.push_str(&format!(" {value:?}"));
            } else {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut visitor = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }

    /// `set_default` is THREAD-scoped, so parallel tests cannot steal each
    /// other's subscriber.
    fn captured(f: impl FnOnce()) -> Vec<String> {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            f();
        }
        let out = logs.lock().unwrap().clone();
        out
    }

    fn assert_line(lines: &[String], sentence: &str) {
        let line = lines
            .iter()
            .find(|l| l.contains(sentence))
            .unwrap_or_else(|| panic!("no line carried {sentence:?}; got {lines:#?}"));
        assert!(line.contains("INFO"), "{line}");
        assert!(
            line.contains("imageId=f0000000-0000-4000-8000-000000000001"),
            "{line}"
        );
        assert!(
            line.contains("userId=11111111-1111-4111-8111-111111111111"),
            "{line}"
        );
    }

    /// The two sentences with v4's two fields around them, in v4's order.
    #[test]
    fn the_two_leg_sentences_carry_v4s_fields() {
        let id = "f0000000-0000-4000-8000-000000000001";
        let user = "11111111-1111-4111-8111-111111111111";
        let up = captured(|| {
            tracing::info!(imageId = %id, userId = %user, "[Images v1] Image uploaded");
        });
        assert_line(&up, "[Images v1] Image uploaded");
        let im = captured(|| {
            tracing::info!(imageId = %id, userId = %user, "[Images v1] Image imported from URL");
        });
        assert_line(&im, "[Images v1] Image imported from URL");
    }

    /// The WIRING, as a source census (the `db_error_key_guard` idiom): each
    /// leg emits its OWN sentence, exactly once, and both live inside the
    /// post-write branch. Presence tests alone would pass a copy-paste that
    /// gave both legs the upload line, and there is no DB state to tell them
    /// apart afterwards — the line is the only difference.
    #[test]
    fn each_leg_emits_its_own_sentence_exactly_once() {
        let whole = include_str!("images.rs");
        // PRODUCTION ONLY. This module quotes both sentences itself, and so does
        // the capture test above — a whole-file count would read 3. Truncating
        // at the FIRST `#[cfg(test)]` is the trap the round records name: this
        // file has THREE test modules and two of them sit mid-file, above code
        // this census must still see. So drop each module-level test block by
        // its own braces instead (they all open at column 0 and close on a bare
        // `}`), and assert afterwards that the zone still holds the code.
        let mut src = String::with_capacity(whole.len());
        let mut skipping = false;
        for line in whole.lines() {
            if line.starts_with("#[cfg(test)]") {
                skipping = true;
                continue;
            }
            if skipping {
                if line == "}" {
                    skipping = false;
                }
                continue;
            }
            src.push_str(line);
            src.push('\n');
        }
        assert!(
            !skipping,
            "an unterminated test module — the census is truncated"
        );
        for anchor in ["pub async fn images_generate", "pub async fn image_upload"] {
            assert!(src.contains(anchor), "the production zone lost {anchor}");
        }
        let src = src.as_str();
        // The needles are assembled at runtime so this census does not count
        // ITSELF — the `activity_span_sites_guard` self-match trap.
        let prefix = "[Images v1] Image";
        for tail in [" uploaded\"", " imported from URL\""] {
            let needle = format!("{prefix}{tail}");
            assert_eq!(
                src.matches(&needle).count(),
                1,
                "exactly one PRODUCTION emission site carries {needle:?}"
            );
        }
    }
}
