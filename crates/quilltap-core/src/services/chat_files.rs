//! The LLM-load half of v4 `lib/chat-files-v2.ts` — turning a chat's file ids
//! into provider attachments at send time. (The upload/ingest half — S3/local
//! storage, `uploadChatFile`, `validateChatFile` — is Phase-4 host work, out of
//! scope.)
//!
//! Two byte sources:
//!
//! * a legacy `files`-table row → bytes via the injected [`FileBytesStore`] seam
//!   (v4 `fileStorageManager.downloadFile(entry)`);
//! * a Scriptorium mount file → bytes from the mount-index DB (`doc_mount_blobs`),
//!   no host seam.
//!
//! Both apply the resize DECISION over the injected
//! [`crate::files::image_processing::ImageTranscoder`] seam.
//!
//! Faithful quirks: NO dedup by id (a repeated id loads twice); a per-file load
//! failure is skipped + logged, never fatal; the legacy path keeps the ORIGINAL
//! `fileEntry.size` on the descriptor while the mount path uses the post-resize
//! `buffer.length`.

use serde_json::{json, Value};

use crate::db::files::FileEntry;
use crate::db::runtime::Db;
use crate::files::image_processing::{
    calculate_base64_size, can_resize_image, get_provider_max_base64_size,
    resize_image_for_provider, ImageTranscoder, DEFAULT_QUALITY,
};
use crate::model::completion::CompletionProvider;
use crate::services::file_fallback::{self, FallbackDeps, FallbackFile, FallbackType};
use crate::services::message_context::LanternLoad;

/// The host byte store (v4 `fileStorageManager.downloadFile(entry)`) — reads a
/// legacy `files`-table entry's bytes from S3/local disk. `Err` = the download
/// failed (v4 throws; the caller skips the file). The differential injects a
/// canned per-fileId store mirrored by the oracle's FSM mock; the production impl
/// is Phase 4.
pub trait FileBytesStore: Send + Sync {
    fn download_file(&self, entry: &FileEntry) -> Result<Vec<u8>, String>;
}

/// A store that fails every read (an instance with no host byte layer wired).
pub struct NotConfiguredBytes;
impl FileBytesStore for NotConfiguredBytes {
    fn download_file(&self, _entry: &FileEntry) -> Result<Vec<u8>, String> {
        Err("file storage is not configured on this host".to_string())
    }
}

/// v4 `LoadChatFilesOptions`.
#[derive(Clone, Debug, Default)]
pub struct LoadChatFilesOptions {
    /// For size-limit calculation (`None` disables resize on the legacy path).
    pub provider: Option<String>,
    /// Default `true`.
    pub auto_resize: bool,
}

impl LoadChatFilesOptions {
    /// v4's `{ provider }` construction (autoResize defaults true).
    pub fn with_provider(provider: Option<String>) -> Self {
        LoadChatFilesOptions {
            provider,
            auto_resize: true,
        }
    }
}

/// v4 `getFileApiPath(fileId)` — `/api/v1/files/{fileId}`.
fn get_file_api_path(file_id: &str) -> String {
    format!("/api/v1/files/{file_id}")
}

/// v4 `readFileAsBase64(fileId, mimeType, provider?)` — download + (conditional)
/// resize → `(base64, outputMimeType)`. `Err(message)` on any failure (v4 throws;
/// the loop catches + skips). The `wasResized` flag v4 returns is dropped (the
/// caller reads only `(data, mimeType)`).
fn read_file_as_base64(
    db: &Db,
    bytes: &dyn FileBytesStore,
    transcoder: &dyn ImageTranscoder,
    file_id: &str,
    mime_type: &str,
    provider: Option<&str>,
) -> Result<(String, String), String> {
    let fid = file_id.to_string();
    let entry = db
        .read_main(move |c| crate::db::files::FilesRepository::new(c).find_by_id(&fid))
        .map_err(|e| format!("files.findById failed: {e}"))?;
    let entry = entry.ok_or_else(|| format!("File not found: {file_id}"))?;

    if entry
        .storage_key
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(format!(
            "File {file_id} has no storage key - file may need migration"
        ));
    }

    let mut buffer = bytes.download_file(&entry)?;
    let mut output_mime_type = mime_type.to_string();

    if let Some(provider) = provider {
        if mime_type.starts_with("image/") && can_resize_image(mime_type) {
            let max_base64_size = get_provider_max_base64_size(provider);
            let base64_size = calculate_base64_size(buffer.len());
            if base64_size > max_base64_size {
                let result = resize_image_for_provider(
                    provider,
                    &buffer,
                    mime_type,
                    DEFAULT_QUALITY,
                    transcoder,
                );
                if result.was_resized {
                    buffer = result.buffer;
                    output_mime_type = result.mime_type;
                }
            }
        }
    }

    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&buffer);
    Ok((data, output_mime_type))
}

/// v4 `loadMountFileAsAttachment(mountFileId, options)` — a Scriptorium mount file
/// as a provider attachment descriptor (`FileAttachment` JSON), or `None` when it
/// can't be resolved/read.
fn load_mount_file_as_attachment(
    db: &Db,
    transcoder: &dyn ImageTranscoder,
    mount_file_id: &str,
    options: &LoadChatFilesOptions,
) -> Option<Value> {
    let provider = options.provider.clone();
    let auto_resize = options.auto_resize;

    // Try as a link id first, then fall back to treating the arg as a file id.
    let arg = mount_file_id.to_string();
    let mount_link = db
        .read_mount_index(move |c| {
            let repo = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(c);
            if let Some(link) = repo.find_by_id_with_content(&arg)? {
                return Ok(Some(link));
            }
            let links = repo.find_with_content_by_file_id(&arg)?;
            Ok(links.into_iter().next())
        })
        .ok()
        .flatten()?;

    let file_id = mount_link.file_id.clone();
    let blob = db
        .read_mount_index(move |c| {
            crate::db::doc_mount_blobs::DocMountBlobsRepository::new(c).find_by_file_id(&file_id)
        })
        .ok()
        .flatten();
    let blob = match blob {
        Some(b) => b,
        // Native-text documents (.md/.txt/.json in a database store) have no
        // blob; their bytes live in `doc_mount_documents`. Serve the document
        // text so an attached markdown document actually reaches the LLM
        // (v4 bug 38, `7bcd8515`) instead of 404ing.
        None => {
            if let Some(text_mime) =
                crate::services::mount_index::path_utils::native_text_attachment_mime(
                    &mount_link.relative_path,
                )
            {
                let fid = mount_link.file_id.clone();
                if let Some(content) = db
                    .read_mount_index(move |c| {
                        crate::db::doc_mount_documents::DocMountDocumentsRepository::new(c)
                            .find_content_by_file_id(&fid)
                    })
                    .ok()
                    .flatten()
                {
                    use base64::Engine;
                    let bytes = content.into_bytes();
                    let size = bytes.len();
                    let url = format!(
                        "/api/v1/mount-points/{}/files/{}",
                        mount_link.mount_point_id,
                        crate::tools::photo::encode_uri(&mount_link.relative_path)
                    );
                    let filename = mount_link
                        .original_file_name
                        .clone()
                        .unwrap_or_else(|| mount_link.file_name.clone());
                    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    return Some(json!({
                        "id": mount_link.id,
                        "filepath": url,
                        "filename": filename,
                        "mimeType": text_mime,
                        "size": size,
                        "data": data,
                        "url": url,
                    }));
                }
            }
            return None;
        }
    };

    let blob_id = blob.id.clone();
    let bytes = db
        .read_mount_index(move |c| {
            crate::db::doc_mount_blobs::DocMountBlobsRepository::new(c).read_data(&blob_id)
        })
        .ok()
        .flatten()?;

    let mut buffer = bytes;
    let mut output_mime_type = blob.stored_mime_type.clone();

    if auto_resize {
        if let Some(provider) = provider.as_deref() {
            if output_mime_type.starts_with("image/") && can_resize_image(&output_mime_type) {
                let max_base64_size = get_provider_max_base64_size(provider);
                let base64_size = calculate_base64_size(buffer.len());
                if base64_size > max_base64_size {
                    let result = resize_image_for_provider(
                        provider,
                        &buffer,
                        &output_mime_type,
                        DEFAULT_QUALITY,
                        transcoder,
                    );
                    if result.was_resized {
                        buffer = result.buffer;
                        output_mime_type = result.mime_type;
                    }
                }
            }
        }
    }

    let filename = mount_link
        .original_file_name
        .clone()
        .unwrap_or_else(|| mount_link.file_name.clone());
    let url = format!(
        "/api/v1/mount-points/{}/blobs/{}",
        mount_link.mount_point_id,
        crate::tools::photo::encode_uri(&mount_link.relative_path)
    );
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(&buffer);

    Some(json!({
        "id": mount_link.id,
        "filepath": url,
        "filename": filename,
        "mimeType": output_mime_type,
        "size": buffer.len(),
        "data": data,
        "url": url,
    }))
}

/// v4 `loadChatFilesForLLM(fileIds, options)` — load every id (input order, NO
/// dedup) into a provider attachment (`FileAttachment` JSON). A legacy `files` hit
/// takes the FSM path; otherwise the mount path. A per-file failure is skipped +
/// logged (never fatal). The returned array can be SHORTER than `file_ids` when
/// loads fail — v4's positional pairing downstream relies on this (faithfully
/// kept).
pub fn load_chat_files_for_llm(
    db: &Db,
    bytes: &dyn FileBytesStore,
    transcoder: &dyn ImageTranscoder,
    file_ids: &[String],
    options: &LoadChatFilesOptions,
) -> Vec<Value> {
    let mut attachments: Vec<Value> = Vec::new();

    for file_id in file_ids {
        let fid = file_id.clone();
        let entry =
            db.read_main(move |c| crate::db::files::FilesRepository::new(c).find_by_id(&fid));

        match entry {
            Ok(Some(file_entry)) => {
                // Legacy `files` table hit — FSM path.
                let provider = if options.auto_resize {
                    options.provider.as_deref()
                } else {
                    None
                };
                match read_file_as_base64(
                    db,
                    bytes,
                    transcoder,
                    file_id,
                    &file_entry.mime_type,
                    provider,
                ) {
                    Ok((data, mime_type)) => {
                        attachments.push(json!({
                            "id": file_entry.id,
                            "filepath": get_file_api_path(&file_entry.id),
                            "filename": file_entry.original_filename,
                            // The (possibly resized) output mime; `size` is the
                            // ORIGINAL fileEntry.size (v4 does not re-derive it here).
                            "mimeType": mime_type,
                            "size": file_entry.size,
                            "data": data,
                        }));
                    }
                    Err(_) => {
                        // Load failure — skip + log (v4's catch).
                        continue;
                    }
                }
            }
            Ok(None) => {
                // Not in `files` → try the mount store.
                if let Some(mount_attachment) =
                    load_mount_file_as_attachment(db, transcoder, file_id, options)
                {
                    attachments.push(mount_attachment);
                }
                // else: not found in either → log + skip (nothing pushed).
            }
            Err(_) => {
                // A DB read error is swallowed like v4's per-file catch.
                continue;
            }
        }
    }

    attachments
}

// ============================================================================
// loadAndProcessFiles (context-builder.service.ts:116–194) + the Lantern K seam
// ============================================================================

/// The everything the attachment-processing composition needs (v4's `repos` +
/// `connectionProfile` + `userId`, plus the two host seams).
pub struct ProcessFilesDeps<'a, CMP: CompletionProvider> {
    pub db: &'a Db,
    pub bytes: &'a dyn FileBytesStore,
    pub transcoder: &'a dyn ImageTranscoder,
    pub completion: &'a CMP,
    pub user_id: &'a str,
    /// The wall clock for the vision `logLLMCall` `durationMs` (frozen in the
    /// differential; the dump normalizes it).
    pub now_ms: i64,
}

impl<'a, CMP: CompletionProvider> ProcessFilesDeps<'a, CMP> {
    fn fallback_deps(&self) -> FallbackDeps<'a, CMP> {
        FallbackDeps {
            db: self.db,
            completion: self.completion,
            transcoder: self.transcoder,
            user_id: self.user_id,
            now_ms: self.now_ms,
        }
    }
}

/// The observable outcome of [`load_and_process_files`] — the subset the
/// orchestrator threads onto the user message + into `buildMessageContext`.
#[derive(Clone, Debug, Default)]
pub struct ProcessedFiles {
    /// The `matched` file ids (chat-linked ∩ requested) — the `addLink` targets.
    pub attached_file_ids: Vec<String>,
    /// v4 `messageContentPrefix` — `None` when empty (byte-identical downstream).
    pub message_content_prefix: Option<String>,
    /// v4 `attachmentsToSend` — the kept (provider-supported) attachments.
    pub attachments_to_send: Vec<Value>,
}

/// v4 `loadAndProcessFiles(chatId, fileIds, connectionProfile, userId)` — read the
/// chat's linked files, load + fallback-dispatch each, accumulate the message
/// prefix, and keep only the provider-supported attachments.
pub async fn load_and_process_files<CMP: CompletionProvider>(
    deps: &ProcessFilesDeps<'_, CMP>,
    chat_id: &str,
    connection_profile: &Value,
    file_ids: &[String],
) -> ProcessedFiles {
    if file_ids.is_empty() {
        return ProcessedFiles::default();
    }
    let provider = connection_profile
        .get("provider")
        .and_then(Value::as_str)
        .map(str::to_string);

    // The chat's linked files, filtered to the requested ids (rowid order).
    let chat_id_owned = chat_id.to_string();
    let chat_files = deps
        .db
        .read_main(move |c| {
            crate::db::files::FilesRepository::new(c).find_link_meta_by_linked_to(&chat_id_owned)
        })
        .unwrap_or_default();
    let requested: std::collections::HashSet<&str> = file_ids.iter().map(String::as_str).collect();
    let matched: Vec<crate::db::files::FileLinkMeta> = chat_files
        .into_iter()
        .filter(|f| requested.contains(f.id.as_str()))
        .collect();

    let options = LoadChatFilesOptions::with_provider(provider);
    let ids: Vec<String> = matched.iter().map(|f| f.id.clone()).collect();
    let file_attachments =
        load_chat_files_for_llm(deps.db, deps.bytes, deps.transcoder, &ids, &options);

    let fallback_deps = deps.fallback_deps();
    let mut message_content_prefix = String::new();
    let mut fallback_results: Vec<file_fallback::FallbackResult> = Vec::new();
    // Positional pairing (v4's `attachedFiles[i]`), faithfully kept even when a
    // load failure shortened `file_attachments`.
    for (i, fa) in file_attachments.iter().enumerate() {
        let Some(meta) = matched.get(i) else { break };
        let file = FallbackFile {
            id: meta.id.clone(),
            filename: meta.original_filename.clone(),
            // fileMetadata (attachedFiles) carries the ORIGINAL mime.
            mime_type: meta.mime_type.clone(),
            data: fa.get("data").and_then(Value::as_str).map(str::to_string),
        };
        let result = file_fallback::process_file_attachment_fallback(
            &fallback_deps,
            &file,
            connection_profile,
        )
        .await;
        message_content_prefix.push_str(&file_fallback::format_fallback_as_message_prefix(&result));
        fallback_results.push(result);
    }

    // Keep only provider-supported attachments (`unsupported` with NO error).
    let attachments_to_send: Vec<Value> = file_attachments
        .into_iter()
        .enumerate()
        .filter(|(idx, _)| match fallback_results.get(*idx) {
            None => true,
            Some(f) => f.type_ == FallbackType::Unsupported && f.error.is_none(),
        })
        .map(|(_, fa)| fa)
        .collect();

    ProcessedFiles {
        attached_file_ids: matched.iter().map(|f| f.id.clone()).collect(),
        message_content_prefix: if message_content_prefix.is_empty() {
            None
        } else {
            Some(message_content_prefix)
        },
        attachments_to_send,
    }
}

/// The Lantern K-seam body (v4 `buildMessageContext` section K, 662–757). Loads
/// the collected prior-image file ids, fallback-dispatches each, and returns the
/// accumulated description prefix + the kept attachments (v4:
/// `[...attachmentsToSend, ...lanternAttachmentsToKeep]` is merged by the caller).
pub async fn load_lantern_images<CMP: CompletionProvider>(
    deps: &ProcessFilesDeps<'_, CMP>,
    connection_profile: &Value,
    file_ids: &[String],
    provider: &str,
) -> LanternLoad {
    let options = LoadChatFilesOptions::with_provider(Some(provider.to_string()));
    let extra = load_chat_files_for_llm(deps.db, deps.bytes, deps.transcoder, file_ids, &options);
    if extra.is_empty() {
        return LanternLoad::default();
    }

    let fallback_deps = deps.fallback_deps();
    let mut lantern_prefix = String::new();
    let mut keep: Vec<Value> = Vec::new();
    for fa in extra {
        let file = FallbackFile {
            id: fa
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            filename: fa
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            // Lantern's fileMetadata carries the LOADED (possibly resized) mime.
            mime_type: fa
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            data: fa.get("data").and_then(Value::as_str).map(str::to_string),
        };
        let result = file_fallback::process_file_attachment_fallback(
            &fallback_deps,
            &file,
            connection_profile,
        )
        .await;
        lantern_prefix.push_str(&file_fallback::format_fallback_as_message_prefix(&result));
        if result.type_ == FallbackType::Unsupported && result.error.is_none() {
            keep.push(fa);
        }
    }

    LanternLoad {
        prefix: lantern_prefix,
        attachments: keep,
    }
}

/// The production [`crate::services::message_context::MessageContextSeams`] — the
/// real Lantern K-seam loader. Holds the byte/transcoder/completion seams + the
/// responding connection profile; the orchestrator swaps this in for
/// `NoopMessageContextSeams` at the `build_message_context` call.
pub struct RealMessageContextSeams<'a, CMP: CompletionProvider> {
    pub deps: ProcessFilesDeps<'a, CMP>,
    pub connection_profile: &'a Value,
}

impl<CMP: CompletionProvider + Sync> crate::services::message_context::MessageContextSeams
    for RealMessageContextSeams<'_, CMP>
{
    async fn load_lantern_images(&self, file_ids: &[String], _provider: &str) -> LanternLoad {
        // v4's K section uses `connectionProfile.provider` (the responding
        // profile), NOT the formatting provider the message-context seam passes
        // (which is the possibly-rerouted `effectiveProfile.provider`).
        let provider = self
            .connection_profile
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        load_lantern_images(&self.deps, self.connection_profile, file_ids, &provider).await
    }
}

// ============================================================================
// The upload/ingest half (v4 `lib/chat-files-v2.ts` `uploadChatFile`) — P4.6ah.
// Composed over the `file_storage.rs` write seams; runs on the writer.
// ============================================================================

use crate::db::files::{CreateOptions, FileCreate, FileFull, FilesRepository};
use crate::files::text_detection::{detect_text_content, get_best_mime_type};
use crate::services::file_storage::{
    get_inherited_tags, transcode_to_webp, write_project_file_to_mount_store,
    write_user_upload_to_mount_store, PixelCodec, TRANSCODE_WEBP_QUALITY,
};

/// v4 `MAX_FILE_SIZE` (`chat-files-v2.ts:72`) — 10 MB (size only, no type gate).
const MAX_CHAT_FILE_SIZE: usize = 10 * 1024 * 1024;

/// The uploaded-file result (v4 `ChatFileUploadResult`) — the fields the chat-file
/// route echoes as `{file}` plus the sha the caller drops.
#[derive(Debug, Clone)]
pub struct ChatUploadedFile {
    pub id: String,
    pub filename: String,
    pub filepath: String,
    pub mime_type: String,
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// v4 `ChatFileDuplicateResult` — the conflict envelope the route returns 200.
#[derive(Debug, Clone)]
pub struct ChatUploadDuplicate {
    /// `'filename' | 'content' | 'both'`.
    pub conflict_type: String,
    pub existing_id: String,
    pub existing_filename: String,
    pub existing_size: i64,
    pub existing_created_at: String,
    pub existing_sha256: String,
    pub new_filename: String,
    pub new_size: i64,
    pub new_sha256: String,
}

/// The `uploadChatFile` outcome — a stored/linked file, or an unresolved
/// project-scope duplicate.
pub enum ChatUploadOutcome {
    Uploaded(ChatUploadedFile),
    Duplicate(ChatUploadDuplicate),
}

/// Errors the chat-file route maps: a 10 MB overflow → v4's message-sniffed 400.
pub enum ChatUploadError {
    SizeExceeded,
    Db(crate::db::DbError),
}

impl From<crate::db::DbError> for ChatUploadError {
    fn from(e: crate::db::DbError) -> Self {
        ChatUploadError::Db(e)
    }
}

/// v4 `uploadChatFile` (`chat-files-v2.ts:123`). `project_id` is the chat's
/// projectId (the route passes `chat.projectId`). Runs the whole read+write on
/// the single writer (byte writes go through the `file_storage.rs` seams). The
/// fire-and-forget `autoDescribeChatImageAttachment` (v4:395) is a named no-op —
/// behaviorally invisible to the route's synchronous contract (enumerated in the
/// lane report).
#[allow(clippy::too_many_arguments)]
pub async fn upload_chat_file(
    db: &Db,
    codec: std::sync::Arc<dyn PixelCodec>,
    user_id: &str,
    chat_id: &str,
    project_id: Option<String>,
    filename: String,
    content_type: String,
    data: Vec<u8>,
    resolution: Option<String>,
    conflicting_file_id: Option<String>,
) -> Result<ChatUploadOutcome, ChatUploadError> {
    // validateChatFile — size only. v4 measures the INPUT file (`file.size` on the
    // uploaded `File`), before any transcode — so this stays on `data`.
    if data.len() > MAX_CHAT_FILE_SIZE {
        return Err(ChatUploadError::SizeExceeded);
    }
    let user_id = user_id.to_string();
    let chat_id = chat_id.to_string();
    db.write(move |ws| {
        let main = ws.main().connection();
        let mount = ws
            .mount_index()
            .ok_or_else(|| {
                crate::db::DbError::Internal("chat upload requires the mount-index database".into())
            })?
            .connection();
        upload_chat_file_conn(
            main,
            mount,
            codec.as_ref(),
            &user_id,
            &chat_id,
            project_id.as_deref().filter(|s| !s.is_empty()),
            &filename,
            &content_type,
            &data,
            resolution.as_deref(),
            conflicting_file_id.as_deref(),
        )
    })
    .await
    .map_err(ChatUploadError::Db)
}

/// The conn-level body of [`upload_chat_file`] (v4 `uploadChatFile` +
/// `uploadFileToProject`).
#[allow(clippy::too_many_arguments)]
fn upload_chat_file_conn(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    codec: &dyn PixelCodec,
    user_id: &str,
    chat_id: &str,
    project_id: Option<&str>,
    filename: &str,
    content_type: &str,
    data: &[u8],
    resolution: Option<&str>,
    conflicting_file_id: Option<&str>,
) -> Result<ChatUploadOutcome, crate::db::DbError> {
    // Detect text content and infer a better MIME type if needed. This reads the
    // *input* bytes, before any transcode, which is the only thing it can mean
    // (v4 `:138-140`).
    let text_detection = detect_text_content(data, filename, content_type);
    let input_mime_type = get_best_mime_type(&text_detection, content_type);

    // Bug 117 (v4 `0b0617fee`): run the transcode the storage bridge is about to
    // run, HERE, before anything is hashed — the same shape the generated-image
    // path (`image_job_storage.rs`) has always had, and the reason all 2541 of
    // v4's generated rows joined cleanly while 118 of 239 chat uploads did not.
    //
    // `transcode_to_webp` is the bridge's own function (`store_mount_blob` calls
    // it with these exact arguments) and is a no-op for anything already WebP or
    // not an image, so handing it the result a second time changes nothing: the
    // bytes hashed here are the bytes that land on disk. That makes one hash
    // serve both jobs — dedup against other uploads, *and* the join to
    // `doc_mount_files.sha256` that carries a description into the search index
    // (`photos::auto_describe_attachment`) and lets `describe_image` /
    // `attach_image` (`tools::photo`) resolve a mount link back to its FileEntry.
    //
    // The residual is v4's own, and it lands differently here: WebP encoding has
    // to be deterministic for dedup to keep matching two uploads of the same
    // source file. In v4 that is sharp, stable for a given version. In v5 the
    // encoder is the [`PixelCodec`] host seam, so the bargain is the same one a
    // sharp version bump makes — a changed encoder costs a missed duplicate (a
    // second row, nothing worse).
    //
    // P4.73: the codec this site is handed is now the HOST codec at every
    // production call (`api::engine`'s `ChatFileUpload` arm), so v5 transcodes
    // chat uploads exactly as v4's sharp does — the convergence P4.D152 named
    // as its follow-up candidate, and the close of the divergence recorded at
    // `api/files.rs:1116-1118` (measured live in the 2026-09-03 dogfood walk:
    // a PNG came back `image/png`). A locked or codec-less engine still falls
    // back to `NotConfiguredPixelCodec`, whose failed encode passes the
    // ORIGINAL bytes through — which is v4's own sharp-unavailable branch.
    //
    // The parameter stays injectable for two reasons: the differential drives
    // it with a byte-CHANGING codec (where bug 117's pre-fix ORDER is
    // measurably wrong and this one measurably right), and the WIRING — which
    // codec the dispatch arm reaches for, something no differential can see —
    // is pinned by `quilltap-harness/tests/chat_upload_codec_wiring.rs`.
    let stored = transcode_to_webp(codec, data, &input_mime_type, TRANSCODE_WEBP_QUALITY);
    let buffer = stored.data;
    let mime_type = stored.stored_mime_type;
    let sha256 = stored.sha256;

    let category = if mime_type.starts_with("image/") {
        "IMAGE"
    } else {
        "ATTACHMENT"
    };
    // linkedTo = [chatId] (the route passes no messageId).
    let linked_to: Vec<String> = vec![chat_id.to_string()];
    let files = FilesRepository::new(main);

    if let Some(pid) = project_id {
        // Content duplicate: same sha in the SAME project.
        let by_hash = files.find_by_sha256_full(&sha256)?;
        let content_dup = by_hash
            .iter()
            .find(|f| f.project_id.as_deref() == Some(pid));
        // Filename duplicate in the project.
        let by_name = files.find_by_filename_in_project(user_id, pid, filename)?;
        let filename_dup = by_name.first();

        let has_content = content_dup.is_some();
        let has_name = filename_dup.is_some();

        if (has_content || has_name) && resolution.is_none() {
            let conflict_type = if has_content && has_name {
                "both"
            } else if has_content {
                "content"
            } else {
                "filename"
            };
            // v4 prefers the filename dup for the existingFile echo.
            let existing = filename_dup.or(content_dup).expect("a duplicate exists");
            return Ok(ChatUploadOutcome::Duplicate(ChatUploadDuplicate {
                conflict_type: conflict_type.to_string(),
                existing_id: existing.id.clone(),
                existing_filename: existing.original_filename.clone(),
                existing_size: existing.size,
                existing_created_at: existing.created_at.clone(),
                existing_sha256: existing.sha256.clone(),
                new_filename: filename.to_string(),
                // v4 `newFile.size: buffer.length` — `buffer` is the transcoded
                // buffer after bug 117, so both echoed values describe the bytes
                // that WOULD be stored, not the ones that arrived.
                new_size: stored.size_bytes as i64,
                new_sha256: sha256.clone(),
            }));
        }

        if let Some(res) = resolution {
            if res == "skip" {
                let existing: Option<FileFull> = match conflicting_file_id {
                    Some(cfid) => files.find_full_by_id(cfid)?,
                    None => filename_dup.or(content_dup).cloned(),
                };
                if let Some(ef) = existing {
                    // Link the existing file to the chat (addLink only touches
                    // linkedTo/updatedAt — the echoed fields are unchanged).
                    for entity_id in &linked_to {
                        files.add_link(&ef.id, entity_id)?;
                    }
                    return Ok(ChatUploadOutcome::Uploaded(uploaded_from_full(&ef)));
                }
                // skip with no existing file falls through to uploadFileToProject.
            }
            if res == "replace" {
                if let Some(cfid) = conflicting_file_id {
                    if files.find_full_by_id(cfid)?.is_some() {
                        // deleteFile (storage) is best-effort / DB-invisible — skip.
                        files.delete(cfid)?;
                    }
                }
            }
            let final_filename = if res == "keepBoth" {
                let project_files = files.find_by_project_id(user_id, pid)?;
                let existing_names: std::collections::HashSet<String> = project_files
                    .iter()
                    .map(|f| f.original_filename.clone())
                    .collect();
                generate_unique_filename(filename, &existing_names)
            } else {
                filename.to_string()
            };
            return upload_file_to_project(
                main,
                mount,
                codec,
                &buffer,
                &final_filename,
                &mime_type,
                &sha256,
                category,
                user_id,
                Some(pid),
                &linked_to,
                text_detection.is_plain_text,
            )
            .map(ChatUploadOutcome::Uploaded);
        }
    }

    // Non-project path (also reached by project files with no conflict/resolution):
    // global sha dedup extends linkedTo, else a fresh upload.
    let existing_files = files.find_by_sha256_full(&sha256)?;
    if let Some(existing) = existing_files.first() {
        let mut merged = existing.linked_to.clone();
        let mut added = false;
        for e in &linked_to {
            if !merged.contains(e) {
                merged.push(e.clone());
                added = true;
            }
        }
        if added {
            files.update(
                &existing.id,
                &crate::db::files::FileUpdate {
                    linked_to: Some(merged),
                    updated_at: crate::clock::now_iso(),
                    ..Default::default()
                },
            )?;
        }
        return Ok(ChatUploadOutcome::Uploaded(uploaded_from_full(existing)));
    }

    upload_file_to_project(
        main,
        mount,
        codec,
        &buffer,
        filename,
        &mime_type,
        &sha256,
        category,
        user_id,
        project_id,
        &linked_to,
        text_detection.is_plain_text,
    )
    .map(ChatUploadOutcome::Uploaded)
}

/// v4 `uploadFileToProject` (`chat-files-v2.ts:343`) — mint the fileId, write the
/// bytes (project store `/` vs the Quilltap Uploads mount under `chat/`), inherit
/// tags, create the metadata row (id == storage path), return the result. The
/// fire-and-forget image auto-describe is a named no-op (deferred).
///
/// `data` / `mime_type` / `sha256` all describe the *stored* bytes — the caller
/// has already run the bridge's transcode (bug 117). The hash is passed in only
/// so a bridge that disagrees can be caught below; the bridge's own answer is
/// what reaches the row.
#[allow(clippy::too_many_arguments)]
fn upload_file_to_project(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    codec: &dyn PixelCodec,
    data: &[u8],
    filename: &str,
    mime_type: &str,
    sha256: &str,
    category: &str,
    user_id: &str,
    project_id: Option<&str>,
    linked_to: &[String],
    is_plain_text: bool,
) -> Result<ChatUploadedFile, crate::db::DbError> {
    let file_id = uuid::Uuid::new_v4().to_string();
    // The bridges may transcode bitmap uploads to WebP; the FileEntry must record
    // the stored mimeType/size/sha256 — all three — not the input. mimeType and
    // size were always taken from here; sha256 was not, and that one omission is
    // bug 117: `files.sha256` named bytes that were never stored, so every join
    // to `doc_mount_files.sha256` was between two different languages and
    // returned an empty result nobody logged.
    let (storage_key, stored_mime, stored_size, stored_sha256, file_folder_path, file_project_id) =
        if let Some(pid) = project_id {
            let uploaded = write_project_file_to_mount_store(
                mount,
                codec,
                pid,
                filename,
                data,
                mime_type,
                Some("/"),
                None,
            )?;
            (
                uploaded.storage_key(),
                uploaded.stored_mime_type,
                uploaded.size_bytes as f64,
                uploaded.sha256,
                Some("/".to_string()),
                Some(pid.to_string()),
            )
        } else {
            let written = write_user_upload_to_mount_store(
                main, mount, codec, filename, data, mime_type, "chat", None,
            )?;
            (
                written.storage_key(),
                written.stored_mime_type,
                written.size_bytes as f64,
                written.sha256,
                None,
                None,
            )
        };

    let stored_sha256 = resolve_stored_sha256(sha256, stored_sha256, filename, &stored_mime);

    let inherited_tags = get_inherited_tags(main, mount, linked_to, user_id);
    let files = FilesRepository::new(main);
    let now = crate::clock::now_iso();
    files.create(
        &FileCreate {
            user_id: user_id.to_string(),
            sha256: stored_sha256.clone(),
            original_filename: filename.to_string(),
            mime_type: stored_mime.clone(),
            size: stored_size,
            width: None,
            height: None,
            is_plain_text: Some(is_plain_text),
            linked_to: linked_to.to_vec(),
            source: "UPLOADED".to_string(),
            category: category.to_string(),
            generation_prompt: None,
            generation_model: None,
            generation_revised_prompt: None,
            description: None,
            tags: inherited_tags,
            project_id: file_project_id,
            folder_path: file_folder_path,
            storage_key: Some(storage_key),
            file_status: "ok".to_string(),
        },
        &CreateOptions {
            id: file_id.clone(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;

    // autoDescribeChatImageAttachment (v4:395) — fire-and-forget host seam, a
    // named no-op here (invisible to the route's synchronous contract).

    Ok(ChatUploadedFile {
        id: file_id.clone(),
        filename: filename.to_string(),
        filepath: get_file_api_path(&file_id),
        mime_type: stored_mime,
        size: stored_size as i64,
        width: None,
        height: None,
    })
}

/// Build the uploaded-file echo from an existing row (skip/dedup paths). v4
/// returns `width || undefined` — a zero/absent width is dropped.
fn uploaded_from_full(f: &FileFull) -> ChatUploadedFile {
    ChatUploadedFile {
        id: f.id.clone(),
        filename: f.original_filename.clone(),
        filepath: get_file_api_path(&f.id),
        mime_type: f.mime_type.clone(),
        size: f.size,
        width: f.width.filter(|w| *w != 0),
        height: f.height.filter(|h| *h != 0),
    }
}

/// v4's disagree-warn (`chat-files-v2.ts:399-411`) and the rule it enforces:
/// **the bridge wins**.
///
/// The caller pre-ran the bridge's own transcode, so these agree by construction
/// — which is exactly why this is a separate function. No corpus can drive a
/// disagreement through the upload path (both hashes come from the same codec
/// over the same bytes), so the arm would be dead to every differential. Driving
/// it directly is the only honest pin for the branch v4 wrote as a tripwire: if
/// a bridge ever changes its storage policy, the dedup hash computed upstream is
/// stale, and this says so rather than letting the join rot silently a second
/// time (bug 117).
fn resolve_stored_sha256(
    pre_upload_sha256: &str,
    stored_sha256: String,
    filename: &str,
    stored_mime_type: &str,
) -> String {
    if stored_sha256 != pre_upload_sha256 {
        tracing::warn!(
            context = "chat-files-v2",
            filename = %filename,
            pre_upload_sha256 = %pre_upload_sha256,
            stored_sha256 = %stored_sha256,
            stored_mime_type = %stored_mime_type,
            "Stored-bytes hash differs from the pre-upload hash; recording the stored one"
        );
    }
    stored_sha256
}

/// v4 `generateUniqueFilename` (`chat-files-v2.ts:79`) — append ` (1)`, ` (2)`, …
/// before the extension until the name is free.
fn generate_unique_filename(
    filename: &str,
    existing: &std::collections::HashSet<String>,
) -> String {
    if !existing.contains(filename) {
        return filename.to_string();
    }
    let (basename, ext) = match filename.rfind('.') {
        Some(i) if i > 0 => (&filename[..i], &filename[i..]),
        _ => (filename, ""),
    };
    let mut counter = 1u32;
    loop {
        let candidate = format!("{basename} ({counter}){ext}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod chat_upload_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    struct CaptureLayer(Arc<Mutex<Vec<String>>>);
    #[derive(Default)]
    struct V(String);
    impl tracing::field::Visit for V {
        fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
            self.0.push_str(&format!(" {}={v}", f.name()));
        }
        fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
            if f.name() == "message" {
                self.0.push_str(&format!(" {v:?}"));
            } else {
                self.0.push_str(&format!(" {}={v:?}", f.name()));
            }
        }
    }
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(&self, e: &tracing::Event<'_>, _c: tracing_subscriber::layer::Context<'_, S>) {
            let mut v = V::default();
            e.record(&mut v);
            self.0
                .lock()
                .unwrap()
                .push(format!("{}{}", e.metadata().level(), v.0));
        }
    }

    fn capture(pre: &str, stored: &str) -> (String, Vec<String>) {
        let logs = Arc::new(Mutex::new(Vec::new()));
        let sub = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let out = {
            let _g = tracing::subscriber::set_default(sub);
            resolve_stored_sha256(pre, stored.to_string(), "shot.png", "image/webp")
        };
        let l = logs.lock().unwrap().clone();
        (out, l)
    }

    /// The bridge wins, and says so with v4's sentence and its five fields.
    #[test]
    fn a_disagreeing_bridge_wins_and_warns() {
        let (out, logs) = capture("input-hash", "stored-hash");
        assert_eq!(out, "stored-hash");
        assert_eq!(logs.len(), 1, "exactly one warn: {logs:?}");
        let line = &logs[0];
        assert!(line.starts_with("WARN "), "{line}");
        // `tracing` renders a static message through `record_debug` on a
        // `format_args!`, so it lands UNQUOTED — a bare `contains` on the
        // sentence would accept a corrupted one ("… stored one (muted)" contains
        // "… stored one"). Anchoring the trailing ` context=` makes it exact.
        assert!(
            line.contains(
                "Stored-bytes hash differs from the pre-upload hash; recording the stored one context="
            ),
            "{line}"
        );
        for f in [
            "context=chat-files-v2",
            "filename=shot.png",
            "pre_upload_sha256=input-hash",
            "stored_sha256=stored-hash",
            "stored_mime_type=image/webp",
        ] {
            assert!(line.contains(f), "missing {f} in {line}");
        }
    }

    /// The production case: the caller pre-ran the bridge's transcode, so the two
    /// agree and the tripwire stays SILENT. A warn attached to the wrong branch
    /// is exactly what a presence-only assertion cannot catch.
    #[test]
    fn an_agreeing_bridge_is_silent() {
        let (out, logs) = capture("same-hash", "same-hash");
        assert_eq!(out, "same-hash");
        assert!(logs.is_empty(), "expected silence, got {logs:?}");
    }
}
