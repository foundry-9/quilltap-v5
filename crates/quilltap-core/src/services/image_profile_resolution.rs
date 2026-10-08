//! Image-profile resolution — v4 `lib/image-gen/profile-resolution.ts`
//! `resolveImageProfileForChat`: the four-tier chain (chat → story-backgrounds
//! default → project default → user default), each candidate ownership- and
//! `apiKeyId`-checked.
//!
//! The story-background enqueue gate that used to live here
//! (`queue_story_background_if_enabled`) MOVED to
//! [`crate::services::auto_title`] with v4 `00c290c9a` (bugs 163/164), where v4
//! moved it too: a changed title is the only thing that pulls the Lantern's
//! auto-trigger, so the gate sits beside the one chokepoint every automatic
//! title goes through.

use rusqlite::Connection;
use serde_json::Value;

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

/// v4 `resolveWardrobeImageProfile(userId, repos, override)`
/// (`profile-resolution.ts:119-141`, `7c8572869`) — the image profile that
/// draws a wardrobe item's picture, as the whole PROFILE row (the generation
/// reads its `apiKeyId`, `id`, `name`, `provider`, `modelName`, `parameters`).
///
/// Priority: (1) the per-generation override (the editor's "▾" pick); (2)
/// `chatSettings.wardrobeImageSettings.imageProfileId` (Settings → Images);
/// (3) the user's default image profile. Each of the first two must exist,
/// belong to the user and carry an API key; the default needs only the key
/// (`findDefault` already scopes by user). The Lantern's
/// `storyBackgroundsSettings.defaultImageProfileId` is deliberately NOT
/// consulted: the backdrop desk is chosen for landscapes, not for a garment a
/// provider might refuse. Both repository reads are v4's fallback reads (a
/// failure logs and reads as "none").
pub fn resolve_wardrobe_image_profile(
    main: &Connection,
    user_id: &str,
    chat_settings: Option<&Value>,
    override_id: Option<&str>,
) -> Option<Value> {
    let truthy = |s: &&str| !s.is_empty();
    let has_key = |p: &Value| {
        p.get("apiKeyId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .is_some()
    };
    let usable = |p: &Value| p.get("userId").and_then(Value::as_str) == Some(user_id) && has_key(p);

    if let Some(id) = override_id.filter(truthy) {
        if let Some(p) = crate::db::image_profiles::find_by_id_or_none(main, id) {
            if usable(&p) {
                return Some(p);
            }
        }
    }

    if let Some(id) = chat_settings
        .and_then(|cs| cs.get("wardrobeImageSettings"))
        .and_then(|w| w.get("imageProfileId"))
        .and_then(Value::as_str)
        .filter(truthy)
    {
        if let Some(p) = crate::db::image_profiles::find_by_id_or_none(main, id) {
            if usable(&p) {
                return Some(p);
            }
        }
    }

    crate::db::image_profiles::find_all_or_empty(main)
        .into_iter()
        .find(|p| {
            p.get("isDefault").and_then(Value::as_bool) == Some(true)
                && p.get("userId").and_then(Value::as_str) == Some(user_id)
        })
        .filter(has_key)
}
