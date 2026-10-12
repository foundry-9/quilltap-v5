//! Outfit-change side effects — what follows any change to what a character
//! has on in a chat (v4 `lib/wardrobe/outfit-change-effects.ts`, P4.D279
//! keystone C1 §7).
//!
//! Two effects, and every surface that changes an outfit fires them here:
//!
//! - the character's per-chat avatar is refreshed, when the chat has opted in
//!   ([`trigger_avatar_generation_if_enabled`]);
//! - Aurora's wardrobe announcement is queued. Inside a model turn the
//!   orchestrator hands the tools a per-turn `pending_wardrobe_announcements`
//!   set, so N wardrobe edits in one response collapse into one announcement,
//!   drained at turn end by [`flush_pending_wardrobe_announcements`]. Without
//!   the set (the operator's `?action=equip`) the announcement is enqueued at
//!   once; the job itself is debounced.
//!
//! The opening of a chat and a seat joining mid-chat refresh the avatar only
//! ([`refresh_avatar_for_outfit`]) — the Host's opening whisper says what
//! everyone is wearing there.
//!
//! The per-turn set is v4's `Set<string>`; v5 carries it as the tool context's
//! `Arc<Mutex<HashSet<String>>>` (`services::tool_execution`), shared by
//! reference exactly as v4 shares the Set. Every function here never fails:
//! v4 declares each `Promise<void>` and catches inside.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::db::runtime::Db;
use crate::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use crate::services::queue_service::enqueue_wardrobe_outfit_announcement;

/// Who changed whose outfit, and the turn's announcement set if there is one
/// (v4 `OutfitChangeContext`).
#[derive(Clone, Copy, Debug)]
pub struct OutfitChangeContext<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
    /// Per-turn announcement queue the orchestrator threads through the tools
    /// (v4's optional `pendingWardrobeAnnouncements?: Set<string>`).
    pub pending_wardrobe_announcements: Option<&'a Arc<Mutex<HashSet<String>>>>,
}

/// Refresh the character's per-chat avatar if the chat auto-updates avatars.
/// Never fails (v4 `refreshAvatarForOutfit`).
///
/// `source_context` is v4's `sourceContext`, handed on as the trigger's
/// `callerContext` — read only as the `context` field of its WARN lines, never
/// written into the job. It is `&'static str` because every v4 call site
/// passes a literal and [`AvatarGenerationParams::caller_context`] is typed so.
pub async fn refresh_avatar_for_outfit(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    source_context: &'static str,
) {
    trigger_avatar_generation_if_enabled(
        db,
        &AvatarGenerationParams {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
            character_id: character_id.to_string(),
            caller_context: source_context,
            image_profile_id_override: None,
            equipped_slots_override: None,
            force: false,
        },
    )
    .await;
}

/// Enqueue one Aurora announcement now. Never fails (v4
/// `scheduleWardrobeAnnouncement`).
///
/// v4 wraps the enqueue in `try/catch`: success logs DEBUG `Wardrobe outfit
/// announcement scheduled` (a reused still-PENDING job counts as scheduled —
/// `enqueueWardrobeOutfitAnnouncement` resolves either way), any throw logs
/// WARN `Failed to schedule wardrobe outfit announcement` with the bare error
/// message. v5's enqueue answers a `Result`, so its `Err` is that catch.
pub async fn schedule_wardrobe_announcement(
    db: &Db,
    source_context: &str,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
) {
    match enqueue_wardrobe_outfit_announcement(db, user_id, chat_id, character_id).await {
        Ok(_) => {
            tracing::debug!(
                context = source_context,
                chatId = chat_id,
                characterId = character_id,
                "Wardrobe outfit announcement scheduled"
            );
        }
        Err(error) => {
            tracing::warn!(
                context = source_context,
                chatId = chat_id,
                characterId = character_id,
                error = %crate::db::fallback::error_text(&error),
                "Failed to schedule wardrobe outfit announcement"
            );
        }
    }
}

/// Note that a character's outfit changed. With a turn's announcement set the
/// character joins it; without one the announcement is enqueued now (v4
/// `recordPendingWardrobeAnnouncement`).
pub async fn record_pending_wardrobe_announcement(
    db: &Db,
    ctx: &OutfitChangeContext<'_>,
    source_context: &str,
) {
    if let Some(pending) = ctx.pending_wardrobe_announcements {
        // A poisoned lock is another thread's panic mid-insert; v4's Set
        // cannot be in that state, and the announcement must still be made —
        // recover the guard rather than drop the character on the floor.
        pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(ctx.character_id.to_string());
        return;
    }
    schedule_wardrobe_announcement(
        db,
        source_context,
        ctx.user_id,
        ctx.chat_id,
        ctx.character_id,
    )
    .await;
}

/// Both effects of an outfit change. Call ONCE per gesture (or per tool call,
/// however many operations it applied), and only when something landed (v4
/// `notifyWardrobeChanged`): the avatar refresh first, then the announcement.
pub async fn notify_wardrobe_changed(
    db: &Db,
    ctx: &OutfitChangeContext<'_>,
    source_context: &'static str,
) {
    refresh_avatar_for_outfit(
        db,
        ctx.user_id,
        ctx.chat_id,
        ctx.character_id,
        source_context,
    )
    .await;
    record_pending_wardrobe_announcement(db, ctx, source_context).await;
}

/// Drain the turn's announcement set, one announcement per character.
/// Idempotent: the set is cleared, so a second drain in the same turn is a
/// no-op (v4 `flushPendingWardrobeAnnouncements`). A missing or empty set is a
/// no-op too.
///
/// v4 iterates its `Set` in insertion order; v5's set is an unordered
/// [`HashSet`] (the recorded W4.1d2 decision — the tool context carries a
/// `HashSet`, not an ordered set), so the drained ids are SORTED for a
/// deterministic enqueue order. Each enqueue is independent and deduped per
/// character, so the order changes neither the set of jobs produced nor any
/// job's payload — only the order of the `scheduled` DEBUG lines.
///
/// The set is drained and released under the lock BEFORE any enqueue — v4
/// clears the Set before its first `await` for the same reason: a wardrobe
/// tool running on the same turn while a flush is mid-flight must land in a
/// fresh set, not be dropped by a clear that follows the enqueues.
pub async fn flush_pending_wardrobe_announcements(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    pending: Option<&Arc<Mutex<HashSet<String>>>>,
) {
    let Some(pending) = pending else {
        return;
    };
    let mut character_ids: Vec<String> = {
        let mut guard = pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_empty() {
            return;
        }
        guard.drain().collect()
    };
    character_ids.sort();
    for character_id in character_ids {
        schedule_wardrobe_announcement(
            db,
            "orchestrator-turn-end",
            user_id,
            chat_id,
            &character_id,
        )
        .await;
    }
}
