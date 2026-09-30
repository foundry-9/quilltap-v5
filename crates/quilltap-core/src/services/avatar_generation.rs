//! Shared avatar-generation trigger for wardrobe changes (v4
//! `lib/wardrobe/avatar-generation.ts`). Checks whether a chat has avatar
//! generation enabled, resolves the appropriate image profile, and enqueues a
//! `CHARACTER_AVATAR_GENERATION` job. Failures are caught and logged — they must
//! never affect the caller's result.
//!
//! Callers of [`trigger_avatar_generation_if_enabled`]: the four `[Chats v1]`
//! API sites (`chat_create`, `chat_cast`, `chat_outfits`) and, since P4.123, the
//! four wardrobe-tool sites — the executor's `run_wardrobe_{create,wear,take_off,
//! archive}` via [`trigger_for_characters`], AFTER the writer closure resolves
//! (the trigger is async and cannot run on the writer thread). **Named ordering
//! divergence:** v4 triggers BEFORE its state re-read; v5 triggers after the
//! whole write commits — observable only when the re-read itself fails (v5's op
//! fails with no job where v4 has already enqueued). No differential can plant
//! that failure; it is pinned in neither direction.
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
    /// v4 `callerContext` — who asked (`'[Chats v1] chat-open'`,
    /// `'[Chats v1] participant-join'`, …). It is NEVER written into the job:
    /// v4 reads it only as the `context` field of the trigger's WARN lines
    /// (P4.D238).
    pub caller_context: &'static str,
    /// One-shot override: use this profile instead of the chat's default. The
    /// chat's stored `imageProfileId` is NOT mutated.
    pub image_profile_id_override: Option<String>,
    /// One-shot equipped-slots override (a JSON `{ top, bottom, footwear,
    /// accessories, hair }` object) forwarded into the job payload.
    pub equipped_slots_override: Option<Value>,
    /// Reroll: bypass the avatar configuration cache and generate
    /// unconditionally (v4 `7fbf8a55b`). Set by the manual regenerate button;
    /// **automatic triggers leave it false**, which is the whole point of the
    /// cache — a wardrobe change that returns a character to an outfit they have
    /// worn before costs nothing.
    pub force: bool,
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
        // v4's catch → WARN, then the structured error result with the message.
        Err(e) => {
            tracing::warn!(
                context = params.caller_context,
                chat_id = %params.chat_id,
                character_id = %params.character_id,
                error = %e,
                "Failed to enqueue avatar generation"
            );
            AvatarGenerationResult::NotQueued {
                reason: "error".to_string(),
                message: e.to_string(),
            }
        }
    }
}

async fn trigger_avatar_generation_inner(
    db: &Db,
    params: &AvatarGenerationParams,
) -> Result<AvatarGenerationResult, DbError> {
    let chat_id = params.chat_id.clone();
    // v4's repository reads are FALLBACK reads (`null` / `[]` + an ERROR line),
    // so a read failure takes the not-found / next-tier arm, never the catch —
    // and v4's `getCollection()` sits inside the same `safeQuery`, so a POOL
    // failure takes the same arm with the same line (P4.123 item 9): the whole
    // checkout goes under the one fallback home.
    let chat = crate::db::fallback::find_by_id_or_none("chats", &params.chat_id, || {
        db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id))
    });
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
        let lookup = over.clone();
        let profile = crate::db::fallback::find_by_id_or_none("image_profiles", &over, || {
            db.read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &lookup))
        });
        match profile {
            Some(p) => {
                image_profile_id = p.get("id").and_then(Value::as_str).map(str::to_string);
            }
            None => tracing::warn!(
                context = params.caller_context,
                chat_id = %params.chat_id,
                image_profile_id_override = %over,
                "Avatar generation override profile not found, falling back"
            ),
        }
    }

    if image_profile_id.is_none() {
        if let Some(chat_profile) = chat.get("imageProfileId").and_then(Value::as_str) {
            let chat_profile = chat_profile.to_string();
            let lookup = chat_profile.clone();
            let profile =
                crate::db::fallback::find_by_id_or_none("image_profiles", &chat_profile, || {
                    db.read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &lookup))
                });
            if let Some(p) = profile {
                image_profile_id = p.get("id").and_then(Value::as_str).map(str::to_string);
            }
        }
    }

    if image_profile_id.is_none() {
        let all = crate::db::fallback::find_all_or_empty("image_profiles", || {
            db.read_main(|conn| crate::db::image_profiles::find_all(conn))
        });
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
        params.force,
    )
    .await?;

    Ok(AvatarGenerationResult::Queued)
}

/// v4 `triggerAvatarGenerationIfEnabled`: trigger avatar generation ONLY when the
/// chat has `avatarGenerationEnabled` and is NOT an autonomous room. Used by
/// automatic triggers (wardrobe changes). Failures are swallowed — automatic
/// paths must never affect the caller's result.
///
/// v4's own catch here (WARN `Failed to enqueue avatar generation after outfit
/// change`) is UNREACHABLE in real v4 and is not ported: its only reads are
/// `repos.chats.findById` — a fallback-mode `safeQuery` that logs ERROR `Error
/// finding entity by ID` and answers `null` — and `triggerAvatarGeneration`,
/// which catches everything itself. v5 takes the same fallback read
/// ([`crate::db::chats_read::find_by_id_or_none`]) and this function returns
/// `()`, so there is nothing left for a catch to see (P4.D238).
pub async fn trigger_avatar_generation_if_enabled(db: &Db, params: &AvatarGenerationParams) {
    let chat_id = params.chat_id.clone();
    // The pool checkout under the same fallback as the read (P4.123 item 9).
    let chat = crate::db::fallback::find_by_id_or_none("chats", &params.chat_id, || {
        db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id))
    });
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

/// The wardrobe tools' shared trigger loop (P4.123): one awaited
/// [`trigger_avatar_generation_if_enabled`] per character id, with v4's literal
/// `callerContext`. Never fails the caller.
pub async fn trigger_for_characters(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_ids: &[String],
    caller_context: &'static str,
) {
    for character_id in character_ids {
        trigger_avatar_generation_if_enabled(
            db,
            &AvatarGenerationParams {
                user_id: user_id.to_string(),
                chat_id: chat_id.to_string(),
                character_id: character_id.clone(),
                caller_context,
                image_profile_id_override: None,
                equipped_slots_override: None,
                force: false,
            },
        )
        .await;
    }
}
