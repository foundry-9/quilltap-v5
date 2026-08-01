//! The `run_custom` LLM tool — v4 `lib/tools/run-custom-tool.ts` (the pure half)
//! + `lib/tools/handlers/run-custom-handler.ts` (the DB-integrated handler).
//!
//! A single tool through which a model runs any of the user-authored
//! pseudo-tools on Pascal's roster (`Tools/*.tool.json` documents in the chat's
//! document stores). One tool rather than one per definition: the tool list
//! stays stable for prompt caching, the snapshot doesn't churn as users write
//! new files, and the Zod chokepoint stays honest. The roster rides IN the
//! description, rebuilt on every tool build.
//!
//! The roll happens server-side and the outcome is persisted as a message the
//! model did not author, so a failure cannot be talked into a success and
//! regenerating a reply does not re-roll.

use serde_json::{json, Map, Value};

use crate::db::characters_read;
use crate::db::runtime::Db;
use crate::pascal::custom_tool_types::{
    display_title, AnyOperand, LlmComparator, NumberOrParamRef, NumericComparator, ParamComparator,
    ParamRef, Roll, RollRange, StringOperand, Visibility, When, WhenObject,
};
use crate::pascal::custom_tools::{
    execute_custom_tool, CustomToolRunError, CustomToolRunResult, LlmInvoker, ResolvedParams,
    RollForm,
};
use crate::pascal::js_value::{json_stringify, number_to_string};
use crate::pascal::roster::{resolve_custom_tool_roster, DiscoveredCustomTool, RosterContext};
use crate::pascal::side_effects::apply_effects_for_run;
use crate::services::pascal_writer::{
    build_pascal_result_content, post_pascal_result, PostPascalResultParams, PostedPascalMessage,
};
use crate::services::prospero_notifications::{
    post_prospero_custom_tool_error, ProsperoCustomToolErrorArgs,
};
use crate::state::cascade::StateCascadeResult;
use crate::tools::rng::RandomBytes;

// ===========================================================================
// Input schema + validation (v4 `runCustomToolInputSchema` / `validateRunCustomInput`)
// ===========================================================================

/// The validated `run_custom` input. Mirrors v4's Zod `runCustomToolInputSchema`
/// (`{ tool, parameters?, private? }`), after unknown keys are stripped.
#[derive(Debug, Clone, PartialEq)]
pub struct RunCustomToolInput {
    /// Name of the custom tool to run, exactly as listed in the description.
    pub tool: String,
    /// Arguments keyed by parameter name. `None` for both an absent and an
    /// explicit-null `parameters` (v4's `parameters ?? null`).
    pub parameters: Option<Map<String, Value>>,
    /// Roll privately. `None` defers to the tool's own `defaultVisibility`.
    pub private: Option<bool>,
}

/// Validate `run_custom` input the way v4's `z.object({...}).safeParse` does:
/// `tool` a required string, `parameters` an optional nullable record, `private`
/// an optional boolean; unknown keys stripped (the object schema is not strict).
/// `None` when the shape does not parse.
pub fn validate_run_custom_input(input: &Value) -> Option<RunCustomToolInput> {
    let obj = input.as_object()?;

    // `tool: z.string()` — required.
    let tool = obj.get("tool")?.as_str()?.to_string();

    // `parameters: z.record(z.string(), z.unknown()).nullable().optional()`:
    // absent OR null → no params; an object → those params; any other type fails.
    let parameters = match obj.get("parameters") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => Some(map.clone()),
        Some(_) => return None,
    };

    // `private: z.boolean().optional()` — present must be a bool.
    let private = match obj.get("private") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };

    Some(RunCustomToolInput {
        tool,
        parameters,
        private,
    })
}

// ===========================================================================
// The tool description — the roster rides inside it (v4 `buildRunCustomDescription`)
// ===========================================================================

/// Fixed preamble; the roster is appended per build.
///
/// The metadata sentence says only that outcome tables MAY consult the sheet —
/// never which keys exist or what they hold. Those are per-character and often
/// the point of the table (a `revealOdds: false` lock that opens for whoever
/// carries the key); enumerating them here would leak every character's secrets
/// into every participant's tool block on every call.
const RUN_CUSTOM_PREAMBLE: &str = concat!(
    "Run one of this scene's custom tools — user-authored actions with a random outcome.\n",
    "The roll happens server-side and its result is posted as a permanent message by Pascal the Croupier, so you cannot choose the outcome: run the tool and then narrate whatever it returns, including failure.\n",
    "Do not describe the result before calling, and do not re-run a tool to get a better answer.\n",
    "An outcome table may also consult your own character's metadata, so the same tool can deal differently to different characters.\n",
    "Some tools additionally pose a question to a separate model mid-run and let its answer steer the outcome; that consult happens server-side too, and you never speak for it.\n",
    "Some tools record side effects when they run — adjusting the scene's persistent state or the rolling character's own records, server-side, as part of the roll.\n",
    "\n",
    "Available tools:"
);

/// Render a `NumberOrParamRef` operand bare (v4 template coercion `${v}`):
/// a `$param` reference prints its target's name, a literal its JS `String(n)`.
fn render_nopr(v: &NumberOrParamRef) -> String {
    match v {
        NumberOrParamRef::ParamRef(ParamRef { param }) => param.clone(),
        NumberOrParamRef::Number(n) => number_to_string(*n),
        // v4's `${v}` template coercion on a $state object → "[object Object]".
        NumberOrParamRef::StateRef(_) => "[object Object]".to_string(),
    }
}

/// Render a roll spec compactly for the roster listing (v4 `describeRoll`).
fn describe_roll(roll: &Option<Roll>) -> String {
    match roll {
        None => "random value from 0 to 1".to_string(),
        Some(Roll::Dice(notation)) => format!("dice {notation}"),
        Some(Roll::Range(range)) => describe_roll_range(range),
    }
}

fn describe_roll_range(range: &RollRange) -> String {
    // v4's `part('', v, fallback)`: undefined → fallback, else the bare render.
    let part = |v: &Option<NumberOrParamRef>, fallback: &str| -> String {
        match v {
            None => fallback.to_string(),
            Some(x) => render_nopr(x),
        }
    };

    let mut bits = vec![format!(
        "random value from {} to {}",
        part(&range.min, "0"),
        part(&range.max, "1")
    )];
    if let Some(m) = &range.multiplier {
        bits.push(format!("× {}", render_nopr(m)));
    }
    if let Some(o) = &range.offset {
        bits.push(format!("+ {}", render_nopr(o)));
    }
    if range.round == Some(true) {
        bits.push("rounded".to_string());
    }
    bits.join(" ")
}

/// The comparator keys, paired with the operator a reader expects to see.
const COMPARATOR_SYMBOLS: [(&str, &str); 8] = [
    ("gt", ">"),
    ("gte", ">="),
    ("lt", "<"),
    ("lte", "<="),
    ("eq", "="),
    ("neq", "!="),
    ("contains", "contains"),
    ("ncontains", "does not contain"),
];

/// Render a containment operand: a literal string (JSON-quoted, exactly as
/// `describe_any_operand` renders one) or the parameter it points at.
fn describe_string_operand(operand: &StringOperand) -> String {
    match operand {
        StringOperand::ParamRef(ParamRef { param }) => param.clone(),
        StringOperand::String(s) => json_stringify(&Value::String(s.clone())),
        // v4 `describeOperand` has no $state arm: `String(object)` → "[object Object]".
        StringOperand::StateRef(_) => "[object Object]".to_string(),
    }
}

/// Render a comparator operand: a literal, or the parameter it points at
/// (v4 `describeOperand`). A string operand is JSON-quoted; a number is
/// `String(n)`; a boolean is `"true"`/`"false"`.
fn describe_any_operand(operand: &AnyOperand) -> String {
    match operand {
        AnyOperand::ParamRef(ParamRef { param }) => param.clone(),
        AnyOperand::String(s) => json_stringify(&Value::String(s.clone())),
        AnyOperand::Number(n) => number_to_string(*n),
        AnyOperand::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        // v4 `describeOperand` has no $state arm: `String(object)` → "[object Object]".
        AnyOperand::StateRef(_) => "[object Object]".to_string(),
    }
}

/// `describeOperand` for a numeric-only operand (bare `value`/`roll` tests).
fn describe_numeric_operand(operand: &NumberOrParamRef) -> String {
    match operand {
        NumberOrParamRef::ParamRef(ParamRef { param }) => param.clone(),
        NumberOrParamRef::Number(n) => number_to_string(*n),
        // v4 `describeOperand` has no $state arm: `String(object)` → "[object Object]".
        NumberOrParamRef::StateRef(_) => "[object Object]".to_string(),
    }
}

/// One present numeric comparator key → `"<subject> <symbol> <operand>"`.
fn describe_numeric_comparator(cmp: &NumericComparator, subject: &str) -> Vec<String> {
    let by_key = |key: &str| -> Option<&NumberOrParamRef> {
        match key {
            "gt" => cmp.gt.as_ref(),
            "gte" => cmp.gte.as_ref(),
            "lt" => cmp.lt.as_ref(),
            "lte" => cmp.lte.as_ref(),
            "eq" => cmp.eq.as_ref(),
            "neq" => cmp.neq.as_ref(),
            _ => None,
        }
    };
    COMPARATOR_SYMBOLS
        .iter()
        .filter_map(|(key, symbol)| {
            by_key(key).map(|op| format!("{subject} {symbol} {}", describe_numeric_operand(op)))
        })
        .collect()
}

/// One present param/metadata comparator key → `"<subject> <symbol> <operand>"`.
/// `eq`/`neq` widen to strings and booleans (an [`AnyOperand`]).
fn describe_param_comparator(cmp: &ParamComparator, subject: &str) -> Vec<String> {
    let numeric = |key: &str| -> Option<&NumberOrParamRef> {
        match key {
            "gt" => cmp.gt.as_ref(),
            "gte" => cmp.gte.as_ref(),
            "lt" => cmp.lt.as_ref(),
            "lte" => cmp.lte.as_ref(),
            _ => None,
        }
    };
    COMPARATOR_SYMBOLS
        .iter()
        .filter_map(|(key, symbol)| match *key {
            "eq" => cmp
                .eq
                .as_ref()
                .map(|op| format!("{subject} {symbol} {}", describe_any_operand(op))),
            "neq" => cmp
                .neq
                .as_ref()
                .map(|op| format!("{subject} {symbol} {}", describe_any_operand(op))),
            "contains" => cmp
                .contains
                .as_ref()
                .map(|op| format!("{subject} {symbol} {}", describe_string_operand(op))),
            "ncontains" => cmp
                .ncontains
                .as_ref()
                .map(|op| format!("{subject} {symbol} {}", describe_string_operand(op))),
            other => numeric(other)
                .map(|op| format!("{subject} {symbol} {}", describe_numeric_operand(op))),
        })
        .collect()
}

/// Render an `llm` test (v4 `describeLlmComparator`): the `ok` flag reads as a
/// sentence, the rest as ordinary comparators against "the consulted answer".
fn describe_llm_comparator(cmp: &LlmComparator) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(ok) = cmp.ok {
        parts.push(
            if ok {
                "the consult succeeded"
            } else {
                "the consult failed"
            }
            .to_string(),
        );
    }
    // v4 hands the whole comparator to `describeComparator`, which reads it as a
    // ParamComparator — so the eight keys render exactly as they do elsewhere.
    let as_param = ParamComparator {
        gt: cmp.gt.clone(),
        gte: cmp.gte.clone(),
        lt: cmp.lt.clone(),
        lte: cmp.lte.clone(),
        eq: cmp.eq.clone(),
        neq: cmp.neq.clone(),
        contains: cmp.contains.clone(),
        ncontains: cmp.ncontains.clone(),
    };
    parts.extend(describe_param_comparator(&as_param, "the consulted answer"));
    parts
}

/// The bare `value` comparator keys of a `when` object (all `NumberOrParamRef`).
fn describe_value_comparator(when: &WhenObject) -> Vec<String> {
    let cmp = NumericComparator {
        gt: when.gt.clone(),
        gte: when.gte.clone(),
        lt: when.lt.clone(),
        lte: when.lte.clone(),
        eq: when.eq.clone(),
        neq: when.neq.clone(),
    };
    describe_numeric_comparator(&cmp, "value")
}

/// Render an outcome test (v4 `describeWhen`). Every subject is named — the bare
/// comparators are about `value`, so saying so keeps them legible beside a
/// `roll` or a parameter test.
fn describe_when(when: &When) -> String {
    let when = match when {
        When::CatchAll(_) => return "otherwise".to_string(),
        When::Object(w) => w,
    };

    let mut parts: Vec<String> = Vec::new();
    parts.extend(describe_value_comparator(when));
    if let Some(roll) = &when.roll {
        parts.extend(describe_numeric_comparator(roll, "roll"));
    }
    if let Some(params) = &when.params {
        for (name, cmp) in params {
            parts.extend(describe_param_comparator(cmp, name));
        }
    }
    // Metadata clauses render like any other comparator — but ONLY here, inside
    // a table the author chose to reveal. `revealOdds: false` returns before
    // this is ever called, which is how an author keeps a metadata branch secret.
    if let Some(metadata) = &when.metadata {
        for (key, cmp) in metadata {
            parts.extend(describe_param_comparator(cmp, &format!("your {key}")));
        }
    }
    if let Some(llm) = &when.llm {
        parts.extend(describe_llm_comparator(llm));
    }

    parts.join(" and ")
}

/// Compose the tool description for a resolved roster (v4
/// `buildRunCustomDescription`).
///
/// Called on EVERY tool build, so a definition added, edited, or deleted
/// mid-chat is reflected on the very next LLM call — and a brand-new chat gets
/// the full roster on turn one with no initialisation step.
///
/// A tool with `revealOdds: false` contributes its name, description, and
/// parameters only: the model learns the tool exists and how to call it, but
/// never what the odds are.
///
/// `build_run_custom_description(&[])` MUST equal the empty-roster description
/// pinned in `tools/definitions/data.rs` (the §2 byte-identity obligation).
pub fn build_run_custom_description(roster: &[DiscoveredCustomTool]) -> String {
    let mut lines: Vec<String> = vec![RUN_CUSTOM_PREAMBLE.to_string()];

    for entry in roster {
        let definition = &entry.definition;
        lines.push(format!(
            "\n• {} — {}",
            definition.name, definition.description
        ));

        if let Some(params) = &definition.parameters {
            for (name, spec) in params {
                let bounds = if spec.min.is_some() || spec.max.is_some() {
                    let mut segs: Vec<String> = Vec::new();
                    if let Some(min) = spec.min {
                        segs.push(format!("min {}", number_to_string(min)));
                    }
                    if let Some(max) = spec.max {
                        segs.push(format!("max {}", number_to_string(max)));
                    }
                    format!(", {}", segs.join(", "))
                } else {
                    String::new()
                };
                let detail = match &spec.description {
                    Some(d) if !d.is_empty() => format!(" — {d}"),
                    _ => String::new(),
                };
                lines.push(format!(
                    "    - {name} ({}, default {}{bounds}){detail}",
                    spec.param_type.as_str(),
                    json_stringify(&spec.default),
                ));
            }
        }

        if definition.reveal_odds == Some(false) {
            lines.push("    The odds are not disclosed.".to_string());
            continue;
        }

        lines.push(format!("    Roll: {}", describe_roll(&definition.roll)));
        if definition.llm.is_some() {
            // The consult's existence is part of the odds; the prompt itself is
            // NOT rendered — it is instructions for a different model, and
            // quoting it here would invite this one to answer it.
            lines.push("    Consults a separate model; outcomes may test its answer.".to_string());
        }
        if let Some(effects) = definition.effects.as_ref().filter(|e| !e.is_empty()) {
            // Targets only — vocabulary, never values or conditions. Under
            // `revealOdds: false` this line is gone with the rest of the odds
            // (the `continue` above): the consequences stay as hidden as the
            // thresholds. (The human popup still sees the targets via the
            // vocabulary — the file is the user's own.)
            //
            // Deduplicated in DECLARATION order, v4's `[...new Set(...)]`.
            let mut targets: Vec<&str> = Vec::new();
            for effect in effects {
                if !targets.contains(&effect.target.as_str()) {
                    targets.push(&effect.target);
                }
            }
            lines.push(format!("    Side effects: writes {}", targets.join(", ")));
        }
        for outcome in &definition.outcomes {
            lines.push(format!(
                "    {} → {}",
                describe_when(&outcome.when),
                outcome.state.as_str()
            ));
        }
    }

    lines.join("\n")
}

// ===========================================================================
// The handler (v4 `lib/tools/handlers/run-custom-handler.ts`)
// ===========================================================================

/// Context for a `run_custom` execution (v4 `RunCustomToolContext`, minus the
/// `onPosted` sink — v5 returns the posted message instead, so both the
/// executor's SSE emit and the route's `messages: [pascalMessage]` reply read it
/// from the same return value).
#[derive(Debug, Clone, Default)]
pub struct RunCustomToolContext {
    pub user_id: String,
    pub chat_id: String,
    /// The rolling character — their vault is the `character` tier, and their
    /// `metadata.json` is the fact sheet outcome tables may consult.
    pub character_id: Option<String>,
    /// Fast path for the character's vault; the group tier still needs `character_id`.
    pub character_mount_point_id: Option<String>,
    /// Every character in the chat — their vaults form the `participant` tier.
    pub character_ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    /// The caller's participant id — the whisper target for a private roll.
    pub caller_participant_id: Option<String>,
}

/// The compact result handed back to the model (v4 `RunCustomToolOutput`).
#[derive(Debug, Clone, PartialEq)]
pub struct RunCustomToolOutput {
    pub success: bool,
    pub tool: Option<String>,
    pub value: Option<f64>,
    pub state: Option<String>,
    pub message: Option<String>,
    pub whispered: Option<bool>,
    pub error: Option<String>,
}

/// Shape the failure the same way whether it was a bad input or a bad roll
/// (v4 `failure`).
fn failure(error: impl Into<String>, tool: Option<&str>) -> RunCustomToolOutput {
    RunCustomToolOutput {
        success: false,
        tool: tool.map(str::to_string),
        value: None,
        state: None,
        message: None,
        whispered: None,
        error: Some(error.into()),
    }
}

/// Post the Prospero error bubble for a failed run and return the model's result
/// (v4 `reportFailure`). Pascal only ever announces genuine outcomes, so a
/// failure is Prospero's to report — there is no Pascal message for a roll that
/// never happened.
async fn report_failure(
    db: &Db,
    ctx: &RunCustomToolContext,
    tool_name: &str,
    reason: String,
    whisper: bool,
) -> RunCustomToolOutput {
    post_prospero_custom_tool_error(
        db,
        ProsperoCustomToolErrorArgs {
            chat_id: ctx.chat_id.clone(),
            tool_name: tool_name.to_string(),
            reason: reason.clone(),
            whisper,
            caller_participant_id: ctx.caller_participant_id.clone(),
        },
    )
    .await;
    failure(reason, Some(tool_name))
}

/// Convert an ordered [`ResolvedParams`] into the JSON object v4 stores.
fn params_to_object(params: &ResolvedParams) -> Map<String, Value> {
    params
        .iter()
        .map(|(k, v)| (k.clone(), v.to_value()))
        .collect()
}

/// Execute the `run_custom` tool (v4 `executeRunCustomTool`). Returns the model
/// result and, on a successful post, the persisted Pascal message (v4's
/// `onPosted` payload) so the caller can surface it over SSE / return it as
/// `messages: [pascalMessage]`.
pub async fn execute_run_custom_tool<R: RandomBytes + Send>(
    db: &Db,
    ctx: &RunCustomToolContext,
    input: &Value,
    rng: &mut R,
) -> (RunCustomToolOutput, Option<PostedPascalMessage>) {
    execute_run_custom_tool_with_consult(db, ctx, input, rng, None).await
}

/// The same handler with the LLM-consult seam in hand (v4 `a2d9a3c8`'s
/// `buildCustomToolLlmInvoker({ userId, chatId })` argument).
///
/// **Why the seam is a second entry point rather than a field on
/// [`RunCustomToolContext`]:** the invoker is per-run machinery, not run
/// context — v4 builds it fresh at the call site for the same reason. Since
/// P4.6bd the production executor (`tools/executor.rs`) builds a
/// `SeamConsultInvoker` over the assembled `ConsultRunner` and calls THIS
/// form, so a model-driven `run_custom` on an `llm` tool consults for real.
/// The four-argument wrapper above remains for callers that legitimately hold
/// no provider (focused tests, seamless assemblies); through it an
/// `llm`-bearing definition takes the fail-soft "no LLM invoker was available
/// in this context" path and shows the author's `errorMessage`.
pub async fn execute_run_custom_tool_with_consult<R: RandomBytes + Send>(
    db: &Db,
    ctx: &RunCustomToolContext,
    input: &Value,
    rng: &mut R,
    llm_invoke: Option<&dyn LlmInvoker>,
) -> (RunCustomToolOutput, Option<PostedPascalMessage>) {
    let Some(parsed) = validate_run_custom_input(input) else {
        return (
            failure(
                "Invalid input: `tool` must be the name of an available custom tool.",
                None,
            ),
            None,
        );
    };
    let RunCustomToolInput {
        tool: tool_name,
        parameters,
        private: is_private,
    } = parsed;

    // The rolling character's fact sheet, read BEFORE the roster: availability
    // gates are answered against it, so a tool the sheet disqualifies is never
    // even listed. The read hydrates the vault, and fails with the
    // vault-unavailable error when that vault is broken — which lands on the
    // same Prospero error bubble as any other refused run, since neither a gate
    // nor a table that consults metadata can honestly be dealt without it.
    //
    // The read goes through `find_by_id_raw` + the overlay rather than
    // `characters_read::find_by_id` so the sentence in the bubble is v4's own
    // `CharacterVaultUnavailableError` message (`find_by_id` erases it into a
    // generic `DbError::Key`); `api::custom_tools::resolve_metadata` reaches for
    // the same pair, for the same reason.
    let mut metadata: Map<String, Value> = Map::new();
    if let Some(character_id) = &ctx.character_id {
        let cid = character_id.clone();
        let read = db.read_main(|main| {
            db.read_mount_index(|mount| {
                let raw = characters_read::find_by_id_raw(main, &cid)?;
                let repo = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
                Ok(crate::db::vault_read_overlay::apply_document_store_overlay_one(&repo, raw))
            })
        });
        let read = match read {
            Ok(inner) => inner,
            Err(e) => Err(crate::db::vault_read_overlay::OverlayOneError::Db(e)),
        };
        match read {
            Ok(Some(character)) => {
                if let Some(map) = character.get("metadata").and_then(Value::as_object) {
                    metadata = map.clone();
                }
            }
            Ok(None) => {}
            Err(error) => {
                let reason = match error {
                    crate::db::vault_read_overlay::OverlayOneError::Db(e) => e.to_string(),
                    crate::db::vault_read_overlay::OverlayOneError::Unavailable(u) => format!(
                        "Character {} has no usable vault (characterDocumentMountPointId={}): \
                         properties.json missing",
                        u.character_id, u.mount_id
                    ),
                };
                return (
                    report_failure(
                        db,
                        ctx,
                        &tool_name,
                        format!("the rolling character's vault could not be read: {reason}"),
                        // No definition in hand yet, so its `defaultVisibility` is
                        // unknowable: only an explicit `private: true` can whisper
                        // this one.
                        is_private == Some(true),
                    )
                    .await,
                    None,
                );
            }
        }
    }

    // Resolved fresh: a definition added or edited mid-chat is live on this call.
    let roster_ctx = RosterContext {
        user_id: ctx.user_id.clone(),
        chat_id: ctx.chat_id.clone(),
        character_id: ctx.character_id.clone(),
        character_mount_point_id: ctx.character_mount_point_id.clone(),
        character_ids: ctx.character_ids.clone(),
        project_id: ctx.project_id.clone(),
        // The sheet the gates are answered against, already in hand.
        metadata: Some(metadata.clone()),
    };
    let roster = match db.read_main(|main| {
        db.read_mount_index(|mount| Ok(resolve_custom_tool_roster(&roster_ctx, main, mount)))
    }) {
        Ok(r) => r,
        Err(e) => return (failure(e.to_string(), Some(&tool_name)), None),
    };

    let Some(entry) = roster.get(&tool_name).cloned() else {
        // A name the model invented, a file the user just deleted, or a
        // definition this character's fact sheet is not offered. Either way this
        // is not worth an error bubble in the transcript — tell the model.
        let available: Vec<&str> = roster.tools.iter().map(|(k, _)| k.as_str()).collect();
        let msg = if available.is_empty() {
            format!("No custom tool named \"{tool_name}\". This scene has none.")
        } else {
            format!(
                "No custom tool named \"{tool_name}\". Available: {}.",
                available.join(", ")
            )
        };
        return (failure(msg, Some(&tool_name)), None);
    };

    let default_whisper = entry.definition.default_visibility == Some(Visibility::Whisper);

    let metadata_arg = if metadata.is_empty() {
        None
    } else {
        Some(&metadata)
    };

    // The state cascade (chat → project → group → general) the run's `$state`
    // references resolve against (v4 `f48f34dc`) — retained WHOLE, not just
    // `.merged`, because the effect applier needs the per-tier objects to decide
    // where a write lives (v4 `c4d4b0de`). Read once here; never re-read.
    // Scoped to the rolling character's own groups (Knowledge's rule).
    // Fail-soft: every `$state` ref carries a required fallback, so a run is
    // always dealable even if state can't be read — state effects then skip.
    let cascade: Option<StateCascadeResult> = {
        use crate::state::cascade::{resolve_state_cascade, GroupScope};
        let cid = ctx.chat_id.clone();
        let scope = match &ctx.character_id {
            Some(id) if !id.is_empty() => GroupScope::Character {
                character_id: id.clone(),
            },
            _ => GroupScope::None,
        };
        db.read_main(|main| {
            let Some(chat) = crate::db::chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };
            // A degraded (no mount-index) open still resolves the chat tier.
            let resolved = match db.read_mount_index(|mount| {
                Ok(resolve_state_cascade(main, Some(mount), &chat, &scope))
            }) {
                Ok(c) => c,
                Err(_) => resolve_state_cascade(main, None, &chat, &scope),
            };
            Ok(Some(resolved))
        })
        .unwrap_or(None)
    };
    let tool_state: Value = cascade
        .as_ref()
        .map(|c| c.merged.clone())
        .unwrap_or_else(|| Value::Object(Map::new()));

    let result: CustomToolRunResult = match execute_custom_tool(
        &entry.definition,
        parameters.as_ref(),
        is_private,
        metadata_arg,
        Some(&tool_state),
        rng,
        // v4 builds the invoker only when the definition declares an `llm`
        // block; withholding it otherwise is the same thing, since a tool with
        // no block never consults.
        entry.definition.llm.as_ref().and(llm_invoke),
    )
    .await
    {
        Ok(r) => r,
        Err(CustomToolRunError(reason)) => {
            let whisper = is_private.unwrap_or(default_whisper);
            return (
                report_failure(db, ctx, &tool_name, reason, whisper).await,
                None,
            );
        }
    };

    // Effects apply BEFORE the Pascal post: if the post then fails, the effects
    // stand — they happened, and the existing "outcome could not be posted"
    // failure path still tells the model. (An error out of `execute_custom_tool`
    // means no effects were ever applied.) The applier never fails the run.
    let applied_effects = apply_effects_for_run(
        db,
        &ctx.chat_id,
        &result,
        cascade,
        ctx.character_id.clone(),
        &metadata,
    )
    .await;

    let whispered = result.visibility == Visibility::Whisper;

    // A private roll is whispered to the rolling character alone. The human
    // operator sees it regardless — Pascal is one of the operator-facing senders
    // the Salon's whisper filter exempts (see app/salon/[id]/whisper-visibility.ts).
    let target_participant_ids = if whispered {
        ctx.caller_participant_id.clone().map(|c| vec![c])
    } else {
        None
    };

    let tool_title = display_title(&entry.definition.name, entry.definition.title.as_deref());

    let body =
        build_pascal_result_content(&tool_title, result.chip_label.as_deref(), &result.message);

    // Assemble `pascalMeta` field-for-field with v4's handler (:208-224).
    let mut pascal_meta = Map::new();
    pascal_meta.insert("tool".into(), json!(result.tool));
    pascal_meta.insert("toolTitle".into(), json!(tool_title));
    // `...(result.chipLabel ? { chipLabel } : {})` — v4 `c4d4b0de`, right after
    // `toolTitle`. `preserve_order` makes the placement byte-sensitive.
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
        Value::Object(params_to_object(&result.params)),
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
        let obj: Map<String, Value> = metadata_tested
            .iter()
            .map(|(k, v)| (k.clone(), v.to_value()))
            .collect();
        pascal_meta.insert("metadataTested".into(), Value::Object(obj));
    }
    if let Some(llm) = &result.llm {
        pascal_meta.insert("llm".into(), Value::Object(llm.to_wire()));
    }
    // `...(appliedEffects.length ? { effects: appliedEffects } : {})` — v4
    // `c4d4b0de`, after `llm` and before `invokedBy`. Only the writes that
    // LANDED are recorded; a skipped or dropped effect leaves no trace here.
    if !applied_effects.is_empty() {
        pascal_meta.insert(
            "effects".into(),
            serde_json::to_value(&applied_effects).unwrap_or(Value::Null),
        );
    }
    pascal_meta.insert("invokedBy".into(), json!("llm"));
    if let Some(caller) = &ctx.caller_participant_id {
        pascal_meta.insert("callerParticipantId".into(), json!(caller));
    }

    let posted = post_pascal_result(
        db,
        PostPascalResultParams {
            chat_id: ctx.chat_id.clone(),
            content: body.content,
            opaque_content: body.opaque_content,
            pascal_meta: Value::Object(pascal_meta),
            target_participant_ids,
        },
    )
    .await;

    let Some(posted) = posted else {
        // The roll was real but the announcement did not land. Say so rather than
        // letting the model narrate an outcome the transcript has no record of.
        return (
            report_failure(
                db,
                ctx,
                &tool_name,
                "the outcome could not be posted to the chat".to_string(),
                whispered,
            )
            .await,
            None,
        );
    };

    (
        RunCustomToolOutput {
            success: true,
            tool: Some(result.tool),
            value: Some(result.value),
            state: Some(result.state.as_str().to_string()),
            message: Some(result.message),
            whispered: Some(whispered),
            error: None,
        },
        Some(posted),
    )
}

/// Format a run_custom result for the model's context (v4
/// `formatRunCustomResults`). Deliberately terse: the model already has the
/// outcome message, and Pascal's bubble is what the scene reads.
pub fn format_run_custom_results(output: &RunCustomToolOutput) -> String {
    if !output.success {
        return format!(
            "Custom tool error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let whisper = if output.whispered == Some(true) {
        " (whispered — only you saw this)"
    } else {
        ""
    };
    format!(
        "{}: {} [{}]{whisper}",
        output.tool.as_deref().unwrap_or_default(),
        output.message.as_deref().unwrap_or_default(),
        output.state.as_deref().unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::definitions::definition_json_by_name;

    /// §2 byte-identity obligation: `build_run_custom_description(&[])` MUST
    /// equal the empty-roster description pinned in `tools/definitions/data.rs`
    /// (v4 `runCustomToolDefinition.description` = `buildRunCustomDescription([])`).
    #[test]
    fn empty_roster_description_matches_data_rs() {
        let json = definition_json_by_name("run_custom").expect("run_custom in catalog");
        let def: Value = serde_json::from_str(json).unwrap();
        let pinned = def["description"].as_str().unwrap();
        assert_eq!(build_run_custom_description(&[]), pinned);
    }

    #[test]
    fn validate_input_shapes() {
        // Minimal valid input.
        let v = validate_run_custom_input(&serde_json::json!({ "tool": "unlock" })).unwrap();
        assert_eq!(v.tool, "unlock");
        assert!(v.parameters.is_none());
        assert!(v.private.is_none());

        // null parameters and explicit private.
        let v = validate_run_custom_input(
            &serde_json::json!({ "tool": "x", "parameters": null, "private": true }),
        )
        .unwrap();
        assert!(v.parameters.is_none());
        assert_eq!(v.private, Some(true));

        // object parameters + unknown key stripped.
        let v = validate_run_custom_input(
            &serde_json::json!({ "tool": "x", "parameters": { "n": 3 }, "junk": 1 }),
        )
        .unwrap();
        assert_eq!(v.parameters.unwrap()["n"], serde_json::json!(3));

        // Rejections: missing tool, non-string tool, non-object/non-null params,
        // non-bool private, non-object input.
        assert!(validate_run_custom_input(&serde_json::json!({})).is_none());
        assert!(validate_run_custom_input(&serde_json::json!({ "tool": 5 })).is_none());
        assert!(
            validate_run_custom_input(&serde_json::json!({ "tool": "x", "parameters": 5 }))
                .is_none()
        );
        assert!(
            validate_run_custom_input(&serde_json::json!({ "tool": "x", "private": "yes" }))
                .is_none()
        );
        assert!(validate_run_custom_input(&serde_json::json!("nope")).is_none());
    }
}
