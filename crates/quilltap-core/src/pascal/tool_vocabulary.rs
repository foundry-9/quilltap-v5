//! Tool vocabulary — what a definition actually quotes (v4
//! `lib/pascal/tool-vocabulary.ts`, `faab6881` + `6864bf0e`).
//!
//! A custom tool's parameters are declared, so a form can be generated from
//! them. What the tool does with them is not: whether `visible_request` ever
//! reaches an outcome message, whether the table reads
//! `{{metadata.hasAnsibleAccess}}`, whether the roll's own value is even
//! mentioned. To the operator filling in the run dialog, all of that is
//! invisible — which is the confusion this module exists to remove.
//!
//! # Quoted, not merely available
//!
//! Every field below reports an **occurrence**: a placeholder that appears in
//! some string Pascal renders (an outcome message, or the oracle prompt), or a
//! metadata key some `when` clause tests, or a `$state` reference the definition
//! carries. A tool that rolls dice but never writes `{{dice}}` does not quote it,
//! and the dialog does not offer it. The list is what this tool says, not what
//! the format permits.
//!
//! # What this deliberately is, and is not
//!
//! It is a **vocabulary**: the names a tool quotes. It is emphatically **not the
//! odds** — no roll spec, no thresholds, no outcome table, nothing about which
//! value wins which row. The custom-tools roster endpoint withholds all of that
//! on purpose, and this summary keeps that line: it says "this tool reads
//! `hasAnsibleAccess`", never "it succeeds when `hasAnsibleAccess` is true".

use serde::Serialize;
use serde_json::Value;

use super::custom_tool_types::{
    is_state_ref_value, parse_effect_target, EffectTarget, EffectValue, QtapCustomTool, When,
};
use super::placeholders::{scan_placeholders, PlaceholderRef};
use crate::collation::locale_compare;

/// v4 `0506517d3` moved the pattern and the three family prefixes into
/// [`super::placeholders`]; only `state.` is still spelled here, and only
/// because the effect-TARGET syntax (a different grammar, which the collapse
/// deliberately left alone) needs its length to re-slice a parsed raw target.
const STATE_PREFIX: &str = "state.";

/// What a definition quotes. Every field means "this tool actually says so".
///
/// All seven keys are ALWAYS present — `false`s and empty arrays, never absent —
/// so a caller never has to distinguish "none" from "not computed". The key
/// order is v4's interface declaration order, which is the wire contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ToolVocabulary {
    /// True when some rendered string quotes `{{value}}`.
    pub value: bool,
    /// True when some rendered string quotes `{{roll}}`.
    pub roll: bool,
    /// True when some rendered string quotes `{{dice}}`.
    pub dice: bool,
    /// True when some rendered string quotes `{{llm}}`.
    pub llm: bool,
    /// Declared parameters quoted as `{{params.name}}`. Restricted to declared
    /// names: a placeholder naming a parameter that does not exist renders as
    /// written, and offering it here would advertise a hole as a feature.
    pub params: Vec<String>,
    /// Keys of the invoking character's `metadata.json` this tool reads — from
    /// `when.metadata` tests, from its availability gate, and from
    /// `{{metadata.key}}` placeholders. Sorted.
    pub metadata: Vec<String>,
    /// Paths into the merged persistent state this tool reads — from `$state`
    /// references and from `{{state.path}}` placeholders. Sorted.
    pub state: Vec<String>,
    /// State paths this tool's effects may WRITE. A write is a different claim
    /// than a read — "this tool consults the encounter count" and "this tool
    /// changes it" deserve different sentences — so writes get their own list
    /// rather than folding into `state`. Sorted.
    #[serde(rename = "stateWrites")]
    pub state_writes: Vec<String>,
    /// Metadata keys this tool's effects may WRITE on the rolling character. Sorted.
    #[serde(rename = "metadataWrites")]
    pub metadata_writes: Vec<String>,
}

/// True when a tool quotes nothing at all, and so has no vocabulary to show.
pub fn is_empty_vocabulary(vocabulary: &ToolVocabulary) -> bool {
    !vocabulary.value
        && !vocabulary.roll
        && !vocabulary.dice
        && !vocabulary.llm
        && vocabulary.params.is_empty()
        && vocabulary.metadata.is_empty()
        && vocabulary.state.is_empty()
        && vocabulary.state_writes.is_empty()
        && vocabulary.metadata_writes.is_empty()
}

/// The mutable accumulator v4 calls `found` — insertion-ordered lists standing
/// in for its `Set`s, since the result is sorted anyway and exact-string dedup
/// is all a `Set` contributes here.
#[derive(Default)]
struct Found {
    value: bool,
    roll: bool,
    dice: bool,
    llm: bool,
    params: Vec<String>,
    metadata: Vec<String>,
    state: Vec<String>,
    state_writes: Vec<String>,
    metadata_writes: Vec<String>,
}

fn add(into: &mut Vec<String>, name: &str) {
    if !into.iter().any(|n| n == name) {
        into.push(name.to_string());
    }
}

/// Collect everything a definition quotes (v4 `collectToolVocabulary`). Pure and
/// total: a definition that quotes nothing yields `false`s and empty lists
/// rather than absent keys.
pub fn collect_tool_vocabulary(definition: &QtapCustomTool) -> ToolVocabulary {
    let declared: Vec<&str> = definition
        .parameters
        .as_ref()
        .map(|ps| ps.iter().map(|(k, _)| k.as_str()).collect())
        .unwrap_or_default();
    let mut found = Found::default();

    for outcome in &definition.outcomes {
        // A catch-all tests nothing, so it names nothing — but it still carries
        // a message, and that message may well quote a fact sheet.
        if let When::Object(when) = &outcome.when {
            for (key, _) in when.metadata.iter().flatten() {
                add(&mut found.metadata, key);
            }
        }
        collect_placeholders(&outcome.message, &declared, &mut found);
    }

    // The availability gate reads the same fact sheet, and naming its keys stays
    // on the right side of the odds line: that a tool consults `toolAbilities` is
    // vocabulary; that it is withheld unless `toolAbilities` contains
    // "programmable" is not, and is not said here. A gated-out tool never reaches
    // a roster listing at all, so this only ever describes one the reader has.
    for gate in [&definition.available_when, &definition.withheld_when] {
        for (key, _) in gate.iter().flat_map(|g| g.metadata.iter()) {
            add(&mut found.metadata, key);
        }
    }

    if let Some(llm) = definition.llm.as_ref() {
        collect_placeholders(&llm.prompt, &declared, &mut found);
    }

    // The chip label is a rendered string like any outcome message.
    if let Some(chip_label) = definition.chip_label.as_ref() {
        collect_placeholders(chip_label, &declared, &mut found);
    }

    // Effects: an expression's `{{ref}}`s are the outcome-message vocabulary
    // verbatim, so the one placeholder scanner reads them too; a condition's
    // metadata keys are reads like an outcome row's; and each target is a WRITE,
    // reported on its own lists because it is a different claim than a read.
    for effect in definition.effects.iter().flatten() {
        if let EffectValue::Str(source) = &effect.value {
            collect_placeholders(source, &declared, &mut found);
        }
        for when in effect.when.iter() {
            for (key, _) in when.base.metadata.iter().flatten() {
                add(&mut found.metadata, key);
            }
        }

        let Ok(target) = parse_effect_target(&effect.target) else {
            continue; // load-rejected; nothing honest to report
        };
        match &target {
            EffectTarget::State { raw, .. } => {
                add(&mut found.state_writes, &raw[STATE_PREFIX.len()..]);
            }
            EffectTarget::Metadata { key, .. } => add(&mut found.metadata_writes, key),
        }
    }

    // Every `$state` reference, wherever it sits — a parameter default, a roll
    // field, a comparator operand. Walked rather than enumerated: the schema is
    // free to grow new sites, and a list of them here would silently fall behind.
    //
    // v4 walks the parsed definition object; v5 walks its serialization, which is
    // the same object — the definition differential pins that byte-for-byte.
    let as_json = serde_json::to_value(definition).unwrap_or(Value::Null);
    collect_state_refs(&as_json, &mut found.state);

    let sorted = |mut values: Vec<String>| -> Vec<String> {
        values.sort_by(|a, b| locale_compare(a, b));
        values
    };

    ToolVocabulary {
        value: found.value,
        roll: found.roll,
        dice: found.dice,
        llm: found.llm,
        params: sorted(found.params),
        metadata: sorted(found.metadata),
        state: sorted(found.state),
        state_writes: sorted(found.state_writes),
        metadata_writes: sorted(found.metadata_writes),
    }
}

/// Harvest every placeholder family out of one rendered string.
///
/// v4 `0506517d3` routed this through [`scan_placeholders`]; so does v5. The
/// three bare-prefix keys were dropped by this scanner before the collapse too
/// — `declared.contains("")` is false, and the metadata/state arms guarded the
/// empty remainder by hand — so the counts do not move. What DOES move is the
/// trim: the hand-rolled loop used Rust's `.trim()`, and JS trims U+FEFF where
/// Rust does not, so `{{\u{FEFF}params.x}}` used to classify as unknown here
/// and be dropped. `scan_placeholders` trims the way v4 does.
fn collect_placeholders(text: &str, declared: &[&str], found: &mut Found) {
    for scanned in scan_placeholders(text) {
        match scanned.place_ref {
            PlaceholderRef::Value => found.value = true,
            PlaceholderRef::Roll => found.roll = true,
            PlaceholderRef::Dice => found.dice = true,
            PlaceholderRef::Llm => found.llm = true,
            PlaceholderRef::Params { ref name } => {
                if declared.contains(&name.as_str()) {
                    add(&mut found.params, name);
                }
            }
            PlaceholderRef::Metadata { ref key } => add(&mut found.metadata, key),
            PlaceholderRef::State { ref path } => add(&mut found.state, path),
            PlaceholderRef::Unknown { .. } => {}
        }
    }
}

/// Depth-first walk for `{ "$state": "..." }` objects anywhere in the tree.
fn collect_state_refs(node: &Value, into: &mut Vec<String>) {
    match node {
        Value::Array(items) => {
            for item in items {
                collect_state_refs(item, into);
            }
        }
        Value::Object(_) => {
            if is_state_ref_value(node) {
                if let Some(path) = node.get("$state").and_then(Value::as_str) {
                    add(into, path);
                }
                return;
            }
            for value in node.as_object().expect("checked").values() {
                collect_state_refs(value, into);
            }
        }
        _ => {}
    }
}
