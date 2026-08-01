//! Audience resolution for ad-hoc announcements — v4
//! `lib/services/announcer/audience.ts` (`resolveAnnouncementAudience`), the
//! Insert Announcement composer's "Who hears it" section.
//!
//! An announcement is public by default. When the operator names an audience, the
//! bubble is posted as a whisper — so the ids they send must be re-verified
//! against the chat's *current* participants before anything is persisted. A
//! dangling id would produce a message no character can ever see and no filter
//! would ever surface: a whisper into the void.
//!
//! Shared by the post action (which refuses unknown ids) and the preview action
//! (which uses the resolved names to tell the character who they are addressing).
//!
//! Pinned by `post_office_routes_equivalence` (the resolution + persistence arms)
//! and `announcer_tier3_equivalence` (the names reaching the rewrite).

use serde_json::Value;

use crate::db::runtime::Db;
use crate::jsstr::js_trim;

/// v4 `ResolvedAnnouncementAudience`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedAnnouncementAudience {
    /// Ids to persist — `None` when the announcement is public.
    pub target_participant_ids: Option<Vec<String>>,
    /// Display names of the resolved targets, in the order requested.
    pub target_names: Vec<String>,
    /// Requested ids that are not current participants of this chat.
    pub unknown_ids: Vec<String>,
}

/// v4's `PUBLIC` constant.
fn public() -> ResolvedAnnouncementAudience {
    ResolvedAnnouncementAudience::default()
}

/// JS `new Set(requested)` iteration order: first appearance wins, duplicates
/// collapse.
fn dedupe_preserving_order(requested: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    requested
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect()
}

/// Resolve a requested audience against a chat's participants (v4
/// `resolveAnnouncementAudience`).
///
/// Duplicates collapse and order is preserved. A removed participant is not a
/// valid target: whispering to someone who has left the scene is exactly the
/// dangling case this guards against. Callers decide what to do with
/// `unknown_ids` — the post action rejects, the preview action ignores them and
/// carries on with whatever resolved.
pub fn resolve_announcement_audience(
    db: &Db,
    chat_id: &str,
    requested: Option<&[String]>,
) -> ResolvedAnnouncementAudience {
    let requested = match requested {
        None => return public(),
        Some([]) => return public(),
        Some(r) => r,
    };

    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => chat,
        // v4 `if (!chat)` — every requested id is unknown. A read ERROR takes the
        // same arm: v4's `findById` resolves to `null` for a missing row and the
        // caller's own try/catch is one layer up, so an unreadable chat cannot
        // resolve an audience either way.
        _ => {
            return ResolvedAnnouncementAudience {
                target_participant_ids: None,
                target_names: Vec::new(),
                unknown_ids: dedupe_preserving_order(requested),
            }
        }
    };

    // `chat.participants.filter(p => !p.removedAt && p.status !== 'removed')`,
    // keyed by PARTICIPANT id. JS falsiness: an absent, null or empty-string
    // `removedAt` all pass.
    let live: Vec<&Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter(|p| {
                    !matches!(p.get("removedAt"), Some(Value::String(s)) if !s.is_empty())
                        && p.get("status").and_then(Value::as_str) != Some("removed")
                })
                .collect()
        })
        .unwrap_or_default();
    let by_id = |id: &str| -> Option<&Value> {
        live.iter()
            .copied()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(id))
    };

    let mut target_participant_ids: Vec<String> = Vec::new();
    let mut unknown_ids: Vec<String> = Vec::new();
    for id in dedupe_preserving_order(requested) {
        if by_id(&id).is_some() {
            target_participant_ids.push(id);
        } else {
            unknown_ids.push(id);
        }
    }

    let target_names: Vec<String> = target_participant_ids
        .iter()
        .map(|id| {
            let participant = by_id(id).expect("resolved id is a live participant");
            // `if (!participant.characterId) return 'Someone'` — the empty string
            // is falsy too.
            let Some(character_id) = participant
                .get("characterId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                return "Someone".to_string();
            };
            let cid = character_id.to_string();
            // v4 `repos.characters.findById` (the vault overlay finder), then
            // `character?.name?.trim() || 'Someone'`. A THROWING read (an
            // unavailable vault) warns and yields 'Someone' — the participant row
            // is valid, so the whisper stays deliverable, we just can't name them.
            db.read_main(|main| {
                db.read_mount_index(|mount| {
                    crate::db::characters_read::find_by_id(main, mount, &cid)
                })
            })
            .ok()
            .flatten()
            .and_then(|c| {
                c.get("name")
                    .and_then(Value::as_str)
                    .map(|n| js_trim(n).to_string())
            })
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Someone".to_string())
        })
        .collect();

    ResolvedAnnouncementAudience {
        target_participant_ids: (!target_participant_ids.is_empty())
            .then_some(target_participant_ids),
        target_names,
        unknown_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_collapses_and_preserves_first_appearance() {
        let ids: Vec<String> = ["b", "a", "b", "c", "a"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(dedupe_preserving_order(&ids), vec!["b", "a", "c"]);
    }
}
