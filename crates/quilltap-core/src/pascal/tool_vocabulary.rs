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

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use super::custom_tool_types::{is_state_ref_value, QtapCustomTool, When};
use crate::collation::locale_compare;

/// The placeholder families `render_template` understands.
///
/// v4 declares this `g`-flagged and resets `lastIndex` per call; a Rust
/// `find_iter` is stateless, so only the MATCHING semantics carry over —
/// `[^}]+` (so `{{}}` never matches and a `}` cannot appear inside a
/// placeholder), then `.trim()` on the capture.
static PLACEHOLDER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("placeholder pattern compiles"));

const PARAMS_PREFIX: &str = "params.";
const METADATA_PREFIX: &str = "metadata.";
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
    }
}

/// Harvest every placeholder family out of one rendered string.
fn collect_placeholders(text: &str, declared: &[&str], found: &mut Found) {
    for caps in PLACEHOLDER_PATTERN.captures_iter(text) {
        let key = caps[1].trim();

        match key {
            "value" => {
                found.value = true;
                continue;
            }
            "roll" => {
                found.roll = true;
                continue;
            }
            "dice" => {
                found.dice = true;
                continue;
            }
            "llm" => {
                found.llm = true;
                continue;
            }
            _ => {}
        }

        if let Some(name) = key.strip_prefix(PARAMS_PREFIX) {
            if declared.contains(&name) {
                add(&mut found.params, name);
            }
            continue;
        }
        if let Some(name) = key.strip_prefix(METADATA_PREFIX) {
            if !name.is_empty() {
                add(&mut found.metadata, name);
            }
            continue;
        }
        if let Some(path) = key.strip_prefix(STATE_PREFIX) {
            if !path.is_empty() {
                add(&mut found.state, path);
            }
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
