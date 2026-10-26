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
use crate::chat::character_keyed_fields::{
    remap_character_keyed_chat_fields, CharacterKeyedRemaps,
};
use crate::inventory::folders::is_inventory_item_document_path;
use crate::vault_overlay::{is_wardrobe_item_document_path, wardrobe_item_id_for_document};
use crate::wardrobe::WARDROBE_SLOT_TYPES;

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
///
/// **Fallible since P4.D317 (v4 `7953da3a0`).** v4's outfit-history arm reads
/// each row's slots with NO normalize — `(row.slots?.[slot] ?? []).map(…)` — so
/// a non-array slot value (or a `null` row) throws a `TypeError` that aborts
/// the WHOLE `remapBackupData`, and the new-account restore fails before it
/// writes anything. Ported faithfully (R-K / R-9 (b) — a v4 filing candidate,
/// not fixed): the `Err` carries the failure for the restore to refuse with;
/// `replace` mode never remaps, and salvages such a row in `upsert_raw_rows`.
pub fn remap_backup_data(
    data: &BackupData,
    target_user_id: &str,
    remapper: &mut UuidRemapper,
) -> Result<BackupData, String> {
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
    // P4.D327 (v4 `4fbdad186`, `uuid-remap.ts:303-306`): "Inventory object
    // ids travel verbatim, so a picture naming one keeps it." Built from the
    // ORIGINAL data, no mint, right after the wardrobe map.
    let inventory_ids = inventory_item_ids(data);

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
                        .map(|e| {
                            Value::String(remap_link_id(
                                r,
                                &inventory_ids,
                                &wardrobe_item_id_remap,
                                e,
                            ))
                        })
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
    // P4.D317 (v4 `e8e17b2d9`, `uuid-remap.ts:331`): the characters the backup
    // carries, built ONCE before the chats pass — bug 252's character-keyed
    // pass drops an entry for any other character instead of minting.
    let backup_character_ids: HashSet<String> = data
        .characters
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let chats = each(&data.chats, |c| {
        remap_chat(
            r,
            c,
            target_user_id,
            &wardrobe_item_id_remap,
            &backup_character_ids,
        )
    });

    // Remap memories — note: NO `userId` rewrite. v4 leaves the backup's own
    // value in place for this collection.
    //
    // `supersededById` and `consolidatedFrom` are memory-id references (digest
    // <-> cold members), remapped in lockstep with `id` exactly like
    // `relatedMemoryIds` (v4 `d58548051`). Old backups lack them;
    // remapFields/remapArrayFields skip absent or null values.
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
                "supersededById",
            ],
            &["tags", "relatedMemoryIds", "consolidatedFrom"],
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
            // officialMountPointId remapped in lockstep with the store's own
            // row, so the restore can bind the project to its archived store
            // (bug 185, v4 `ed8b15b50`).
            &[
                "id",
                "officialMountPointId",
                "staticBackgroundImageId",
                "storyBackgroundImageId",
                "defaultImageProfileId",
                "defaultRoleplayTemplateId",
            ],
            &["characterRoster"],
            target_user_id,
        )
    });

    // Remap groups. Like projects, officialMountPointId is remapped in lockstep
    // with the store's own row so the restore binds the group to its archived
    // store rather than minting a fresh one (bug 185, v4 `ed8b15b50`);
    // membership and links carry the group id forward.
    let groups = each(&data.groups, |g| {
        r.remap_fields(g, &["id", "officialMountPointId"])
    });

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

    // P4.D293 (v4 `af48f6cc2`): the vector-index metas are no longer
    // remapped — no backup writes them and the restore ignores them.
    //
    // Vector entries (LEGACY, pre-Phase-B archives) — id is the memory id
    // (already remapped through the memory pass), so the restore's adoption
    // matches the REMAPPED memory (v4 `uuid-remap.ts:515-527`). characterId
    // is also remapped.
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

    // === P4.D317 (v4 `7953da3a0`, `uuid-remap.ts:664-675`) ===
    // "Outfit history. `id`, `chatId`, `characterId` and `actorCharacterId` go
    // through the remapper; slot item ids follow the same item remap as the
    // wear ledger (an id the backup carries no document for passes through and
    // falls under the stale-item rule). Indices, cursor and dates are kept."
    let mut wardrobe_history = Vec::with_capacity(data.wardrobe_history.len());
    for row in &data.wardrobe_history {
        wardrobe_history.push(remap_wardrobe_history_row(r, &wardrobe_item_id_remap, row)?);
    }
    // === end P4.D317 ===

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

    Ok(BackupData {
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
        // P4.D317 (`:733`) — LAST, always present (`[]` when the backup had none).
        wardrobe_history,
    })
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

/// v4 `remapLinkId` (`:304-306` at `322a5488e`) — `inventoryIds.has(id) ? id
/// : wardrobeItemIdRemap.get(id) ?? remapper.remap(id)`: an Inventory object
/// id FIRST (kept verbatim, P4.D327), then the wardrobe item map, then the
/// remapper. Both sets are keyed by strings, so a non-string element (a
/// `Set.has` / `Map.get` miss) always goes through the remapper, as
/// `remapArray` sent it before.
fn remap_link_id(
    r: &mut UuidRemapper,
    inventory_ids: &HashSet<String>,
    items: &HashMap<String, String>,
    element: &Value,
) -> String {
    if let Some(id) = element.as_str().filter(|id| inventory_ids.contains(*id)) {
        return id.to_string();
    }
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

// === P4.D327: the Inventory's documents in a backup (v4 `4fbdad186`,
// `uuid-remap.ts:182-257` at `322a5488e`) ===

/// v4 `inventoryDocuments(data)` — "The Inventory's documents in a backup:
/// each object file under `Inventory/` (any depth) with its parsed JSON,
/// skipping pictures and anything that does not parse into an object
/// carrying an id." In LINK order; `id` is any NON-EMPTY string (NOT
/// UUID-checked — v4 does not call `parseInventoryItemFile` here).
///
/// The parse is the Inventory's one V8 `JSON.parse` twin
/// ([`crate::inventory::types::js_json_parse`] — serde underneath): on a
/// `1e400`, a lone-surrogate `\u` escape or nesting past 128 v4 reads the
/// file and v5 skips it, so v5 mints a stranger for that object's picture
/// link where v4 keeps its id (the P4.D317 R-2 class, recorded). A link whose
/// `relativePath` is not a string THROWS in v4 (`.toLowerCase()` on
/// `undefined`); v5 treats it as not an object (the P4.D264 class).
fn inventory_documents(data: &BackupData) -> Vec<(&Value, Map<String, Value>, String)> {
    let content_by_file_id = content_by_file_id(data);
    let mut out = Vec::new();
    for link in &data.doc_mount_file_links {
        let Some(relative_path) = link.get("relativePath").and_then(Value::as_str) else {
            continue;
        };
        if !is_inventory_item_document_path(relative_path) {
            continue;
        }
        let file_id = js_string(link.get("fileId").unwrap_or(&Value::Null));
        let Some(content) = content_by_file_id.get(&file_id).and_then(Value::as_str) else {
            continue;
        };
        let Ok(Value::Object(json)) = crate::inventory::types::js_json_parse(content) else {
            continue;
        };
        let Some(id) = json
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        out.push((link, json, id));
    }
    out
}

/// v4 `inventoryItemIds(data)` — "Inventory object ids in a backup. Like a
/// wardrobe frontmatter id, an object's id lives in its file and travels
/// verbatim (as do the `parentId` and kit `itemIds` that name other objects),
/// so these ids are never remapper keys: a picture's `files.linkedTo` and a
/// chat's `equippedInventory` keep naming them as they are."
fn inventory_item_ids(data: &BackupData) -> HashSet<String> {
    inventory_documents(data)
        .into_iter()
        .map(|(_, _, id)| id)
        .collect()
}

/// v4 `InventoryImagePointerFix` — "One Inventory object whose current-picture
/// pointer must follow its file's new id."
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryImagePointerFix {
    /// The object's mount, already remapped.
    pub mount_point_id: String,
    /// The object's mount as the backup named it (before the remap).
    pub source_mount_point_id: String,
    /// The object's id (unchanged by the remap).
    pub item_id: String,
    /// The picture's remapped `files` id.
    pub image_file_id: String,
}

/// v4 `planInventoryImagePointerFixes(original, remapper)` — "The Inventory
/// analogue of `planWardrobeImagePointerFixes`: an object's `imageFileId`
/// lives in its JSON file, which the remap never rewrites, so list the
/// pointers the restore must repoint once the vaults have landed. Pass the
/// ORIGINAL backup data and the same remapper." The restore runs it AFTER the
/// wardrobe plan and BEFORE [`remap_backup_data`] (`restore.ts:119-121`), and
/// per qualifying link it MINTS the object's mount, THEN the picture's file.
pub fn plan_inventory_image_pointer_fixes(
    original: &BackupData,
    r: &mut UuidRemapper,
) -> Vec<InventoryImagePointerFix> {
    let file_ids: HashSet<String> = original
        .files
        .iter()
        .map(|f| js_string(f.get("id").unwrap_or(&Value::Null)))
        .collect();
    let mut fixes = Vec::new();
    for (link, json, id) in inventory_documents(original) {
        let Some(pointer) = json.get("imageFileId").and_then(Value::as_str) else {
            continue;
        };
        if !file_ids.contains(pointer) {
            continue;
        }
        let mount = link.get("mountPointId").unwrap_or(&Value::Null);
        let mount_point_id = r.remap(mount);
        let image_file_id = r.remap_str(pointer);
        fixes.push(InventoryImagePointerFix {
            mount_point_id,
            source_mount_point_id: js_string(mount),
            item_id: id,
            image_file_id,
        });
    }
    fixes
}
// === end P4.D327 ===

/// v4 `:92-145` — the character pass and its five legacy-shape extras, in v4's
/// order.
fn remap_character(r: &mut UuidRemapper, char_row: &Value, target_user_id: &str) -> Value {
    let mut remapped = chain_owned(
        r,
        char_row,
        // The vault pointer is remapped like any FK (bug 185, v4 `ed8b15b50`):
        // the remapper maps one original id to one new id everywhere, so the
        // pointer names the id its vault's mount-point row is restored under,
        // and the restore binds the character to that vault rather than
        // minting one.
        &[
            "id",
            "characterDocumentMountPointId",
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

/// P4.D317 — one row of v4's outfit-history arm (`uuid-remap.ts:664-675`):
///
/// ```ts
/// const slots = bySlot((slot) => (row.slots?.[slot] ?? []).map((id) => wardrobeItemIdRemap.get(id) ?? id));
/// return { ...remapper.remapFields(row, ['id', 'chatId', 'characterId', 'actorCharacterId']), slots, slotsHash: hashEquippedSlots(slots) };
/// ```
///
/// - The slots are read with NO normalize: a missing / `null` slot reads `[]`,
///   a non-string element passes through as given (a `Map.get` miss), and a
///   non-array, non-nullish slot value makes the `.map` THROW — `Err`, which
///   aborts the whole remap (R-9 (b), faithful). A `null` row throws too
///   (`row.slots` on `null`).
/// - `remapFields` rewrites STRING values only, in field order (the mint
///   order): a `null` actor stays `null`, and a `characterId` the backup does
///   not carry is MINTED — a phantom that names nobody, where bug 252's chat
///   pass DROPS the same character's entry (R-9 (a), faithful; a filing
///   candidate).
/// - `slots` / `slotsHash` land on their EXISTING keys (the spread keeps a
///   key's position) or are appended in that order; the hash is recomputed
///   over the rebuilt five-slot bag, non-string elements and all. An absent
///   `actorToolName` stays absent (the arm never adds it).
fn remap_wardrobe_history_row(
    r: &mut UuidRemapper,
    item_remap: &HashMap<String, String>,
    row: &Value,
) -> Result<Value, String> {
    let slots_value = match row {
        Value::Null => return Err("Cannot read properties of null (reading 'slots')".to_string()),
        Value::Object(m) => m.get("slots"),
        // `"abc".slots` / `[].slots` / `(5).slots` — undefined.
        _ => None,
    };
    let mut slots = Map::new();
    for slot in WARDROBE_SLOT_TYPES {
        // `row.slots?.[slot]` — only an object (an array, for an index-named
        // key it never is) has the slot; anything else reads undefined.
        let value = match slots_value {
            Some(Value::Object(s)) => s.get(slot).filter(|v| !v.is_null()),
            _ => None,
        };
        let ids = match value {
            None => Vec::new(),
            Some(Value::Array(ids)) => ids
                .iter()
                .map(|id| match id {
                    Value::String(s) => item_remap
                        .get(s)
                        .map(|mapped| Value::String(mapped.clone()))
                        .unwrap_or_else(|| id.clone()),
                    other => other.clone(),
                })
                .collect(),
            Some(_) => {
                // V8's own text for the transpiled `(row.slots?.[slot] ?? [])
                // .map(…)` (measured from the oracle).
                return Err("((intermediate value) ?? []).map is not a function".to_string());
            }
        };
        slots.insert(slot.to_string(), Value::Array(ids));
    }
    let slots = Value::Object(slots);
    let hash: String = crate::db::doc_mount_file_links::sha256_of_string(
        &crate::pascal::js_value::json_stringify(&slots),
    )
    .chars()
    .take(16)
    .collect();
    let mut out = match r.remap_fields(row, &["id", "chatId", "characterId", "actorCharacterId"]) {
        Value::Object(m) => m,
        // `{ ...nonObject }`: an array spreads its elements under index keys,
        // a string its UTF-16 units; a number or boolean spreads nothing.
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        Value::String(s) => s
            .encode_utf16()
            .enumerate()
            .map(|(i, unit)| {
                (
                    i.to_string(),
                    Value::String(String::from_utf16_lossy(&[unit])),
                )
            })
            .collect(),
        _ => Map::new(),
    };
    out.insert("slots".to_string(), slots);
    out.insert("slotsHash".to_string(), Value::String(hash));
    Ok(Value::Object(out))
}

/// v4 `PARTICIPANT_ID_JSON_FIELDS` (`49f8906c3`, pin `uuid-remap.ts:178-182`) —
/// the chat columns that hold participant ids as a JSON-encoded string array
/// rather than a real array, so `remap_array_fields` cannot see them.
const PARTICIPANT_ID_JSON_FIELDS: [&str; 3] = [
    "turnQueue",
    "spokenThisCycleParticipantIds",
    "cycleOrderParticipantIds",
];

/// v4 `remapJsonIdList(value, remapper)` (`49f8906c3`, pin
/// `uuid-remap.ts:189-197`) — remap a JSON-encoded array of ids. "A value that
/// is not a JSON string array is returned untouched — the turn manager already
/// reads a malformed value as empty, so there is nothing to gain by rewriting
/// it." A non-string, an unparsable string, a parsed non-array, or an array
/// holding any non-string comes back as the ORIGINAL value (the original
/// string, whitespace and all); a valid list is re-serialised COMPACT, as
/// `JSON.stringify` writes it, every element through the remapper in order.
///
/// Recorded divergence class `JSON_LONE_SURROGATE_ID` (P4.D312 R-3, no row):
/// V8's `JSON.parse` accepts a lone-surrogate `\u` escape where `serde_json`
/// refuses it, so such a list is remapped by v4 and left untouched here. No
/// real seat id is a lone surrogate.
fn remap_json_id_list(value: &Value, r: &mut UuidRemapper) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    let Ok(parsed) = serde_json::from_str::<Value>(text) else {
        return value.clone();
    };
    match parsed.as_array() {
        Some(ids) if ids.iter().all(Value::is_string) => {
            let remapped = r.remap_array(&parsed);
            Value::String(serde_json::to_string(&remapped).expect("a string array serialises"))
        }
        _ => value.clone(),
    }
}

/// v4 `:176-211` — the chat pass. `participants` and `messages` are read from
/// the ORIGINAL chat and assigned onto the remapped copy, so both keep their
/// original positions (trap 1).
fn remap_chat(
    r: &mut UuidRemapper,
    chat: &Value,
    target_user_id: &str,
    wardrobe_item_id_remap: &HashMap<String, String>,
    backup_character_ids: &HashSet<String>,
) -> Value {
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
            // Message-id reference: messages are remapped below, so the
            // "other-subject", episode and turn extraction watermarks must
            // follow their messages (v4 `d58548051`; P4.D293 — `c2795733f`'s
            // episode key, `64743ee13`'s turn key, LAST in `BUNDLE_CURSOR_
            // FIELDS` order — the mint order the corpus pins). The remapper
            // always mints, so a cursor naming a message outside the archive
            // still gets a fresh id.
            "otherExtractionWatermarkMessageId",
            "episodeExtractionWatermarkMessageId",
            "turnExtractionWatermarkMessageId",
        ],
        &["tags", "impersonatingParticipantIds"],
        target_user_id,
    );

    // P4.D293 (v4 `545e299bb`, `uuid-remap.ts:320-324`): "The scenario source
    // names a mount; mounts are remapped by the same remapper, so the pointer
    // follows its store." Only for a TRUTHY source (an absent / `null` / `""`
    // one is left as it came); the result — re-pointed, or `null` for a source
    // the schema refuses (no mint then), a JSON string becoming an OBJECT — is
    // assigned onto the existing key, which keeps its place. Through C1-11's
    // `remap_scenario_source`; the mint lands after the chat scalars and
    // `userId`, before the participants.
    if is_truthy(chat.get("scenarioSource")) {
        let remapper = std::cell::RefCell::new(&mut *r);
        let source = crate::services::carriers::scenario_source_remap::remap_scenario_source(
            &chat["scenarioSource"],
            |id| Some(remapper.borrow_mut().remap(&Value::String(id.to_string()))),
        );
        if let Some(obj) = remapped.as_object_mut() {
            obj.insert(
                "scenarioSource".to_string(),
                source
                    .and_then(|s| serde_json::to_value(s).ok())
                    .unwrap_or(Value::Null),
            );
        }
    }

    // === P4.D317 (v4 `e8e17b2d9`, `uuid-remap.ts:360-380`; the carried
    // Inventory joined at `322a5488e`, P4.D327) — bug 252 ===
    // "Outfits, outfit avatars, Courier checkpoints, queued outfit notices and
    // the carried Inventory are keyed by character id (bug 252); one pass
    // carries them all. An entry
    // for a character the backup does not carry is dropped rather than given a
    // minted id that names nobody." Run on the ORIGINAL chat; each patched
    // column assigned onto its EXISTING key (`Object.assign` keeps the slot; a
    // null / absent column is never in the patch). AFTER the scenario source,
    // BEFORE the participant-id lists — the mint order: an unseen avatar file
    // and a Courier message id now mint HERE, and the chat's own messages
    // reuse the message id below.
    {
        let remapper = std::cell::RefCell::new(&mut *r);
        let character_id = |id: &str| {
            backup_character_ids
                .contains(id)
                .then(|| remapper.borrow_mut().remap_str(id))
        };
        let wardrobe_item_id = |id: &str| {
            wardrobe_item_id_remap
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.to_string())
        };
        let fresh = |id: &str| remapper.borrow_mut().remap_str(id);
        let keyed = remap_character_keyed_chat_fields(
            chat,
            &CharacterKeyedRemaps {
                character_id: &character_id,
                wardrobe_item_id: &wardrobe_item_id,
                file_id: &fresh,
                message_id: &fresh,
            },
        );
        let chat_id = chat.get("id").map(js_string);
        if !keyed.unreadable.is_empty() {
            let fields = serde_json::to_string(&keyed.unreadable).unwrap_or_default();
            tracing::warn!(
                target: "quilltap::backup::uuid_remap",
                module = "backup:uuid-remap",
                chatId = chat_id.as_deref(),
                fieldsJson = %fields,
                "Chat character-keyed columns unreadable; left as given"
            );
        }
        if !keyed.patch.is_empty() {
            let fields: Vec<&String> = keyed.patch.keys().collect();
            let fields = serde_json::to_string(&fields).unwrap_or_default();
            tracing::debug!(
                target: "quilltap::backup::uuid_remap",
                module = "backup:uuid-remap",
                chatId = chat_id.as_deref(),
                fieldsJson = %fields,
                droppedEntries = keyed.dropped_entries,
                "Remapped character-keyed chat fields"
            );
        }
        if let Some(obj) = remapped.as_object_mut() {
            for (field, value) in keyed.patch {
                obj.insert(field, value);
            }
        }
    }
    // === end P4.D317 ===

    // P4.D312 (v4 `49f8906c3`, pin `uuid-remap.ts:352-361`): "Turn-manager
    // state names participants inside JSON strings; left alone, the rotation
    // and turn queue would name seats the restore has renamed." Key PRESENCE
    // (`field in chat`), assigned onto the existing key so it keeps its
    // place; AFTER the scenario source and BEFORE the participants — the mint
    // order the counter ids make observable.
    for field in PARTICIPANT_ID_JSON_FIELDS {
        if let Some(value) = chat.get(field) {
            let remapped_list = remap_json_id_list(value, r);
            if let Some(obj) = remapped.as_object_mut() {
                obj.insert(field.to_string(), remapped_list);
            }
        }
    }

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
                    // `targetParticipantIds` is a whisper's audience:
                    // unremapped, a whisper would address seats that no
                    // longer exist (v4 `49f8906c3`, pin `:373-381`).
                    chain(
                        r,
                        m,
                        &["id", "swipeGroupId", "participantId"],
                        &["attachments", "targetParticipantIds"],
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

#[cfg(test)]
mod seat_id_tests {
    //! P4.D312 — v4 `49f8906c3`'s two `uuid-remapper.test.ts` cases (`@@
    //! -524`), transcribed one for one through the chat pass, plus the compact
    //! re-serialisation and the present-`null` arm.

    use super::*;
    use serde_json::json;

    fn counting() -> UuidRemapper {
        let mut n = 0u32;
        UuidRemapper::with_id_source(Box::new(move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }))
    }

    /// P4.D317 (bug 252) — the patched columns land on their EXISTING keys
    /// (v4's `Object.assign` onto the spread copy): a re-keyed column keeps
    /// its position, a `null` one stays `null`, an absent one stays absent
    /// (v4's "leaves a chat with no outfit alone").
    #[test]
    fn character_keyed_columns_keep_their_key_positions() {
        let chat = json!({
            "id": "chat-old",
            "characterAvatars": { "char-a": { "imageId": "file-1" } },
            "equippedOutfit": null,
            "title": "kept",
            "participants": [],
            "messages": [],
        });
        let carried: HashSet<String> = ["char-a".to_string()].into_iter().collect();
        let out = remap_chat(&mut counting(), &chat, "u", &HashMap::new(), &carried);
        let keys: Vec<&str> = out
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "characterAvatars",
                "equippedOutfit",
                "title",
                "participants",
                "messages",
                "userId"
            ]
        );
        assert_eq!(out["equippedOutfit"], Value::Null);
        assert!(out.get("courierCheckpoints").is_none());
        let avatars = out["characterAvatars"].as_object().unwrap();
        assert_eq!(avatars.len(), 1);
        assert!(
            !avatars.contains_key("char-a"),
            "re-keyed off the archive id"
        );
    }

    fn parse_list(v: &Value) -> Vec<String> {
        serde_json::from_str(v.as_str().expect("a JSON string")).expect("a JSON list")
    }

    /// `it('re-points the turn rotation, turn queue and whisper audiences at
    /// the renamed seats')`
    #[test]
    fn re_points_the_turn_rotation_turn_queue_and_whisper_audiences_at_the_renamed_seats() {
        let chat = json!({
            "id": "chat-old",
            "participants": [{ "id": "seat-a" }, { "id": "seat-b" }],
            "turnQueue": "[\"seat-b\"]",
            "spokenThisCycleParticipantIds": "[\"seat-a\"]",
            "cycleOrderParticipantIds": "[\"seat-b\",\"seat-a\"]",
            "messages": [{ "id": "msg-old", "participantId": "seat-a", "targetParticipantIds": ["seat-b"] }],
        });
        let out = remap_chat(
            &mut counting(),
            &chat,
            "target-user",
            &HashMap::new(),
            &HashSet::new(),
        );
        let seat_a = out["participants"][0]["id"].as_str().unwrap().to_string();
        let seat_b = out["participants"][1]["id"].as_str().unwrap().to_string();

        assert_ne!(seat_a, "seat-a");
        assert_eq!(parse_list(&out["turnQueue"]), vec![seat_b.clone()]);
        assert_eq!(
            parse_list(&out["spokenThisCycleParticipantIds"]),
            vec![seat_a.clone()]
        );
        assert_eq!(
            parse_list(&out["cycleOrderParticipantIds"]),
            vec![seat_b.clone(), seat_a]
        );
        assert_eq!(
            out["messages"][0]["targetParticipantIds"],
            json!([seat_b.clone()])
        );
        // The mint order: the turn queue names seat-b before the participants
        // are reached, so seat-b's counter is the LOWER one.
        assert!(seat_b.as_str() < out["participants"][0]["id"].as_str().unwrap());
    }

    /// `it('leaves a malformed or absent turn-rotation value as it found it')`
    #[test]
    fn leaves_a_malformed_or_absent_turn_rotation_value_as_it_found_it() {
        let chat = json!({
            "id": "chat-old",
            "participants": [],
            "messages": [],
            "cycleOrderParticipantIds": "not json",
        });
        let out = remap_chat(
            &mut counting(),
            &chat,
            "target-user",
            &HashMap::new(),
            &HashSet::new(),
        );
        assert_eq!(out["cycleOrderParticipantIds"], json!("not json"));
        assert!(out.get("turnQueue").is_none());
    }

    /// A valid list comes back COMPACT (`JSON.stringify`), whitespace dropped,
    /// with the key in its original position.
    #[test]
    fn a_pretty_list_is_re_serialised_compact_in_place() {
        let chat = json!({
            "id": "c",
            "turnQueue": "[ \"x\" , \"y\" ]",
            "title": "after",
            "participants": [],
            "messages": [],
        });
        let out = remap_chat(
            &mut counting(),
            &chat,
            "u",
            &HashMap::new(),
            &HashSet::new(),
        );
        assert_eq!(
            out["turnQueue"],
            json!("[\"00000000-0000-4000-8000-000000000002\",\"00000000-0000-4000-8000-000000000003\"]")
        );
        let keys: Vec<&str> = out
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys[..3], ["id", "turnQueue", "title"]);
    }

    /// A present `null` is visited (key presence, not truthiness) and left
    /// `null`; so are a number and a REAL array (not an array field here).
    #[test]
    fn a_present_non_string_value_is_left_as_it_came() {
        for value in [json!(null), json!(5), json!(["seat-r"])] {
            let chat = json!({ "id": "c", "turnQueue": value, "participants": [], "messages": [] });
            let out = remap_chat(
                &mut counting(),
                &chat,
                "u",
                &HashMap::new(),
                &HashSet::new(),
            );
            assert_eq!(out["turnQueue"], value);
        }
    }
}

// === P4.D327: the Inventory arms, unit-tested beside the port ===
#[cfg(test)]
mod inventory_tests {
    use super::*;
    use serde_json::json;

    fn counting() -> UuidRemapper {
        let mut n = 0u32;
        UuidRemapper::with_id_source(Box::new(move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }))
    }

    /// A backup carrying only `files` and object documents (`(mount, path,
    /// content)` per document).
    fn backup(files: Vec<Value>, docs: &[(&str, &str, Value)]) -> BackupData {
        BackupData {
            files,
            doc_mount_file_links: docs
                .iter()
                .enumerate()
                .map(|(i, (mount, path, _))| {
                    json!({ "id": format!("link-{i}"), "mountPointId": mount, "fileId": format!("df-{i}"), "relativePath": path })
                })
                .collect(),
            doc_mount_documents: docs
                .iter()
                .enumerate()
                .map(|(i, (_, _, content))| json!({ "id": format!("doc-{i}"), "fileId": format!("df-{i}"), "content": content }))
                .collect(),
            doc_mount_points: Vec::new(),
            characters: Vec::new(),
            chats: Vec::new(),
            tags: Vec::new(),
            connection_profiles: Vec::new(),
            image_profiles: Vec::new(),
            embedding_profiles: Vec::new(),
            memories: Vec::new(),
            prompt_templates: Vec::new(),
            roleplay_templates: Vec::new(),
            provider_models: Vec::new(),
            projects: Vec::new(),
            groups: Vec::new(),
            llm_logs: Vec::new(),
            plugin_configs: Vec::new(),
            chat_settings: Vec::new(),
            folders: Vec::new(),
            wardrobe_items: Vec::new(),
            character_plugin_data: Vec::new(),
            conversation_annotations: Vec::new(),
            chat_documents: Vec::new(),
            chat_informs: Vec::new(),
            instance_settings: Vec::new(),
            embedding_status: Vec::new(),
            conversation_chunks: Vec::new(),
            tfidf_vocabularies: Vec::new(),
            vector_entries: Vec::new(),
            doc_mount_folders: Vec::new(),
            doc_mount_files: Vec::new(),
            doc_mount_chunks: Vec::new(),
            doc_mount_blobs: Vec::new(),
            project_doc_mount_links: Vec::new(),
            group_doc_mount_links: Vec::new(),
            group_character_members: Vec::new(),
            text_replacement_rules: Vec::new(),
            wardrobe_wear: Vec::new(),
            wardrobe_history: Vec::new(),
        }
    }

    fn object(id: &str, image: Option<&str>) -> Value {
        let mut o = json!({ "id": id, "name": "thing" });
        if let Some(image) = image {
            o["imageFileId"] = json!(image);
        }
        Value::String(o.to_string())
    }

    #[test]
    fn remap_link_id_keeps_an_inventory_id_first_then_the_item_map_then_mints() {
        let mut r = counting();
        let inventory: HashSet<String> = ["obj".to_string(), "both".to_string()]
            .into_iter()
            .collect();
        let items: HashMap<String, String> = [
            ("item".to_string(), "item-new".to_string()),
            ("both".to_string(), "item-wins?".to_string()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            remap_link_id(&mut r, &inventory, &items, &json!("obj")),
            "obj"
        );
        assert_eq!(
            remap_link_id(&mut r, &inventory, &items, &json!("both")),
            "both"
        );
        assert_eq!(
            remap_link_id(&mut r, &inventory, &items, &json!("item")),
            "item-new"
        );
        assert_eq!(
            remap_link_id(&mut r, &inventory, &items, &json!("chat")),
            "00000000-0000-4000-8000-000000000001"
        );
        // A non-string element never matches the set — the remapper mints.
        assert_eq!(
            remap_link_id(&mut r, &inventory, &items, &json!(7.5)),
            "00000000-0000-4000-8000-000000000002"
        );
    }

    #[test]
    fn inventory_documents_walks_the_predicate_edges() {
        let data = backup(
            Vec::new(),
            &[
                ("m", "Inventory/images/x/pic.json", object("a", None)),
                ("m", "Inventory/notes.md", object("b", None)),
                ("m", "inventory/lower.json", object("c", None)),
                (
                    "m",
                    "Inventory/a/b/c/d/e/f/g/h/i/deep.json",
                    object("d", None),
                ),
                ("m", "Inventory/array.json", json!("[{\"id\":\"e\"}]")),
                ("m", "Inventory/null.json", json!("null")),
                ("m", "Inventory/empty.json", object("", None)),
                ("m", "Inventory/number.json", json!("{\"id\":5}")),
                ("m", "Inventory/broken.json", json!("{\"id\":")),
                ("m", "Inventory/scalar.json", json!(42)),
                ("m", "Inventory/obj-1.json", object("obj-1", None)),
            ],
        );
        let ids: Vec<String> = inventory_documents(&data)
            .into_iter()
            .map(|(_, _, id)| id)
            .collect();
        assert_eq!(ids, ["c", "d", "obj-1"]);
    }

    #[test]
    fn the_plan_mints_each_mount_then_its_picture_in_link_order() {
        let data = backup(
            vec![json!({ "id": "pic-a" }), json!({ "id": "pic-b" })],
            &[
                (
                    "mount-A",
                    "Inventory/one.json",
                    object("one", Some("pic-a")),
                ),
                ("mount-A", "Inventory/bad.json", object("bad", None)),
                (
                    "mount-A",
                    "Inventory/missing.json",
                    object("missing", Some("pic-zzz")),
                ),
                (
                    "mount-B",
                    "Inventory/two.json",
                    object("two", Some("pic-b")),
                ),
            ],
        );
        let mut r = counting();
        let fixes = plan_inventory_image_pointer_fixes(&data, &mut r);
        let got: Vec<(&str, &str, &str, &str)> = fixes
            .iter()
            .map(|f| {
                (
                    f.mount_point_id.as_str(),
                    f.source_mount_point_id.as_str(),
                    f.item_id.as_str(),
                    f.image_file_id.as_str(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                (
                    "00000000-0000-4000-8000-000000000001",
                    "mount-A",
                    "one",
                    "00000000-0000-4000-8000-000000000002"
                ),
                (
                    "00000000-0000-4000-8000-000000000003",
                    "mount-B",
                    "two",
                    "00000000-0000-4000-8000-000000000004"
                ),
            ]
        );
        assert_eq!(
            serde_json::to_string(&fixes[0]).unwrap(),
            r#"{"mountPointId":"00000000-0000-4000-8000-000000000001","sourceMountPointId":"mount-A","itemId":"one","imageFileId":"00000000-0000-4000-8000-000000000002"}"#
        );
    }
}
// === end P4.D327 ===
