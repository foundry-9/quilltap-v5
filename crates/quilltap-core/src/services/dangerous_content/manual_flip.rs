//! Manual Concierge state transitions (v4
//! `lib/services/dangerous-content/manual-flip.ts`).
//!
//! The single chokepoint that translates the requested UI tri-state into the
//! right combination of database writes + a synthetic Concierge announcement.
//!
//! State mapping (UI → storage):
//!   - `safe`    → `conciergeOverride = NULL`, `isDangerousChat = false`
//!   - `flagged` → `conciergeOverride = NULL`, `isDangerousChat = true`
//!   - `off`     → `conciergeOverride = 'OFF'`, `isDangerousChat` preserved
//!
//! Returning to Safe clears the classifier metadata so the scheduled scanner
//! re-evaluates on the next user message.
//!
//! ## Writes without the frozen `ChatUpdate`
//!
//! v4 persists via `repos.chats.update(chatId, {...})`, which does NOT mint
//! `updatedAt` (the danger fields are not passed → preserved). Since `db/chats.rs`
//! (the `ChatUpdate` setters) is owned by the parallel W4.4a batch, this writes
//! the exact danger columns with a raw multi-column `UPDATE` that sets no
//! `updatedAt` — byte-identical to v4's `chats.update` result (the
//! `[[standalone-write-avoids-frozen-chatupdate]]` pattern, generalized to the
//! danger column set).
//!
//! ## Announcement seam
//!
//! v4 posts a synthetic Concierge bubble through
//! `postConciergeManualAnnouncement` (`concierge-notifications/writer.ts`), a
//! personified-system writer. That is seamed ([`ConciergeAnnouncer`], default
//! no-op) — a W4.6 personified-writer deferral; the differential mocks it to a
//! no-op, matching.

use serde_json::Value;

use crate::clock::now_iso;
use crate::db::runtime::Db;
use crate::db::DbError;

use super::chat_override::{get_concierge_state, ConciergeState};

/// The manual Concierge announcement seam (v4 `postConciergeManualAnnouncement`).
/// `kind` is one of `manual-flagged` / `manual-safe` / `manual-on-duty` /
/// `manual-off-duty`. Default no-op (the personified writer is a W4.6 deferral).
pub trait ConciergeAnnouncer {
    fn post_manual(&self, chat_id: &str, kind: &str);
}

/// A [`ConciergeAnnouncer`] that posts nothing — the faithful wiring until the
/// personified writer lands (W4.6).
pub struct NoConciergeAnnouncer;
impl ConciergeAnnouncer for NoConciergeAnnouncer {
    fn post_manual(&self, _chat_id: &str, _kind: &str) {}
}

/// Result of [`apply_concierge_flip`] (v4 `ApplyConciergeFlipResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyConciergeFlipResult {
    pub new_state: ConciergeState,
    pub changed: bool,
}

/// v4 `applyConciergeFlip`. Persists the requested tri-state (a no-op when it
/// already matches the stored state) and posts the matching announcement.
///
/// `chat` is the current chat row (v4's `chat: ChatMetadata`), read once by the
/// caller — used for the pre-flip current state and `messageCount`.
pub async fn apply_concierge_flip<An: ConciergeAnnouncer>(
    db: &Db,
    announcer: &An,
    chat_id: &str,
    requested: ConciergeState,
    chat: &Value,
) -> Result<ApplyConciergeFlipResult, DbError> {
    let current = get_concierge_state(Some(chat));
    if current == requested {
        return Ok(ApplyConciergeFlipResult {
            new_state: requested,
            changed: false,
        });
    }

    let now = now_iso();
    // `chat.messageCount ?? 0`.
    let message_count = chat
        .get("messageCount")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    let chat_id_owned = chat_id.to_string();
    match requested {
        ConciergeState::Flagged => {
            let now2 = now.clone();
            db.write(move |writers| {
                // Stamp classification metadata so sticky-true kicks in and the
                // background scanner leaves it alone.
                writers.main().connection().execute(
                    "UPDATE chats SET \
                       \"conciergeOverride\" = NULL, \
                       \"isDangerousChat\" = 1, \
                       \"dangerScore\" = NULL, \
                       \"dangerCategories\" = '[]', \
                       \"dangerClassifiedAt\" = ?1, \
                       \"dangerClassifiedAtMessageCount\" = ?2 \
                     WHERE id = ?3",
                    rusqlite::params![now2, message_count, chat_id_owned],
                )?;
                Ok(())
            })
            .await?;
            announcer.post_manual(chat_id, "manual-flagged");
        }
        ConciergeState::Safe => {
            db.write(move |writers| {
                // Clear classification metadata so the scheduled scan re-evaluates.
                writers.main().connection().execute(
                    "UPDATE chats SET \
                       \"conciergeOverride\" = NULL, \
                       \"isDangerousChat\" = 0, \
                       \"dangerScore\" = NULL, \
                       \"dangerCategories\" = '[]', \
                       \"dangerClassifiedAt\" = NULL, \
                       \"dangerClassifiedAtMessageCount\" = NULL \
                     WHERE id = ?1",
                    rusqlite::params![chat_id_owned],
                )?;
                Ok(())
            })
            .await?;
            let kind = if current == ConciergeState::Off {
                "manual-on-duty"
            } else {
                "manual-safe"
            };
            announcer.post_manual(chat_id, kind);
        }
        ConciergeState::Off => {
            db.write(move |writers| {
                writers.main().connection().execute(
                    "UPDATE chats SET \"conciergeOverride\" = 'OFF' WHERE id = ?1",
                    rusqlite::params![chat_id_owned],
                )?;
                Ok(())
            })
            .await?;
            announcer.post_manual(chat_id, "manual-off-duty");
        }
    }

    Ok(ApplyConciergeFlipResult {
        new_state: requested,
        changed: true,
    })
}
