//! Image-profile resolution + the story-background enqueue gate.
//!
//!   - [`resolve_image_profile_for_chat`] — v4 `lib/image-gen/profile-resolution.ts`
//!     `resolveImageProfileForChat`: the four-tier chain (chat → story-backgrounds
//!     default → project default → user default), each candidate ownership- and
//!     `apiKeyId`-checked.
//!   - [`queue_story_background_if_enabled`] — v4
//!     `lib/background-jobs/handlers/title-update.ts` `queueStoryBackgroundIfEnabled`:
//!     the `storyBackgroundsSettings.enabled` gate + autonomous-chat skip +
//!     profile resolution + participant characterIds + the enqueue.
//!
//! **Wiring point (tracked):** v4 calls `queueStoryBackgroundIfEnabled` INSIDE the
//! `TITLE_UPDATE` job handler, after a successful (non-help-chat) rename. v5 has
//! no `TITLE_UPDATE` handler registered yet; when it lands it must invoke
//! [`queue_story_background_if_enabled`] with the resolved chat, chat settings,
//! and new title. This module ports the gate so the handler wiring is a one-liner.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::runtime::Db;

/// v4 `resolveImageProfileForChat`: the four-tier image-profile resolution. Sync —
/// the caller supplies the main connection and the (optional) mount-index
/// connection (the project tier reads the store overlay; skipped when absent).
pub fn resolve_image_profile_for_chat(
    main: &Connection,
    mount: Option<&Connection>,
    user_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
) -> Option<String> {
    // A candidate profile is usable iff it's owned by the user and has an API key.
    let usable = |profile: &Value| -> Option<String> {
        let owned = profile.get("userId").and_then(Value::as_str) == Some(user_id);
        let has_key = profile
            .get("apiKeyId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .is_some();
        if owned && has_key {
            profile
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        }
    };
    let resolve_by_id = |id: &str| -> Option<String> {
        crate::db::image_profiles::find_by_id(main, id)
            .ok()
            .flatten()
            .and_then(|p| usable(&p))
    };

    // 1. The chat's image profile (most specific).
    if let Some(id) = chat
        .get("imageProfileId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        if let Some(hit) = resolve_by_id(id) {
            return Some(hit);
        }
    }

    // 2. The story-backgrounds default profile.
    if let Some(id) = chat_settings
        .and_then(|cs| cs.get("storyBackgroundsSettings"))
        .and_then(|s| s.get("defaultImageProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        if let Some(hit) = resolve_by_id(id) {
            return Some(hit);
        }
    }

    // 3. The project's default image profile (needs the store overlay).
    if let (Some(mount), Some(project_id)) = (
        mount,
        chat.get("projectId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty()),
    ) {
        let repo = crate::db::projects::ProjectsRepository::new(main, mount);
        if let Ok(Some(project)) = repo.find_by_id(project_id) {
            if let Some(id) = project
                .get("defaultImageProfileId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                if let Some(hit) = resolve_by_id(id) {
                    return Some(hit);
                }
            }
        }
    }

    // 4. The user's default image profile (`apiKeyId` only — findDefault scopes
    //    by user already).
    if let Ok(all) = crate::db::image_profiles::find_all(main) {
        if let Some(def) = all.iter().find(|p| {
            p.get("isDefault").and_then(Value::as_bool) == Some(true)
                && p.get("userId").and_then(Value::as_str) == Some(user_id)
        }) {
            if def
                .get("apiKeyId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .is_some()
            {
                return def.get("id").and_then(Value::as_str).map(str::to_string);
            }
        }
    }

    None
}

/// v4 `queueStoryBackgroundIfEnabled`: enqueue a story-background job only when
/// story backgrounds are enabled and the chat is not an autonomous room, an image
/// profile resolves, and the chat has participants. Errors are swallowed
/// (automatic path). `chat` is the chats row, `chat_settings` the user's settings.
pub async fn queue_story_background_if_enabled(
    db: &Db,
    user_id: &str,
    chat: &Value,
    chat_settings: Option<&Value>,
    new_title: &str,
) {
    // 1. `storyBackgroundsSettings?.enabled` gate.
    let enabled = chat_settings
        .and_then(|cs| cs.get("storyBackgroundsSettings"))
        .and_then(|s| s.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true);
    if !enabled {
        return;
    }
    // 2. Autonomous-chat skip.
    if chat.get("chatType").and_then(Value::as_str) == Some("autonomous") {
        return;
    }
    // 3. Resolve the image profile (dual-connection read).
    let chat_owned = chat.clone();
    let settings_owned = chat_settings.cloned();
    let user_owned = user_id.to_string();
    let image_profile_id = db
        .write(move |writers| {
            let main = writers.main().connection();
            let mount = writers.mount_index().map(|w| w.connection());
            Ok(resolve_image_profile_for_chat(
                main,
                mount,
                &user_owned,
                &chat_owned,
                settings_owned.as_ref(),
            ))
        })
        .await
        .ok()
        .flatten();
    let Some(image_profile_id) = image_profile_id else {
        return;
    };
    // 4. Participant characterIds.
    let character_ids: Vec<String> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("characterId").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if character_ids.is_empty() {
        return;
    }

    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);
    // Errors swallowed — the automatic path must never affect the caller.
    let _ = crate::services::queue_service::enqueue_story_background_generation(
        db,
        user_id,
        chat_id,
        &image_profile_id,
        &character_ids,
        Some(new_title),
        project_id,
    )
    .await;
}
