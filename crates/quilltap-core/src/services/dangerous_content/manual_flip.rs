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
///
/// E.7 (P4.D225, v4 `49059fb14`): widened with `details` — the auto-switch's
/// tally for `auto-flagged-refusals` (`None` for every other kind). One method
/// with an `Option`, so a mock stays one method.
pub trait ConciergeAnnouncer {
    fn post_manual(
        &self,
        chat_id: &str,
        kind: &str,
        details: Option<&crate::services::concierge_notifications::ConciergeAutoFlagDetails>,
    ) -> impl std::future::Future<Output = ()> + Send;
}

/// A [`ConciergeAnnouncer`] that posts nothing.
pub struct NoConciergeAnnouncer;
impl ConciergeAnnouncer for NoConciergeAnnouncer {
    async fn post_manual(
        &self,
        _chat_id: &str,
        _kind: &str,
        _details: Option<&crate::services::concierge_notifications::ConciergeAutoFlagDetails>,
    ) {
    }
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
    async fn post_manual(
        &self,
        chat_id: &str,
        kind: &str,
        details: Option<&crate::services::concierge_notifications::ConciergeAutoFlagDetails>,
    ) {
        use crate::services::concierge_notifications as cn;
        if let Some(k) = cn::ConciergeManualKind::from_wire(kind) {
            cn::post_concierge_manual_announcement_with_details(self.db, chat_id, k, details).await;
        }
    }
}

/// Who asked for a transition (v4 `ApplyConciergeFlipOptions.by`, `49059fb14`).
/// Omitted means the operator — every caller but the refusal ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlipBy {
    #[default]
    Operator,
    Concierge,
}

impl FlipBy {
    pub fn as_str(self) -> &'static str {
        match self {
            FlipBy::Operator => "operator",
            FlipBy::Concierge => "concierge",
        }
    }
}

/// Why the Concierge flipped (v4 `ApplyConciergeFlipOptions.reason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipReason {
    Refusals,
    Classifier,
}

impl FlipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            FlipReason::Refusals => "refusals",
            FlipReason::Classifier => "classifier",
        }
    }
}

/// v4 `ApplyConciergeFlipOptions` (`49059fb14`, #74).
#[derive(Clone, Debug, Default)]
pub struct ApplyConciergeFlipOptions {
    pub by: FlipBy,
    pub reason: Option<FlipReason>,
    /// For `{ by: concierge, reason: refusals }`: what the announcement says
    /// about the refusals that earned the switch.
    pub refusals: Option<crate::services::concierge_notifications::ConciergeAutoFlagDetails>,
}

/// v4 `MODERATION_REFUSALS_CATEGORY` — the `dangerCategories` stamp an
/// auto-switch leaves, for the header pill's tooltip. Written as v4's
/// `chats.update` JSON-serializes `['moderation-refusals']`: no spaces.
pub const MODERATION_REFUSALS_CATEGORY: &str = "moderation-refusals";

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
/// caller — used for the pre-flip current state and `messageCount`. The
/// operator's form — v4's `options = {}` (every caller but the ledger).
pub async fn apply_concierge_flip<An: ConciergeAnnouncer>(
    db: &Db,
    announcer: &An,
    chat_id: &str,
    requested: ConciergeState,
    chat: &Value,
) -> Result<ApplyConciergeFlipResult, DbError> {
    apply_concierge_flip_with(
        db,
        announcer,
        chat_id,
        requested,
        chat,
        &ApplyConciergeFlipOptions::default(),
    )
    .await
}

/// v4 `applyConciergeFlip(chatId, requested, chat, options)` (`49059fb14`).
///
/// Two rules a hurried port collapses (recorded, pinned both arms):
/// `dangerCategories` is stamped `["moderation-refusals"]` on ANY `by:
/// concierge` flip to Flagged, but the `auto-flagged-refusals` bubble is posted
/// ONLY for `reason: refusals` — `by: concierge, reason: classifier` stamps the
/// category and posts `manual-flagged` (no caller does that today). The
/// `monitored` arm resets the refusal ledger ("a fresh start: stale refusals
/// must not immediately undo the operator's return to Monitored").
pub async fn apply_concierge_flip_with<An: ConciergeAnnouncer>(
    db: &Db,
    announcer: &An,
    chat_id: &str,
    requested: ConciergeState,
    chat: &Value,
    options: &ApplyConciergeFlipOptions,
) -> Result<ApplyConciergeFlipResult, DbError> {
    let by = options.by;
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
            // The Concierge's own switch leaves a category so the header
            // pill's tooltip has something to say about why.
            let categories = if by == FlipBy::Concierge {
                format!("[\"{MODERATION_REFUSALS_CATEGORY}\"]")
            } else {
                "[]".to_string()
            };
            db.write(move |writers| {
                // Stamp classification metadata so sticky-true kicks in and the
                // background scanner leaves it alone.
                writers.main().connection().execute(
                    "UPDATE chats SET \
                       \"conciergeOverride\" = NULL, \
                       \"isDangerousChat\" = 1, \
                       \"dangerScore\" = NULL, \
                       \"dangerCategories\" = ?4, \
                       \"dangerClassifiedAt\" = ?1, \
                       \"dangerClassifiedAtMessageCount\" = ?2 \
                     WHERE id = ?3",
                    rusqlite::params![now2, message_count, chat_id_owned, categories],
                )?;
                Ok(())
            })
            .await?;
            if by == FlipBy::Concierge && options.reason == Some(FlipReason::Refusals) {
                announcer
                    .post_manual(chat_id, "auto-flagged-refusals", options.refusals.as_ref())
                    .await;
            } else {
                announcer.post_manual(chat_id, "manual-flagged", None).await;
            }
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
            // v4 `49059fb14`: a fresh start — stale refusals must not
            // immediately undo the operator's return to Monitored.
            let reset_id = chat_id.to_string();
            db.write(move |writers| {
                writers
                    .main()
                    .chats()
                    .reset_moderation_refusal_ledger(&reset_id);
                Ok(())
            })
            .await?;
            let kind =
                if current == ConciergeState::Vouched || current == ConciergeState::Uncensored {
                    "manual-resumed"
                } else {
                    "manual-safe"
                };
            announcer.post_manual(chat_id, kind, None).await;
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
            announcer.post_manual(chat_id, "manual-vouched", None).await;
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
            announcer
                .post_manual(chat_id, "manual-uncensored", None)
                .await;
        }
    }

    // v4's closing line: the message names who flipped; the bag gains `by`
    // and `reason` (`reason` absent for an operator — `undefined`).
    // (Two macro calls because `tracing`'s message must be a literal.)
    let reason = options.reason.map(FlipReason::as_str);
    if by == FlipBy::Concierge {
        tracing::info!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            from = current.as_str(),
            to = requested.as_str(),
            by = by.as_str(),
            reason,
            "Concierge state flipped by the Concierge"
        );
    } else {
        tracing::info!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            from = current.as_str(),
            to = requested.as_str(),
            by = by.as_str(),
            reason,
            "Concierge state flipped manually"
        );
    }

    Ok(ApplyConciergeFlipResult {
        new_state: requested,
        changed: true,
    })
}

#[cfg(test)]
mod tests {
    //! P4.D225 (v4 `49059fb14`): the closing log line — the message names who
    //! flipped; the bag gains `by` and `reason` (`reason` ABSENT for an
    //! operator, v4's `undefined`). The row effects are the
    //! `danger_resolver_equivalence` differential's; this pins what it SAYS.
    use super::*;
    use crate::db::runtime::DbPaths;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-0000000000f1";

    fn provisioned() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("rs");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    fn flip(db: &Db, requested: ConciergeState, options: ApplyConciergeFlipOptions) -> Vec<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let chat = db
            .read_main(|c| crate::db::chats_read::find_by_id(c, CHAT))
            .unwrap()
            .unwrap();
        let (_, lines) = crate::test_support::captured_with(|| {
            rt.block_on(apply_concierge_flip_with(
                db,
                &NoConciergeAnnouncer,
                CHAT,
                requested,
                &chat,
                &options,
            ))
            .unwrap()
        });
        lines
            .into_iter()
            .filter(|l| l.contains("quilltap::concierge_manual_flip"))
            .collect()
    }

    #[test]
    fn the_flip_line_names_who_flipped_and_why() {
        let (_dir, db) = provisioned();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID, "title": "Flips", "participants": [],
        }))
        .unwrap();
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.into(),
            created_at: "2026-09-25T00:00:00.000Z".into(),
            updated_at: "2026-09-25T00:00:00.000Z".into(),
        };
        rt.block_on(db.write(move |w| w.main().chats().create(&create, &opts).map(|_| ())))
            .unwrap();
        drop(rt);

        let lines = flip(
            &db,
            ConciergeState::Flagged,
            ApplyConciergeFlipOptions {
                by: FlipBy::Concierge,
                reason: Some(FlipReason::Refusals),
                refusals: None,
            },
        );
        assert_eq!(
            lines,
            [format!("INFO quilltap::concierge_manual_flip Concierge state flipped by the Concierge chat_id={CHAT} from=monitored to=flagged by=concierge reason=refusals")]
        );
        let lines = flip(
            &db,
            ConciergeState::Monitored,
            ApplyConciergeFlipOptions::default(),
        );
        assert_eq!(
            lines,
            [format!("INFO quilltap::concierge_manual_flip Concierge state flipped manually chat_id={CHAT} from=flagged to=monitored by=operator")]
        );
        // A no-op flip logs nothing (v4 returns before the line).
        let lines = flip(
            &db,
            ConciergeState::Monitored,
            ApplyConciergeFlipOptions::default(),
        );
        assert!(lines.is_empty(), "{lines:?}");
    }
}
