//! The `files` row for a picture already written to a mount — v4
//! `lib/files/generated-file-row.ts` (`9ecebf986`, P4.D283).
//!
//! Every image path ends the same way: a storage bridge has put the bytes in a
//! store (a character vault, the Lantern Backgrounds mount, a project store, a
//! wardrobe item's folder) and answered with a storageKey and what it actually
//! stored; the caller then mints the `files` row that gives the picture a URL,
//! a thumbnail and its generation record. This is that row, once — the avatar
//! job, the Lantern's story background, the `generate_image` tool, a wardrobe
//! item's picture and the avatar preview all mint through it.
//!
//! Two rules it keeps for every caller:
//!
//! - **Stored, not sent.** The bridges transcode bitmaps to WebP, so the row's
//!   `mimeType` and `size` are the bridge's `storedMimeType` / `sizeBytes`,
//!   never the input's — a vision provider rejects bytes whose declared type
//!   is wrong ("media_type X but image is Y").
//! - **No label on a generated picture.** `description` is what
//!   `describe_image` and the blind-model fallback read as "what this picture
//!   shows"; a stub such as "Story background for: <title>" shadowed the
//!   prompt and the vision path behind it (bug 132). It is null unless the
//!   caller has a real description to record. The prompt is the account of
//!   record.
//!
//! Dimensions are the ones measured from the stored bytes (see
//! `image_gen::image_attempt::decode_provider_image`); providers often return
//! a different shape than was asked for, so a requested size is never
//! recorded.
//!
//! The row's timestamps are the caller's clock (v4 `repos.files.create` stamps
//! `new Date()`; the jobs here pass their pinned `now`), and the id is the
//! caller's when given (`input.id` — "in the job child the create is buffered
//! and the returned row is synthetic, so callers rely on `input.id`").
//!
//! This runs inside the caller's write — on the writer thread, so its DEBUG is
//! invisible to the thread-scoped capture rig (memory note
//! `capture-rig-writer-thread`); `generated_file_row_equivalence` pins it with
//! the row on the caller thread.

use rusqlite::Connection;

use crate::db::files::{CreateOptions, FileCreate, FileRow, FilesRepository};
use crate::db::DbError;

const LOG_CONTEXT: &str = "files.generated-file-row";

/// What a storage bridge reports having written (v4 `StoredImageBytes`).
#[derive(Clone, Debug, PartialEq)]
pub struct StoredImageBytes {
    pub storage_key: String,
    pub stored_mime_type: String,
    pub size_bytes: i64,
}

/// The provider-bound prompt, the model that answered, and its revision (v4
/// `GeneratedFileRowInput.generation`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenerationRecord {
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub revised_prompt: Option<String>,
}

/// v4 `GeneratedFileRowInput`.
#[derive(Clone, Debug)]
pub struct GeneratedFileRowInput<'a> {
    pub user_id: &'a str,
    /// Pre-chosen row id; a fresh UUID when omitted.
    pub id: Option<&'a str>,
    /// Hash of the stored bytes.
    pub sha256: &'a str,
    pub original_filename: &'a str,
    pub stored: &'a StoredImageBytes,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub linked_to: Vec<String>,
    /// Default `[]`.
    pub tags: Vec<String>,
    /// Default `'GENERATED'`.
    pub source: Option<&'a str>,
    pub generation: Option<&'a GenerationRecord>,
    /// The avatar configuration cache key; null on every other picture.
    pub generation_key: Option<&'a str>,
    /// A real description only — never a label (bug 132). Default null.
    pub description: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub folder_path: Option<&'a str>,
    /// The row's `createdAt` / `updatedAt` (v4's `new Date()` at the create).
    pub now_iso: &'a str,
}

/// v4 `createGeneratedFileRow(repos, input)` — create the IMAGE `files` row
/// for a picture the bridge has already stored. Returns the row as created
/// (the caller's id when given).
pub fn create_generated_file_row(
    main: &Connection,
    input: &GeneratedFileRowInput<'_>,
) -> Result<FileRow, DbError> {
    let id = input
        .id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let source = input.source.unwrap_or("GENERATED");
    let generation = input.generation;
    let file = FileRow {
        id: id.clone(),
        user_id: input.user_id.to_string(),
        sha256: Some(input.sha256.to_string()),
        original_filename: input.original_filename.to_string(),
        mime_type: input.stored.stored_mime_type.clone(),
        size: input.stored.size_bytes,
        width: input.width.map(|w| w as i64),
        height: input.height.map(|h| h as i64),
        is_plain_text: None,
        linked_to: input
            .linked_to
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect(),
        source: source.to_string(),
        category: "IMAGE".to_string(),
        generation_prompt: generation.and_then(|g| g.prompt.clone()),
        generation_model: generation.and_then(|g| g.model.clone()),
        generation_revised_prompt: generation.and_then(|g| g.revised_prompt.clone()),
        generation_key: input.generation_key.map(str::to_string),
        description: input.description.map(str::to_string),
        tags: input
            .tags
            .iter()
            .map(|s| serde_json::Value::String(s.clone()))
            .collect(),
        project_id: input.project_id.map(str::to_string),
        folder_path: input.folder_path.map(str::to_string),
        storage_key: Some(input.stored.storage_key.clone()),
        file_status: Some("ok".to_string()),
        created_at: input.now_iso.to_string(),
        updated_at: input.now_iso.to_string(),
    };

    FilesRepository::new(main).create(
        &FileCreate {
            user_id: file.user_id.clone(),
            sha256: input.sha256.to_string(),
            original_filename: file.original_filename.clone(),
            mime_type: file.mime_type.clone(),
            size: input.stored.size_bytes as f64,
            width: input.width,
            height: input.height,
            is_plain_text: None,
            linked_to: input.linked_to.clone(),
            source: source.to_string(),
            category: "IMAGE".to_string(),
            generation_prompt: file.generation_prompt.clone(),
            generation_model: file.generation_model.clone(),
            generation_revised_prompt: file.generation_revised_prompt.clone(),
            generation_key: file.generation_key.clone(),
            description: file.description.clone(),
            tags: input.tags.clone(),
            project_id: file.project_id.clone(),
            folder_path: file.folder_path.clone(),
            storage_key: file.storage_key.clone(),
            file_status: "ok".to_string(),
        },
        &CreateOptions {
            id: id.clone(),
            created_at: input.now_iso.to_string(),
            updated_at: input.now_iso.to_string(),
        },
    )?;

    tracing::debug!(
        context = LOG_CONTEXT,
        fileId = %id,
        source = source,
        storedMimeType = %input.stored.stored_mime_type,
        sizeBytes = input.stored.size_bytes,
        linkedToJson = %serde_json::Value::Array(
            input
                .linked_to
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect()
        ),
        hasGenerationKey = input.generation_key.is_some(),
        "[GeneratedFileRow] Created image file row"
    );

    Ok(file)
}
