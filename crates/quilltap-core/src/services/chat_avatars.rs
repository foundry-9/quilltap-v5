//! The per-chat **avatar-override** service layer (P4.9E1A) — a differential
//! port of v4's `app/api/v1/chats/[id]/actions/avatars.ts` (`handleGetAvatars`
//! :18, `handleSetAvatar` :69, `handleRemoveAvatar` :130) and
//! `actions/toggle-avatar-generation.ts` (`handleToggleAvatarGeneration` :22).
//!
//! An override is stored on the CHARACTER, not the chat: `characters
//! .avatarOverrides` is an array of `{chatId, imageId}`, so "this chat's
//! avatars" is a scan of every character the user owns. v4 does exactly that
//! (`characters.findByUserId` → `flatMap` → filter by `chatId`), and the port
//! keeps the same traversal so the response array's ORDER matches: characters in
//! repository order, each character's overrides in stored order.
//!
//! Both mutating verbs write through the ported character UPDATE
//! ([`crate::db::vault_character_update::update_character`]); `avatarOverrides`
//! is not a vault-managed field, so it lands on the slim row.
//!
//! ## `toggle-avatar-generation` and the chat-column write
//!
//! v4 flips `chats.avatarGenerationEnabled` through `repos.chats.update`. v5's
//! [`ChatUpdate`](crate::db::chats::ChatUpdate) has no such field and
//! `db/chats.rs` belongs to a sibling lane this round, so the flip is a raw
//! single-column `UPDATE` — the `[[standalone-write-avoids-frozen-chatupdate]]`
//! pattern the order blesses for exactly this case. It is byte-identical to v4's
//! write: one column, no `updatedAt` mint (v4's `_update` preserves it).
//!
//! Toggling ON then enqueues one `CHARACTER_AVATAR_GENERATION` job per
//! LLM-controlled character participant, resolving the image profile
//! chat-level-first then the marked default. Every part of that is best-effort:
//! v4 wraps the whole block in a try/catch and each enqueue in its own, so a
//! failure never fails the toggle.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, image_profiles, vault_character_update, DbError};
use crate::photos::photo_link_summary::get_photo_link_summary_by_sha256;
use crate::photos::resolve_character_avatar::resolve_character_avatar;
use crate::services::queue_service::enqueue_character_avatar_generation;

/// v4's `{ error, status }` failure arm for this family.
#[derive(Debug, Clone)]
pub struct AvatarError {
    pub status: u16,
    pub message: String,
}

impl AvatarError {
    fn not_found(resource: &str) -> Self {
        Self {
            status: 404,
            message: format!("{resource} not found"),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
        }
    }
}

impl From<DbError> for AvatarError {
    fn from(e: DbError) -> Self {
        AvatarError::internal(e.to_string())
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn read_main_mount<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError>,
{
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// The `{chatId, characterId, imageId, character, image}` element v4 builds for
/// both `get-avatars` and `set-avatar`. `image` is `null` when the id resolves to
/// nothing (only reachable on the GET — `set-avatar` 404s first).
fn build_override_entry(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    character_name: &str,
    image_id: &str,
) -> Result<Value, DbError> {
    let resolved = resolve_character_avatar(main, mount, Some(image_id))?;
    let image = match resolved {
        Some(r) => {
            let link_summary = match r.sha256.as_deref().filter(|s| !s.is_empty()) {
                Some(sha) => get_photo_link_summary_by_sha256(mount, sha)?,
                None => Value::Null,
            };
            json!({
                "id": r.id,
                "filepath": r.url,
                "url": Value::Null,
                "sha256": r.sha256,
                "linkSummary": link_summary,
            })
        }
        None => Value::Null,
    };
    Ok(json!({
        "chatId": chat_id,
        "characterId": character_id,
        "imageId": image_id,
        "character": { "id": character_id, "name": character_name },
        "image": image,
    }))
}

/// v4 `handleGetAvatars` — every avatar override any of the user's characters
/// carries for this chat, enriched with the resolved image + its link summary.
pub fn get_avatars(db: &Db, user_id: &str, chat_id: &str) -> Result<Value, AvatarError> {
    let cid = chat_id.to_string();
    if db
        .read_main(move |c| chats_read::find_by_id(c, &cid))?
        .is_none()
    {
        return Err(AvatarError::not_found("Chat"));
    }
    let uid = user_id.to_string();
    let entries = read_main_mount(db, |main, mount| {
        let characters = characters_read::find_by_user_id(main, mount, &uid)?;
        let mut out = Vec::new();
        for character in &characters {
            let character_id = s(character, "id").unwrap_or_default();
            let character_name = s(character, "name").unwrap_or_default();
            let Some(overrides) = character.get("avatarOverrides").and_then(Value::as_array) else {
                continue;
            };
            for o in overrides {
                if s(o, "chatId").as_deref() != Some(chat_id) {
                    continue;
                }
                let image_id = s(o, "imageId").unwrap_or_default();
                out.push(build_override_entry(
                    main,
                    mount,
                    chat_id,
                    &character_id,
                    &character_name,
                    &image_id,
                )?);
            }
        }
        Ok(out)
    })
    // v4 wraps the whole handler in a try/catch → serverError.
    .map_err(|_| AvatarError::internal("Failed to fetch avatar overrides"))?;

    Ok(json!({ "data": entries }))
}

/// Write the character's `avatarOverrides` array through the ported character
/// UPDATE (an unmanaged field → the slim row).
async fn write_overrides(
    db: &Db,
    character_id: &str,
    overrides: Vec<Value>,
) -> Result<(), DbError> {
    let cid = character_id.to_string();
    let mut patch = Map::new();
    patch.insert("avatarOverrides".into(), Value::Array(overrides));
    db.write(move |w| {
        let mount =
            w.mount_index()
                .map(|m| m.connection())
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?;
        let main = w.main().connection();
        vault_character_update::update_character(main, mount, &cid, &patch)?;
        Ok(())
    })
    .await
}

/// v4 `handleSetAvatar` — pin an image as this character's avatar in this chat.
/// Replaces any existing override for the chat, else appends.
pub async fn set_avatar(
    db: &Db,
    chat_id: &str,
    character_id: &str,
    image_id: &str,
) -> Result<Value, AvatarError> {
    let cid = character_id.to_string();
    let Some(character) = read_main_mount(db, |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    })?
    else {
        return Err(AvatarError::not_found("Character"));
    };
    let iid = image_id.to_string();
    let resolved = read_main_mount(db, |main, mount| {
        resolve_character_avatar(main, mount, Some(&iid))
    })?;
    let Some(resolved) = resolved else {
        return Err(AvatarError::not_found("Image"));
    };

    let existing: Vec<Value> = character
        .get("avatarOverrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let entry = json!({ "chatId": chat_id, "imageId": image_id });
    let mut updated = existing.clone();
    match existing
        .iter()
        .position(|o| s(o, "chatId").as_deref() == Some(chat_id))
    {
        Some(idx) => updated[idx] = entry,
        None => updated.push(entry),
    }
    write_overrides(db, character_id, updated).await?;

    let link_summary = match resolved.sha256.as_deref().filter(|s| !s.is_empty()) {
        Some(sha) => {
            let sha = sha.to_string();
            db.read_mount_index(move |mount| get_photo_link_summary_by_sha256(mount, &sha))?
        }
        None => Value::Null,
    };
    let data = json!({
        "chatId": chat_id,
        "characterId": character_id,
        "imageId": image_id,
        "character": {
            "id": s(&character, "id").unwrap_or_default(),
            "name": s(&character, "name").unwrap_or_default(),
        },
        "image": {
            "id": resolved.id,
            "filepath": resolved.url,
            "url": Value::Null,
            "sha256": resolved.sha256,
            "linkSummary": link_summary,
        },
    });
    Ok(json!({ "data": data }))
}

/// v4 `handleRemoveAvatar` — drop this chat's override from the character.
pub async fn remove_avatar(
    db: &Db,
    chat_id: &str,
    character_id: &str,
) -> Result<Value, AvatarError> {
    let cid = character_id.to_string();
    let Some(character) = read_main_mount(db, |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    })?
    else {
        return Err(AvatarError::not_found("Character"));
    };
    let updated: Vec<Value> = character
        .get("avatarOverrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|o| s(o, "chatId").as_deref() != Some(chat_id))
        .collect();
    write_overrides(db, character_id, updated).await?;
    Ok(json!({ "data": { "success": true } }))
}

/// v4 `handleToggleAvatarGeneration` — flip `chats.avatarGenerationEnabled` and,
/// when turning ON, enqueue an avatar-generation job for every LLM-controlled
/// character participant.
pub async fn toggle_avatar_generation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
) -> Result<Value, AvatarError> {
    let cid = chat_id.to_string();
    let Some(chat) = db.read_main(move |c| chats_read::find_by_id(c, &cid))? else {
        // v4 answers serverError here — the POST dispatcher already 404'd a
        // missing chat, so this arm is only reachable on a race.
        return Err(AvatarError::internal("Chat not found"));
    };
    // `!chat.avatarGenerationEnabled` — null/false/absent → true.
    let new_value = !chat
        .get("avatarGenerationEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let cid = chat_id.to_string();
    let updated = db
        .write(move |w| {
            let n = w.main().connection().execute(
                "UPDATE chats SET avatarGenerationEnabled = ?1 WHERE id = ?2",
                rusqlite::params![new_value as i64, cid],
            )?;
            Ok(n > 0)
        })
        .await?;
    if !updated {
        return Err(AvatarError::internal("Failed to update chat"));
    }

    if new_value {
        // Best-effort from here down (v4's outer try/catch): resolve the image
        // profile chat-level-first, then the marked default; silently do nothing
        // when neither exists.
        let cid = chat_id.to_string();
        let chat_profile = chat
            .get("imageProfileId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let resolved_profile = db.read_main(move |c| {
            if let Some(pid) = chat_profile {
                if let Some(p) = image_profiles::find_by_id(c, &pid)? {
                    return Ok(s(&p, "id"));
                }
            }
            Ok(image_profiles::find_all(c)?
                .into_iter()
                .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true))
                .and_then(|p| s(&p, "id")))
        });
        if let Ok(Some(image_profile_id)) = resolved_profile {
            let chat_after = db
                .read_main(move |c| chats_read::find_by_id(c, &cid))
                .ok()
                .flatten();
            let participants = chat_after
                .as_ref()
                .and_then(|c| c.get("participants"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for p in &participants {
                if s(p, "type").as_deref() != Some("CHARACTER") {
                    continue;
                }
                let Some(character_id) = s(p, "characterId").filter(|v| !v.is_empty()) else {
                    continue;
                };
                if s(p, "controlledBy").as_deref() == Some("user") {
                    continue;
                }
                // v4 wraps each enqueue in its own try/catch and only warns.
                let _ = enqueue_character_avatar_generation(
                    db,
                    user_id,
                    chat_id,
                    &character_id,
                    &image_profile_id,
                    None,
                )
                .await;
            }
        }
    }

    Ok(json!({ "avatarGenerationEnabled": new_value }))
}
