//! v4 `lib/backup/restore/archive.ts` — extract the archive to a scratch
//! directory, then read every data file back off disk.
//!
//! **Mechanism divergence (pre-approved, recorded).** v4 shells out to
//! `unzip -o <zip> -d <dir>`; v5 links the `zip` crate (the same one the backup
//! writer uses). The extracted TREE is the contract, never the unzip mechanics
//! — the differential proves it by feeding both sides the *same committed
//! archive*.
//!
//! **One threading difference.** v4 mints the extract directory under
//! `os.tmpdir()` unconditionally; v5 takes the scratch root from the caller
//! (ultimately `BackupHost::temp_dir()`), so a host that puts scratch somewhere
//! else is honoured and the tests get an isolated root.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::json_stream::{read_json_array_file, read_json_array_file_optional, read_json_file};
use super::legacy_migrations::{
    dedupe_and_order_slot_types, looks_legacy, ordered_component_ids, upgrade_legacy_equipped_slots,
};
use crate::services::backup::collect::BackupData;

/// v4 `cleanupDir` (`:82`) — `rm -rf` semantics, warn and continue on failure.
pub fn cleanup_dir(dir_path: &Path) {
    if let Err(e) = std::fs::remove_dir_all(dir_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(dir = %dir_path.display(), error = %e, "Failed to clean up temp directory");
        }
    }
}

/// A parsed archive plus the scratch tree it was read from.
///
/// The scratch tree is removed when this value drops — v4's `finally
/// cleanupDir(extractDir)` in both `previewRestore` (`preview.ts:77`) and
/// `restore` (`restore.ts:899`), expressed as ownership so no path can leak it.
pub struct ExtractedBackup {
    /// v4's `data` minus `manifest` (v5's [`BackupData`] carries no manifest
    /// field — see the P4.9G6 §2 contract).
    pub data: BackupData,
    /// v4's `data.manifest`, kept beside the collections.
    pub manifest: Value,
    /// `<extractDir>/<rootFolder>` — v4's `rootPath`, what every subsequent
    /// disk read is relative to.
    pub root_path: PathBuf,
    extract_dir: PathBuf,
}

impl ExtractedBackup {
    /// `backupFormat` off the manifest — the discriminator
    /// [`get_file_from_extracted_backup`] keys the storage layout off.
    pub fn backup_format(&self) -> Option<i64> {
        self.manifest.get("backupFormat").and_then(Value::as_i64)
    }
}

impl Drop for ExtractedBackup {
    fn drop(&mut self) {
        cleanup_dir(&self.extract_dir);
    }
}

/// v4 `extractZipToTemp` (`:99`) — extract, then locate the root.
///
/// The root is the single top-level directory whose name starts with
/// `quilltap-backup-`; failing that, a bare `manifest.json` in the extract dir
/// (the flat-zip shape) with `rootFolder: ''`; failing both, the extract dir is
/// cleaned up and v4's verbatim error is returned.
fn extract_zip_to_temp(zip_path: &Path, temp_root: &Path) -> Result<(PathBuf, String), String> {
    let extract_dir = temp_root.join(format!("quilltap-restore-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

    let extracted = (|| -> Result<(), String> {
        let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        archive.extract(&extract_dir).map_err(|e| e.to_string())
    })();
    if let Err(e) = extracted {
        cleanup_dir(&extract_dir);
        return Err(e);
    }

    let root_entry = std::fs::read_dir(&extract_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("quilltap-backup-"));

    match root_entry {
        Some(name) => Ok((extract_dir, name)),
        None => {
            if extract_dir.join("manifest.json").exists() {
                // Flat structure — the extract dir itself is the root.
                Ok((extract_dir, String::new()))
            } else {
                cleanup_dir(&extract_dir);
                Err(
                    "Invalid backup: no quilltap-backup-* folder or manifest.json found"
                        .to_string(),
                )
            }
        }
    }
}

/// v4 `parseBackupZip` (`:135`).
///
/// The required/optional split is behavior, not taste: `manifest.json` plus
/// `characters` / `chats` / `tags` / `connection-profiles` / `image-profiles` /
/// `embedding-profiles` / `memories` / `files` **throw when absent**; every
/// other collection falls back to `[]`. An archive missing `data/tags.json`
/// fails; one missing `data/folders.json` does not.
///
/// Two legacy folds happen HERE rather than in the orchestrator: the
/// per-character `equippedOutfit` slot upgrade (`:145-183`) and
/// `data/outfit-presets.json` → composite wardrobe items (`:209-242`).
///
/// Any failure cleans the extract dir up first (`:316-320`).
pub fn parse_backup_zip(zip_path: &Path, temp_root: &Path) -> Result<ExtractedBackup, String> {
    let (extract_dir, root_folder) = extract_zip_to_temp(zip_path, temp_root)?;
    let root_path = if root_folder.is_empty() {
        extract_dir.clone()
    } else {
        extract_dir.join(&root_folder)
    };

    match parse_tree(&root_path) {
        Ok((data, manifest)) => Ok(ExtractedBackup {
            data,
            manifest,
            root_path,
            extract_dir,
        }),
        Err(e) => {
            cleanup_dir(&extract_dir);
            Err(e)
        }
    }
}

/// The body of `parseBackupZip`'s `try` block, factored out so the single
/// `catch { cleanupDir; throw }` above has one place to run.
fn parse_tree(root: &Path) -> Result<(BackupData, Value), String> {
    let req = |rel: &str| read_json_array_file(root, rel);
    let opt = |rel: &str| read_json_array_file_optional(root, rel);

    let manifest = read_json_file(root, "manifest.json")?;
    let characters = req("data/characters.json")?;
    let mut chats = req("data/chats.json")?;
    upgrade_chat_equipped_outfits(&mut chats);
    let tags = req("data/tags.json")?;
    let connection_profiles = req("data/connection-profiles.json")?;
    let image_profiles = req("data/image-profiles.json")?;
    let embedding_profiles = req("data/embedding-profiles.json")?;
    let memories = req("data/memories.json")?;
    let files = req("data/files.json")?;

    let prompt_templates = opt("data/prompt-templates.json")?;
    let roleplay_templates = opt("data/roleplay-templates.json")?;
    let provider_models = opt("data/provider-models.json")?;
    let projects = opt("data/projects.json")?;
    let groups = opt("data/groups.json")?;
    let llm_logs = opt("data/llm-logs.json")?;
    let plugin_configs = opt("data/plugin-configs.json")?;
    let chat_settings = opt("data/chat-settings.json")?;
    let folders = opt("data/folders.json")?;
    let mut wardrobe_items = opt("data/wardrobe-items.json")?;
    fold_legacy_outfit_presets(root, &mut wardrobe_items)?;
    let character_plugin_data = opt("data/character-plugin-data.json")?;
    let conversation_annotations = opt("data/conversation-annotations.json")?;

    let chat_documents = opt("data/chat-documents.json")?;
    let instance_settings = opt("data/instance-settings.json")?;
    let embedding_status = opt("data/embedding-status.json")?;
    let conversation_chunks = opt("data/conversation-chunks.json")?;
    let tfidf_vocabularies = opt("data/tfidf-vocabularies.json")?;
    let vector_index_metas = opt("data/vector-index-metas.json")?;
    let vector_entries = opt("data/vector-entries.json")?;
    let doc_mount_points = opt("data/doc-mount-points.json")?;
    let doc_mount_folders = opt("data/doc-mount-folders.json")?;
    let doc_mount_files = opt("data/doc-mount-files.json")?;
    let doc_mount_file_links = opt("data/doc-mount-file-links.json")?;
    let doc_mount_chunks = opt("data/doc-mount-chunks.json")?;
    let doc_mount_documents = opt("data/doc-mount-documents.json")?;
    let doc_mount_blobs = opt("data/doc-mount-blobs.json")?;
    let project_doc_mount_links = opt("data/project-doc-mount-links.json")?;
    let group_doc_mount_links = opt("data/group-doc-mount-links.json")?;
    let group_character_members = opt("data/group-character-members.json")?;
    let text_replacement_rules = opt("data/text-replacement-rules.json")?;

    Ok((
        BackupData {
            characters,
            chats,
            tags,
            connection_profiles,
            image_profiles,
            embedding_profiles,
            memories,
            files,
            prompt_templates,
            roleplay_templates,
            provider_models,
            projects,
            groups,
            llm_logs,
            plugin_configs,
            chat_settings,
            folders,
            wardrobe_items,
            character_plugin_data,
            conversation_annotations,
            chat_documents,
            instance_settings,
            embedding_status,
            conversation_chunks,
            tfidf_vocabularies,
            vector_index_metas,
            vector_entries,
            doc_mount_points,
            doc_mount_folders,
            doc_mount_files,
            doc_mount_file_links,
            doc_mount_chunks,
            doc_mount_documents,
            doc_mount_blobs,
            project_doc_mount_links,
            group_doc_mount_links,
            group_character_members,
            text_replacement_rules,
        },
        manifest,
    ))
}

/// v4 `archive.ts:145-183` — pre-rework backups stored each character's
/// `equippedOutfit` slot values as a single UUID-or-null; upgrade them in place
/// so the restore path consumes one uniform structure.
fn upgrade_chat_equipped_outfits(chats: &mut [Value]) {
    let mut touched = 0usize;
    for chat in chats.iter_mut() {
        let Some(Value::Object(equipped)) = chat.get_mut("equippedOutfit") else {
            continue;
        };
        let legacy_ids: Vec<String> = equipped
            .iter()
            .filter(|(_, slots)| looks_legacy(slots))
            .map(|(id, _)| id.clone())
            .collect();
        let mut mutated = false;
        for id in legacy_ids {
            let slots = equipped.get(&id).cloned().unwrap_or(Value::Null);
            if let Some(upgraded) = upgrade_legacy_equipped_slots(&slots) {
                equipped.insert(id, upgraded);
                mutated = true;
            }
        }
        if mutated {
            touched += 1;
        }
    }
    if touched > 0 {
        tracing::info!(
            chats_touched = touched,
            "Upgraded legacy per-character equippedOutfit slot shape"
        );
    }
}

/// v4 `archive.ts:209-242` — `data/outfit-presets.json` folds into composite
/// wardrobe items. New backups never write that file; only older archives carry
/// it. The preset's own `id`/`createdAt`/`updatedAt` are preserved so any
/// pre-rework reference stays valid.
fn fold_legacy_outfit_presets(root: &Path, wardrobe_items: &mut Vec<Value>) -> Result<(), String> {
    let presets = read_json_array_file_optional(root, "data/outfit-presets.json")?;
    if presets.is_empty() {
        return Ok(());
    }
    tracing::info!(
        legacy_preset_count = presets.len(),
        existing_wardrobe_item_count = wardrobe_items.len(),
        "Folded legacy outfit presets into composite wardrobe items"
    );
    for preset in &presets {
        let slots = preset.get("slots").cloned().unwrap_or(Value::Null);
        let mut m = Map::new();
        m.insert(
            "id".into(),
            preset.get("id").cloned().unwrap_or(Value::Null),
        );
        m.insert(
            "characterId".into(),
            preset.get("characterId").cloned().unwrap_or(Value::Null),
        );
        m.insert(
            "title".into(),
            preset.get("name").cloned().unwrap_or(Value::Null),
        );
        m.insert(
            "description".into(),
            preset.get("description").cloned().unwrap_or(Value::Null),
        );
        m.insert(
            "types".into(),
            Value::Array(
                dedupe_and_order_slot_types(&slots)
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        m.insert(
            "componentItemIds".into(),
            Value::Array(ordered_component_ids(&slots)),
        );
        m.insert("appropriateness".into(), Value::Null);
        m.insert("isDefault".into(), Value::Bool(false));
        // Legacy presets were worn as a whole outfit — preserve replace semantics.
        m.insert("replace".into(), Value::Bool(true));
        m.insert("migratedFromClothingRecordId".into(), Value::Null);
        m.insert("archivedAt".into(), Value::Null);
        m.insert(
            "createdAt".into(),
            preset.get("createdAt").cloned().unwrap_or(Value::Null),
        );
        m.insert(
            "updatedAt".into(),
            preset.get("updatedAt").cloned().unwrap_or(Value::Null),
        );
        wardrobe_items.push(Value::Object(m));
    }
    Ok(())
}

/// v4 `getFileFromExtractedBackup` (`:328`) — a `storageKey` tries
/// `files/<storageKey>` first and **falls through** on a miss to the old
/// `files/<category>/<id>_<originalFilename>`; both missing → warn and `null`
/// (which the orchestrator turns into a `File not found in backup: …` warning,
/// not a failure).
///
/// ## ⚠ RULED DIVERGENCE (2026-07-25) — `>= 2`, not `=== 2`
///
/// v4 gates the storage-key lookup on `backupFormat === 2` (`:334`). The
/// staging writer has been at **format 4** for two format revisions, and it
/// stages user files at `files/<storageKey>` — so on every modern archive v4
/// skips the only path the bytes are actually at, misses the legacy path, and
/// restores **not one user file**, reporting each as `File not found in
/// backup`. The bytes are in the archive the whole time; only the reader's gate
/// is wrong.
///
/// Reproducing that faithfully would mean "restore nothing", so the ruling
/// (`status-log.md` → "Ruling — the two v4 restore bugs") is that v5 diverges:
/// **`backup_format >= 2`**. Every format v4 has ever written a `storageKey`
/// for is covered, format 1 still takes the legacy path alone, and the
/// fall-through is unchanged — so this reads a strict superset of what v4 reads
/// and can never read *less*. Reader-side only: the backup writer is untouched
/// and v5's archives stay byte-identical to v4's.
///
/// `system_restore_state` asserts the divergence in BOTH directions (v4 must
/// restore zero files, v5 must restore them) so it can never drift into an
/// unnoticed difference.
pub fn get_file_from_extracted_backup(
    root_path: &Path,
    file: &Value,
    backup_format: Option<i64>,
) -> Option<Vec<u8>> {
    let s = |k: &str| file.get(k).and_then(Value::as_str).unwrap_or("");
    if backup_format.is_some_and(|f| f >= 2) {
        let key = s("storageKey");
        if !key.is_empty() {
            let mut p = root_path.join("files");
            for part in key.split('/') {
                p.push(part);
            }
            if let Ok(bytes) = std::fs::read(&p) {
                return Some(bytes);
            }
            // Fall through to the old format as fallback.
        }
    }
    let old = root_path.join("files").join(s("category")).join(format!(
        "{}_{}",
        s("id"),
        s("originalFilename")
    ));
    match std::fs::read(&old) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            tracing::warn!(file_id = s("id"), "File not found in extracted backup");
            None
        }
    }
}

/// v4 `countNpmPluginsInExtractedBackup` (`:363`) — directories under
/// `plugins/npm`, `0` when the path is absent.
pub fn count_npm_plugins_in_extracted_backup(root_path: &Path) -> usize {
    let plugins = root_path.join("plugins").join("npm");
    match std::fs::read_dir(&plugins) {
        Ok(entries) => entries.flatten().filter(|e| e.path().is_dir()).count(),
        Err(_) => 0,
    }
}
