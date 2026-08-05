//! v4 `lib/backup/restore/preview.ts` — `previewRestore`: extract, count what
//! each entity WOULD restore, clean up. It writes nothing anywhere, and the
//! differential's `restore_preview_writes_nothing` case proves v5's doesn't
//! either.

use std::path::Path;

use super::archive::{count_npm_plugins_in_extracted_backup, parse_backup_zip};
use super::{ProfileCounts, RestoreSummary, TemplateCounts};

/// v4 `previewRestore(zipPath)` (`:20`) — the 41-key [`RestoreSummary`] over
/// the archive's *input* lengths.
///
/// Two of the 41 are constants here, not counts: `userInstalledThemes` is
/// hard-coded `0` ("Counted after zip extraction; not shown in preview",
/// `:55`) and `warnings` is always empty (`:74`).
pub fn preview_restore(zip_path: &Path, temp_root: &Path) -> Result<RestoreSummary, String> {
    let extracted = parse_backup_zip(zip_path, temp_root)?;
    let d = &extracted.data;

    Ok(RestoreSummary {
        characters: d.characters.len(),
        chats: d.chats.len(),
        messages: d.total_messages(),
        tags: d.tags.len(),
        files: d.files.len(),
        memories: d.memories.len(),
        profiles: ProfileCounts {
            connection: d.connection_profiles.len(),
            image: d.image_profiles.len(),
            embedding: d.embedding_profiles.len(),
        },
        templates: TemplateCounts {
            prompt: d.prompt_templates.len(),
            roleplay: d.roleplay_templates.len(),
        },
        provider_models: d.provider_models.len(),
        projects: d.projects.len(),
        groups: d.groups.len(),
        llm_logs: d.llm_logs.len(),
        plugin_configs: d.plugin_configs.len(),
        chat_settings: d.chat_settings.len(),
        folders: d.folders.len(),
        wardrobe_items: d.wardrobe_items.len(),
        npm_plugins: count_npm_plugins_in_extracted_backup(&extracted.root_path),
        character_plugin_data: d.character_plugin_data.len(),
        conversation_annotations: d.conversation_annotations.len(),
        // Counted after zip extraction; not shown in preview (v4 `:55`).
        user_installed_themes: 0,
        chat_documents: d.chat_documents.len(),
        instance_settings: d.instance_settings.len(),
        embedding_status: d.embedding_status.len(),
        conversation_chunks: d.conversation_chunks.len(),
        tfidf_vocabularies: d.tfidf_vocabularies.len(),
        vector_index_metas: d.vector_index_metas.len(),
        vector_entries: d.vector_entries.len(),
        doc_mount_points: d.doc_mount_points.len(),
        doc_mount_folders: d.doc_mount_folders.len(),
        doc_mount_files: d.doc_mount_files.len(),
        doc_mount_file_links: d.doc_mount_file_links.len(),
        doc_mount_chunks: d.doc_mount_chunks.len(),
        doc_mount_documents: d.doc_mount_documents.len(),
        doc_mount_blobs: d.doc_mount_blobs.len(),
        project_doc_mount_links: d.project_doc_mount_links.len(),
        group_doc_mount_links: d.group_doc_mount_links.len(),
        group_character_members: d.group_character_members.len(),
        text_replacement_rules: d.text_replacement_rules.len(),
        embedding_reconcile: None,
        warnings: Vec::new(),
    })
    // `extracted` drops here — v4's `finally cleanupDir(extractDir)` (`:77`).
}
