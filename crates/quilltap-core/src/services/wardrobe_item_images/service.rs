//! The item-image WRITES and the client summary — v4 `lib/wardrobe/
//! item-images.ts` (`7c8572869`, #82): `toWardrobeImageSummary` (`:143-171`),
//! `addWardrobeItemImage` (`:228-281`), `setCurrentWardrobeItemImage`
//! (`:284-299`), `deleteWardrobeItemImage` (`:317-343`).
//!
//! Each operation is ONE v4 operation and runs inside ONE `Db::write` closure
//! at its caller (the route verb, the generation, the job): the bridge write,
//! the `files` row and the item's frontmatter pointer land together. Every
//! fn here takes the two writer connections, so the differentials drive it on
//! the test thread (the capture rig sees its lines).
//!
//! The item's `imageFileId` (the frontmatter's only picture key — the history
//! is the `files` rows) is written ONLY here; the PUT item routes validate a
//! hand-chosen value through `primitives::assert_item_image_choice`.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::files::{CreateOptions, FileCreate, FileRow, FilesRepository};
use crate::db::vault_wardrobe_public::{WardrobePatch, WardrobePublicError, NO_MOUNT_MESSAGE};
use crate::db::DbError;
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_container::{scope_str, WardrobeItemHome};
use crate::services::wardrobe_image_bridge::{
    write_wardrobe_item_image, LeafSeed, WardrobeImageKind, WriteWardrobeItemImageInput,
};
use crate::vault_overlay::WardrobeItem;

use super::primitives::{
    assert_item_image_choice, find_item_image_rows, remove_image_file, ItemImageChoiceError,
};

const LOG_CONTEXT: &str = "wardrobe.item-images";

/// What an item-image operation can fail with — the three shapes v4's route
/// maps (`mapWriteError`, `route.ts:109-121`) and the rest it rethrows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemImageError {
    /// v4 `CharacterArchivedError` — a write against an archived character's
    /// item (→ 409).
    Archived { character_id: String },
    /// v4 `ForeignWardrobeImageError` — the picture is not one of the item's
    /// own (→ 400).
    Foreign { item_id: String, file_id: String },
    /// Anything else v4 lets propagate (→ the middleware's 500), carrying the
    /// thrown message.
    Failed(String),
}

impl ItemImageError {
    /// The error's own message (v4's `.message`).
    pub fn message(&self) -> String {
        match self {
            ItemImageError::Archived { character_id } => {
                DbError::character_archived_message(character_id)
            }
            ItemImageError::Foreign { item_id, file_id } => {
                format!("File {file_id} is not an image of wardrobe item {item_id}")
            }
            ItemImageError::Failed(m) => m.clone(),
        }
    }
}

impl From<DbError> for ItemImageError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::CharacterArchived { character_id } => {
                ItemImageError::Archived { character_id }
            }
            other => ItemImageError::Failed(crate::db::fallback::error_text(&other)),
        }
    }
}

impl From<WardrobePublicError> for ItemImageError {
    fn from(e: WardrobePublicError) -> Self {
        match e {
            WardrobePublicError::Db(db) => db.into(),
            WardrobePublicError::NoMount => {
                ItemImageError::Failed(format!("Cannot update wardrobe item: {NO_MOUNT_MESSAGE}"))
            }
            WardrobePublicError::Cycle(m) => ItemImageError::Failed(m),
        }
    }
}

impl From<ItemImageChoiceError> for ItemImageError {
    fn from(e: ItemImageChoiceError) -> Self {
        match e {
            ItemImageChoiceError::Foreign { item_id, file_id } => {
                ItemImageError::Foreign { item_id, file_id }
            }
            ItemImageChoiceError::Read(m) => ItemImageError::Failed(m),
        }
    }
}

/// v4 `wardrobeImageUrl(fileId)`.
pub fn wardrobe_image_url(file_id: &str) -> String {
    format!("/api/v1/files/{file_id}")
}

/// v4 `wardrobeImageThumbnailUrl(fileId)`.
pub fn wardrobe_image_thumbnail_url(file_id: &str) -> String {
    format!("/api/v1/files/{file_id}?action=thumbnail")
}

/// v4 `toWardrobeImageSummary(file)` — `{ fileId, url, thumbnailUrl, source,
/// createdAt, prompt?, model? }` in that key order; `prompt` / `model` only
/// when the generation fields are TRUTHY (an empty string is omitted).
pub fn to_wardrobe_image_summary(file: &FileRow) -> Value {
    let mut m = Map::new();
    m.insert("fileId".into(), json!(file.id));
    m.insert("url".into(), json!(wardrobe_image_url(&file.id)));
    m.insert(
        "thumbnailUrl".into(),
        json!(wardrobe_image_thumbnail_url(&file.id)),
    );
    m.insert("source".into(), json!(file.source));
    m.insert("createdAt".into(), json!(file.created_at));
    if let Some(p) = file.generation_prompt.as_deref().filter(|s| !s.is_empty()) {
        m.insert("prompt".into(), json!(p));
    }
    if let Some(p) = file.generation_model.as_deref().filter(|s| !s.is_empty()) {
        m.insert("model".into(), json!(p));
    }
    Value::Object(m)
}

/// v4 `listWardrobeItemImages(repos, itemId)` — the item's pictures, newest
/// first (P4.D255's primitive under v4's name).
pub fn list_wardrobe_item_images(
    main: &Connection,
    item_id: &str,
) -> Result<Vec<FileRow>, DbError> {
    find_item_image_rows(main, item_id)
}

/// v4 `AddWardrobeItemImageInput`.
#[derive(Debug, Clone)]
pub struct AddWardrobeItemImageInput<'a> {
    pub user_id: &'a str,
    pub kind: WardrobeImageKind,
    pub content: &'a [u8],
    pub content_type: &'a str,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub generation_prompt: Option<&'a str>,
    pub generation_model: Option<&'a str>,
    pub generation_revised_prompt: Option<&'a str>,
}

/// v4 `addWardrobeItemImage`'s `{ file, item }`.
#[derive(Debug, Clone)]
pub struct AddedWardrobeItemImage {
    pub file: FileRow,
    pub item: Option<WardrobeItem>,
}

/// The ids and instants an operation mints (v4 `randomUUID()` / `new Date()`),
/// injected so a caller (or a differential) can pin them.
#[derive(Debug, Clone)]
pub struct ImageMint {
    /// The new `files` row id.
    pub file_id: String,
    /// The `files` row's `createdAt` / `updatedAt`.
    pub now_iso: String,
    /// The bridge's leaf-name seed.
    pub leaf: LeafSeed,
}

impl ImageMint {
    /// The production mint: fresh UUIDs, the wall clock.
    pub fn now() -> Self {
        let now_ms = crate::clock::now_unix_ms();
        ImageMint {
            file_id: uuid::Uuid::new_v4().to_string(),
            now_iso: crate::clock::iso_from_unix_ms(now_ms),
            leaf: LeafSeed {
                now_ms,
                uuid: uuid::Uuid::new_v4().to_string(),
            },
        }
    }
}

/// v4 `addWardrobeItemImage(repos, home, input)` — store a new picture for
/// the item and make it current: the mount (the tombstone refuses FIRST),
/// the bridge write, the `files` row, then ONE item update. The update is not
/// deferred — the editor expects the current pointer to be durable when the
/// response returns.
pub fn add_wardrobe_item_image(
    main: &Connection,
    mount: &Connection,
    home: &WardrobeItemHome,
    input: &AddWardrobeItemImageInput<'_>,
    mint: &ImageMint,
    blob_webp: &dyn WebpTranscoder,
) -> Result<AddedWardrobeItemImage, ItemImageError> {
    let mount_point_id = home.resolve_mount(main)?;
    // v4's CURLY quotes.
    let description = format!("Wardrobe image for “{}”", home.item_title());

    let written = write_wardrobe_item_image(
        mount,
        &WriteWardrobeItemImageInput {
            mount_point_id: &mount_point_id,
            item_id: home.item_id(),
            kind: input.kind,
            content: input.content,
            content_type: input.content_type,
            description: Some(&description),
            leaf_name: None,
        },
        &mint.leaf,
        blob_webp,
    )?;

    let file = FileRow {
        id: mint.file_id.clone(),
        user_id: input.user_id.to_string(),
        sha256: Some(written.sha256.clone()),
        original_filename: written.leaf_name.clone(),
        mime_type: written.stored_mime_type.clone(),
        size: written.size_bytes,
        width: input.width,
        height: input.height,
        is_plain_text: None,
        linked_to: vec![json!(home.item_id())],
        source: input.kind.file_source().to_string(),
        category: "IMAGE".to_string(),
        generation_prompt: input.generation_prompt.map(str::to_string),
        generation_model: input.generation_model.map(str::to_string),
        generation_revised_prompt: input.generation_revised_prompt.map(str::to_string),
        generation_key: None,
        description: Some(description),
        tags: vec![json!(home.item_id())],
        project_id: None,
        folder_path: None,
        storage_key: Some(written.storage_key.clone()),
        file_status: Some("ok".to_string()),
        created_at: mint.now_iso.clone(),
        updated_at: mint.now_iso.clone(),
    };
    create_file_row(main, &file)?;

    let item = home.update(
        main,
        mount,
        &WardrobePatch {
            image_file_id: Some(Some(file.id.clone())),
            ..Default::default()
        },
    )?;

    tracing::info!(
        context = LOG_CONTEXT,
        scope = scope_str(home.scope),
        itemId = %home.item_id(),
        fileId = %file.id,
        kind = input.kind.as_str(),
        sizeBytes = written.size_bytes,
        "[WardrobeImages] Added wardrobe item image"
    );
    Ok(AddedWardrobeItemImage { file, item })
}

/// `files.create` of a fully-formed row (v4 `repos.files.create(data, { id })`
/// — the row's own timestamps as the create's).
pub(crate) fn create_file_row(main: &Connection, file: &FileRow) -> Result<(), DbError> {
    let strings = |v: &[Value]| -> Vec<String> {
        v.iter()
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect()
    };
    FilesRepository::new(main).create(
        &FileCreate {
            user_id: file.user_id.clone(),
            sha256: file.sha256.clone().unwrap_or_default(),
            original_filename: file.original_filename.clone(),
            mime_type: file.mime_type.clone(),
            size: file.size as f64,
            width: file.width.map(|w| w as f64),
            height: file.height.map(|h| h as f64),
            is_plain_text: file.is_plain_text,
            linked_to: strings(&file.linked_to),
            source: file.source.clone(),
            category: file.category.clone(),
            generation_prompt: file.generation_prompt.clone(),
            generation_model: file.generation_model.clone(),
            generation_revised_prompt: file.generation_revised_prompt.clone(),
            generation_key: file.generation_key.clone(),
            description: file.description.clone(),
            tags: strings(&file.tags),
            project_id: file.project_id.clone(),
            folder_path: file.folder_path.clone(),
            storage_key: file.storage_key.clone(),
            file_status: file.file_status.clone().unwrap_or_else(|| "ok".to_string()),
        },
        &CreateOptions {
            id: file.id.clone(),
            created_at: file.created_at.clone(),
            updated_at: file.updated_at.clone(),
        },
    )
}

/// v4 `setCurrentWardrobeItemImage(repos, home, fileId)` — make one of the
/// item's own pictures current. The ownership check first, then the mount (a
/// tombstone refuses here, before the write, the way every other write does),
/// then the update.
pub fn set_current_wardrobe_item_image(
    main: &Connection,
    mount: &Connection,
    home: &WardrobeItemHome,
    file_id: &str,
) -> Result<String, ItemImageError> {
    assert_item_image_choice(main, home.item_id(), Some(file_id))?;
    home.resolve_mount(main)?;
    home.update(
        main,
        mount,
        &WardrobePatch {
            image_file_id: Some(Some(file_id.to_string())),
            ..Default::default()
        },
    )?;
    tracing::debug!(
        context = LOG_CONTEXT,
        itemId = %home.item_id(),
        fileId = %file_id,
        "[WardrobeImages] Set current wardrobe item image"
    );
    Ok(file_id.to_string())
}

/// v4 `deleteWardrobeItemImage(repos, home, fileId)` — delete one of the
/// item's pictures. When it was current, the next-newest becomes current (or
/// none); a DANGLING pointer (current names no listed picture) is repaired the
/// same way. Answers the new current id.
pub fn delete_wardrobe_item_image(
    main: &Connection,
    mount: &Connection,
    home: &WardrobeItemHome,
    file_id: &str,
) -> Result<Option<String>, ItemImageError> {
    let images = list_wardrobe_item_images(main, home.item_id())?;
    let Some(target) = images.iter().find(|f| f.id == file_id) else {
        return Err(ItemImageError::Foreign {
            item_id: home.item_id().to_string(),
            file_id: file_id.to_string(),
        });
    };

    // Refuse a tombstone before touching anything.
    home.resolve_mount(main)?;
    remove_image_file(main, mount, home.item_id(), target)?;

    let mut current = home.image_file_id().map(str::to_string);
    let dangling = current
        .as_deref()
        .is_some_and(|c| !images.iter().any(|f| f.id == c));
    if current.as_deref() == Some(file_id) || dangling {
        current = images
            .iter()
            .find(|f| f.id != file_id)
            .map(|f| f.id.clone());
        home.update(
            main,
            mount,
            &WardrobePatch {
                image_file_id: Some(current.clone()),
                ..Default::default()
            },
        )?;
    }

    tracing::info!(
        context = LOG_CONTEXT,
        itemId = %home.item_id(),
        fileId = %file_id,
        current = current.as_deref().unwrap_or("null"),
        "[WardrobeImages] Deleted wardrobe item image"
    );
    Ok(current)
}
