//! Photo album tool handlers — port of v4
//! `lib/tools/handlers/doc-edit/photo-handlers.ts`
//! (`handleKeepImage` / `handleListImages` / `handleAttachImage`).
//!
//! Photos are documents in the character vault — a `photos/` subfolder in the
//! character's mount point. `keep_image` hard-links an existing image binary (dedup
//! by sha256) and writes a Markdown context document as the link's `extractedText`;
//! `list_images` is a facade over the vault search; `attach_image` resurfaces a kept
//! image into the current outgoing message.
//!
//! These handlers compose [`crate::photos::save_image_to_album`] (behind the injected
//! [`FileBytesStore`] bytes seam) + the ported vault reads/search. Each is driven by
//! the dispatcher ([`crate::tools::executor`]) inside a both-connections `Db::write`
//! closure (the wardrobe/doc-edit precedent), since they span the MAIN db
//! (`characters`/`chats`/`files`) and the mount-index db (the store).

use std::sync::LazyLock;

use regex::Regex;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::characters_read;
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkRow, LinkWithContent};
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::files::FilesRepository;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate, JS_WS_CLASS};
use crate::photos::keep_image_markdown::{
    basename_of_relative_path, parse_kept_image_frontmatter, KeptImageAttributionRole,
    KeptImageFrontmatter,
};
use crate::photos::photos_paths::is_photos_relative_path;
use crate::photos::save_image_to_album::{
    save_image_to_album, FileBytesStore, SaveImageAttribution, SaveImageErrorCode,
    SaveImageSideEffects, SaveImageToAlbumInput,
};

/// The context a photo handler needs (the subset of
/// [`crate::services::tool_execution::ToolExecutionContext`] these consume).
#[derive(Debug, Clone)]
pub struct PhotoToolContext {
    pub chat_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub character_id: Option<String>,
    pub embedding_profile_id: Option<String>,
}

/// A photo-handler result — v4's `{ success, result?, error?, formattedText? }`.
/// `result` is the tool's structured output (JSON `Value`); the dispatcher spreads
/// `formattedText` into the row.
#[derive(Debug, Clone)]
pub struct PhotoToolResult {
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub formatted_text: Option<String>,
}

impl PhotoToolResult {
    fn err(message: impl Into<String>) -> Self {
        PhotoToolResult {
            success: false,
            result: None,
            error: Some(message.into()),
            formatted_text: None,
        }
    }
}

/// v4 `KeepImageOutput`.
#[derive(Debug, Serialize)]
struct KeepImageOutput {
    success: bool,
    mount_point: String,
    relative_path: String,
    link_id: String,
    kept_at: String,
    file_id: String,
    sha256: String,
}

/// v4 `ListedImage`.
#[derive(Debug, Clone, Serialize)]
struct ListedImage {
    uuid: String,
    relative_path: String,
    mount_point: String,
    linked_by: Option<String>,
    linked_by_id: Option<String>,
    kept_at: String,
    caption: Option<String>,
    tags: Vec<String>,
    generation_prompt_excerpt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    relevance_score: Option<f64>,
}

/// v4 `ListImagesOutput`.
#[derive(Debug, Serialize)]
struct ListImagesOutput {
    images: Vec<ListedImage>,
    total: usize,
    has_more: bool,
}

/// v4 `AttachedImageDescriptor`. Note the field is camelCase `mimeType` (unlike the
/// snake_case `keep_image`/`list_images` outputs) — v4's descriptor matches the
/// `processToolCalls` image-collector shape.
#[derive(Debug, Serialize)]
struct AttachedImageDescriptor {
    id: String,
    filename: String,
    filepath: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<i64>,
    sha256: String,
}

/// A character's resolved vault (v4 `ResolvedCharacterVault`).
struct ResolvedCharacterVault {
    character_id: String,
    character_name: String,
    mount_point_id: String,
    mount_point_name: String,
}

/// A visible photo vault (self or a transparent peer's), v4 `VisiblePhotoVault`.
struct VisiblePhotoVault {
    /// Carried for parity with v4's struct; the listing uses `mount_point_name`.
    #[allow(dead_code)]
    character_name: String,
    mount_point_id: String,
    mount_point_name: String,
}

/// v4 `resolveActingCharacterVault` = `getCharacterVaultStore` composed with the
/// character read for the name: the overlaid character → `characterDocumentMountPointId`
/// → the mount point (must be `mountType='database'` + `storeType='character'`).
fn resolve_acting_character_vault(
    main: &Connection,
    mount: &Connection,
    character_id: Option<&str>,
) -> Option<ResolvedCharacterVault> {
    let cid = character_id?;
    // v4 `repos.characters.findById` (overlaid) — for the name + FK.
    let character = characters_read::find_by_id(main, mount, cid).ok()??;
    let character_name = character.get("name").and_then(Value::as_str)?.to_string();
    let mp_id = character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)?;
    // v4 getCharacterVaultStore: the mount must be database-backed + a character store.
    let mp = DocMountPointsRepository::new(mount)
        .find_store_naming_by_id(mp_id)
        .ok()??;
    if mp.mount_type != "database" {
        return None;
    }
    if mp.store_type.as_deref() != Some("character") {
        return None;
    }
    Some(ResolvedCharacterVault {
        character_id: character.get("id").and_then(Value::as_str)?.to_string(),
        character_name,
        mount_point_id: mp.id,
        mount_point_name: mp.name,
    })
}

// ===========================================================================
// keep_image
// ===========================================================================

/// v4 `handleKeepImage`. Resolves the acting character's vault, saves the image via
/// [`save_image_to_album`], and maps the `ALREADY_SAVED` collision to the byte-exact
/// keep-specific wording. `kept_at` is the injected ISO clock; `bytes` the image-byte
/// seam; `side_effects` the (recorded) mount-invalidation/embedding-enqueue seam.
#[allow(clippy::too_many_arguments)]
pub fn handle_keep_image(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &PhotoToolContext,
    bytes: &dyn FileBytesStore,
    side_effects: &dyn SaveImageSideEffects,
    kept_at: &str,
) -> PhotoToolResult {
    if ctx.character_id.is_none() {
        return PhotoToolResult::err("keep_image requires a character context");
    }
    let Some(vault) = resolve_acting_character_vault(main, mount, ctx.character_id.as_deref())
    else {
        return PhotoToolResult::err(
            "No database-backed character vault is linked to the acting character",
        );
    };

    // v4 input: uuid (required), caption?, tags? (default []).
    let uuid = arg_str(args, "uuid").unwrap_or_default();
    let caption = arg_str(args, "caption");
    let tags: Vec<String> = args
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let input = SaveImageToAlbumInput {
        mount_point_id: &vault.mount_point_id,
        file_id: &uuid,
        caption: caption.as_deref(),
        tags: &tags,
        // v4: chatId ?? null (the sceneState snapshot source).
        chat_id: Some(ctx.chat_id.as_str()),
        attribution: SaveImageAttribution {
            name: vault.character_name.clone(),
            id: Some(vault.character_id.clone()),
            role: KeptImageAttributionRole::Character,
        },
    };

    match save_image_to_album(main, mount, &input, bytes, side_effects, kept_at) {
        Ok(saved) => {
            let result = KeepImageOutput {
                success: true,
                mount_point: saved.mount_point_name.clone(),
                relative_path: saved.relative_path.clone(),
                link_id: saved.link_id,
                kept_at: saved.kept_at,
                file_id: saved.file_id.clone(),
                sha256: saved.sha256,
            };
            // v4: caption ? ` ("${caption}")` : ''
            let formatted_caption = match caption.as_deref() {
                Some(c) if !c.is_empty() => format!(" (\"{c}\")"),
                _ => String::new(),
            };
            PhotoToolResult {
                success: true,
                result: Some(serde_json::to_value(&result).unwrap_or(Value::Null)),
                error: None,
                formatted_text: Some(format!(
                    "Kept image {} as [{}] {}{}",
                    saved.file_id, saved.mount_point_name, saved.relative_path, formatted_caption
                )),
            }
        }
        Err(err) => {
            // v4: ALREADY_SAVED with the existing path/createdAt → keep-specific wording.
            if err.code == SaveImageErrorCode::AlreadySaved {
                if let (Some(rel), Some(created)) =
                    (&err.existing_relative_path, &err.existing_created_at)
                {
                    return PhotoToolResult::err(format!(
                        "Image already kept by {} on {} as {}",
                        vault.character_name, created, rel
                    ));
                }
            }
            PhotoToolResult::err(err.message)
        }
    }
}

// ===========================================================================
// list_images
// ===========================================================================

/// The embedding boundary the semantic branch of `list_images` uses. Kept as a small
/// trait object so the handler (called from the sync dispatch closure) can await it
/// via the caller's runtime — see the dispatcher wiring.
pub type ListImagesEmbedFn<'a> = dyn Fn(&str) -> Result<Vec<f32>, String> + 'a;

/// v4 `handleListImages`. Collects the visible photo vaults (self + transparent
/// peers under the Shared Vaults gate — `allowCrossCharacterVaultReads`), then either
/// the semantic branch (a non-empty `query` → embed + `search_document_chunks` over
/// `photos/`, with the SILENT fallback to plain listing on any error) or the plain
/// branch (per-vault `photos/` links, `createdAt DESC`). `embed` runs the embedding
/// call (the dispatcher adapts its async provider into this sync closure).
pub fn handle_list_images(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &PhotoToolContext,
    peer_character_ids: &[String],
    embed: &ListImagesEmbedFn<'_>,
) -> PhotoToolResult {
    if ctx.character_id.is_none() {
        return PhotoToolResult::err("list_images requires a character context");
    }
    let Some(self_vault) = resolve_acting_character_vault(main, mount, ctx.character_id.as_deref())
    else {
        return PhotoToolResult::err(
            "No database-backed character vault is linked to the acting character",
        );
    };

    let vaults = collect_visible_photo_vaults(main, mount, &self_vault, peer_character_ids);
    let mount_point_ids: Vec<String> = vaults.iter().map(|v| v.mount_point_id.clone()).collect();

    // v4: limit = max(1, min(input.limit ?? 20, 100)); offset = max(0, input.offset ?? 0).
    // The min-then-max nesting == clamp(1, 100) for the non-NaN inputs the schema
    // permits (a number).
    let limit = arg_num(args, "limit").unwrap_or(20.0).clamp(1.0, 100.0) as usize;
    let offset = arg_num(args, "offset").unwrap_or(0.0).max(0.0) as usize;

    let trimmed_query = arg_str(args, "query")
        .map(|q| js_trim(&q).to_string())
        .unwrap_or_default();
    let tags_filter: Option<Vec<String>> = args.get("tags").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    });
    let saved_by_filter = arg_str(args, "saved_by");

    let links_repo = DocMountFileLinksRepository::new(mount);

    // Branch 1: semantic search across the visible vaults' photos/ folders.
    if !trimmed_query.is_empty() {
        if let Some(page_result) = semantic_branch(
            mount,
            &links_repo,
            &vaults,
            &mount_point_ids,
            &trimmed_query,
            tags_filter.as_deref(),
            saved_by_filter.as_deref(),
            limit,
            offset,
            embed,
        ) {
            return page_result;
        }
        // Fall through to the listing branch on embedding/search failure (SILENT).
    }

    // Branch 2: plain listing of photos/ across visible vaults.
    let mut candidates: Vec<(LinkRow, &VisiblePhotoVault)> = Vec::new();
    for vault in &vaults {
        let links = match links_repo.find_by_mount_point_id(&vault.mount_point_id) {
            Ok(l) => l,
            Err(_) => continue,
        };
        for link in links {
            if !is_photos_relative_path(Some(&link.relative_path)) {
                continue;
            }
            candidates.push((link, vault));
        }
    }
    // v4: sort b.createdAt.localeCompare(a.createdAt) — DESC by createdAt string.
    // ISO timestamps sort identically under code-unit compare; stable to preserve
    // v4's Array.sort stability among equal timestamps.
    candidates.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at));

    let mut filtered: Vec<ListedImage> = Vec::new();
    for (link, vault) in &candidates {
        let meta = parse_kept_image_frontmatter(link.extracted_text.as_deref());
        if !matches_tag_filter(&meta.tags, tags_filter.as_deref()) {
            continue;
        }
        if !matches_saved_by_filter(&meta, saved_by_filter.as_deref()) {
            continue;
        }
        let excerpt = extract_prompt_excerpt(link.extracted_text.as_deref());
        filtered.push(build_listed_image_from_link(
            link, vault, &meta, excerpt, None,
        ));
    }

    finish_list(filtered, offset, limit, false, "No images saved yet.")
}

/// The semantic branch. Returns `None` on any error (v4's silent fall-through).
#[allow(clippy::too_many_arguments)]
fn semantic_branch(
    mount: &Connection,
    links_repo: &DocMountFileLinksRepository<'_>,
    vaults: &[VisiblePhotoVault],
    mount_point_ids: &[String],
    trimmed_query: &str,
    tags_filter: Option<&[String]>,
    saved_by_filter: Option<&str>,
    limit: usize,
    offset: usize,
    embed: &ListImagesEmbedFn<'_>,
) -> Option<PhotoToolResult> {
    let embedding = embed(trimmed_query).ok()?;
    // v4: overscan = max(limit*3, 30)
    let overscan = (limit * 3).max(30);
    let options = crate::services::knowledge_injector::document_search::DocumentSearchOptions {
        mount_point_ids: Some(mount_point_ids.to_vec()),
        path_prefix: Some(format!("{}/", crate::photos::photos_paths::PHOTOS_FOLDER)),
        limit: Some(overscan),
        min_score: Some(0.3),
        query: Some(trimmed_query.to_string()),
        apply_literal_phrase_boost: true,
        ..Default::default()
    };
    let hits = crate::services::knowledge_injector::document_search::search_document_chunks(
        mount, &embedding, &options,
    )
    .ok()?;

    // bestByLink: Map<fileId, { score, mountPointId, relativePath }> — best score wins.
    // Preserve first-seen order (v4 Map iteration order) among distinct fileIds.
    let mut best_order: Vec<String> = Vec::new();
    let mut best: std::collections::HashMap<String, (f64, String, String)> =
        std::collections::HashMap::new();
    for hit in &hits {
        match best.get(&hit.file_id) {
            Some((score, _, _)) if *score >= hit.score => {}
            _ => {
                if !best.contains_key(&hit.file_id) {
                    best_order.push(hit.file_id.clone());
                }
                best.insert(
                    hit.file_id.clone(),
                    (
                        hit.score,
                        hit.mount_point_id.clone(),
                        hit.relative_path.clone(),
                    ),
                );
            }
        }
    }

    let mut matched: Vec<ListedImage> = Vec::new();
    for file_id in &best_order {
        let (score, mount_point_id, relative_path) = best.get(file_id).unwrap();
        let Some(vault) = vaults.iter().find(|v| &v.mount_point_id == mount_point_id) else {
            continue;
        };
        let Ok(Some(link)) = links_repo.find_by_mount_point_and_path(mount_point_id, relative_path)
        else {
            continue;
        };
        if !is_photos_relative_path(Some(&link.relative_path)) {
            continue;
        }
        let meta = parse_kept_image_frontmatter(link.extracted_text.as_deref());
        if !matches_tag_filter(&meta.tags, tags_filter) {
            continue;
        }
        if !matches_saved_by_filter(&meta, saved_by_filter) {
            continue;
        }
        let excerpt = extract_prompt_excerpt(link.extracted_text.as_deref());
        matched.push(build_listed_image_from_link(
            &link,
            vault,
            &meta,
            excerpt,
            Some(*score),
        ));
    }

    // v4: matched.sort((a,b) => (b.relevance_score ?? 0) - (a.relevance_score ?? 0))
    // — stable, DESC by relevance_score.
    matched.sort_by(|a, b| {
        let bs = b.relevance_score.unwrap_or(0.0);
        let as_ = a.relevance_score.unwrap_or(0.0);
        bs.partial_cmp(&as_).unwrap_or(std::cmp::Ordering::Equal)
    });

    Some(finish_list(
        matched,
        offset,
        limit,
        true,
        &format!("No images matched query \"{trimmed_query}\"."),
    ))
}

/// Shared paginate + format tail for both branches. `is_semantic` picks the empty
/// message the caller passes; `no_match_msg` is the branch-specific empty text.
fn finish_list(
    all: Vec<ListedImage>,
    offset: usize,
    limit: usize,
    _is_semantic: bool,
    no_match_msg: &str,
) -> PhotoToolResult {
    let total = all.len();
    // v4: matched.slice(offset, offset + limit)
    let page: Vec<ListedImage> = all.into_iter().skip(offset).take(limit).collect();
    let has_more = offset + limit < total;

    let formatted = if page.is_empty() {
        no_match_msg.to_string()
    } else {
        page.iter()
            .map(format_listed_image_line)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let output = ListImagesOutput {
        images: page,
        total,
        has_more,
    };
    PhotoToolResult {
        success: true,
        result: Some(serde_json::to_value(&output).unwrap_or(Value::Null)),
        error: None,
        formatted_text: Some(formatted),
    }
}

fn collect_visible_photo_vaults(
    main: &Connection,
    mount: &Connection,
    self_vault: &ResolvedCharacterVault,
    peer_character_ids: &[String],
) -> Vec<VisiblePhotoVault> {
    let mut vaults = vec![VisiblePhotoVault {
        character_name: self_vault.character_name.clone(),
        mount_point_id: self_vault.mount_point_id.clone(),
        mount_point_name: self_vault.mount_point_name.clone(),
    }];
    if peer_character_ids.is_empty() {
        return vaults;
    }
    for peer_id in peer_character_ids {
        let Ok(Some(peer)) = characters_read::find_by_id(main, mount, peer_id) else {
            continue;
        };
        // v4: peer.systemTransparency !== true → skip.
        if peer.get("systemTransparency").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(peer_vault) = resolve_acting_character_vault(main, mount, Some(peer_id)) else {
            continue;
        };
        vaults.push(VisiblePhotoVault {
            character_name: peer_vault.character_name,
            mount_point_id: peer_vault.mount_point_id,
            mount_point_name: peer_vault.mount_point_name,
        });
    }
    vaults
}

fn build_listed_image_from_link(
    link: &LinkRow,
    vault: &VisiblePhotoVault,
    meta: &KeptImageFrontmatter,
    generation_prompt_excerpt: String,
    relevance_score: Option<f64>,
) -> ListedImage {
    ListedImage {
        uuid: link.id.clone(),
        relative_path: link.relative_path.clone(),
        mount_point: vault.mount_point_name.clone(),
        linked_by: meta.linked_by.clone(),
        linked_by_id: meta.linked_by_id.clone(),
        kept_at: link.created_at.clone(),
        caption: meta.caption.clone(),
        tags: meta.tags.clone(),
        generation_prompt_excerpt,
        relevance_score,
    }
}

fn matches_tag_filter(link_tags: &[String], filter: Option<&[String]>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    if filter.is_empty() {
        return true;
    }
    let lowered: std::collections::HashSet<String> =
        link_tags.iter().map(|t| t.to_lowercase()).collect();
    filter.iter().any(|f| lowered.contains(&f.to_lowercase()))
}

fn matches_saved_by_filter(meta: &KeptImageFrontmatter, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let f = filter.to_lowercase();
    meta.linked_by.as_deref().map(str::to_lowercase) == Some(f.clone())
        || meta.linked_by_id.as_deref().map(str::to_lowercase) == Some(f)
}

/// v4 `extractPromptExcerpt`: pluck the paragraph after `## Original prompt`, cap at
/// 200 UTF-16 units + `…`. The regex uses JS `\s`; the `[^\n…]`/`[^\n#…]` inner
/// classes are literal-`\n` (not all line terminators), so they are reproduced as
/// negated `\n` classes.
static PROMPT_EXCERPT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let ws = &JS_WS_CLASS[1..JS_WS_CLASS.len() - 1];
    Regex::new(&format!(
        r"##[{ws}]+Original prompt[{ws}]*\n+([^\n][^\n]*(?:\n[^\n#][^\n]*)*)"
    ))
    .unwrap()
});

fn extract_prompt_excerpt(extracted_text: Option<&str>) -> String {
    let Some(text) = extracted_text else {
        return String::new();
    };
    let Some(caps) = PROMPT_EXCERPT_REGEX.captures(text) else {
        return String::new();
    };
    let para = js_trim(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
    // v4: para.length > 200 ? `${para.slice(0,200).trimEnd()}…` : para
    if utf16_len(para) > 200 {
        let sliced = utf16_truncate(para, 200);
        format!("{}…", crate::jsstr::js_trim_end(&sliced))
    } else {
        para.to_string()
    }
}

fn format_listed_image_line(img: &ListedImage) -> String {
    // v4: score ` [score ${relevance_score.toFixed(2)}]` when set.
    let score = match img.relevance_score {
        Some(s) => format!(" [score {}]", crate::jsnum::to_fixed(s, 2)),
        None => String::new(),
    };
    let tags = if img.tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", img.tags.join(", "))
    };
    let caption = match &img.caption {
        Some(c) if !c.is_empty() => format!(": {c}"),
        _ => String::new(),
    };
    let linked_by = img.linked_by.as_deref().unwrap_or("unknown");
    format!(
        "  {}  {}  (kept by {}){}{}{}",
        img.uuid, img.relative_path, linked_by, score, tags, caption
    )
}

// ===========================================================================
// attach_image
// ===========================================================================

/// v4 `handleAttachImage`. Resolves the uuid as a link id (then, as a fallback, an
/// image-v2 file uuid → sha256 → self-vault link), enforces the self-vault-only rule,
/// and builds the descriptor. `image_by_id_sha` provides the fallback FileEntry's
/// `(category, sha256)` for an image-v2 uuid (the images-v2 store read); `None` when
/// there is no such entry.
pub fn handle_attach_image(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &PhotoToolContext,
) -> PhotoToolResult {
    if ctx.character_id.is_none() {
        return PhotoToolResult::err("attach_image requires a character context");
    }
    let Some(vault) = resolve_acting_character_vault(main, mount, ctx.character_id.as_deref())
    else {
        return PhotoToolResult::err(
            "No database-backed character vault is linked to the acting character",
        );
    };

    let uuid = arg_str(args, "uuid").unwrap_or_default();
    let links_repo = DocMountFileLinksRepository::new(mount);
    let files_repo = FilesRepository::new(main);

    // Try the uuid as a link id first.
    let mut link: Option<LinkWithContent> = match links_repo.find_by_id_with_content(&uuid) {
        Ok(l) => l,
        Err(e) => return PhotoToolResult::err(e.to_string()),
    };

    // Fall back to an image-v2 file uuid → sha256 → a link in THIS vault (photos/).
    if link.is_none() {
        if let Ok(Some(entry)) = files_repo.find_by_id(&uuid) {
            if entry.category == "IMAGE" {
                if let Ok(Some((collision_id, _, _))) =
                    find_existing_photos_link_by_sha(mount, &vault.mount_point_id, &entry.sha256)
                {
                    link = links_repo
                        .find_by_id_with_content(&collision_id)
                        .ok()
                        .flatten();
                }
            }
        }
    }

    let Some(link) = link else {
        return PhotoToolResult::err(format!(
            "No kept image found for uuid {} in {}'s vault. Call keep_image first to save it.",
            uuid, vault.character_name
        ));
    };

    // Attach is scoped to the caller's own vault.
    if link.mount_point_id != vault.mount_point_id {
        return PhotoToolResult::err(format!(
            "Image {} lives in another character's vault — keep it in your own photos folder first",
            link.id
        ));
    }
    if !is_photos_relative_path(Some(&link.relative_path)) {
        return PhotoToolResult::err(format!(
            "Link {} is not a kept image (path: {})",
            link.id, link.relative_path
        ));
    }

    let meta = parse_kept_image_frontmatter(link.extracted_text.as_deref());
    let filename = basename_of_relative_path(&link.relative_path);
    // v4: `/api/v1/mount-points/${mountPointId}/blobs/${encodeURI(relativePath)}`
    let filepath = format!(
        "/api/v1/mount-points/{}/blobs/{}",
        link.mount_point_id,
        encode_uri(&link.relative_path)
    );

    // Width/height: opportunistically join from image-v2 by sha256 (best effort).
    let mut width: Option<i64> = None;
    let mut height: Option<i64> = None;
    if let Ok(sisters) = files_repo.find_by_sha256(&link.sha256) {
        if let Some(sister) = sisters.first() {
            if sister.width.is_some() {
                width = sister.width;
            }
            if sister.height.is_some() {
                height = sister.height;
            }
        }
    }

    let descriptor = AttachedImageDescriptor {
        id: link.id.clone(),
        filename: filename.clone(),
        filepath,
        // v4: link.originalMimeType ?? 'application/octet-stream'
        mime_type: link
            .original_mime_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string()),
        size: link.file_size_bytes,
        width,
        height,
        sha256: link.sha256.clone(),
    };

    // v4: caption ` ("${caption}")`, tags ` [${tags.join(', ')}]`.
    let caption_fragment = match &meta.caption {
        Some(c) if !c.is_empty() => format!(" (\"{c}\")"),
        _ => String::new(),
    };
    let tags_fragment = if meta.tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", meta.tags.join(", "))
    };
    // v4: `Attached ${filename} kept by ${meta.linkedBy ?? vault.characterName}${…}${…}`
    let kept_by = meta.linked_by.clone().unwrap_or(vault.character_name);
    PhotoToolResult {
        success: true,
        // v4 result is [descriptor] (an array with the lone descriptor).
        result: Some(Value::Array(vec![
            serde_json::to_value(&descriptor).unwrap_or(Value::Null)
        ])),
        error: None,
        formatted_text: Some(format!(
            "Attached {filename} kept by {kept_by}{caption_fragment}{tags_fragment}"
        )),
    }
}

/// v4 `findExistingPhotosLinkBySha` (the handler's private copy — same as the
/// save-service one): the first `photos/` link in the mount whose file sha matches.
fn find_existing_photos_link_by_sha(
    mount: &Connection,
    mount_point_id: &str,
    sha256: &str,
) -> Result<Option<(String, String, String)>, crate::db::DbError> {
    let links = DocMountFileLinksRepository::new(mount).find_by_mount_point_id(mount_point_id)?;
    for l in links {
        if l.sha256 == sha256 && is_photos_relative_path(Some(&l.relative_path)) {
            return Ok(Some((l.id, l.relative_path, l.created_at)));
        }
    }
    Ok(None)
}

// ===========================================================================
// helpers
// ===========================================================================

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn arg_num(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(Value::as_f64)
}

/// V8 `encodeURI` — percent-encode everything except the "unreserved + reserved"
/// set that `encodeURI` leaves intact. v4 uses `encodeURI(relativePath)` on the
/// blob URL; a vault path is ASCII-ish (`photos/<ts>-<slug>.<ext>`), but reproduce
/// the full rule so non-ASCII slugs round-trip. `encodeURI` does NOT escape:
/// `A-Za-z0-9` and `; , / ? : @ & = + $ - _ . ! ~ * ' ( ) #`. Shared with the
/// W4.4b mount-file attachment loader (`services::chat_files`), which builds the
/// same blob URL.
pub(crate) fn encode_uri(s: &str) -> String {
    const SAFE: &[u8] = b";,/?:@&=+$-_.!~*'()#";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || SAFE.contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(nibble_hex(b >> 4));
            out.push(nibble_hex(b & 0xf));
        }
    }
    out
}

fn nibble_hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'A' + (n - 10)) as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_excerpt_caps_and_ellipsis() {
        let md = "---\n---\n\n## Original prompt\n\nA short prompt.\n\nmore\n";
        assert_eq!(extract_prompt_excerpt(Some(md)), "A short prompt.");

        let long = format!("## Original prompt\n\n{}\n", "x".repeat(250));
        let out = extract_prompt_excerpt(Some(&long));
        assert!(out.ends_with('…'));
        assert_eq!(utf16_len(&out), 201); // 200 + the ellipsis
    }

    #[test]
    fn prompt_excerpt_missing_returns_empty() {
        assert_eq!(extract_prompt_excerpt(Some("no heading here")), "");
        assert_eq!(extract_prompt_excerpt(None), "");
    }

    #[test]
    fn encode_uri_leaves_slashes_escapes_spaces() {
        assert_eq!(encode_uri("photos/a b.webp"), "photos/a%20b.webp");
        assert_eq!(
            encode_uri("photos/2026-05-14T07-22-33.000Z-x.webp"),
            "photos/2026-05-14T07-22-33.000Z-x.webp"
        );
    }

    #[test]
    fn tag_and_saved_by_filters() {
        let meta = KeptImageFrontmatter {
            tags: vec!["Sunset".into()],
            linked_by: Some("Aurora".into()),
            linked_by_id: Some("char-1".into()),
            linked_by_role: "character".into(),
            generation_model: None,
            caption: None,
        };
        assert!(matches_tag_filter(
            &meta.tags,
            Some(&["sunset".to_string()])
        ));
        assert!(!matches_tag_filter(
            &meta.tags,
            Some(&["beach".to_string()])
        ));
        assert!(matches_tag_filter(&meta.tags, None));
        assert!(matches_saved_by_filter(&meta, Some("aurora")));
        assert!(matches_saved_by_filter(&meta, Some("char-1")));
        assert!(!matches_saved_by_filter(&meta, Some("bob")));
        assert!(matches_saved_by_filter(&meta, None));
    }
}
