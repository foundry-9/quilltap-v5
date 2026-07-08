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
        .flatten()?;

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
