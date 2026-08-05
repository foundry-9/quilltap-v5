//! The backup staging tree — v4 `createBackup`'s disk projection
//! (`lib/backup/backup-service.ts:584-734`), everything up to (but not
//! including) the zip.
//!
//! ## The two JSON writers, byte-for-byte
//!
//! - [`write_json_array_file`] is v4 `writeJsonArrayFile` (`:505`): the array is
//!   streamed one element at a time so a multi-hundred-MB `llm-logs.json` never
//!   hits V8's ~512 MB max-string limit. The layout that streaming produces is
//!   part of the archive contract: an empty array is the two bytes `[]` plus a
//!   newline; otherwise `[\n`, each element pretty-printed at indent 2 and then
//!   shifted right by two more spaces, `,\n` between elements, `\n]` and a final
//!   newline.
//! - [`write_json_file`] is v4 `writeJsonFile` (`:492`) for the manifest:
//!   `JSON.stringify(data, null, 2)` with **no** trailing newline.
//!
//! ## Warn-and-continue
//!
//! v4 never fails a backup because one user file or one blob could not be read
//! (`:647`, `:673`): it logs and moves on, so a partially-damaged instance still
//! produces a usable archive. [`StageReport`] carries the same outcome as data
//! so the caller can log/count it.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::collect::{read_doc_mount_blob_bytes, BackupData};
use crate::db::runtime::Db;
use crate::services::file_storage::{is_mount_blob_storage_key, StorageBackend};

/// What the staging pass actually managed to write (v4 logs these; the caller
/// may too). Never an error: staging failures are warn-and-continue by design.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageReport {
    pub files_staged: usize,
    pub files_skipped: usize,
    pub blobs_staged: usize,
    pub blobs_skipped: usize,
    /// The `originalFilename` of every user file whose bytes could not be
    /// staged, in collection order (dogfood #59). v4 has no analogue — it warns
    /// to its module logger and forgets. The names reach the operator through
    /// the backup response, because this is the last moment at which the loss is
    /// still recoverable.
    pub skipped_files: Vec<String>,
}

/// The host directories v4 copies wholesale into the archive. `None` for a host
/// with no such directory — v4's `fs.existsSync` guard (`:688`, `:709`).
#[derive(Debug, Clone, Default)]
pub struct HostDirs {
    /// `<base>/plugins/npm` — each subdirectory is one npm plugin.
    pub npm_plugins: Option<PathBuf>,
    /// `<base>/themes` — each subdirectory is a theme bundle (`.cache` excluded),
    /// plus `themes-index.json` when present.
    pub themes: Option<PathBuf>,
}

/// v4's data-file order and filenames (`:591-632`). `outfit-presets.json` is no
/// longer written — pre-rework presets fold into composite wardrobe items at
/// restore time instead (`:609`).
fn data_files(data: &BackupData, compact: bool) -> Vec<(&'static str, &Vec<Value>)> {
    // v4 `7189a968`: a compact backup OMITS the six derived embedding data
    // files outright rather than writing empty arrays — the restore reader
    // treats all of them as optional, so an absent file and an empty one
    // behave identically on the way back in, and the absent one is what makes
    // the archive small.
    let omit = |name: &'static str| {
        compact
            && matches!(
                name,
                "embedding-status.json"
                    | "conversation-chunks.json"
                    | "tfidf-vocabularies.json"
                    | "vector-index-metas.json"
                    | "vector-entries.json"
                    | "doc-mount-chunks.json"
            )
    };
    let files: Vec<(&'static str, &Vec<Value>)> = vec![
        ("characters.json", &data.characters),
        ("chats.json", &data.chats),
        ("tags.json", &data.tags),
        ("connection-profiles.json", &data.connection_profiles),
        ("image-profiles.json", &data.image_profiles),
        ("embedding-profiles.json", &data.embedding_profiles),
        ("memories.json", &data.memories),
        ("files.json", &data.files),
        ("prompt-templates.json", &data.prompt_templates),
        ("roleplay-templates.json", &data.roleplay_templates),
        ("provider-models.json", &data.provider_models),
        ("projects.json", &data.projects),
        ("groups.json", &data.groups),
        ("llm-logs.json", &data.llm_logs),
        ("plugin-configs.json", &data.plugin_configs),
        ("chat-settings.json", &data.chat_settings),
        ("folders.json", &data.folders),
        ("wardrobe-items.json", &data.wardrobe_items),
        ("character-plugin-data.json", &data.character_plugin_data),
        (
            "conversation-annotations.json",
            &data.conversation_annotations,
        ),
        // Format-3 additions (older restorers simply skip these missing files).
        ("chat-documents.json", &data.chat_documents),
        ("instance-settings.json", &data.instance_settings),
        ("embedding-status.json", &data.embedding_status),
        ("conversation-chunks.json", &data.conversation_chunks),
        ("tfidf-vocabularies.json", &data.tfidf_vocabularies),
        ("vector-index-metas.json", &data.vector_index_metas),
        ("vector-entries.json", &data.vector_entries),
        ("doc-mount-points.json", &data.doc_mount_points),
        ("doc-mount-folders.json", &data.doc_mount_folders),
        ("doc-mount-files.json", &data.doc_mount_files),
        ("doc-mount-file-links.json", &data.doc_mount_file_links),
        ("doc-mount-chunks.json", &data.doc_mount_chunks),
        ("doc-mount-documents.json", &data.doc_mount_documents),
        ("doc-mount-blobs.json", &data.doc_mount_blobs),
        (
            "project-doc-mount-links.json",
            &data.project_doc_mount_links,
        ),
        ("group-doc-mount-links.json", &data.group_doc_mount_links),
        (
            "group-character-members.json",
            &data.group_character_members,
        ),
        ("text-replacement-rules.json", &data.text_replacement_rules),
    ];
    files.into_iter().filter(|(name, _)| !omit(name)).collect()
}

/// v4 `writeJsonArrayFile` (`:505`).
pub fn write_json_array_file(path: &Path, items: &[Value]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    if items.is_empty() {
        out.push_str("[]\n");
    } else {
        out.push_str("[\n");
        for (i, item) in items.iter().enumerate() {
            let json = serde_json::to_string_pretty(item).expect("serializable");
            for line in json.split('\n') {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            // The newline just written belongs to the element's last line; v4
            // emits `indented + ',\n'` between elements, so back up and insert
            // the comma before it.
            if i != items.len() - 1 {
                out.pop();
                out.push_str(",\n");
            }
        }
        out.push_str("]\n");
    }
    std::fs::write(path, out)
}

/// v4 `writeJsonFile` (`:492`) — small objects only (the manifest).
pub fn write_json_file(path: &Path, data: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(data).expect("serializable"),
    )
}

/// Stage the whole archive tree under `staging_dir` (v4 `:584-734`), in v4's
/// order: the 38 data files, then user files, then doc-mount blob bytes, then
/// npm plugins, then theme bundles, and the manifest LAST.
pub fn stage_backup(
    db: &Db,
    backend: &dyn StorageBackend,
    data: &BackupData,
    manifest: &Value,
    staging_dir: &Path,
    host_dirs: &HostDirs,
    compact: bool,
) -> std::io::Result<StageReport> {
    std::fs::create_dir_all(staging_dir.join("data"))?;
    for (name, items) in data_files(data, compact) {
        write_json_array_file(&staging_dir.join("data").join(name), items)?;
    }

    let mut report = StageReport::default();

    // User files, one at a time (`:638`). `storageKey`-less rows are skipped
    // silently — v4's `if (file.storageKey)` guard, not an error.
    //
    // ## ⚠ dogfood #59 — a skip here is where the operator's files go missing
    //
    // v4 `moduleLogger.warn('Failed to download file for backup, skipping', …)`
    // and continues, so one unreadable file cannot sink a backup. v5 counted the
    // skip and said NOTHING AT ALL — not even to the log — which is how the
    // 2026-08-03 walk learned about it only at RESTORE time, months later, as 19
    // × `File not found in backup: <name>`. Measured on that instance: 17 of the
    // 2,085 `files` rows carry a legacy disk key (`<projectId>/<name>`) whose
    // bytes are no longer under `<base>/files` — the document-store cutover moved
    // them and left the rows behind. The rows are broken, not the backup, but the
    // backup is where the operator can still do something about it.
    //
    // So: parity with v4's logger below, and — the actual repair — the names ride
    // out on [`StageReport::skipped_files`] and reach the operator in the backup
    // response. See `api::system_backup::backup_create`.
    for file in &data.files {
        let Some(key) = file.get("storageKey").and_then(Value::as_str) else {
            continue;
        };
        let name = file
            .get("originalFilename")
            .and_then(Value::as_str)
            .unwrap_or(key);
        match download_by_key(db, backend, key) {
            Ok(bytes) => {
                let dest = join_relative(&staging_dir.join("files"), key);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                match std::fs::write(&dest, bytes) {
                    Ok(()) => report.files_staged += 1,
                    Err(e) => {
                        tracing::warn!(
                            target: "quilltap::backup",
                            storage_key = key,
                            error = %e,
                            "Failed to stage file for backup, skipping",
                        );
                        report.files_skipped += 1;
                        report.skipped_files.push(name.to_string());
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::backup",
                    storage_key = key,
                    error = %e,
                    "Failed to download file for backup, skipping",
                );
                report.files_skipped += 1;
                report.skipped_files.push(name.to_string());
            }
        }
    }

    // Doc-mount blob bytes, keyed by blob id (`:665`). v4 only creates the
    // directory when there is at least one blob.
    if !data.doc_mount_blobs.is_empty() {
        let blobs_dir = staging_dir.join("mount-blobs");
        std::fs::create_dir_all(&blobs_dir)?;
        for blob in &data.doc_mount_blobs {
            let Some(id) = blob.get("id").and_then(Value::as_str) else {
                report.blobs_skipped += 1;
                continue;
            };
            let bytes = db
                .read_mount_index(|c| Ok(read_doc_mount_blob_bytes(c, id)))
                .ok()
                .flatten();
            match bytes {
                Some(b) => match std::fs::write(blobs_dir.join(id), b) {
                    Ok(()) => report.blobs_staged += 1,
                    Err(e) => {
                        tracing::warn!(
                            target: "quilltap::backup",
                            blob_id = id,
                            error = %e,
                            "Failed to stage doc-store blob for backup, skipping",
                        );
                        report.blobs_skipped += 1;
                    }
                },
                None => {
                    tracing::warn!(
                        target: "quilltap::backup",
                        blob_id = id,
                        "Doc-store blob bytes missing, skipping",
                    );
                    report.blobs_skipped += 1;
                }
            }
        }
    }

    // npm plugins + theme bundles: best-effort recursive copies (`:687`, `:708`).
    if let Some(src) = host_dirs.npm_plugins.as_ref().filter(|p| p.is_dir()) {
        let _ = copy_subdirs(src, &staging_dir.join("plugins").join("npm"), &[]);
    }
    if let Some(src) = host_dirs.themes.as_ref().filter(|p| p.is_dir()) {
        let dest = staging_dir.join("themes");
        let _ = copy_subdirs(src, &dest, &[".cache"]);
        let index = src.join("themes-index.json");
        if index.is_file() {
            let _ = std::fs::create_dir_all(&dest);
            let _ = std::fs::copy(&index, dest.join("themes-index.json"));
        }
    }

    // The manifest is written last, after all data is staged (`:734`).
    write_json_file(&staging_dir.join("manifest.json"), manifest)?;
    Ok(report)
}

/// v4 `countNpmPlugins` (`:392`) / `countUserInstalledThemes` (`:412`) — the two
/// filesystem-derived manifest counts. A missing directory counts 0.
pub fn count_subdirs(dir: Option<&Path>, exclude: &[&str]) -> i64 {
    let Some(dir) = dir else { return 0 };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !exclude.contains(&name.as_str())
        })
        .count() as i64
}

/// v4 `FileStorageManager.downloadFile` reduced to what staging needs: the
/// mount-blob branch reads the `doc_mount_blobs` row, everything else goes to
/// the disk backend.
fn download_by_key(db: &Db, backend: &dyn StorageBackend, key: &str) -> Result<Vec<u8>, String> {
    if is_mount_blob_storage_key(Some(key)) {
        let blob_id = crate::services::file_storage::parse_mount_blob_storage_key(key)
            .map(|(_mp, id)| id)
            .ok_or_else(|| format!("Mount-blob not found for storageKey: {key}"))?;
        return db
            .read_mount_index(|c| Ok(read_doc_mount_blob_bytes(c, &blob_id)))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Mount-blob not found for storageKey: {key}"));
    }
    backend.download(key)
}

/// Join a `/`-separated storage key under `base`, refusing any component that
/// would escape it. v4 relies on `path.join` plus the backend's own traversal
/// guard; making it explicit here keeps a malicious storage key from writing
/// outside the staging tree.
fn join_relative(base: &Path, key: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in key.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        out.push(part);
    }
    out
}

/// Copy each immediate subdirectory of `src` into `dest`, recursively.
fn copy_subdirs(src: &Path, dest: &Path, exclude: &[&str]) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if exclude.contains(&name.as_str()) {
            continue;
        }
        copy_dir_recursive(&entry.path(), &dest.join(&name))?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if from.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_array_file_is_v4s_two_bytes_plus_newline() {
        let dir = std::env::temp_dir().join(format!("qt-stage-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("empty.json");
        write_json_array_file(&p, &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "[]\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn array_file_matches_v4s_streamed_layout() {
        let dir = std::env::temp_dir().join(format!("qt-stage-arr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("two.json");
        write_json_array_file(&p, &[json!({"a": 1}), json!({"b": [2]})]).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "[\n  {\n    \"a\": 1\n  },\n  {\n    \"b\": [\n      2\n    ]\n  }\n]\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn manifest_file_has_no_trailing_newline() {
        let dir = std::env::temp_dir().join(format!("qt-stage-man-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("manifest.json");
        write_json_file(&p, &json!({"version": "1.0"})).unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "{\n  \"version\": \"1.0\"\n}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
