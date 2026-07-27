//! Merge-conversation ("fold another thread in") — a differential port of v4
//! `lib/chat/apply-chat-merge.ts` plus its action handler
//! `app/api/v1/chats/[id]/actions/merge.ts` (`handleMergeConversation` :20).
//!
//! This is the lane's one genuinely NEW subsystem: nothing in v5 had an
//! `applyChatMerge`. It is the inverse of "Continue Elsewhere"
//! ([`super::chat_continuation`]) — rather than forking forward into a fresh
//! chat, it pulls a source chat's company IN at the latest point:
//!
//!   1. Every source character not already a participant of the target joins as
//!      an **LLM-controlled** participant (the target keeps its own
//!      user-controlled character as the operator's voice). Each join posts a
//!      Host welcome bubble, compiles that participant's identity stack, and
//!      applies the chosen starting outfit.
//!   2. A Host **recap** bubble lands at the tail of the target, linking back to
//!      the source and carrying the source's rolling summary.
//!   3. A reciprocal Host **back-link** bubble lands in the source.
//!
//! A merge does NOT replay turns and does NOT touch the target's turn state —
//! the recap stands in for the history, and newcomers slot into the turn order
//! naturally through `addParticipant`.
//!
//! ## What "already here" means
//!
//! ANY participant row for a character counts, `removed` ones included — so a
//! previously dismissed character is not re-added as a duplicate row. That is
//! why the skip set is built from every CHARACTER participant of the target
//! regardless of status, while the INCOMING set drops `removed` rows on the
//! SOURCE side (a character dismissed there does not travel).
//!
//! ## Errors are swallowed per character, and the bubbles come last
//!
//! One bad character must not abort the merge, so each join is wrapped; the
//! recap and back-link are posted only when at least one character actually came
//! across, so a no-op merge leaves no orphan bubbles in either chat.
//!
//! ## `llm_choose` (P4.9E3B — the refusal is GONE)
//!
//! The cheap-LLM pick runs BEFORE the per-character write closure through the
//! [`OutfitLlmChooseRunner`] host seam, and the selection is rewritten to
//! `manual` (the decided slots) or `default` (v4's any-failure fallback —
//! including an unwired runner) so the sync path inside the writer stays the
//! one implementation. The four model-free modes are unchanged
//! ([`apply_outfit_selection_sync`]).
//!
//! Pinned by `chat_admin_routes_equivalence`.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, connection_profiles, DbError};
use crate::services::chat_admin::{bad_request, internal, load_chat, not_found, ok};
use crate::services::host_notifications::{
    post_host_add_announcement, post_host_merge_from_announcement, post_host_merge_to_announcement,
    HostAddAnnouncement, HostCharacter, HostMergeFromAnnouncement, HostMergeToAnnouncement,
};
use crate::services::outfit_selections::{
    apply_outfit_selection_sync, OutfitContext, OutfitLlmChooseRequest, OutfitLlmChooseRunner,
    OutfitSelection,
};
use crate::services::system_prompt_compiler::compile_identity_stack_for_participant;
use crate::wardrobe::Slots;

/// v4 `ApplyChatMergeResult`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyChatMergeResult {
    /// Character IDs actually added to the target chat.
    pub merged_character_ids: Vec<String>,
    /// Source character IDs skipped because they were already in the target.
    pub skipped_already_present_character_ids: Vec<String>,
    /// Whether the recap bubble was posted in the target chat.
    pub posted_recap: bool,
    /// Whether the back-link bubble was posted in the source chat.
    pub posted_source_back_link: bool,
}

impl ApplyChatMergeResult {
    /// The wire shape v4's handler returns as `merge`.
    pub fn to_json(&self) -> Value {
        json!({
            "mergedCharacterIds": self.merged_character_ids,
            "skippedAlreadyPresentCharacterIds": self.skipped_already_present_character_ids,
            "postedRecap": self.posted_recap,
            "postedSourceBackLink": self.posted_source_back_link,
        })
    }
}

/// v4 `resolveMergedProfileId`: prefer the profile the character used in the
/// SOURCE chat (when it still resolves), then the user's default, then their
/// first profile. `None` when the user has none.
fn resolve_merged_profile_id(
    main: &Connection,
    source_profile_id: Option<&str>,
    user_id: &str,
) -> Result<Option<String>, DbError> {
    if let Some(id) = source_profile_id.filter(|s| !s.is_empty()) {
        if connection_profiles::find_by_id(main, id)?.is_some() {
            return Ok(Some(id.to_string()));
        }
    }
    if let Some(default) = connection_profiles::find_default(main, user_id)? {
        if let Some(id) = default.get("id").and_then(Value::as_str) {
            return Ok(Some(id.to_string()));
        }
    }
    let all = connection_profiles::find_all(main)?;
    Ok(all
        .first()
        .and_then(|p| p.get("id").and_then(Value::as_str))
        .map(str::to_string))
}

/// The source participants that should travel: CHARACTER rows that are present
/// (not `removed`), not already in the target, allowed by the operator's gate
/// (when one was given), de-duplicated by `characterId`, SOURCE order preserved.
/// Also accumulates the skipped-already-present ids, in v4's encounter order.
fn incoming_characters(
    source_chat: &Value,
    target_chat: &Value,
    include: Option<&[String]>,
    skipped: &mut Vec<String>,
) -> Vec<Value> {
    let target_character_ids: std::collections::HashSet<String> = target_chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
                .filter_map(|p| p.get("characterId").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    // v4: an EMPTY allowlist is no gate at all (the handler rejects `[]` before
    // it gets here, but `applyChatMerge` is defensive about it).
    let allow: Option<std::collections::HashSet<&str>> = include
        .filter(|ids| !ids.is_empty())
        .map(|ids| ids.iter().map(String::as_str).collect());

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    let Some(participants) = source_chat.get("participants").and_then(Value::as_array) else {
        return out;
    };
    for p in participants {
        if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let Some(cid) = p.get("characterId").and_then(Value::as_str) else {
            continue;
        };
        if p.get("status").and_then(Value::as_str) == Some("removed") {
            continue;
        }
        if target_character_ids.contains(cid) {
            if !skipped.iter().any(|s| s == cid) {
                skipped.push(cid.to_string());
            }
            continue;
        }
        if let Some(allow) = &allow {
            if !allow.contains(cid) {
                continue;
            }
        }
        if !seen.insert(cid.to_string()) {
            continue;
        }
        out.push(p.clone());
    }
    out
}

/// v4's `updatedChat.participants.find(p => CHARACTER && characterId === cid &&
/// status !== 'removed')` — the row `addParticipant` just inserted.
fn find_new_participant(chat: &Value, character_id: &str) -> Option<Value> {
    chat.get("participants")
        .and_then(Value::as_array)?
        .iter()
        .find(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("characterId").and_then(Value::as_str) == Some(character_id)
                && p.get("status").and_then(Value::as_str) != Some("removed")
        })
        .cloned()
}

/// v4 `applyChatMerge`: add the source chat's missing characters into the target,
/// then post the recap and back-link bubbles. See the module header.
pub async fn apply_chat_merge(
    db: &Db,
    target_chat_id: &str,
    source_chat_id: &str,
    user_id: &str,
    include_character_ids: Option<&[String]>,
    outfit_selections: &[OutfitSelection],
    outfit_runner: Option<&std::sync::Arc<dyn OutfitLlmChooseRunner>>,
) -> Result<ApplyChatMergeResult, DbError> {
    let mut result = ApplyChatMergeResult::default();

    if target_chat_id == source_chat_id {
        // v4 warns and returns the empty result.
        return Ok(result);
    }

    let sid = source_chat_id.to_string();
    let Some(source_chat) = db.read_main(move |c| chats_read::find_by_id(c, &sid))? else {
        return Ok(result);
    };
    let tid = target_chat_id.to_string();
    let Some(target_chat) = db.read_main(move |c| chats_read::find_by_id(c, &tid))? else {
        return Ok(result);
    };

    let incoming = incoming_characters(
        &source_chat,
        &target_chat,
        include_character_ids,
        &mut result.skipped_already_present_character_ids,
    );

    let uid = user_id.to_string();
    let chat_settings = db
        .read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))
        .ok()
        .flatten();
    let cheap_settings = chat_settings
        .as_ref()
        .and_then(|s| s.get("cheapLLMSettings"))
        .filter(|v| !v.is_null())
        .cloned();

    // Character tags to fold into the chat's tag set, mirroring the
    // add-character flow so merged characters surface under the chat's tags too.
    // Applied ONCE after all joins.
    let mut tags_to_merge: Vec<String> = Vec::new();

    // v4 seeds `displayOrder` from the target's non-removed participant count.
    let mut display_order: i64 = target_chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter(|p| p.get("status").and_then(Value::as_str) != Some("removed"))
                .count()
        })
        .unwrap_or(0) as i64;

    let scenario_text = target_chat
        .get("scenarioText")
        .and_then(Value::as_str)
        .map(str::to_string);

    for source_participant in &incoming {
        let character_id = source_participant
            .get("characterId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // v4 reads the character through the overlay; a missing one is skipped.
        let cid = character_id.clone();
        let character = match db.read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
        }) {
            Ok(Some(c)) => c,
            // A missing character (or a read failure — v4's try/catch) skips.
            other => {
                eprintln!("DBG character read: {:?}", other.map(|o| o.is_some()));
                continue;
            }
        };

        let source_profile_id = source_participant
            .get("connectionProfileId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let uid = user_id.to_string();
        let connection_profile_id = match db.read_main(move |main| {
            resolve_merged_profile_id(main, source_profile_id.as_deref(), &uid)
        }) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let participant = json!({
            "type": "CHARACTER",
            "characterId": character_id,
            "controlledBy": "llm",
            "connectionProfileId": connection_profile_id,
            "imageProfileId": Value::Null,
            "displayOrder": display_order,
            "isActive": true,
            "status": "active",
            "hasHistoryAccess": false,
            "joinScenario": Value::Null,
        });
        let target = target_chat_id.to_string();
        let added = db
            .write(move |w| {
                crate::db::chats_participants::ChatParticipantsRepository::new(
                    w.main().connection(),
                )
                .add_participant(&target, &participant)
            })
            .await;
        if !matches!(added, Ok(true)) {
            continue;
        }
        display_order += 1;

        let tid = target_chat_id.to_string();
        let Ok(Some(updated_chat)) = db.read_main(move |c| chats_read::find_by_id(c, &tid)) else {
            continue;
        };
        let Some(new_participant) = find_new_participant(&updated_chat, &character_id) else {
            continue;
        };
        let participant_id = new_participant
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        result.merged_character_ids.push(character_id.clone());
        if let Some(tags) = character.get("tags").and_then(Value::as_array) {
            for t in tags.iter().filter_map(Value::as_str) {
                if !tags_to_merge.iter().any(|x| x == t) {
                    tags_to_merge.push(t.to_string());
                }
            }
        }

        // The Host welcome bubble — the same announcement add-participant posts.
        post_host_add_announcement(
            db,
            HostAddAnnouncement {
                chat_id: target_chat_id.to_string(),
                character: HostCharacter {
                    name: character
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    description: character
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    character_document_mount_point_id: character
                        .get("characterDocumentMountPointId")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                },
                participant_id: participant_id.clone(),
                initial_status: new_participant
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            },
        )
        .await;

        // The identity stack + the starting outfit, both inside the writer.
        // v4 default: "Same as last conversation", sourced from the merged chat.
        let selection = outfit_selections
            .iter()
            .find(|s| s.character_id == character_id)
            .cloned()
            .unwrap_or(OutfitSelection {
                character_id: character_id.clone(),
                mode: "previous_chat".to_string(),
                slots: None,
            });
        // `llm_choose` consults the cheap LLM OUTSIDE the writer (P4.9E3B):
        // the decided slots become a `manual` selection; any failure — task,
        // read, or an unwired runner — becomes `default`, v4's own fallback.
        let selection = if selection.mode == "llm_choose" {
            let chosen = match outfit_runner {
                Some(runner) => {
                    runner
                        .choose(OutfitLlmChooseRequest {
                            chat_id: target_chat_id.to_string(),
                            character_id: character_id.clone(),
                            scenario_text: scenario_text.clone(),
                            cheap_settings: cheap_settings.clone(),
                        })
                        .await
                }
                None => {
                    tracing::warn!(
                        target_chat_id,
                        character_id,
                        "[ChatMerge] llm_choose outfit runner is not assembled — \
                         falling back to the default outfit (v4's own failure shape)"
                    );
                    None
                }
            };
            match chosen {
                Some(slots) => OutfitSelection {
                    character_id: character_id.clone(),
                    mode: "manual".to_string(),
                    slots: Some(slots),
                },
                None => OutfitSelection {
                    character_id: character_id.clone(),
                    mode: "default".to_string(),
                    slots: None,
                },
            }
        } else {
            selection
        };
        let target = target_chat_id.to_string();
        let source = source_chat_id.to_string();
        let cheap = cheap_settings.clone();
        let uid = user_id.to_string();
        let scenario = scenario_text.clone();
        let pid = participant_id.clone();
        let chat_for_stack = updated_chat.clone();
        let _ = db
            .write(move |w| {
                let main = w.main().connection();
                let Some(mount_w) = w.mount_index() else {
                    return Ok(());
                };
                let mount = mount_w.connection();
                // v4 logs a compile failure and continues.
                let _ = compile_identity_stack_for_participant(main, mount, &chat_for_stack, &pid);
                let outfits = ChatOutfitsRepository::new(main);
                let ctx = OutfitContext {
                    user_id: &uid,
                    scenario_text: scenario.as_deref(),
                    cheap_settings: cheap.as_ref(),
                    source_chat_id: Some(&source),
                };
                // v4 logs an outfit failure and continues.
                let _ =
                    apply_outfit_selection_sync(main, mount, &outfits, &target, &selection, &ctx);
                Ok(())
            })
            .await;
    }

    // Fold merged characters' tags into the chat's tag set (ONE update).
    if !tags_to_merge.is_empty() {
        let tid = target_chat_id.to_string();
        if let Ok(Some(refreshed)) = db.read_main(move |c| chats_read::find_by_id(c, &tid)) {
            let existing: Vec<String> = refreshed
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let mut merged = existing.clone();
            for tag_id in &tags_to_merge {
                if !existing.iter().any(|t| t == tag_id) {
                    merged.push(tag_id.clone());
                }
            }
            if merged.len() != existing.len() {
                let tid = target_chat_id.to_string();
                let patch = ChatUpdate {
                    tags: Some(merged),
                    ..Default::default()
                };
                let _ = db
                    .write(move |w| {
                        ChatsRepository::new(w.main().connection())
                            .update(&tid, &patch)
                            .map(|_| ())
                    })
                    .await;
            }
        }
    }

    // Recap + back-link ONLY when at least one character actually came across.
    if !result.merged_character_ids.is_empty() {
        result.posted_recap = post_host_merge_from_announcement(
            db,
            HostMergeFromAnnouncement {
                chat_id: target_chat_id.to_string(),
                source_chat_id: source_chat_id.to_string(),
                source_title: source_chat
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                summary_text: source_chat
                    .get("contextSummary")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            },
        )
        .await
        .is_some();

        // Last, so we never link FROM a chat we failed to populate.
        result.posted_source_back_link = post_host_merge_to_announcement(
            db,
            HostMergeToAnnouncement {
                chat_id: source_chat_id.to_string(),
                target_chat_id: target_chat_id.to_string(),
                target_title: target_chat
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            },
        )
        .await
        .is_some();
    }

    Ok(result)
}

/// v4's `OutfitSelectionSchema` rows off the wire.
/// (v4's `outfitSelections` is `.optional()`, so an absent array is an EMPTY
/// selection list, not a validation failure — every merged character then takes
/// the merge's own `previous_chat` default.)
fn parse_outfit_selections(rows: Option<&[Value]>) -> Option<Vec<OutfitSelection>> {
    let rows: &[Value] = rows.unwrap_or(&[]);
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let character_id = r.get("characterId").and_then(Value::as_str)?.to_string();
        let mode = r.get("mode").and_then(Value::as_str)?.to_string();
        let slots = r.get("slots").and_then(Value::as_object).map(|s| {
            let arr = |k: &str| -> Vec<String> {
                s.get(k)
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Slots {
                top: arr("top"),
                bottom: arr("bottom"),
                footwear: arr("footwear"),
                accessories: arr("accessories"),
            }
        });
        out.push(OutfitSelection {
            character_id,
            mode,
            slots,
        });
    }
    Some(out)
}

/// v4 `?action=merge-conversation` (`merge.ts:20`) — the action handler around
/// [`apply_chat_merge`]. `chat_id` is the merge TARGET.
pub async fn chat_merge_conversation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    source_chat_id: &str,
    character_ids: Option<&[String]>,
    outfit_selections: Option<&[Value]>,
    outfit_runner: Option<&std::sync::Arc<dyn OutfitLlmChooseRunner>>,
) -> Response {
    let chat = match load_chat(db, chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    if source_chat_id == chat_id {
        return bad_request("Cannot merge a conversation into itself");
    }
    // An explicit, empty allowlist means the operator gated everyone out.
    if character_ids.is_some_and(|ids| ids.is_empty()) {
        return bad_request("Select at least one character to merge in.");
    }

    let Some(selections) = parse_outfit_selections(outfit_selections) else {
        return crate::services::chat_admin::validation_error();
    };
    let sid = source_chat_id.to_string();
    match db.read_main(move |c| chats_read::find_by_id(c, &sid)) {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Source chat"),
        Err(e) => return internal(e),
    }

    let merge = match apply_chat_merge(
        db,
        chat_id,
        source_chat_id,
        user_id,
        character_ids,
        &selections,
        outfit_runner,
    )
    .await
    {
        Ok(r) => r,
        // v4's try/catch → serverError.
        Err(_) => return Response::error(ErrorKind::Internal, "Failed to merge conversation"),
    };

    if merge.merged_character_ids.is_empty() {
        // No bubbles were posted in this case, so reporting a no-op is safe.
        return bad_request("None of the chosen characters could be merged in (already present).");
    }

    let refreshed = load_chat(db, chat_id).ok().flatten().unwrap_or(chat);
    let mut body = Map::new();
    body.insert("success".into(), json!(true));
    body.insert("merge".into(), merge.to_json());
    body.insert("chat".into(), refreshed);
    ok(Value::Object(body))
}
