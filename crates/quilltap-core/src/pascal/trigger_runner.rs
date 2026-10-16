//! Trigger phrases — the runner (v4 `lib/pascal/trigger-runner.ts`,
//! `362426c8f`).
//!
//! When the chat setting `customToolTriggers` is on (it is off by default, and
//! dead while `customTools` is off), a message from anyone in the chat that
//! contains one of a custom tool's declared `triggers` phrases runs that tool,
//! with every parameter at its default, exactly as any other run: same roster
//! resolution, same cascade, same effects, same Pascal post, same Prospero
//! failure bubble — the ONE run service ([`super::run_service`]).
//!
//! Called from the two sites that already run the auto-RNG scan — the
//! operator's message in the orchestrator and the character's reply in the
//! finalizer ([`crate::services::trigger_phrase_sites`]) — so only words
//! someone said are ever scanned. Pascal's own outcomes, Carina's answers and
//! the staff's announcements never pass through either, so none of them can
//! fire a trigger.
//!
//! Cost, stated honestly: the phrases live in the definitions, and the
//! definitions live in the roster, so there is no cheaper pre-check than
//! resolving the roster once per message (with the setting on) and scanning
//! it. Rosters are resolved per call and never cached — a `.tool.json` edited
//! mid-chat must be live on the next message.
//!
//! v5 has no forked job child: `on_posted` is the spine's event sink, and the
//! autonomous driver's sink is a no-op for `pascalResult` (the post surfaces
//! from the transcript, as a `run_custom` post does). **Never fails** — every
//! failure is a log line and an empty answer, so every line here carries a
//! capture pin.

use serde_json::{Map, Value};

use super::custom_tool_types::Visibility;
use super::llm_consult::{ConsultRunner, CustomToolConsultContext, SeamConsultInvoker};
use super::roster::{resolve_custom_tool_roster, RosterContext};
use super::run_service::{run_custom_tool, InvokedBy, RunCustomToolOutcome, RunCustomToolParams};
use super::trigger_phrases::scan_trigger_phrases;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::jsstr::js_trim;
use crate::pascal::custom_tools::LlmInvoker;
use crate::services::prospero_notifications::{
    post_prospero_custom_tool_error, ProsperoCustomToolErrorArgs,
};

const CONTEXT: &str = "pascal.trigger-runner";

/// Whose words these are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriggeredSpeaker {
    /// The participant whose words these are — the whisper target for a
    /// private tool.
    pub participant_id: Option<String>,
    /// Their character, when the seat is a character (LLM-driven, user-driven
    /// or impersonated). `None` = the bare operator.
    pub character_id: Option<String>,
}

/// v4's `Pick<ChatSettings, 'customTools' | 'customToolTriggers'>` — each key
/// three-state, as the `Pick` is (an absent `customTools` is ENABLED).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TriggerSettings {
    pub custom_tools: Option<bool>,
    pub custom_tool_triggers: Option<bool>,
}

/// Whether a chat's settings let trigger phrases fire at all (v4
/// `triggersEnabled`): `customTools !== false && customToolTriggers ===
/// true`. No settings at all → `false`.
pub fn triggers_enabled(chat_settings: Option<&TriggerSettings>) -> bool {
    chat_settings
        .is_some_and(|s| s.custom_tools != Some(false) && s.custom_tool_triggers == Some(true))
}

/// One message's inputs (v4 `RunTriggeredCustomToolsParams`, minus `onPosted`,
/// which is [`run_triggered_custom_tools`]'s own argument).
#[derive(Debug, Clone, Default)]
pub struct RunTriggeredCustomToolsParams {
    pub user_id: String,
    pub chat_id: String,
    /// The message text as persisted.
    pub content: String,
    pub speaker: TriggeredSpeaker,
    pub project_id: Option<String>,
    /// Every character in the chat (the roster's `participant` tier).
    pub character_ids: Vec<String>,
    /// The chat settings already in hand at the call site.
    pub chat_settings: Option<TriggerSettings>,
}

/// Run every tool the message's words fire (v4 `runTriggeredCustomTools`).
/// Returns the Pascal messages that were posted, in firing order — empty when
/// disabled, nothing matched, or every run failed. `now_ms` is ONE clock
/// reading for the whole message (v4 `:92`); `on_posted` surfaces each post
/// the instant it lands. **Never fails.**
pub async fn run_triggered_custom_tools(
    db: &Db,
    params: RunTriggeredCustomToolsParams,
    consult: Option<&dyn ConsultRunner>,
    now_ms: i64,
    on_posted: &mut (dyn FnMut(&Value) + Send),
) -> Vec<Value> {
    // Gated off silently: a log here would fire on every message of every chat.
    if !triggers_enabled(params.chat_settings.as_ref()) || js_trim(&params.content).is_empty() {
        return Vec::new();
    }

    match run_triggered(db, &params, consult, now_ms, on_posted).await {
        Ok(posted) => posted,
        Err(error) => {
            tracing::warn!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = params.chat_id.as_str(),
                speakerParticipantId = params.speaker.participant_id.as_deref().unwrap_or("null"),
                error = %error,
                "Trigger-phrase scan failed; no custom tools fired",
            );
            Vec::new()
        }
    }
}

async fn run_triggered(
    db: &Db,
    params: &RunTriggeredCustomToolsParams,
    consult: Option<&dyn ConsultRunner>,
    now_ms: i64,
    on_posted: &mut (dyn FnMut(&Value) + Send),
) -> Result<Vec<Value>, DbError> {
    let RunTriggeredCustomToolsParams {
        user_id,
        chat_id,
        content,
        speaker,
        ..
    } = params;

    // Perspective: the speaker's. A character seat (LLM-driven, user-driven or
    // impersonated) brings its own fact sheet and vault tier; the bare operator
    // brings none — the "run nobody made" posture, under which availability
    // gates fail closed and effects that need a character skip.
    let mut metadata: Map<String, Value> = Map::new();
    let mut character_mount_point_id: Option<String> = None;
    if let Some(character_id) = &speaker.character_id {
        match crate::tools::run_custom::read_character_hydrated(db, character_id) {
            Ok(Some(character)) => {
                if let Some(map) = character.get("metadata").and_then(Value::as_object) {
                    metadata = map.clone();
                }
                character_mount_point_id = character
                    .get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            Ok(None) => {
                tracing::warn!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    chatId = chat_id.as_str(),
                    characterId = character_id.as_str(),
                    "Trigger-phrase scan skipped — speaking character not found",
                );
                return Ok(Vec::new());
            }
            Err(error) => {
                // A broken vault (v4's `CharacterVaultUnavailableError`) or any
                // other read failure. Nothing was asked for, so no Prospero
                // bubble — just the log.
                tracing::warn!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    chatId = chat_id.as_str(),
                    characterId = character_id.as_str(),
                    error = error.as_str(),
                    "Trigger-phrase scan skipped — speaking character unreadable",
                );
                return Ok(Vec::new());
            }
        }
    }

    // Gates and `disabled` are honoured by the roster itself: a tool the
    // speaker cannot be dealt cannot be triggered by them. A failed read here
    // is the outer arm's WARN.
    let roster_ctx = RosterContext {
        user_id: user_id.clone(),
        chat_id: chat_id.clone(),
        character_id: speaker.character_id.clone(),
        character_mount_point_id,
        character_ids: Some(params.character_ids.clone()),
        project_id: params.project_id.clone(),
        metadata: Some(metadata.clone()),
    };
    let roster = db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(resolve_custom_tool_roster(&roster_ctx, main, mount, now_ms))
        })
    })?;

    let scan = scan_trigger_phrases(
        content,
        roster
            .tools
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.definition.triggers.as_deref())),
    );

    if !scan.dropped.is_empty() {
        let fired: Vec<&str> = scan.hits.iter().map(|h| h.tool.as_str()).collect();
        let dropped: Vec<&str> = scan.dropped.iter().map(|h| h.tool.as_str()).collect();
        tracing::warn!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id.as_str(),
            firedJson = %serde_json::to_string(&fired).unwrap_or_default(),
            droppedJson = %serde_json::to_string(&dropped).unwrap_or_default(),
            "Trigger phrases over the per-message cap; later tools not fired",
        );
    }
    if scan.hits.is_empty() {
        return Ok(Vec::new());
    }

    // A private triggered roll is whispered to the speaker, as a private
    // `run_custom` is whispered to its caller; for the bare operator, to their
    // userId — the manual route's rule (a UUID no character's filter admits).
    let whisper_target_ids: Vec<String> = vec![speaker
        .participant_id
        .clone()
        .unwrap_or_else(|| user_id.clone())];

    let mut posted: Vec<Value> = Vec::new();
    for hit in &scan.hits {
        let Some(entry) = roster.get(&hit.tool) else {
            continue;
        };

        // v4 builds the invoker inside the service only when the definition
        // declares `llm`; here the runner in hand (the spine's) is wrapped per
        // run, exactly as the executor's `run_run_custom` does. With no runner
        // in hand the definition takes `execute_custom_tool`'s no-invoker arm.
        let invoker = match (entry.definition.llm.as_ref(), consult) {
            (Some(_), Some(runner)) => Some(SeamConsultInvoker {
                runner,
                db,
                context: CustomToolConsultContext {
                    user_id: user_id.clone(),
                    chat_id: Some(chat_id.clone()),
                },
            }),
            _ => None,
        };
        let mut rng = crate::tools::rng::OsRandomBytes;
        let outcome = run_custom_tool(
            db,
            RunCustomToolParams {
                user_id,
                chat_id,
                chat: None,
                entry,
                parameters: None,
                private: None,
                character_id: speaker.character_id.as_deref(),
                metadata: &metadata,
                whisper_target_ids: Some(whisper_target_ids.clone()),
                invoked_by: InvokedBy::Trigger,
                caller_participant_id: speaker.participant_id.as_deref(),
                trigger_phrase: Some(&hit.phrase),
                now_ms,
                pending_wardrobe_announcements: None,
            },
            &mut rng,
            invoker.as_ref().map(|i| i as &dyn LlmInvoker),
        )
        .await;
        let outcome = match outcome {
            Ok(o) => o,
            Err(error) => RunCustomToolOutcome::Refused {
                reason: error.to_string(),
                whisper: entry.definition.default_visibility == Some(Visibility::Whisper),
            },
        };

        let (result, message) = match outcome {
            RunCustomToolOutcome::Refused { reason, whisper } => {
                tracing::debug!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    chatId = chat_id.as_str(),
                    tool = hit.tool.as_str(),
                    phrase = hit.phrase.as_str(),
                    reason = reason.as_str(),
                    "Triggered custom tool refused",
                );
                // Pascal never announces a run that did not happen — Prospero
                // reports it, exactly as for a model's refused `run_custom`.
                // Then on to the next.
                post_prospero_custom_tool_error(
                    db,
                    ProsperoCustomToolErrorArgs {
                        chat_id: chat_id.clone(),
                        tool_name: hit.tool.clone(),
                        reason,
                        whisper,
                        caller_participant_id: Some(whisper_target_ids[0].clone()),
                    },
                )
                .await;
                continue;
            }
            RunCustomToolOutcome::Ok {
                result,
                posted: None,
                ..
            } => {
                // The roll was real (its effects stand) but the announcement
                // did not land — said, as the LLM entrance says it, rather than
                // left silent.
                tracing::debug!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    chatId = chat_id.as_str(),
                    tool = hit.tool.as_str(),
                    "Triggered custom tool ran but its announcement did not land",
                );
                post_prospero_custom_tool_error(
                    db,
                    ProsperoCustomToolErrorArgs {
                        chat_id: chat_id.clone(),
                        tool_name: hit.tool.clone(),
                        reason: "the outcome could not be posted to the chat".to_string(),
                        whisper: result.visibility == Visibility::Whisper,
                        caller_participant_id: Some(whisper_target_ids[0].clone()),
                    },
                )
                .await;
                continue;
            }
            RunCustomToolOutcome::Ok {
                result,
                posted: Some(p),
                ..
            } => (result, p.message),
        };

        on_posted(&message);
        posted.push(message);

        tracing::info!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id.as_str(),
            tool = hit.tool.as_str(),
            phrase = hit.phrase.as_str(),
            speakerParticipantId = speaker.participant_id.as_deref().unwrap_or("null"),
            invokedBy = "trigger",
            state = result.state.as_str(),
            "Custom tool fired by trigger phrase",
        );
    }

    Ok(posted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4 `trigger-runner.test.ts`'s gate rows: off, null settings, master off
    /// → disabled; an ABSENT `customTools` is enabled; `customToolTriggers`
    /// must be exactly `true`.
    #[test]
    fn the_gate() {
        let s = |custom_tools, custom_tool_triggers| TriggerSettings {
            custom_tools,
            custom_tool_triggers,
        };
        assert!(!triggers_enabled(None));
        assert!(!triggers_enabled(Some(&s(None, None))));
        assert!(!triggers_enabled(Some(&s(Some(true), Some(false)))));
        assert!(!triggers_enabled(Some(&s(Some(false), Some(true)))));
        assert!(triggers_enabled(Some(&s(None, Some(true)))));
        assert!(triggers_enabled(Some(&s(Some(true), Some(true)))));
    }
}
