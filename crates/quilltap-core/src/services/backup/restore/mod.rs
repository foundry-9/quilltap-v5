//! The restore half of the backup family (P4.9G5 units 3–5) — v4
//! `lib/backup/restore/**` plus `lib/backup/restore-service.ts`'s barrel.
//!
//! - [`json_stream`] — the disk-backed JSON readers (v4 `json-stream.ts`).
//! - [`legacy_migrations`] — the pure pre-rework folds (v4 `legacy-migrations.ts`).
//! - [`archive`] — extract + `parseBackupZip` + the extracted-tree readers.
//! - [`preview`] — `previewRestore`, the write-free count pass.
//!
//! v4's `delete-service.ts` is NOT here: it landed as
//! [`crate::services::delete_all`] in P4.9G3 and the orchestrator calls it.
//!
//! The orchestrator itself (v4 `restore/restore.ts`) is unit 4; until it lands
//! `systemRestoreExecute` answers a loud typed refusal naming this module.

pub mod archive;
pub mod json_stream;
pub mod legacy_migrations;
pub mod preview;

use serde::Serialize;

pub use archive::{cleanup_dir, parse_backup_zip, ExtractedBackup};
pub use preview::preview_restore;

/// v4 `RestoreSummary` (`lib/backup/types.ts`) — the **41-key** shape both
/// `previewRestore` (`preview.ts:29-74`) and `restore` (`restore.ts:840-887`)
/// answer with. Field ORDER here is v4's, and it is what reaches the wire.
///
/// The two functions fill it differently and deliberately: preview reports the
/// archive's INPUT lengths throughout, while restore reports the actually-
/// restored counters for everything EXCEPT `characters`/`chats`/`tags`/
/// `memories` and the three `profiles` (which stay `data.*.length` even when a
/// row failed). That mix is v4's; it is reproduced field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub characters: usize,
    pub chats: usize,
    pub messages: usize,
    pub tags: usize,
    pub files: usize,
    pub memories: usize,
    pub profiles: ProfileCounts,
    pub templates: TemplateCounts,
    pub provider_models: usize,
    pub projects: usize,
    pub groups: usize,
    pub llm_logs: usize,
    pub plugin_configs: usize,
    pub chat_settings: usize,
    pub folders: usize,
    pub wardrobe_items: usize,
    pub npm_plugins: usize,
    pub character_plugin_data: usize,
    pub conversation_annotations: usize,
    pub user_installed_themes: usize,
    pub chat_documents: usize,
    pub instance_settings: usize,
    pub embedding_status: usize,
    pub conversation_chunks: usize,
    pub tfidf_vocabularies: usize,
    pub vector_index_metas: usize,
    pub vector_entries: usize,
    pub doc_mount_points: usize,
    pub doc_mount_folders: usize,
    pub doc_mount_files: usize,
    pub doc_mount_file_links: usize,
    pub doc_mount_chunks: usize,
    pub doc_mount_documents: usize,
    pub doc_mount_blobs: usize,
    pub project_doc_mount_links: usize,
    pub group_doc_mount_links: usize,
    pub group_character_members: usize,
    pub text_replacement_rules: usize,
    pub warnings: Vec<String>,
}

/// v4's nested `profiles` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileCounts {
    pub connection: usize,
    pub image: usize,
    pub embedding: usize,
}

/// v4's nested `templates` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemplateCounts {
    pub prompt: usize,
    pub roleplay: usize,
}
