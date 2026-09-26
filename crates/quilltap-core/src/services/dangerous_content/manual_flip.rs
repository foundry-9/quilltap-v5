//! Concierge state transitions (v4
//! `lib/services/dangerous-content/manual-flip.ts`, at `4d370a90f` — the
//! phase-3 rewrite, #75).
//!
//! The single chokepoint that translates a requested Concierge state into the
//! right database writes and the Concierge's announcement, so no caller has to
//! know the rules. The operator's control (the Salon sidebar, the New Chat
//! form), the refusal ledger's auto-switch and the classifier's verdict all
//! come through here.
//!
//! State mapping (requested → storage):
//!   - `moderated` → `conciergeMode 'moderated'`, setBy/reason NULL; the
//!     classifier's telemetry and the refusal ledger cleared
//!   - `unmoderated` → `conciergeMode 'unmoderated'`, setBy = by, reason = reason
//!   - `locked` → `conciergeMode 'locked'`, setBy `operator`, reason `manual`
//!
//! `conciergeOverride` is never written: it is legacy, kept only so an old row
//! can be derived (every raw `UPDATE chats SET "conciergeOverride" = …` this
//! module used to carry is gone with `4d370a90f`).
//!
//! The state is written through [`ChatsRepository::set_concierge_mode`], never
//! a whole-row update. The Concierge's own moves are a compare-and-set against
//! the state he decided on, so a decision made on a snapshot (a classifier run
//! that took seconds, a refusal check that awaited settings) can never
//! overwrite a state the operator chose meanwhile; when the set misses, nothing
//! is announced. Every transition posts a brief Concierge bubble so the history
//! stays honest about which state was in effect when; a change of provenance
//! alone (the operator confirming the Concierge's switch) is written silently.
//!
//! [`ChatsRepository::set_concierge_mode`]: crate::db::chats::ChatsRepository::set_concierge_mode

use serde_json::Value;

use crate::db::chats::ChatUpdate;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::concierge_notifications::{ConciergeAutoFlagDetails, ConciergeDangerDetails};

use super::chat_override::{
    get_concierge_provenance, get_concierge_reason, get_concierge_state, ConciergeModeColumns,
    ConciergeReason, ConciergeSetBy, ConciergeState,
};
use super::refusal_ledger::is_job_child;

/// The Concierge announcement seam (v4 `postConciergeManualAnnouncement` +
/// `postConciergeDangerAnnouncement`). `post_manual`'s `kind` is one of the
/// four transition wire strings v4 `4d370a90f` emits — `set-moderated` /
/// `set-unmoderated` / `set-locked` / `auto-unmoderated` — with `details` the
/// auto-switch's tally for `auto-unmoderated` (`None` otherwise).
/// `post_danger` is the classifier's own switch (v4 posts the DANGER
/// announcement with the verdict, not a transition kind). Async (the writer
/// awaits the single-writer channel); RPITIT so the future is `Send` without
/// boxing.
pub trait ConciergeAnnouncer {
    fn post_manual(
        &self,
        chat_id: &str,
        kind: &str,
        details: Option<&ConciergeAutoFlagDetails>,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn post_danger(
        &self,
        chat_id: &str,
        details: Option<&ConciergeDangerDetails>,
    ) -> impl std::future::Future<Output = ()> + Send;
}

/// A [`ConciergeAnnouncer`] that posts nothing.
pub struct NoConciergeAnnouncer;
impl ConciergeAnnouncer for NoConciergeAnnouncer {
    async fn post_manual(
        &self,
        _chat_id: &str,
        _kind: &str,
        _details: Option<&ConciergeAutoFlagDetails>,
    ) {
    }

    async fn post_danger(&self, _chat_id: &str, _details: Option<&ConciergeDangerDetails>) {}
}

/// The real Concierge announcer — posts the personified bubbles through the
/// ported [`crate::services::concierge_notifications`] writer. A `kind` that is
/// not a transition wire string is ignored.
pub struct RealConciergeAnnouncer<'a> {
    pub db: &'a Db,
}
impl ConciergeAnnouncer for RealConciergeAnnouncer<'_> {
    async fn post_manual(
        &self,
        chat_id: &str,
        kind: &str,
        details: Option<&ConciergeAutoFlagDetails>,
    ) {
        use crate::services::concierge_notifications as cn;
        if let Some(k) = cn::ConciergeManualKind::from_wire(kind) {
            cn::post_concierge_manual_announcement_with_details(self.db, chat_id, k, details).await;
        }
    }

    async fn post_danger(&self, chat_id: &str, details: Option<&ConciergeDangerDetails>) {
        crate::services::concierge_notifications::post_concierge_danger_announcement(
            self.db, chat_id, details,
        )
        .await;
    }
}

/// Who asked for a transition (v4 `ApplyConciergeFlipOptions.by`). Omitted
/// means the operator — every caller but the refusal ledger and the classifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlipBy {
    #[default]
    Operator,
    Concierge,
}

impl FlipBy {
    pub fn as_str(self) -> &'static str {
        self.set_by().as_str()
    }

    fn set_by(self) -> ConciergeSetBy {
        match self {
            FlipBy::Operator => ConciergeSetBy::Operator,
            FlipBy::Concierge => ConciergeSetBy::Concierge,
        }
    }
}

/// Why (v4 `ApplyConciergeFlipOptions.reason: Exclude<ConciergeModeReason,
/// 'migration'>`). Omitted means `manual`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlipReason {
    Manual,
    Refusals,
    Classifier,
}

impl FlipReason {
    pub fn as_str(self) -> &'static str {
        self.concierge_reason().as_str()
    }

    fn concierge_reason(self) -> ConciergeReason {
        match self {
            FlipReason::Manual => ConciergeReason::Manual,
            FlipReason::Refusals => ConciergeReason::Refusals,
            FlipReason::Classifier => ConciergeReason::Classifier,
        }
    }
}

/// v4 `ApplyConciergeFlipOptions` (`4d370a90f`).
#[derive(Clone, Debug, Default)]
pub struct ApplyConciergeFlipOptions {
    pub by: FlipBy,
    /// `None` = v4's `options.reason ?? 'manual'`.
    pub reason: Option<FlipReason>,
    /// For `{ by: concierge, reason: refusals }`: what the announcement says
    /// about the refusals that earned the switch.
    pub refusals: Option<ConciergeAutoFlagDetails>,
    /// For `{ by: concierge, reason: classifier }`: the verdict the
    /// announcement reports.
    pub classification: Option<ConciergeDangerDetails>,
}

/// Result of [`apply_concierge_flip`] (v4 `ApplyConciergeFlipResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyConciergeFlipResult {
    /// The state requested by the caller (the CURRENT state on a refused or
    /// abandoned Concierge move — v4 returns `current` there).
    pub new_state: ConciergeState,
    /// Whether anything was written.
    pub changed: bool,
}

/// v4 renders a `null` field as `null`; `tracing` has no null, so a `None`
/// renders as the literal (the refusal ledger's convention).
fn or_null(v: Option<&str>) -> &str {
    v.unwrap_or("null")
}

/// The operator's form — v4's `options = {}` (every caller but the ledger and
/// the classifier switch).
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

/// v4 `applyConciergeFlip(chatId, requested, chat, options)` at `4d370a90f`,
/// its eight steps in order. `chat` is the row the caller read (v4 reads only
/// its three Concierge columns). Logger `ConciergeManualFlip`.
pub async fn apply_concierge_flip_with<An: ConciergeAnnouncer>(
    db: &Db,
    announcer: &An,
    chat_id: &str,
    requested: ConciergeState,
    chat: &Value,
    options: &ApplyConciergeFlipOptions,
) -> Result<ApplyConciergeFlipResult, DbError> {
    // 1. Who, why, and where the chat stands.
    let by = options.by;
    let reason = options.reason.unwrap_or(FlipReason::Manual);
    let current = get_concierge_state(Some(chat));
    let current_by = get_concierge_provenance(Some(chat)).map(ConciergeSetBy::as_str);
    let current_reason = get_concierge_reason(Some(chat));
    let current_reason_str = current_reason.as_ref().and_then(Value::as_str);

    // 2. The provenance the requested state would carry.
    let (next_by, next_reason) = match requested {
        ConciergeState::Moderated => (None, None),
        ConciergeState::Locked => (
            Some(ConciergeSetBy::Operator),
            Some(ConciergeReason::Manual),
        ),
        ConciergeState::Unmoderated => (Some(by.set_by()), Some(reason.concierge_reason())),
    };
    let next_by_str = next_by.map(ConciergeSetBy::as_str);
    let next_reason_str = next_reason.map(ConciergeReason::as_str);

    // 3. Same state.
    if current == requested {
        // v4 compares the reason with `===` against whatever is stored — an
        // unknown stored reason (a string outside the enum, or a non-string)
        // never equals the next reason.
        let reason_matches = match &current_reason {
            None => next_reason.is_none(),
            Some(Value::String(s)) => next_reason_str == Some(s.as_str()),
            Some(_) => false,
        };
        if current_by == next_by_str && (reason_matches || requested == ConciergeState::Moderated) {
            tracing::debug!(
                target: "quilltap::concierge_manual_flip",
                chat_id = %chat_id,
                state = current.as_str(),
                by = or_null(current_by),
                reason = or_null(current_reason_str),
                "Concierge flip is a no-op: state and provenance already match"
            );
            return Ok(ApplyConciergeFlipResult {
                new_state: requested,
                changed: false,
            });
        }
        // Same state, new provenance. The Concierge never overwrites the
        // operator's own choice; the operator may adopt the Concierge's.
        if by == FlipBy::Concierge {
            tracing::debug!(
                target: "quilltap::concierge_manual_flip",
                chat_id = %chat_id,
                state = current.as_str(),
                by = or_null(current_by),
                "Concierge flip skipped: the Concierge does not re-attribute a state the operator chose"
            );
            return Ok(ApplyConciergeFlipResult {
                new_state: requested,
                changed: false,
            });
        }
        let columns = ConciergeModeColumns {
            concierge_mode: current,
            concierge_mode_set_by: next_by,
            concierge_mode_reason: next_reason,
        };
        let id = chat_id.to_string();
        db.write(move |w| Ok(w.main().chats().set_concierge_mode(&id, &columns, None)))
            .await?;
        tracing::info!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            state = current.as_str(),
            from_by = or_null(current_by),
            to_by = or_null(next_by_str),
            from_reason = or_null(current_reason_str),
            to_reason = or_null(next_reason_str),
            "Concierge state provenance updated"
        );
        return Ok(ApplyConciergeFlipResult {
            new_state: requested,
            changed: true,
        });
    }

    // 4. The Concierge moves only a Moderated chat, and only to Unmoderated.
    //    Locked is the operator's to keep, and returning a chat is the
    //    operator's call.
    if by == FlipBy::Concierge
        && (current != ConciergeState::Moderated || requested != ConciergeState::Unmoderated)
    {
        tracing::warn!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            from = current.as_str(),
            to = requested.as_str(),
            reason = reason.as_str(),
            "Concierge flip refused: the Concierge may only move a Moderated chat to Unmoderated"
        );
        return Ok(ApplyConciergeFlipResult {
            new_state: current,
            changed: false,
        });
    }

    // 5. Dead by construction in v5 (v4 `4d370a90f`'s job-child refusal — a
    //    buffered write cannot report whether it landed): v5's job runner is
    //    in-process, so `is_job_child()` is constantly false.
    if by == FlipBy::Concierge && is_job_child() {
        tracing::warn!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            to = requested.as_str(),
            reason = reason.as_str(),
            "Concierge flip refused in the job child; the parent decides"
        );
        return Ok(ApplyConciergeFlipResult {
            new_state: current,
            changed: false,
        });
    }

    // 6. The operator's choice is authoritative and lands unconditionally; the
    //    Concierge's lands only if the chat is still in the state he read.
    let columns = ConciergeModeColumns {
        concierge_mode: requested,
        concierge_mode_set_by: next_by,
        concierge_mode_reason: next_reason,
    };
    let expected = (by == FlipBy::Concierge).then_some(current);
    let id = chat_id.to_string();
    let written = db
        .write(move |w| Ok(w.main().chats().set_concierge_mode(&id, &columns, expected)))
        .await?;
    if !written {
        tracing::info!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            expected = current.as_str(),
            to = requested.as_str(),
            by = by.as_str(),
            reason = reason.as_str(),
            "Concierge flip abandoned: the chat changed state since it was read"
        );
        return Ok(ApplyConciergeFlipResult {
            new_state: current,
            changed: false,
        });
    }

    // 7. The state has landed; now its consequences, in v4's order (a crash
    //    between the two leaves the state written and the telemetry stale —
    //    v4-faithful).
    match requested {
        ConciergeState::Moderated => {
            // Clearing the classifier's telemetry lets the scheduled scan
            // re-evaluate on the next user message, and emptying the ledger
            // stops stale refusals from immediately undoing the operator's
            // choice. v4's whole-row `chats.update` of the five danger columns
            // (the Concierge columns are patch-only, so it cannot rewind step
            // 6; `updatedAt` preserved).
            let id = chat_id.to_string();
            db.write(move |w| {
                let chats = w.main().chats();
                chats.update(
                    &id,
                    &ChatUpdate {
                        is_dangerous_chat: Some(Some(false)),
                        danger_score: Some(None),
                        danger_categories: Some(Vec::new()),
                        danger_classified_at: Some(None),
                        danger_classified_at_message_count: Some(None),
                        ..Default::default()
                    },
                )?;
                chats.reset_moderation_refusal_ledger(&id);
                Ok(())
            })
            .await?;
            announcer.post_manual(chat_id, "set-moderated", None).await;
        }
        ConciergeState::Unmoderated => {
            if by == FlipBy::Operator {
                announcer
                    .post_manual(chat_id, "set-unmoderated", None)
                    .await;
            } else if reason == FlipReason::Classifier {
                announcer
                    .post_danger(chat_id, options.classification.as_ref())
                    .await;
            } else {
                announcer
                    .post_manual(chat_id, "auto-unmoderated", options.refusals.as_ref())
                    .await;
            }
        }
        ConciergeState::Locked => {
            announcer.post_manual(chat_id, "set-locked", None).await;
        }
    }

    // 8. The closing line (two macro calls: `tracing`'s message is a literal).
    if by == FlipBy::Concierge {
        tracing::info!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            from = current.as_str(),
            to = requested.as_str(),
            by = by.as_str(),
            reason = or_null(next_reason_str),
            "Concierge state switched by the Concierge"
        );
    } else {
        tracing::info!(
            target: "quilltap::concierge_manual_flip",
            chat_id = %chat_id,
            from = current.as_str(),
            to = requested.as_str(),
            by = by.as_str(),
            reason = or_null(next_reason_str),
            "Concierge state switched by the operator"
        );
    }

    Ok(ApplyConciergeFlipResult {
        new_state: requested,
        changed: true,
    })
}

#[cfg(test)]
mod tests {
    //! The flip's log lines, capture-pinned whole (level, target, message,
    //! every field in order) with their silence legs, over a provisioned
    //! instance. (The repository's DEBUG is pinned in `db::chats`.) The row effects and v4's lines at large are the
    //! `danger_resolver_equivalence` differential's (v4 `manual-flip.test.ts`
    //! mirrored by name there).
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
        let rt = rt();
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID, "title": "Flips", "participants": [],
        }))
        .unwrap();
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.into(),
            created_at: "2026-09-25T00:00:00.000Z".into(),
            updated_at: "2026-09-25T00:00:00.000Z".into(),
        };
        rt.block_on(db.write(move |w| {
            crate::test_support::ensure_p4d225_columns(w.main().connection());
            w.main().chats().create(&create, &opts).map(|_| ())
        }))
        .unwrap();
        (dir, db)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn flip(
        db: &Db,
        requested: ConciergeState,
        options: ApplyConciergeFlipOptions,
        snapshot: Option<Value>,
    ) -> (ApplyConciergeFlipResult, Vec<String>) {
        let rt = rt();
        let mut chat = db
            .read_main(|c| crate::db::chats_read::find_by_id(c, CHAT))
            .unwrap()
            .unwrap();
        if let Some(Value::Object(s)) = snapshot {
            for (k, v) in s {
                chat.as_object_mut().unwrap().insert(k, v);
            }
        }
        let (r, lines) = crate::test_support::captured_with(|| {
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
        let lines = lines
            .into_iter()
            // The repository's `Concierge state write` DEBUG runs on the
            // single-writer thread, which a thread-scoped capture cannot see;
            // `db::chats`'s own tests pin it by calling the repository directly.
            .filter(|l| l.contains(" quilltap::concierge_manual_flip "))
            .collect();
        (r, lines)
    }

    fn concierge(reason: FlipReason) -> ApplyConciergeFlipOptions {
        ApplyConciergeFlipOptions {
            by: FlipBy::Concierge,
            reason: Some(reason),
            ..Default::default()
        }
    }

    #[test]
    fn every_line_the_chokepoint_writes_in_order() {
        let (_dir, db) = provisioned();

        // A fresh chat's NULL reads as Moderated: the operator's no-op.
        let (r, lines) = flip(&db, ConciergeState::Moderated, Default::default(), None);
        assert!(!r.changed);
        assert_eq!(lines, [format!("DEBUG quilltap::concierge_manual_flip Concierge flip is a no-op: state and provenance already match chat_id={CHAT} state=moderated by=null reason=null")]);

        // The Concierge's refusals switch: the compare-and-set against Moderated.
        let (r, lines) = flip(
            &db,
            ConciergeState::Unmoderated,
            concierge(FlipReason::Refusals),
            None,
        );
        assert!(r.changed);
        assert_eq!(lines, [
            format!("INFO quilltap::concierge_manual_flip Concierge state switched by the Concierge chat_id={CHAT} from=moderated to=unmoderated by=concierge reason=refusals"),
        ]);

        // The Concierge never re-attributes …
        let (r, lines) = flip(
            &db,
            ConciergeState::Unmoderated,
            concierge(FlipReason::Classifier),
            None,
        );
        assert!(!r.changed);
        assert_eq!(lines, [format!("DEBUG quilltap::concierge_manual_flip Concierge flip skipped: the Concierge does not re-attribute a state the operator chose chat_id={CHAT} state=unmoderated by=concierge")]);

        // … but the operator adopts the Concierge's switch, silently.
        let (r, lines) = flip(&db, ConciergeState::Unmoderated, Default::default(), None);
        assert!(r.changed);
        assert_eq!(lines, [
            format!("INFO quilltap::concierge_manual_flip Concierge state provenance updated chat_id={CHAT} state=unmoderated from_by=concierge to_by=operator from_reason=refusals to_reason=manual"),
        ]);

        // The Concierge may only move a Moderated chat to Unmoderated.
        let (r, lines) = flip(
            &db,
            ConciergeState::Moderated,
            concierge(FlipReason::Refusals),
            None,
        );
        assert_eq!(
            (r.new_state, r.changed),
            (ConciergeState::Unmoderated, false)
        );
        assert_eq!(lines, [format!("WARN quilltap::concierge_manual_flip Concierge flip refused: the Concierge may only move a Moderated chat to Unmoderated chat_id={CHAT} from=unmoderated to=moderated reason=refusals")]);

        // The operator locks it (an unconditional write: no `expected`).
        let (r, lines) = flip(&db, ConciergeState::Locked, Default::default(), None);
        assert!(r.changed);
        assert_eq!(lines, [
            format!("INFO quilltap::concierge_manual_flip Concierge state switched by the operator chat_id={CHAT} from=unmoderated to=locked by=operator reason=manual"),
        ]);

        // A classifier decision made on a stale Moderated snapshot misses.
        let (r, lines) = flip(
            &db,
            ConciergeState::Unmoderated,
            concierge(FlipReason::Classifier),
            Some(
                json!({ "conciergeMode": "moderated", "conciergeModeSetBy": null, "conciergeModeReason": null }),
            ),
        );
        assert_eq!((r.new_state, r.changed), (ConciergeState::Moderated, false));
        assert_eq!(lines, [
            format!("INFO quilltap::concierge_manual_flip Concierge flip abandoned: the chat changed state since it was read chat_id={CHAT} expected=moderated to=unmoderated by=concierge reason=classifier"),
        ]);

        // The return to Moderated: `reason=null` on the closing line.
        let (r, lines) = flip(&db, ConciergeState::Moderated, Default::default(), None);
        assert!(r.changed);
        assert_eq!(lines, [
            format!("INFO quilltap::concierge_manual_flip Concierge state switched by the operator chat_id={CHAT} from=locked to=moderated by=operator reason=null"),
        ]);
    }

    /// v4's step order: the state lands FIRST (`setConciergeMode`), THEN the
    /// `moderated` arm's telemetry clear and ledger reset — a crash between
    /// leaves the state written and the telemetry stale, which is v4's shape.
    /// Two `AFTER UPDATE OF` triggers record which write came first (the
    /// order's M4 reddens exactly this).
    #[test]
    fn the_state_lands_before_the_telemetry_clear() {
        let (_dir, db) = provisioned();
        let rt = rt();
        rt.block_on(db.write(|w| {
            w.main().connection().execute_batch(
                "CREATE TABLE flip_seq (n INTEGER PRIMARY KEY AUTOINCREMENT, what TEXT);\
                 CREATE TRIGGER seq_mode AFTER UPDATE OF conciergeMode ON chats \
                   BEGIN INSERT INTO flip_seq (what) VALUES ('mode'); END;\
                 CREATE TRIGGER seq_telemetry AFTER UPDATE OF isDangerousChat ON chats \
                   BEGIN INSERT INTO flip_seq (what) VALUES ('telemetry'); END;\
                 CREATE TRIGGER seq_ledger AFTER UPDATE OF moderationRefusalCount ON chats \
                   BEGIN INSERT INTO flip_seq (what) VALUES ('ledger'); END;",
            )?;
            Ok(())
        }))
        .unwrap();
        flip(&db, ConciergeState::Locked, Default::default(), None);
        rt.block_on(db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DELETE FROM flip_seq;")?;
            Ok(())
        }))
        .unwrap();
        flip(&db, ConciergeState::Moderated, Default::default(), None);
        let seq: Vec<String> = db
            .read_main(|c| {
                let mut st = c.prepare("SELECT what FROM flip_seq ORDER BY n")?;
                let rows = st
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .unwrap();
        assert_eq!(seq, ["mode", "telemetry", "ledger"]);
    }

    /// Silence: the job-child arm is dead by construction (v4 `4d370a90f`'s
    /// `QUILLTAP_JOB_CHILD` refusal) — no flip ever writes its line.
    #[test]
    fn the_job_child_refusal_never_fires() {
        let (_dir, db) = provisioned();
        let (_, lines) = flip(
            &db,
            ConciergeState::Unmoderated,
            concierge(FlipReason::Refusals),
            None,
        );
        assert!(
            lines
                .iter()
                .all(|l| !l.contains("refused in the job child")),
            "{lines:?}"
        );
    }
}
