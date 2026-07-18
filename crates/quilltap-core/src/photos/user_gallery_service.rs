//! v4 `lib/photos/user-gallery-service.ts` — the backend for the "My Photos"
//! screen (`GET|POST /api/v1/photos`, `GET|DELETE /api/v1/photos/[id]`).
//!
//! Carrying v4's own framing: the human user's gallery is a deduped roll-up of
//! every `photos/` folder across the instance — character vaults, project
//! stores, Quilltap General, Quilltap Uploads. Whenever an image is saved (via
//! `keep_image`, the Salon Save-Image dialog, or the legacy "save to my gallery"
//! button) it lands in some mount point's `photos/` folder; this service
//! surfaces them all under one roof on `/photos`, deduped by sha256 so the same
//! bytes appearing in multiple albums show as a single card with a "linked in N
//! places" badge.
//!
//! The save path still piggy-backs on `link_blob_content` — the same
//! content-addressed hard-link plumbing the rest of the photo system uses — and
//! writes into the Quilltap Uploads mount when called directly. The list path,
//! however, is mount-agnostic.
//!
//! ## The two injected seams
//!
//! - **The query embedding.** v4 calls `generateEmbeddingForUser(query, userId)`
//!   inline. The core keeps that async provider call at the `api::photos` layer
//!   and hands the resulting vector in, so the service body stays sync (the
//!   writer-thread rule). [`needs_query_embedding`] is the ONE predicate both
//!   sides branch on, so the caller can never disagree with the service about
//!   whether a vector was required.
//! - **The image bytes.** The save leg reads the stored bytes through the
//!   existing [`FileBytesStore`] seam (v4's `fileStorageManager.downloadFile`).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkBlobInput};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::files::FilesRepository;
use crate::db::DbError;
use crate::jsstr::{is_js_ws, js_trim, js_trim_end, utf16_len, utf16_truncate};
use crate::photos::keep_image_markdown::{
    basename_of_relative_path, build_kept_image_markdown, build_slug_and_filename,
    parse_kept_image_frontmatter, sha256_of_buffer, sha256_of_string, BuildKeptImageMarkdownInput,
    BuildSlugAndFilenameInput, KeptImageAttributionRole, SceneState,
};
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::photos_paths::{
    build_photos_relative_path, is_photos_relative_path, PHOTOS_FOLDER,
};
use crate::photos::resolve_character_avatar::build_mount_file_url;
use crate::photos::save_image_to_album::{
    chunk_and_insert_extracted_text, resolve_unique_relative_path, FileBytesStore,
};

/// v4 `USER_LINKED_BY_NAME` — the attribution name a user-gallery save records.
const USER_LINKED_BY_NAME: &str = "You";

/// v4 `DEFAULT_LIMIT` (`user-gallery-service.ts:103`). NOTE: 24, not the
/// character gallery's 60 — the two services have different defaults.
const DEFAULT_LIMIT: i64 = 24;

/// v4's two-stage semantic gate (`user-gallery-service.ts:355-356`). Carried with
/// v4's reasoning: cosine similarity has a per-corpus noise floor — random
/// gibberish like "asdfasdf" still scores ~0.55-0.60 against image-prompt
/// embeddings, so the default 0.3 minScore lets thousands of false-positive hits
/// through. Real semantic matches peak distinctly above the noise (e.g.
/// "wardrobe" tops 0.84, "covenant" tops 0.78).
const SEMANTIC_PEAK_GATE: f64 = 0.65;
const SEMANTIC_TRAIL_BAND: f64 = 0.2;

/// The failure surface of the save/remove legs. v4's service `throw`s plain
/// `Error`s and the ROUTE decides 400-vs-500 by substring-testing the message
/// (`app/api/v1/photos/route.ts:92-99`, `[id]/route.ts:46-48`) — a quirk worth
/// preserving verbatim, so the service carries only the message and
/// `api::photos` replicates the substring chain.
#[derive(Debug)]
pub enum UserGalleryError {
    /// A v4 `throw new Error(message)` — the message is v4-verbatim.
    Message(String),
    Db(DbError),
}

impl From<DbError> for UserGalleryError {
    fn from(e: DbError) -> Self {
        UserGalleryError::Db(e)
    }
}

impl UserGalleryError {
    fn msg(m: impl Into<String>) -> Self {
        UserGalleryError::Message(m.into())
    }
}

/// v4's `if (query && query.trim().length > 0)` guard
/// (`user-gallery-service.ts:311`) — the ONE predicate that decides whether the
/// semantic branch runs, shared by `api::photos` (which must generate the
/// embedding first) and [`list_user_gallery`] (which consumes it).
pub fn needs_query_embedding(query: Option<&str>) -> bool {
    query.is_some_and(|q| !js_trim(q).is_empty())
}

/// v4 `listUserGallery` — every `photos/` link across every enabled mount point,
/// deduped by sha256, most-recent first (or ranked by semantic similarity when a
/// query is supplied).
///
/// Each entry surfaces the *primary* link (the most recently created one for a
/// given sha256) plus the full link-summary so the UI can show how many other
/// places the same bytes live and which characters/projects own them.
///
/// `query_embedding` MUST be `Some` exactly when [`needs_query_embedding`] is
/// true for `query`; the mismatch is a caller bug and surfaces as an error
/// rather than a silently different ranking.
pub fn list_user_gallery(
    mount: &Connection,
    query: Option<&str>,
    query_embedding: Option<&[f32]>,
    tags: Option<&[String]>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Value, UserGalleryError> {
    // v4: `Math.max(1, Math.min(limit ?? 24, 200))` / `Math.max(0, offset ?? 0)`.
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 200);
    let effective_offset = offset.unwrap_or(0).max(0);

    // 1. Fan out across every enabled mount point and collect links whose
    //    relativePath lives under `photos/`. Quilltap is single-user, so we
    //    don't filter by userId at the link layer.
    //
    //    v4's `findEnabled()` is `findByFilter({enabled: true})` — no ORDER BY,
    //    so both sides walk rowid order; the same holds for `findByMountPointId`.
    //    That shared order is what makes the dedup tie-break below deterministic.
    let points = DocMountPointsRepository::new(mount);
    let links_repo = DocMountFileLinksRepository::new(mount);
    let mut all_photo_links: Vec<(crate::db::doc_mount_file_links::LinkRow, String)> = Vec::new();
    for mp in points.find_enabled_for_docedit()? {
        for link in links_repo.find_by_mount_point_id(&mp.id)? {
            if is_photos_relative_path(Some(&link.relative_path)) {
                all_photo_links.push((link, mp.id.clone()));
            }
        }
    }

    if all_photo_links.is_empty() {
        return Ok(json!({ "entries": [], "total": 0, "hasMore": false }));
    }

    // 2. Dedupe by sha256. For each unique image, keep the most recently created
    //    link as the "primary" — that's the one whose mount + path drive the
    //    displayed thumbnail and the user-facing remove action.
    //
    //    v4 compares with `createdAt.localeCompare(...) > 0`. On ISO-8601 stamps
    //    (ASCII digits and fixed punctuation, all the same length) ICU collation
    //    and byte order agree, so a plain `>` is exact here. The STRICT `>` means
    //    an exact tie keeps the FIRST link encountered in the walk above.
    //
    //    `by_sha` mirrors a JS `Map`: `slots` holds (sha256, primary index) in
    //    INSERTION order (step 3 iterates it and the order reaches the wire), and
    //    `slot_of` is the O(1) lookup. Re-setting an existing key keeps its
    //    original position, exactly like `Map.set`.
    let mut slots: Vec<(String, usize)> = Vec::new();
    let mut slot_of: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (idx, (link, _)) in all_photo_links.iter().enumerate() {
        match slot_of.get(link.sha256.as_str()) {
            Some(&slot) => {
                if link.created_at > all_photo_links[slots[slot].1].0.created_at {
                    slots[slot].1 = idx;
                }
            }
            None => {
                slot_of.insert(link.sha256.as_str(), slots.len());
                slots.push((link.sha256.clone(), idx));
            }
        }
    }
    let by_sha = slots;

    // 3. Rank. With a query, run semantic search across every mount with at least
    //    one photo link, constrained to `photos/`. Map scores back to sha256 via
    //    the linkers' relativePaths. Without a query, sort by keptAt desc.
    //
    //    `ranked` holds (sha256, index-into-all_photo_links, relevance).
    let mut ranked: Vec<(String, usize, Option<f64>)> = Vec::new();

    if needs_query_embedding(query) {
        let query = query.expect("needs_query_embedding implies Some");
        let Some(embedding) = query_embedding else {
            return Err(UserGalleryError::msg(
                "list_user_gallery: a query was supplied without its embedding",
            ));
        };

        // v4 `Array.from(new Set(...))` — first-seen order.
        let mut photo_mount_ids: Vec<String> = Vec::new();
        for (_, mp_id) in &all_photo_links {
            if !photo_mount_ids.iter().any(|m| m == mp_id) {
                photo_mount_ids.push(mp_id.clone());
            }
        }

        use crate::services::knowledge_injector::document_search::{
            search_document_chunks, DocumentSearchOptions,
        };
        let hits = search_document_chunks(
            mount,
            embedding,
            &DocumentSearchOptions {
                project_id: None,
                mount_point_ids: Some(photo_mount_ids),
                // v4 passes no minScore → the service default (0.3).
                min_score: None,
                limit: Some((effective_limit * 8) as usize),
                path_prefix: Some(format!("{PHOTOS_FOLDER}/")),
                query: Some(query.to_string()),
                apply_literal_phrase_boost: true,
                literal_boost_fraction: None,
                include_blocked: false,
            },
        )?;

        // Build (mountPointId, lowerRelativePath) -> score so we can match the
        // primary link back exactly. Photos with the same caption text across two
        // mounts would otherwise collide on path alone.
        let mut by_key: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for hit in &hits {
            let key = format!(
                "{}::{}",
                hit.mount_point_id,
                hit.relative_path.to_lowercase()
            );
            by_key
                .entry(key)
                .and_modify(|prior| {
                    if hit.score > *prior {
                        *prior = hit.score;
                    }
                })
                .or_insert(hit.score);
        }
        for (sha256, primary_idx) in &by_sha {
            // Match against any of the linker rows so a hit in any vault counts.
            let mut best: Option<f64> = None;
            for (link, mp_id) in &all_photo_links {
                if &link.sha256 != sha256 {
                    continue;
                }
                let key = format!("{}::{}", mp_id, link.relative_path.to_lowercase());
                if let Some(&score) = by_key.get(&key) {
                    if best.is_none_or(|b| score > b) {
                        best = Some(score);
                    }
                }
            }
            if let Some(best) = best {
                ranked.push((sha256.clone(), *primary_idx, Some(best)));
            }
        }
        // v4 `sort((a, b) => (b.relevance ?? 0) - (a.relevance ?? 0))` — JS sort is
        // stable, so equal scores keep insertion (bySha) order; `sort_by` is too.
        ranked.sort_by(|a, b| {
            b.2.unwrap_or(0.0)
                .partial_cmp(&a.2.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // The two-stage gate: a noise query returns nothing at all; otherwise keep
        // only the results within SEMANTIC_TRAIL_BAND of the peak, so the long tail
        // of marginal hits doesn't bloat the page.
        let top_score = ranked.first().and_then(|r| r.2).unwrap_or(0.0);
        if top_score < SEMANTIC_PEAK_GATE {
            ranked = Vec::new();
        } else {
            let cutoff = top_score - SEMANTIC_TRAIL_BAND;
            ranked.retain(|r| r.2.unwrap_or(0.0) >= cutoff);
        }
    } else {
        ranked = by_sha
            .iter()
            .map(|(sha, idx)| (sha.clone(), *idx, None))
            .collect();
        // v4 `b.primary.link.createdAt.localeCompare(a…)` — desc, stable.
        ranked.sort_by(|a, b| {
            all_photo_links[b.1]
                .0
                .created_at
                .cmp(&all_photo_links[a.1].0.created_at)
        });
    }

    // 4. Apply the optional tag filter. Tags are pulled from kept-image
    //    frontmatter on the primary link; if a non-primary link has different tags
    //    they're still visible via the link-summary expansion in the UI.
    let filtered: Vec<&(String, usize, Option<f64>)> = ranked
        .iter()
        .filter(|candidate| {
            let Some(wanted) = tags.filter(|t| !t.is_empty()) else {
                return true;
            };
            let meta = parse_kept_image_frontmatter(
                all_photo_links[candidate.1].0.extracted_text.as_deref(),
            );
            let lowered: std::collections::HashSet<String> =
                meta.tags.iter().map(|t| t.to_lowercase()).collect();
            wanted.iter().any(|t| lowered.contains(&t.to_lowercase()))
        })
        .collect();

    let total = filtered.len() as i64;
    // JS `slice(offset, offset + limit)` — an out-of-range offset yields [].
    let start = effective_offset.min(total) as usize;
    let end = (effective_offset + effective_limit).min(total) as usize;
    let page = &filtered[start..end];

    let mut entries: Vec<Value> = Vec::with_capacity(page.len());
    for (_, primary_idx, relevance) in page {
        let (link, mp_id) = &all_photo_links[*primary_idx];
        let mut entry = entry_object(mount, link, mp_id)?;
        // v4 sets `relevanceScore: relevance` — `undefined` in the no-query branch,
        // which `JSON.stringify` DROPS, so the key must be absent there.
        if let Some(score) = relevance {
            // Insert BEFORE linkSummary to match v4's object-literal key order.
            let obj = entry
                .as_object_mut()
                .expect("entry_object builds an object");
            let link_summary = obj.shift_remove("linkSummary");
            // `JSON.stringify` renders a whole-valued JS number WITHOUT a
            // decimal point (`1`, not `1.0`), and a perfect cosine match scores
            // exactly 1 — so a bare `json!(f64)` would put `1.0` on a wire v4
            // spells `1`. Pinned by `list_query_dawn`/`list_query_covenant`.
            obj.insert(
                "relevanceScore".into(),
                crate::db::js_number_to_json(*score),
            );
            if let Some(ls) = link_summary {
                obj.insert("linkSummary".into(), ls);
            }
        }
        entries.push(entry);
    }

    let has_more = effective_offset + (page.len() as i64) < total;
    Ok(json!({ "entries": entries, "total": total, "hasMore": has_more }))
}

/// v4 `getUserGalleryEntry` — the single-entry read behind
/// `GET /api/v1/photos/[id]`. `Ok(None)` when the link is unknown OR isn't a
/// `photos/` link (both are v4's `return null` → the route's 404).
pub fn get_user_gallery_entry(
    mount: &Connection,
    link_id: &str,
) -> Result<Option<Value>, UserGalleryError> {
    let Some(link) = DocMountFileLinksRepository::new(mount).find_by_id_with_content(link_id)?
    else {
        return Ok(None);
    };
    if !is_photos_relative_path(Some(&link.relative_path)) {
        return Ok(None);
    }
    let meta = parse_kept_image_frontmatter(link.extracted_text.as_deref());
    let link_summary = get_photo_link_summary_by_sha256(mount, &link.sha256)?;
    Ok(Some(json!({
        "linkId": link.id,
        "mountPointId": link.mount_point_id,
        "relativePath": link.relative_path,
        "fileName": link.file_name,
        "blobUrl": build_mount_file_url(&link.mount_point_id, &link.relative_path),
        // v4 `link.originalMimeType ?? 'image/webp'`.
        "mimeType": link.original_mime_type.as_deref().unwrap_or("image/webp"),
        "sha256": link.sha256,
        "fileSizeBytes": link.file_size_bytes,
        "keptAt": link.created_at,
        "caption": meta.caption,
        "tags": meta.tags,
        "generationPromptExcerpt": extract_prompt_excerpt(link.extracted_text.as_deref()),
        "linkSummary": link_summary,
    })))
}

/// v4 `removeFromUserGallery` — remove a gallery entry (cascades to its chunks
/// and, if it was the last hard link to the bytes, drops the file row and blob).
///
/// `/photos` surfaces every `photos/` link across every enabled mount, so the
/// removal target may live in a character vault, a project store, Quilltap
/// General, or Quilltap Uploads — we accept any of them, as long as the link's
/// relativePath is under `photos/`.
///
/// Returns `(deleted, file_gc)`. `(false, false)` for an unknown link (v4's
/// early `return { deleted: false, fileGC: false }` → the route's 404). v4's
/// `deleted: result.fileId !== null` is always `true` once the link resolved
/// above, so the `true` here is exact, not an approximation.
///
/// A WRITE — run inside `Db::write`.
pub fn remove_from_user_gallery(
    mount: &Connection,
    link_id: &str,
) -> Result<(bool, bool), UserGalleryError> {
    let links = DocMountFileLinksRepository::new(mount);
    let Some(link) = links.find_by_id_with_content(link_id)? else {
        return Ok((false, false));
    };
    if !is_photos_relative_path(Some(&link.relative_path)) {
        return Err(UserGalleryError::msg("Link is not a gallery entry"));
    }
    let file_gc = links.delete_with_gc(link_id)?;
    // invalidateMountPoint is a host-side recorded seam (no-op in the core).
    Ok((true, file_gc))
}

/// v4 `saveToUserGallery` — hard-link an image-v2 `FileEntry` into the user's
/// photo gallery (the Quilltap Uploads mount's `photos/` folder). Mirrors the
/// character-vault `keep_image` flow: writes a Markdown context document into
/// `extractedText`, chunks it, and dedupes by sha256 so two saves of the same
/// image are refused.
///
/// `kept_at` is the injected ISO clock (v4 mints `new Date().toISOString()`).
///
/// A WRITE — run inside `Db::write` (both `main` and `mount` are writable).
// v4's input bag lowered flat, matching the `save_to_character_gallery` precedent.
#[allow(clippy::too_many_arguments)]
pub fn save_to_user_gallery(
    main: &Connection,
    mount: &Connection,
    file_id: &str,
    caption: Option<&str>,
    tags: &[String],
    chat_id: Option<&str>,
    user_id: &str,
    bytes: &dyn FileBytesStore,
    kept_at: &str,
) -> Result<Value, UserGalleryError> {
    let Some(target_mount_point_id) =
        crate::services::file_storage::get_user_uploads_store(main, mount)
    else {
        return Err(UserGalleryError::msg(
            "Quilltap Uploads mount has not been provisioned",
        ));
    };

    let files = FilesRepository::new(main);
    let Some(file_entry) = files.find_by_id(file_id)? else {
        return Err(UserGalleryError::msg(format!("Image not found: {file_id}")));
    };
    // v4 reads one row and tests `.category` / `.mimeType` / `.userId`. The core's
    // `FileEntry` projection carries the generation columns but not `userId`, so
    // the ownership check re-reads the same row through the `FileFull`
    // projection — the two reads always resolve the same row.
    if file_entry.category != "IMAGE" && !file_entry.mime_type.starts_with("image/") {
        return Err(UserGalleryError::msg(format!(
            "File {file_id} is not an image (category={})",
            file_entry.category
        )));
    }
    let owner = files.find_full_by_id(file_id)?.map(|f| f.user_id);
    if owner.as_deref() != Some(user_id) {
        return Err(UserGalleryError::msg(format!(
            "File {file_id} is not owned by the calling user"
        )));
    }

    // Read the bytes and hash the ACTUAL bytes once, reusing the value for both
    // the re-save guard and the link write — the FileEntry's sha256 is an
    // upload-time input hash that can diverge from the stored bytes across a
    // transcode, which would let the guard miss duplicates and record a hash that
    // won't match what we store.
    let buffer = match bytes.read_image_buffer(&file_entry) {
        Ok(Some(b)) if !b.is_empty() => b,
        Ok(_) => {
            return Err(UserGalleryError::msg(format!(
                "Image {file_id} has empty bytes"
            )))
        }
        Err(msg) => return Err(UserGalleryError::msg(msg)),
    };
    let sha256 = sha256_of_buffer(&buffer);

    // Re-save guard: refuse a second save for the same sha256 in the user's
    // photos/ folder. Uses the link summary helper to find any existing entry.
    let summary = get_photo_link_summary_by_sha256(mount, &sha256)?;
    if let Some(existing) = summary
        .get("linkers")
        .and_then(Value::as_array)
        .and_then(|ls| {
            ls.iter().find(|l| {
                l.get("mountPointId").and_then(Value::as_str)
                    == Some(target_mount_point_id.as_str())
                    && is_photos_relative_path(l.get("relativePath").and_then(Value::as_str))
            })
        })
    {
        let rel = existing
            .get("relativePath")
            .and_then(Value::as_str)
            .unwrap_or("");
        let at = existing
            .get("linkedAt")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(UserGalleryError::msg(format!(
            "Image already saved to your gallery at {rel} on {at}"
        )));
    }

    // Capture scene state (if chatId was supplied) for the same provenance story
    // the character vault tells.
    let mut parsed_scene_state: Option<SceneState> = None;
    let mut scene_state_malformed = false;
    if let Some(chat_id) = chat_id {
        if let Some(chat) = crate::db::chats_read::find_by_id(main, chat_id)? {
            if let Some(raw) = chat.get("sceneState") {
                if !raw.is_null() {
                    match serde_json::from_value::<SceneState>(raw.clone()) {
                        Ok(scene) => parsed_scene_state = Some(scene),
                        Err(_) => scene_state_malformed = true,
                    }
                }
            }
        }
    }

    let markdown = build_kept_image_markdown(&BuildKeptImageMarkdownInput {
        generation_prompt: file_entry.generation_prompt.as_deref(),
        generation_revised_prompt: file_entry.generation_revised_prompt.as_deref(),
        generation_model: file_entry.generation_model.as_deref(),
        scene_state: parsed_scene_state.as_ref(),
        scene_state_malformed,
        linked_by_name: USER_LINKED_BY_NAME,
        linked_by_id: Some(user_id),
        linked_by_role: Some(KeptImageAttributionRole::User),
        tags,
        caption,
        kept_at,
    });
    let extracted_text_sha256 = sha256_of_string(&markdown);

    let slug = build_slug_and_filename(&BuildSlugAndFilenameInput {
        caption,
        generation_prompt: file_entry.generation_prompt.as_deref(),
        mime_type: &file_entry.mime_type,
        kept_at,
    });
    let desired_path = build_photos_relative_path(&slug.filename);
    let relative_path = resolve_unique_relative_path(mount, &target_mount_point_id, &desired_path)?;
    let links = DocMountFileLinksRepository::new(mount);
    links.ensure_folder_path(&target_mount_point_id, PHOTOS_FOLDER)?;

    let result = links.link_blob_content(&LinkBlobInput {
        mount_point_id: target_mount_point_id.clone(),
        relative_path: relative_path.clone(),
        file_name: basename_of_relative_path(&relative_path),
        file_type: None,
        original_file_name: file_entry.original_filename.clone(),
        original_mime_type: file_entry.mime_type.clone(),
        stored_mime_type: file_entry.mime_type.clone(),
        sha256: sha256.clone(),
        data: buffer,
        description: Some(caption.unwrap_or("").to_string()),
        conversion_status: None,
        extracted_text: Some(markdown.clone()),
        extracted_text_sha256: Some(extracted_text_sha256),
        extraction_status: Some("converted".to_string()),
    })?;

    chunk_and_insert_extracted_text(mount, &result.link_id, &markdown, kept_at)?;
    // invalidateMountPoint / emitDocumentWritten / refreshStats /
    // enqueueEmbeddingJobsForMountPoint are host-side recorded seams (no-ops in
    // the core), exactly as in `save_image_to_album`.

    let mount_point_name = DocMountPointsRepository::new(mount)
        .find_store_naming_by_id(&target_mount_point_id)?
        .map(|mp| mp.name)
        // v4 `mp?.name ?? 'Quilltap Uploads'`.
        .unwrap_or_else(|| "Quilltap Uploads".to_string());

    Ok(json!({
        "linkId": result.link_id,
        "mountPointId": target_mount_point_id,
        "mountPointName": mount_point_name,
        "relativePath": relative_path,
        "keptAt": kept_at,
        "fileId": file_entry.id,
        "sha256": sha256,
    }))
}

/// The shared list/get entry projection (v4 builds the same literal twice —
/// `user-gallery-service.ts:391` and `:465`; the only difference is the list
/// form's extra `relevanceScore`, spliced in by the caller).
fn entry_object(
    mount: &Connection,
    link: &crate::db::doc_mount_file_links::LinkRow,
    mount_point_id: &str,
) -> Result<Value, UserGalleryError> {
    let meta = parse_kept_image_frontmatter(link.extracted_text.as_deref());
    let link_summary = get_photo_link_summary_by_sha256(mount, &link.sha256)?;
    let mut obj = Map::new();
    obj.insert("linkId".into(), json!(link.id));
    obj.insert("mountPointId".into(), json!(mount_point_id));
    obj.insert("relativePath".into(), json!(link.relative_path));
    obj.insert("fileName".into(), json!(link.file_name));
    obj.insert(
        "blobUrl".into(),
        json!(build_mount_file_url(mount_point_id, &link.relative_path)),
    );
    obj.insert(
        "mimeType".into(),
        json!(link.original_mime_type.as_deref().unwrap_or("image/webp")),
    );
    obj.insert("sha256".into(), json!(link.sha256));
    obj.insert("fileSizeBytes".into(), json!(link.file_size_bytes));
    obj.insert("keptAt".into(), json!(link.created_at));
    obj.insert("caption".into(), json!(meta.caption));
    obj.insert("tags".into(), json!(meta.tags));
    obj.insert(
        "generationPromptExcerpt".into(),
        json!(extract_prompt_excerpt(link.extracted_text.as_deref())),
    );
    obj.insert("linkSummary".into(), link_summary);
    Ok(Value::Object(obj))
}

/// v4 `extractPromptExcerpt` (`user-gallery-service.ts:441`) — the first
/// paragraph under the `## Original prompt` heading, truncated to 200 UTF-16
/// units with an ellipsis.
///
/// The v4 source is the regex
/// `/##\s+Original prompt\s*\n+([^\n][^\n]*(?:\n[^\n#][^\n]*)*)/`, hand-rolled
/// here (the core has no JS-regex engine). Read literally: after the heading and
/// its trailing whitespace run — which must END in at least one newline, that's
/// what `\s*\n+` amounts to under backtracking — the capture takes one non-empty
/// line and then every following line that starts with neither a newline (so a
/// blank line ends it) nor a `#` (so the next heading ends it).
fn extract_prompt_excerpt(extracted_text: Option<&str>) -> String {
    let Some(text) = extracted_text else {
        return String::new();
    };
    let Some(para) = match_prompt_paragraph(text) else {
        return String::new();
    };
    let para = js_trim(para);
    if utf16_len(para) > 200 {
        // v4 `${para.slice(0, 200).trimEnd()}…`.
        format!("{}…", js_trim_end(&utf16_truncate(para, 200)))
    } else {
        para.to_string()
    }
}

/// The regex above, applied left-to-right like `String.prototype.match`: the
/// FIRST start position that matches wins.
fn match_prompt_paragraph(text: &str) -> Option<&str> {
    const HEADING: &str = "Original prompt";
    let bytes = text.as_bytes();
    for (start, _) in text.char_indices() {
        // `##`
        if !text[start..].starts_with("##") {
            continue;
        }
        let mut p = start + 2;
        // `\s+` — at least one JS whitespace char.
        let ws_start = p;
        while let Some(c) = text[p..].chars().next() {
            if is_js_ws(c) {
                p += c.len_utf8();
            } else {
                break;
            }
        }
        if p == ws_start {
            continue;
        }
        // `Original prompt`
        if !text[p..].starts_with(HEADING) {
            continue;
        }
        p += HEADING.len();
        // `\s*\n+` — consume the whitespace run, which must contain a newline at
        // or after `p`; `\n+` then eats the trailing newline run. Backtracking
        // makes the last newline in the run the one `\n+` starts at.
        let run_start = p;
        let mut run_end = p;
        while let Some(c) = text[run_end..].chars().next() {
            if is_js_ws(c) {
                run_end += c.len_utf8();
            } else {
                break;
            }
        }
        let Some(last_nl) = text[run_start..run_end].rfind('\n').map(|i| run_start + i) else {
            continue;
        };
        // `\n+` is greedy from `last_nl`; by construction it is the final newline
        // of the run, so it consumes exactly one.
        let mut q = last_nl + 1;
        while bytes.get(q) == Some(&b'\n') {
            q += 1;
        }
        // `[^\n][^\n]*` — one non-empty line.
        let first = text[q..].chars().next();
        if first.is_none() || first == Some('\n') {
            continue;
        }
        let cap_start = q;
        let mut end = q + text[q..].find('\n').unwrap_or(text.len() - q);
        // `(?:\n[^\n#][^\n]*)*` — further lines starting with neither \n nor #.
        loop {
            if bytes.get(end) != Some(&b'\n') {
                break;
            }
            let next = text[end + 1..].chars().next();
            match next {
                Some(c) if c != '\n' && c != '#' => {
                    let line_start = end + 1;
                    end = line_start
                        + text[line_start..]
                            .find('\n')
                            .unwrap_or(text.len() - line_start);
                }
                _ => break,
            }
        }
        return Some(&text[cap_start..end]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_excerpt_takes_the_first_paragraph_under_the_heading() {
        let md = "---\ntags: []\n---\n\n## Original prompt\n\nA brass owl at dusk.\nStill the same paragraph.\n\n## Scene\n\nelsewhere\n";
        assert_eq!(
            extract_prompt_excerpt(Some(md)),
            "A brass owl at dusk.\nStill the same paragraph."
        );
    }

    #[test]
    fn prompt_excerpt_stops_at_the_next_heading_without_a_blank_line() {
        let md = "## Original prompt\n\nfirst line\n## Scene\n";
        assert_eq!(extract_prompt_excerpt(Some(md)), "first line");
    }

    #[test]
    fn prompt_excerpt_absent_heading_or_text_is_empty() {
        assert_eq!(extract_prompt_excerpt(None), "");
        assert_eq!(extract_prompt_excerpt(Some("## Scene\n\nx")), "");
        // Heading present but nothing after it.
        assert_eq!(extract_prompt_excerpt(Some("## Original prompt\n\n")), "");
    }

    #[test]
    fn prompt_excerpt_truncates_at_200_utf16_units() {
        let long = "x".repeat(250);
        let md = format!("## Original prompt\n\n{long}\n");
        let out = extract_prompt_excerpt(Some(&md));
        assert_eq!(utf16_len(&out), 201); // 200 + the ellipsis
        assert!(out.ends_with('…'));
    }

    /// The hand-rolled regex is a transcription, so it is pinned against the REAL
    /// JS engine: every `expected` below is the verbatim output of v4's
    /// `extractPromptExcerpt` (the literal regex from
    /// `user-gallery-service.ts:443`) run under Node 24 over the same input.
    /// Regenerate with `harness/oracle/cases/photos-prompt-excerpt.mjs`.
    #[test]
    fn prompt_excerpt_matches_the_js_regex_vectors() {
        let y198 = "y".repeat(198);
        let x200 = "x".repeat(200);
        let vectors: &[(&str, &str)] = &[
            (
                "---\ntags: []\n---\n\n## Original prompt\n\nA brass owl at dusk.\nStill the same paragraph.\n\n## Scene\n\nelsewhere\n",
                "A brass owl at dusk.\nStill the same paragraph.",
            ),
            ("## Original prompt\n\nfirst line\n## Scene\n", "first line"),
            ("## Scene\n\nx", ""),
            ("## Original prompt\n\n", ""),
            // `###` still matches — the regex finds `##` at offset 1.
            ("### Original prompt\n\nheading three\n", "heading three"),
            ("## Original prompt   \n   \nindented start\n", "indented start"),
            ("##\tOriginal prompt\nno blank line\nsecond\n", "no blank line\nsecond"),
            ("## Original prompt\n\n   trailing spaces   \n", "trailing spaces"),
            ("## Original promptX\n\nnope\n", ""),
            // A `#` may START the captured paragraph; it only terminates a
            // CONTINUATION line (`[^\n][^\n]*` vs `\n[^\n#][^\n]*`).
            ("## Original prompt\n\n#hash starts line\n", "#hash starts line"),
            ("## Original prompt\n\nline one\n\nline two\n", "line one"),
            ("intro\n## Original prompt\n\nafter intro\n", "after intro"),
            ("## Original prompt\n \n \n after mixed ws\n", "after mixed ws"),
        ];
        for (input, expected) in vectors {
            assert_eq!(
                extract_prompt_excerpt(Some(input)),
                *expected,
                "input: {input:?}"
            );
        }
        // The two truncation vectors, built to avoid 250-char literals.
        assert_eq!(
            extract_prompt_excerpt(Some(&format!(
                "## Original prompt\n\n{}\n",
                "x".repeat(250)
            ))),
            format!("{x200}…")
        );
        // 198 y's + three spaces + a continuation line: slice(0,200) then trimEnd
        // drops the two captured spaces, leaving exactly the 198 y's.
        assert_eq!(
            extract_prompt_excerpt(Some(&format!("## Original prompt\n\n{y198}   \nmore\n"))),
            format!("{y198}…")
        );
    }

    #[test]
    fn needs_query_embedding_matches_v4_guard() {
        assert!(!needs_query_embedding(None));
        assert!(!needs_query_embedding(Some("")));
        assert!(!needs_query_embedding(Some("   ")));
        assert!(needs_query_embedding(Some(" owl ")));
    }
}
