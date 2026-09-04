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

/// v4 zod's `uuid()` source pattern, hand-matched (the `api/settings.rs`
/// `zod_uuid_ok` transcription — a THIRD copy of the same predicate; folding
/// the three into one home is a named consolidation candidate, recorded in the
/// lane record rather than done here, because the other two live in files this
/// lane does not own).
fn zod_uuid_ok(s: &str) -> bool {
    if s == "00000000-0000-0000-0000-000000000000" || s == "ffffffff-ffff-ffff-ffff-ffffffffffff" {
        return true;
    }
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            14 => {
                if !c.is_ascii_hexdigit() || !(b'1'..=b'8').contains(&c) {
                    return false;
                }
            }
            19 => {
                if !matches!(c, b'8' | b'9' | b'a' | b'b' | b'A' | b'B') {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

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
    let all_uuid = |arr: &[Value]| arr.iter().all(|v| v.as_str().is_some_and(zod_uuid_ok));
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
/// ingest already inherited is a no-op, and a non-string raw id is pushed onto
/// the array as-is.
pub(crate) fn add_raw_tags(
    main: &rusqlite::Connection,
    file_id: &str,
    tag_ids: &[Value],
) -> Result<(), DbError> {
    for tag in tag_ids {
        match tag.as_str() {
            Some(s) => {
                FilesRepository::new(main).add_tag(file_id, s)?;
            }
            // v4's `addTag` pushes the raw value into the JSON array; the
            // repository's string-typed twin cannot, so the raw arm is spelled
            // out here rather than widening the repository for one caller.
            None => add_raw_tag_value(main, file_id, tag)?,
        }
    }
    Ok(())
}

/// The non-string half of [`add_raw_tags`] — v4's `addTag` is
/// `tags.includes(tagId) ? noop : [...tags, tagId]` over the hydrated array,
/// so a number is compared with `===` and appended as a number.
fn add_raw_tag_value(
    main: &rusqlite::Connection,
    file_id: &str,
    tag: &Value,
) -> Result<(), DbError> {
    use rusqlite::OptionalExtension;
    let cell: Option<Option<String>> = main
        .query_row(
            "SELECT tags FROM files WHERE id = ?1",
            rusqlite::params![file_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(cell) = cell else { return Ok(()) };
    let mut arr = raw_json_array(cell.as_deref());
    if arr.iter().any(|t| t == tag) {
        return Ok(());
    }
    arr.push(tag.clone());
    let now = crate::clock::now_iso();
    main.execute(
        "UPDATE files SET tags = ?1, updatedAt = ?2 WHERE id = ?3",
        rusqlite::params![Value::Array(arr).to_string(), now, file_id],
    )?;
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
/// are pinned by unit tests here, not by a corpus row, because the wire cannot
/// show them.
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
            Some(arr.clone())
        }
    };
    Ok((url.to_string(), tags))
}

/// Zod 4's `z.url()` — a WHATWG-parseable absolute URL.
fn zod_url_ok(s: &str) -> bool {
    let Some(scheme_end) = s.find("://") else {
        return false;
    };
    let scheme = &s[..scheme_end];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return false;
    }
    let rest = &s[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    !rest[..authority_end].is_empty()
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

/// The `pathname` of a WHATWG URL — everything after the authority and before
/// `?`/`#`. Deliberately narrow: this only ever sees a URL that already passed
/// v4's `z.url()`, so the scheme and authority are well formed.
fn whatwg_pathname(url: &str) -> String {
    let after_scheme = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let rest = &after_scheme[path_start..];
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    rest[..end].to_string()
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
    // a UUID either, so the validation fails identically. The stored value
    // would differ if it could ever be written — it cannot, so it is
    // unobservable; the alternative is widening `IngestParams.linked_to` to
    // `Vec<Value>` for a path that always refuses.
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
            // inherited is a no-op and a non-string raw id is appended as-is.
            add_raw_tags(main, &entry.id, &tags_for_loop)?;
            let refreshed = crate::db::files::FilesRepository::new(main)
                .find_by_id(&entry.id)?
                .unwrap_or(entry);
            Ok((refreshed, user))
        })
        .await;

    match written {
        Ok((entry, user)) => {
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
