//! Operator intervention — the pending-hold registry (v4
//! `lib/pascal/intervention/registry.ts`).
//!
//! A held custom-tool run waits here, inside the turn that dealt it, until the
//! operator answers the dialog (or presses Stop, which vetoes it). The HTTP
//! actions that resolve a hold run on a different thread from the turn that
//! is waiting, so the registry is process-global (ruling R-10 — v4's
//! `processSingleton`, never a per-engine field): a hold must survive the
//! SPA's reload.
//!
//! - **No timeout** (I2). A server restart loses the entry, but it also kills
//!   the turn that was waiting — and a held run has applied no effects and
//!   posted nothing, so nothing is left half-done.
//! - **First wins.** The first resolve settles the hold and removes it; a later
//!   one (a second tab, a double click) is told it already settled.
//! - Every add and every removal publishes `customToolInterventions` for the
//!   chat — a hint only, never data.
//!
//! The await lives in the run service: [`hold`] hands back the receiver. v4's
//! 15 s `keepAlive` ping is NOT ported (ruling R-M, `HOLD_KEEP_ALIVE_NO_STREAM`
//! — v5's send is a JSON dispatch awaited whole, and the Event channel pings
//! itself), so there is no `KEEP_ALIVE_MS` here.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, OnceLock};

use tokio::sync::oneshot;

use super::overrides::InterventionOverrides;
use super::recompute::{RecomputedRun, RunSnapshot};
use crate::db::tiered_mount_pool::MountTier;
use crate::pascal::custom_tool_types::QtapCustomTool;
use crate::pascal::custom_tools::{CustomToolRunResult, LlmConsultResult, RecordedDraw};
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;

const CONTEXT: &str = "pascal.intervention.registry";

/// How many settled ids to remember, so a late resolve gets 409 rather than
/// 404.
pub const SETTLED_MEMORY: usize = 200;

/// Why a hold was vetoed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VetoCause {
    /// The operator swept it away in the dialog.
    Dialog,
    /// The operator pressed Stop on the turn (I9).
    Stop,
}

impl VetoCause {
    pub fn as_str(self) -> &'static str {
        match self {
            VetoCause::Dialog => "dialog",
            VetoCause::Stop => "stop",
        }
    }
}

/// The operator's answer to a hold (v4 `InterventionDecision`).
#[derive(Debug, Clone, PartialEq)]
pub enum InterventionDecision {
    /// Deal the run as recomputed under these overrides. `run` is the server's
    /// own recompute, never the client's.
    Deal {
        overrides: Box<InterventionOverrides>,
        run: Box<RecomputedRun>,
        reasks: usize,
    },
    AsRolled,
    Veto {
        cause: VetoCause,
    },
}

impl InterventionDecision {
    /// The `kind` the log and the wire name.
    pub fn kind(&self) -> &'static str {
        match self {
            InterventionDecision::Deal { .. } => "deal",
            InterventionDecision::AsRolled => "as-rolled",
            InterventionDecision::Veto { .. } => "veto",
        }
    }
}

/// Which entrance reached the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entrance {
    /// A model's `run_custom` call.
    Llm,
    /// A trigger phrase.
    Trigger,
}

impl Entrance {
    pub fn as_str(self) -> &'static str {
        match self {
            Entrance::Llm => "llm",
            Entrance::Trigger => "trigger",
        }
    }
}

/// A hold as the run service registers it — [`PendingIntervention`] without
/// the id and the clock the registry stamps.
#[derive(Debug, Clone)]
pub struct NewHold {
    pub chat_id: String,
    pub user_id: String,
    pub entrance: Entrance,
    pub character_id: Option<String>,
    pub character_name: Option<String>,
    /// The phrase that reached a triggered run, as written in the definition.
    pub trigger_phrase: Option<String>,
    /// The definition as it was dealt — a mid-hold file edit is not picked up.
    pub definition: QtapCustomTool,
    pub definition_tier: MountTier,
    pub definition_mount_id: String,
    pub snapshot: RunSnapshot,
    pub draw: RecordedDraw,
    pub baseline: CustomToolRunResult,
    /// The original answer, then one per Re-ask. The last is the answer in use.
    pub consults: Vec<LlmConsultResult>,
}

/// One held run (v4 `PendingIntervention`).
#[derive(Debug, Clone)]
pub struct PendingIntervention {
    pub id: String,
    /// Epoch ms when the hold was registered.
    pub created_at: i64,
    pub hold: NewHold,
}

/// The answer to a resolve (v4 `SettleOutcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    Resolved,
    /// An id this process never held (the API's 404).
    Unknown,
    /// Already resolved (the API's 409).
    Settled,
}

impl SettleOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            SettleOutcome::Resolved => "resolved",
            SettleOutcome::Unknown => "unknown",
            SettleOutcome::Settled => "settled",
        }
    }
}

/// Whether an id is pending, recently settled, or unknown to this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterventionStatus {
    Pending,
    Settled,
    Unknown,
}

impl InterventionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InterventionStatus::Pending => "pending",
            InterventionStatus::Settled => "settled",
            InterventionStatus::Unknown => "unknown",
        }
    }
}

struct Entry {
    pending: PendingIntervention,
    resolve: Option<oneshot::Sender<InterventionDecision>>,
}

#[derive(Default)]
struct RegistryState {
    /// Insertion order — the stable sort's tie rule for equal `created_at`.
    entries: Vec<Entry>,
    settled: VecDeque<String>,
}

fn lock() -> MutexGuard<'static, RegistryState> {
    static REGISTRY: OnceLock<Mutex<RegistryState>> = OnceLock::new();
    // Every mutation is a single push / remove, so a poisoned lock still
    // holds a consistent map.
    REGISTRY
        .get_or_init(|| Mutex::new(RegistryState::default()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// Register a hold; returns its id and the receiver the decision arrives on.
/// Never fails. Logs `Custom tool run held for the operator` and publishes
/// `customToolInterventions` for the chat.
pub fn hold(new: NewHold, now_ms: i64) -> (String, oneshot::Receiver<InterventionDecision>) {
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    let chat_id = new.chat_id.clone();
    tracing::info!(
        target: "quilltap::pascal",
        context = CONTEXT,
        chatId = new.chat_id.as_str(),
        holdId = id.as_str(),
        tool = new.definition.name.as_str(),
        entrance = new.entrance.as_str(),
        characterId = new.character_id.as_deref().unwrap_or("null"),
        "Custom tool run held for the operator",
    );
    lock().entries.push(Entry {
        pending: PendingIntervention {
            id: id.clone(),
            created_at: now_ms,
            hold: new,
        },
        resolve: Some(tx),
    });
    publish_realtime(RealtimeTopic::CustomToolInterventions, Some(&chat_id));
    (id, rx)
}

/// Resolve a hold. First wins: [`SettleOutcome::Settled`] when it was already
/// resolved, [`SettleOutcome::Unknown`] for an id this process never held.
pub fn settle_intervention(id: &str, decision: InterventionDecision) -> SettleOutcome {
    let entry = {
        let mut state = lock();
        match state.entries.iter().position(|e| e.pending.id == id) {
            Some(i) => {
                let entry = state.entries.remove(i);
                state.settled.push_back(id.to_string());
                while state.settled.len() > SETTLED_MEMORY {
                    state.settled.pop_front();
                }
                Ok(entry)
            }
            None => Err(if state.settled.iter().any(|s| s == id) {
                SettleOutcome::Settled
            } else {
                SettleOutcome::Unknown
            }),
        }
    };
    let mut entry = match entry {
        Ok(e) => e,
        Err(outcome) => {
            tracing::warn!(
                target: "quilltap::pascal",
                context = CONTEXT,
                holdId = id,
                outcome = outcome.as_str(),
                "Custom tool hold resolve refused",
            );
            return outcome;
        }
    };
    let cause = match &decision {
        InterventionDecision::Veto { cause } => cause.as_str(),
        _ => "dialog",
    };
    tracing::info!(
        target: "quilltap::pascal",
        context = CONTEXT,
        chatId = entry.pending.hold.chat_id.as_str(),
        holdId = id,
        tool = entry.pending.hold.definition.name.as_str(),
        decision = decision.kind(),
        cause,
        "Custom tool hold resolved",
    );
    if let Some(tx) = entry.resolve.take() {
        // A waiter that went away (its turn died) simply never hears it.
        let _ = tx.send(decision);
    }
    publish_realtime(
        RealtimeTopic::CustomToolInterventions,
        Some(&entry.pending.hold.chat_id),
    );
    SettleOutcome::Resolved
}

/// Whether an id is pending, recently settled, or unknown. Reads only.
pub fn intervention_status(id: &str) -> InterventionStatus {
    let state = lock();
    if state.entries.iter().any(|e| e.pending.id == id) {
        InterventionStatus::Pending
    } else if state.settled.iter().any(|s| s == id) {
        InterventionStatus::Settled
    } else {
        InterventionStatus::Unknown
    }
}

/// A pending hold (a snapshot of it — the definition and the draw are frozen
/// anyway; `consults` as of this read), or `None`.
pub fn get_intervention(id: &str) -> Option<PendingIntervention> {
    lock()
        .entries
        .iter()
        .find(|e| e.pending.id == id)
        .map(|e| e.pending.clone())
}

/// This chat's pending holds, oldest first (a STABLE sort — equal `created_at`
/// keeps insertion order, as JS's `Array.prototype.sort` does).
pub fn list_interventions(chat_id: &str) -> Vec<PendingIntervention> {
    let mut holds: Vec<PendingIntervention> = lock()
        .entries
        .iter()
        .filter(|e| e.pending.hold.chat_id == chat_id)
        .map(|e| e.pending.clone())
        .collect();
    holds.sort_by_key(|p| p.created_at);
    holds
}

/// Record a Re-ask's answer; it becomes the answer in use. `false` when the
/// hold has settled meanwhile.
pub fn add_consult(id: &str, consult: LlmConsultResult) -> bool {
    let mut state = lock();
    match state.entries.iter_mut().find(|e| e.pending.id == id) {
        Some(e) => {
            e.pending.hold.consults.push(consult);
            true
        }
        None => false,
    }
}

/// Test seam (v4 `resetInterventionRegistryForTests`): forget every hold
/// without resolving it. A real `pub fn` — the harness needs it.
pub fn reset_intervention_registry_for_tests() {
    let mut state = lock();
    state.entries.clear();
    state.settled.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pascal::custom_tool_types::safe_parse;
    use crate::pascal::custom_tools::{execute_custom_tool, RunSpan};
    use crate::realtime::publish_sites::HintCapture;

    pub(crate) fn sample_hold(chat_id: &str) -> NewHold {
        let definition = safe_parse(&serde_json::json!({
            "name": "pick",
            "description": "Pick.",
            "intervention": {"enabled": true},
            "outcomes": [{"when": true, "message": "-", "state": "info"}],
        }))
        .expect("definition");
        let baseline = futures_util::FutureExt::now_or_never(execute_custom_tool(
            &definition,
            None,
            None,
            None,
            None,
            &mut crate::tools::rng::FixedBytes::new(vec![0; 6]),
            None,
            None,
            0,
            None,
            None,
            None,
        ))
        .expect("sync")
        .expect("dealt");
        assert_eq!(
            baseline.roll_span,
            RunSpan {
                min: 0.0,
                max: 1.0,
                min_exclusive: None,
                max_exclusive: Some(true)
            }
        );
        NewHold {
            chat_id: chat_id.into(),
            user_id: "u".into(),
            entrance: Entrance::Llm,
            character_id: None,
            character_name: None,
            trigger_phrase: None,
            definition,
            definition_tier: MountTier::Global,
            definition_mount_id: "m".into(),
            snapshot: RunSnapshot {
                params: None,
                private: None,
                metadata: Default::default(),
                progress: Default::default(),
                now_ms: 0,
                character: None,
                state: serde_json::json!({}),
                inventory: None,
            },
            draw: baseline.draw.clone(),
            baseline,
            consults: Vec::new(),
        }
    }

    #[test]
    fn resolves_first_wins_and_forgets_the_hold_on_settle() {
        let chat = format!("chat-{}", uuid::Uuid::new_v4());
        let (id, mut rx) = hold(sample_hold(&chat), 1);
        assert_eq!(intervention_status(&id), InterventionStatus::Pending);
        assert_eq!(list_interventions(&chat).len(), 1);
        assert_eq!(
            settle_intervention(&id, InterventionDecision::AsRolled),
            SettleOutcome::Resolved
        );
        assert_eq!(rx.try_recv().unwrap(), InterventionDecision::AsRolled);
        assert_eq!(
            settle_intervention(
                &id,
                InterventionDecision::Veto {
                    cause: VetoCause::Dialog
                }
            ),
            SettleOutcome::Settled
        );
        assert_eq!(intervention_status(&id), InterventionStatus::Settled);
        assert!(get_intervention(&id).is_none());
        assert!(list_interventions(&chat).is_empty());
        assert!(!add_consult(
            &id,
            LlmConsultResult {
                ok: true,
                output: "x".into(),
                prompt: "p".into(),
                reason: None,
                provider: None,
                model: None
            }
        ));
        assert_eq!(
            settle_intervention("never-held", InterventionDecision::AsRolled),
            SettleOutcome::Unknown
        );
        assert_eq!(
            intervention_status("never-held"),
            InterventionStatus::Unknown
        );
    }

    #[test]
    fn lists_oldest_first_and_keeps_insertion_order_on_ties() {
        let chat = format!("chat-{}", uuid::Uuid::new_v4());
        let other = format!("chat-{}", uuid::Uuid::new_v4());
        let (late, _r1) = hold(sample_hold(&chat), 30);
        let (tie_a, _r2) = hold(sample_hold(&chat), 10);
        let (_elsewhere, _r3) = hold(sample_hold(&other), 5);
        let (tie_b, _r4) = hold(sample_hold(&chat), 10);
        let ids: Vec<String> = list_interventions(&chat)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, vec![tie_a.clone(), tie_b.clone(), late.clone()]);
        assert!(add_consult(
            &tie_a,
            LlmConsultResult {
                ok: true,
                output: "y".into(),
                prompt: "p".into(),
                reason: None,
                provider: None,
                model: None
            }
        ));
        assert_eq!(get_intervention(&tie_a).unwrap().hold.consults.len(), 1);
        for id in [late, tie_a, tie_b] {
            settle_intervention(&id, InterventionDecision::AsRolled);
        }
    }

    #[test]
    fn the_settled_ring_forgets_past_two_hundred() {
        let chat = format!("chat-{}", uuid::Uuid::new_v4());
        let mut ids = Vec::new();
        for _ in 0..=SETTLED_MEMORY {
            let (id, _rx) = hold(sample_hold(&chat), 0);
            settle_intervention(&id, InterventionDecision::AsRolled);
            ids.push(id);
        }
        // Other tests settle concurrently into the same ring, so only the
        // oldest of OUR 201 is certainly gone; the newest is certainly kept.
        assert_eq!(intervention_status(&ids[0]), InterventionStatus::Unknown);
        assert_eq!(
            intervention_status(ids.last().unwrap()),
            InterventionStatus::Settled
        );
    }

    /// The two publish points (P4.D124's rule — a positive leg and a silence
    /// leg each): `hold` publishes the chat-scoped hint, `settle` publishes it
    /// again, and a LATE settle publishes nothing.
    #[tokio::test]
    async fn publishes_the_topic_on_add_and_on_removal() {
        let mut cap = HintCapture::start();
        let chat = format!("chat-{}", uuid::Uuid::new_v4());
        let (id, _rx) = hold(sample_hold(&chat), 1);
        let want = vec![("customToolInterventions".to_string(), Some(chat.clone()))];
        assert_eq!(cap.drain_expecting(1).await, want);
        settle_intervention(&id, InterventionDecision::AsRolled);
        assert_eq!(cap.drain_expecting(1).await, want);
        settle_intervention(&id, InterventionDecision::AsRolled);
        assert_eq!(cap.drain().await, vec![]);
    }
}
