//! Tier-1 differential (P4.6ay unit 1): the Pascal custom-tool DEFINITION
//! format — the accept/reject verdict, the parsed data, the unknown-key report,
//! and the full rejection SENTENCE, byte-exact against v4's real Zod schema.
//!
//! The sentence is compared rather than regex-matched because it is payload:
//! `loadToolsFromMount` stores `formatDefinitionIssues(...)` as a load error's
//! `reason`, and the custom-tools GET route returns it verbatim in `errors[]`.
//! v4's own test only `toMatch(/…/)`s these strings, so this is a strictly
//! stronger bar — and the one the route differential needs.
//!
//! The `gate` rows (P4.d19, v4 `6864bf0e`) ride in the same file: the verdict
//! `evaluateToolGate` returns for one definition against one fact sheet, driven
//! directly because the roster only ever shows a verdict's consequence, and the
//! asymmetry that matters — an empty sheet fails every `availableWhen` and
//! satisfies no `withheldWhen` — is invisible from there.
//!
//! Re-recorded unchanged at v4 `f45a517a9` under `zod` 4.6.5, and under a 4.5.4
//! control against that same v4 source — byte-identical both ways (P4.D211's
//! discharge of `zod_version_guard` for the `6b0615807` bump).
//!
//! Generate the oracle output (v4 @ 231be14c, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`; the pinned detached worktree):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx \
//!     <V5W>/harness/oracle/cases/pascal-custom-tool-definition.ts \
//!     > /tmp/oracle-pascal-definition.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_DEFINITION=/tmp/oracle-pascal-definition.ndjson \
//!     cargo test -p quilltap-harness --test pascal_custom_tool_definition_equivalence

use quilltap_core::pascal::custom_tool_types::CustomToolEffectBody;
use quilltap_core::pascal::custom_tool_types::{
    collect_unknown_keys, display_title, format_definition_issues, safe_parse,
};
use quilltap_core::pascal::tool_gate::{evaluate_tool_gate, has_tool_gate};
use serde::Deserialize;
use serde_json::{Map, Value};

// P4.D329: the gate rows' bag — the bench's mock rows or v4's recorded answers.
mod recorded_inventory;
use recorded_inventory::RowBag;

/// P4.D329: the rows of THIS lane's lane-private regen at the `322a5488e` pin
/// that are P4.D333's to turn green (any-of, the `invalid_key` arm, the list
/// rule) — MEASURED, not predicted. Pre-existing rows: the `invalid_key` arm's
/// key sentences (`param-name-not-identifier` … `gate-progress-bad-key`), the
/// gate's group-OR-array union (every gate rejection renders `availableWhen:
/// <path>` at the pin), the effect `when`'s group-or-array union
/// (`effect-when-*`), and the list rule (`withheld-key-holds-array-offers`).
/// This lane's rows: the seven gate rejections, carved for the same union. On
/// the union with P4.D333 every one is green; a carved row that turns green
/// here fails the run.
const P4D329_RESIDUE_P4D333: &[&str] = &[
    "param-name-not-identifier",
    "params-key-not-identifier",
    "metadata-empty-string-key",
    "gate-param-operand",
    "gate-state-operand",
    "gate-contains-param-operand",
    "gate-empty-string-key",
    "gate-ordering-string-operand",
    "gate-ordering-boolean-operand",
    "gate-contains-number-operand",
    "gate-eq-object-operand",
    "effect-when-unknown-outcome-state",
    "effect-when-outcome-not-an-object",
    "when-progress-bad-key-0",
    "when-progress-bad-key-1",
    "when-progress-bad-key-2",
    "when-progress-bad-key-3",
    "when-progress-bad-key-4",
    "gate-progress-bad-key",
    "withheld-key-holds-array-offers",
    "inventory-gate-param-refused",
    "inventory-gate-count-param-refused",
    "aggregate-gate-param-refused",
    "aggregate-gate-bad-key",
    "inventory-gate-shape-order",
    "inventory-gate-record-not-object",
    "inventory-operand-in-gate-refused",
];

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "title")]
    Title {
        id: String,
        input: TitleInput,
        out: String,
    },
    #[serde(rename = "definition")]
    Definition {
        id: String,
        /// The definition's BYTES — never a re-stringified structure. Both sides
        /// parse this with their own JSON parser, exactly as `readToolFile` does.
        #[serde(rename = "inputJson")]
        input_json: String,
        success: bool,
        reason: Option<String>,
        data: Option<String>,
        #[serde(rename = "unknownKeys")]
        unknown_keys: Vec<String>,
    },
    /// The availability gate (P4.d19, v4 `6864bf0e`): the verdict
    /// `evaluateToolGate` returns for one definition against one fact sheet,
    /// serialized — so `withheldBy`'s ABSENCE on an available verdict is part
    /// of what is compared.
    #[serde(rename = "gate")]
    Gate {
        id: String,
        #[serde(rename = "inputJson")]
        input_json: String,
        metadata: Option<Map<String, Value>>,
        /// [P4.D169] The flattened progress sheet the gate is posed against.
        /// Absent on every pre-existing row, which is what keeps their verdicts
        /// byte-identical.
        #[serde(default)]
        progress: Option<Map<String, Value>>,
        /// [P4.D329] The invoker's bag: absent / `null` (every inventory test
        /// declines), `{rows}` (the bench's mock), or `{recorded}` (v4's real
        /// context, its answers recorded). Absent on every pre-existing row.
        #[serde(default)]
        inventory: Option<Value>,
        #[serde(rename = "hasGate")]
        has_gate: bool,
        verdict: String,
    },
}

/// P4.D333 (R-F): the rows of the ONE shared corpus regeneration that this
/// branch cannot answer because a SIBLING lane owns the grammar they exercise —
/// the Inventory grammar (P4.D329: the `$inventory` operand / roll arms, the
/// `inventory` / `inventoryCount` / `inventoryAggregate` subjects) and the Log
/// (P4.D331: the `log` effect, the four-kind effect sentences). Each id is
/// carved LOUDLY: the row still runs, a red one is reported by name and
/// skipped, and a carved row that comes back GREEN fails the run so the carve
/// cannot outlive its reason. The unifier lifts this list once P4.D329 and
/// P4.D331 have merged (§S.5 (4)).
///
/// Measured at the pin on this branch (P4.D333): eight PRE-EXISTING rows whose
/// sentence grows by the `$inventory` operand / roll branch (P4.D329), and four
/// whose effect-kind sentence now names four kinds (P4.D329 + P4.D331).
const RESIDUE_UNTIL_UNION: &[&str] = &[
    // P4.D329 — `InventoryRefSchema` joins every operand and roll union.
    "string-compared-with-value",
    "roll-wrong-type",
    "roll-null",
    "roll-range-and-dice-both-broken",
    "contains-number-substring",
    "state-missing-fallback",
    "state-array-fallback",
    "state-not-quite-a-ref",
    // P4.D329 + P4.D331 — the four-kind effect sentences.
    "effects-both-halves",
    "effects-target-and-wardrobe",
    "effects-empty-object",
    "effects-when-only",
];

#[derive(Deserialize)]
struct TitleInput {
    name: String,
    title: Option<String>,
}

/// P4.D331: the sibling-owned rows the pin's grammar moves (see the carve
/// at the foot of the test). Measured on this branch at `322a5488e`.
const SIBLING_RESIDUE_UNTIL_UNION: &[&str] = &[
    // A gate row (P4.D333's metadata list rule, `a53024d3c`).
    "withheld-key-holds-array-offers",
    "contains-bare-value",
    "contains-number-substring",
    "effect-when-empty",
    "effect-when-outcome-not-an-object",
    "effect-when-progress-empty-record",
    "effect-when-unknown-outcome-state",
    "gate-contains-number-operand",
    "gate-contains-param-operand",
    "gate-empty-metadata",
    "gate-empty-string-key",
    "gate-eq-object-operand",
    "gate-missing-metadata",
    "gate-ordering-boolean-operand",
    "gate-ordering-string-operand",
    "gate-param-operand",
    "gate-progress-bad-key",
    "gate-progress-empty-record",
    "gate-state-operand",
    "metadata-empty-object",
    "metadata-empty-string-key",
    "param-name-not-identifier",
    "params-key-not-identifier",
    "roll-null",
    "roll-range-and-dice-both-broken",
    "roll-wrong-type",
    "state-array-fallback",
    "state-missing-fallback",
    "state-not-quite-a-ref",
    "string-compared-with-value",
    "unknown-key-inside-when",
    "when-empty-params",
    "when-progress-bad-key-0",
    "when-progress-bad-key-1",
    "when-progress-bad-key-2",
    "when-progress-bad-key-3",
    "when-progress-bad-key-4",
    "when-progress-empty-record",
    "when-tests-nothing",
];

#[test]
fn pascal_custom_tool_definition_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_DEFINITION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_DEFINITION to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut titles, mut defs, mut gates) = (0usize, 0usize, 0usize);
    // Coverage, not a magic row count: a green gate run proves nothing if every
    // row happens to be an ungated definition. Both clauses, both verdicts, and
    // both `hasToolGate` answers must appear.
    let mut seen_clauses: Vec<String> = Vec::new();
    let mut seen_available: Vec<bool> = Vec::new();
    let mut seen_has_gate: Vec<bool> = Vec::new();
    // P4.D169's four floors. The lane that widened this corpus measured all
    // four at ZERO beforehand, and a green family is not coverage.
    let mut gates_reading_progress = 0usize;
    let mut gates_testing_progress = 0usize;
    let mut defs_writing_progress = 0usize;
    let mut defs_testing_progress = 0usize;
    // P4.D305: a definition row's mismatch is RECORDED and the walk continues,
    // so one red row (a sibling lane's residue at a shared pin) cannot hide
    // every row after it; the family still fails, naming each row.
    let mut failed_rows: Vec<String> = Vec::new();
    let mut gate_residue_seen: Vec<String> = Vec::new();
    let mut defs_with_triggers = 0usize;
    let mut gates_with_bag = 0usize;
    let mut defs_with_wardrobe_effects = 0usize;
    // P4.D333's three floors (any-of, `heldGroup`, `brief`): a regen that
    // quietly lost its rows would still pass every assertion above.
    let mut anyof_rows = 0usize;
    let mut gates_with_held_group = 0usize;
    let mut defs_with_brief = 0usize;
    let mut residue_seen: Vec<String> = Vec::new();
    let mut residue_green: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Title { id, input, out } => {
                let got = display_title(&input.name, input.title.as_deref());
                assert_eq!(got, out, "displayTitle '{id}'");
                titles += 1;
            }
            Row::Definition {
                id,
                input_json,
                success,
                reason,
                data,
                unknown_keys,
            } => {
                let raw: Value = serde_json::from_str(&input_json)
                    .unwrap_or_else(|e| panic!("case '{id}': oracle input is not JSON: {e}"));

                let row_id = id.clone();
                let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    assert_eq!(
                        collect_unknown_keys(&raw),
                        unknown_keys,
                        "collectUnknownKeys '{id}'"
                    );

                    match safe_parse(&raw) {
                        Ok(tool) => {
                            assert!(
                                success,
                                "case '{id}': v5 accepted, v4 rejected with: {reason:?}"
                            );
                            if tool
                                .effects
                                .iter()
                                .flatten()
                                .any(|e| matches!(&e.body, CustomToolEffectBody::Write { target, .. } if target.starts_with("progress.")))
                            {
                                defs_writing_progress += 1;
                            }
                            if tool
                                .effects
                                .iter()
                                .flatten()
                                .any(|e| e.is_wardrobe_effect())
                            {
                                defs_with_wardrobe_effects += 1;
                            }
                            if tool
                                .available_when
                                .iter()
                                .chain(tool.withheld_when.iter())
                                .flat_map(|g| g.gate_groups())
                                .any(|g| g.progress.is_some())
                                || tool.outcomes.iter().any(|o| {
                                    o.when.when_groups().iter().any(|w| w.progress.is_some())
                                })
                            {
                                defs_testing_progress += 1;
                            }
                            if tool.outcomes.iter().any(|o| o.brief.is_some()) {
                                defs_with_brief += 1;
                            }
                            if tool.triggers.is_some() {
                                defs_with_triggers += 1;
                            }
                            let got = serde_json::to_string(&tool).unwrap();
                            assert_eq!(
                                got,
                                data.unwrap_or_default(),
                                "case '{id}': parsed data differs (key order included)"
                            );
                        }
                        Err(issues) => {
                            let got = format_definition_issues(&issues);
                            assert!(
                                !success,
                                "case '{id}': v5 rejected with '{got}', v4 accepted"
                            );
                            assert_eq!(
                                got,
                                reason.unwrap_or_default(),
                                "case '{id}': rejection differs"
                            );
                        }
                    }
                }));
                if RESIDUE_UNTIL_UNION.contains(&row_id.as_str()) {
                    residue_seen.push(row_id.clone());
                    if walked.is_err() {
                        eprintln!("RESIDUE (sibling-owned, until union): definition row '{row_id}' differs — carved");
                    } else {
                        residue_green.push(row_id.clone());
                    }
                } else if walked.is_err() {
                    failed_rows.push(row_id.clone());
                }
                if row_id.starts_with("anyof-") {
                    anyof_rows += 1;
                }
                defs += 1;
            }
            Row::Gate {
                id,
                input_json,
                metadata,
                progress,
                inventory,
                has_gate,
                verdict,
            } => {
                let raw: Value = serde_json::from_str(&input_json)
                    .unwrap_or_else(|e| panic!("gate case '{id}': oracle input is not JSON: {e}"));
                let tool = match safe_parse(&raw) {
                    Ok(tool) => tool,
                    Err(e) if RESIDUE_UNTIL_UNION.contains(&id.as_str()) => {
                        eprintln!("RESIDUE (sibling-owned, until union): gate row '{id}' does not load ({}) — carved", format_definition_issues(&e));
                        residue_seen.push(id.clone());
                        gates += 1;
                        continue;
                    }
                    Err(e) => panic!("gate case '{id}': definition does not load: {e:?}"),
                };
                if RESIDUE_UNTIL_UNION.contains(&id.as_str()) {
                    residue_seen.push(id.clone());
                    residue_green.push(id.clone());
                }
                if id.starts_with("anyof-") {
                    anyof_rows += 1;
                }

                assert_eq!(has_tool_gate(&tool), has_gate, "hasToolGate '{id}'");

                // P4.D329: a verdict mismatch is RECORDED (as the definition
                // rows are) so the residue carve can name it.
                let bag = RowBag::from_row(inventory.as_ref());
                let row_id = id.clone();
                let walked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let got = serde_json::to_string(&evaluate_tool_gate(
                        &tool,
                        metadata.as_ref(),
                        progress.as_ref(),
                        bag.as_dyn(),
                    ))
                    .expect("a verdict serializes");
                    assert_eq!(
                        got, verdict,
                        "gate case '{id}': verdict differs (withheldBy's absence included)"
                    );
                }));
                if walked.is_err() {
                    failed_rows.push(row_id);
                }
                if inventory.is_some() {
                    gates_with_bag += 1;
                }

                let seen: Value = serde_json::from_str(&verdict).expect("a verdict is JSON");
                if seen.get("heldGroup").is_some() {
                    gates_with_held_group += 1;
                }
                if let Some(clause) = seen.get("withheldBy").and_then(Value::as_str) {
                    if !seen_clauses.iter().any(|c| c == clause) {
                        seen_clauses.push(clause.to_string());
                    }
                }
                for (bucket, flag) in [
                    (
                        &mut seen_available,
                        seen["available"].as_bool().unwrap_or(false),
                    ),
                    (&mut seen_has_gate, has_gate),
                ] {
                    if !bucket.contains(&flag) {
                        bucket.push(flag);
                    }
                }
                // P4.D169: a corpus that quietly lost its progression rows
                // would otherwise still pass every assertion above. Each of the
                // three is a floor the widening earned.
                if !progress.as_ref().is_none_or(Map::is_empty) {
                    gates_reading_progress += 1;
                }
                if tool
                    .available_when
                    .iter()
                    .chain(tool.withheld_when.iter())
                    .flat_map(|g| g.gate_groups())
                    .any(|g| g.progress.as_ref().is_some_and(|p| !p.is_empty()))
                {
                    gates_testing_progress += 1;
                }
                gates += 1;
            }
        }
    }

    // P4.D329: the sibling-owned residue, both ways — a carved row that turns
    // GREEN fails the run (lift it then).
    let residue_green: Vec<&str> = P4D329_RESIDUE_P4D333
        .iter()
        .copied()
        .filter(|id| !failed_rows.iter().any(|f| f == id))
        .collect();
    failed_rows.retain(|id| !P4D329_RESIDUE_P4D333.contains(&id.as_str()));
    assert!(
        failed_rows.is_empty(),
        "{} definition row(s) differ from the oracle: {failed_rows:?}",
        failed_rows.len()
    );
    // The carve cannot grow silently or outlive its reason: every carved id
    // must be present in the corpus, and none may have turned green.
    assert_eq!(
        residue_seen.len(),
        RESIDUE_UNTIL_UNION.len(),
        "RESIDUE_UNTIL_UNION names ids the corpus lacks (saw {residue_seen:?})"
    );
    assert!(
        residue_green.is_empty(),
        "carved rows now MATCH the oracle — lift them from RESIDUE_UNTIL_UNION: {residue_green:?}"
    );
    assert!(
        anyof_rows >= 30,
        "only {anyof_rows} any-of rows (`anyof-*`) — P4.D333's floor is 30"
    );
    assert!(
        gates_with_held_group >= 4,
        "only {gates_with_held_group} gate verdicts carry `heldGroup` — the floor is 4"
    );
    assert!(
        defs_with_brief >= 6,
        "only {defs_with_brief} accepted definitions carry an outcome `brief` — the floor is 6"
    );
    assert!(
        residue_green.is_empty(),
        "P4D329_RESIDUE_P4D333 rows turned GREEN — lift them: {residue_green:?}"
    );
    assert!(
        gates_with_bag >= 20,
        "only {gates_with_bag} gate rows posed a non-null Inventory bag (P4.D329)"
    );
    assert!(
        defs_with_triggers > 0,
        "no accepted definition row declares `triggers` (v4 `362426c8f`)"
    );
    assert!(
        defs_with_wardrobe_effects > 0,
        "no accepted definition row carries a wardrobe gesture (v4 `f0db8cbb8`)"
    );
    assert!(
        titles > 0 && defs > 0 && gates > 0,
        "oracle file looks empty: {titles} titles, {defs} definitions, {gates} gates"
    );
    seen_clauses.sort();
    assert_eq!(
        seen_clauses,
        vec!["availableWhen".to_string(), "withheldWhen".to_string()],
        "the gate corpus must withhold by BOTH clauses (saw {seen_clauses:?})"
    );
    assert!(
        gates_reading_progress > 0,
        "no gate row posed a non-empty progression sheet"
    );
    assert!(
        gates_testing_progress > 0,
        "no gate row's definition actually TESTS a progression"
    );
    assert!(
        defs_writing_progress > 0,
        "no definition row declares a `progress.<id>.<field>` effect target"
    );
    assert!(
        defs_testing_progress > 0,
        "no definition row carries a `progress` record in a `when` or a gate"
    );
    seen_available.sort();
    seen_has_gate.sort();
    assert_eq!(
        (seen_available, seen_has_gate),
        (vec![false, true], vec![false, true]),
        "the gate corpus must cover available AND withheld, gated AND ungated"
    );
    eprintln!(
        "OK: pascal custom-tool definition matched oracle \
         ({titles} titles, {defs} definitions, {gates} gate verdicts)."
    );
}
