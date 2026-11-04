//! Custom Tools API v1 — operator intervention (Pascal the Croupier; v4
//! `app/api/v1/chats/[id]/custom-tools/interventions.ts` at the `236c04e35`
//! pin, P4.D345).
//!
//! The operator's side of a held run. A model's `run_custom` or a trigger
//! phrase reaching an intervenable tool deals the run and then holds it inside
//! the turn (`pascal::intervention::registry`); these actions read and resolve
//! the hold:
//!
//! - `GET  ?action=interventions`        — this chat's pending holds, oldest first.
//! - `POST ?action=intervention-preview` — recompute under `{id, overrides}`. Pure.
//! - `POST ?action=intervention-reask`   — pose the consult again, then recompute (💸).
//! - `POST ?action=intervention-resolve` — `{id, decision}`: deal, as rolled, or veto.
//!
//! The final run is always recomputed here from the frozen snapshot — never
//! taken from the client — and an override naming a field the author hid is a
//! 400: the flag is a rule, not a UI hint. The bodies arrive as JSON values and
//! the Zod twins run HERE, so both transports answer the same sentences
//! (ruling R-12); the HTTP edge's only own sentence is `The request body is
//! not JSON`.

use serde_json::{json, Map, Value};

use super::types::{ErrorKind, Response};
use crate::db::runtime::Db;
use crate::pascal::custom_tools::{pose_llm_consult, CustomToolRunError, LlmInvoker};
use crate::pascal::intervention::overrides::{
    check_overrides, join_messages, parse_preview_body, parse_resolve_body, DealtShape,
    InterventionDecisionInput, InterventionOverrides,
};
use crate::pascal::intervention::recompute::{recompute_run, RecomputeInput, RecomputedRun};
use crate::pascal::intervention::registry::{
    add_consult, get_intervention, intervention_status, list_interventions, settle_intervention,
    InterventionDecision, InterventionStatus, PendingIntervention, SettleOutcome, VetoCause,
};
use crate::pascal::intervention::view::{hold_view, preview_of};
use crate::pascal::llm_consult::{ConsultRunner, CustomToolConsultContext, SeamConsultInvoker};

/// v4 `interventions.ts`'s `HANDLER` — the `context` of the five lines.
const HANDLER: &str = "api.v1.chats.custom-tools.interventions";

/// 404 — v4 `notFound('Held run')`.
pub const HELD_RUN_NOT_FOUND: &str = "Held run not found";
/// 409 — a late resolve or a Re-ask on a settled hold.
pub const ALREADY_SETTLED: &str = "That run has already been settled";
/// 400 — Re-ask on a tool with no `llm` block (v4 `b02d902f3`; the old
/// eligibility refusal was DELETED and is not ported).
pub const CONSULTS_NO_ORACLE: &str = "This tool consults no oracle";

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn not_found() -> Response {
    Response::error(ErrorKind::NotFound, HELD_RUN_NOT_FOUND)
}
fn conflict() -> Response {
    Response::error(ErrorKind::Conflict, ALREADY_SETTLED)
}

/// The body v4's `readBody` would have parsed: `{id?, overrides?}` /
/// `{id?, decision?}` rebuilt from the dispatch fields, each present only when
/// the caller sent it (so a missing key reaches the Zod twin's `received
/// undefined` sentence).
fn body_of(id: Option<Value>, key: &str, value: Option<Value>) -> Value {
    let mut m = Map::new();
    if let Some(id) = id {
        m.insert("id".into(), id);
    }
    if let Some(v) = value {
        m.insert(key.into(), v);
    }
    Value::Object(m)
}

/// The hold, if it belongs to this chat (another chat's hold is `None` → 404).
fn find_hold(chat_id: &str, id: &str) -> Option<PendingIntervention> {
    get_intervention(id).filter(|p| p.hold.chat_id == chat_id)
}

/// Refuse an override the author's flags or the run's shape do not allow.
fn refusal(pending: &PendingIntervention, overrides: &InterventionOverrides) -> Option<String> {
    let baseline = &pending.hold.baseline;
    check_overrides(
        &pending.hold.definition,
        DealtShape {
            roll_form: baseline.roll_form,
            dice_count: baseline.dice_rolls.as_ref().map(Vec::len),
        },
        overrides,
    )
}

/// Recompute against the hold's frozen snapshot, the recorded draw and the
/// answer in use (the LAST consult).
async fn recompute(
    pending: &PendingIntervention,
    overrides: &InterventionOverrides,
) -> Result<RecomputedRun, CustomToolRunError> {
    let hold = &pending.hold;
    recompute_run(
        RecomputeInput {
            definition: &hold.definition,
            snapshot: &hold.snapshot,
            draw: &hold.draw,
            consult: hold.consults.last(),
            overrides,
            dealt_outcome_index: Some(hold.baseline.outcome_index),
        },
        // A replay never draws; the OS source is there only because the
        // engine's signature asks for one.
        &mut crate::tools::rng::OsRandomBytes,
    )
    .await
}

/// `GET ?action=interventions` → `{holds}`.
pub fn chat_custom_tool_interventions_list(chat_id: &str) -> Response {
    let holds: Vec<Value> = list_interventions(chat_id).iter().map(hold_view).collect();
    tracing::debug!(
        target: "quilltap::pascal",
        context = HANDLER,
        chatId = chat_id,
        holds = holds.len() as u64,
        "Custom tool holds listed",
    );
    Response::CustomToolInterventions(json!({ "holds": holds }))
}

/// `POST ?action=intervention-preview` → `{preview}`.
pub async fn chat_custom_tool_intervention_preview(
    chat_id: &str,
    id: Option<Value>,
    overrides: Option<Value>,
) -> Response {
    let (hold_id, overrides) = match parse_preview_body(&body_of(id, "overrides", overrides)) {
        Ok(b) => b,
        Err(issues) => return bad_request(join_messages(&issues)),
    };
    let Some(pending) = find_hold(chat_id, &hold_id) else {
        return not_found();
    };
    if let Some(refused) = refusal(&pending, &overrides) {
        return bad_request(refused);
    }
    match recompute(&pending, &overrides).await {
        Ok(recomputed) => {
            let pinned = serde_json::to_string(&overrides.keys()).unwrap_or_default();
            tracing::debug!(
                target: "quilltap::pascal",
                context = HANDLER,
                chatId = chat_id,
                holdId = pending.id.as_str(),
                pinnedJson = pinned.as_str(),
                staleAnswer = recomputed.stale_answer,
                "Custom tool hold previewed",
            );
            Response::CustomToolInterventionPreview(json!({
                "preview": preview_of(&pending.hold.definition, &recomputed),
            }))
        }
        Err(e) => bad_request(e.0),
    }
}

/// `POST ?action=intervention-reask` → `{preview, consults}` — 💸 the one
/// spending action: one consult through the SAME seam invoker the run's own
/// consult used (v4 `buildCustomToolLlmInvoker({userId, chatId})`). With no
/// runner (a read-only / canned assembly) the consult fails SOFT into the
/// author's `errorMessage`, as v4's invoker does with no profile.
pub async fn chat_custom_tool_intervention_reask(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    id: Option<Value>,
    overrides: Option<Value>,
    consult: Option<&dyn ConsultRunner>,
) -> Response {
    let invoker = consult.map(|runner| SeamConsultInvoker {
        runner,
        db,
        context: CustomToolConsultContext {
            user_id: user_id.to_string(),
            chat_id: Some(chat_id.to_string()),
        },
    });
    chat_custom_tool_intervention_reask_with_invoker(
        chat_id,
        id,
        overrides,
        invoker.as_ref().map(|i| i as &dyn LlmInvoker),
    )
    .await
}

/// [`chat_custom_tool_intervention_reask`] over an invoker already in hand —
/// the seam the route family scripts (v4's `buildCustomToolLlmInvoker` is
/// doubled there the same way).
pub async fn chat_custom_tool_intervention_reask_with_invoker(
    chat_id: &str,
    id: Option<Value>,
    overrides: Option<Value>,
    invoker: Option<&dyn LlmInvoker>,
) -> Response {
    let (hold_id, overrides) = match parse_preview_body(&body_of(id, "overrides", overrides)) {
        Ok(b) => b,
        Err(issues) => return bad_request(join_messages(&issues)),
    };
    let Some(pending) = find_hold(chat_id, &hold_id) else {
        return not_found();
    };
    // A hidden consult may still be re-asked: Re-ask poses the prompt as the
    // run now renders it and edits nothing the author hid (a hidden prompt's
    // override is refused below).
    let Some(llm) = pending.hold.definition.llm.clone() else {
        return bad_request(CONSULTS_NO_ORACLE);
    };
    // A fresh answer replaces a pinned one: Re-ask unpins the answer.
    let overrides = overrides.without_llm_answer();
    if let Some(refused) = refusal(&pending, &overrides) {
        return bad_request(refused);
    }

    // v4 wraps this tail in `try … catch` (`interventions.ts:278-304`): a
    // `CustomToolRunError` → 400, anything else → WARN `Custom tool hold re-ask
    // failed` + rethrow (500). Every error below is the typed run error, so
    // that WARN arm has no v5 counterpart (`UNREACHABLE_REASK_NON_RUN_ERROR`,
    // recorded beside `pascal_intervention_route_equivalence`'s `MESSAGES`).
    let before = match recompute(&pending, &overrides).await {
        Ok(r) => r,
        Err(e) => return bad_request(e.0),
    };
    let prompt = before.prompt_now.clone().unwrap_or(llm.prompt);
    let answer = match pose_llm_consult(&pending.hold.definition, prompt, invoker).await {
        Ok(a) => a,
        Err(e) => return bad_request(e.0),
    };
    let ok = answer.ok;
    if !add_consult(&pending.id, answer) {
        return conflict();
    }
    // The hold as it now stands — the new consult is the answer in use.
    let Some(pending) = get_intervention(&pending.id) else {
        return conflict();
    };
    tracing::debug!(
        target: "quilltap::pascal",
        context = HANDLER,
        chatId = chat_id,
        holdId = pending.id.as_str(),
        ok,
        reasks = (pending.hold.consults.len() - 1) as u64,
        "Custom tool hold re-asked",
    );
    match recompute(&pending, &overrides).await {
        Ok(after) => Response::CustomToolInterventionReask(json!({
            "preview": preview_of(&pending.hold.definition, &after),
            "consults": pending
                .hold
                .consults
                .iter()
                .map(|c| json!({ "ok": c.ok, "output": c.output, "prompt": c.prompt }))
                .collect::<Vec<_>>(),
        })),
        Err(e) => bad_request(e.0),
    }
}

/// `POST ?action=intervention-resolve` → `{resolved}`.
pub async fn chat_custom_tool_intervention_resolve(
    chat_id: &str,
    id: Option<Value>,
    decision: Option<Value>,
) -> Response {
    let (hold_id, decision) = match parse_resolve_body(&body_of(id, "decision", decision)) {
        Ok(b) => b,
        Err(issues) => return bad_request(join_messages(&issues)),
    };

    let Some(pending) = find_hold(chat_id, &hold_id) else {
        // Settled already, or never held here: first wins, and says which.
        let status = intervention_status(&hold_id);
        tracing::warn!(
            target: "quilltap::pascal",
            context = HANDLER,
            chatId = chat_id,
            holdId = hold_id.as_str(),
            status = status.as_str(),
            "Custom tool hold resolve refused",
        );
        return if status == InterventionStatus::Settled {
            conflict()
        } else {
            not_found()
        };
    };

    let overrides = match decision {
        InterventionDecisionInput::Deal { overrides } => overrides,
        other => {
            let settled = match other {
                InterventionDecisionInput::Veto => InterventionDecision::Veto {
                    cause: VetoCause::Dialog,
                },
                _ => InterventionDecision::AsRolled,
            };
            if settle_intervention(&pending.id, settled) != SettleOutcome::Resolved {
                return conflict();
            }
            return Response::CustomToolInterventionResolve(json!({ "resolved": other.kind() }));
        }
    };

    if let Some(refused) = refusal(&pending, &overrides) {
        return bad_request(refused);
    }
    let run = match recompute(&pending, &overrides).await {
        Ok(r) => r,
        Err(e) => return bad_request(e.0),
    };
    let original_consults = usize::from(pending.hold.baseline.llm.is_some());
    let reasks = pending
        .hold
        .consults
        .len()
        .saturating_sub(original_consults);
    let outcome = settle_intervention(
        &pending.id,
        InterventionDecision::Deal {
            overrides,
            run: Box::new(run),
            reasks,
        },
    );
    if outcome != SettleOutcome::Resolved {
        return conflict();
    }
    Response::CustomToolInterventionResolve(json!({ "resolved": "deal" }))
}
