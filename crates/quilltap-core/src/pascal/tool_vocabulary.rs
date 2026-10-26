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
    is_state_ref_value, parse_effect_target, CustomToolEffectBody, EffectTarget, EffectValue,
    EffectWhen, ParamComparator, QtapCustomTool, ToolGateGroup, WardrobeEffectAction, WhenObject,
};
// === P4.D329: the inventory vocabulary ===
use super::custom_tool_types::{is_inventory_ref_value, InventoryEffectAction};
use super::inventory_subjects::parse_inventory_operand;
use super::placeholders::{scan_placeholders, PlaceholderRef};
use crate::collation::locale_compare;

/// v4 `0506517d3` moved the pattern and the three family prefixes into
/// [`super::placeholders`]; only `state.` is still spelled here, and only
/// because the effect-TARGET syntax (a different grammar, which the collapse
/// deliberately left alone) needs its length to re-slice a parsed raw target.
const STATE_PREFIX: &str = "state.";

/// What a definition quotes. Every field means "this tool actually says so".
///
/// EVERY key is ALWAYS present — `false`s and empty arrays, never absent — so a
/// caller never has to distinguish "none" from "not computed". The key order is
/// v4's interface declaration order, which is the wire contract.
///
/// (The count is deliberately not written down. It said "seven" while the
/// struct carried nine, and P4.D169 took it to twelve; a number here is a fact
/// that goes stale silently, and the field list below is the only one that
/// cannot.)
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
    /// Progression IDS this tool READS — from `when.progress` tests, from its
    /// availability gate, and from `{{progress.<id>.<field>}}` placeholders.
    /// Ids, not `<id>.<field>` keys: "this tool consults your cannon" is the
    /// honest sentence for a run dialog, where "it consults cannon.percent"
    /// edges toward the odds the roster deliberately withholds. Sorted.
    pub progress: Vec<String>,
    /// Progression ids this tool's effects may WRITE. A write is a different
    /// claim than a read — "it consults the cannon" and "it re-arms the cannon"
    /// deserve different sentences — so writes get their own list. Sorted.
    #[serde(rename = "progressWrites")]
    pub progress_writes: Vec<String>,
    /// True when some rendered string quotes `{{now}}`.
    pub now: bool,
    /// True when some rendered string quotes `{{character}}` (v4 `a05c6f755`,
    /// between `now` and `wears` in the interface AND the returned object —
    /// wire order).
    pub character: bool,
    /// Item references this tool's wardrobe effects may PUT ON the rolling
    /// character, verbatim (a `{{params.garment}}` stays as written — it is
    /// vocabulary, not a resolved garment). Sorted. (v4 `f0db8cbb8`, appended
    /// after `now` in the interface AND the returned object — wire order.)
    pub wears: Vec<String>,
    /// Item references this tool's wardrobe effects may TAKE OFF, verbatim. Sorted.
    #[serde(rename = "takesOff")]
    pub takes_off: Vec<String>,
    /// Slots this tool's wardrobe effects may CLEAR. Sorted.
    #[serde(rename = "clearsSlots")]
    pub clears_slots: Vec<String>,
    // === P4.D329: the five Inventory keys, in the pin's wire order ===
    /// Inventory object references this tool READS — tested by an `inventory`
    /// / `inventoryCount` subject (outcomes, effect conditions, the gate),
    /// quoted by `{{inventory.<ref>.*}}`, or drawn from by a `$inventory`
    /// operand. Verbatim, sorted.
    #[serde(rename = "inventoryReads")]
    pub inventory_reads: Vec<String>,
    /// `inventoryAggregate` keys this tool tests (`equipped.names`,
    /// `owned.metadata.toolVersion.max`) — outcomes, effect conditions, the
    /// gate. A metadata key read through an aggregate is the OBJECTS'
    /// vocabulary, not the character's, so it is not added to `metadata`.
    /// Sorted.
    #[serde(rename = "inventoryAggregates")]
    pub inventory_aggregates: Vec<String>,
    /// References this tool's inventory effects may EQUIP, verbatim. Sorted.
    pub equips: Vec<String>,
    /// References this tool's inventory effects may UNEQUIP, verbatim. Sorted.
    pub unequips: Vec<String>,
    /// Archetype references this tool's inventory effects may PICK UP,
    /// verbatim. Sorted.
    #[serde(rename = "picksUp")]
    pub picks_up: Vec<String>,
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
        && vocabulary.progress.is_empty()
        && vocabulary.progress_writes.is_empty()
        && !vocabulary.now
        && !vocabulary.character
        && vocabulary.wears.is_empty()
        && vocabulary.takes_off.is_empty()
        && vocabulary.clears_slots.is_empty()
        // === P4.D329 ===
        && vocabulary.inventory_reads.is_empty()
        && vocabulary.inventory_aggregates.is_empty()
        && vocabulary.equips.is_empty()
        && vocabulary.unequips.is_empty()
        && vocabulary.picks_up.is_empty()
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
    progress: Vec<String>,
    progress_writes: Vec<String>,
    now: bool,
    character: bool,
    wears: Vec<String>,
    takes_off: Vec<String>,
    clears_slots: Vec<String>,
    // === P4.D329 ===
    inventory: Vec<String>,
    inventory_aggregates: Vec<String>,
    equips: Vec<String>,
    unequips: Vec<String>,
    picks_up: Vec<String>,
}

// === P4.D329: one group's reads ===
/// One group's reads — the same for an outcome row, an effect condition and a
/// gate group (v4's `collectGroup`, `a53024d3c`), walked over every group of
/// an any-of.
trait GroupReads {
    fn metadata_tests(&self) -> Option<&Vec<(String, ParamComparator)>>;
    fn progress_tests(&self) -> Option<&Vec<(String, ParamComparator)>>;
    /// The `inventory` keys, then the `inventoryCount` keys.
    fn inventory_keys(&self) -> Vec<&String>;
    fn inventory_aggregate_keys(&self) -> Vec<&String>;
}

impl GroupReads for WhenObject {
    fn metadata_tests(&self) -> Option<&Vec<(String, ParamComparator)>> {
        self.metadata.as_ref()
    }
    fn progress_tests(&self) -> Option<&Vec<(String, ParamComparator)>> {
        self.progress.as_ref()
    }
    fn inventory_keys(&self) -> Vec<&String> {
        let states = self.inventory.iter().flatten().map(|(k, _)| k);
        let counts = self.inventory_count.iter().flatten().map(|(k, _)| k);
        states.chain(counts).collect()
    }
    fn inventory_aggregate_keys(&self) -> Vec<&String> {
        self.inventory_aggregate
            .iter()
            .flatten()
            .map(|(k, _)| k)
            .collect()
    }
}

impl GroupReads for EffectWhen {
    fn metadata_tests(&self) -> Option<&Vec<(String, ParamComparator)>> {
        self.base.metadata_tests()
    }
    fn progress_tests(&self) -> Option<&Vec<(String, ParamComparator)>> {
        self.base.progress_tests()
    }
    fn inventory_keys(&self) -> Vec<&String> {
        self.base.inventory_keys()
    }
    fn inventory_aggregate_keys(&self) -> Vec<&String> {
        self.base.inventory_aggregate_keys()
    }
}

impl GroupReads for ToolGateGroup {
    fn metadata_tests(&self) -> Option<&Vec<(String, ParamComparator)>> {
        self.metadata.as_ref()
    }
    fn progress_tests(&self) -> Option<&Vec<(String, ParamComparator)>> {
        self.progress.as_ref()
    }
    fn inventory_keys(&self) -> Vec<&String> {
        let states = self.inventory.iter().flatten().map(|(k, _)| k);
        let counts = self.inventory_count.iter().flatten().map(|(k, _)| k);
        states.chain(counts).collect()
    }
    fn inventory_aggregate_keys(&self) -> Vec<&String> {
        self.inventory_aggregate
            .iter()
            .flatten()
            .map(|(k, _)| k)
            .collect()
    }
}

fn collect_group(group: &impl GroupReads, found: &mut Found) {
    for (key, _) in group.metadata_tests().into_iter().flatten() {
        add(&mut found.metadata, key);
    }
    for (key, _) in group.progress_tests().into_iter().flatten() {
        add(&mut found.progress, progression_id(key));
    }
    for key in group.inventory_keys() {
        add(&mut found.inventory, key);
    }
    for key in group.inventory_aggregate_keys() {
        add(&mut found.inventory_aggregates, key);
    }
}

/// The progression id out of a `"<id>.<field>"` sheet key. A key that somehow
/// carries no dot (load validation forbids it) names the whole string, which is
/// the least surprising thing to report and never throws.
fn progression_id(key: &str) -> &str {
    match key.find('.') {
        Some(dot) if dot > 0 => &key[..dot],
        _ => key,
    }
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
        for group in outcome.when.when_groups() {
            collect_group(group, &mut found);
        }
        collect_placeholders(&outcome.message, &declared, &mut found);
        // A brief is rendered like a prompt (v4 `322a5488e`).
        if let Some(brief) = &outcome.brief {
            collect_placeholders(brief, &declared, &mut found);
        }
    }

    // The availability gate reads the same fact sheet, and naming its keys stays
    // on the right side of the odds line: that a tool consults `toolAbilities` is
    // vocabulary; that it is withheld unless `toolAbilities` contains
    // "programmable" is not, and is not said here. A gated-out tool never reaches
    // a roster listing at all, so this only ever describes one the reader has.
    for gate in [&definition.available_when, &definition.withheld_when] {
        for group in gate.iter().flat_map(|g| g.gate_groups()) {
            collect_group(group, &mut found);
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
    // A wardrobe gesture's item references are templates too, scanned the same
    // way, and the gesture itself is reported on the dress/undress lists.
    for effect in definition.effects.iter().flatten() {
        if let CustomToolEffectBody::Write {
            value: EffectValue::Str(source),
            ..
        } = &effect.body
        {
            collect_placeholders(source, &declared, &mut found);
        }
        for group in effect.when.iter().flat_map(|w| w.effect_when_groups()) {
            collect_group(group, &mut found);
        }

        let target = match &effect.body {
            // === P4.D329: the inventory gesture ===
            CustomToolEffectBody::Inventory(gesture) => {
                for item in &gesture.items {
                    collect_placeholders(item, &declared, &mut found);
                }
                let list = match gesture.action {
                    InventoryEffectAction::Equip => &mut found.equips,
                    InventoryEffectAction::Unequip => &mut found.unequips,
                    InventoryEffectAction::PickUp => &mut found.picks_up,
                };
                for item in &gesture.items {
                    add(list, item);
                }
                continue;
            }
            CustomToolEffectBody::Wardrobe(gesture) => {
                let items = gesture.items.as_deref().unwrap_or_default();
                for item in items {
                    collect_placeholders(item, &declared, &mut found);
                }
                match gesture.action {
                    WardrobeEffectAction::Wear => {
                        for item in items {
                            add(&mut found.wears, item);
                        }
                    }
                    WardrobeEffectAction::TakeOff => {
                        for item in items {
                            add(&mut found.takes_off, item);
                        }
                    }
                    WardrobeEffectAction::ClearSlot => {
                        if let Some(slot) = gesture.slot.as_deref() {
                            add(&mut found.clears_slots, slot);
                        }
                    }
                }
                continue;
            }
            CustomToolEffectBody::Write { target, .. } => target,
        };
        let Ok(target) = parse_effect_target(target) else {
            continue; // load-rejected; nothing honest to report
        };
        match &target {
            EffectTarget::State { raw, .. } => {
                add(&mut found.state_writes, &raw[STATE_PREFIX.len()..]);
            }
            EffectTarget::Progress { id, .. } => add(&mut found.progress_writes, id),
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
    collect_state_refs(&as_json, &mut found.state, &mut found.inventory);

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
        progress: sorted(found.progress),
        progress_writes: sorted(found.progress_writes),
        now: found.now,
        character: found.character,
        wears: sorted(found.wears),
        takes_off: sorted(found.takes_off),
        clears_slots: sorted(found.clears_slots),
        // === P4.D329 ===
        inventory_reads: sorted(found.inventory),
        inventory_aggregates: sorted(found.inventory_aggregates),
        equips: sorted(found.equips),
        unequips: sorted(found.unequips),
        picks_up: sorted(found.picks_up),
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
            // The winning row has no vocabulary of its own (v4 `322a5488e`).
            PlaceholderRef::Outcome { .. } => {}
            PlaceholderRef::Params { ref name } => {
                if declared.contains(&name.as_str()) {
                    add(&mut found.params, name);
                }
            }
            PlaceholderRef::Metadata { ref key } => add(&mut found.metadata, key),
            PlaceholderRef::State { ref path } => add(&mut found.state, path),
            PlaceholderRef::Progress { ref id, .. } => add(&mut found.progress, id),
            PlaceholderRef::Now => found.now = true,
            PlaceholderRef::Character => found.character = true,
            // === P4.D329 ===
            PlaceholderRef::Inventory { ref reference, .. } => add(&mut found.inventory, reference),
            PlaceholderRef::Unknown { .. } => {}
        }
    }
}

/// Depth-first walk for `{ "$state": "..." }` objects anywhere in the tree,
/// and beside them `{ "$inventory": "<ref>.metadata.<key>" }` (whose ref is an
/// Inventory read — P4.D329; RETURNS, not walked further).
fn collect_state_refs(node: &Value, into: &mut Vec<String>, inventory: &mut Vec<String>) {
    match node {
        Value::Array(items) => {
            for item in items {
                collect_state_refs(item, into, inventory);
            }
        }
        Value::Object(_) => {
            if is_state_ref_value(node) {
                if let Some(path) = node.get("$state").and_then(Value::as_str) {
                    add(into, path);
                }
                return;
            }
            if is_inventory_ref_value(node) {
                if let Some((reference, _)) = node
                    .get("$inventory")
                    .and_then(Value::as_str)
                    .and_then(parse_inventory_operand)
                {
                    add(inventory, &reference);
                }
                return;
            }
            for value in node.as_object().expect("checked").values() {
                collect_state_refs(value, into, inventory);
            }
        }
        _ => {}
    }
}
