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
//! 3. **The wardrobe-announcement drain** — a turn's wardrobe edits collect a
//!    per-character set on the tool-execution context; at end-of-turn the
//!    orchestrator drains it ([`flush_pending_wardrobe_announcements`]),
//!    enqueuing one debounced `WARDROBE_OUTFIT_ANNOUNCEMENT` job per character
//!    (via [`crate::services::queue_service::enqueue_wardrobe_outfit_announcement`]);
//!    the job body ([`handle_wardrobe_outfit_announcement`]) later resolves the
//!    character's equipped outfit and posts an `outfit-change` whisper.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::jsstr;
use crate::services::queue_service::enqueue_wardrobe_outfit_announcement;
use crate::tools::wardrobe_shared::{
    resolve_equipped_outfit_values, resolve_project_mount_point_ids_for_chat,
};
use crate::wardrobe::{describe_outfit, OutfitSlotValues, Slots};

// ============================================================================
// Outfit content builders (byte-exact vs v4 `writer.ts`)
// ============================================================================

/// v4 `buildOpeningOutfitContent`. Opening-of-chat outfit announcement,
/// persona-voiced.
pub fn build_opening_outfit_content(character_name: &str, outfit: &OutfitSlotValues) -> String {
    let outfit_text = describe_outfit(outfit);
    [
        format!("*Aurora regards {character_name} and pronounces upon their attire —*"),
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
            "*Aurora marks an alteration to {character_name}'s attire. They are now turned out as follows —*"
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
// The wardrobe-announcement drain + job handler
// ============================================================================

/// v4 `flushPendingWardrobeAnnouncements` (`wardrobe-handler-shared.ts`). Drain
/// the per-turn `pending_wardrobe_announcements` set, enqueuing one debounced
/// `WARDROBE_OUTFIT_ANNOUNCEMENT` job per character (collapsing N wardrobe edits
/// in a single LLM response into one announcement per character). Idempotent —
/// the set is emptied under the lock before any enqueue. Safe when the set is
/// missing/empty (no-op). A single enqueue failure is swallowed (v4
/// `scheduleWardrobeAnnouncement`'s try/catch → warn) so it never blocks the
/// others.
///
/// v4 iterates the `Set` in insertion order; the v5 set is an (unordered)
/// [`HashSet`] (a W4.1d2 decision — the legacy "no set → enqueue immediately"
/// fallback is likewise not ported), so the drained ids are sorted for a
/// deterministic enqueue order. Each enqueue is independent and
/// per-character-deduped, so order does not change the set of jobs produced.
pub async fn flush_pending_wardrobe_announcements(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    pending: &Arc<Mutex<HashSet<String>>>,
) {
    let mut character_ids: Vec<String> = {
        let Ok(mut guard) = pending.lock() else {
            return;
        };
        if guard.is_empty() {
            return;
        }
        guard.drain().collect()
    };
    character_ids.sort();
    for character_id in character_ids {
        // v4 wraps each enqueue in try/catch → warn: a scheduling failure for one
        // character must not strand the others.
        let _ = enqueue_wardrobe_outfit_announcement(db, user_id, chat_id, &character_id).await;
    }
}

/// v4 `handleWardrobeOutfitAnnouncement` (`handlers/wardrobe-announcement.ts`),
/// the debounced job body. Reads the chat's `equippedOutfit[characterId]` slots
/// (skipping silently when the chat is gone, the character has no equipped slots,
/// or the stored slot shape is not a JSON object — v4's `EquippedSlotsSchema.
/// safeParse` failure branch), resolves the equipped outfit across the project
/// tier, reads the character name, and posts an `outfit-change` whisper.
///
/// Ported as a plain public function taking the job `payload` (`{ chatId,
/// characterId }`); the job-runner dispatch row that registers it for the
/// `WARDROBE_OUTFIT_ANNOUNCEMENT` type is owned by W4.8 (see the module note).
pub async fn handle_wardrobe_outfit_announcement(db: &Db, payload: &Value) -> Result<(), DbError> {
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
    if chat_id.is_empty() {
        return Ok(());
    }

    let cid = chat_id.clone();
    let char_id = character_id.clone();
    // Everything needing BOTH connections (chat read, project-mount resolution,
    // outfit resolution over the vault, the overlaid character name) runs in one
    // writer closure that holds both — the `wardrobe_write` precedent.
    let resolved: Option<(OutfitSlotValues, String)> = db
        .write(move |w| {
            let Some(mount_w) = w.mount_index() else {
                return Ok(None);
            };
            let mount = mount_w.connection();
            let main = w.main().connection();

            // v4 `const chat = await repos.chats.findById(chatId); if (!chat) return;`
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };

            // v4 `const rawSlots = (chat.equippedOutfit ?? {})[characterId];`
            // A missing/null value → skip; a non-object → `EquippedSlotsSchema.
            // safeParse` fails → skip (per-element UUID validation is a documented
            // minor seam — `Slots::from_value` is lenient there; the corpus feeds
            // only validated slot shapes).
            let Some(raw_slots) = chat.get("equippedOutfit").and_then(|e| e.get(&char_id)) else {
                return Ok(None);
            };
            if !raw_slots.is_object() {
                return Ok(None);
            }
            let slots = Slots::from_value(Some(raw_slots));

            // v4 `resolveProjectMountPointIds(chat.projectId)` — the chat we just
            // loaded carries `projectId`, so the chat-keyed resolver yields the
            // identical link set.
            let project_mounts = resolve_project_mount_point_ids_for_chat(main, mount, &cid);
            let docs = DocMountDocumentsRepository::new(mount);
            let outfit =
                resolve_equipped_outfit_values(main, &docs, &char_id, &slots, &project_mounts)?;

            // v4 `const character = await repos.characters.findById(characterId);
            // const charName = character?.name ?? 'A character';`
            let name = characters_read::find_by_id(main, mount, &char_id)?
                .and_then(|c| c.get("name").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "A character".to_string());

            Ok(Some((outfit, name)))
        })
        .await?;

    let Some((outfit, char_name)) = resolved else {
        return Ok(());
    };
    post_outfit_change_whisper(db, &chat_id, &char_name, &outfit).await;
    Ok(())
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
            match handle_wardrobe_outfit_announcement(db, &payload).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e.to_string()),
            }
        })
    }
}
