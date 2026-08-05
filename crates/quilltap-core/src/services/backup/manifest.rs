//! v4 `createManifest` (`lib/backup/backup-service.ts:432`) — the entity-count
//! header written last into the staging root.
//!
//! `appVersion` is a parameter rather than a constant: v4 reads
//! `process.env.npm_package_version` and falls back to `'2.0.0'`
//! (`backup-service.ts:44`), which makes it an environment property, not a data
//! property. The host passes v5's own version; the differential pins both sides
//! to the same string.

use serde_json::{Map, Value};

use super::collect::BackupData;

/// The counts the manifest carries that come from the host filesystem rather
/// than the database (v4 `countNpmPlugins` :392 / `countUserInstalledThemes` :412).
#[derive(Debug, Clone, Copy, Default)]
pub struct HostCounts {
    pub npm_plugins: i64,
    pub user_installed_themes: i64,
}

/// v4 `createManifest(userId, data)`.
pub fn create_manifest(
    user_id: &str,
    data: &BackupData,
    app_version: &str,
    created_at: &str,
    host: HostCounts,
    // v4 `7189a968`: `...(options?.compact && { compact: true })` — the key is
    // OMITTED when false, so a full backup's manifest is byte-identical to a
    // pre-drift one.
    compact: bool,
) -> Value {
    fn n(counts: &mut Map<String, Value>, key: &str, len: usize) {
        counts.insert(key.into(), Value::from(len as i64));
    }
    let mut counts = Map::new();
    let c = &mut counts;
    n(c, "characters", data.characters.len());
    n(c, "chats", data.chats.len());
    n(c, "messages", data.total_messages());
    n(c, "tags", data.tags.len());
    n(c, "connectionProfiles", data.connection_profiles.len());
    n(c, "imageProfiles", data.image_profiles.len());
    n(c, "embeddingProfiles", data.embedding_profiles.len());
    n(c, "memories", data.memories.len());
    n(c, "files", data.files.len());
    n(c, "promptTemplates", data.prompt_templates.len());
    n(c, "roleplayTemplates", data.roleplay_templates.len());
    n(c, "providerModels", data.provider_models.len());
    n(c, "projects", data.projects.len());
    n(c, "groups", data.groups.len());
    n(c, "llmLogs", data.llm_logs.len());
    n(c, "pluginConfigs", data.plugin_configs.len());
    n(c, "chatSettings", data.chat_settings.len());
    n(c, "folders", data.folders.len());
    n(c, "wardrobeItems", data.wardrobe_items.len());
    c.insert("npmPlugins".into(), Value::from(host.npm_plugins));
    n(c, "characterPluginData", data.character_plugin_data.len());
    n(
        c,
        "conversationAnnotations",
        data.conversation_annotations.len(),
    );
    c.insert(
        "userInstalledThemes".into(),
        Value::from(host.user_installed_themes),
    );
    n(c, "chatDocuments", data.chat_documents.len());
    n(c, "instanceSettings", data.instance_settings.len());
    n(c, "embeddingStatus", data.embedding_status.len());
    n(c, "conversationChunks", data.conversation_chunks.len());
    n(c, "tfidfVocabularies", data.tfidf_vocabularies.len());
    n(c, "vectorIndexMetas", data.vector_index_metas.len());
    n(c, "vectorEntries", data.vector_entries.len());
    n(c, "docMountPoints", data.doc_mount_points.len());
    n(c, "docMountFolders", data.doc_mount_folders.len());
    n(c, "docMountFiles", data.doc_mount_files.len());
    n(c, "docMountFileLinks", data.doc_mount_file_links.len());
    n(c, "docMountChunks", data.doc_mount_chunks.len());
    n(c, "docMountDocuments", data.doc_mount_documents.len());
    n(c, "docMountBlobs", data.doc_mount_blobs.len());
    n(
        c,
        "projectDocMountLinks",
        data.project_doc_mount_links.len(),
    );
    n(c, "groupDocMountLinks", data.group_doc_mount_links.len());
    n(
        c,
        "groupCharacterMembers",
        data.group_character_members.len(),
    );
    n(c, "textReplacementRules", data.text_replacement_rules.len());

    let mut m = Map::new();
    m.insert("version".into(), Value::String("1.0".into()));
    m.insert("backupFormat".into(), Value::from(4));
    m.insert("createdAt".into(), Value::String(created_at.into()));
    m.insert("userId".into(), Value::String(user_id.into()));
    m.insert("appVersion".into(), Value::String(app_version.into()));
    if compact {
        m.insert("compact".into(), Value::Bool(true));
    }
    m.insert("counts".into(), Value::Object(counts));
    Value::Object(m)
}
