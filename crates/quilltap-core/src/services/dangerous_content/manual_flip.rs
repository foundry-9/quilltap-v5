//! Manual Concierge state transitions (v4
//! `lib/services/dangerous-content/manual-flip.ts`).
//!
//! The Salon sidebar exposes a four-state per-chat Concierge control. This
//! module is the single chokepoint that translates the requested UI state into
//! the right combination of database writes + a synthetic Concierge
//! announcement, so the PUT handler doesn't have to know the rules.
//!
//! State mapping (UI → storage):
//!   - `monitored`  → `conciergeOverride = NULL`, `isDangerousChat = false`
//!   - `flagged`    → `conciergeOverride = NULL`, `isDangerousChat = true`
//!   - `vouched`    → `conciergeOverride = 'OFF'`, `isDangerousChat` preserved
//!   - `uncensored` → `conciergeOverride = 'UNCENSORED'`, `isDangerousChat` preserved
//!
//! Returning to Monitored clears the classifier metadata so the scheduled
//! scanner re-evaluates on the next user message. Every transition posts a brief
//! Concierge bubble into the chat so the history remains honest about which mode
//! was in effect when.
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
/// `kind` is one of the FIVE manual wire strings: `manual-flagged` /
/// `manual-safe` / `manual-vouched` / `manual-resumed` / `manual-uncensored`.
/// Now closed by [`RealConciergeAnnouncer`] (W4.6b — the ported
/// `concierge_notifications` writer). Async (the writer awaits the single-writer
/// channel); RPITIT so the future is `Send` without boxing.
pub trait ConciergeAnnouncer {
    fn post_manual(
        &self,
        chat_id: &str,
        kind: &str,
    ) -> impl std::future::Future<Output = ()> + Send;
}

/// A [`ConciergeAnnouncer`] that posts nothing.
pub struct NoConciergeAnnouncer;
impl ConciergeAnnouncer for NoConciergeAnnouncer {
    async fn post_manual(&self, _chat_id: &str, _kind: &str) {}
}

/// The real manual Concierge announcer (v4 `postConciergeManualAnnouncement`) —
/// posts the personified bubble through the ported
/// [`crate::services::concierge_notifications`] writer. A `kind` that is not one
/// of the five manual wire strings is ignored (v4's exhaustive switch never emits
/// another).
pub struct RealConciergeAnnouncer<'a> {
    pub db: &'a crate::db::runtime::Db,
}
impl ConciergeAnnouncer for RealConciergeAnnouncer<'_> {
    async fn post_manual(&self, chat_id: &str, kind: &str) {
        use crate::services::concierge_notifications as cn;
        if let Some(k) = cn::ConciergeManualKind::from_wire(kind) {
            cn::post_concierge_manual_announcement(self.db, chat_id, k).await;
        }
    }
}

/// Result of [`apply_concierge_flip`] (v4 `ApplyConciergeFlipResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyConciergeFlipResult {
    pub new_state: ConciergeState,
    pub changed: bool,
}

/// v4 `applyConciergeFlip`. Persists the requested four-state (a no-op when it
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
            announcer.post_manual(chat_id, "manual-flagged").await;
        }
        ConciergeState::Monitored => {
            db.write(move |writers| {
                // Returning to Monitored from Flagged or from an operator state.
                // Clearing the classification metadata lets the scheduled scan
                // re-evaluate on the next user message — the user wants future
                // moderation to behave as if we'd never settled the question.
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
            let kind =
                if current == ConciergeState::Vouched || current == ConciergeState::Uncensored {
                    "manual-resumed"
                } else {
                    "manual-safe"
                };
            announcer.post_manual(chat_id, kind).await;
        }
        ConciergeState::Vouched => {
            // Vouched Safe preserves the prior isDangerousChat so the operator
            // can return to Monitored or Flagged later and pick up where they
            // were: the UPDATE names the override column and nothing else.
            db.write(move |writers| {
                writers.main().connection().execute(
                    "UPDATE chats SET \"conciergeOverride\" = 'OFF' WHERE id = ?1",
                    rusqlite::params![chat_id_owned],
                )?;
                Ok(())
            })
            .await?;
            announcer.post_manual(chat_id, "manual-vouched").await;
        }
        ConciergeState::Uncensored => {
            // Uncensored likewise preserves isDangerousChat, so returning to
            // Monitored re-enters the classifier cleanly.
            db.write(move |writers| {
                writers.main().connection().execute(
                    "UPDATE chats SET \"conciergeOverride\" = 'UNCENSORED' WHERE id = ?1",
                    rusqlite::params![chat_id_owned],
                )?;
                Ok(())
            })
            .await?;
            announcer.post_manual(chat_id, "manual-uncensored").await;
        }
    }

    Ok(ApplyConciergeFlipResult {
        new_state: requested,
        changed: true,
    })
}
