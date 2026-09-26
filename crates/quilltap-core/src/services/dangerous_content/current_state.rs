//! The chat's Concierge state as it stands *now* (v4
//! `lib/services/dangerous-content/current-state.ts`, NEW at `4d370a90f`, #75)
//! — for decisions taken at the moment a provider refuses, not when the
//! request began.
//!
//! A turn or a picture reads its chat once, before a provider call that can
//! take many seconds. If the operator locks the chat meanwhile, a failover gate
//! that trusts that snapshot would send the refused request to the uncensored
//! desk anyway, which is exactly what Locked promises never happens. The
//! failover chokepoints ask this instead of the snapshot.

use crate::db::chats_read;
use crate::db::runtime::Db;

use super::chat_override::{get_concierge_state, ConciergeState};

/// v4 `readCurrentConciergeState(chatId, snapshot?)`. Re-reads the chat's
/// Concierge state; falls back to `snapshot` when there is no chat to read or
/// the read fails, and never fails. A missing snapshot reads as Moderated,
/// like a missing column. Logger `ConciergeCurrentState`.
pub fn read_current_concierge_state(
    db: &Db,
    chat_id: Option<&str>,
    snapshot: Option<ConciergeState>,
) -> ConciergeState {
    let fallback = snapshot.unwrap_or(ConciergeState::Moderated);
    // v4 `if (!chatId) return fallback` — JS falsiness: an empty id too.
    let Some(chat_id) = chat_id.filter(|id| !id.is_empty()) else {
        return fallback;
    };
    match db.read_main(|c| chats_read::find_by_id(c, chat_id)) {
        Ok(None) => {
            tracing::debug!(
                target: "quilltap::concierge_current_state",
                chat_id = %chat_id,
                snapshot = fallback.as_str(),
                "Current Concierge state: chat not found; using the snapshot"
            );
            fallback
        }
        Ok(Some(chat)) => {
            let state = get_concierge_state(Some(&chat));
            if state != fallback {
                tracing::info!(
                    target: "quilltap::concierge_current_state",
                    chat_id = %chat_id,
                    snapshot = fallback.as_str(),
                    current = state.as_str(),
                    "Concierge state changed since the request began"
                );
            }
            state
        }
        Err(error) => {
            tracing::warn!(
                target: "quilltap::concierge_current_state",
                chat_id = %chat_id,
                snapshot = fallback.as_str(),
                error = %error,
                "Could not re-read the Concierge state; using the snapshot"
            );
            fallback
        }
    }
}
