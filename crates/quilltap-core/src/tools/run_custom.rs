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

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use crate::db::characters_read;
use crate::db::runtime::Db;
use crate::pascal::custom_tool_types::{
    AnyOperand, CustomToolEffect, CustomToolEffectBody, LlmComparator, NumberOrParamRef,
    NumericComparator, ParamComparator, ParamRef, Roll, RollRange, StringOperand, Visibility,
    WardrobeEffectAction, When, WhenObject,
};
use crate::pascal::custom_tools::LlmInvoker;
// === P4.D329: the inventory describers ===
use crate::pascal::custom_tool_types::{
    InventoryEffectAction, InventoryStateOperand, StateRefFallback,
};
use crate::pascal::inventory_aggregate::{
    is_list_aggregate, parse_aggregate_key, AggregateField, AggregateScope,
};
use crate::pascal::js_value::{json_stringify, number_to_string};
use crate::pascal::roster::{resolve_custom_tool_roster, DiscoveredCustomTool, RosterContext};
use crate::pascal::run_service::{
    run_custom_tool, InvokedBy, RunCustomToolOutcome, RunCustomToolParams,
};
use crate::pascal::tool_vocabulary::collect_tool_vocabulary;
use crate::services::pascal_writer::PostedPascalMessage;
use crate::services::prospero_notifications::{
    post_prospero_custom_tool_error, ProsperoCustomToolErrorArgs,
};
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
    "Some tools record side effects when they run — adjusting the scene's persistent state, the rolling character's own records, what they are wearing, or what they carry, server-side, as part of the roll.\n",
    "Some tools consult, or adjust, the rolling character's timed progressions — recharges, gestations, countdowns — server-side; you cannot set one yourself.\n",
    "\n",
    "Available tools:"
);

/// The `Side effects:` line's clauses (v4 `describeEffects`, `f0db8cbb8`):
/// `writes a, b` for the value writes (targets deduplicated in declaration
/// order, v4's `[...new Set(...)]`), then one clause per wardrobe gesture in
/// file order — `wears Cloak of Night, Boots`, `takes off {{params.garment}}`
/// (+ ` (<slot> only)` when one is named), `clears hair` — each pushed only if
/// that exact string is not already there; `"; "`-joined. Items render
/// verbatim and joined blindly (`mode` never appears; an `add_to_slot` wear
/// takes no slot suffix).
pub fn describe_effects(effects: &[CustomToolEffect]) -> String {
    let mut targets: Vec<&str> = Vec::new();
    for effect in effects {
        if let CustomToolEffectBody::Write { target, .. } = &effect.body {
            if !targets.contains(&target.as_str()) {
                targets.push(target);
            }
        }
    }
    let mut clauses: Vec<String> = Vec::new();
    if !targets.is_empty() {
        clauses.push(format!("writes {}", targets.join(", ")));
    }
    for effect in effects {
        let CustomToolEffectBody::Wardrobe(gesture) = &effect.body else {
            continue;
        };
        let items = gesture.items.as_deref().unwrap_or_default().join(", ");
        let clause = match gesture.action {
            WardrobeEffectAction::Wear => format!("wears {items}"),
            WardrobeEffectAction::TakeOff => match gesture.slot.as_deref() {
                Some(slot) => format!("takes off {items} ({slot} only)"),
                None => format!("takes off {items}"),
            },
            // v4 `clears ${slot}` — load validation guarantees the slot.
            WardrobeEffectAction::ClearSlot => {
                format!("clears {}", gesture.slot.as_deref().unwrap_or("undefined"))
            }
        };
        if !clauses.contains(&clause) {
            clauses.push(clause);
        }
    }
    // === P4.D329: one clause per inventory gesture — `equips Excalibur`,
    // `unequips lantern`, `picks up magic bow` — AFTER the wardrobe clauses and
    // BEFORE the Log's `writes to the log` (P4.D331's, LAST). ===
    for effect in effects {
        let CustomToolEffectBody::Inventory(gesture) = &effect.body else {
            continue;
        };
        let verb = match gesture.action {
            InventoryEffectAction::Equip => "equips",
            InventoryEffectAction::Unequip => "unequips",
            InventoryEffectAction::PickUp => "picks up",
        };
        let clause = format!("{verb} {}", gesture.items.join(", "));
        if !clauses.contains(&clause) {
            clauses.push(clause);
        }
    }
    // === P4.D331: `writes to the log` — LAST, once (v4 `run-custom-tool.ts:356`).
    if effects.iter().any(CustomToolEffect::is_log_effect) {
        clauses.push("writes to the log".to_string());
    }
    clauses.join("; ")
}

/// Render a `NumberOrParamRef` operand bare (v4 template coercion `${v}`):
/// a `$param` reference prints its target's name, a literal its JS `String(n)`.
fn render_nopr(v: &NumberOrParamRef) -> String {
    match v {
        NumberOrParamRef::ParamRef(ParamRef { param }) => param.clone(),
        NumberOrParamRef::Number(n) => number_to_string(*n),
        // v4's `${v}` template coercion on a $state object → "[object Object]".
        NumberOrParamRef::StateRef(_) => "[object Object]".to_string(),
        // P4.D329: likewise a $inventory object.
        NumberOrParamRef::InventoryRef(_) => "[object Object]".to_string(),
    }
}

/// Render a roll spec compactly for the roster listing (v4 `describeRoll`).
fn describe_roll(roll: &Option<Roll>) -> String {
    match roll {
        None => "random value from 0 to 1".to_string(),
        Some(Roll::Dice(notation)) => format!("dice {notation}"),
        // === P4.D329: `dice from ${roll.$inventory} (else ${roll.fallback})` ===
        // JS template coercion of the fallback: only a STRING fallback loads
        // for a whole roll, but the arm stays total.
        Some(Roll::Inventory(r)) => format!(
            "dice from {} (else {})",
            r.inventory,
            fallback_template_text(&r.fallback)
        ),
        Some(Roll::Range(range)) => describe_roll_range(range),
    }
}

/// JS `${fallback}` over the three fallback types (P4.D329).
fn fallback_template_text(fallback: &StateRefFallback) -> String {
    match fallback {
        StateRefFallback::Number(n) => number_to_string(*n),
        StateRefFallback::String(s) => s.clone(),
        StateRefFallback::Bool(b) => b.to_string(),
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
        // P4.D329: nor a $inventory arm.
        StringOperand::InventoryRef(_) => "[object Object]".to_string(),
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
        // P4.D329: nor a $inventory arm.
        AnyOperand::InventoryRef(_) => "[object Object]".to_string(),
    }
}

/// `describeOperand` for a numeric-only operand (bare `value`/`roll` tests).
fn describe_numeric_operand(operand: &NumberOrParamRef) -> String {
    match operand {
        NumberOrParamRef::ParamRef(ParamRef { param }) => param.clone(),
        NumberOrParamRef::Number(n) => number_to_string(*n),
        // v4 `describeOperand` has no $state arm: `String(object)` → "[object Object]".
        NumberOrParamRef::StateRef(_) => "[object Object]".to_string(),
        // P4.D329: nor a $inventory arm.
        NumberOrParamRef::InventoryRef(_) => "[object Object]".to_string(),
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
/// `roll` or a parameter test. An any-of reads as each group in parentheses
/// joined by "or" (a one-test group goes bare — v4 `a53024d3c`).
fn describe_when(when: &When) -> String {
    match when {
        When::CatchAll(_) => "otherwise".to_string(),
        When::Object(w) => describe_when_group(w).join(" and "),
        When::AnyOf(_) => when
            .when_groups()
            .iter()
            .map(|group| {
                let parts = describe_when_group(group);
                if parts.len() == 1 {
                    parts[0].clone()
                } else {
                    format!("({})", parts.join(" and "))
                }
            })
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

/// Render one AND-group's tests (v4 `describeWhenGroup`).
fn describe_when_group(when: &WhenObject) -> Vec<String> {
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
    // === P4.D329: the three inventory clauses (after `llm`) ===
    for (reference, cmp) in when.inventory.iter().flatten() {
        if let Some(eq) = &cmp.eq {
            parts.push(format!(
                "your {reference} is {}",
                describe_state_operand(eq)
            ));
        }
        if let Some(neq) = &cmp.neq {
            parts.push(format!(
                "your {reference} is not {}",
                describe_state_operand(neq)
            ));
        }
    }
    for (reference, cmp) in when.inventory_count.iter().flatten() {
        parts.extend(describe_numeric_comparator(
            cmp,
            &format!("your equipped {reference} count"),
        ));
    }
    for (key, cmp) in when.inventory_aggregate.iter().flatten() {
        parts.extend(describe_aggregate(key, cmp));
    }
    parts
}

// === P4.D329: the aggregate describers ===
/// v4 `describeOperand` over an `inventory` state operand: a literal state is
/// JSON-quoted, a `$param` names its parameter.
fn describe_state_operand(operand: &InventoryStateOperand) -> String {
    match operand {
        InventoryStateOperand::State(s) => json_stringify(&Value::String(s.as_str().to_string())),
        InventoryStateOperand::ParamRef(ParamRef { param }) => param.clone(),
    }
}

/// v4 `SCOPE_WORDS` — the possessive a scope reads as, and how its count is
/// asked.
fn scope_words(scope: AggregateScope) -> (&'static str, &'static str) {
    match scope {
        AggregateScope::Equipped => (
            "your equipped objects'",
            "how many objects you have equipped",
        ),
        AggregateScope::Owned => ("your owned objects'", "how many objects you own"),
        AggregateScope::Reachable => ("your reachable objects'", "how many objects you can reach"),
    }
}

/// v4 `LIST_WORDS` — the list comparators read as sentences about entries,
/// not as symbols.
const LIST_WORDS: [(&str, &str); 4] = [
    ("eq", "includes"),
    ("neq", "includes no"),
    ("contains", "has an entry containing"),
    ("ncontains", "has no entry containing"),
];

/// v4 `describeAggregate(key, comparator)` — render one `inventoryAggregate`
/// test.
fn describe_aggregate(key: &str, cmp: &ParamComparator) -> Vec<String> {
    let parsed = match parse_aggregate_key(key) {
        Ok(parsed) => parsed,
        Err(_) => return describe_param_comparator(cmp, &format!("your inventory's {key}")),
    };
    let (possessive, count) = scope_words(parsed.scope);
    let subject = match &parsed.field {
        AggregateField::Count => return describe_param_comparator(cmp, count),
        AggregateField::Metadata { key, fold } => match fold {
            // `possessive.replace(/'$/, '')` — the trailing apostrophe off.
            Some(crate::pascal::inventory_aggregate::AggregateFold::Count) => format!(
                "how many of {} carry a numeric metadata.{key}",
                possessive.strip_suffix('\'').unwrap_or(possessive)
            ),
            Some(fold) => format!("{possessive} metadata.{key}.{}", fold.as_str()),
            None => format!("{possessive} metadata.{key}"),
        },
        AggregateField::Names => format!("{possessive} names"),
        AggregateField::Titles => format!("{possessive} titles"),
    };
    if !is_list_aggregate(&parsed) {
        return describe_param_comparator(cmp, &subject);
    }
    LIST_WORDS
        .iter()
        .filter_map(|(k, verb)| {
            let operand = match *k {
                "eq" => cmp.eq.as_ref().map(describe_any_operand),
                "neq" => cmp.neq.as_ref().map(describe_any_operand),
                "contains" => cmp.contains.as_ref().map(describe_string_operand),
                _ => cmp.ncontains.as_ref().map(describe_string_operand),
            }?;
            Some(format!("{subject} {verb} {operand}"))
        })
        .collect()
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
            // vocabulary — the file is the user's own.) Wardrobe gestures name
            // their item references verbatim, placeholders and all — what may
            // be worn, not which outcome wears it.
            lines.push(format!("    Side effects: {}", describe_effects(effects)));
        }
        // === P4.D329: which objects the tool consults — vocabulary, never the
        // odds (AFTER `Side effects:`, BEFORE the outcome rows; gone under
        // `revealOdds: false` with the rest). ===
        let vocabulary = collect_tool_vocabulary(definition);
        if !vocabulary.inventory_reads.is_empty() {
            lines.push(format!(
                "    Reads: your inventory for {}",
                vocabulary.inventory_reads.join(", ")
            ));
        }
        if !vocabulary.inventory_aggregates.is_empty() {
            lines.push(format!(
                "    Takes stock of: {}",
                vocabulary.inventory_aggregates.join(", ")
            ));
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
    pub project_id: Option<String>,
    /// The caller's participant id — the whisper target for a private roll.
    pub caller_participant_id: Option<String>,
    /// Epoch milliseconds at run start — ONE reading for the whole run (v4
    /// `run-custom-handler.ts:126`). The roster's gates, the progress sheet the
    /// tables and templates read, `{{now}}`, and the `updatedAt` an effect
    /// stamps all take THIS instant, so a tool cannot see one moment and record
    /// another. Injected rather than read here so a differential can freeze it;
    /// production passes `now_unix_ms()`.
    pub now_ms: i64,
    /// The turn's wardrobe announcement set, so a tool's `wardrobe` effect
    /// joins the turn's single Aurora announcement alongside any
    /// `wardrobe_wear` call (v4 `f0db8cbb8`). The executor hands its own
    /// context's `Arc` — the SAME set the wardrobe tools borrow.
    pub pending_wardrobe_announcements: Option<Arc<Mutex<HashSet<String>>>>,
    /// The turn's outfit-history intents, created beside the set (v4
    /// `05005c562`). The effects applier supplies the `'pascal'` provenance.
    pub pending_wardrobe_history:
        Option<crate::services::outfit_change_effects::PendingWardrobeHistory>,
    /// The turn's pending Inventory changes (v4 `run-custom-handler.ts:58`
    /// `pendingInventoryAnnouncements?`, `e1872c94f`) — the SAME map the
    /// inventory tools get, so a tool's `inventory` gesture and the model's own
    /// `inventory_*` calls read as one Pascal line. The LAST field (R-G).
    pub pending_inventory_announcements:
        Option<crate::services::inventory_change_effects::PendingInventoryAnnouncements>,
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

/// v4 `run-custom-handler.ts`'s `CONTEXT` — the `context` field of the
/// handler's own lines (the run service's carry `pascal.run-service`).
const HANDLER_CONTEXT: &str = "run-custom-handler";

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

/// v4's `logger.warn('run_custom validation failed', { context, userId,
/// input })` (`run-custom-handler.ts:104-106`): `input` is whatever the model
/// sent — a string as itself, anything else on the file layer's `…Json` name
/// (§R.5), so a number stays a number in the envelope rather than becoming
/// its Display text (the §3 review of the `322a5488e` follow-ups
/// unification).
fn warn_validation_failed(user_id: &str, input: &Value) {
    match input {
        Value::String(s) => tracing::warn!(
            target: "quilltap::pascal",
            context = HANDLER_CONTEXT,
            userId = user_id,
            input = s.as_str(),
            "run_custom validation failed",
        ),
        _ => tracing::warn!(
            target: "quilltap::pascal",
            context = HANDLER_CONTEXT,
            userId = user_id,
            inputJson = %serde_json::to_string(input).unwrap_or_default(),
            "run_custom validation failed",
        ),
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
    tracing::warn!(
        target: "quilltap::pascal",
        context = HANDLER_CONTEXT,
        userId = ctx.user_id.as_str(),
        chatId = ctx.chat_id.as_str(),
        tool = tool_name,
        reason = reason.as_str(),
        "Custom tool run failed",
    );
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

/// v4 `repos.characters.findById(id)` as the custom-tool entrances see it:
/// the row hydrated through the vault overlay, `Ok(None)` when no such
/// character, and `Err(message)` — v4's `getErrorMessage(error)` — when the
/// read fails or the vault is broken (v4's `CharacterVaultUnavailableError`).
///
/// The read goes through `find_by_id_raw` + the overlay rather than
/// `characters_read::find_by_id` so the message is v4's own
/// `CharacterVaultUnavailableError` sentence (`find_by_id` erases it into a
/// generic `DbError::Internal`); `api::custom_tools::resolve_metadata` reaches
/// for the same pair, for the same reason. Shared by the LLM entrance and the
/// trigger runner (`pascal::trigger_runner`).
pub(crate) fn read_character_hydrated(
    db: &Db,
    character_id: &str,
) -> Result<Option<Value>, String> {
    let read = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let raw = characters_read::find_by_id_raw(main, character_id)?;
            let repo = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
            Ok(crate::db::vault_read_overlay::apply_document_store_overlay_one(&repo, raw))
        })
    });
    let read = match read {
        Ok(inner) => inner,
        Err(e) => Err(crate::db::vault_read_overlay::OverlayOneError::Db(e)),
    };
    read.map_err(|error| match error {
        crate::db::vault_read_overlay::OverlayOneError::Db(e) => e.to_string(),
        crate::db::vault_read_overlay::OverlayOneError::Unavailable(u) => format!(
            "Character {} has no usable vault (characterDocumentMountPointId={}): \
             properties.json missing",
            u.character_id, u.mount_id
        ),
    })
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
        warn_validation_failed(&ctx.user_id, input);
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
    // generic `DbError::Internal`); `api::custom_tools::resolve_metadata` reaches for
    // the same pair, for the same reason.
    let mut metadata: Map<String, Value> = Map::new();
    if let Some(character_id) = &ctx.character_id {
        match read_character_hydrated(db, character_id) {
            Ok(Some(character)) => {
                if let Some(map) = character.get("metadata").and_then(Value::as_object) {
                    metadata = map.clone();
                }
            }
            Ok(None) => {}
            Err(reason) => {
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
        project_id: ctx.project_id.clone(),
        // The sheet the gates are answered against, already in hand.
        metadata: Some(metadata.clone()),
        // P4.D329: the bag loads lazily, only when a gate names it.
        inventory: None,
    };
    let roster = match db.read_main(|main| {
        db.read_mount_index(|mount| {
            Ok(resolve_custom_tool_roster(
                &roster_ctx,
                main,
                mount,
                ctx.now_ms,
            ))
        })
    }) {
        Ok(r) => r,
        Err(e) => return (failure(e.to_string(), Some(&tool_name)), None),
    };

    let Some(entry) = roster.get(&tool_name).cloned() else {
        // A name the model invented, a file the user just deleted, or a
        // definition this character's fact sheet is not offered. Either way this
        // is not worth an error bubble in the transcript — tell the model.
        let available: Vec<&str> = roster.tools.iter().map(|(k, _)| k.as_str()).collect();
        tracing::warn!(
            target: "quilltap::pascal",
            context = HANDLER_CONTEXT,
            chatId = ctx.chat_id.as_str(),
            tool = tool_name.as_str(),
            // v4 logs the roster's key list as an array (the file layer's
            // `…Json` rule, §R.5).
            availableJson = %serde_json::to_string(&available).unwrap_or_default(),
            "run_custom named an unknown tool",
        );
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

    // The deal itself — cascade, roll, effects, Pascal's post — is the shared
    // run service's (`pascal::run_service`, v4 `lib/pascal/run-service.ts`);
    // this entrance only resolves the roster from the caller's perspective and
    // reports back to the model.
    let outcome = run_custom_tool(
        db,
        RunCustomToolParams {
            user_id: &ctx.user_id,
            chat_id: &ctx.chat_id,
            chat: None,
            entry: &entry,
            parameters: parameters.as_ref(),
            private: is_private,
            character_id: ctx.character_id.as_deref(),
            metadata: &metadata,
            // v4's handler passes no name → the service reads the RAW row.
            character_name: None,
            // A private roll is whispered to the rolling character alone: every
            // other participant's context excludes it. The human operator sees
            // it regardless — Pascal is one of the operator-facing senders the
            // Salon's whisper filter exempts.
            whisper_target_ids: ctx.caller_participant_id.clone().map(|c| vec![c]),
            invoked_by: InvokedBy::Llm,
            caller_participant_id: ctx.caller_participant_id.as_deref(),
            trigger_phrase: None,
            now_ms: ctx.now_ms,
            pending_wardrobe_announcements: ctx.pending_wardrobe_announcements.as_ref(),
            pending_wardrobe_history: ctx.pending_wardrobe_history.as_ref(),
            pending_inventory_announcements: ctx.pending_inventory_announcements.as_ref(),
        },
        rng,
        llm_invoke,
    )
    .await;

    let (result, posted) = match outcome {
        Ok(RunCustomToolOutcome::Ok { result, posted, .. }) => (result, posted),
        Ok(RunCustomToolOutcome::Refused { reason, whisper }) => {
            return (
                report_failure(db, ctx, &tool_name, reason, whisper).await,
                None,
            );
        }
        // v4's `catch` — the service rethrows only what is not a
        // `CustomToolRunError`; the model's result is the same failure shape.
        Err(error) => {
            let whisper = is_private
                .unwrap_or(entry.definition.default_visibility == Some(Visibility::Whisper));
            return (
                report_failure(db, ctx, &tool_name, error.to_string(), whisper).await,
                None,
            );
        }
    };
    let whispered = result.visibility == Visibility::Whisper;

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

    let result = *result;
    tracing::info!(
        target: "quilltap::pascal",
        context = HANDLER_CONTEXT,
        userId = ctx.user_id.as_str(),
        chatId = ctx.chat_id.as_str(),
        tool = result.tool.as_str(),
        tier = serde_json::to_value(entry.tier)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
        rollForm = match result.roll_form {
            crate::pascal::custom_tools::RollForm::Range => "range",
            crate::pascal::custom_tools::RollForm::Dice => "dice",
        },
        value = %crate::pascal::js_value::number_to_string(result.value),
        state = result.state.as_str(),
        outcomeIndex = result.outcome_index as u64,
        whispered,
        "Custom tool run completed",
    );
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

    /// v4 `describeEffects` edge pins beyond the corpus (P4.D306 Tier 2 item
    /// 16): items are joined blindly (a comma inside a title reads as two),
    /// and ` (<slot> only)` renders on any take-off naming a slot, composite
    /// or not — the description never consults a wardrobe.
    #[test]
    fn describe_effects_joins_blindly_and_suffixes_any_named_take_off_slot() {
        let tool = crate::pascal::custom_tool_types::safe_parse(&serde_json::json!({
            "name": "edges", "description": "d",
            "effects": [
                { "wardrobe": { "action": "wear", "items": ["Hat, Feathered"] } },
                { "wardrobe": { "action": "take_off", "items": ["Regalia"], "slot": "top" } },
                { "target": "state.x", "value": 1 },
                { "target": "state.x", "value": 2 },
            ],
            "outcomes": [{ "when": true, "message": "x", "state": "info" }],
        }))
        .unwrap();
        assert_eq!(
            describe_effects(tool.effects.as_deref().unwrap()),
            "writes state.x; wears Hat, Feathered; takes off Regalia (top only)"
        );
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
