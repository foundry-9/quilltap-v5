//! Shared avatar-generation trigger for wardrobe changes (v4
//! `lib/wardrobe/avatar-generation.ts`). Checks whether a chat has avatar
//! generation enabled, resolves the appropriate image profile, and enqueues a
//! `CHARACTER_AVATAR_GENERATION` job. Failures are caught and logged — they must
//! never affect the caller's result.
//!
//! This closes the W4.1d2 wardrobe deferral (the wardrobe handlers' equip path
//! calls [`trigger_avatar_generation_if_enabled`]; that corpus kept
//! `avatarGenerationEnabled` false so the trigger was a verified no-op — now the
//! trigger is real and can be banked firing).
//!
//! The avatar JOB HANDLER itself (v4 `character-avatar-generation-handler.ts`)
//! and `STORY_BACKGROUND` are the tracked follow-up **W4.9c** — they reuse this
//! subsystem + the scene tasks' remaining two functions.

use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::DbError;

/// v4 `AvatarGenerationParams` — the inputs the trigger needs. The
/// `equipped_slots_override` is a one-shot `EquippedSlots` map (`{ top, bottom,
/// footwear, accessories, hair }`, forwarded into the job payload verbatim when
/// set).
#[derive(Clone, Debug)]
pub struct AvatarGenerationParams {
    pub user_id: String,
    pub chat_id: String,
    pub character_id: String,
    /// One-shot override: use this profile instead of the chat's default. The
    /// chat's stored `imageProfileId` is NOT mutated.
    pub image_profile_id_override: Option<String>,
    /// One-shot equipped-slots override (a JSON `{ top, bottom, footwear,
    /// accessories, hair }` object) forwarded into the job payload.
    pub equipped_slots_override: Option<Value>,
}

/// v4 `AvatarGenerationResult` — a structured result so callers can surface a
/// failure to the user (the manual regenerate button consumes it).
#[derive(Clone, Debug, PartialEq)]
pub enum AvatarGenerationResult {
    Queued,
    NotQueued {
        /// `"chat-not-found" | "no-image-profile" | "error"`.
        reason: String,
        message: String,
    },
}

/// v4 `triggerAvatarGeneration`: UNCONDITIONALLY trigger avatar generation.
/// Resolves the image profile from the override first, then the chat-level
/// setting, then the global default. Used by the manual regenerate-avatar button
/// — the chat-level toggle does NOT gate this path.
pub async fn trigger_avatar_generation(
    db: &Db,
    params: &AvatarGenerationParams,
) -> AvatarGenerationResult {
    match trigger_avatar_generation_inner(db, params).await {
        Ok(result) => result,
        // v4's catch → structured error result with the message.
        Err(e) => AvatarGenerationResult::NotQueued {
            reason: "error".to_string(),
            message: e.to_string(),
        },
    }
}

async fn trigger_avatar_generation_inner(
    db: &Db,
    params: &AvatarGenerationParams,
) -> Result<AvatarGenerationResult, DbError> {
    let chat_id = params.chat_id.clone();
    let chat = db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id))?;
    let Some(chat) = chat else {
        return Ok(AvatarGenerationResult::NotQueued {
            reason: "chat-not-found".to_string(),
            message: "Chat not found.".to_string(),
        });
    };

    // Resolve image profile: explicit override → chat-level → global default.
    let mut image_profile_id: Option<String> = None;

    if let Some(over) = params
        .image_profile_id_override
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        let over = over.to_string();
        let profile =
            db.read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &over))?;
        if let Some(p) = profile {
            image_profile_id = p.get("id").and_then(Value::as_str).map(str::to_string);
        }
        // else: v4 warns + falls back.
    }

    if image_profile_id.is_none() {
        if let Some(chat_profile) = chat.get("imageProfileId").and_then(Value::as_str) {
            let chat_profile = chat_profile.to_string();
            let profile = db.read_main(move |conn| {
                crate::db::image_profiles::find_by_id(conn, &chat_profile)
            })?;
            if let Some(p) = profile {
                image_profile_id = p.get("id").and_then(Value::as_str).map(str::to_string);
            }
        }
    }

    if image_profile_id.is_none() {
        let all = db.read_main(crate::db::image_profiles::find_all)?;
        let default = all
            .into_iter()
            .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true));
        if let Some(p) = default {
            image_profile_id = p.get("id").and_then(Value::as_str).map(str::to_string);
        }
    }

    let Some(image_profile_id) = image_profile_id else {
        return Ok(AvatarGenerationResult::NotQueued {
            reason: "no-image-profile".to_string(),
            message: "No image profile is configured. Set one in Settings → Images before generating avatars.".to_string(),
        });
    };

    crate::services::queue_service::enqueue_character_avatar_generation(
        db,
        &params.user_id,
        &params.chat_id,
        &params.character_id,
        &image_profile_id,
        params.equipped_slots_override.clone(),
    )
    .await?;

    Ok(AvatarGenerationResult::Queued)
}

/// v4 `triggerAvatarGenerationIfEnabled`: trigger avatar generation ONLY when the
/// chat has `avatarGenerationEnabled` and is NOT an autonomous room. Used by
/// automatic triggers (wardrobe changes). Failures are swallowed — automatic
/// paths must never affect the caller's result.
pub async fn trigger_avatar_generation_if_enabled(db: &Db, params: &AvatarGenerationParams) {
    let chat_id = params.chat_id.clone();
    let chat = match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id)) {
        Ok(chat) => chat,
        Err(_) => return, // v4's catch swallows.
    };
    let Some(chat) = chat else {
        return;
    };
    // `!chat?.avatarGenerationEnabled` — absent / false / null → skip.
    if chat.get("avatarGenerationEnabled").and_then(Value::as_bool) != Some(true) {
        return;
    }
    // Autonomous rooms: automatic avatar refresh is disabled by design.
    if chat.get("chatType").and_then(Value::as_str) == Some("autonomous") {
        return;
    }
    // Swallow the result (automatic path).
    let _ = trigger_avatar_generation(db, params).await;
}
