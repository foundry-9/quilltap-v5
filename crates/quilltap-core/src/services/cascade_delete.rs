//! v4 `lib/cascade-delete.ts` — the character cascade-delete preview + executor.
//!
//! `get_cascade_delete_preview` composes three finders over the RAW (non-overlaid)
//! character (`find_by_id_raw` — a broken vault must stay deletable), and
//! `execute_cascade_delete` runs the destructive fan-out.
//!
//! **Corpus coverage:** the differential targets Aria, whose avatar is a
//! VAULT-LINK and whose one exclusive chat carries no message attachments. So the
//! exercised paths are: the exclusive-chat participant filter, the vault-link
//! exclusive-image branch, the memory count/unlink-batch, and the destructive
//! chat/gallery/memory/vector/plugin/character deletes. The legacy-`files`
//! exclusive-image branch (`find_exclusive_images_for_character`'s legacy path)
//! and `find_exclusive_images_for_chats` are ported faithfully but NOT
//! differential-exercised (flagged). `delete_file_completely`'s host-storage byte
//! delete (v4 `fileStorageManager.deleteFile`) is a host seam — the core removes
//! the `files` metadata row; the host wires the byte reclaim.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::db::characters::CharactersRepository;
use crate::db::chats::ChatsRepository;
use crate::db::files::FilesRepository;
use crate::db::vector_indices::VectorIndicesRepository;
use crate::db::{characters_read, chats_read, memories_read, DbError};
use crate::photos::character_gallery_service::remove_from_character_gallery;
use crate::photos::resolve_character_avatar::{resolve_character_avatar, AvatarKind};

/// One exclusive-image entry (v4 `ExclusiveCharacterImage`).
#[derive(Debug, Clone)]
pub enum ExclusiveCharacterImage {
    /// A legacy `files`-table avatar (pre-Phase-3 / just-imported).
    LegacyFile { id: String },
    /// A vault `doc_mount_file_links` avatar — exclusive by construction.
    VaultLink { id: String, link_id: String },
}

/// The cascade preview (v4 `CascadeDeletePreview`). `exclusive_chats` carries each
/// chat `Value` + its `messageCount` for the handler's projection; the two image
/// lists are counted by the handler.
pub struct CascadeDeletePreview {
    pub character_name: String,
    pub exclusive_chats: Vec<(Value, i64)>,
    pub exclusive_character_images: Vec<ExclusiveCharacterImage>,
    pub exclusive_chat_images: Vec<String>,
    pub memory_count: i64,
}

/// v4 `findExclusiveChatsForCharacter` — chats whose only AI-controlled CHARACTER
/// participant is this character (user-controlled participants are ignored).
/// Returns `(chat, messageCount)` pairs.
pub fn find_exclusive_chats_for_character(
    main: &Connection,
    character_id: &str,
) -> Result<Vec<(Value, i64)>, DbError> {
    let chats = chats_read::find_by_character_id(main, character_id)?;
    let mut out = Vec::new();
    for chat in chats {
        let ai_chars: Vec<&Value> = chat
            .get("participants")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|p| {
                        p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                            && p.get("controlledBy").and_then(Value::as_str) != Some("user")
                    })
                    .collect()
            })
            .unwrap_or_default();
        if ai_chars.len() == 1
            && ai_chars[0].get("characterId").and_then(Value::as_str) == Some(character_id)
        {
            let message_count = chat
                .get("messageCount")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            out.push((chat, message_count));
        }
    }
    Ok(out)
}

/// Read a legacy `files` row's `linkedTo` array (not exposed on [`FileEntry`]).
fn file_linked_to(main: &Connection, file_id: &str) -> Result<Vec<String>, DbError> {
    let raw: Option<String> = main
        .query_row(
            "SELECT linkedTo FROM files WHERE id = ?1",
            params![file_id],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(raw
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default())
}

/// v4 `findExclusiveImagesForCharacter` — the character's `defaultImageId` +
/// `avatarOverrides` candidate ids, resolved: vault-links are exclusive by
/// construction; legacy files pass the historic `linkedTo` + not-used-elsewhere
/// exclusivity check. Reads the RAW character row (broken-vault-safe).
pub fn find_exclusive_images_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Vec<ExclusiveCharacterImage>, DbError> {
    let Some(character) = characters_read::find_by_id_raw(main, character_id)? else {
        return Ok(vec![]);
    };

    let mut candidate_ids: Vec<String> = Vec::new();
    if let Some(did) = character.get("defaultImageId").and_then(Value::as_str) {
        candidate_ids.push(did.to_string());
    }
    if let Some(overrides) = character.get("avatarOverrides").and_then(Value::as_array) {
        for o in overrides {
            if let Some(iid) = o.get("imageId").and_then(Value::as_str) {
                candidate_ids.push(iid.to_string());
            }
        }
    }

    let files = FilesRepository::new(main);
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Dedup preserving first-seen order (v4 iterates a Set).
    let mut ordered = Vec::new();
    let mut in_set = std::collections::HashSet::new();
    for id in candidate_ids {
        if in_set.insert(id.clone()) {
            ordered.push(id);
        }
    }

    for id in ordered {
        if seen.contains(&id) {
            continue;
        }
        let Some(resolved) = resolve_character_avatar(main, mount, Some(&id))? else {
            continue;
        };
        if resolved.kind == AvatarKind::VaultLink {
            seen.insert(id.clone());
            out.push(ExclusiveCharacterImage::VaultLink {
                id: id.clone(),
                link_id: id,
            });
            continue;
        }
        // Legacy files-table avatar (NOT exercised by the vault-link corpus).
        let Some(image) = files.find_by_id(&id)? else {
            continue;
        };
        let linked_to = file_linked_to(main, &image.id)?;
        let is_exclusive =
            linked_to.is_empty() || (linked_to.len() == 1 && linked_to[0] == character_id);
        if !is_exclusive {
            continue;
        }
        let chars_default = characters_read::find_by_default_image_id(main, mount, &image.id)?;
        let chars_overrides =
            characters_read::find_by_avatar_override_image_id(main, mount, &image.id)?;
        let used_elsewhere = chars_default
            .iter()
            .any(|c| c.get("id").and_then(Value::as_str) != Some(character_id))
            || chars_overrides
                .iter()
                .any(|c| c.get("id").and_then(Value::as_str) != Some(character_id));
        if used_elsewhere {
            continue;
        }
        seen.insert(id.clone());
        out.push(ExclusiveCharacterImage::LegacyFile { id: image.id });
    }
    Ok(out)
}

/// v4 `findExclusiveImagesForChats` — image ids attached to messages in the given
/// chats that aren't linked elsewhere or used by any character. NOT exercised by
/// the corpus (the exclusive chat carries no attachments) — ported faithfully.
pub fn find_exclusive_images_for_chats(
    main: &Connection,
    mount: &Connection,
    chat_ids: &[String],
) -> Result<Vec<String>, DbError> {
    if chat_ids.is_empty() {
        return Ok(vec![]);
    }
    let chat_id_set: std::collections::HashSet<&str> =
        chat_ids.iter().map(String::as_str).collect();
    let mut image_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for chat_id in chat_ids {
        let messages = crate::db::chats_messages_read::get_messages(main, chat_id)?;
        for m in messages {
            if m.get("type").and_then(Value::as_str) != Some("message") {
                continue;
            }
            if let Some(atts) = m.get("attachments").and_then(Value::as_array) {
                for a in atts {
                    if let Some(aid) = a.as_str() {
                        if seen.insert(aid.to_string()) {
                            image_ids.push(aid.to_string());
                        }
                    }
                }
            }
        }
    }
    if image_ids.is_empty() {
        return Ok(vec![]);
    }
    let files = FilesRepository::new(main);
    let mut out = Vec::new();
    for id in image_ids {
        let Some(image) = files.find_by_id(&id)? else {
            continue;
        };
        let linked_to = file_linked_to(main, &image.id)?;
        let linked_to_others = linked_to.iter().any(|e| !chat_id_set.contains(e.as_str()));
        if linked_to_others {
            continue;
        }
        let chars_default = characters_read::find_by_default_image_id(main, mount, &image.id)?;
        let chars_overrides =
            characters_read::find_by_avatar_override_image_id(main, mount, &image.id)?;
        if chars_default.is_empty() && chars_overrides.is_empty() {
            out.push(image.id);
        }
    }
    Ok(out)
}

/// v4 `getCascadeDeletePreview`. `None` when the character row is absent.
pub fn get_cascade_delete_preview(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Option<CascadeDeletePreview>, DbError> {
    let Some(character) = characters_read::find_by_id_raw(main, character_id)? else {
        return Ok(None);
    };
    let character_name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let exclusive_chats = find_exclusive_chats_for_character(main, character_id)?;
    let exclusive_character_images =
        find_exclusive_images_for_character(main, mount, character_id)?;
    let chat_ids: Vec<String> = exclusive_chats
        .iter()
        .filter_map(|(c, _)| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let exclusive_chat_images = find_exclusive_images_for_chats(main, mount, &chat_ids)?;
    let memory_count = memories_read::count_by_character_id(main, character_id)?;

    Ok(Some(CascadeDeletePreview {
        character_name,
        exclusive_chats,
        exclusive_character_images,
        exclusive_chat_images,
        memory_count,
    }))
}

/// v4 `deleteFileCompletely` — the GC-safe file delete chokepoint. The host-side
/// byte reclaim (`fileStorageManager.deleteFile`) is a seam (unwired in the core);
/// here we remove the `files` metadata row. Returns whether a row was deleted.
fn delete_file_completely(main: &Connection, file_id: &str) -> Result<bool, DbError> {
    let files = FilesRepository::new(main);
    if files.find_by_id(file_id)?.is_none() {
        return Ok(false);
    }
    // Host byte reclaim is wired by the host driver; the core deletes metadata.
    files.delete(file_id)
}

/// v4 `executeCascadeDelete`. Deletes the character + its memories, and — when
/// requested — its exclusive chats + images. Returns
/// `(success, deletedChats, deletedImages, deletedMemories)`. A WRITE.
pub fn execute_cascade_delete(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    delete_exclusive_chats: bool,
    delete_exclusive_images: bool,
) -> Result<(bool, i64, i64, i64), DbError> {
    if characters_read::find_by_id_raw(main, character_id)?.is_none() {
        return Ok((false, 0, 0, 0));
    }
    let Some(preview) = get_cascade_delete_preview(main, mount, character_id)? else {
        return Ok((false, 0, 0, 0));
    };

    let mut deleted_chats = 0i64;
    let mut deleted_images = 0i64;
    let mut deleted_memories = 0i64;

    let chats_repo = ChatsRepository::new(main);
    if delete_exclusive_chats {
        if delete_exclusive_images {
            for id in &preview.exclusive_chat_images {
                if delete_file_completely(main, id)? {
                    deleted_images += 1;
                }
            }
        }
        for (chat, _) in &preview.exclusive_chats {
            if let Some(chat_id) = chat.get("id").and_then(Value::as_str) {
                if chats_repo.delete(chat_id)? {
                    deleted_chats += 1;
                }
            }
        }
    }

    if delete_exclusive_images {
        for image in &preview.exclusive_character_images {
            match image {
                ExclusiveCharacterImage::VaultLink { link_id, .. } => {
                    let (deleted, _gc) =
                        remove_from_character_gallery(main, mount, character_id, link_id)
                            .map_err(gallery_db_err)?;
                    if deleted {
                        deleted_images += 1;
                    }
                }
                ExclusiveCharacterImage::LegacyFile { id } => {
                    if delete_file_completely(main, id)? {
                        deleted_images += 1;
                    }
                }
            }
        }
    }

    // Always scrub memories through the unlink-batch chokepoint (cross-character
    // relatedMemoryIds get rewritten before the rows go away).
    if preview.memory_count > 0 {
        let memories = memories_read::find_by_character_id(main, character_id)?;
        let ids: Vec<String> = memories
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
            .collect();
        deleted_memories =
            crate::db::memories::MemoriesRepository::new(main).delete_many_with_unlink(&ids)?;
    }

    // Vector index (embeddings).
    VectorIndicesRepository::new(main).delete_by_character_id(character_id)?;

    // Plugin data.
    main.execute(
        "DELETE FROM character_plugin_data WHERE characterId = ?1",
        params![character_id],
    )?;

    // Finally the character (slim row).
    CharactersRepository::new(main).delete(character_id)?;

    Ok((true, deleted_chats, deleted_images, deleted_memories))
}

/// Collapse a [`GalleryError`] from the gallery-removal call into a [`DbError`]
/// (the cascade never surfaces the character-not-found / bad-vault variants — the
/// preview already resolved the character + vault).
fn gallery_db_err(e: crate::photos::character_gallery_service::GalleryError) -> DbError {
    use crate::photos::character_gallery_service::GalleryError;
    match e {
        GalleryError::Db(err) => err,
        GalleryError::CharacterNotFound => DbError::Key("character not found".into()),
        GalleryError::BadRequest(msg) => DbError::Key(msg),
    }
}
