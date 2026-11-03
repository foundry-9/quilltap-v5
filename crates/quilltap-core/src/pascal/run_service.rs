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

use super::custom_tool_types::Visibility;
use super::custom_tools::{
    execute_custom_tool, CustomToolRunError, CustomToolRunResult, LlmInvoker, RollForm,
};
use super::roster::DiscoveredCustomTool;
use super::side_effects::{apply_effects_for_run, AppliedEffect};
// === P4.D329: the rolling character's bag ===
use super::tool_vocabulary::collect_tool_vocabulary;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::inventory::context::load_inventory_context;
use crate::inventory::refs::InventoryContext;
use crate::jsstr::js_trim;
use crate::progressions::flatten_progressions;
use crate::services::pascal_writer::{
    build_pascal_result_content, post_pascal_result, PostPascalResultParams, PostedPascalMessage,
};
use crate::state::cascade::{resolve_state_cascade, GroupScope, StateCascadeResult};
// === P4.D345: operator intervention ===
use crate::pascal::intervention::overrides::Announcer;
use crate::pascal::intervention::recompute::{
    changed_fields, facts_of, presentation_for, RecomputedRun, RunSnapshot,
};
use crate::pascal::intervention::registry::{
    self, Entrance, InterventionDecision, NewHold, VetoCause,
};
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
    /// The rolling character's name, for `{{character}}`, when the entrance
    /// already holds it. Absent with a `characterId` → read here. A TRI-STATE,
    /// as v4's `characterName?: string | null` is (`a05c6f755`): `None` = the
    /// entrance supplied none, read the RAW row (v4 `undefined`); `Some(None)`
    /// = explicitly nobody, no read (v4 `null`); `Some(Some(name))` = the
    /// entrance's copy, then `trim() || null`.
    pub character_name: Option<Option<&'a str>>,
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
    /// The turn's wardrobe announcement set, when the run happens inside a
    /// model turn — a `wardrobe` effect then joins the turn's one Aurora
    /// announcement (v4 `f0db8cbb8`). The SAME set the executor's context and
    /// the wardrobe-tool table carry (a `&mut` would need a std `MutexGuard`
    /// across an `.await`). `None` → the announcement is enqueued at once.
    pub pending_wardrobe_announcements: Option<&'a Arc<Mutex<HashSet<String>>>>,
    /// The turn's outfit-history intents, created beside the set (v4
    /// `05005c562`). The effects applier supplies the `'pascal'` provenance.
    pub pending_wardrobe_history:
        Option<&'a crate::services::outfit_change_effects::PendingWardrobeHistory>,
    /// The turn's pending Inventory changes (v4 `RunCustomToolParams.
    /// pendingInventoryAnnouncements?`, `e1872c94f`), beside the wardrobe's
    /// two. `None` → an `inventory` gesture posts its line at once. The LAST
    /// field (R-G); the run's pass-through to the applier is P4.D329's.
    pub pending_inventory_announcements:
        Option<&'a crate::services::inventory_change_effects::PendingInventoryAnnouncements>,
    // === P4.D345: operator intervention (v4 `839575d89`) — the LAST two ===
    /// Whether this entrance may hold the run for the operator (v4
    /// `mayHold`). Entrances decide; the service checks the rest — the
    /// definition, the chat settings, the process and the chat type.
    pub may_hold: bool,
    /// The turn's Stop signal (`TurnStopReach`): a Stop while held vetoes the
    /// run (I9). `None` outside a turn.
    pub stop: Option<crate::services::turn_control::StopSignal>,
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
    // === P4.D345 ===
    /// The operator set the table aside: nothing was applied and nothing
    /// posted (v4 `{ ok: 'vetoed' }`).
    Vetoed,
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
        character_name,
        now_ms,
        ..
    } = params;
    // The derived progress sheet, from the SAME snapshot the gates read,
    // against the one clock reading. Costs no reads: progressions live in the
    // metadata. A malformed entry is dropped by the shared reader; the prompt
    // path already warns about the same vault once per turn.
    let progress = flatten_progressions(
        Some(&Value::Object(metadata.clone())),
        now_ms,
        &mut |_, _| {},
    );

    // v4 resolves the name right after the progress sheet, BEFORE the cascade.
    let character_name = resolve_character_name(db, character_id, character_name, chat_id);

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
    // P4.D345: the chat is RETAINED past the cascade (v4 `let chat =
    // params.chat ?? null; chat = chat ?? await findById(chatId)`), so the
    // hold's autonomous check reuses it.
    let mut read_chat: Option<Value> = None;
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
                    read_chat = Some(owned.clone());
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

    // === P4.D329: the rolling character's bag, BETWEEN the cascade read and
    // the deal (v4 `loadRunInventory`) ===
    let inventory = load_run_inventory(db, &entry.definition, character_id, chat_id);

    let metadata_arg = if metadata.is_empty() {
        None
    } else {
        Some(metadata)
    };
    let chat: Option<&Value> = held_chat.or(read_chat.as_ref());
    let mut result: CustomToolRunResult = match execute_custom_tool(
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
        character_name.as_deref(),
        // P4.D329: the twelfth positional.
        inventory.as_deref().map(|i| i as &dyn InventoryContext),
        // P4.D345: a fresh deal — the hold's recompute replays through
        // `pascal::intervention::recompute`, never through here.
        None,
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

    // === P4.D345: operator intervention — the deal is done, nothing is
    // applied or posted yet (v4 `run-service.ts:200-220`) ===
    let mut presentation = presentation_for(&entry.definition, result.visibility, None, None);
    let mut intervention: Option<Value> = None;
    if may_hold_run(db, &params, chat) {
        let held = hold_for_operator(
            &params,
            &result,
            character_name.clone(),
            RunSnapshot {
                params: params.parameters.cloned(),
                private: params.private,
                metadata: metadata.clone(),
                progress: progress.clone(),
                now_ms,
                character: character_name.clone(),
                state: tool_state.clone(),
                inventory: inventory.clone(),
            },
        )
        .await;
        match held {
            HoldOutcome::Veto => return Ok(RunCustomToolOutcome::Vetoed),
            HoldOutcome::AsRolled => {}
            HoldOutcome::Deal { run, record } => {
                result = run.result;
                presentation = run.presentation;
                intervention = Some(record);
            }
        }
    }

    // The applier never fails the run. `character_id` `None` → metadata,
    // progress and wardrobe effects skip.
    let applied_effects = apply_effects_for_run(
        db,
        params.user_id,
        chat_id,
        &result,
        cascade,
        character_id.map(str::to_string),
        metadata,
        now_ms,
        params.pending_wardrobe_announcements,
        params.pending_wardrobe_history,
        // === P4.D329: the inventory pass's two inputs ===
        inventory.clone(),
        params.pending_inventory_announcements,
    )
    .await;

    let whispered = result.visibility == Visibility::Whisper;
    let target_participant_ids = if whispered {
        params.whisper_target_ids.filter(|t| !t.is_empty())
    } else {
        None
    };

    let tool_title = presentation.title.clone();
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
        intervention.as_ref(),
    );

    // `announceAsCharacter`: the rolling character speaks the outcome in
    // Pascal's place. Still Pascal's record (systemSender, pascalMeta, the
    // chip) — only the face and name on the bubble, and the speaker tag the
    // model reads, change. A run nobody made has no one to speak it, so Pascal
    // does. (v4 `presentation.announcer === 'character' ? characterId :
    // null`, then the truthy spread — P4.D345: the PRESENTATION decides, so
    // the operator's announcer override lands here.)
    let announcer_character_id = if presentation.announcer == Announcer::Character {
        character_id.filter(|c| !c.is_empty())
    } else {
        None
    };

    let posted = post_pascal_result(
        db,
        PostPascalResultParams {
            chat_id: chat_id.to_string(),
            content: body.content,
            opaque_content: body.opaque_content,
            pascal_meta: Value::Object(pascal_meta),
            target_participant_ids,
            announcer_character_id: announcer_character_id.map(str::to_string),
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
        announcedBy = if announcer_character_id.is_some() {
            "character"
        } else {
            "pascal"
        },
        posted = posted.is_some(),
        "Custom tool dealt",
    );

    Ok(RunCustomToolOutcome::Ok {
        result: Box::new(result),
        posted,
        applied_effects,
    })
}

/// The rolling character's name for `{{character}}` (v4 `resolveCharacterName`,
/// `a05c6f755`): the entrance's copy when it has one, else a RAW read (a broken
/// vault costs the placeholder, never the run). `None` for a run nobody made.
///
/// v4's three branches, one-to-one: `if (!characterId) return null;` → no id;
/// `if (supplied !== undefined) return supplied?.trim() || null;` → `Some(_)`;
/// then `characters.findByIdRaw(characterId)` → `character?.name?.trim() ||
/// null`. `findByIdRaw` is `_findById`, a `safeQuery` FALLBACK: a failed row
/// read logs the repository's own ERROR and answers null, so it never reaches
/// v4's catch — v4 reaches that DEBUG only by mock. Its v5 home is the one
/// failure the read can still report: the connection itself (`read_main`
/// `Err`). `name` is not vault-managed, so the RAW row names the character even
/// when the vault is broken.
fn resolve_character_name(
    db: &Db,
    character_id: Option<&str>,
    supplied: Option<Option<&str>>,
    chat_id: &str,
) -> Option<String> {
    let character_id = character_id.filter(|c| !c.is_empty())?;
    let trimmed = |name: &str| {
        let t = js_trim(name);
        (!t.is_empty()).then(|| t.to_string())
    };
    if let Some(supplied) = supplied {
        return supplied.and_then(trimmed);
    }
    match db.read_main(|main| {
        Ok(crate::db::characters_read::find_by_id_raw_or_none(
            main,
            character_id,
        ))
    }) {
        Ok(row) => row
            .as_ref()
            .and_then(|r| r.get("name"))
            .and_then(Value::as_str)
            .and_then(trimmed),
        Err(error) => {
            tracing::debug!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                characterId = character_id,
                error = %crate::db::fallback::error_text(&error),
                "Custom tool run: rolling character name unavailable, {{{{character}}}} renders as written",
            );
            None
        }
    }
}

/// Assemble `pascalMeta` field-for-field with v4's service (`:183-203`) — the
/// ONE assembly every entrance's post now carries. Key order is the wire's
/// (`preserve_order`): `chipLabel` right after `toolTitle`; `effects` after
/// `llm`; then `invokedBy`, `triggerPhrase` (ONLY on a `'trigger'` run that
/// carries a phrase — a `'user'` run handed one emits none), and
/// `callerParticipantId` when set.
#[allow(clippy::too_many_arguments)]
fn build_pascal_meta(
    result: &CustomToolRunResult,
    tool_title: &str,
    entry: &DiscoveredCustomTool,
    applied_effects: &[AppliedEffect],
    invoked_by: InvokedBy,
    trigger_phrase: Option<&str>,
    caller_participant_id: Option<&str>,
    // P4.D345: `...(intervention ? { intervention } : {})` — the LAST key.
    intervention: Option<&Value>,
) -> Map<String, Value> {
    let mut pascal_meta = Map::new();
    pascal_meta.insert("tool".into(), json!(result.tool));
    pascal_meta.insert("toolTitle".into(), json!(tool_title));
    // v4 spreads it on truthiness (`...(result.chipLabel ? …)`): a label that
    // rendered empty leaves no key.
    if let Some(chip_label) = result.chip_label.as_deref().filter(|c| !c.is_empty()) {
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
    if let Some(record) = intervention {
        pascal_meta.insert("intervention".into(), record.clone());
    }
    pascal_meta
}

// === P4.D345: operator intervention — the hold (v4 `run-service.ts:300-410`) ===

/// v4 `process.env.QUILLTAP_JOB_CHILD === '1'` — constantly `false` in v5:
/// the in-process job runner never forks, so a run never executes in a job
/// child (the same ruling as `services::dangerous_content::refusal_ledger::
/// is_job_child`, declared module-local rather than reaching into that
/// `pub(super)` twin). The reason's bytes are ported, pinned NEGATIVE.
fn is_job_child() -> bool {
    false
}

/// The five reasons a run on an intervenable tool is not held, in v4's order.
const NOT_HELD_OPERATORS_OWN_RUN: &str = "this entrance never holds (the operator's own run)";
const NOT_HELD_JOB_CHILD: &str = "running in the job child";
const NOT_HELD_AUTONOMOUS: &str = "an autonomous room";
const NOT_HELD_CUSTOM_TOOLS_OFF: &str = "custom tools are off";
const NOT_HELD_INTERVENTION_OFF: &str = "the operator switched intervention off";

/// Whether this run may be held for the operator (v4 `mayHoldRun`). A tool
/// that is not intervenable returns `false` SILENTLY; every other "no" is
/// logged at debug with its reason.
fn may_hold_run(db: &Db, params: &RunCustomToolParams<'_>, chat: Option<&Value>) -> bool {
    let definition = &params.entry.definition;
    if !crate::pascal::intervention::eligibility::is_intervenable(definition) {
        return false;
    }
    let skip = |reason: &str| {
        tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = params.chat_id,
            tool = definition.name.as_str(),
            reason,
            "Custom tool run not held",
        );
        false
    };
    if !params.may_hold {
        return skip(NOT_HELD_OPERATORS_OWN_RUN);
    }
    // The job child has nobody watching and cannot wait on the parent's
    // dialog.
    if is_job_child() {
        return skip(NOT_HELD_JOB_CHILD);
    }
    if chat.and_then(|c| c.get("chatType")).and_then(Value::as_str) == Some("autonomous") {
        return skip(NOT_HELD_AUTONOMOUS);
    }

    // `chatSettings.findByUserId` is a `safeQuery` FALLBACK — a failed row
    // read logs the repository's own ERROR and answers null (hold). v4's catch
    // has ONE v5 home: the read connection itself.
    match db.read_main(|main| {
        Ok(crate::db::chat_settings::find_by_user_id_or_none(
            main,
            params.user_id,
        ))
    }) {
        Ok(settings) => {
            let flag = |key: &str| {
                settings
                    .as_ref()
                    .and_then(|s| s.get(key))
                    .and_then(Value::as_bool)
            };
            if flag("customTools") == Some(false) {
                return skip(NOT_HELD_CUSTOM_TOOLS_OFF);
            }
            if flag("customToolIntervention") == Some(false) {
                return skip(NOT_HELD_INTERVENTION_OFF);
            }
        }
        Err(error) => {
            // Unreadable settings: the default is on, and a held run is
            // recoverable (the operator can let it stand), where a skipped
            // hold is not.
            tracing::debug!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = params.chat_id,
                error = %crate::db::fallback::error_text(&error),
                "Custom tool hold: chat settings unreadable, holding by default",
            );
        }
    }
    true
}

/// What the hold came to (v4 `HoldOutcome`).
enum HoldOutcome {
    Veto,
    AsRolled,
    Deal {
        run: Box<RecomputedRun>,
        record: Value,
    },
}

/// Hold the dealt run until the operator answers (v4 `holdForOperator`). A
/// deal that changes nothing is an as-rolled deal and records nothing — the
/// badge marks a change, and "I looked and let it stand" is not one.
///
/// Holds NO connection, write batch or lock across the await: the registry
/// hands back a receiver, and the only thing awaited is it (or the Stop).
async fn hold_for_operator(
    params: &RunCustomToolParams<'_>,
    result: &CustomToolRunResult,
    character_name: Option<String>,
    snapshot: RunSnapshot,
) -> HoldOutcome {
    let entry = params.entry;
    let definition = &entry.definition;
    let (id, mut rx) = registry::hold(
        NewHold {
            chat_id: params.chat_id.to_string(),
            user_id: params.user_id.to_string(),
            entrance: if params.invoked_by == InvokedBy::Trigger {
                Entrance::Trigger
            } else {
                Entrance::Llm
            },
            character_id: params.character_id.map(str::to_string),
            character_name,
            trigger_phrase: params
                .trigger_phrase
                .filter(|p| !p.is_empty())
                .map(str::to_string),
            definition: definition.clone(),
            definition_tier: entry.tier,
            definition_mount_id: entry.mount_point_id.clone(),
            snapshot,
            draw: result.draw.clone(),
            baseline: result.clone(),
            consults: result.llm.iter().cloned().collect(),
        },
        crate::clock::now_unix_ms(),
    );

    // The Stop veto (I9) through `TurnStopReach`: a stop settles the hold as
    // `Veto { Stop }` through the SAME door as the dialog (one log, one
    // publish, the ring) — and the receiver then ALWAYS yields: the Stop's
    // veto, or the operator's earlier decision (first wins). An
    // already-stopped turn settles right after the add has published (v4's
    // `queueMicrotask`).
    let decision = match &params.stop {
        None => (&mut rx).await,
        Some(stop) => {
            let stopped = std::pin::pin!(stop.wait());
            match futures_util::future::select(&mut rx, stopped).await {
                futures_util::future::Either::Left((decision, _)) => decision,
                futures_util::future::Either::Right(((), _pending)) => {
                    registry::settle_intervention(
                        &id,
                        InterventionDecision::Veto {
                            cause: VetoCause::Stop,
                        },
                    );
                    (&mut rx).await
                }
            }
        }
    };
    // A sender dropped without a decision cannot happen (settle always
    // sends); were the registry reset under a waiting turn (a test seam), the
    // run stands as rolled — nothing applied twice, nothing lost.
    let Ok(answer) = decision else {
        return HoldOutcome::AsRolled;
    };

    match answer {
        InterventionDecision::Veto { cause } => {
            tracing::info!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = params.chat_id,
                tool = definition.name.as_str(),
                invokedBy = params.invoked_by.as_str(),
                cause = cause.as_str(),
                "Custom tool run set aside by the operator",
            );
            HoldOutcome::Veto
        }
        InterventionDecision::AsRolled => HoldOutcome::AsRolled,
        InterventionDecision::Deal { run, reasks, .. } => {
            let before = facts_of(
                definition,
                result,
                &presentation_for(definition, result.visibility, None, None),
            );
            let after = facts_of(definition, &run.result, &run.presentation);
            let changed = changed_fields(&before, &after);
            let changed_json = serde_json::to_string(&changed).unwrap_or_default();
            tracing::info!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = params.chat_id,
                tool = definition.name.as_str(),
                changedJson = changed_json.as_str(),
                reasks = reasks as u64,
                staleAnswer = run.stale_answer,
                "Custom tool run dealt by the operator",
            );
            if changed.is_empty() {
                return HoldOutcome::AsRolled;
            }
            // `resolvedAt` is stamped at RESOLVE time (v4 `new Date()` after
            // the await — R-11), never the run's `now_ms`.
            let mut record = Map::new();
            record.insert(
                "resolvedAt".into(),
                json!(crate::clock::iso_from_unix_ms(crate::clock::now_unix_ms())),
            );
            record.insert("decision".into(), json!("deal"));
            record.insert("changed".into(), json!(changed));
            record.insert("dealt".into(), before.to_wire());
            if reasks > 0 {
                record.insert("reasks".into(), json!(reasks));
            }
            if run.stale_answer {
                record.insert("staleAnswer".into(), json!(true));
            }
            HoldOutcome::Deal {
                run,
                record: Value::Object(record),
            }
        }
    }
}

// === P4.D329: the rolling character's bag ===
/// v4 `loadRunInventory` — the rolling character's Inventory for this run:
/// loaded once, and only when the definition tests, quotes, draws from or
/// changes it (its vocabulary names a read, an aggregate, or a gesture). `None`
/// for a run nobody made, a definition that never mentions the bag, or a vault
/// that cannot be read (logged). A different rule from the roster's lazy load
/// (a gate naming an inventory subject) — two thunks, two tests.
fn load_run_inventory(
    db: &Db,
    definition: &super::custom_tool_types::QtapCustomTool,
    character_id: Option<&str>,
    chat_id: &str,
) -> Option<Arc<dyn InventoryContext + Send + Sync>> {
    let character_id = character_id.filter(|c| !c.is_empty())?;
    let vocabulary = collect_tool_vocabulary(definition);
    let needed = vocabulary.inventory_reads.len()
        + vocabulary.inventory_aggregates.len()
        + vocabulary.equips.len()
        + vocabulary.unequips.len()
        + vocabulary.picks_up.len()
        > 0;
    if !needed {
        return None;
    }
    let loaded = db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(load_inventory_context(
                main,
                mount,
                character_id,
                Some(chat_id),
            ))
        })
    });
    let error = match loaded {
        Ok(Ok(loaded)) => return Some(Arc::new(loaded.context)),
        Ok(Err(e)) => e.to_string(),
        Err(e) => e.to_string(),
    };
    tracing::warn!(
        target: "quilltap::pascal",
        context = CONTEXT,
        chatId = chat_id,
        characterId = character_id,
        tool = definition.name.as_str(),
        error,
        "Custom tool run: the rolling character's inventory could not be read; inventory tests decline",
    );
    None
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
                character_name: None,
                whisper_target_ids: Some(vec!["u".into()]),
                invoked_by: InvokedBy::Trigger,
                caller_participant_id: None,
                trigger_phrase: Some("flip it"),
                now_ms: 0,
                pending_wardrobe_announcements: None,
                pending_wardrobe_history: None,
                pending_inventory_announcements: None,
                may_hold: false,
                stop: None,
            },
            &mut crate::tools::rng::FixedBytes::new(vec![0; 6]),
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
    /// and NOT the refused line, and (no chat) NOT the cascade line. P4.D319:
    /// `announcedBy` now rides between `whispered` and `posted` (v4
    /// `a05c6f755`), and the writer's own `dropped` DEBUG (R-I — the writer
    /// was silent before) precedes it, the chat being absent.
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
                "DEBUG quilltap::pascal Pascal outcome dropped — no such chat context=pascal \
                 chatId=c tool=coin"
                    .to_string(),
                "DEBUG quilltap::pascal Custom tool dealt context=pascal.run-service chatId=c \
                 tool=coin tier=character invokedBy=trigger state=info effects=0 \
                 whispered=false announcedBy=pascal posted=false"
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
                &mut crate::tools::rng::FixedBytes::new(vec![0; 6]),
                None,
                None,
                0,
                None,
                None,
                None,
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
                None,
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

    /// v4 `run-service.ts:186` spreads `chipLabel` on TRUTHINESS: a label
    /// template that renders empty (a string parameter defaulting to `""` —
    /// every triggered run deals defaults) leaves no key, where a present
    /// `Some("")` would write `"chipLabel":""` (the `25b8fab6a` unification's
    /// §3 review catch).
    #[test]
    fn an_empty_rendered_chip_label_is_omitted() {
        let entry = entry();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut result = rt
            .block_on(crate::pascal::custom_tools::execute_custom_tool(
                &entry.definition,
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
            .expect("dealt");
        let meta = |result: &CustomToolRunResult| {
            serde_json::to_string(&build_pascal_meta(
                result,
                "Coin",
                &entry,
                &[],
                InvokedBy::Trigger,
                None,
                None,
                None,
            ))
            .unwrap()
        };
        result.chip_label = Some(String::new());
        let empty = meta(&result);
        assert!(!empty.contains("chipLabel"), "{empty}");
        result.chip_label = Some("Heads".into());
        let labelled = meta(&result);
        assert!(
            labelled.contains(r#""toolTitle":"Coin","chipLabel":"Heads","definitionTier""#),
            "{labelled}"
        );
    }
}

#[cfg(test)]
mod inventory_read_tests {
    //! P4.D329 (R-15): `load_run_inventory`'s capture pins, each with its
    //! silence leg. The loader is the real `load_inventory_context` (v4's
    //! `loadInventoryContext`), whose reads are v4's fallbacks: on a fresh
    //! instance (no such character, no such chat) it loads an empty bag
    //! SILENTLY. The WARN arm is a read that fails outright — a degraded mount
    //! index — and is pinned whole.
    use super::*;
    use crate::test_support::global_capture;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "a1000000-0000-4000-8000-00000000000a";

    fn db() -> (Db, tempfile::TempDir) {
        open(true)
    }

    /// A fresh instance, its mount index attached or not (degraded).
    fn open(with_mount: bool) -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: with_mount.then(|| data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (db, dir)
    }

    fn definition(extra: Value) -> super::super::custom_tool_types::QtapCustomTool {
        let mut doc = json!({
            "name": "probe",
            "description": "A probe.",
            "outcomes": [{ "when": true, "message": "-", "state": "info" }],
        });
        for (k, v) in extra.as_object().unwrap() {
            doc[k] = v.clone();
        }
        crate::pascal::custom_tool_types::safe_parse(&doc).expect("the definition loads")
    }

    fn read(
        db: &Db,
        definition: &super::super::custom_tool_types::QtapCustomTool,
        character_id: Option<&str>,
    ) -> (bool, Vec<String>) {
        global_capture::install();
        let (bag, lines) =
            global_capture::capture(|| load_run_inventory(db, definition, character_id, "chat-1"));
        (
            bag.is_some(),
            lines
                .into_iter()
                .filter(|l| l.contains(" quilltap::pascal "))
                .collect(),
        )
    }

    #[test]
    fn a_tool_naming_no_inventory_reads_nothing() {
        let (db, _dir) = db();
        let (bag, lines) = read(&db, &definition(json!({})), Some(CHAR));
        assert!(!bag);
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn nobody_rolled_reads_nothing() {
        let (db, _dir) = db();
        let d = definition(
            json!({ "outcomes": [{ "when": true, "message": "{{inventory.sword.title}}", "state": "info" }] }),
        );
        for who in [None, Some("")] {
            let (bag, lines) = read(&db, &d, who);
            assert!(!bag);
            assert!(lines.is_empty(), "{lines:?}");
        }
    }

    #[test]
    fn every_door_reads_the_bag() {
        let (db, _dir) = db();
        let (degraded, _ddir) = open(false);
        // A message placeholder, a parameter default, a consult prompt, a
        // gesture and an aggregate: each names the bag, so each reads it — a
        // bag on a readable instance (silently: v4's fallback reads), the
        // WARN when the read itself fails.
        for extra in [
            json!({ "outcomes": [{ "when": true, "message": "{{inventory.sword.title}}", "state": "info" }] }),
            json!({ "parameters": { "b": { "type": "number", "default": { "$inventory": "sword.metadata.bonus", "fallback": 0 } } } }),
            json!({ "llm": { "prompt": "Describe {{inventory.sword.title}}.", "errorMessage": "Silence." } }),
            json!({ "effects": [{ "inventory": { "action": "unequip", "items": ["cloak"] } }] }),
            json!({ "availableWhen": { "inventoryAggregate": { "owned.count": { "gte": 1 } } } }),
        ] {
            let d = definition(extra.clone());
            let (bag, lines) = read(&db, &d, Some(CHAR));
            assert!(bag, "{extra}");
            assert!(lines.is_empty(), "{extra}: {lines:?}");

            let (bag, lines) = read(&degraded, &d, Some(CHAR));
            assert!(!bag, "{extra}");
            assert_eq!(
                lines,
                vec![format!(
                    "WARN quilltap::pascal Custom tool run: the rolling character's inventory could not be read; inventory tests decline context=pascal.run-service chatId=chat-1 characterId={CHAR} tool=probe error=partition not available: mountIndex"
                )],
                "{extra}"
            );
        }
    }
}

#[cfg(test)]
mod rolling_character_tests {
    //! P4.D319 (v4 `a05c6f755`): v4's six `run-service.test.ts` `the rolling
    //! character` `it`s, over a real provisioned `Db` — the name's tri-state
    //! (R-1), the RAW read (R-2) and the announcer.
    use super::*;
    use crate::db::tiered_mount_pool::MountTier;
    use crate::test_support::global_capture;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
    const CHAR: &str = "a1000000-0000-4000-8000-00000000000a";
    const SEAT: &str = "b1000000-0000-4000-8000-000000000001";

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

    /// A one-seat chat and the rolling character's RAW row, named `raw_name`.
    /// `characterDocumentMountPointId` names a mount that does not exist — a
    /// vault the overlay could never read — so a run that names the character
    /// proves the read was the RAW one (R-2).
    async fn seed(db: &Db, raw_name: &'static str) {
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "The Rolling Room",
            "participants": [
                { "id": SEAT, "type": "CHARACTER", "controlledBy": "llm",
                  "characterId": CHAR,
                  "createdAt": "2026-05-01T00:00:00.000Z",
                  "updatedAt": "2026-05-01T00:00:00.000Z" }
            ],
        }))
        .expect("a ChatCreate");
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: "2026-05-01T00:00:00.000Z".to_string(),
            updated_at: "2026-05-01T00:00:00.000Z".to_string(),
        };
        db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection()).create(&create, &opts)?;
            w.main().connection().execute(
                "INSERT INTO characters (id, userId, name, characterDocumentMountPointId, \
                 createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                rusqlite::params![
                    CHAR,
                    crate::api::SINGLE_USER_ID,
                    raw_name,
                    "f0000000-0000-4000-8000-0000000000ff",
                    "2026-05-01T00:00:00.000Z"
                ],
            )?;
            Ok(())
        })
        .await
        .expect("seed");
    }

    fn entry(announce: Option<bool>) -> DiscoveredCustomTool {
        let mut raw = json!({
            "name": "dresser",
            "description": "Roll for it.",
            "roll": { "min": 3, "max": 3 },
            "chipLabel": "For {{character}}",
            "outcomes": [{ "when": true, "message": "{{character}} rolls {{value}}", "state": "info" }],
        });
        if let Some(a) = announce {
            raw["announceAsCharacter"] = json!(a);
        }
        let definition =
            crate::pascal::custom_tool_types::safe_parse(&raw).expect("the definition loads");
        DiscoveredCustomTool {
            definition,
            tier: MountTier::Character,
            mount_point_id: "m1".into(),
            mount_name: "Vault".into(),
            definition_path: "Tools/dresser.tool.json".into(),
        }
    }

    async fn run(
        db: &Db,
        entry: &DiscoveredCustomTool,
        character_id: Option<&str>,
        character_name: Option<Option<&str>>,
    ) -> (PostedPascalMessage, Vec<String>) {
        let metadata = Map::new();
        let (outcome, lines) = global_capture::capture_async(run_custom_tool(
            db,
            RunCustomToolParams {
                user_id: crate::api::SINGLE_USER_ID,
                chat_id: CHAT,
                chat: None,
                entry,
                parameters: None,
                private: None,
                character_id,
                metadata: &metadata,
                character_name,
                whisper_target_ids: None,
                invoked_by: InvokedBy::Llm,
                caller_participant_id: None,
                trigger_phrase: None,
                now_ms: 0,
                pending_wardrobe_announcements: None,
                pending_wardrobe_history: None,
                pending_inventory_announcements: None,
                may_hold: false,
                stop: None,
            },
            &mut crate::tools::rng::FixedBytes::new(vec![0; 6]),
            None,
        ))
        .await;
        let RunCustomToolOutcome::Ok {
            posted: Some(posted),
            ..
        } = outcome.expect("never Err")
        else {
            panic!("dealt and posted");
        };
        let lines = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal "))
            .collect();
        (posted, lines)
    }

    fn content(posted: &PostedPascalMessage) -> &str {
        posted.message["content"].as_str().unwrap()
    }

    /// v4 `renders {{character}} with the name the entrance supplied, without
    /// a read` — the RAW row says otherwise, and the supplied copy wins.
    #[tokio::test(flavor = "current_thread")]
    async fn renders_the_supplied_name_without_a_read() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "Raw Bertie").await;
        let (posted, _) = run(&db, &entry(None), Some(CHAR), Some(Some("  Supplied  "))).await;
        assert_eq!(content(&posted), "🎲 **For Supplied**\n\nSupplied rolls 3");
    }

    /// v4 `reads the name when the entrance did not supply one` — the RAW row,
    /// trimmed twice over, and named even though its vault cannot be read.
    #[tokio::test(flavor = "current_thread")]
    async fn reads_the_raw_name_when_none_is_supplied() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "  Bertie  ").await;
        let (posted, _) = run(&db, &entry(None), Some(CHAR), None).await;
        assert_eq!(content(&posted), "🎲 **For Bertie**\n\nBertie rolls 3");
    }

    /// v4's `null` — the entrance said "nobody", so no read: as written.
    #[tokio::test(flavor = "current_thread")]
    async fn an_explicit_nobody_reads_nothing_and_renders_as_written() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "Raw Bertie").await;
        let (posted, _) = run(&db, &entry(None), Some(CHAR), Some(None)).await;
        assert_eq!(
            content(&posted),
            "🎲 **For {{character}}**\n\n{{character}} rolls 3"
        );
    }

    /// v4 `leaves {{character}} as written when the name cannot be read` —
    /// v4 reaches its catch only by mock (`findByIdRaw` is a fallback read);
    /// v5's home is the read connection itself failing (R-2). A fresh `Db` has
    /// opened no read connection yet, so removing the main file makes the next
    /// checkout fail.
    #[tokio::test(flavor = "current_thread")]
    async fn an_unreadable_name_renders_as_written_and_says_why() {
        global_capture::install();
        let (db, dir) = db();
        std::fs::remove_file(dir.path().join("data").join("quilltap.db")).unwrap();
        let (name, lines) =
            global_capture::capture(|| resolve_character_name(&db, Some(CHAR), None, CHAT));
        assert_eq!(name, None);
        let lines: Vec<_> = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal "))
            .collect();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(&format!(
                "DEBUG quilltap::pascal Custom tool run: rolling character name unavailable, \
                 {{{{character}}}} renders as written context=pascal.run-service chatId={CHAT} \
                 characterId={CHAR} error="
            )),
            "{}",
            lines[0]
        );
        assert!(
            !lines[0].contains("sqlite error"),
            "the bare message: {}",
            lines[0]
        );
    }

    /// The silence legs: a supplied name and a readable row log nothing of
    /// the name's own.
    #[tokio::test(flavor = "current_thread")]
    async fn a_resolved_name_logs_nothing() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "Bertie").await;
        for supplied in [None, Some(Some("Bertie")), Some(None)] {
            let (_, lines) =
                global_capture::capture(|| resolve_character_name(&db, Some(CHAR), supplied, CHAT));
            assert!(lines.is_empty(), "{lines:?}");
        }
        // A run nobody made never reads either.
        let (name, lines) =
            global_capture::capture(|| resolve_character_name(&db, None, None, CHAT));
        assert_eq!(name, None);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// v4 `posts under Pascal unless the tool says the character announces it`.
    #[tokio::test(flavor = "current_thread")]
    async fn posts_under_pascal_by_default() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "Bertie").await;
        for announce in [None, Some(false)] {
            let (posted, lines) = run(&db, &entry(announce), Some(CHAR), None).await;
            assert!(posted.message.get("customAnnouncer").is_none());
            assert_eq!(posted.message["systemSender"], "pascal");
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains("Custom tool dealt") && l.contains(" announcedBy=pascal ")),
                "{lines:?}"
            );
        }
    }

    /// v4 `hands the post to the rolling character when announceAsCharacter is
    /// on` — `customAnnouncer` LAST, after `pascalMeta`; still Pascal's record.
    #[tokio::test(flavor = "current_thread")]
    async fn hands_the_post_to_the_rolling_character() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "Bertie").await;
        let (posted, lines) = run(&db, &entry(Some(true)), Some(CHAR), None).await;
        let keys: Vec<&str> = posted
            .message
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(&keys[keys.len() - 2..], &["pascalMeta", "customAnnouncer"]);
        assert_eq!(
            serde_json::to_string(&posted.message["customAnnouncer"]).unwrap(),
            format!(r#"{{"kind":"character","characterId":"{CHAR}"}}"#)
        );
        assert_eq!(posted.message["systemSender"], "pascal");
        assert_eq!(posted.message["systemKind"], "custom-tool-result");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Custom tool dealt") && l.contains(" announcedBy=character ")),
            "{lines:?}"
        );
        // The persisted row carries the column, as `{kind, characterId}`.
        let id = posted.id.clone();
        let stored: Option<String> = db
            .read_main(move |m| {
                Ok(m.query_row(
                    "SELECT customAnnouncer FROM chat_messages WHERE id = ?1",
                    [&id],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(
            stored.as_deref(),
            Some(format!(r#"{{"kind":"character","characterId":"{CHAR}"}}"#).as_str())
        );
    }

    /// v4 `leaves a run nobody made with Pascal, even when announceAsCharacter
    /// is on`.
    #[tokio::test(flavor = "current_thread")]
    async fn a_run_nobody_made_stays_pascals() {
        global_capture::install();
        let (db, _dir) = db();
        seed(&db, "Bertie").await;
        let (posted, _) = run(&db, &entry(Some(true)), None, None).await;
        assert!(posted.message.get("customAnnouncer").is_none());
        assert_eq!(
            content(&posted),
            "🎲 **For {{character}}**\n\n{{character}} rolls 3"
        );
    }
}

// === P4.D345: the hold's own pins (each line with its silence leg) ===
#[cfg(test)]
mod hold_tests {
    use super::*;
    use crate::db::tiered_mount_pool::MountTier;
    use crate::pascal::intervention::registry::{
        list_interventions, settle_intervention, InterventionDecision, SettleOutcome,
    };
    use crate::realtime::publish_sites::HintCapture;
    use crate::test_support::global_capture;
    use serde_json::json;
    use std::time::Duration;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

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

    fn entry(intervention: Value) -> DiscoveredCustomTool {
        let definition = crate::pascal::custom_tool_types::safe_parse(&json!({
            "name": "coin",
            "description": "Flip a coin.",
            "intervention": intervention,
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

    fn chat_id() -> String {
        format!("c-{}", uuid::Uuid::new_v4())
    }

    async fn run(
        db: &Db,
        entry: &DiscoveredCustomTool,
        chat_id: &str,
        chat: Option<&Value>,
        may_hold: bool,
        stop: Option<crate::services::turn_control::StopSignal>,
    ) -> RunCustomToolOutcome {
        let metadata = Map::new();
        run_custom_tool(
            db,
            RunCustomToolParams {
                user_id: "u",
                chat_id,
                chat,
                entry,
                parameters: None,
                private: None,
                character_id: None,
                metadata: &metadata,
                character_name: None,
                whisper_target_ids: Some(vec!["u".into()]),
                invoked_by: InvokedBy::Llm,
                caller_participant_id: None,
                trigger_phrase: None,
                now_ms: 0,
                pending_wardrobe_announcements: None,
                pending_wardrobe_history: None,
                pending_inventory_announcements: None,
                may_hold,
                stop,
            },
            &mut crate::tools::rng::FixedBytes::new(vec![0; 6]),
            None,
        )
        .await
        .expect("never Err")
    }

    fn pascal_lines(lines: Vec<String>) -> Vec<String> {
        lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal ") && !l.contains("Pascal outcome dropped"))
            .collect()
    }

    /// Wait (yielding) until the chat shows one hold; its id.
    async fn held(chat: &str) -> String {
        for _ in 0..2000 {
            if let Some(p) = list_interventions(chat).first() {
                return p.id.clone();
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("no hold appeared for {chat}");
    }

    /// The operator's own run never holds: the entrance's reason, then the
    /// deal. A NON-intervenable tool says nothing (the silence leg — and the
    /// service's other pins assert it on every run).
    #[tokio::test(flavor = "current_thread")]
    async fn the_operators_own_run_is_not_held_and_says_why() {
        global_capture::install();
        let (db, _dir) = db();
        let chat = chat_id();
        let (outcome, lines) = global_capture::capture_async(run(
            &db,
            &entry(json!({"enabled": true})),
            &chat,
            None,
            false,
            None,
        ))
        .await;
        assert!(matches!(outcome, RunCustomToolOutcome::Ok { .. }));
        let lines = pascal_lines(lines);
        assert_eq!(
            lines[0],
            format!(
                "DEBUG quilltap::pascal Custom tool run not held context=pascal.run-service \
                 chatId={chat} tool=coin reason={NOT_HELD_OPERATORS_OWN_RUN}"
            )
        );
        assert_eq!(lines.len(), 2, "{lines:?}");

        let (_, silent) = global_capture::capture_async(run(
            &db,
            &entry(json!({"enabled": false})),
            &chat,
            None,
            false,
            None,
        ))
        .await;
        assert!(!pascal_lines(silent).iter().any(|l| l.contains("not held")));
    }

    /// An autonomous room never holds.
    #[tokio::test(flavor = "current_thread")]
    async fn an_autonomous_room_is_not_held() {
        global_capture::install();
        let (db, _dir) = db();
        let chat = chat_id();
        let room = json!({"id": chat, "chatType": "autonomous", "participants": []});
        let (_, lines) = global_capture::capture_async(run(
            &db,
            &entry(json!({"enabled": true})),
            &chat,
            Some(&room),
            true,
            None,
        ))
        .await;
        assert!(pascal_lines(lines)[0].ends_with(&format!("reason={NOT_HELD_AUTONOMOUS}")),);
    }

    /// `running in the job child` is unreachable in v5 (the in-process runner
    /// never forks): the reason's bytes exist and no run can reach them.
    #[test]
    fn the_job_child_reason_is_pinned_negative() {
        assert!(!is_job_child());
        assert_eq!(NOT_HELD_JOB_CHILD, "running in the job child");
    }

    /// A held run waits — nothing posted — until the operator lets it stand;
    /// then it deals as rolled. The hold's INFO and the resolve's INFO, in
    /// order, on the waiting thread.
    #[tokio::test(flavor = "current_thread")]
    async fn a_held_run_waits_for_the_operator_and_deals_as_rolled() {
        global_capture::install();
        let (db, _dir) = db();
        let chat = chat_id();
        let tool = entry(json!({"enabled": true}));
        let ((outcome, settled), lines) = global_capture::capture_async(async {
            tokio::join!(run(&db, &tool, &chat, None, true, None), async {
                let id = held(&chat).await;
                settle_intervention(&id, InterventionDecision::AsRolled)
            })
        })
        .await;
        assert_eq!(settled, SettleOutcome::Resolved);
        assert!(matches!(outcome, RunCustomToolOutcome::Ok { .. }));
        let lines = pascal_lines(lines);
        assert!(lines[0].starts_with(
            "INFO quilltap::pascal Custom tool run held for the operator \
             context=pascal.intervention.registry"
        ));
        assert!(lines[0].ends_with("tool=coin entrance=llm characterId=null"));
        assert!(lines[1].contains("Custom tool hold resolved"));
        assert!(lines[1].ends_with("decision=as-rolled cause=dialog"));
        assert!(lines[2].contains("Custom tool dealt"));
        assert!(list_interventions(&chat).is_empty());
    }

    /// The Stop veto (I9) through `TurnStopReach`: a Stop while held settles
    /// the hold as `Veto { Stop }` through the registry's own door (its line,
    /// its publish), and the run answers `Vetoed` with the `cause=stop` INFO.
    #[tokio::test(flavor = "current_thread")]
    async fn stop_vetoes_a_held_run() {
        global_capture::install();
        let mut cap = HintCapture::start();
        let (db, _dir) = db();
        let chat = chat_id();
        let registration = crate::services::turn_control::register_active_turn(&chat);
        let stop = registration.handle().stop_signal();
        let tool = entry(json!({"enabled": true}));
        let ((outcome, ()), lines) = global_capture::capture_async(async {
            tokio::join!(run(&db, &tool, &chat, None, true, Some(stop)), async {
                held(&chat).await;
                crate::services::turn_control::stop_active_turns(
                    &chat,
                    Some(Duration::from_millis(5)),
                )
                .await;
            })
        })
        .await;
        drop(registration);
        assert!(matches!(outcome, RunCustomToolOutcome::Vetoed));
        let lines = pascal_lines(lines);
        assert!(lines[1].ends_with("decision=veto cause=stop"), "{lines:?}");
        assert_eq!(
            lines[2],
            format!(
                "INFO quilltap::pascal Custom tool run set aside by the operator \
                 context=pascal.run-service chatId={chat} tool=coin invokedBy=llm cause=stop"
            )
        );
        assert_eq!(lines.len(), 3, "nothing dealt, nothing posted: {lines:?}");
        let hints = cap.drain_expecting(1).await;
        assert_eq!(
            hints,
            vec![("customToolInterventions".to_string(), Some(chat.clone()))]
        );
        assert!(list_interventions(&chat).is_empty());
    }

    /// An ALREADY-stopped turn settles right after the add has been logged and
    /// published (v4's `queueMicrotask`): the add's line precedes the
    /// removal's, and the run is vetoed without waiting.
    #[tokio::test(flavor = "current_thread")]
    async fn an_already_stopped_turn_vetoes_after_the_add() {
        global_capture::install();
        let (db, _dir) = db();
        let chat = chat_id();
        let registration = crate::services::turn_control::register_active_turn(&chat);
        crate::services::turn_control::stop_active_turns(&chat, Some(Duration::from_millis(1)))
            .await;
        let stop = registration.handle().stop_signal();
        let (outcome, lines) = global_capture::capture_async(run(
            &db,
            &entry(json!({"enabled": true})),
            &chat,
            None,
            true,
            Some(stop),
        ))
        .await;
        drop(registration);
        assert!(matches!(outcome, RunCustomToolOutcome::Vetoed));
        let lines = pascal_lines(lines);
        assert!(lines[0].contains("Custom tool run held for the operator"));
        assert!(lines[1].ends_with("decision=veto cause=stop"));
    }

    /// `pascalMeta.intervention` is the LAST key, after `callerParticipantId`.
    #[test]
    fn the_intervention_record_rides_last() {
        let tool = entry(json!({"enabled": true}));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let result = rt
            .block_on(crate::pascal::custom_tools::execute_custom_tool(
                &tool.definition,
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
            .expect("dealt");
        let record = json!({"resolvedAt": "2026-11-03T00:00:00.000Z", "decision": "deal"});
        let meta = build_pascal_meta(
            &result,
            "Coin",
            &tool,
            &[],
            InvokedBy::Llm,
            None,
            Some("p1"),
            Some(&record),
        );
        assert_eq!(
            meta.keys().rev().take(2).cloned().collect::<Vec<_>>(),
            vec![
                "intervention".to_string(),
                "callerParticipantId".to_string()
            ]
        );
    }
}
