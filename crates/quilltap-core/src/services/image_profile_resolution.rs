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

/// v4's `LOG_CONTEXT` (`profile-resolution.ts:14`) — the `context` field on
/// every `[ProfileResolution]` line.
const LOG_CONTEXT: &str = "image-gen.profile-resolution";

/// v4 `resolveAvatarImageProfile(userId, repos, { override, chat })`
/// (`profile-resolution.ts:150-210`, `4a38222e6`) — the image profile that
/// paints a character's avatar, as the whole PROFILE row: the chat avatar job
/// (queued by the wardrobe triggers, the regenerate button and the toggle-on
/// sweep) and the wardrobe dialog's out-of-chat preview (P4.D279 §8).
///
/// Priority: (1) the one-shot override (the wardrobe dialog's model pick);
/// (2) the chat's own image profile, when there is a chat — `chat` is the
/// chat row (v4's `Pick<ChatMetadata, 'imageProfileId'>`), `None` for the
/// out-of-chat preview; (3) the user's default image profile. Each of the
/// first two must exist, belong to the user and carry an API key — the checks
/// `resolveImageProfileForChat` and `resolveWardrobeImageProfile` make; the
/// default needs only the key (`findDefault` already scopes by user). A
/// candidate that fails them is passed over for the next rather than handed
/// to a job that would only skip for want of a key — and each pass-over is
/// narrated by v4's `[ProfileResolution]` DEBUG line (`found` says whether
/// the row existed at all); a chain that ends empty logs the third. All three
/// repository reads are v4's fallback reads (a failure logs the base
/// repository's line and reads as "none").
pub fn resolve_avatar_image_profile(
    main: &Connection,
    user_id: &str,
    override_id: Option<&str>,
    chat: Option<&Value>,
) -> Option<Value> {
    // v4's `if (options.override)` / `if (chatProfileId)` — a falsy (empty)
    // id skips the tier without a line.
    let truthy = |s: &&str| !s.is_empty();
    let has_key = |p: &Value| {
        p.get("apiKeyId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .is_some()
    };
    let usable = |p: &Value| p.get("userId").and_then(Value::as_str) == Some(user_id) && has_key(p);

    if let Some(id) = override_id.filter(truthy) {
        let profile = crate::db::image_profiles::find_by_id_or_none(main, id);
        if let Some(p) = profile.as_ref().filter(|p| usable(p)) {
            return Some(p.clone());
        }
        tracing::debug!(
            target: "quilltap::image_profile_resolution",
            context = LOG_CONTEXT,
            override = %id,
            found = profile.is_some(),
            "[ProfileResolution] Avatar override profile unusable; falling back"
        );
    }

    if let Some(chat_profile_id) = chat
        .and_then(|c| c.get("imageProfileId"))
        .and_then(Value::as_str)
        .filter(truthy)
    {
        let profile = crate::db::image_profiles::find_by_id_or_none(main, chat_profile_id);
        if let Some(p) = profile.as_ref().filter(|p| usable(p)) {
            return Some(p.clone());
        }
        tracing::debug!(
            target: "quilltap::image_profile_resolution",
            context = LOG_CONTEXT,
            chatProfileId = %chat_profile_id,
            found = profile.is_some(),
            "[ProfileResolution] Chat image profile unusable for avatars; falling back"
        );
    }

    if let Some(fallback) =
        crate::db::image_profiles::find_default_or_none(main, user_id).filter(has_key)
    {
        return Some(fallback);
    }

    tracing::debug!(
        target: "quilltap::image_profile_resolution",
        context = LOG_CONTEXT,
        userId = %user_id,
        "[ProfileResolution] No usable avatar image profile"
    );
    None
}

#[cfg(test)]
mod avatar_resolution_tests {
    //! Capture pins for `resolve_avatar_image_profile`'s three
    //! `[ProfileResolution]` lines (each with a silence leg) and the
    //! `findDefault` fallback shape, on an in-memory `image_profiles` table.
    //! The field-level equivalence against v4's REAL `resolveAvatarImageProfile`
    //! is the harness family `avatar_image_profile_resolution_equivalence`.

    use super::*;
    use crate::test_support::captured_with;
    use rusqlite::params;
    use serde_json::json;

    const U1: &str = "11111111-1111-4111-8111-111111111111";
    const U2: &str = "22222222-2222-4222-8222-222222222222";
    const KEY: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const OVR_OK: &str = "0e000001-0000-4000-8000-000000000001";
    const OVR_NOKEY: &str = "0e000001-0000-4000-8000-000000000002";
    const OVR_OTHER: &str = "0e000001-0000-4000-8000-000000000003";
    const CHAT_OK: &str = "0e000001-0000-4000-8000-000000000004";
    const DEF_OK: &str = "0e000001-0000-4000-8000-000000000005";
    const UNKNOWN: &str = "0e000001-0000-4000-8000-0000000000ff";

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE image_profiles (\
               id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
               apiKeyId TEXT, baseUrl TEXT, modelName TEXT, parameters TEXT, \
               isDefault INTEGER, isDangerousCompatible INTEGER, tags TEXT, \
               createdAt TEXT, updatedAt TEXT)",
        )
        .unwrap();
        let ins = |id: &str, user: &str, key: Option<&str>, is_default: bool| {
            conn.execute(
                "INSERT INTO image_profiles VALUES (?1, ?2, 'p', 'OPENAI', ?3, NULL, 'm', '{}', ?4, 0, '[]', 't', 't')",
                params![id, user, key, i64::from(is_default)],
            )
            .unwrap();
        };
        ins(OVR_OK, U1, Some(KEY), false);
        ins(OVR_NOKEY, U1, None, false);
        ins(OVR_OTHER, U2, Some(KEY), false);
        ins(CHAT_OK, U1, Some(KEY), false);
        ins(DEF_OK, U1, Some(KEY), true);
        conn
    }

    fn id_of(p: &Option<Value>) -> Option<&str> {
        p.as_ref().and_then(|v| v.get("id")).and_then(Value::as_str)
    }

    #[test]
    fn override_hit_is_silent() {
        let conn = db();
        let (got, lines) = captured_with(|| {
            resolve_avatar_image_profile(
                &conn,
                U1,
                Some(OVR_OK),
                Some(&json!({ "imageProfileId": CHAT_OK })),
            )
        });
        assert_eq!(id_of(&got), Some(OVR_OK));
        assert!(
            lines.is_empty(),
            "a usable override logs nothing: {lines:?}"
        );
    }

    #[test]
    fn keyless_override_logs_found_true_and_falls_to_chat() {
        let conn = db();
        let (got, lines) = captured_with(|| {
            resolve_avatar_image_profile(
                &conn,
                U1,
                Some(OVR_NOKEY),
                Some(&json!({ "imageProfileId": CHAT_OK })),
            )
        });
        assert_eq!(id_of(&got), Some(CHAT_OK));
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::image_profile_resolution [ProfileResolution] Avatar override profile unusable; falling back context=image-gen.profile-resolution override={OVR_NOKEY} found=true"
            )]
        );
    }

    #[test]
    fn unknown_override_logs_found_false_and_falls_to_default() {
        let conn = db();
        let (got, lines) =
            captured_with(|| resolve_avatar_image_profile(&conn, U1, Some(UNKNOWN), None));
        assert_eq!(id_of(&got), Some(DEF_OK));
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::image_profile_resolution [ProfileResolution] Avatar override profile unusable; falling back context=image-gen.profile-resolution override={UNKNOWN} found=true"
            ).replace("found=true", "found=false")]
        );
    }

    #[test]
    fn other_users_chat_profile_logs_and_falls_to_default() {
        let conn = db();
        let (got, lines) = captured_with(|| {
            resolve_avatar_image_profile(
                &conn,
                U1,
                None,
                Some(&json!({ "imageProfileId": OVR_OTHER })),
            )
        });
        assert_eq!(id_of(&got), Some(DEF_OK));
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::image_profile_resolution [ProfileResolution] Chat image profile unusable for avatars; falling back context=image-gen.profile-resolution chatProfileId={OVR_OTHER} found=true"
            )]
        );
    }

    #[test]
    fn empty_ids_skip_their_tiers_silently() {
        let conn = db();
        let (got, lines) = captured_with(|| {
            resolve_avatar_image_profile(
                &conn,
                U1,
                Some(""),
                Some(&json!({ "imageProfileId": "" })),
            )
        });
        assert_eq!(id_of(&got), Some(DEF_OK));
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn no_default_logs_the_third_line() {
        let conn = db();
        let (got, lines) = captured_with(|| resolve_avatar_image_profile(&conn, U2, None, None));
        assert!(got.is_none());
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::image_profile_resolution [ProfileResolution] No usable avatar image profile context=image-gen.profile-resolution userId={U2}"
            )]
        );
    }

    #[test]
    fn keyless_default_is_passed_over() {
        let conn = db();
        conn.execute(
            "UPDATE image_profiles SET apiKeyId = NULL WHERE id = ?1",
            params![DEF_OK],
        )
        .unwrap();
        let (got, lines) = captured_with(|| resolve_avatar_image_profile(&conn, U1, None, None));
        assert!(got.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("No usable avatar image profile userId=")
                || lines[0].contains("No usable avatar image profile context=")
        );
    }

    #[test]
    fn find_default_scopes_by_user_and_reads_the_whole_row() {
        let conn = db();
        let row = crate::db::image_profiles::find_default(&conn, U1)
            .unwrap()
            .expect("U1 has a default");
        assert_eq!(row["id"], DEF_OK);
        assert_eq!(row["isDefault"], true);
        assert!(crate::db::image_profiles::find_default(&conn, U2)
            .unwrap()
            .is_none());
    }

    #[test]
    fn find_default_failure_logs_the_inner_find_one_by_filter_line_only() {
        let conn = db();
        // A BLOB `name` fails the row read (v4: `validate` throws a ZodError
        // inside `findOneByFilter`'s fallback `safeQuery`).
        conn.execute(
            "UPDATE image_profiles SET name = x'00000000' WHERE id = ?1",
            params![DEF_OK],
        )
        .unwrap();
        let (got, lines) = captured_with(|| resolve_avatar_image_profile(&conn, U1, None, None));
        assert!(got.is_none());
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding entity by filter collection=image_profiles error=Invalid column type Blob at index: 2, name: name".to_string(),
                format!("DEBUG quilltap::image_profile_resolution [ProfileResolution] No usable avatar image profile context=image-gen.profile-resolution userId={U1}"),
            ]
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Error finding default image profile")),
            "v4's outer RETHROW line is unreachable outside the strict scope"
        );
    }
}
