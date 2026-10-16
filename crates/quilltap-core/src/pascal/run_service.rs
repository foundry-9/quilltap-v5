//! The one run service for every custom-tool entrance (v4
//! `lib/pascal/run-service.ts`, `362426c8f`).
//!
//! A custom tool is reached three ways — a model's `run_custom`
//! ([`crate::tools::run_custom`]), the operator's popup
//! ([`crate::api::custom_tools::chat_custom_tool_run`]), and a trigger phrase
//! overheard in the chat ([`super::trigger_runner`]). Each entrance resolves
//! the roster its own way (their perspective rules differ) and reports a
//! failure its own way (a model result, an HTTP response, a Prospero bubble).
//! Everything between — the state cascade, the deal, the effects, Pascal's
//! post — is the same sequence, and lives here so a fix to it lands at every
//! entrance at once.
//!
//! Order is load-bearing:
//!
//!   1. The state cascade is read ONCE, whole (the effect applier needs the
//!      per-tier objects). Fail-soft: every `$state` ref carries a fallback, so
//!      a run is always dealable; state effects then skip.
//!   2. Effects apply BEFORE the Pascal post. If the post then fails, the
//!      effects stand — they happened — and the entrance reports the missing
//!      announcement. A refusal out of `execute_custom_tool` means no effects
//!      were ever applied.
//!   3. Pascal never announces a run that did not happen. A refused run comes
//!      back as [`RunCustomToolOutcome::Refused`] and the entrance hands it to
//!      Prospero.
//!
//! Every line here is emitted on the CALLER thread (the cascade read is a
//! `db.read_main`; the post's `db.write` logs nothing), so the thread-scoped
//! capture rig sees it.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use super::custom_tool_types::{display_title, Visibility};
use super::custom_tools::{
    execute_custom_tool, CustomToolRunError, CustomToolRunResult, LlmInvoker, RollForm,
};
use super::roster::DiscoveredCustomTool;
use super::side_effects::{apply_effects_for_run, AppliedEffect};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::progressions::flatten_progressions;
use crate::services::pascal_writer::{
    build_pascal_result_content, post_pascal_result, PostPascalResultParams, PostedPascalMessage,
};
use crate::state::cascade::{resolve_state_cascade, GroupScope, StateCascadeResult};
use crate::tools::rng::RandomBytes;

const CONTEXT: &str = "pascal.run-service";

/// Who reached for the tool — the provenance `pascalMeta.invokedBy` records.
/// The ROW keeps `invokedBy` a plain `String`
/// ([`crate::db::chats_messages::PascalMetaIn`]); this enum is the service's
/// input, serialised to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokedBy {
    /// A model's `run_custom`.
    Llm,
    /// The operator's Run Tool popup.
    User,
    /// A trigger phrase said in the chat (v4 `362426c8f`).
    Trigger,
}

impl InvokedBy {
    /// The wire string — `"llm" | "user" | "trigger"`.
    pub fn as_str(self) -> &'static str {
        match self {
            InvokedBy::Llm => "llm",
            InvokedBy::User => "user",
            InvokedBy::Trigger => "trigger",
        }
    }
}

/// One run's inputs (v4 `RunCustomToolParams`).
pub struct RunCustomToolParams<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    /// The chat, when the entrance already holds it — saves the cascade a read.
    /// `None` → read here.
    pub chat: Option<&'a Value>,
    /// The resolved roster entry — the CALLER resolves the roster (its
    /// perspective rules differ per entrance).
    pub entry: &'a DiscoveredCustomTool,
    /// Supplied parameters; `None` = every default.
    pub parameters: Option<&'a Map<String, Value>>,
    /// Explicit privacy, or `None` for the definition's default.
    pub private: Option<bool>,
    /// The rolling character, or `None` for a run nobody made (metadata and
    /// progress effects then skip, and the group tier is skipped).
    pub character_id: Option<&'a str>,
    /// The rolling character's hydrated fact sheet (`{}` for a run nobody made).
    pub metadata: &'a Map<String, Value>,
    /// Who the whisper goes to when the run is private. `None` → the post is
    /// not targeted.
    pub whisper_target_ids: Option<Vec<String>>,
    /// Provenance for `pascalMeta`.
    pub invoked_by: InvokedBy,
    /// Recorded in `pascalMeta` when set.
    pub caller_participant_id: Option<&'a str>,
    /// The phrase that fired a triggered run, as written in the definition.
    pub trigger_phrase: Option<&'a str>,
    /// One clock reading for the whole run (v4 `params.nowMs ?? Date.now()` —
    /// the CALLERS resolve the default, so a differential can freeze it).
    pub now_ms: i64,
    /// RESERVED — always `None` here and never read. v4 `f0db8cbb8` gives it a
    /// use: the turn's wardrobe announcement set, threaded into the effect
    /// applier so a `wardrobe` effect joins the turn's ONE Aurora
    /// announcement. Typed as the set the executor's context and the
    /// wardrobe-tool table already carry, so that lane adds the USE without a
    /// retype (a `&mut` would need a std `MutexGuard` across an `.await`).
    pub pending_wardrobe_announcements: Option<&'a Arc<Mutex<HashSet<String>>>>,
}

/// What one run came to (v4 `RunCustomToolOutcome`).
#[derive(Debug)]
pub enum RunCustomToolOutcome {
    /// The hand was dealt. `posted` is `None` when the announcement did not
    /// land — the effects still stand.
    Ok {
        result: Box<CustomToolRunResult>,
        posted: Option<PostedPascalMessage>,
        applied_effects: Vec<AppliedEffect>,
    },
    /// The deal refused (a `CustomToolRunError`). `whisper` is v4's
    /// `params.private ?? defaultVisibility === 'whisper'`, for the entrance's
    /// Prospero bubble.
    Refused { reason: String, whisper: bool },
}

/// Deal one hand: cascade → execute → effects → Pascal's post (v4
/// `runCustomTool`). Resolves the group scope from `character_id` (a run nobody
/// made gets [`GroupScope::None`]). A `CustomToolRunError` is
/// [`RunCustomToolOutcome::Refused`]; the `Err` arm is v4's rethrow of anything
/// else — reserved here, since `execute_custom_tool` is total over
/// `CustomToolRunError` and every other step is fail-soft. Never posts the
/// Prospero error bubble — that stays with the entrance, which knows how to
/// report to its caller.
pub async fn run_custom_tool<R: RandomBytes + Send>(
    db: &Db,
    params: RunCustomToolParams<'_>,
    rng: &mut R,
    llm_invoke: Option<&dyn LlmInvoker>,
) -> Result<RunCustomToolOutcome, DbError> {
    let RunCustomToolParams {
        chat_id,
        entry,
        character_id,
        metadata,
        now_ms,
        ..
    } = params;
    // Reserved for `f0db8cbb8` (see the field); deliberately unread here.
    let _ = params.pending_wardrobe_announcements;

    // The derived progress sheet, from the SAME snapshot the gates read,
    // against the one clock reading. Costs no reads: progressions live in the
    // metadata. A malformed entry is dropped by the shared reader; the prompt
    // path already warns about the same vault once per turn.
    let progress = flatten_progressions(
        Some(&Value::Object(metadata.clone())),
        now_ms,
        &mut |_, _| {},
    );

    // The group tier is scoped to the rolling character's own groups, or
    // skipped for a run nobody made rather than borrowing an arbitrary
    // participant's.
    let scope = match character_id {
        Some(id) if !id.is_empty() => GroupScope::Character {
            character_id: id.to_string(),
        },
        _ => GroupScope::None,
    };
    let held_chat = params.chat;
    let cascade_read: Result<Option<StateCascadeResult>, DbError> = db.read_main(|main| {
        // `params.chat ?? (await chats.findById(chatId))`. v4's `findById` is
        // a `safeQuery` FALLBACK read: a failed read logs the repository's own
        // ERROR and answers `null` — so an unreadable chat is "no chat" here,
        // never the catch below (measured at the pin: `run-llm-chat-unreadable`
        // in the handler family pins the silence).
        let owned;
        let chat = match held_chat {
            Some(chat) => chat,
            None => match crate::db::chats_read::find_by_id_or_none(main, chat_id) {
                Some(found) => {
                    owned = found;
                    &owned
                }
                // No chat → no cascade, SILENTLY (v4 logs nothing here).
                None => return Ok(None),
            },
        };
        // A degraded (no mount-index) open still resolves the chat tier.
        let resolved = match db
            .read_mount_index(|mount| Ok(resolve_state_cascade(main, Some(mount), chat, &scope)))
        {
            Ok(c) => c,
            Err(_) => resolve_state_cascade(main, None, chat, &scope),
        };
        Ok(Some(resolved))
    });
    // v4's catch covers a THROW inside `resolveStateCascade` — whose every
    // repository read is itself a fallback — so through v4's real code it is
    // effectively unreachable. Its v5 home is the one failure the closure can
    // still report: the read connection itself (a degraded / closed engine).
    let cascade: Option<StateCascadeResult> = match cascade_read {
        Ok(c) => c,
        Err(error) => {
            tracing::debug!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                tool = entry.definition.name.as_str(),
                error = %error,
                "Custom tool run: state cascade unavailable, defaulting to {{}}",
            );
            None
        }
    };
    let tool_state: Value = cascade
        .as_ref()
        .map(|c| c.merged.clone())
        .unwrap_or_else(|| Value::Object(Map::new()));

    let metadata_arg = if metadata.is_empty() {
        None
    } else {
        Some(metadata)
    };
    let result: CustomToolRunResult = match execute_custom_tool(
        &entry.definition,
        params.parameters,
        params.private,
        metadata_arg,
        Some(&tool_state),
        rng,
        // v4 builds the invoker only when the definition declares an `llm`
        // block; withholding it otherwise is the same thing, since a tool with
        // no block never consults.
        entry.definition.llm.as_ref().and(llm_invoke),
        Some(&progress),
        now_ms,
    )
    .await
    {
        Ok(r) => r,
        Err(CustomToolRunError(reason)) => {
            let whisper = params
                .private
                .unwrap_or(entry.definition.default_visibility == Some(Visibility::Whisper));
            tracing::debug!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                tool = entry.definition.name.as_str(),
                invokedBy = params.invoked_by.as_str(),
                reason = reason.as_str(),
                "Custom tool run refused",
            );
            return Ok(RunCustomToolOutcome::Refused { reason, whisper });
        }
    };

    // The applier never fails the run. `character_id` `None` → metadata and
    // progress effects skip.
    let applied_effects = apply_effects_for_run(
        db,
        chat_id,
        &result,
        cascade,
        character_id.map(str::to_string),
        metadata,
        now_ms,
    )
    .await;

    let whispered = result.visibility == Visibility::Whisper;
    let target_participant_ids = if whispered {
        params.whisper_target_ids.filter(|t| !t.is_empty())
    } else {
        None
    };

    let tool_title = display_title(&entry.definition.name, entry.definition.title.as_deref());
    let body =
        build_pascal_result_content(&tool_title, result.chip_label.as_deref(), &result.message);

    let pascal_meta = build_pascal_meta(
        &result,
        &tool_title,
        entry,
        &applied_effects,
        params.invoked_by,
        params.trigger_phrase,
        params.caller_participant_id,
    );

    let posted = post_pascal_result(
        db,
        PostPascalResultParams {
            chat_id: chat_id.to_string(),
            content: body.content,
            opaque_content: body.opaque_content,
            pascal_meta: Value::Object(pascal_meta),
            target_participant_ids,
        },
    )
    .await;

    let tier = serde_json::to_value(entry.tier).unwrap_or(Value::Null);
    tracing::debug!(
        target: "quilltap::pascal",
        context = CONTEXT,
        chatId = chat_id,
        tool = result.tool.as_str(),
        tier = tier.as_str().unwrap_or_default(),
        invokedBy = params.invoked_by.as_str(),
        state = result.state.as_str(),
        effects = applied_effects.len() as u64,
        whispered,
        posted = posted.is_some(),
        "Custom tool dealt",
    );

    Ok(RunCustomToolOutcome::Ok {
        result: Box::new(result),
        posted,
        applied_effects,
    })
}

/// Assemble `pascalMeta` field-for-field with v4's service (`:183-203`) — the
/// ONE assembly every entrance's post now carries. Key order is the wire's
/// (`preserve_order`): `chipLabel` right after `toolTitle`; `effects` after
/// `llm`; then `invokedBy`, `triggerPhrase` (ONLY on a `'trigger'` run that
/// carries a phrase — a `'user'` run handed one emits none), and
/// `callerParticipantId` when set.
fn build_pascal_meta(
    result: &CustomToolRunResult,
    tool_title: &str,
    entry: &DiscoveredCustomTool,
    applied_effects: &[AppliedEffect],
    invoked_by: InvokedBy,
    trigger_phrase: Option<&str>,
    caller_participant_id: Option<&str>,
) -> Map<String, Value> {
    let mut pascal_meta = Map::new();
    pascal_meta.insert("tool".into(), json!(result.tool));
    pascal_meta.insert("toolTitle".into(), json!(tool_title));
    if let Some(chip_label) = &result.chip_label {
        pascal_meta.insert("chipLabel".into(), json!(chip_label));
    }
    pascal_meta.insert(
        "definitionTier".into(),
        serde_json::to_value(entry.tier).unwrap_or(Value::Null),
    );
    pascal_meta.insert("definitionMountId".into(), json!(entry.mount_point_id));
    pascal_meta.insert(
        "params".into(),
        Value::Object(
            result
                .params
                .iter()
                .map(|(k, v)| (k.clone(), v.to_value()))
                .collect(),
        ),
    );
    pascal_meta.insert(
        "rollForm".into(),
        json!(match result.roll_form {
            RollForm::Range => "range",
            RollForm::Dice => "dice",
        }),
    );
    if let Some(notation) = &result.notation {
        pascal_meta.insert("notation".into(), json!(notation));
    }
    pascal_meta.insert("raw".into(), crate::db::js_number_to_json(result.raw));
    if let Some(dice_rolls) = &result.dice_rolls {
        pascal_meta.insert("diceRolls".into(), json!(dice_rolls));
    }
    pascal_meta.insert("value".into(), crate::db::js_number_to_json(result.value));
    pascal_meta.insert("state".into(), json!(result.state.as_str()));
    pascal_meta.insert("outcomeIndex".into(), json!(result.outcome_index));
    if let Some(metadata_tested) = &result.metadata_tested {
        pascal_meta.insert(
            "metadataTested".into(),
            Value::Object(
                metadata_tested
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_value()))
                    .collect(),
            ),
        );
    }
    if let Some(llm) = &result.llm {
        pascal_meta.insert("llm".into(), Value::Object(llm.to_wire()));
    }
    // Only the writes that LANDED are recorded; a skipped or dropped effect
    // leaves no trace here.
    if !applied_effects.is_empty() {
        pascal_meta.insert(
            "effects".into(),
            serde_json::to_value(applied_effects).unwrap_or(Value::Null),
        );
    }
    pascal_meta.insert("invokedBy".into(), json!(invoked_by.as_str()));
    if let (InvokedBy::Trigger, Some(phrase)) = (invoked_by, trigger_phrase) {
        if !phrase.is_empty() {
            pascal_meta.insert("triggerPhrase".into(), json!(phrase));
        }
    }
    if let Some(caller) = caller_participant_id.filter(|c| !c.is_empty()) {
        pascal_meta.insert("callerParticipantId".into(), json!(caller));
    }
    pascal_meta
}

#[cfg(test)]
mod tests {
    //! The service's own capture pins, each with its silence leg (every line
    //! the service logs is on the CALLER thread, so the thread rig sees it).
    //! The families (`pascal_run_custom_handler` / `_custom_tools_route` /
    //! `_trigger_runner`) compare the same lines against v4's at every
    //! entrance; these pin each arm in isolation.
    use super::*;
    use crate::db::tiered_mount_pool::MountTier;
    use crate::test_support::global_capture;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A freshly provisioned instance holding NO chat: the cascade's chat read
    /// answers None (silently) and the post's chat check fails, so a dealt run
    /// is `posted: None`.
    fn db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (db, dir)
    }

    fn entry() -> DiscoveredCustomTool {
        let definition = crate::pascal::custom_tool_types::safe_parse(&json!({
            "name": "coin",
            "description": "Flip a coin.",
            "roll": { "min": 0.5, "max": 0.5 },
            "outcomes": [{ "when": true, "message": "Heads.", "state": "info" }],
        }))
        .expect("the definition loads");
        DiscoveredCustomTool {
            definition,
            tier: MountTier::Character,
            mount_point_id: "m1".into(),
            mount_name: "Vault".into(),
            definition_path: "Tools/coin.tool.json".into(),
        }
    }

    async fn run(
        db: &Db,
        entry: &DiscoveredCustomTool,
        parameters: Option<&Map<String, Value>>,
    ) -> (RunCustomToolOutcome, Vec<String>) {
        let metadata = Map::new();
        let (outcome, lines) = global_capture::capture_async(run_custom_tool(
            db,
            RunCustomToolParams {
                user_id: "u",
                chat_id: "c",
                chat: None,
                entry,
                parameters,
                private: None,
                character_id: None,
                metadata: &metadata,
                whisper_target_ids: Some(vec!["u".into()]),
                invoked_by: InvokedBy::Trigger,
                caller_participant_id: None,
                trigger_phrase: Some("flip it"),
                now_ms: 0,
                pending_wardrobe_announcements: None,
            },
            &mut crate::tools::rng::FixedBytes::new(vec![]),
            None,
        ))
        .await;
        let lines = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal "))
            .collect();
        (outcome.expect("never Err"), lines)
    }

    /// A dealt run logs `Custom tool dealt` with v4's fields in v4's order —
    /// and NOT the refused line, and (no chat) NOT the cascade line.
    #[tokio::test(flavor = "current_thread")]
    async fn a_dealt_run_logs_dealt_and_nothing_else() {
        global_capture::install();
        let (db, _dir) = db();
        let (outcome, lines) = run(&db, &entry(), None).await;
        assert!(matches!(
            outcome,
            RunCustomToolOutcome::Ok { posted: None, .. }
        ));
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::pascal Custom tool dealt context=pascal.run-service chatId=c \
                 tool=coin tier=character invokedBy=trigger state=info effects=0 \
                 whispered=false posted=false"
                    .to_string()
            ]
        );
    }

    /// A refused run logs `Custom tool run refused` — and NOT `dealt`.
    #[tokio::test(flavor = "current_thread")]
    async fn a_refused_run_logs_refused_and_not_dealt() {
        global_capture::install();
        let (db, _dir) = db();
        let mut nonsense = Map::new();
        nonsense.insert("nonsense".into(), json!(1));
        let (outcome, lines) = run(&db, &entry(), Some(&nonsense)).await;
        let RunCustomToolOutcome::Refused { reason, whisper } = outcome else {
            panic!("refused");
        };
        assert!(!whisper, "a public tool with no `private` is not whispered");
        assert_eq!(
            lines,
            vec![format!(
                "DEBUG quilltap::pascal Custom tool run refused context=pascal.run-service \
                 chatId=c tool=coin invokedBy=trigger reason={reason}"
            )]
        );
    }

    /// `triggerPhrase` is spread ONLY on a `'trigger'` run carrying a phrase
    /// (v4 `run-service.test.ts:128-135` — a `'user'` run handed one emits
    /// none), between `invokedBy` and `callerParticipantId`.
    #[test]
    fn the_trigger_phrase_rides_only_a_trigger_run() {
        let entry = entry();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = rt
            .block_on(crate::pascal::custom_tools::execute_custom_tool(
                &entry.definition,
                None,
                None,
                None,
                None,
                &mut crate::tools::rng::FixedBytes::new(vec![]),
                None,
                None,
                0,
            ))
            .expect("dealt");
        let meta = |by, phrase| {
            serde_json::to_string(&build_pascal_meta(
                &result,
                "Coin",
                &entry,
                &[],
                by,
                phrase,
                Some("p1"),
            ))
            .unwrap()
        };
        let trigger = meta(InvokedBy::Trigger, Some("flip it"));
        assert!(
            trigger.ends_with(
                r#""invokedBy":"trigger","triggerPhrase":"flip it","callerParticipantId":"p1"}"#
            ),
            "{trigger}"
        );
        let user = meta(InvokedBy::User, Some("flip it"));
        assert!(!user.contains("triggerPhrase"), "{user}");
        assert!(
            user.ends_with(r#""invokedBy":"user","callerParticipantId":"p1"}"#),
            "{user}"
        );
    }
}
