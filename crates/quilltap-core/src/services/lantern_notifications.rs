//! Writer for image-pipeline chat notifications — v4
//! `lib/services/lantern-notifications/{writer,resolver}.ts`.
//!
//! When an image is produced by one of the three pipelines (story background,
//! character avatar, or the `generate_image` tool), this helper injects a
//! synthetic ASSISTANT-role chat message announcing the image and attaching its
//! file id. Characters see the announcement in their recent-history context, and
//! vision-capable providers receive the image as multimodal content next turn.
//!
//! Attribution by kind:
//!   - `background`      → The Lantern (atmospheric backdrops)  — `systemSender: 'lantern'`
//!   - `avatar`          → Aurora (portrait keeper)             — `systemSender: 'aurora'`
//!   - `character-image` → The Lantern (ad-hoc image on request)— `systemSender: 'lantern'`
//!
//! `systemKind` is the kind string (`'avatar'` / `'background'` / `'character-image'`).
//! A fourth, IMAGE-LESS kind — `background-refused` — has its own writer
//! ([`post_lantern_refusal_notification`], v4 `ce2f1dabf`, #77): the Lantern's
//! painter refused the scene and nobody painted it instead.
//!
//! Gated by `alertCharactersOfLanternImages` (chat override) and
//! `defaultAlertCharactersOfLanternImages` (project default), falling back to OFF
//! when both are null ([`is_lantern_image_alert_enabled`]).
//!
//! Errors never propagate — image generation must never fail because an
//! announcement couldn't be written. The byte-exact `character-image` body is
//! also carried by [`crate::tools::generate_image::lantern_character_image_notification`]
//! (its W4.9a seam); this module composes the full writer (all three kinds) that
//! the parent wires into that `LanternNotificationSink`.

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;

/// The three image-pipeline kinds (v4 `LanternNotificationKind`).
#[derive(Debug, Clone)]
pub enum LanternNotificationKind {
    /// A regenerated character portrait (Aurora).
    Avatar { character_name: String },
    /// An atmospheric scene backdrop (the Lantern).
    Background,
    /// An ad-hoc image produced on a character's request (the Lantern).
    CharacterImage { requester_name: String },
}

impl LanternNotificationKind {
    /// The `systemKind` string (v4 `kind.kind`).
    fn system_kind(&self) -> &'static str {
        match self {
            LanternNotificationKind::Avatar { .. } => "avatar",
            LanternNotificationKind::Background => "background",
            LanternNotificationKind::CharacterImage { .. } => "character-image",
        }
    }

    /// The `systemSender` string (v4 `senderForKind`): Aurora keeps portraits,
    /// the Lantern everything else.
    fn sender(&self) -> &'static str {
        match self {
            LanternNotificationKind::Avatar { .. } => "aurora",
            _ => "lantern",
        }
    }
}

/// The trimmed non-empty aim, or `None` (v4 `const aim = prompt?.trim();` where an
/// empty string is falsy).
fn aim(prompt: Option<&str>) -> Option<&str> {
    prompt.map(js_trim).filter(|s| !s.is_empty())
}

/// Persona-voiced announcement body (v4 `buildContent`). Each branch surfaces the
/// image uuid inline so any character reading the announcement in chat history
/// has the handle required to call `describe_image` (what is in it?),
/// `keep_image` (file it) or `attach_image` (hold it up again) on it. Lantern
/// images already ride into vision-capable turns as real attachments —
/// `describe_image` is for the models that can't see them, and for when a
/// character wants the particulars in words. (v4 `a14a1811`: comment-only —
/// no emitted string moved; the family regen stays byte-identical.)
pub fn build_content(
    kind: &LanternNotificationKind,
    prompt: Option<&str>,
    file_id: &str,
) -> String {
    let aim = aim(prompt);
    match kind {
        LanternNotificationKind::Avatar { character_name } => match aim {
            Some(aim) => format!(
                "Aurora is requesting a new portrait be commissioned for {character_name}, with the following description which omits unnecessary detail: \"{aim}\". The previous likeness is retired with due ceremony; the new one is attached here, catalogued under uuid `{file_id}`, should anyone care to take a fresh look."
            ),
            None => format!(
                "Aurora is requesting a new portrait be commissioned for {character_name}. The previous likeness is retired with due ceremony; the new one is attached here, catalogued under uuid `{file_id}`, should anyone care to take a fresh look."
            ),
        },
        LanternNotificationKind::Background => match aim {
            Some(aim) => format!(
                "The Lantern has projected a new backdrop behind the proceedings, aiming for: \"{aim}\". The resulting image (uuid `{file_id}`) hangs just above, attached for your perusal."
            ),
            None => format!(
                "The Lantern has projected a new backdrop behind the proceedings. The resulting image (uuid `{file_id}`) hangs just above, attached for your perusal."
            ),
        },
        LanternNotificationKind::CharacterImage { requester_name } => format!(
            "The Lantern, acting upon the instructions of {requester_name}, has produced the following picture, catalogued under uuid `{file_id}`. It is attached here, should anyone care to examine it."
        ),
    }
}

/// Opaque-audience announcement body (v4 `buildOpaqueContent`).
pub fn build_opaque_content(
    kind: &LanternNotificationKind,
    prompt: Option<&str>,
    file_id: &str,
) -> String {
    let aim = aim(prompt);
    match kind {
        LanternNotificationKind::Avatar { character_name } => match aim {
            Some(aim) => format!(
                "A new portrait has been commissioned for {character_name}, with this description (which omits unnecessary detail): \"{aim}\". The previous likeness is retired; the new one is attached here, catalogued under uuid `{file_id}`."
            ),
            None => format!(
                "A new portrait has been commissioned for {character_name}. The previous likeness is retired; the new one is attached here, catalogued under uuid `{file_id}`."
            ),
        },
        LanternNotificationKind::Background => match aim {
            Some(aim) => format!(
                "A new background has been generated for this scene, aiming for: \"{aim}\". The resulting image (uuid `{file_id}`) is attached above for your perusal."
            ),
            None => format!(
                "A new background has been generated for this scene. The resulting image (uuid `{file_id}`) is attached above for your perusal."
            ),
        },
        LanternNotificationKind::CharacterImage { requester_name } => format!(
            "A picture has been generated at {requester_name}'s request, catalogued under uuid `{file_id}`. It is attached here for your examination."
        ),
    }
}

/// Resolve the "alert characters of Lantern images" setting (v4
/// `isLanternImageAlertEnabled`): chat → project → global-OFF. A null/absent
/// value at the inner layer inherits from the outer layer.
pub fn is_lantern_image_alert_enabled(chat: Option<&Value>, project: Option<&Value>) -> bool {
    if let Some(v) = chat.and_then(|c| c.get("alertCharactersOfLanternImages")) {
        if !v.is_null() {
            return v.as_bool().unwrap_or(false);
        }
    }
    if let Some(v) = project.and_then(|p| p.get("defaultAlertCharactersOfLanternImages")) {
        if !v.is_null() {
            return v.as_bool().unwrap_or(false);
        }
    }
    false
}

/// Parameters for [`post_lantern_image_notification`] (v4 `PostParams`).
#[derive(Debug, Clone)]
pub struct LanternPostParams {
    pub chat_id: String,
    pub file_id: String,
    pub kind: LanternNotificationKind,
    /// Generation prompt to quote; callers pass it because the file row may be a
    /// still-buffered write not yet readable from the DB.
    pub prompt: Option<String>,
    /// The Concierge's call sheet when the picture was refused on the way and
    /// rerouted ([`crate::services::dangerous_content::image_failover`], v4
    /// `8bd080267`). Rendered under the bubble's avatar. Empty when the first
    /// profile answered — and then OMITTED from the message, as v4 omits it.
    pub route_trail: Vec<crate::services::route_trail::RouteAttempt>,
}

/// Post an image-pipeline notification (v4 `postLanternImageNotification`). Reads
/// the chat (missing → no-op), the project (for the gate), applies the
/// alert-enabled gate, then inserts the `MessageEvent` (ASSISTANT role,
/// `attachments: [fileId]`, `participantId: null`, `systemSender`/`systemKind`
/// per kind) and best-effort links the file to the message. Errors never
/// propagate (v4 catches + logs). Returns the posted message (`Some`) when a
/// bubble was written, else `None` (no chat / gated off / failure).
pub async fn post_lantern_image_notification(db: &Db, params: LanternPostParams) -> Option<Value> {
    // Read the chat (existence + `alertCharactersOfLanternImages`) and the
    // project (`defaultAlertCharactersOfLanternImages`, a store-managed overlay
    // field → the OVERLAID projects read spanning both DBs, matching v4's
    // `repos.chats.findById` + `repos.projects.findById`).
    let cid = params.chat_id.clone();
    let chat = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid))
        .ok()?;
    let chat = chat?;

    let project: Option<Value> = match chat
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(project_id) => {
            let pid = project_id.to_string();
            db.read_main(|main| {
                db.read_mount_index(|mount| {
                    let repo = crate::db::projects::ProjectsRepository::new(main, mount);
                    repo.find_by_id(&pid).map_err(|e| {
                        crate::db::DbError::Internal(format!("project read failed: {e:?}"))
                    })
                })
            })
            .ok()
            .flatten()
        }
        None => None,
    };

    if !is_lantern_image_alert_enabled(Some(&chat), project.as_ref()) {
        return None;
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let content = build_content(&params.kind, params.prompt.as_deref(), &params.file_id);
    let opaque_content =
        build_opaque_content(&params.kind, params.prompt.as_deref(), &params.file_id);

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [params.file_id.clone()],
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": params.kind.sender(),
        "systemKind": params.kind.system_kind(),
    });
    let mut message = message;
    if !params.route_trail.is_empty() {
        message.as_object_mut().unwrap().insert(
            "routeTrail".into(),
            serde_json::to_value(&params.route_trail).unwrap_or(Value::Null),
        );
    }

    let failed = |error: String| {
        // v4's outer catch (the add is the one step here that can throw).
        tracing::error!(
            target: "quilltap::lantern_notification",
            context = "lantern-notifications",
            chat_id = %params.chat_id,
            file_id = %params.file_id,
            kind = params.kind.system_kind(),
            error = %error,
            "[LanternNotification] Failed to post announcement"
        );
    };
    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(event) => event,
        Err(e) => {
            failed(e.to_string());
            return None;
        }
    };
    let cid = params.chat_id.clone();
    if let Err(e) = db
        .write(move |writers| writers.main().chat_messages().add_message(&cid, &event))
        .await
    {
        failed(e.to_string());
        return None;
    }

    // Best-effort file→message link (v4 warns, never throws, on a link failure).
    let file_id = params.file_id.clone();
    let msg_id = message_id.clone();
    if let Err(e) = db
        .write(move |writers| {
            writers
                .main()
                .files()
                .add_link(&file_id, &msg_id)
                .map(|_| ())
        })
        .await
    {
        tracing::warn!(
            target: "quilltap::lantern_notification",
            context = "lantern-notifications",
            chat_id = %params.chat_id,
            file_id = %params.file_id,
            message_id = %message_id,
            error = %e,
            "[LanternNotification] Could not link file to message"
        );
    }

    // v4's success line; `routeTrailLength` is P4.D225's (`8bd080267`). The
    // three lines predate #73 in v4 and were never ported here — the trail
    // is what brought them in.
    tracing::info!(
        target: "quilltap::lantern_notification",
        context = "lantern-notifications",
        chat_id = %params.chat_id,
        file_id = %params.file_id,
        message_id = %message_id,
        kind = params.kind.system_kind(),
        route_trail_length = params.route_trail.len(),
        "[LanternNotification] Announcement posted"
    );
    Some(message)
}

/// The Lantern's painter refused the scene and nobody painted it instead (v4
/// `LanternRefusalKind`, NEW at `ce2f1dabf`, #77). No image, so it has its own
/// writer ([`post_lantern_refusal_notification`]) rather than a branch of the
/// image announcement — deliberately NOT a [`LanternNotificationKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanternRefusal {
    /// Provider of the image profile that refused (e.g. `GOOGLE`).
    pub provider: String,
    /// Its image model.
    pub model_name: String,
}

impl LanternRefusal {
    /// v4 `refusal.kind` — the ONE kind, `background-refused`.
    pub const SYSTEM_KIND: &'static str = "background-refused";
}

/// v4 `buildLanternRefusalContent` (`ce2f1dabf`). The em dash is U+2014; the
/// apostrophe is ASCII.
pub fn build_lantern_refusal_content(refusal: &LanternRefusal) -> String {
    format!(
        "The Lantern's usual painter ({} {}) would not take the scene \u{2014} called it improper and downed brushes. The backdrop stays as it was.",
        refusal.provider, refusal.model_name
    )
}

/// v4 `buildLanternRefusalOpaqueContent` (`ce2f1dabf`).
pub fn build_lantern_refusal_opaque_content(refusal: &LanternRefusal) -> String {
    format!(
        "Story background refused by {} {} on content grounds; the previous backdrop is unchanged.",
        refusal.provider, refusal.model_name
    )
}

/// Tell the operator the Lantern's painter refused the backdrop (v4
/// `postLanternRefusalNotification`, `ce2f1dabf`).
///
/// **Not gated by the image-alert setting**: that setting decides whether
/// *characters* are shown new pictures, and this is a report to the operator
/// — the Salon's "Try uncensored" button rides on it (identified by
/// `systemKind: 'background-refused'`). It replaces the Concierge's own
/// `refusal-*` bubble for the Lantern (one bubble per refusal — the job hands
/// the chokepoint `announce_unresolved_refusal: false`). Never fails: a
/// missing chat is a DEBUG and `None`, a write failure an ERROR and `None`.
pub async fn post_lantern_refusal_notification(
    db: &Db,
    chat_id: &str,
    refusal: &LanternRefusal,
    route_trail: &[crate::services::route_trail::RouteAttempt],
) -> Option<Value> {
    let failed = |error: String| {
        tracing::error!(
            target: "quilltap::lantern_notification",
            context = "lantern-notifications",
            chat_id = %chat_id,
            error = %error,
            "[LanternNotification] Failed to post background refusal"
        );
    };
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &cid)) {
        Ok(chat) => chat,
        Err(e) => {
            failed(e.to_string());
            return None;
        }
    };
    if chat.is_none() {
        tracing::debug!(
            target: "quilltap::lantern_notification",
            context = "lantern-notifications",
            chat_id = %chat_id,
            "[LanternNotification] Refusal bubble skipped: chat not found"
        );
        return None;
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let mut message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": build_lantern_refusal_content(refusal),
        "opaqueContent": build_lantern_refusal_opaque_content(refusal),
        "attachments": [],
        "createdAt": crate::clock::now_iso(),
        "participantId": Value::Null,
        "systemSender": "lantern",
        "systemKind": LanternRefusal::SYSTEM_KIND,
    });
    if !route_trail.is_empty() {
        message.as_object_mut().unwrap().insert(
            "routeTrail".into(),
            serde_json::to_value(route_trail).unwrap_or(Value::Null),
        );
    }
    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(event) => event,
        Err(e) => {
            failed(e.to_string());
            return None;
        }
    };
    let cid = chat_id.to_string();
    if let Err(e) = db
        .write(move |writers| writers.main().chat_messages().add_message(&cid, &event))
        .await
    {
        failed(e.to_string());
        return None;
    }

    tracing::info!(
        target: "quilltap::lantern_notification",
        context = "lantern-notifications",
        chat_id = %chat_id,
        message_id = %message_id,
        provider = %refusal.provider,
        model_name = %refusal.model_name,
        route_trail_length = route_trail.len(),
        "[LanternNotification] Background refusal posted"
    );
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn alert_gate_chat_wins() {
        let chat = json!({ "alertCharactersOfLanternImages": true });
        let project = json!({ "defaultAlertCharactersOfLanternImages": false });
        assert!(is_lantern_image_alert_enabled(Some(&chat), Some(&project)));
    }

    #[test]
    fn alert_gate_chat_null_falls_to_project() {
        let chat = json!({ "alertCharactersOfLanternImages": null });
        let project = json!({ "defaultAlertCharactersOfLanternImages": true });
        assert!(is_lantern_image_alert_enabled(Some(&chat), Some(&project)));
    }

    #[test]
    fn alert_gate_both_null_is_off() {
        let chat = json!({ "alertCharactersOfLanternImages": null });
        assert!(!is_lantern_image_alert_enabled(Some(&chat), None));
        assert!(!is_lantern_image_alert_enabled(None, None));
    }

    /// v4 `buildLanternRefusalContent` / `…OpaqueContent` (`ce2f1dabf`) — the
    /// em dash is U+2014 and the apostrophe ASCII (the tier-1 rows are in
    /// `post_office_concierge_lantern_suparna`).
    #[test]
    fn refusal_bodies_are_v4_bytes() {
        let r = LanternRefusal {
            provider: "GOOGLE".into(),
            model_name: "imagen-4".into(),
        };
        assert_eq!(
            build_lantern_refusal_content(&r),
            "The Lantern's usual painter (GOOGLE imagen-4) would not take the scene \u{2014} called it improper and downed brushes. The backdrop stays as it was."
        );
        assert_eq!(
            build_lantern_refusal_opaque_content(&r),
            "Story background refused by GOOGLE imagen-4 on content grounds; the previous backdrop is unchanged."
        );
        assert_eq!(LanternRefusal::SYSTEM_KIND, "background-refused");
    }

    #[test]
    fn character_image_body_matches_seam_prefix() {
        let kind = LanternNotificationKind::CharacterImage {
            requester_name: "Friday".into(),
        };
        let body = build_content(&kind, None, "abc-123");
        assert_eq!(
            body,
            "The Lantern, acting upon the instructions of Friday, has produced the following picture, catalogued under uuid `abc-123`. It is attached here, should anyone care to examine it."
        );
    }
}
