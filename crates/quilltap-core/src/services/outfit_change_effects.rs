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
//! Beside each announcement the outfit history is settled (P4.D316, v4
//! `05005c562`): one entry per announcement, never per slot gesture. Inside a
//! turn the intent waits in `pending_wardrobe_history` (last intent per
//! character wins) and is drained with the announcements; outside one it is
//! settled at once. A rollback sets `history_intent` to move the cursor
//! instead of appending. Create both turn containers with
//! [`new_wardrobe_turn_containers`].
//!
//! The per-turn set is v4's `Set<string>`; v5 carries it as the tool context's
//! `Arc<Mutex<HashSet<String>>>` (`services::tool_execution`), shared by
//! reference exactly as v4 shares the Set; the history map is v4's
//! `Map<characterId, intent>` the same way. v4's `turn-containers.ts` is a
//! separate module only so call sites can thread the containers under jest
//! mocks of this one — a file boundary v5 has no reason for, so the factory
//! lives here (NO-PORT as a file; the factory is the contract). Every function
//! here never fails: v4 declares each `Promise<void>` and catches inside.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::db::chat_wardrobe_history::{
    settle_db, WardrobeHistoryIntent, WardrobeHistoryProvenance, WardrobeHistorySource,
};
use crate::db::runtime::Db;
use crate::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use crate::services::inventory_change_effects::PendingInventoryAnnouncements;
use crate::services::queue_service::enqueue_wardrobe_outfit_announcement;

/// Per-turn announcement queue the orchestrator threads through the tools
/// (v4's optional `pendingWardrobeAnnouncements?: Set<string>`).
pub type PendingWardrobeAnnouncements = Arc<Mutex<HashSet<String>>>;

/// Per-turn history intents, created beside the set. Last intent per character
/// wins (v4's optional `pendingWardrobeHistory?: Map<string,
/// WardrobeHistoryIntent>`).
pub type PendingWardrobeHistory = Arc<Mutex<HashMap<String, WardrobeHistoryIntent>>>;

/// The three per-turn containers, created together (v4
/// `newWardrobeTurnContainers`): the announcement set, the history intents,
/// and beside them the Inventory's pending changes (P4.D328, v4 `e1872c94f`
/// — drained by `inventory_change_effects::flush_pending_inventory_announcements`
/// AFTER the saved turn). Every surface that runs a model turn's tools mints
/// them HERE — never a set by hand (v4's outfit-history fence 4: a set
/// without its map would settle each change at once instead of at turn end).
pub fn new_wardrobe_turn_containers() -> (
    PendingWardrobeAnnouncements,
    PendingWardrobeHistory,
    PendingInventoryAnnouncements,
) {
    (
        Arc::new(Mutex::new(HashSet::new())),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::new(Mutex::new(Default::default())),
    )
}

/// Who changed whose outfit, and the turn's containers if there are any (v4
/// `OutfitChangeContext extends WardrobeTurnContainers`). The two containers
/// are INDEPENDENTLY optional — a set without its map is a legal, handled
/// state ([`record_pending_wardrobe_announcement`]).
#[derive(Clone, Copy, Debug)]
pub struct OutfitChangeContext<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
    /// Who is changing the outfit. Required: no caller may forget it.
    pub provenance: &'a WardrobeHistoryProvenance,
    /// Rollback only: move the cursor instead of appending. Everyone else
    /// leaves it unset.
    pub history_intent: Option<&'a WardrobeHistoryIntent>,
    /// Per-turn announcement queue the orchestrator threads through the tools.
    pub pending_wardrobe_announcements: Option<&'a PendingWardrobeAnnouncements>,
    /// Per-turn history intents, created beside the set.
    pub pending_wardrobe_history: Option<&'a PendingWardrobeHistory>,
}

/// Settle one history intent (v4's private `settleHistory`). The repository's
/// `settle` itself never fails (its own savepoint + WARN); v5's after-commit
/// form can still answer `Err` when the writer is gone, and that is this
/// guard's WARN — never propagated. (v4 reaches its twin only on a missing
/// repository, which its real container never is: the line is v5-only
/// reachable.)
async fn settle_history(
    db: &Db,
    chat_id: &str,
    character_id: &str,
    intent: &WardrobeHistoryIntent,
    source_context: &str,
) {
    if let Err(error) = settle_db(db, chat_id, character_id, intent).await {
        tracing::warn!(
            context = source_context,
            chatId = chat_id,
            characterId = character_id,
            intentJson = %serde_json::to_string(intent).unwrap_or_default(),
            error = %crate::db::fallback::error_text(&error),
            "Failed to settle wardrobe history"
        );
    }
}

/// Whose per-chat avatar to refresh (v4's `Pick<OutfitChangeContext, 'userId'
/// | 'chatId' | 'characterId'>`).
#[derive(Clone, Copy, Debug)]
pub struct AvatarRefreshWho<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
}

/// Refresh the character's per-chat avatar if the chat auto-updates avatars:
/// the character's appearance changed — their outfit, or what they carry (the
/// Inventory calls this too). Never fails (v4 `refreshAvatarForAppearance`,
/// `4fbdad186`).
///
/// `source_context` is v4's `sourceContext`, handed on as the trigger's
/// `callerContext` — read only as the `context` field of its WARN lines, never
/// written into the job. It is `&'static str` because every v4 call site
/// passes a literal (`'inventory-change'` included) and
/// [`AvatarGenerationParams::caller_context`] is typed so.
pub async fn refresh_avatar_for_appearance(
    db: &Db,
    who: &AvatarRefreshWho<'_>,
    source_context: &'static str,
) {
    trigger_avatar_generation_if_enabled(db, &avatar_refresh_params(who, source_context)).await;
}

/// The trigger's parameters for a refresh — ONE body under both names.
fn avatar_refresh_params(
    who: &AvatarRefreshWho<'_>,
    source_context: &'static str,
) -> AvatarGenerationParams {
    AvatarGenerationParams {
        user_id: who.user_id.to_string(),
        chat_id: who.chat_id.to_string(),
        character_id: who.character_id.to_string(),
        caller_context: source_context,
        image_profile_id_override: None,
        equipped_slots_override: None,
        force: false,
    }
}

/// The outfit's name for [`refresh_avatar_for_appearance`] (v4 `export const
/// refreshAvatarForOutfit = refreshAvatarForAppearance` — one function, two
/// names), kept with its old positional signature so every outfit caller
/// compiles unchanged.
pub async fn refresh_avatar_for_outfit(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    source_context: &'static str,
) {
    refresh_avatar_for_appearance(
        db,
        &AvatarRefreshWho {
            user_id,
            chat_id,
            character_id,
        },
        source_context,
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

/// Note that a character's outfit changed (v4
/// `recordPendingWardrobeAnnouncement`). With a turn's containers the
/// character joins the announcement set and its history intent replaces any
/// earlier one this turn; without them the announcement is enqueued and the
/// history settled now — the announcement first, then the entry, neither
/// waiting on the other's success. (The turn-end drain runs the OTHER order —
/// settle, then announce; v4 pins both.)
pub async fn record_pending_wardrobe_announcement(
    db: &Db,
    ctx: &OutfitChangeContext<'_>,
    source_context: &str,
) {
    let intent = ctx
        .history_intent
        .cloned()
        .unwrap_or_else(|| WardrobeHistoryIntent::Append {
            provenance: ctx.provenance.clone(),
        });
    if let Some(pending) = ctx.pending_wardrobe_announcements {
        // A poisoned lock is another thread's panic mid-insert; v4's Set
        // cannot be in that state, and the announcement must still be made —
        // recover the guard rather than drop the character on the floor.
        pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(ctx.character_id.to_string());
        if let Some(history) = ctx.pending_wardrobe_history {
            history
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(ctx.character_id.to_string(), intent);
        } else {
            // A turn set without its history map (a caller that predates the
            // factory): record now rather than lose the entry.
            settle_history(db, ctx.chat_id, ctx.character_id, &intent, source_context).await;
        }
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
    settle_history(db, ctx.chat_id, ctx.character_id, &intent, source_context).await;
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

/// Drain the turn's containers: for each character, settle its history
/// intent, then schedule its one announcement (v4
/// `flushPendingWardrobeAnnouncements`). Idempotent: both containers are
/// cleared, so a second drain in the same turn is a no-op. A missing or EMPTY
/// set is a no-op too — and leaves a non-empty map INTACT (v4 returns before
/// its clears).
///
/// v4 iterates its `Set` in insertion order; v5's set is an unordered
/// [`HashSet`] (the recorded W4.1d2 decision — the tool context carries a
/// `HashSet`, not an ordered set), so the drained ids are SORTED for a
/// deterministic order. Each settle and each enqueue is independent and keyed
/// per character, so the order changes neither the rows nor the jobs produced
/// — only the order of the per-character lines and realtime publishes.
///
/// Both containers are drained and released under their locks BEFORE any
/// await — v4 clears both before its first `await` for the same reason: a
/// wardrobe tool running on the same turn while a flush is mid-flight must
/// land in fresh containers, not be dropped by a clear that follows.
pub async fn flush_pending_wardrobe_announcements(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    pending: Option<&PendingWardrobeAnnouncements>,
    history: Option<&PendingWardrobeHistory>,
) {
    let Some(pending) = pending else {
        return;
    };
    let (mut character_ids, mut intents): (Vec<String>, HashMap<String, WardrobeHistoryIntent>) = {
        let mut guard = pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_empty() {
            return;
        }
        let ids = guard.drain().collect();
        let intents = match history {
            Some(history) => history
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .drain()
                .collect(),
            None => HashMap::new(),
        };
        (ids, intents)
    };
    character_ids.sort();
    for character_id in character_ids {
        // Every character in the set has an intent; the default keeps the
        // drain total if one somehow does not.
        let intent =
            intents
                .remove(&character_id)
                .unwrap_or_else(|| WardrobeHistoryIntent::Append {
                    provenance: WardrobeHistoryProvenance::new(WardrobeHistorySource::Character)
                        .with_actor(character_id.clone()),
                });
        settle_history(db, chat_id, &character_id, &intent, "orchestrator-turn-end").await;
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

#[cfg(test)]
mod tests {
    //! v4 `outfit-change-effects.test.ts` (`05005c562`), every `it` the order
    //! names, over a bare main DB: a `chats(id, equippedOutfit)` table, the
    //! keystone's `chat_wardrobe_history` (its one test plant) and no
    //! `background_jobs` — so every schedule takes its WARN leg, on the caller
    //! thread, which is where the ORDER of the two halves is observable. (The
    //! repository's own lines run on the writer thread, invisible to the
    //! thread-scoped capture; the rows are the proof there.)
    use super::*;
    use crate::db::chat_wardrobe_history::ChatWardrobeHistoryRepository;
    use crate::test_support::global_capture;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c0c0c0c0-0000-4000-8000-000000000001";
    const A: &str = "a0a0a0a0-0000-4000-8000-00000000000a";
    const B: &str = "b0b0b0b0-0000-4000-8000-00000000000b";

    async fn open() -> (Db, tempfile::TempDir) {
        global_capture::install();
        let dir = tempfile::Builder::new()
            .prefix("qt-outfit-effects-unit-")
            .tempdir()
            .unwrap();
        let db = Db::open_main(dir.path().join("main.db"), PEPPER).unwrap();
        // The real `chats` DDL (the D23 dump): the live-outfit read goes
        // through `chats_read`, whose row validation a thin table fails.
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("provisioning/fresh_schema.json")).unwrap();
        let chats_ddl = schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
            .unwrap()
            .to_string();
        db.write(move |w| {
            let c = w.main().connection();
            c.execute_batch(&chats_ddl)?;
            c.execute(
                "INSERT INTO chats (id, userId, title, createdAt, equippedOutfit, updatedAt) VALUES (?1, 'u', 'Effects', '2026-01-01T00:00:00.000Z', ?2, '2026-01-01T00:00:00.000Z')",
                rusqlite::params![
                    CHAT,
                    format!(r#"{{"{A}":{{"top":["dress"]}},"{B}":{{"footwear":["boots"]}}}}"#)
                ],
            )?;
            crate::test_support::ensure_chat_wardrobe_history_on(c);
            Ok(())
        })
        .await
        .unwrap();
        (db, dir)
    }

    fn character(id: &str) -> WardrobeHistoryProvenance {
        WardrobeHistoryProvenance::new(WardrobeHistorySource::Character).with_actor(id)
    }

    fn rows(db: &Db, character_id: &str) -> Vec<(i64, String, Option<String>, bool)> {
        let ch = character_id.to_string();
        db.read_main(move |c| ChatWardrobeHistoryRepository::new(c).find_entries(CHAT, &ch))
            .unwrap()
            .into_iter()
            .map(|r| (r.entry_index, r.source, r.actor_character_id, r.is_current))
            .collect()
    }

    fn ctx<'a>(
        character_id: &'a str,
        provenance: &'a WardrobeHistoryProvenance,
        set: Option<&'a PendingWardrobeAnnouncements>,
        map: Option<&'a PendingWardrobeHistory>,
    ) -> OutfitChangeContext<'a> {
        OutfitChangeContext {
            user_id: "u",
            chat_id: CHAT,
            character_id,
            provenance,
            history_intent: None,
            pending_wardrobe_announcements: set,
            pending_wardrobe_history: map,
        }
    }

    /// Kill the writer thread (a panicking job), so every later write answers
    /// `WriterGone` — the only way v5's after-commit settle can fail.
    async fn kill_writer(db: &Db) {
        let _ = db
            .write(|_| -> Result<(), crate::db::DbError> { panic!("kill the writer (test)") })
            .await;
    }

    fn position(lines: &[String], message: &str) -> usize {
        lines
            .iter()
            .position(|l| l.contains(message))
            .unwrap_or_else(|| panic!("{message} not logged: {lines:#?}"))
    }

    /// [4fbdad186] `refresh_avatar_for_appearance` and its outfit alias build
    /// ONE set of trigger parameters (v4's `refreshAvatarForOutfit =
    /// refreshAvatarForAppearance` — one function, two names).
    #[test]
    fn refresh_avatar_for_appearance_builds_the_outfit_alias_params() {
        let who = AvatarRefreshWho {
            user_id: "u",
            chat_id: CHAT,
            character_id: A,
        };
        let params = avatar_refresh_params(&who, "inventory-change");
        assert_eq!(
            (
                params.user_id.as_str(),
                params.chat_id.as_str(),
                params.character_id.as_str(),
                params.caller_context,
                params.image_profile_id_override.is_none(),
                params.equipped_slots_override.is_none(),
                params.force,
            ),
            ("u", CHAT, A, "inventory-change", true, true, false)
        );
    }

    #[test]
    fn the_factory_makes_both_containers_empty_and_separate() {
        let (set, map, _) = new_wardrobe_turn_containers();
        assert!(set.lock().unwrap().is_empty());
        assert!(map.lock().unwrap().is_empty());
        let (set2, _, _) = new_wardrobe_turn_containers();
        assert!(
            !Arc::ptr_eq(&set, &set2),
            "each call mints fresh containers"
        );
    }

    #[tokio::test]
    async fn record_defers_both_in_a_turn() {
        let (db, _dir) = open().await;
        let (set, map, _) = new_wardrobe_turn_containers();
        let p = character(A);
        record_pending_wardrobe_announcement(&db, &ctx(A, &p, Some(&set), Some(&map)), "t").await;
        assert_eq!(
            set.lock().unwrap().iter().cloned().collect::<Vec<_>>(),
            vec![A.to_string()]
        );
        assert_eq!(
            map.lock().unwrap().get(A),
            Some(&WardrobeHistoryIntent::Append {
                provenance: p.clone()
            })
        );
        assert!(rows(&db, A).is_empty(), "nothing settled inside a turn");
    }

    #[tokio::test]
    async fn record_announces_then_settles_without_containers() {
        let (db, _dir) = open().await;
        let p = character(A);
        let ((), lines) = global_capture::capture_async(record_pending_wardrobe_announcement(
            &db,
            &ctx(A, &p, None, None),
            "wardrobe-wear-handler",
        ))
        .await;
        // The schedule's WARN (no background_jobs) is logged; the settle wrote
        // the first-touch seed (0, empty) and the change (1, the caller's).
        assert!(lines
            .iter()
            .any(|l| l.contains("Failed to schedule wardrobe outfit announcement")));
        assert_eq!(
            rows(&db, A),
            vec![
                (0, "seeded".into(), None, false),
                (1, "character".into(), Some(A.into()), true)
            ]
        );
    }

    #[tokio::test]
    async fn record_carries_a_cursor_intent() {
        let (db, _dir) = open().await;
        let p = character(A);
        record_pending_wardrobe_announcement(&db, &ctx(A, &p, None, None), "t").await;
        let cursor = WardrobeHistoryIntent::Cursor { index: 0 };
        let mut c = ctx(A, &p, None, None);
        c.history_intent = Some(&cursor);
        record_pending_wardrobe_announcement(&db, &c, "t").await;
        assert_eq!(
            rows(&db, A).iter().map(|r| r.3).collect::<Vec<_>>(),
            vec![true, false],
            "the cursor moved back to 0"
        );
        // Inside a turn the cursor intent is what waits in the map.
        let (set, map, _) = new_wardrobe_turn_containers();
        let mut c = ctx(A, &p, Some(&set), Some(&map));
        c.history_intent = Some(&cursor);
        record_pending_wardrobe_announcement(&db, &c, "t").await;
        assert_eq!(map.lock().unwrap().get(A), Some(&cursor));
    }

    /// v5-only reachability: v4's guard WARN fires only on a missing
    /// repository (its real `settle` swallows every failure); v5's after-commit
    /// settle answers `Err` when the writer is gone. The announcement is still
    /// made FIRST, and the settle's WARN follows it with v4's fields.
    #[tokio::test]
    async fn still_announces_when_the_settle_fails_and_warns_after() {
        let (db, _dir) = open().await;
        kill_writer(&db).await;
        let p = character(A);
        let ((), lines) = global_capture::capture_async(record_pending_wardrobe_announcement(
            &db,
            &ctx(A, &p, None, None),
            "wardrobe-wear-handler",
        ))
        .await;
        let schedule = position(&lines, "Failed to schedule wardrobe outfit announcement");
        let settle = position(&lines, "Failed to settle wardrobe history");
        assert!(
            schedule < settle,
            "outside a turn: announce, THEN settle: {lines:#?}"
        );
        let line = &lines[settle];
        assert!(
            line.contains(&format!(
                "Failed to settle wardrobe history context=wardrobe-wear-handler chatId={CHAT} characterId={A} intentJson={{\"kind\":\"append\",\"provenance\":{{\"source\":\"character\",\"actorCharacterId\":\"{A}\"}}}} error="
            )),
            "v4's fields in v4's order: {line}"
        );
    }

    #[tokio::test]
    async fn a_set_without_its_map_settles_at_once() {
        let (db, _dir) = open().await;
        let (set, _, _) = new_wardrobe_turn_containers();
        let p = character(A);
        record_pending_wardrobe_announcement(&db, &ctx(A, &p, Some(&set), None), "t").await;
        assert_eq!(
            set.lock().unwrap().len(),
            1,
            "the announcement still waits for turn end"
        );
        assert_eq!(rows(&db, A).len(), 2, "the history did not wait");
    }

    #[tokio::test]
    async fn the_last_intent_wins_and_characters_stay_separate() {
        let (db, _dir) = open().await;
        let (set, map, _) = new_wardrobe_turn_containers();
        let (pa, pb) = (character(A), character(B));
        let cursor = WardrobeHistoryIntent::Cursor { index: 2 };
        record_pending_wardrobe_announcement(&db, &ctx(A, &pa, Some(&set), Some(&map)), "t").await;
        let mut c = ctx(A, &pa, Some(&set), Some(&map));
        c.history_intent = Some(&cursor);
        record_pending_wardrobe_announcement(&db, &c, "t").await;
        record_pending_wardrobe_announcement(&db, &ctx(B, &pb, Some(&set), Some(&map)), "t").await;
        let map = map.lock().unwrap();
        assert_eq!(
            map.get(A),
            Some(&cursor),
            "A's later cursor replaced its append"
        );
        assert_eq!(
            map.get(B),
            Some(&WardrobeHistoryIntent::Append { provenance: pb })
        );
        assert_eq!(set.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn flush_settles_each_intent_announces_each_and_clears_both() {
        let (db, _dir) = open().await;
        let (set, map, _) = new_wardrobe_turn_containers();
        let operator = WardrobeHistoryProvenance::new(WardrobeHistorySource::Operator);
        record_pending_wardrobe_announcement(&db, &ctx(A, &operator, Some(&set), Some(&map)), "t")
            .await;
        // B joins the set with NO map entry: the drain's default intent names
        // the character itself.
        set.lock().unwrap().insert(B.to_string());
        let ((), lines) = global_capture::capture_async(flush_pending_wardrobe_announcements(
            &db,
            "u",
            CHAT,
            Some(&set),
            Some(&map),
        ))
        .await;
        assert_eq!(rows(&db, A).last().unwrap().1, "operator");
        assert_eq!(
            rows(&db, B).last().unwrap(),
            &(1, "character".into(), Some(B.into()), true)
        );
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("Failed to schedule wardrobe outfit announcement"))
                .count(),
            2,
            "one announcement each"
        );
        assert!(set.lock().unwrap().is_empty() && map.lock().unwrap().is_empty());
        // Idempotent: a second drain is a no-op.
        let ((), again) = global_capture::capture_async(flush_pending_wardrobe_announcements(
            &db,
            "u",
            CHAT,
            Some(&set),
            Some(&map),
        ))
        .await;
        assert!(again.is_empty(), "{again:#?}");
        assert_eq!(rows(&db, A).len(), 2);
    }

    #[tokio::test]
    async fn flush_settles_before_it_announces() {
        let (db, _dir) = open().await;
        let (set, map, _) = new_wardrobe_turn_containers();
        let p = character(A);
        record_pending_wardrobe_announcement(&db, &ctx(A, &p, Some(&set), Some(&map)), "t").await;
        kill_writer(&db).await;
        let ((), lines) = global_capture::capture_async(flush_pending_wardrobe_announcements(
            &db,
            "u",
            CHAT,
            Some(&set),
            Some(&map),
        ))
        .await;
        let settle = position(&lines, "Failed to settle wardrobe history");
        let schedule = position(&lines, "Failed to schedule wardrobe outfit announcement");
        assert!(
            settle < schedule,
            "at the drain: settle, THEN announce: {lines:#?}"
        );
        assert!(lines[settle].contains("context=orchestrator-turn-end"));
    }

    #[tokio::test]
    async fn flush_is_a_no_op_without_containers_or_with_an_empty_set() {
        let (db, _dir) = open().await;
        let ((), lines) = global_capture::capture_async(flush_pending_wardrobe_announcements(
            &db, "u", CHAT, None, None,
        ))
        .await;
        assert!(lines.is_empty());
        // An empty set with a non-empty map: v4 returns before its clears.
        let (set, map, _) = new_wardrobe_turn_containers();
        map.lock()
            .unwrap()
            .insert(A.to_string(), WardrobeHistoryIntent::Cursor { index: 0 });
        flush_pending_wardrobe_announcements(&db, "u", CHAT, Some(&set), Some(&map)).await;
        assert_eq!(map.lock().unwrap().len(), 1, "the map is left intact");
        assert!(rows(&db, A).is_empty());
    }

    /// P4.D316 item 7 — the arrival settle runs on the commit's own writer
    /// connection, inside the repository's savepoint: with the history table
    /// gone the settle fails (its own WARN, on the writer thread) and the
    /// outfit the commit wrote STANDS — v4 runs two transactions, so a settle
    /// failure there can never undo the outfit either.
    #[tokio::test]
    async fn an_arrival_settle_failure_never_rolls_the_outfit_back() {
        let (db, _dir) = open().await;
        let written = db
            .write(|w| {
                let c = w.main().connection();
                crate::test_support::ensure_wear_ledger_on(c);
                c.execute_batch("DROP TABLE chat_wardrobe_history;")?;
                let slots = crate::wardrobe::Slots::from_value(Some(
                    &serde_json::json!({ "top": ["gown"] }),
                ));
                crate::services::wardrobe_wear_commit::commit_equipped_outfit(
                    c,
                    crate::services::wardrobe_wear_commit::CommitEquippedOutfitInput {
                        chat_id: CHAT,
                        character_id: A,
                        next_slots: &slots,
                        worn_bundles: &[],
                        source: crate::db::wardrobe_wear_stats::EquipSource::ChatStart,
                        at: None,
                    },
                )?;
                crate::services::outfit_selections::settle_arrival(
                    c,
                    CHAT,
                    A,
                    crate::db::wardrobe_wear_stats::EquipSource::ChatStart,
                );
                Ok(crate::db::chats_outfits::ChatOutfitsRepository::new(c)
                    .get_equipped_outfit_for_character(CHAT, A))
            })
            .await
            .unwrap();
        assert_eq!(written.unwrap()["top"], serde_json::json!(["gown"]));
        let after = db
            .read_main(|c| {
                Ok(crate::db::chats_outfits::ChatOutfitsRepository::new(c)
                    .get_equipped_outfit_for_character(CHAT, A))
            })
            .unwrap();
        assert_eq!(
            after.unwrap()["top"],
            serde_json::json!(["gown"]),
            "the outfit committed"
        );
    }
}
