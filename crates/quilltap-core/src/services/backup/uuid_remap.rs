//! v4 `lib/backup/restore/uuid-remap.ts` — the `new-account` rewrite.
//!
//! v4's own header: *"New-account remapping: rewrite every UUID in the parsed
//! backup to a fresh value (and reassign ownership to the target user) so the
//! data can be imported alongside an existing account without id collisions.
//! Cross-references are remapped consistently via the shared [`UuidRemapper`]
//! so the graph stays internally connected."*
//!
//! JSON in, JSON out — this module touches no database, no filesystem, no
//! route. The risk here is not orchestration, it is a **missing field name**:
//! drop one and the restored graph carries a cross-reference to a row that was
//! never created. `backup_uuid_remap_equivalence` is the proof.
//!
//! ## The three order-sensitivity traps (`serde_json` is `preserve_order` here,
//! so key order is observable and the differential is byte-level)
//!
//! 1. **`{ ...remapFields(x, [...]), userId: targetUserId }`** — a JS spread
//!    followed by an explicit key. If `userId` already exists it keeps its
//!    **original position** with the new value; if absent it is **appended**.
//!    `Map::insert` (an `IndexMap` under `preserve_order`) has exactly those
//!    semantics — see [`with_user_id`].
//! 2. **`delete` is `shift_remove`, never `remove`.** `serde_json`'s
//!    `Map::remove` is a *swap*-remove under `preserve_order`: it moves the last
//!    entry into the hole and scrambles every following key.
//! 3. **The `remapFields` → `remapArrayFields` chain order is load-bearing.**
//!    v4 flags it itself (`:82`, "IMPORTANT: Chain remapFields → remapArrayFields
//!    so array spread doesn't overwrite remapped scalar fields"). The scalar
//!    pass runs FIRST and the array pass runs on ITS result — never the reverse,
//!    never both on the original.
//!
//! Statement order matters too: it fixes the order ids enter the memo, which the
//! differential pins with a counting id source. v4 computes the collections top
//! to bottom and evaluates `characterPluginData` / `conversationAnnotations`
//! **inside the return literal**, i.e. last of all — reproduced here.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::collect::BackupData;
use super::uuid_remapper::UuidRemapper;
use crate::vault_overlay::{is_wardrobe_item_document_path, wardrobe_item_id_for_document};

/// v4 `:61` — settings keys whose values are mount-point UUIDs. These need
/// remapping in new-account mode so they keep pointing at the right mount
/// points. Exactly three.
pub const MOUNT_POINT_SETTING_KEYS: [&str; 3] = [
    "lanternBackgroundsMountPointId",
    "userUploadsMountPointId",
    "generalMountPointId",
];

/// JS truthiness — the guard v4 writes as `if (x)`.
fn is_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        // Arrays and objects are always truthy in JS — including `[]`.
        Some(_) => true,
    }
}

/// Trap 1: `{ ...row, userId: targetUserId }`. An existing `userId` keeps its
/// position and takes the new value; an absent one is appended.
fn with_user_id(mut row: Value, target_user_id: &str) -> Value {
    if let Some(obj) = row.as_object_mut() {
        obj.insert(
            "userId".to_string(),
            Value::String(target_user_id.to_string()),
        );
    }
    row
}

/// Trap 3: the mandatory `remapFields` → `remapArrayFields` chain.
fn chain(r: &mut UuidRemapper, row: &Value, scalars: &[&str], arrays: &[&str]) -> Value {
    let scalar_pass = r.remap_fields(row, scalars);
    r.remap_array_fields(&scalar_pass, arrays)
}

/// `chain` + trap 1, the shape most of the entity table takes.
fn chain_owned(
    r: &mut UuidRemapper,
    row: &Value,
    scalars: &[&str],
    arrays: &[&str],
    target_user_id: &str,
) -> Value {
    with_user_id(chain(r, row, scalars, arrays), target_user_id)
}

/// `remapFields` alone + trap 1 (no array fields on this entity).
fn fields_owned(
    r: &mut UuidRemapper,
    row: &Value,
    scalars: &[&str],
    target_user_id: &str,
) -> Value {
    with_user_id(r.remap_fields(row, scalars), target_user_id)
}

/// Map a collection through `f`, in row order (which is memo order).
fn each(rows: &[Value], f: impl FnMut(&Value) -> Value) -> Vec<Value> {
    rows.iter().map(f).collect()
}

/// v4 `lib/backup/restore/uuid-remap.ts:70` `remapBackupData(data, targetUserId,
/// remapper)` — rewrite every UUID in a parsed backup to a fresh value (and
/// reassign ownership) so it can be restored ALONGSIDE existing data.
///
/// The manifest is NOT part of this transform: v5's [`BackupData`] carries no
/// manifest field, so v4's `manifest: data.manifest` pass-through (`:406`) is
/// the caller's business.
pub fn remap_backup_data(
    data: &BackupData,
    target_user_id: &str,
    remapper: &mut UuidRemapper,
) -> BackupData {
    let r = remapper;

    // Remap tags
    let tags = each(&data.tags, |t| fields_owned(r, t, &["id"], target_user_id));

    // P4.D264 (v4 `7c8572869`, `uuid-remap.ts:188-192`): "Wardrobe item ids
    // are not remapper keys (see buildWardrobeItemIdRemap). Built up front: a
    // wardrobe picture's `files` row names its item in `linkedTo` / `tags`, and
    // those must follow the item, not mint a stranger." Built HERE — after
    // tags, BEFORE files — because it MINTS the Wardrobe-holding mount's id
    // (R-A: every later counter id lands in v4's order).
    let wardrobe_item_id_remap = build_wardrobe_item_id_remap(data, r);

    // Remap files
    // IMPORTANT: Chain remapFields → array remaps so array spread doesn't
    // overwrite remapped scalar fields (v4 `:82`). Since `7c8572869` the two
    // arrays go element by element through `remapLinkId` (the item map first,
    // else the remapper) rather than `remapArrayFields`: `linkedTo` then
    // `tags`, each only when it IS an array, each rewritten in place (a JS
    // spread of an existing key keeps its position) — the same order, guard
    // and positions `remapArrayFields` had, so only the element lookup moved.
    let files = each(&data.files, |f| {
        let mut out = r.remap_fields(f, &["id", "projectId"]);
        if let Some(obj) = out.as_object_mut() {
            for field in ["linkedTo", "tags"] {
                if let Some(Value::Array(elements)) = obj.get(field).cloned() {
                    let mapped = elements
                        .iter()
                        .map(|e| Value::String(remap_link_id(r, &wardrobe_item_id_remap, e)))
                        .collect();
                    obj.insert(field.to_string(), Value::Array(mapped));
                }
            }
        }
        with_user_id(out, target_user_id)
    });

    // Remap characters
    let characters = each(&data.characters, |c| remap_character(r, c, target_user_id));

    // Remap connection profiles.
    //
    // `fallbackProfileId` (v4 `65f5021c8`) points at another row in this same
    // table, so it has to be remapped alongside `id` or a restored profile's
    // understudy would name a uuid that no longer exists. The remapper is lazy
    // and consistent — the same old id always yields the same new one — so a
    // profile naming an understudy that appears later in the array is remapped
    // correctly either way.
    let connection_profiles = each(&data.connection_profiles, |p| {
        chain_owned(
            r,
            p,
            &["id", "apiKeyId", "fallbackProfileId"],
            &["tags"],
            target_user_id,
        )
    });

    // Remap image profiles
    let image_profiles = each(&data.image_profiles, |p| {
        chain_owned(r, p, &["id", "apiKeyId"], &["tags"], target_user_id)
    });

    // Remap embedding profiles
    let embedding_profiles = each(&data.embedding_profiles, |p| {
        chain_owned(r, p, &["id", "apiKeyId"], &["tags"], target_user_id)
    });

    // Remap chats (complex due to participants and messages)
    let chats = each(&data.chats, |c| remap_chat(r, c, target_user_id));

    // Remap memories — note: NO `userId` rewrite. v4 leaves the backup's own
    // value in place for this collection.
    let memories = each(&data.memories, |m| {
        chain(
            r,
            m,
            &[
                "id",
                "characterId",
                "aboutCharacterId",
                "chatId",
                "sourceMessageId",
                "projectId",
            ],
            &["tags", "relatedMemoryIds"],
        )
    });

    // Remap prompt templates
    let prompt_templates = each(&data.prompt_templates, |t| {
        chain_owned(r, t, &["id"], &["tags"], target_user_id)
    });

    // Remap roleplay templates
    let roleplay_templates = each(&data.roleplay_templates, |t| {
        chain_owned(r, t, &["id"], &["tags"], target_user_id)
    });

    // Provider models are global and don't need remapping, just copy them
    let provider_models = data.provider_models.clone();

    // Remap projects
    let projects = each(&data.projects, |p| {
        chain_owned(
            r,
            p,
            &[
                "id",
                "staticBackgroundImageId",
                "storyBackgroundImageId",
                "defaultImageProfileId",
                "defaultRoleplayTemplateId",
            ],
            &["characterRoster"],
            target_user_id,
        )
    });

    // Remap groups. Only the id needs remapping — like projects, the
    // officialMountPointId is discarded and re-provisioned by `groups.create`, so
    // it's intentionally left alone (membership/links carry the group id forward).
    let groups = each(&data.groups, |g| r.remap_fields(g, &["id"]));

    // Remap LLM logs.
    // `connectionProfileId` / `imageProfileId` must be in this list (v4
    // `0cde7fbc`): connection and image profiles are themselves remapped above,
    // so a log row left holding the source instance's profile id would name
    // nothing on the receiving one — and The Almanack's per-profile attribution
    // would read every restored row as a deleted profile. Both are nullable and
    // `remap_fields` only touches string fields, so pre-4.9 rows (which carry
    // null) pass through untouched.
    let llm_logs = each(&data.llm_logs, |l| {
        fields_owned(
            r,
            l,
            &[
                "id",
                "messageId",
                "chatId",
                "characterId",
                "connectionProfileId",
                "imageProfileId",
            ],
            target_user_id,
        )
    });

    // Remap plugin configs
    let plugin_configs = each(&data.plugin_configs, |c| {
        fields_owned(r, c, &["id"], target_user_id)
    });

    // Remap chat settings. A backup from before 4.10 carries the retired
    // Concierge settings (dangerousContentSettings, the top-level vision
    // fallback, the crafter under cheapLLMSettings) and no conciergeSettings;
    // translate them here, exactly as add-concierge-settings-v1 does, before
    // the repository's schema strips the old keys (v4 `3b463d6b1`, #76). The
    // Unmoderated flag reads the BACKUP's chats.
    let backup_has_unmoderated_chats = backup_has_unmoderated_chats(&data.chats);
    let chat_settings = each(&data.chat_settings, |s| {
        remap_chat_settings(r, s, target_user_id, backup_has_unmoderated_chats)
    });

    // Remap folders
    let folders = each(&data.folders, |f| {
        fields_owned(r, f, &["id", "parentFolderId", "projectId"], target_user_id)
    });

    // Remap wardrobe items. componentItemIds reference other wardrobe items, which
    // share the same UUID space; remap them along with id/characterId so cross-refs
    // stay consistent in new-account mode. Legacy outfit presets folded into
    // composites at parse time pass through this same path.
    // P4.D264 (v4 `7c8572869`, `:459-464`): "imageFileId names a `files`
    // row, remapped with the files above."
    let wardrobe_items = each(&data.wardrobe_items, |i| {
        chain(
            r,
            i,
            &["id", "characterId", "imageFileId"],
            &["componentItemIds"],
        )
    });

    // Chat documents reference chat IDs that have been remapped above.
    let chat_documents = each(&data.chat_documents, |d| {
        r.remap_fields(d, &["id", "chatId"])
    });

    // === P4.D205 (v4 `e7d77bb60`, `uuid-remap.ts:345-362`) ===
    // An Inform row references the chat, one of its participants
    // (`participantId` is a SEAT id, never a character id), the Host record
    // message and — once consumed — the assistant message that carried it. All
    // four were remapped with the chat above, so the remapper answers with the
    // same new ids. `batchId` is not a row anywhere; it is remapped anyway so
    // the whole batch moves together and **no id from the source instance
    // survives the restore**. The nullable fields are left alone when absent —
    // `remap_fields` only touches strings.
    let chat_informs = each(&data.chat_informs, |i| {
        r.remap_fields(
            i,
            &[
                "id",
                "chatId",
                "batchId",
                "participantId",
                "recordMessageId",
                "consumedByMessageId",
            ],
        )
    });
    // === end P4.D205 ===

    // Conversation chunks reference chats and individual messages.
    let conversation_chunks = each(&data.conversation_chunks, |c| {
        chain(r, c, &["id", "chatId"], &["messageIds"])
    });

    // Vector index meta — id and characterId share the same value by convention.
    let vector_index_metas = each(&data.vector_index_metas, |m| {
        r.remap_fields(m, &["id", "characterId"])
    });

    // Vector entries — id is typically the memory id (already remapped through
    // the memory pass). characterId is also remapped.
    let vector_entries = each(&data.vector_entries, |e| {
        r.remap_fields(e, &["id", "characterId"])
    });

    // TF-IDF vocabularies. profileId is an embedding-profile id; userId moves
    // to the target user.
    let tfidf_vocabularies = each(&data.tfidf_vocabularies, |v| {
        fields_owned(r, v, &["id", "profileId"], target_user_id)
    });

    // Embedding status — entityId could be a memory/file/help_doc/etc. id, all
    // of which are already in the mapping by the time embeddingStatus is touched.
    let embedding_status = each(&data.embedding_status, |e| {
        fields_owned(r, e, &["id", "entityId", "profileId"], target_user_id)
    });

    // Document store tables — remap every FK to keep the graph internally
    // consistent. doc_mount_files row ids are shared by doc_mount_documents and
    // doc_mount_blobs (fileId), so remapping a file id once means everything
    // that points at it gets the same new id.
    let doc_mount_points = each(&data.doc_mount_points, |m| r.remap_fields(m, &["id"]));
    let doc_mount_folders = each(&data.doc_mount_folders, |f| {
        r.remap_fields(f, &["id", "mountPointId", "parentId"])
    });
    let doc_mount_files = each(&data.doc_mount_files, |f| r.remap_fields(f, &["id"]));
    let doc_mount_file_links = each(&data.doc_mount_file_links, |l| {
        r.remap_fields(l, &["id", "fileId", "mountPointId", "folderId"])
    });
    let doc_mount_chunks = each(&data.doc_mount_chunks, |c| {
        r.remap_fields(c, &["id", "linkId", "mountPointId"])
    });
    let doc_mount_documents = each(&data.doc_mount_documents, |d| {
        r.remap_fields(d, &["id", "fileId"])
    });
    let doc_mount_blobs = each(&data.doc_mount_blobs, |b| {
        r.remap_fields(b, &["id", "fileId"])
    });
    let project_doc_mount_links = each(&data.project_doc_mount_links, |l| {
        r.remap_fields(l, &["id", "projectId", "mountPointId"])
    });
    // Group join tables — direct analogues. groupId/characterId resolve to the
    // same new ids the group/character rows received (remap is consistent by
    // original id, so order within this function doesn't matter).
    let group_doc_mount_links = each(&data.group_doc_mount_links, |l| {
        r.remap_fields(l, &["id", "groupId", "mountPointId"])
    });
    let group_character_members = each(&data.group_character_members, |m| {
        r.remap_fields(m, &["id", "groupId", "characterId"])
    });

    // P4.D264 (v4 `3ee3b1342`, `:561-571`): "Wardrobe wear ledger. `id`,
    // `wearerCharacterId` and `lastWornChatId` go through the remapper like any
    // FK (a wearer or chat absent from the backup gets a fresh id that names
    // nothing — the readers already label a missing wearer and 'a chat since
    // deleted'). `itemId` does not: see buildWardrobeItemIdRemap. An item id
    // the backup carries no document for passes through unchanged."
    // `{ ...remapFields(row, …), itemId: … }` — `itemId` keeps its position
    // when present and is APPENDED (as `undefined`, dropped by JSON) when not.
    let wardrobe_wear = each(&data.wardrobe_wear, |row| {
        let mut out = r.remap_fields(row, &["id", "wearerCharacterId", "lastWornChatId"]);
        if let Some(obj) = out.as_object_mut() {
            let mapped = row
                .get("itemId")
                .and_then(Value::as_str)
                .and_then(|id| wardrobe_item_id_remap.get(id))
                .map(|id| Value::String(id.clone()))
                .or_else(|| row.get("itemId").cloned());
            match mapped {
                Some(v) => {
                    obj.insert("itemId".to_string(), v);
                }
                None => {
                    obj.shift_remove("itemId");
                }
            }
        }
        out
    });

    // Instance settings — only the mount-point keys carry UUIDs we need to
    // remap. Everything else is opaque text (numbers, JSON config blobs).
    let instance_settings = each(&data.instance_settings, |row| {
        let is_mount_key = row
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|k| MOUNT_POINT_SETTING_KEYS.contains(&k));
        if is_mount_key && is_truthy(row.get("value")) {
            // v4 REBUILDS the row as exactly `{key, value}` — any other column
            // the row carried is dropped, and the key order is fixed.
            let mut out = Map::new();
            out.insert("key".to_string(), row["key"].clone());
            out.insert("value".to_string(), Value::String(r.remap(&row["value"])));
            Value::Object(out)
        } else {
            row.clone()
        }
    });

    // v4 evaluates these two INSIDE the return literal, i.e. after every
    // statement above — which is where they land in the memo. Order preserved.
    let character_plugin_data = each(&data.character_plugin_data, |d| {
        r.remap_fields(d, &["id", "characterId"])
    });
    let conversation_annotations = each(&data.conversation_annotations, |a| {
        r.remap_fields(a, &["id", "chatId", "sourceMessageId"])
    });

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
        // === P4.D205 ===
        chat_informs,
        // === end P4.D205 ===
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
        // Text replacement rules: global config, no userId, no FKs to remapped
        // entities, and nothing references rule IDs — pass through unchanged.
        text_replacement_rules: data.text_replacement_rules.clone(),
        // P4.D264 (v4 `:629`): the 39th key, ALWAYS present (`[]` when the
        // archive has none).
        wardrobe_wear,
    }
}

/// v4 `buildWardrobeItemIdRemap(data, remapper)` (`uuid-remap.ts:95-122`,
/// `3ee3b1342`). v4's why, carried: "A wardrobe item's id is the `id` in its
/// `Wardrobe/*.md` frontmatter, or — when that is absent — one derived from
/// the file's mount point and path. Neither is a remapper key, and
/// `remapper.remap` would mint a fresh id that names no item, orphaning every
/// wear-ledger row that points at it. Instead: a frontmatter id travels
/// verbatim, so it maps to itself; an item with no frontmatter id has an id
/// derived from its mount point and path, and the mount point *is* remapped —
/// recompute it against the new mount-point id, exactly as the vault reader
/// will after the restore; a legacy (pre-cutover) `wardrobeItems` row is
/// remapped by id like any other row, so its ledger rows follow it."
///
/// ⚠ `remapper.remap(link.mountPointId)` MINTS the mount's new id HERE, which
/// is why the call sites run it where v4 does (P4.D264 R-A). A link whose
/// `relativePath` is not a string THROWS in v4 (`relativePath.toLowerCase()`
/// on `undefined` — a NOT NULL column on every real archive); v5 treats it as
/// not an item.
fn build_wardrobe_item_id_remap(
    data: &BackupData,
    r: &mut UuidRemapper,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content_by_file_id = content_by_file_id(data);
    for link in &data.doc_mount_file_links {
        let Some((mount, relative_path, content)) = wardrobe_link(link, &content_by_file_id) else {
            continue;
        };
        let old_id = wardrobe_item_id_for_document(&js_string(mount), relative_path, content);
        let new_mount = r.remap(mount);
        let new_id = wardrobe_item_id_for_document(&new_mount, relative_path, content);
        map.insert(old_id, new_id);
    }
    for item in &data.wardrobe_items {
        let id = item.get("id").unwrap_or(&Value::Null);
        let new_id = r.remap(id);
        map.insert(js_string(id), new_id);
    }
    map
}

/// v4 `new Map(docMountDocuments.map(doc => [doc.fileId, doc.content]))` — a
/// later document on the same `fileId` wins; only a STRING content counts.
fn content_by_file_id(data: &BackupData) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    for doc in &data.doc_mount_documents {
        let key = js_string(doc.get("fileId").unwrap_or(&Value::Null));
        map.insert(key, doc.get("content").cloned().unwrap_or(Value::Null));
    }
    map
}

/// A `Wardrobe/*.md` link and its document's string content — the two guards
/// both v4 walks share (`isWardrobeItemDocumentPath`, `typeof content ===
/// 'string'`).
fn wardrobe_link<'a>(
    link: &'a Value,
    content_by_file_id: &'a HashMap<String, Value>,
) -> Option<(&'a Value, &'a str, &'a str)> {
    let relative_path = link.get("relativePath").and_then(Value::as_str)?;
    if !is_wardrobe_item_document_path(relative_path) {
        return None;
    }
    let file_id = js_string(link.get("fileId").unwrap_or(&Value::Null));
    let content = content_by_file_id.get(&file_id)?.as_str()?;
    Some((
        link.get("mountPointId").unwrap_or(&Value::Null),
        relative_path,
        content,
    ))
}

/// JS `String(x)` for a map key / template-literal slot (`mountPointId` is a
/// string on every real row).
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// v4 `remapLinkId` (`:192`) — `wardrobeItemIdRemap.get(id) ?? remapper.remap(id)`.
/// The item map's keys are strings, so a non-string element (a `Map.get` miss)
/// always goes through the remapper, as `remapArray` sent it before.
fn remap_link_id(r: &mut UuidRemapper, items: &HashMap<String, String>, element: &Value) -> String {
    if let Some(mapped) = element.as_str().and_then(|id| items.get(id)) {
        return mapped.clone();
    }
    r.remap(element)
}

/// v4 `WardrobeImagePointerFix` (`uuid-remap.ts:126-135`, `7c8572869`) — one
/// wardrobe item whose current-picture pointer must follow its file's new id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeImagePointerFix {
    /// The item's mount, already remapped.
    pub mount_point_id: String,
    /// The item's mount as the backup named it (before the remap).
    pub source_mount_point_id: String,
    /// The item's id in the restored instance.
    pub item_id: String,
    /// The picture's remapped `files` id.
    pub image_file_id: String,
}

/// v4 `planWardrobeImagePointerFixes(original, remapper)` (`:147-172`). v4's
/// why, carried: "New-account mode remaps `files.id`, but an item's
/// `imageFileId` lives in its `Wardrobe/*.md` frontmatter, which this remap
/// never rewrites. List the pointers that need to follow their file, for the
/// restore to apply through the ordinary wardrobe update once the vaults have
/// landed (that path re-projects the document, so content hashes stay
/// consistent). Pass the ORIGINAL (pre-remap) backup data and the same
/// remapper used for `remapBackupData`, so file ids resolve to the ids the rows
/// received." The restore calls it BEFORE [`remap_backup_data`] (`restore.ts:
/// 98-104`), so it primes the remapper with the mount, legacy-item and pointer
/// ids — `backup_uuid_remap_equivalence`'s `restorePath` cases pin that order.
pub fn plan_wardrobe_image_pointer_fixes(
    original: &BackupData,
    r: &mut UuidRemapper,
) -> Vec<WardrobeImagePointerFix> {
    let file_ids: HashSet<String> = original
        .files
        .iter()
        .map(|f| js_string(f.get("id").unwrap_or(&Value::Null)))
        .collect();
    let item_id_remap = build_wardrobe_item_id_remap(original, r);
    let content_by_file_id = content_by_file_id(original);
    let mut fixes = Vec::new();
    for link in &original.doc_mount_file_links {
        let Some((mount, relative_path, content)) = wardrobe_link(link, &content_by_file_id) else {
            continue;
        };
        let fm = crate::markdown::parse_frontmatter(content);
        let Some(pointer) = fm
            .data
            .as_ref()
            .and_then(|d| d.get("imageFileId"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if !file_ids.contains(pointer) {
            continue;
        }
        let source_mount = js_string(mount);
        let old_item_id = wardrobe_item_id_for_document(&source_mount, relative_path, content);
        let mount_point_id = r.remap(mount);
        let item_id = item_id_remap
            .get(&old_item_id)
            .cloned()
            .unwrap_or(old_item_id);
        let image_file_id = r.remap_str(pointer);
        fixes.push(WardrobeImagePointerFix {
            mount_point_id,
            source_mount_point_id: source_mount,
            item_id,
            image_file_id,
        });
    }
    fixes
}

/// v4 `:92-145` — the character pass and its five legacy-shape extras, in v4's
/// order.
fn remap_character(r: &mut UuidRemapper, char_row: &Value, target_user_id: &str) -> Value {
    let mut remapped = chain_owned(
        r,
        char_row,
        &[
            "id",
            "defaultImageId",
            "defaultConnectionProfileId",
            "defaultPartnerId",
            "defaultImageProfileId",
        ],
        &["tags"],
        target_user_id,
    );
    if !remapped.is_object() {
        return remapped;
    }
    let obj = remapped.as_object_mut().expect("checked above");

    // 1. partnerLinks (new format) — `{...link, partnerId: remap(...)}`, so a
    //    link's other keys survive and `partnerId` keeps its position.
    let had_partner_links = is_truthy(obj.get("partnerLinks"));
    if had_partner_links {
        if let Some(links) = obj.get("partnerLinks").and_then(Value::as_array).cloned() {
            let mut out = Vec::with_capacity(links.len());
            for link in &links {
                let mut l = link.as_object().cloned().unwrap_or_default();
                let new = r.remap(l.get("partnerId").unwrap_or(&Value::Null));
                l.insert("partnerId".to_string(), Value::String(new));
                out.push(Value::Object(l));
            }
            obj.insert("partnerLinks".to_string(), Value::Array(out));
        }
    }

    // 2. personaLinks (old backup format) — folded into partnerLinks, and ONLY
    //    the two keys survive. `partnerLinks` is absent here by the guard, so
    //    the insert APPENDS it; then the legacy key is deleted (trap 2).
    if is_truthy(obj.get("personaLinks")) && !had_partner_links {
        let links = obj
            .get("personaLinks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(links.len());
        for link in &links {
            let mut l = Map::new();
            let src = link.as_object();
            let persona = src.and_then(|m| m.get("personaId")).unwrap_or(&Value::Null);
            l.insert("partnerId".to_string(), Value::String(r.remap(persona)));
            // `isDefault: link.isDefault` — an absent source key writes
            // `undefined`, which `JSON.stringify` drops, so the key is omitted.
            if let Some(is_default) = src.and_then(|m| m.get("isDefault")) {
                l.insert("isDefault".to_string(), is_default.clone());
            }
            out.push(Value::Object(l));
        }
        obj.insert("partnerLinks".to_string(), Value::Array(out));
        obj.shift_remove("personaLinks");
    }

    // 3. avatarOverrides — rebuilt as exactly `{chatId, imageId}`; chatId is
    //    remapped first.
    if is_truthy(obj.get("avatarOverrides")) {
        if let Some(overrides) = obj
            .get("avatarOverrides")
            .and_then(Value::as_array)
            .cloned()
        {
            let mut out = Vec::with_capacity(overrides.len());
            for o in &overrides {
                let src = o.as_object();
                let chat_id = src
                    .and_then(|m| m.get("chatId"))
                    .unwrap_or(&Value::Null)
                    .clone();
                let image_id = src
                    .and_then(|m| m.get("imageId"))
                    .unwrap_or(&Value::Null)
                    .clone();
                let mut m = Map::new();
                m.insert("chatId".to_string(), Value::String(r.remap(&chat_id)));
                m.insert("imageId".to_string(), Value::String(r.remap(&image_id)));
                out.push(Value::Object(m));
            }
            obj.insert("avatarOverrides".to_string(), Value::Array(out));
        }
    }

    // 4. physicalDescription: backups may carry either the legacy plural array
    //    (`physicalDescriptions`) or the new singular field
    //    (`physicalDescription`). Collapse to the first record either way.
    if let Some(Value::Array(plural)) = obj.get("physicalDescriptions").cloned() {
        let collapsed = match plural.first() {
            Some(first) => {
                let mut m = first.as_object().cloned().unwrap_or_default();
                let new = r.remap(m.get("id").unwrap_or(&Value::Null));
                m.insert("id".to_string(), Value::String(new));
                Value::Object(m)
            }
            None => Value::Null,
        };
        obj.insert("physicalDescription".to_string(), collapsed);
        obj.shift_remove("physicalDescriptions");
    } else if is_truthy(obj.get("physicalDescription")) {
        if let Some(mut m) = obj
            .get("physicalDescription")
            .and_then(Value::as_object)
            .cloned()
        {
            let new = r.remap(m.get("id").unwrap_or(&Value::Null));
            m.insert("id".to_string(), Value::String(new));
            obj.insert("physicalDescription".to_string(), Value::Object(m));
        }
    }

    // 5. Legacy `clothingRecords` — the table is gone; drop silently so old
    //    backups still restore.
    if is_truthy(obj.get("clothingRecords")) {
        obj.shift_remove("clothingRecords");
    }

    remapped
}

/// v4 `:176-211` — the chat pass. `participants` and `messages` are read from
/// the ORIGINAL chat and assigned onto the remapped copy, so both keep their
/// original positions (trap 1).
fn remap_chat(r: &mut UuidRemapper, chat: &Value, target_user_id: &str) -> Value {
    let mut remapped = chain_owned(
        r,
        chat,
        &[
            "id",
            "activeTypingParticipantId",
            "lastTurnParticipantId",
            "projectId",
            "storyBackgroundImageId",
            "imageProfileId",
        ],
        &["tags", "impersonatingParticipantIds"],
        target_user_id,
    );

    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .map(|p| {
                    r.remap_fields(
                        p,
                        &["id", "characterId", "connectionProfileId", "imageProfileId"],
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let messages: Vec<Value> = chat
        .get("messages")
        .and_then(Value::as_array)
        .map(|ms| {
            ms.iter()
                .map(|m| {
                    chain(
                        r,
                        m,
                        &["id", "swipeGroupId", "participantId"],
                        &["attachments"],
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(obj) = remapped.as_object_mut() {
        obj.insert("participants".to_string(), Value::Array(participants));
        obj.insert("messages".to_string(), Value::Array(messages));
    }
    remapped
}

/// v4 `:271-301` — chat settings plus its three nested bags. Each nested id is
/// written **only when truthy** (v4 uses conditional spreads), so a `null` or
/// absent nested id stays exactly as it was rather than being overwritten with
/// `null`.
/// v4 `(data.chats || []).some(chat =>
/// getConciergeState(withConciergeModeFromLegacy(chat)) === 'unmoderated')` —
/// whether the BACKUP (not the target instance) holds an Unmoderated chat,
/// deriving a pre-4.10 chat's state from its legacy pair first.
pub fn backup_has_unmoderated_chats(chats: &[Value]) -> bool {
    use crate::services::dangerous_content::chat_override::{
        get_concierge_state, with_concierge_mode_from_legacy, ConciergeState,
    };
    chats.iter().any(|chat| {
        get_concierge_state(Some(&with_concierge_mode_from_legacy(chat.clone())))
            == ConciergeState::Unmoderated
    })
}

fn remap_chat_settings(
    r: &mut UuidRemapper,
    settings: &Value,
    target_user_id: &str,
    backup_has_unmoderated_chats: bool,
) -> Value {
    // v4 `3b463d6b1`: `uncensoredImageDescriptionProfileId` is off the scalar
    // list (it moved into `conciergeSettings`); the mint ORDER follows v4's
    // statements — the scalars, then the four desk ids, then the cheap-LLM
    // pair, then the story background.
    let mut remapped = fields_owned(
        r,
        settings,
        &[
            "id",
            "imageDescriptionProfileId",
            "defaultRoleplayTemplateId",
        ],
        target_user_id,
    );
    if !remapped.is_object() {
        return remapped;
    }
    let obj = remapped.as_object_mut().expect("checked above");

    // `remapped.conciergeSettings = { ...concierge, ...each desk id remapped
    // when truthy }`, where `concierge` is the ORIGINAL row translated from
    // its legacy keys when it carries no truthy `conciergeSettings` — the
    // stored object otherwise, wholesale.
    let translated = crate::services::dangerous_content::legacy_concierge_settings::with_concierge_settings_from_legacy(
        settings,
        backup_has_unmoderated_chats,
    );
    let mut concierge = translated
        .get("conciergeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for f in [
        "uncensoredTextProfileId",
        "uncensoredImageProfileId",
        "uncensoredVisionProfileId",
        "imagePromptProfileId",
    ] {
        if is_truthy(concierge.get(f)) {
            let old = concierge[f].clone();
            concierge.insert(f.to_string(), Value::String(r.remap(&old)));
        }
    }
    obj.insert("conciergeSettings".to_string(), Value::Object(concierge));

    remap_nested_bag(
        r,
        obj,
        "cheapLLMSettings",
        &["userDefinedProfileId", "defaultCheapProfileId"],
        false,
    );
    // storyBackgroundsSettings is guarded on the ONE id
    // (`?.defaultImageProfileId`), not on the bag — the bag is rewritten only
    // when that id is present and truthy.
    remap_nested_bag(
        r,
        obj,
        "storyBackgroundsSettings",
        &["defaultImageProfileId"],
        true,
    );
    // P4.D264 (v4 `7c8572869`, `:440-445`): the same shape — guarded on the
    // ONE id (`?.imageProfileId` truthy), the bag spread-rewritten in place.
    remap_nested_bag(r, obj, "wardrobeImageSettings", &["imageProfileId"], true);

    remapped
}

/// One `{ ...bag, ...(bag.k ? { k: remap(bag.k) } : {}) }` pass.
///
/// `guard_on_fields`: v4 guards `storyBackgroundsSettings` on its single id
/// rather than on the bag, so an untouched bag is not even rewritten. The
/// observable difference is nil for an object (the rebuild is key-for-key), but
/// it is faithfully what v4 branches on.
fn remap_nested_bag(
    r: &mut UuidRemapper,
    obj: &mut Map<String, Value>,
    bag_key: &str,
    fields: &[&str],
    guard_on_fields: bool,
) {
    let guard = if guard_on_fields {
        fields.iter().all(|f| {
            is_truthy(
                obj.get(bag_key)
                    .and_then(Value::as_object)
                    .and_then(|b| b.get(*f)),
            )
        })
    } else {
        is_truthy(obj.get(bag_key))
    };
    if !guard {
        return;
    }
    let Some(mut bag) = obj.get(bag_key).and_then(Value::as_object).cloned() else {
        return;
    };
    for f in fields {
        if is_truthy(bag.get(*f)) {
            let old = bag[*f].clone();
            bag.insert((*f).to_string(), Value::String(r.remap(&old)));
        }
    }
    obj.insert(bag_key.to_string(), Value::Object(bag));
}
