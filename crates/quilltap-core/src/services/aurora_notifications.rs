//! Aurora notification writers (W4.6b) — the POST half of v4
//! `lib/services/aurora-notifications/{core-whisper,writer}.ts` + the per-turn
//! wardrobe-announcement drain (v4 `lib/tools/handlers/wardrobe-handler-shared.ts`
//! `flushPendingWardrobeAnnouncements`) and its debounced job handler (v4
//! `lib/background-jobs/handlers/wardrobe-announcement.ts`).
//!
//! Aurora is the personified wardrobe/appearance system. Three families live
//! here, all writing `systemSender: 'aurora'` ASSISTANT messages through the
//! ported [`crate::db::chats_messages`] insert marshaling, all error-swallowing
//! (a failure never blocks turn processing — v4's try/catch → `null`):
//!
//! 1. **The Core whisper** ([`post_core_whisper`], `systemKind: 'core-whisper'`,
//!    always targeted to the responding character's participant id). The
//!    config/packet/content builders (`resolveCoreWhisperConfig` /
//!    `assembleCorePacket` / `buildCoreWhisper*`) are already ported in
//!    [`crate::services::core_whisper`] — this is the POST only. It closes
//!    [`crate::services::build_context::BuildContextSeams::post_core_whisper`]
//!    (the parent wires the `Db` at the composition point).
//!
//! 2. **The outfit whispers** ([`post_opening_outfit_whisper`] /
//!    [`post_outfit_change_whisper`], `systemKind: 'opening-outfit'` /
//!    `'outfit-change'`, public — no targeting). Each has a byte-exact
//!    persona-voiced `content` builder and a persona-stripped `opaqueContent`
//!    builder (swapped into an opaque participant's LLM context by the
//!    context-builder).
//!
//! 3. **The wardrobe-announcement job** — a turn's wardrobe edits collect a
//!    per-character set on the tool-execution context; at end-of-turn the
//!    orchestrator drains it through the outfit-change effects module
//!    ([`crate::services::outfit_change_effects::flush_pending_wardrobe_announcements`],
//!    v4 `9ecebf986` moved the drain there), enqueuing one debounced
//!    `WARDROBE_OUTFIT_ANNOUNCEMENT` job per character; the job body
//!    ([`handle_wardrobe_outfit_announcement`]) later resolves the character's
//!    equipped outfit over their wearable pool and posts an `outfit-change`
//!    whisper.

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::jsstr;
use crate::services::resolve_equipped::resolve_equipped_outfit_for_character;
use crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat;
use crate::wardrobe::{describe_outfit, OutfitSlotValues, Slots};
use crate::wearable_pool::{load_wearable_pool, LoadWearablePoolOptions};

// ============================================================================
// Outfit content builders (byte-exact vs v4 `writer.ts`)
// ============================================================================

/// v4 `buildOpeningOutfitContent`. Opening-of-chat outfit announcement,
/// persona-voiced.
pub fn build_opening_outfit_content(character_name: &str, outfit: &OutfitSlotValues) -> String {
    let outfit_text = describe_outfit(outfit);
    [
        format!("Aurora regards {character_name} and pronounces upon their attire:"),
        String::new(),
        String::new(),
        jsstr::js_trim_end(&outfit_text).to_string(),
    ]
    .join("\n")
}

/// v4 `buildOpeningOutfitOpaqueContent`. Persona-stripped opening announcement.
pub fn build_opening_outfit_opaque_content(
    character_name: &str,
    outfit: &OutfitSlotValues,
) -> String {
    let outfit_text = describe_outfit(outfit);
    [
        format!("{character_name}'s current attire:"),
        String::new(),
        jsstr::js_trim_end(&outfit_text).to_string(),
    ]
    .join("\n")
}

/// v4 `buildOutfitChangeContent`. Mid-chat outfit-change announcement,
/// persona-voiced.
pub fn build_outfit_change_content(character_name: &str, outfit: &OutfitSlotValues) -> String {
    let outfit_text = describe_outfit(outfit);
    [
        format!(
            "Aurora marks an alteration to {character_name}'s attire. They are now turned out as follows:"
        ),
        String::new(),
        String::new(),
        jsstr::js_trim_end(&outfit_text).to_string(),
    ]
    .join("\n")
}

/// v4 `buildOutfitChangeOpaqueContent`. Persona-stripped change announcement.
pub fn build_outfit_change_opaque_content(
    character_name: &str,
    outfit: &OutfitSlotValues,
) -> String {
    let outfit_text = describe_outfit(outfit);
    [
        format!("{character_name} is now wearing:"),
        String::new(),
        jsstr::js_trim_end(&outfit_text).to_string(),
    ]
    .join("\n")
}

// ============================================================================
// The shared post primitive (v4 `postAuroraMessage` + `postCoreWhisper`'s inline
// insert — one marshaling site so every Aurora row is byte-identical)
// ============================================================================

/// Persist an Aurora `systemSender: 'aurora'` ASSISTANT message. Mirrors v4
/// `postAuroraMessage` (and the byte-identical inline insert inside
/// `postCoreWhisper`): the `!content.trim()` early return, the `findById`
/// existence guard, the `targetParticipantIds && length>0 ? … : null` gate, and
/// the try/catch → `null`. Returns the posted `MessageEvent` JSON on success (so
/// the caller can splice it into the current turn's in-memory context) or `None`
/// on skip/failure.
async fn post_aurora_message(
    db: &Db,
    chat_id: &str,
    content: &str,
    opaque_content: Option<&str>,
    kind: &str,
    target_participant_ids: Option<Vec<String>>,
) -> Option<Value> {
    // v4 `if (!content || content.trim().length === 0) return null;`
    if content.is_empty() || jsstr::js_trim(content).is_empty() {
        return None;
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4 `targetParticipantIds && targetParticipantIds.length > 0 ? … : null`.
    let targets: Option<Vec<String>> = match target_participant_ids {
        Some(t) if !t.is_empty() => Some(t),
        _ => None,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": null,
        "systemSender": "aurora",
        "systemKind": kind,
        "targetParticipantIds": targets,
    });

    // v4 `ChatEventSchema.parse` before insert; a shape that fails validation
    // would have thrown → caught → null.
    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;

    let cid = chat_id.to_string();
    let posted: Result<bool, DbError> = db
        .write(move |w| {
            let main = w.main();
            // v4 `const chat = await repos.chats.findById(chatId); if (!chat) return null;`
            if chats_read::find_by_id(main.connection(), &cid)?.is_none() {
                return Ok(false);
            }
            main.chat_messages().add_message(&cid, &event)?;
            Ok(true)
        })
        .await;

    match posted {
        Ok(true) => Some(message),
        // v4 catches a persistence failure, logs, and returns null; a missing
        // chat is the same null.
        _ => None,
    }
}

// ============================================================================
// The Core whisper POST (closes `BuildContextSeams::post_core_whisper`)
// ============================================================================

/// v4 `postCoreWhisper` (the POST half). Persists Aurora's per-character Core
/// re-offering as an ASSISTANT message tagged `systemSender: 'aurora'` /
/// `systemKind: 'core-whisper'`, targeted to the responding character's
/// participant id (always set — single-character chats also target their one
/// participant), carrying the persona-voiced `content` and persona-stripped
/// `opaque_content` (both already built by [`crate::services::core_whisper`]).
/// Returns the posted message JSON or `None` (empty content / missing chat /
/// persistence failure — v4's null).
pub async fn post_core_whisper(
    db: &Db,
    chat_id: &str,
    target_participant_id: &str,
    content: &str,
    opaque_content: &str,
) -> Option<Value> {
    post_aurora_message(
        db,
        chat_id,
        content,
        Some(opaque_content),
        "core-whisper",
        Some(vec![target_participant_id.to_string()]),
    )
    .await
}

// ============================================================================
// The outfit whisper POSTs
// ============================================================================

/// v4 `postOpeningOutfitWhisper`. Public (`systemKind: 'opening-outfit'`, no
/// targeting).
pub async fn post_opening_outfit_whisper(
    db: &Db,
    chat_id: &str,
    character_name: &str,
    outfit: &OutfitSlotValues,
) -> Option<Value> {
    let content = build_opening_outfit_content(character_name, outfit);
    let opaque = build_opening_outfit_opaque_content(character_name, outfit);
    post_aurora_message(db, chat_id, &content, Some(&opaque), "opening-outfit", None).await
}

/// v4 `postOutfitChangeWhisper`. Public (`systemKind: 'outfit-change'`, no
/// targeting).
pub async fn post_outfit_change_whisper(
    db: &Db,
    chat_id: &str,
    character_name: &str,
    outfit: &OutfitSlotValues,
) -> Option<Value> {
    let content = build_outfit_change_content(character_name, outfit);
    let opaque = build_outfit_change_opaque_content(character_name, outfit);
    post_aurora_message(db, chat_id, &content, Some(&opaque), "outfit-change", None).await
}

// ============================================================================
// The wardrobe-announcement job handler (the per-turn drain lives in
// `services::outfit_change_effects` — v4 `9ecebf986` moved it to
// `outfit-change-effects.ts`)
// ============================================================================

/// v4 `handleWardrobeOutfitAnnouncement` (`handlers/wardrobe-announcement.ts`),
/// the debounced job body. Logs v4's start INFO (with the job id), reads the
/// chat's `equippedOutfit[characterId]` slots (skipping when the chat is gone or
/// the character has no truthy slot value), validates them through
/// `EquippedSlotsSchema` (a failure WARNs with the first three issues and
/// skips), resolves the equipped outfit across the project tier, reads the
/// character name, and posts an `outfit-change` whisper.
///
/// Takes the job's id (`job.id`) and `payload` (`{ chatId, characterId }`);
/// [`WardrobeOutfitAnnouncementHandler`] is the runner-registry wiring.
pub async fn handle_wardrobe_outfit_announcement(
    db: &Db,
    job_id: &str,
    payload: &Value,
) -> Result<(), DbError> {
    let chat_id = payload
        .get("chatId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let character_id = payload
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // P4.170 B.2: v4's first statement, before any read.
    tracing::info!(
        context = "background-jobs.wardrobe-announcement",
        jobId = %job_id,
        chatId = %chat_id,
        characterId = %character_id,
        "[WardrobeAnnouncement] Starting announcement"
    );
    if chat_id.is_empty() {
        return Ok(());
    }

    let cid = chat_id.clone();
    let char_id = character_id.clone();
    // Everything needing BOTH connections (chat read, project-mount resolution,
    // outfit resolution over the vault, the overlaid character name) runs in one
    // writer closure that holds both — the `wardrobe_write` precedent.
    // `Err(issues)` = the slot shape failed validation: the WARN is logged on
    // the CALLER's thread, once the write returns (a writer-thread line is
    // invisible to the capture rig, and the order is v4's either way).
    let resolved: Result<Option<(OutfitSlotValues, String)>, Vec<Value>> = db
        .write(move |w| {
            let Some(mount_w) = w.mount_index() else {
                return Ok(Ok(None));
            };
            let mount = mount_w.connection();
            let main = w.main().connection();

            // v4 `const chat = await repos.chats.findById(chatId); if (!chat) return;`
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(Ok(None));
            };

            // v4 `const rawSlots = (chat.equippedOutfit ?? {})[characterId];
            // if (rawSlots) { EquippedSlotsSchema.safeParse(rawSlots) … }` — a
            // FALSY value (absent, null, 0, '') skips silently; anything else
            // is parsed, and a failure WARNs with the first three issues and
            // skips (P4.170 B.2).
            let raw_slots = chat.get("equippedOutfit").and_then(|e| e.get(&char_id));
            if !crate::inventory::types::js_truthy(raw_slots) {
                return Ok(Ok(None));
            }
            let raw_slots = raw_slots.expect("truthy is present");
            let issues = equipped_slots_issues(raw_slots);
            if !issues.is_empty() {
                return Ok(Err(issues.iter().take(3).map(|i| i.to_value()).collect()));
            }
            let slots = Slots::from_value(Some(raw_slots));

            // v4 `resolveProjectMountPointIds(chat.projectId)` — the chat we just
            // loaded carries `projectId`, so the chat-keyed resolver yields the
            // identical link set.
            let project_mounts = resolve_project_mount_point_ids_for_chat(main, mount, &cid);
            // v4 `resolveEquippedOutfitForCharacter(await loadWearablePool(repos,
            // characterId, projectMountPointIds), slots).outfitValues`
            // (`9ecebf986`) — the pool's group tier is keyed on THIS
            // character's own memberships.
            let pool = load_wearable_pool(
                main,
                mount,
                &char_id,
                Some(&project_mounts),
                LoadWearablePoolOptions::default(),
            );
            let outfit = resolve_equipped_outfit_for_character(&pool, &slots).outfit_values;

            // v4 `const character = await repos.characters.findById(characterId);
            // const charName = character?.name ?? 'A character';`
            let name = characters_read::find_by_id(main, mount, &char_id)?
                .and_then(|c| c.get("name").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "A character".to_string());

            Ok(Ok(Some((outfit, name))))
        })
        .await?;

    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(first) => {
            tracing::warn!(
                context = "background-jobs.wardrobe-announcement",
                chatId = %chat_id,
                characterId = %character_id,
                issuesJson = %serde_json::Value::Array(first),
                "[WardrobeAnnouncement] equippedOutfit slot shape failed validation, skipping"
            );
            return Ok(());
        }
    };
    let Some((outfit, char_name)) = resolved else {
        return Ok(());
    };
    post_outfit_change_whisper(db, &chat_id, &char_name, &outfit).await;
    Ok(())
}

/// v4 `EquippedSlotsSchema.safeParse(rawSlots)` (`wardrobe.types.ts:310-318`)
/// — the issues, in Zod's order: a non-object is ONE `invalid_type` at the
/// root; else each of the five slots in shape order, each `z.array(UUIDSchema)
/// .default([])` (absent → the default; `null` or a non-array → `invalid_type`
/// array; per element a non-string → `invalid_type` string, a non-UUID string
/// → `invalid_format` uuid). Unknown keys are stripped (no `.strict()`).
fn equipped_slots_issues(raw: &Value) -> Vec<crate::api::zod_issues::ZodIssue> {
    use crate::api::zod_issues::{zod_uuid_ok, ZodIssue};
    let Some(obj) = raw.as_object() else {
        return vec![ZodIssue::invalid_type("object", vec![], Some(raw))];
    };
    let mut issues = Vec::new();
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        let Some(v) = obj.get(slot) else { continue };
        let Some(items) = v.as_array() else {
            issues.push(ZodIssue::invalid_type("array", vec![json!(slot)], Some(v)));
            continue;
        };
        for (i, item) in items.iter().enumerate() {
            let at = vec![json!(slot), json!(i)];
            match item.as_str() {
                Some(s) if zod_uuid_ok(s) => {}
                Some(_) => issues.push(ZodIssue::invalid_uuid(at)),
                None => issues.push(ZodIssue::invalid_type("string", at, Some(item))),
            }
        }
    }
    issues
}

/// The [`crate::services::job_runner::JobHandler`] for `WARDROBE_OUTFIT_ANNOUNCEMENT`
/// jobs — the runner-registry wiring for [`handle_wardrobe_outfit_announcement`].
/// The host registers it via `registry.register("WARDROBE_OUTFIT_ANNOUNCEMENT",
/// Box::new(WardrobeOutfitAnnouncementHandler))`.
pub struct WardrobeOutfitAnnouncementHandler;

impl crate::services::job_runner::JobHandler for WardrobeOutfitAnnouncementHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            match handle_wardrobe_outfit_announcement(db, &job.id, &payload).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e.to_string()),
            }
        })
    }
}
