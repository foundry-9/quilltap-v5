//! Tier-1 differential (P4.D169 item 5): the Pascal side-effect APPLIER —
//! `pascal::side_effects`' planning pass, against v4's real
//! `lib/pascal/side-effects.ts`.
//!
//! v4's two applier suites mock `getRepositories` and assert on the ARGUMENTS
//! of the writes rather than on a table. The oracle drives v4's real
//! `applyCustomToolEffects` the same way, so both sides answer the same
//! question: given a run's resolved effects, what whole next value would each
//! store receive, what does `pascalMeta.effects` record, and what got warned.
//! v5 answers it through `side_effects::differential::plan_run`, the pure
//! planning half of the P4.D35 split — no database, because the thing under
//! test is the plan, not a table.
//!
//! ## The four comparands, and why each exists
//!
//! - `characterWrite` — the whole `metadata` object the ONE `characters.update`
//!   carries. This is the progress branch's load-bearing claim: a
//!   `progress.<id>.<field>` effect folds into the same replace instead of
//!   issuing a write of its own.
//! - `stateWrites` — every touched state store, in v4's fixed commit order,
//!   each with the id it is written under.
//! - `applied` — the returned `AppliedEffect[]`, serialized through v5's own
//!   payload serializer, so `previous`'s absent-vs-`null` distinction is part
//!   of the diff.
//! - `warns` — reassembled as a whole capture line, which pins the level, the
//!   target, the field NAMES and their order, and the message bytes. Without
//!   it the post-validation rollback is invisible: a dropped write and a write
//!   that never applied look identical in `applied`.
//!
//! Both sides read ONE committed corpus (`harness/oracle/fixtures/
//! pascal-side-effects.json`), so a case cannot be added to one side only —
//! the list-driven trap this family was written to avoid.
//!
//! Generate the oracle output (v4 @ 25f534c0b, Node 24; jest ignores `.claude/`
//! paths, hence the /tmp mirror). The recipe names the checkout, never a pin —
//! a lane behind v4 HEAD passes its pin on the driver's `--v4 <pin>` instead:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-pascal-sidefx-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/pascal-side-effects.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_SIDE_EFFECTS_CORPUS="$V5W/harness/oracle/fixtures/pascal-side-effects.json" \
//!   QT_ORACLE_OUT=/tmp/oracle-pascal-side-effects.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- pascal-side-effects
//! Run:
//!   QT_ORACLE_PASCAL_SIDE_EFFECTS=/tmp/oracle-pascal-side-effects.ndjson \
//!     cargo test -p quilltap-harness --test pascal_side_effects_equivalence

use quilltap_core::pascal::custom_tool_types::{
    parse_effect_target, EffectTarget, PutOnMode, WardrobeEffectAction,
};
use quilltap_core::pascal::custom_tools::{ResolvedEffect, ResolvedValue, ResolvedWardrobeGesture};
// P4.D329: the third pass.
use quilltap_core::inventory::refs::InventoryContext;
use quilltap_core::pascal::custom_tool_types::InventoryEffectAction;
use quilltap_core::pascal::custom_tools::ResolvedInventoryGesture;
use quilltap_core::pascal::side_effects::{
    differential::plan_run_logged, AppliedEffect, ApplyCustomToolEffectsParams, EffectTier,
};
use quilltap_core::pascal::wardrobe_effects::IndexedAppliedEffect;
use quilltap_core::services::inventory_change_effects::PendingInventoryAnnouncements;
use quilltap_core::state::cascade::{GroupCandidate, GroupTier, StateCascadeResult};
use quilltap_core::state::paths::PathKey;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/pascal-side-effects.json")
}

/// A corpus value, tagged so NaN survives JSON.
fn resolved_value(v: &Value) -> ResolvedValue {
    let o = v.as_object().expect("value is an object");
    if o.contains_key("nan") {
        return ResolvedValue::Number(f64::NAN);
    }
    if let Some(n) = o.get("n") {
        return ResolvedValue::Number(n.as_f64().expect("n is a number"));
    }
    if let Some(s) = o.get("s") {
        return ResolvedValue::String(s.as_str().expect("s is a string").to_string());
    }
    ResolvedValue::Bool(
        o.get("b")
            .and_then(Value::as_bool)
            .expect("value is one of n/s/b/nan"),
    )
}

/// Rebuild an `EffectTarget` from the corpus' explicit object.
///
/// Deliberately NOT `parse_effect_target` for every row: the underscore-guard
/// case carries `state._secrets.combo`, which load validation REFUSES, and v4's
/// own suite builds that target by hand for exactly that reason — the guard
/// under test is the applier's re-check, not the parser's. Every other shape
/// still round-trips through the parser below as a free consistency check.
fn effect_target(v: &Value) -> EffectTarget {
    let kind = v["kind"].as_str().expect("target has a kind");
    let raw = v["raw"].as_str().expect("target has a raw").to_string();
    match kind {
        "metadata" => EffectTarget::Metadata {
            key: v["key"].as_str().expect("metadata key").to_string(),
            raw,
        },
        "progress" => EffectTarget::Progress {
            id: v["id"].as_str().expect("progress id").to_string(),
            field: v["field"].as_str().expect("progress field").to_string(),
            raw,
        },
        "state" => EffectTarget::State {
            path: v["path"]
                .as_array()
                .expect("state path")
                .iter()
                .map(|seg| match seg {
                    Value::String(s) => PathKey::Prop(s.clone()),
                    Value::Number(n) => PathKey::Index(n.as_u64().expect("index") as usize),
                    other => panic!("unknown path segment {other}"),
                })
                .collect(),
            raw,
        },
        other => panic!("unknown target kind {other}"),
    }
}

fn cascade_of(v: &Value) -> Option<StateCascadeResult> {
    let o = v.as_object()?;
    let tier = &o["groupTier"];
    Some(StateCascadeResult {
        chat_state: o["chatState"].clone(),
        project_state: o["projectState"].clone(),
        group_state: o["groupState"].clone(),
        general_state: o["generalState"].clone(),
        merged: o["merged"].clone(),
        group_tier: GroupTier {
            status: match tier["status"].as_str().expect("groupTier status") {
                "single" => "single",
                "ambiguous" => "ambiguous",
                "none" => "none",
                other => panic!("unknown groupTier status {other}"),
            },
            candidates: tier["candidates"]
                .as_array()
                .expect("candidates")
                .iter()
                .map(|c| GroupCandidate {
                    id: c["id"].as_str().expect("candidate id").to_string(),
                    name: c["name"].as_str().expect("candidate name").to_string(),
                })
                .collect(),
            applied_group_id: tier
                .get("appliedGroupId")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        project_id: o
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// v4's `logger.warn(message, bag)` as this side's capture line would render it.
///
/// Two warn shapes reach here, and the mapping is bag key → tracing field name
/// in v4's own declaration order. One field is deliberately NOT named the same:
/// v4's underscore-guard bag carries `target`, which is `tracing`'s own reserved
/// macro key, so v5 spells it `effect_target`. That is a forced rename, recorded
/// here rather than hidden — the VALUE is byte-compared either way.
fn expected_warn_line(warn: &Value) -> String {
    let bag = &warn["bag"];
    let message = warn["message"].as_str().expect("message");
    // `tracing` registers `message` as the callsite's FIRST field and the
    // capture layer renders it through `record_debug` on a `format_args!`,
    // whose `Debug` is the formatted text with no quotes — so the narration
    // leads the line and the `key=value` pairs follow in declaration order.
    let head = format!(
        "WARN quilltap::pascal {message} context={} chat_id={} tool={}",
        bag["context"].as_str().expect("context"),
        bag["chatId"].as_str().expect("chatId"),
        bag["tool"].as_str().expect("tool"),
    );
    // P4.D331: the log pass's warn — `effectIndex`, `error` (this module's
    // snake_case field convention: `effect_index`).
    if let Some(index) = bag.get("effectIndex") {
        return format!(
            "{head} effect_index={index} error={}",
            bag["error"].as_str().expect("error"),
        );
    }
    match bag.get("progressionId").and_then(Value::as_str) {
        Some(id) => format!(
            "{head} progression_id={id} issue={}",
            bag["issue"].as_str().expect("issue"),
        ),
        None => format!(
            "{head} effect_target={}",
            bag["target"]
                .as_str()
                .expect("underscore-guard bag carries `target`"),
        ),
    }
}

/// A corpus `wardrobe` entry — v4's `ResolvedWardrobeGesture` as JSON.
fn wardrobe_gesture(v: &Value) -> ResolvedWardrobeGesture {
    let action = match v["action"].as_str().expect("gesture action") {
        "wear" => WardrobeEffectAction::Wear,
        "take_off" => WardrobeEffectAction::TakeOff,
        "clear_slot" => WardrobeEffectAction::ClearSlot,
        other => panic!("unknown gesture action {other}"),
    };
    ResolvedWardrobeGesture {
        action,
        items: v["items"]
            .as_array()
            .expect("gesture items always ride")
            .iter()
            .map(|i| i.as_str().expect("item").to_string())
            .collect(),
        mode: v
            .get("mode")
            .and_then(Value::as_str)
            .map(|m| PutOnMode::parse(m).expect("mode")),
        slot: v.get("slot").and_then(Value::as_str).map(str::to_string),
    }
}

/// P4.D329: a corpus `inventory` entry — v4's `ResolvedInventoryGesture`.
fn inventory_gesture(v: &Value) -> ResolvedInventoryGesture {
    let action = match v["action"].as_str().expect("gesture action") {
        "equip" => InventoryEffectAction::Equip,
        "unequip" => InventoryEffectAction::Unequip,
        "pick_up" => InventoryEffectAction::PickUp,
        other => panic!("unknown inventory action {other}"),
    };
    ResolvedInventoryGesture {
        action,
        items: v["items"]
            .as_array()
            .expect("gesture items")
            .iter()
            .map(|i| i.as_str().expect("item").to_string())
            .collect(),
        equip: v.get("equip").and_then(Value::as_bool),
    }
}

/// A row's scripted `wardrobeReturns` — what v4's mocked pass hands back.
fn wardrobe_plan(v: Option<&Value>) -> Option<Vec<IndexedAppliedEffect>> {
    let list = v?.as_array()?;
    Some(
        list.iter()
            .map(|e| IndexedAppliedEffect {
                index: e["index"].as_u64().expect("index") as usize,
                applied: AppliedEffect {
                    target: e["applied"]["target"].as_str().expect("target").to_string(),
                    previous: e["applied"].get("previous").cloned(),
                    next: e["applied"]["next"].clone(),
                    tier: None,
                },
            })
            .collect(),
    )
}

#[test]
fn pascal_side_effects_planning_matches_v4() {
    let path = match std::env::var("QT_ORACLE_PASCAL_SIDE_EFFECTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_SIDE_EFFECTS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let expected: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row is JSON"))
        .collect();

    let corpus_text = std::fs::read_to_string(corpus_path()).expect("corpus reads");
    let corpus: Value = serde_json::from_str(&corpus_text).expect("corpus is JSON");
    let default_now = corpus["nowMs"].as_i64().expect("corpus nowMs");
    let cases = corpus["cases"].as_array().expect("corpus cases");

    assert_eq!(
        cases.len(),
        expected.len(),
        "the oracle is stale: {} corpus cases against {} recorded rows — regenerate",
        cases.len(),
        expected.len()
    );
    // The corpus is what this family is for; a truncated one would pass every
    // remaining row silently.
    assert!(
        cases.len() >= 61,
        "corpus shrank to {} cases (>= 61 expected)",
        cases.len()
    );

    let mut warned_rows = 0usize;
    let mut wardrobe_call_rows = 0usize;
    let mut inventory_call_rows = 0usize;
    let mut character_writes = 0usize;
    let mut progress_writes = 0usize;
    let mut state_write_rows = 0usize;

    for (case, want) in cases.iter().zip(expected.iter()) {
        let name = case["name"].as_str().expect("case name");
        assert_eq!(
            name,
            want["name"].as_str().expect("row name"),
            "corpus and oracle rows are out of step"
        );

        let effects: Vec<ResolvedEffect> = case["effects"]
            .as_array()
            .expect("effects")
            .iter()
            .map(|e| {
                let index = e["index"].as_u64().expect("effect index") as usize;
                // P4.D331: a resolved log line.
                if let Some(log) = e.get("log").and_then(Value::as_str) {
                    return ResolvedEffect::Log {
                        index,
                        log: log.to_string(),
                    };
                }
                if let Some(g) = e.get("wardrobe") {
                    return ResolvedEffect::Wardrobe {
                        index,
                        wardrobe: wardrobe_gesture(g),
                    };
                }
                if let Some(g) = e.get("inventory") {
                    return ResolvedEffect::Inventory {
                        index,
                        inventory: inventory_gesture(g),
                    };
                }
                match e.get("skipped").and_then(Value::as_str) {
                    Some(reason) => ResolvedEffect::Skipped {
                        index,
                        reason: reason.to_string(),
                    },
                    None => {
                        let target = effect_target(&e["target"]);
                        // Free consistency check: every target the load-time
                        // parser accepts must round-trip to the same shape.
                        if let Ok(parsed) =
                            parse_effect_target(e["target"]["raw"].as_str().unwrap())
                        {
                            assert_eq!(
                                parsed, target,
                                "corpus target disagrees with the parser: {name}"
                            );
                        }
                        ResolvedEffect::Applicable {
                            index,
                            target,
                            value: resolved_value(&e["value"]),
                        }
                    }
                }
            })
            .collect();

        let cascade = cascade_of(&case["cascade"]);
        let snapshot: Map<String, Value> = case["metadataSnapshot"]
            .as_object()
            .expect("metadataSnapshot")
            .clone();
        let snapshot_before = snapshot.clone();
        let character_id = case["characterId"].as_str();
        let now_ms = case
            .get("nowMs")
            .and_then(Value::as_i64)
            .unwrap_or(default_now);

        let user_id = case
            .get("userId")
            .and_then(Value::as_str)
            .unwrap_or("user-1");
        let scripted = wardrobe_plan(case.get("wardrobeReturns"));
        // P4.D329: the inventory pass's script (the wardrobe's reader — the
        // returns share `IndexedAppliedEffect`'s shape), and the run's bag and
        // turn map by sentinel.
        let carried = wardrobe_plan(case.get("inventoryReturns"));
        let bag = quilltap_core::pascal::bench_inventory::mock_inventory_context(&[]);
        let bag_given = case.get("inventory").and_then(Value::as_bool) == Some(true);
        let pending: PendingInventoryAnnouncements = Default::default();
        let pending_given = case
            .get("pendingInventoryAnnouncements")
            .and_then(Value::as_bool)
            == Some(true);
        // P4.D331: the log pass's repository, as v4's recorder answers it.
        let log_error = case.get("logCreateError").and_then(Value::as_str);
        let mut log_creates: Vec<Value> = Vec::new();
        let (run, lines) = quilltap_core::test_support::captured_with(|| {
            plan_run_logged(
                ApplyCustomToolEffectsParams {
                    user_id,
                    chat_id: case["chatId"].as_str().expect("chatId"),
                    tool_name: case["toolName"].as_str().expect("toolName"),
                    effects: &effects,
                    cascade: cascade.as_ref(),
                    character_id,
                    metadata_snapshot: &snapshot,
                    now_ms,
                    inventory: bag_given.then_some(&bag as &dyn InventoryContext),
                    pending_inventory_announcements: pending_given.then_some(&pending),
                },
                scripted,
                carried,
                |message, chat_id, character_id| {
                    log_creates.push(json!({
                        "message": message,
                        "chatId": chat_id,
                        "characterId": character_id,
                    }));
                    if let Some(e) = log_error {
                        return Err(quilltap_core::db::DbError::Internal(e.to_string()));
                    }
                    Ok(quilltap_core::db::log_entries::LogEntryRow {
                        id: format!("log-{}", log_creates.len()),
                        message: message.to_string(),
                        chat_id: chat_id.to_string(),
                        character_id: character_id.map(str::to_string),
                        created_at: "2026-10-20T00:00:00.000Z".to_string(),
                        updated_at: "2026-10-20T00:00:00.000Z".to_string(),
                    })
                },
            )
        });
        assert_eq!(
            Value::Array(log_creates),
            want["logCreates"],
            "the log pass's create calls differ: {name}"
        );

        // --- applied (bytes: the merge's ORDER and each record's key order) --
        assert_eq!(
            serde_json::to_string(&run.applied).expect("applied serializes"),
            serde_json::to_string(&want["applied"]).unwrap(),
            "applied effects differ: {name}"
        );

        // --- the wardrobe pass's call (P4.D306) -----------------------------
        // v4's turn set rides the pass's argument; v5's rides
        // `apply_effects_for_run` beside it (the notify is after the write —
        // `pascal_wardrobe_effects_equivalence` proves the set is USED), so the
        // key is carried over from the row it was handed to.
        let got_calls: Vec<Value> = run
            .wardrobe_call
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("userId".into(), json!(c.user_id));
                m.insert("chatId".into(), json!(c.chat_id));
                m.insert("toolName".into(), json!(c.tool_name));
                m.insert(
                    "characterId".into(),
                    c.character_id
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                );
                m.insert(
                    "gestures".into(),
                    Value::Array(
                        c.gestures
                            .iter()
                            .map(|(index, g)| json!({ "index": index, "wardrobe": g }))
                            .collect(),
                    ),
                );
                if let Some(set) = case.get("pendingWardrobeAnnouncements") {
                    m.insert("pendingWardrobeAnnouncements".into(), set.clone());
                }
                Value::Object(m)
            })
            .collect();
        assert_eq!(
            serde_json::to_string(&got_calls).unwrap(),
            serde_json::to_string(&want["wardrobeCalls"]).unwrap(),
            "the wardrobe pass's call differs: {name}"
        );
        if !got_calls.is_empty() {
            wardrobe_call_rows += 1;
        }

        // --- the inventory pass's call (P4.D329) ----------------------------
        let got_inventory_calls: Vec<Value> = run
            .inventory_call
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("userId".into(), json!(c.user_id));
                m.insert("chatId".into(), json!(c.chat_id));
                m.insert("toolName".into(), json!(c.tool_name));
                m.insert(
                    "characterId".into(),
                    c.character_id
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                );
                m.insert(
                    "gestures".into(),
                    Value::Array(
                        c.gestures
                            .iter()
                            .map(|(index, g)| json!({ "index": index, "inventory": g }))
                            .collect(),
                    ),
                );
                if c.inventory {
                    m.insert("inventory".into(), json!("the run bag"));
                }
                if c.pending_inventory_announcements {
                    m.insert("pendingInventoryAnnouncements".into(), json!(true));
                }
                Value::Object(m)
            })
            .collect();
        assert_eq!(
            serde_json::to_string(&got_inventory_calls).unwrap(),
            serde_json::to_string(&want["inventoryCalls"]).unwrap(),
            "the inventory pass's call differs: {name}"
        );
        if !got_inventory_calls.is_empty() {
            inventory_call_rows += 1;
        }

        // --- the one character write ---------------------------------------
        let got_character = match &run.character_write {
            Some(m) => Value::Object(m.clone()),
            None => Value::Null,
        };
        assert_eq!(
            got_character, want["characterWrite"],
            "character write differs: {name}"
        );
        let want_count = want["characterWriteCount"].as_u64().expect("write count");
        assert_eq!(
            u64::from(run.character_write.is_some()),
            want_count,
            "character write COUNT differs: {name} — v4 batches progress writes \
             into the one metadata replace, never a write of its own"
        );
        if run.character_write.is_some() {
            character_writes += 1;
            assert_eq!(
                character_id.map(Value::from).unwrap_or(Value::Null),
                want["characterWriteId"],
                "character write id differs: {name}"
            );
            if got_character.get("progressions").is_some() {
                progress_writes += 1;
            }
        }

        // --- the state writes ----------------------------------------------
        let got_state: Vec<Value> = run
            .state_writes
            .iter()
            .map(|(tier, id, state)| {
                json!({
                    "store": tier.as_str(),
                    "id": id.clone().map(Value::from).unwrap_or(Value::Null),
                    "state": state,
                })
            })
            .collect();
        assert_eq!(
            Value::Array(got_state.clone()),
            want["stateWrites"],
            "state writes differ: {name}"
        );
        if !got_state.is_empty() {
            state_write_rows += 1;
        }

        // --- the warns ------------------------------------------------------
        let got_warns: Vec<&String> = lines.iter().filter(|l| l.starts_with("WARN ")).collect();
        let want_warns = want["warns"].as_array().expect("warns");
        assert_eq!(
            got_warns.len(),
            want_warns.len(),
            "warn COUNT differs: {name} (got {got_warns:?})"
        );
        for (got, w) in got_warns.iter().zip(want_warns.iter()) {
            assert_eq!(**got, expected_warn_line(w), "warn line differs: {name}");
        }
        if !want_warns.is_empty() {
            warned_rows += 1;
        }

        // --- the caller's snapshot -----------------------------------------
        assert_eq!(
            snapshot, snapshot_before,
            "the applier mutated the caller's snapshot: {name}"
        );
        assert_eq!(
            want["snapshotMutated"],
            Value::Bool(false),
            "the ORACLE reports v4 mutating the caller's snapshot: {name}"
        );
    }

    // Coverage floors — a corpus that stopped exercising a branch would
    // otherwise pass in silence.
    assert!(
        warned_rows >= 5,
        "only {warned_rows} rows exercise the post-validation warn"
    );
    assert!(
        character_writes >= 20,
        "only {character_writes} rows issue a character write"
    );
    assert!(
        progress_writes >= 18,
        "only {progress_writes} character writes carry a progressions record"
    );
    assert!(
        state_write_rows >= 10,
        "only {state_write_rows} rows issue a state write"
    );
    // P4.D329: the six `merge-inventory-*` rows hand gestures to the pass.
    assert!(
        inventory_call_rows >= 6,
        "only {inventory_call_rows} rows hand gestures to the inventory pass"
    );
    assert!(
        wardrobe_call_rows >= 5,
        "only {wardrobe_call_rows} rows hand gestures to the wardrobe pass"
    );
    assert!(
        run_touches_every_tier(cases),
        "the corpus no longer reaches all four state tiers"
    );

    eprintln!(
        "pascal_side_effects_equivalence: {} cases, {character_writes} character writes \
         ({progress_writes} carrying progressions), {state_write_rows} state-write rows, \
         {warned_rows} warned",
        cases.len()
    );
}

/// Every tier must still be reachable from the corpus — the state half of this
/// family is what makes it green on the pre-change applier.
fn run_touches_every_tier(cases: &[Value]) -> bool {
    let mut seen = [false; 4];
    for case in cases {
        let Some(cascade) = cascade_of(&case["cascade"]) else {
            continue;
        };
        for e in case["effects"].as_array().unwrap() {
            let Some(t) = e.get("target") else { continue };
            if t["kind"] != "state" {
                continue;
            }
            let Some(first) = t["path"].as_array().and_then(|p| p.first()) else {
                continue;
            };
            let first = first.as_str().unwrap_or_default();
            for (tier, state) in [
                (EffectTier::Project, &cascade.project_state),
                (EffectTier::Group, &cascade.group_state),
                (EffectTier::General, &cascade.general_state),
            ] {
                if state.get(first).is_some() {
                    seen[tier as usize] = true;
                }
            }
            seen[EffectTier::Chat as usize] = true;
        }
    }
    seen.iter().all(|s| *s)
}
