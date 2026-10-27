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

    // === P4.D329: the rolling character's bag, BETWEEN the cascade read and
    // the deal (v4 `loadRunInventory`) ===
    let inventory = load_run_inventory(db, &entry.definition, character_id, chat_id);

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
        character_name.as_deref(),
        // P4.D329: the LAST positional.
        inventory.as_deref().map(|i| i as &dyn InventoryContext),
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

    // `announceAsCharacter`: the rolling character speaks the outcome in
    // Pascal's place. Still Pascal's record (systemSender, pascalMeta, the
    // chip) — only the face and name on the bubble, and the speaker tag the
    // model reads, change. A run nobody made has no one to speak it, so Pascal
    // does. (v4 `=== true ? characterId : null`, then the truthy spread.)
    let announcer_character_id = if entry.definition.announce_as_character == Some(true) {
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
    pascal_meta
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
                &mut crate::tools::rng::FixedBytes::new(vec![]),
                None,
                None,
                0,
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
                &mut crate::tools::rng::FixedBytes::new(vec![]),
                None,
                None,
                0,
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
            },
            &mut crate::tools::rng::FixedBytes::new(vec![]),
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
