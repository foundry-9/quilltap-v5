//! The ten per-entity record generators (v4 `ndjson-writer.ts` :113-607). Each
//! mirrors one `async function* stream<Type>` — same read order, same skip
//! conditions, same `bump()` sites, same emitted key order.
//!
//! Where v4 wraps a read in `try { … } catch { logger.warn }` we swallow the same
//! way (an unreadable sidecar must not sink the export); where v4 lets a read
//! throw (the top-level `findById`s) the error propagates as [`ExportError::Db`].

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::key_order::reorder;
use super::{
    kind_data, resolve_api_key_label, resolve_tag_names, sanitize_profile, with_tag_names, Counts,
    ExportError, BLOB_CHUNK_BYTES,
};
use crate::db::chat_documents;
use crate::db::conversation_annotations;
use crate::db::doc_mount_blobs::DocMountBlobsRepository;
use crate::db::doc_mount_documents::{self, DocMountDocumentsRepository};
use crate::db::doc_mount_folders::DocMountFoldersRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::project_doc_mount_links;
use crate::db::{
    character_plugin_data, characters_read, chats_messages_read, chats_read, connection_profiles,
    embedding_profiles, files::FilesRepository, group_character_members, group_doc_mount_links,
    groups, image_profiles, memories_read, projects, roleplay_templates, tags, wardrobe_read,
};

fn get_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn tag_ids(v: &Value) -> Vec<Value> {
    v.get("tags")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

// ============================================================================
// characters (v4 :113)
// ============================================================================

pub(super) fn stream_characters(
    main: &Connection,
    mount: &Connection,
    ids: &[String],
    include_memories: bool,
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    let docs = DocMountDocumentsRepository::new(mount);

    for id in ids {
        // v4 uses the VAULT-AWARE lookup so managed fields land in the export.
        let Some(character) = characters_read::find_by_id(main, mount, id)? else {
            continue;
        };

        let tag_names = resolve_tag_names(main, &tag_ids(&character));
        let mut record = with_tag_names("character", &character, tag_names);
        // v4 destructures `wardrobeItems` / `pluginData` off the record before
        // emitting; the vault-aware read never carries them, but the strip is
        // reproduced so a future read that does can't leak them onto one line.
        record.remove("wardrobeItems");
        record.remove("pluginData");
        out.push(kind_data("character", Value::Object(record)));
        counts.bump("characters");

        // Wardrobe items — one record each (v4 swallows a failure with a warn).
        if let Ok(items) = wardrobe_read::find_by_character_id(main, &docs, id, false) {
            for item in items {
                let mut m = Map::new();
                m.insert("kind".into(), Value::String("wardrobe_item".into()));
                m.insert("characterId".into(), Value::String(id.clone()));
                // NOT reordered: v4 reads wardrobe items straight off the character
                // VAULT (not through `WardrobeItemSchema.parse`), so the emitted key
                // order is the vault document's own.
                m.insert("data".into(), item);
                out.push(Value::Object(m));
            }
        }

        // Plugin data — one record per plugin, in `Object.keys` order.
        if let Ok(Value::Object(map)) = character_plugin_data::get_plugin_data_map(main, id) {
            for (plugin_name, data) in map {
                let mut m = Map::new();
                m.insert("kind".into(), Value::String("character_plugin_data".into()));
                m.insert("characterId".into(), Value::String(id.clone()));
                m.insert("pluginName".into(), Value::String(plugin_name));
                m.insert("data".into(), data);
                out.push(Value::Object(m));
            }
        }

        if include_memories {
            if let Ok(memories) = memories_read::find_by_character_id(main, id) {
                for memory in memories {
                    out.push(kind_data("memory", reorder("memory", memory)));
                    counts.bump("memories");
                }
            }
        }
    }
    Ok(())
}

// ============================================================================
// chats (v4 :197)
// ============================================================================

pub(super) fn stream_chats(
    main: &Connection,
    mount: &Connection,
    ids: &[String],
    include_memories: bool,
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    // v4 loads the character list ONCE up front (only when memories are on).
    let all_characters = if include_memories {
        characters_read::find_all(main, mount)?
    } else {
        Vec::new()
    };

    for id in ids {
        let Some(chat) = chats_read::find_by_id(main, id)? else {
            continue;
        };

        let tag_names = resolve_tag_names(main, &tag_ids(&chat));

        // v4 `_participantInfo`: `{participantId, characterName, type}` per
        // participant. `characterName` is `undefined` (KEY DROPPED by
        // JSON.stringify) unless the participant is a resolvable CHARACTER.
        let participants = chat
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut participant_info: Vec<Value> = Vec::new();
        for p in &participants {
            let p_type = get_str(p, "type");
            let character_id = get_str(p, "characterId");
            let mut character_name: Option<String> = None;
            if p_type.as_deref() == Some("CHARACTER") {
                if let Some(cid) = character_id.as_deref().filter(|s| !s.is_empty()) {
                    if let Some(c) = characters_read::find_by_id(main, mount, cid)? {
                        character_name = get_str(&c, "name");
                    }
                }
            }
            let mut m = Map::new();
            m.insert(
                "participantId".into(),
                p.get("id").cloned().unwrap_or(Value::Null),
            );
            if let Some(n) = character_name {
                m.insert("characterName".into(), Value::String(n));
            }
            m.insert(
                "type".into(),
                p_type.map(Value::String).unwrap_or(Value::Null),
            );
            participant_info.push(Value::Object(m));
        }

        // Ephemeral per-chat UX state that must not ride a portable .qtap file
        // (v4 :233-237) — dropped before the spread.
        let mut record = with_tag_names("chat", &chat, tag_names);
        record.shift_remove("commonplaceRecallHistory");
        record.shift_remove("commonplaceSceneCache");
        // v4's `_tagNames` spread lands BEFORE `_participantInfo`; the removals
        // above happen on the source object, so re-assert the order by rebuilding
        // with `_tagNames` first (already inserted by `with_tag_names`).
        if !participant_info.is_empty() {
            record.insert("_participantInfo".into(), Value::Array(participant_info));
        }
        out.push(kind_data("chat", Value::Object(record)));
        counts.bump("chats");

        // Messages, one record each (only `type === 'message'` events).
        if let Ok(events) = chats_messages_read::get_messages(main, id) {
            for event in events {
                if event.get("type").and_then(Value::as_str) != Some("message") {
                    continue;
                }
                let mut m = Map::new();
                m.insert("kind".into(), Value::String("chat_message".into()));
                m.insert("chatId".into(), Value::String(id.clone()));
                m.insert("data".into(), reorder("chat_message", event));
                out.push(Value::Object(m));
                counts.bump("messages");
            }
        }

        // Annotations + chat documents AFTER the messages so importers can
        // resolve sourceMessageId / chatId against ids already seen.
        if let Ok(annotations) = conversation_annotations::find_full_json_by_chat_id(main, id) {
            for annotation in annotations {
                let mut m = Map::new();
                m.insert(
                    "kind".into(),
                    Value::String("conversation_annotation".into()),
                );
                m.insert("chatId".into(), Value::String(id.clone()));
                m.insert("data".into(), annotation);
                out.push(Value::Object(m));
                counts.bump("conversationAnnotations");
            }
        }

        if let Ok(chat_docs) = chat_documents::find_full_json_by_chat_id(main, id) {
            for cd in chat_docs {
                let mut m = Map::new();
                m.insert("kind".into(), Value::String("chat_document".into()));
                m.insert("chatId".into(), Value::String(id.clone()));
                m.insert("data".into(), cd);
                out.push(Value::Object(m));
                counts.bump("chatDocuments");
            }
        }

        // Memories scoped to this chat — v4 re-reads EVERY character's memories
        // per chat and filters by chatId (O(chats × characters), reproduced).
        if include_memories {
            for character in &all_characters {
                let Some(cid) = get_str(character, "id") else {
                    continue;
                };
                let Ok(memories) = memories_read::find_by_character_id(main, &cid) else {
                    continue;
                };
                for memory in memories {
                    if memory.get("chatId").and_then(Value::as_str) != Some(id.as_str()) {
                        continue;
                    }
                    out.push(kind_data("memory", reorder("memory", memory)));
                    counts.bump("memories");
                }
            }
        }
    }
    Ok(())
}

// ============================================================================
// roleplay templates (v4 :308)
// ============================================================================

pub(super) fn stream_roleplay_templates(
    main: &Connection,
    user_id: &str,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    for id in ids {
        let Some(template) = roleplay_templates::find_full_json_by_id(main, id)? else {
            continue;
        };
        let is_built_in = template
            .get("isBuiltIn")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_built_in || template.get("userId").and_then(Value::as_str) != Some(user_id) {
            continue;
        }
        let tag_names = resolve_tag_names(main, &tag_ids(&template));
        out.push(kind_data(
            "roleplay_template",
            Value::Object(with_tag_names("roleplay_template", &template, tag_names)),
        ));
        counts.bump("roleplayTemplates");
    }
    Ok(())
}

// ============================================================================
// the three profile families (v4 :329 / :347 / :365)
// ============================================================================

pub(super) fn stream_connection_profiles(
    main: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    for id in ids {
        let Some(profile) = connection_profiles::find_by_id(main, id)? else {
            continue;
        };
        let label = resolve_api_key_label(main, profile.get("apiKeyId").and_then(Value::as_str));
        out.push(kind_data(
            "connection_profile",
            sanitize_profile("connection_profile", &profile, label),
        ));
        counts.bump("connectionProfiles");
    }
    Ok(())
}

pub(super) fn stream_image_profiles(
    main: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    for id in ids {
        let Some(profile) = image_profiles::find_by_id(main, id)? else {
            continue;
        };
        let label = resolve_api_key_label(main, profile.get("apiKeyId").and_then(Value::as_str));
        out.push(kind_data(
            "image_profile",
            sanitize_profile("image_profile", &profile, label),
        ));
        counts.bump("imageProfiles");
    }
    Ok(())
}

pub(super) fn stream_embedding_profiles(
    main: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    for id in ids {
        let Some(profile) = embedding_profiles::find_full_json_by_id(main, id)? else {
            continue;
        };
        let label = resolve_api_key_label(main, profile.get("apiKeyId").and_then(Value::as_str));
        out.push(kind_data(
            "embedding_profile",
            sanitize_profile("embedding_profile", &profile, label),
        ));
        counts.bump("embeddingProfiles");
    }
    Ok(())
}

// ============================================================================
// tags (v4 :383)
// ============================================================================

pub(super) fn stream_tags(
    main: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    for id in ids {
        let Some(tag) = tags::find_full_by_id(main, id)? else {
            continue;
        };
        out.push(kind_data("tag", reorder("tag", tag)));
        counts.bump("tags");
    }
    Ok(())
}

// ============================================================================
// projects (v4 :397)
// ============================================================================

pub(super) fn stream_projects(
    main: &Connection,
    mount: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    let repo = projects::ProjectsRepository::new(main, mount);
    let files = FilesRepository::new(main);

    for id in ids {
        let Some(project) = repo.find_by_id(id)? else {
            continue;
        };

        let mut roster_names: Vec<String> = Vec::new();
        for cid in project
            .get("characterRoster")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let Some(cid) = cid.as_str() else { continue };
            if let Some(c) = characters_read::find_by_id(main, mount, cid)? {
                if let Some(n) = get_str(&c, "name") {
                    roster_names.push(n);
                }
            }
        }

        // v4 loads ALL chats / ALL files and counts in memory (reproduced).
        let chat_count = chats_read::find_all(main)?
            .iter()
            .filter(|c| c.get("projectId").and_then(Value::as_str) == Some(id.as_str()))
            .count();
        let file_count = files
            .find_all()?
            .iter()
            .filter(|f| f.linked_to.iter().any(|l| l == id))
            .count();

        let mut record = super::key_order::reorder_map(
            "project",
            project.as_object().cloned().unwrap_or_default(),
        );
        if !roster_names.is_empty() {
            record.insert(
                "_characterRosterNames".into(),
                Value::Array(roster_names.into_iter().map(Value::String).collect()),
            );
        }
        record.insert("_chatCount".into(), Value::from(chat_count));
        record.insert("_fileCount".into(), Value::from(file_count));
        out.push(kind_data("project", Value::Object(record)));
        counts.bump("projects");
    }
    Ok(())
}

// ============================================================================
// groups (v4 :430)
// ============================================================================

pub(super) fn stream_groups(
    main: &Connection,
    mount: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    let repo = groups::GroupsRepository::new(main, mount);
    let members = group_character_members::GroupCharacterMembersRepository::new(mount);
    let links = group_doc_mount_links::GroupDocMountLinksRepository::new(mount);

    for id in ids {
        let Some(group) = repo.find_by_id(id)? else {
            continue;
        };

        let mut member_character_ids: Vec<String> = Vec::new();
        let mut member_names: Vec<String> = Vec::new();
        if let Ok(member_ids) = members.find_character_ids_by_group_id(id) {
            for cid in member_ids {
                member_character_ids.push(cid.clone());
                if let Some(c) = characters_read::find_by_id(main, mount, &cid)? {
                    if let Some(n) = get_str(&c, "name") {
                        member_names.push(n);
                    }
                }
            }
        }

        let linked_store_ids: Vec<String> = links.find_by_group_id(id).unwrap_or_default();

        let mut record =
            super::key_order::reorder_map("group", group.as_object().cloned().unwrap_or_default());
        if !member_names.is_empty() {
            record.insert(
                "_memberNames".into(),
                Value::Array(member_names.into_iter().map(Value::String).collect()),
            );
        }
        if !member_character_ids.is_empty() {
            record.insert(
                "_memberCharacterIds".into(),
                Value::Array(
                    member_character_ids
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            );
        }
        if !linked_store_ids.is_empty() {
            record.insert(
                "_linkedStoreMountPointIds".into(),
                Value::Array(linked_store_ids.into_iter().map(Value::String).collect()),
            );
        }
        out.push(kind_data("group", Value::Object(record)));
        counts.bump("groups");
    }
    Ok(())
}

// ============================================================================
// document stores (v4 :480) — instance-scoped, mount-index partition only
// ============================================================================

pub(super) fn stream_document_stores(
    mount: &Connection,
    ids: &[String],
    counts: &mut Counts,
    out: &mut Vec<Value>,
) -> Result<(), ExportError> {
    let points = DocMountPointsRepository::new(mount);
    let folders = DocMountFoldersRepository::new(mount);
    let blobs = DocMountBlobsRepository::new(mount);

    for id in ids {
        let Some(mp) = points.find_full_json_by_id(id)? else {
            continue;
        };

        let mount_type = get_str(&mp, "mountType").unwrap_or_default();
        let mut mp_rec = Map::new();
        mp_rec.insert("id".into(), mp.get("id").cloned().unwrap_or(Value::Null));
        mp_rec.insert(
            "name".into(),
            mp.get("name").cloned().unwrap_or(Value::Null),
        );
        mp_rec.insert(
            "basePath".into(),
            mp.get("basePath").cloned().unwrap_or(Value::Null),
        );
        mp_rec.insert("mountType".into(), Value::String(mount_type.clone()));
        // `storeType` is `.optional()` in v4 — an absent key stays absent
        // (`JSON.stringify` drops `undefined`).
        if let Some(v) = mp.get("storeType") {
            mp_rec.insert("storeType".into(), v.clone());
        }
        mp_rec.insert(
            "includePatterns".into(),
            mp.get("includePatterns").cloned().unwrap_or(Value::Null),
        );
        mp_rec.insert(
            "excludePatterns".into(),
            mp.get("excludePatterns").cloned().unwrap_or(Value::Null),
        );
        mp_rec.insert(
            "enabled".into(),
            mp.get("enabled").cloned().unwrap_or(Value::Null),
        );
        out.push(kind_data("doc_mount_point", Value::Object(mp_rec)));
        counts.bump("documentStores");

        if mount_type == "database" {
            // Folders BEFORE documents so import can resolve folderId FKs.
            // v4 sorts by path LENGTH (parents before children) with `Array.sort`
            // — stable in V8, so equal-length folders keep DB read order.
            let mut rows = folders.find_by_mount_point_id(id)?;
            rows.sort_by_key(|f| f.path.chars().count());
            for folder in rows {
                let mut d = Map::new();
                d.insert(
                    "mountPointId".into(),
                    Value::String(folder.mount_point_id.clone()),
                );
                // `parentId` is `.nullable().optional()`: a SQL NULL parses to
                // `undefined`, which `JSON.stringify` DROPS (v4 emits no key).
                if let Some(pid) = folder.parent_id.clone() {
                    d.insert("parentId".into(), Value::String(pid));
                }
                d.insert("name".into(), Value::String(folder.name.clone()));
                d.insert("path".into(), Value::String(folder.path.clone()));
                out.push(kind_data("doc_mount_folder", Value::Object(d)));
                counts.bump("documentStoreFolders");
            }

            for doc in doc_mount_documents::find_full_json_by_mount_point_id(mount, id)? {
                // v4 skips links pointing at blob-type content (exported as blobs).
                let file_type = get_str(&doc, "fileType").unwrap_or_default();
                if !matches!(file_type.as_str(), "markdown" | "txt" | "json" | "jsonl") {
                    continue;
                }
                let mut d = Map::new();
                for key in [
                    "mountPointId",
                    "relativePath",
                    "fileName",
                    "fileType",
                    "content",
                    "contentSha256",
                    "plainTextLength",
                    "lastModified",
                    "folderId",
                ] {
                    d.insert(key.into(), doc.get(key).cloned().unwrap_or(Value::Null));
                }
                out.push(kind_data("doc_mount_document", Value::Object(d)));
                counts.bump("documentStoreDocuments");
            }
        }

        // Blobs — universal across mount types.
        for meta in blobs.list_full_json_by_mount_point(id, None)? {
            let Some(blob_id) = get_str(&meta, "id") else {
                continue;
            };
            let Some(data) = blobs.read_data(&blob_id)? else {
                continue;
            };

            let chunk_count = std::cmp::max(1, data.len().div_ceil(BLOB_CHUNK_BYTES));
            let sha256 = get_str(&meta, "sha256").unwrap_or_default();
            let mount_point_id = get_str(&meta, "mountPointId").unwrap_or_default();

            let mut d = Map::new();
            for key in [
                "mountPointId",
                "relativePath",
                "originalFileName",
                "originalMimeType",
                "storedMimeType",
                "sizeBytes",
                "sha256",
                "description",
            ] {
                d.insert(key.into(), meta.get(key).cloned().unwrap_or(Value::Null));
            }
            // v4's `?? null` coalescers: an absent/NULL value becomes explicit null
            // (except extractionStatus, which defaults to 'none').
            d.insert(
                "descriptionUpdatedAt".into(),
                nullish(meta.get("descriptionUpdatedAt")),
            );
            d.insert("extractedText".into(), nullish(meta.get("extractedText")));
            d.insert(
                "extractedTextSha256".into(),
                nullish(meta.get("extractedTextSha256")),
            );
            d.insert(
                "extractionStatus".into(),
                match meta.get("extractionStatus") {
                    Some(Value::Null) | None => Value::String("none".into()),
                    Some(v) => v.clone(),
                },
            );
            d.insert(
                "extractionError".into(),
                nullish(meta.get("extractionError")),
            );
            d.insert("chunkCount".into(), Value::from(chunk_count));
            out.push(kind_data("doc_mount_blob", Value::Object(d)));
            counts.bump("documentStoreBlobs");

            for index in 0..chunk_count {
                let start = index * BLOB_CHUNK_BYTES;
                let end = std::cmp::min(start + BLOB_CHUNK_BYTES, data.len());
                let slice = &data[start..end];
                let mut c = Map::new();
                c.insert("kind".into(), Value::String("doc_mount_blob_chunk".into()));
                c.insert("mountPointId".into(), Value::String(mount_point_id.clone()));
                c.insert("sha256".into(), Value::String(sha256.clone()));
                c.insert("index".into(), Value::from(index));
                c.insert("total".into(), Value::from(chunk_count));
                c.insert("dataBase64".into(), Value::String(base64_encode(slice)));
                out.push(Value::Object(c));
            }
        }

        for link_project_id in
            project_doc_mount_links::find_project_ids_by_mount_point_id(mount, id)?
        {
            let mut d = Map::new();
            d.insert("projectId".into(), Value::String(link_project_id));
            d.insert("mountPointId".into(), Value::String(id.clone()));
            out.push(kind_data("project_doc_mount_link", Value::Object(d)));
            counts.bump("documentStoreProjectLinks");
        }
    }
    Ok(())
}

/// `x ?? null` — an absent key or a JSON `null` both become explicit `null`.
fn nullish(v: Option<&Value>) -> Value {
    match v {
        Some(Value::Null) | None => Value::Null,
        Some(other) => other.clone(),
    }
}

/// `Buffer.subarray(...).toString('base64')` — standard alphabet with padding.
fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
