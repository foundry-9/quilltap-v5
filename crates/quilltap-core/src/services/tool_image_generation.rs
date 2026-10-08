//! Pictures commissioned by the wardrobe tools — v4 `lib/wardrobe/tool-image-
//! generation.ts` (`b3f937076`, unchanged through `f5e953a3f`; P4.D262).
//!
//! `wardrobe_create` and `wardrobe_update` take an optional `generate_image`
//! flag. Whether anything is drawn is the operator's call, not the model's:
//! `chatSettings.wardrobeImageSettings.generateFromTools` (Settings → Images →
//! Wardrobe Images, off by default) is both the gate and the default. With it
//! off, a model that asks is told so and nothing is spent; with it on, the
//! tool's own default applies unless the model says otherwise.
//!
//! The picture is a background job (`WARDROBE_ITEM_IMAGE_GENERATION`), never an
//! inline wait: a provider call takes twenty-odd seconds and the turn must not
//! stand still for it. The tools' executor calls
//! [`maybe_queue_wardrobe_tool_image`] AFTER the garment's write commits (it is
//! async and enqueues through the `Db`), and the outcome rides back onto the
//! tool's output.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::vault_wardrobe_public::WardrobePatch;
use crate::services::queue_service::{
    enqueue_wardrobe_item_image_generation, WardrobeItemImageJobPayload,
};

const LOG_CONTEXT: &str = "wardrobe.tool-image-generation";

/// v4's four outcomes (`WardrobeToolImageResult.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WardrobeToolImageStatus {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "not-enabled")]
    NotEnabled,
    #[serde(rename = "no-image-profile")]
    NoImageProfile,
    #[serde(rename = "failed")]
    Failed,
}

impl WardrobeToolImageStatus {
    /// v4's wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeToolImageStatus::Queued => "queued",
            WardrobeToolImageStatus::NotEnabled => "not-enabled",
            WardrobeToolImageStatus::NoImageProfile => "no-image-profile",
            WardrobeToolImageStatus::Failed => "failed",
        }
    }
}

/// What became of a picture a wardrobe tool asked for (v4
/// `WardrobeToolImageResult` — key order `status`, `message`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WardrobeToolImageResult {
    pub status: WardrobeToolImageStatus,
    /// One plain sentence for the model.
    pub message: String,
}

/// The `queued` sentence, verbatim.
pub const QUEUED_MESSAGE: &str = "A picture of this item is being drawn in the background. Once it is ready, wardrobe_read and wardrobe_list show its image_file_id.";
/// The `not-enabled` sentence, verbatim.
pub const NOT_ENABLED_MESSAGE: &str = "No picture was made: the operator has not allowed the wardrobe tools to generate pictures (Settings → Images → Wardrobe Images).";
/// The `no-image-profile` sentence, verbatim.
pub const NO_IMAGE_PROFILE_MESSAGE: &str =
    "No picture was made: no usable image profile is configured.";
/// The `failed` sentence, verbatim.
pub const FAILED_MESSAGE: &str = "No picture was made: queueing the picture failed.";

impl WardrobeToolImageResult {
    /// The result for `status`, with v4's sentence.
    pub fn of(status: WardrobeToolImageStatus) -> Self {
        let message = match status {
            WardrobeToolImageStatus::Queued => QUEUED_MESSAGE,
            WardrobeToolImageStatus::NotEnabled => NOT_ENABLED_MESSAGE,
            WardrobeToolImageStatus::NoImageProfile => NO_IMAGE_PROFILE_MESSAGE,
            WardrobeToolImageStatus::Failed => FAILED_MESSAGE,
        };
        WardrobeToolImageResult {
            status,
            message: message.to_string(),
        }
    }
}

/// v4 `wardrobeToolImagesEnabled(settings)` — the operator's switch, read the
/// one way: `settings?.wardrobeImageSettings?.generateFromTools === true`.
/// Absent settings (or bag, or key) read as off. The production read is
/// [`crate::db::chat_settings::wardrobe_tool_images_enabled`] (the same rule
/// over the stored row); this is the pure form, over a settings object.
pub fn wardrobe_tool_images_enabled(settings: Option<&Value>) -> bool {
    settings
        .and_then(|s| s.get("wardrobeImageSettings"))
        .and_then(|w| w.get("generateFromTools"))
        == Some(&Value::Bool(true))
}

/// v4's `wanted = requested ?? (enabled && defaultWhenEnabled)` — an explicit
/// `generate_image` wins either way; otherwise the tool's default applies only
/// when the operator allows pictures.
pub fn wanted(enabled: bool, requested: Option<bool>, default_when_enabled: bool) -> bool {
    requested.unwrap_or(enabled && default_when_enabled)
}

/// v4 `patchChangesLook(item, patch)` (`wardrobe-update-handler.ts:33-46`) —
/// whether a patch changes what a picture of the item would show: the title,
/// the Portrait Cue (`imagePrompt`, an absent cue reading `null`), the types,
/// or the components (an absent list reading `[]`), each compared as v4 does
/// (`join(',')` for the lists).
pub fn patch_changes_look(item: &Value, patch: &WardrobePatch) -> bool {
    let joined = |v: Option<&Value>| -> String {
        v.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|e| e.as_str().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default()
    };
    if let Some(title) = &patch.title {
        if Some(title.as_str()) != item.get("title").and_then(Value::as_str) {
            return true;
        }
    }
    if let Some(image_prompt) = &patch.image_prompt {
        let current = item.get("imagePrompt").and_then(Value::as_str);
        if image_prompt.as_deref() != current {
            return true;
        }
    }
    if let Some(types) = &patch.types {
        if types.join(",") != joined(item.get("types")) {
            return true;
        }
    }
    if let Some(components) = &patch.component_item_ids {
        if components.join(",") != joined(item.get("componentItemIds")) {
            return true;
        }
    }
    false
}

/// v4 `formatWardrobeToolImageLine(result)` — the one line a tool's formatted
/// result carries about its picture.
pub fn format_wardrobe_tool_image_line(result: Option<&WardrobeToolImageResult>) -> Option<String> {
    result.map(|r| format!("- Picture: {}", r.message))
}

/// v4 `formatWardrobeImageHandle(imageFileId)` — how an item's current picture
/// is announced to a model: its file id and the tool that looks at it
/// (`describe_image` resolves an image-v2 file uuid, which is what
/// `imageFileId` is; `keep_image` files it into the character's album).
pub fn format_wardrobe_image_handle(image_file_id: &str) -> String {
    format!(
        "{image_file_id} (pass to describe_image to see it, or keep_image to file it in your album)"
    )
}

/// v4 `QueueWardrobeToolImageArgs`.
#[derive(Debug, Clone)]
pub struct QueueWardrobeToolImageArgs {
    pub user_id: String,
    pub chat_id: String,
    /// The character whose wardrobe holds the item (the RECIPIENT, for a gift).
    pub character_id: String,
    pub item_id: String,
    /// The tool's `generate_image` input; `None` = the tool's default.
    pub requested: Option<bool>,
    /// What the tool does when the operator's switch is on and the model said
    /// nothing.
    pub default_when_enabled: bool,
    /// The call site, for logs.
    pub caller_context: &'static str,
}

/// v4 `resolveWardrobeImageProfile(userId, repos)` with no override
/// (`profile-resolution.ts:119-141`, `7c8572869`): the designated
/// `wardrobeImageSettings.imageProfileId`, then the user's default; each must
/// exist, belong to the user and carry an API key. The Lantern's
/// `storyBackgroundsSettings.defaultImageProfileId` is deliberately NOT
/// consulted (a backdrop desk is chosen for landscapes, not for a garment a
/// provider might refuse). Answers the profile's id.
//
// HANDOFF(P4.D263): `services::image_profile_resolution::resolve_wardrobe_image_profile`
// is P4.D263's (R-G); it was not on this lane's base. This lane-local fn codes
// exactly v4's three steps minus the per-generation override (the tools never
// pass one); the unifier repoints this call at P4.D263's fn and deletes it.
fn resolve_wardrobe_image_profile(main: &Connection, user_id: &str) -> Option<String> {
    let usable = |profile: &Value| -> bool {
        profile.get("userId").and_then(Value::as_str) == Some(user_id)
            && profile
                .get("apiKeyId")
                .and_then(Value::as_str)
                .is_some_and(|k| !k.is_empty())
    };
    let designated = crate::db::chat_settings::find_by_user_id(main, user_id)
        .ok()
        .flatten()
        .and_then(|row| {
            row.get("wardrobeImageSettings")
                .and_then(|w| w.get("imageProfileId"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        });
    if let Some(id) = designated {
        if let Some(profile) = crate::db::image_profiles::find_by_id_or_none(main, &id) {
            if usable(&profile) {
                return profile
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
    }
    // `findDefault(userId)` then `fallback.apiKeyId` (scoped to the user by
    // the filter itself).
    let all = crate::db::image_profiles::find_all_or_empty(main);
    let fallback = all.iter().find(|p| {
        p.get("isDefault").and_then(Value::as_bool) == Some(true)
            && p.get("userId").and_then(Value::as_str) == Some(user_id)
    })?;
    fallback
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty())?;
    fallback
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// v4 `maybeQueueWardrobeToolImage(repos, args)` — queue a picture of
/// `item_id` if the operator allows it and the call wants one. `None` when no
/// picture was wanted (nothing to report). Never fails: a picture is never
/// worth failing the garment over — a failed enqueue is the `failed` outcome
/// and v4's ERROR line.
pub async fn maybe_queue_wardrobe_tool_image(
    db: &Db,
    args: QueueWardrobeToolImageArgs,
) -> Option<WardrobeToolImageResult> {
    let QueueWardrobeToolImageArgs {
        user_id,
        chat_id,
        character_id,
        item_id,
        requested,
        default_when_enabled,
        caller_context,
    } = args;

    let enabled = db
        .read_main(|main| {
            Ok(crate::db::chat_settings::wardrobe_tool_images_enabled(
                main, &user_id,
            ))
        })
        .unwrap_or(false);
    let wanted = wanted(enabled, requested, default_when_enabled);

    // winston drops an `undefined` field, so `requested` is omitted when the
    // model said nothing.
    match requested {
        Some(requested) => tracing::debug!(
            context = LOG_CONTEXT,
            callerContext = caller_context,
            itemId = %item_id,
            enabled,
            requested,
            defaultWhenEnabled = default_when_enabled,
            wanted,
            "[WardrobeToolImage] Deciding on a tool-queued picture"
        ),
        None => tracing::debug!(
            context = LOG_CONTEXT,
            callerContext = caller_context,
            itemId = %item_id,
            enabled,
            defaultWhenEnabled = default_when_enabled,
            wanted,
            "[WardrobeToolImage] Deciding on a tool-queued picture"
        ),
    }

    if !wanted {
        return None;
    }
    if !enabled {
        return Some(WardrobeToolImageResult::of(
            WardrobeToolImageStatus::NotEnabled,
        ));
    }

    let profile_id = db
        .read_main(|main| Ok(resolve_wardrobe_image_profile(main, &user_id)))
        .ok()
        .flatten();
    let Some(profile_id) = profile_id else {
        return Some(WardrobeToolImageResult::of(
            WardrobeToolImageStatus::NoImageProfile,
        ));
    };

    let payload = WardrobeItemImageJobPayload {
        chat_id: chat_id.clone(),
        character_id: character_id.clone(),
        item_id: item_id.clone(),
    };
    match enqueue_wardrobe_item_image_generation(db, &user_id, &payload).await {
        Ok((job_id, is_new)) => {
            tracing::info!(
                context = LOG_CONTEXT,
                callerContext = caller_context,
                chatId = %chat_id,
                characterId = %character_id,
                itemId = %item_id,
                jobId = %job_id,
                isNew = is_new,
                profileId = %profile_id,
                "[WardrobeToolImage] Queued a picture for a wardrobe item"
            );
            Some(WardrobeToolImageResult::of(WardrobeToolImageStatus::Queued))
        }
        Err(e) => {
            tracing::error!(
                context = LOG_CONTEXT,
                callerContext = caller_context,
                itemId = %item_id,
                error = %crate::db::fallback::error_text(&e),
                "[WardrobeToolImage] Could not queue a picture for a wardrobe item"
            );
            Some(WardrobeToolImageResult::of(WardrobeToolImageStatus::Failed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_switch_reads_one_way() {
        assert!(!wardrobe_tool_images_enabled(None));
        assert!(!wardrobe_tool_images_enabled(Some(&json!({}))));
        assert!(!wardrobe_tool_images_enabled(Some(
            &json!({"wardrobeImageSettings": {"imageProfileId": null}})
        )));
        assert!(!wardrobe_tool_images_enabled(Some(
            &json!({"wardrobeImageSettings": {"generateFromTools": "true"}})
        )));
        assert!(wardrobe_tool_images_enabled(Some(
            &json!({"wardrobeImageSettings": {"imageProfileId": null, "generateFromTools": true}})
        )));
    }

    #[test]
    fn an_explicit_flag_wins_and_the_default_needs_the_switch() {
        assert!(wanted(false, Some(true), false));
        assert!(!wanted(true, Some(false), true));
        assert!(wanted(true, None, true));
        assert!(!wanted(true, None, false));
        assert!(!wanted(false, None, true));
    }

    #[test]
    fn the_four_sentences_and_the_line() {
        let r = WardrobeToolImageResult::of(WardrobeToolImageStatus::NotEnabled);
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            json!({"status": "not-enabled", "message": NOT_ENABLED_MESSAGE})
        );
        assert_eq!(
            format_wardrobe_tool_image_line(Some(&r)).unwrap(),
            format!("- Picture: {NOT_ENABLED_MESSAGE}")
        );
        assert_eq!(format_wardrobe_tool_image_line(None), None);
    }
}
