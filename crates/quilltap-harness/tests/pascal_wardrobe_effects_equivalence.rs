//! Tier-2/3 differential: the Pascal wardrobe-effects applier (P4.D306; v4
//! `lib/pascal/wardrobe-effects.ts`, NEW at `f0db8cbb8`), ported as
//! `quilltap_core::pascal::wardrobe_effects` behind
//! `pascal::side_effects::apply_effects_for_run`.
//!
//! Both sides run the SAME step matrix (`pascal-wardrobe-effects.json`, 23
//! steps) in order over ONE copy of the fixture
//! `build-pascal-wardrobe-effects-fixture.ts` bakes (a character's vault
//! wardrobe — v4's own test wardrobe — a General archetype that shares a
//! title with the character's own shirt, a chat with avatar generation off,
//! `background_jobs` materialized). Each step plants the character's starting
//! outfit with the same raw UPDATE, then hands its RESOLVED gestures to the
//! applier: v4's REAL `applyWardrobeEffects` (REAL pool, wear-ops, chokepoint
//! and notify) under tsx; v5's PRODUCTION seam `apply_effects_for_run` (the
//! in-write half inside `Db::write`, then the after-commit notify + DEBUG on
//! this thread) with a synthetic run result carrying the step's gestures —
//! the seam both v5 entrances call, which is why it is driven rather than the
//! inner fn. Compared per step, as BYTES (`serde_json::to_string` — memory
//! note `serde-json-map-eq-ignores-order`):
//!
//! - the applied records (v4's `IndexedAppliedEffect[]`'s `applied` halves —
//!   the run has no writes, so the merged list IS the pass's list in order;
//!   the index merge itself is `pascal_side_effects_equivalence`'s);
//! - the character's `equippedOutfit` after the step;
//! - every `wardrobe_wear_stats` row (minus the clock columns), cumulative —
//!   one credit per committed transition, `source=tool` on put-ons;
//! - the step's announcement set (sorted; `null` when none was passed — the
//!   operator route / trigger runner shape);
//! - the `background_jobs` table, projected as the tier-3 outfit family does;
//! - the lines: the module's own (context `pascal.wardrobe-effects`, which on
//!   the no-set path includes the scheduler's `Wardrobe outfit announcement
//!   scheduled` — the notify hands that context down) WHOLE, every field in
//!   v4's order (a v4 array field is v5's `…Json`), and the chokepoint's
//!   `[applyDisplacement] Outfit change committed` by its `mode` / `itemId` /
//!   `slot` / `source` (the keystone owns the rest of that line).
//!
//! The lines inside the write run on the WRITER thread, which neither
//! `captured_with` nor `global_capture` sees (memory note
//! `capture-rig-writer-thread`, third trap) — so this binary owns a
//! process-wide recording layer, and its ONE test is the only thing that logs.
//!
//! Recorded: v4's `wardrobe-effects.test.ts` scripts a THROWING commit through
//! its mocked `applyDisplacement` (`disk on fire`); a real-DB oracle cannot,
//! so that branch is `wardrobe_effects.rs`'s rig test. v4's `loadToolPool`
//! catch is unreachable through v4's real code (every pool read is a
//! fallback); v5's home for it is a degraded open (no mount-index writer),
//! pinned by the same rig. The pool is read ONCE per run on both sides by
//! construction — pinned by inspection, not by a counter.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN=/tmp/qt-pwe-main.db QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT=/tmp/qt-pwe-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-pascal-wardrobe-effects-fixture.ts
//!   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN=/tmp/qt-pwe-main.db QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT=/tmp/qt-pwe-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/pascal-wardrobe-effects.ts > /tmp/oracle-pascal-wardrobe-effects.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_WARDROBE_EFFECTS=/tmp/oracle-pascal-wardrobe-effects.ndjson \
//!   QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN=/tmp/qt-pwe-main.db QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT=/tmp/qt-pwe-mount.db \
//!     cargo test -p quilltap-harness --test pascal_wardrobe_effects_equivalence -- --nocapture

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::pascal::custom_tool_types::{
    OutcomeState, PutOnMode, Visibility, WardrobeEffectAction,
};
use quilltap_core::pascal::custom_tools::{
    CustomToolRunResult, ResolvedEffect, ResolvedWardrobeGesture, RollForm,
};
use quilltap_core::pascal::side_effects::apply_effects_for_run;
use quilltap_core::services::outfit_change_effects::{
    flush_pending_wardrobe_announcements, new_wardrobe_turn_containers,
};
use quilltap_core::services::tool_execution::{
    create_tool_context, ToolCall, ToolExecutionContext, ToolRunner,
};
use quilltap_core::test_support::CaptureLayer;
use quilltap_core::tools::executor::BuiltInToolRunner;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    pepper: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "toolName")]
    tool_name: String,
    #[serde(rename = "participantId")]
    participant_id: String,
    items: Vec<Value>,
    states: Map<String, Value>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    name: String,
    start: String,
    /// Absent → the spec's character; `null` → a run nobody made.
    #[serde(default, rename = "characterId", deserialize_with = "present")]
    character_id: Option<Option<String>>,
    #[serde(default = "yes", rename = "withSet")]
    with_set: bool,
    gestures: Vec<Value>,
}

fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(d)?))
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct OracleRow {
    step: String,
    returned: String,
    equipped: Value,
    wear: Value,
    set: Value,
    /// [P4.D316] the turn's history Map (`null` without containers).
    map: Value,
    /// [P4.D316] `chat_wardrobe_history`, ids / createdAt not dumped.
    history: Value,
    jobs: Value,
    logs: Vec<Value>,
}

const MODULE_CONTEXT: &str = "pascal.wardrobe-effects";
const COMMITTED: &str = "[applyDisplacement] Outfit change committed";
/// The messages whose lines are this unit's (every one carries the module's
/// context on v4's side).
const MODULE_MESSAGES: [&str; 7] = [
    "Custom tool: no rolling character, wardrobe effects skipped",
    "Custom tool wardrobe effects skipped — the wardrobe could not be read",
    "Custom tool wardrobe effect not applied",
    "Custom tool wardrobe effect failed; that change was dropped",
    "Custom tool wardrobe effects applied",
    "Wardrobe outfit announcement scheduled",
    "Failed to schedule wardrobe outfit announcement",
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// The process-wide recorder: every thread's events (the writer's included).
fn recorder() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns its global subscriber");
        logs
    })
}

fn gesture(v: &Value) -> ResolvedWardrobeGesture {
    ResolvedWardrobeGesture {
        action: match v["action"].as_str().expect("action") {
            "wear" => WardrobeEffectAction::Wear,
            "take_off" => WardrobeEffectAction::TakeOff,
            "clear_slot" => WardrobeEffectAction::ClearSlot,
            other => panic!("unknown action {other}"),
        },
        items: v["items"]
            .as_array()
            .expect("items always ride")
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

/// A finished run whose only effects are the step's gestures.
fn run_result(tool: &str, gestures: &[Value]) -> CustomToolRunResult {
    CustomToolRunResult {
        tool: tool.to_string(),
        params: Vec::new(),
        roll_form: RollForm::Range,
        notation: None,
        raw: 1.0,
        dice_rolls: None,
        value: 1.0,
        state: OutcomeState::Info,
        outcome_index: 0,
        message: "-".into(),
        dice_breakdown: String::new(),
        visibility: Visibility::Public,
        metadata_tested: None,
        llm: None,
        chip_label: None,
        effects: Some(
            gestures
                .iter()
                .map(|g| ResolvedEffect::Wardrobe {
                    index: g["index"].as_u64().expect("index") as usize,
                    wardrobe: gesture(&g["wardrobe"]),
                })
                .collect(),
        ),
    }
}

type Line = (String, String, Vec<(String, String)>);

/// v4's captured line → `(level, message, fields in order)`; an array/object
/// field is v5's `…Json` (the file-layer convention).
fn oracle_line(v: &Value) -> Line {
    let fields = v["fields"]
        .as_object()
        .expect("fields")
        .iter()
        .map(|(k, val)| match val {
            Value::String(s) => (k.clone(), s.clone()),
            Value::Array(_) | Value::Object(_) => (format!("{k}Json"), val.to_string()),
            other => (k.clone(), other.to_string()),
        })
        .collect();
    (
        v["level"].as_str().expect("level").to_lowercase(),
        v["message"].as_str().expect("message").to_string(),
        fields,
    )
}

/// A rig line (`LEVEL target message k=v …`) → the same shape. Field VALUES may
/// carry spaces (a `reason`), so a field runs up to the next ` <ident>=`.
fn rust_line(line: &str, messages: &[&str]) -> Option<Line> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let message = messages.iter().find(|m| rest.starts_with(**m))?;
    let mut tail = rest[message.len()..].trim_start();
    let mut fields = Vec::new();
    while !tail.is_empty() {
        let (key, after) = tail.split_once('=')?;
        let mut end = after.len();
        let mut search = 0;
        while let Some(off) = after[search..].find(' ') {
            let cand = &after[search + off + 1..];
            let ident_len = cand
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            if ident_len > 0 && cand[ident_len..].starts_with('=') {
                end = search + off;
                break;
            }
            search += off + 1;
        }
        fields.push((key.to_string(), after[..end].to_string()));
        tail = after[end..].trim_start();
    }
    Some((level.to_lowercase(), message.to_string(), fields))
}

/// Keep only the chokepoint line's four comparands.
fn committed_tail(line: Line) -> Line {
    let (level, message, fields) = line;
    let keep = ["mode", "itemId", "slot", "source"];
    let fields = keep
        .iter()
        .filter_map(|k| fields.iter().find(|(f, _)| f == k).cloned())
        .collect();
    (level, message, fields)
}

/// [P4.D316] `chat_wardrobe_history` exactly as the oracle's `dumpHistory`.
fn dump_history(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            r#"SELECT "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent" FROM chat_wardrobe_history ORDER BY "chatId", "characterId", "entryIndex""#,
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "chatId": r.get::<_, String>(0)?,
                    "characterId": r.get::<_, String>(1)?,
                    "entryIndex": r.get::<_, i64>(2)?,
                    "slots": r.get::<_, String>(3)?,
                    "slotsHash": r.get::<_, String>(4)?,
                    "source": r.get::<_, String>(5)?,
                    "actorCharacterId": r.get::<_, Option<String>>(6)?,
                    "actorToolName": r.get::<_, Option<String>>(7)?,
                    "isCurrent": r.get::<_, i64>(8)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Array(rows))
    })
    .expect("dump chat_wardrobe_history")
}

fn dump_jobs(db: &Db) -> Value {
    let dump = db
        .read_main(|c| quilltap_core::db::dump_table_json_conn(c, "background_jobs", "rowid"))
        .expect("dump background_jobs");
    let mut jobs: Vec<Value> = dump["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| {
            let payload: Value = match &r["payload"] {
                Value::String(s) => serde_json::from_str(s).expect("payload json"),
                other => other.clone(),
            };
            json!({
                "type": r["type"],
                "status": r["status"],
                "userId": r["userId"],
                "priority": r["priority"],
                "maxAttempts": r["maxAttempts"],
                "attempts": r["attempts"],
                "payload": payload,
            })
        })
        .collect();
    jobs.sort_by_key(|j| format!("{}\0{}", j["type"].as_str().unwrap_or(""), j["payload"]));
    Value::Array(jobs)
}

fn dump_wear(db: &Db) -> Value {
    let mut rows: Vec<Value> = db
        .read_main(|c| {
            let mut st = c.prepare(
                r#"SELECT "itemId", "wearerCharacterId", "wearCount", "lastWornChatId" FROM "wardrobe_wear_stats""#,
            )?;
            let rows = st
                .query_map([], |r| {
                    Ok(json!({
                        "itemId": r.get::<_, String>(0)?,
                        "wearerCharacterId": r.get::<_, Option<String>>(1)?,
                        "wearCount": r.get::<_, i64>(2)?,
                        "lastWornChatId": r.get::<_, Option<String>>(3)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("dump wardrobe_wear_stats");
    rows.sort_by_key(|r| {
        format!(
            "{}|{}",
            r["itemId"].as_str().unwrap_or(""),
            r["wearerCharacterId"].as_str().unwrap_or("")
        )
    });
    Value::Array(rows)
}

#[test]
fn pascal_wardrobe_effects_match_oracle() {
    let logs = recorder();
    let (Some(oracle_path), Some(fx_main), Some(fx_mount)) = (
        env_or_skip("QT_ORACLE_PASCAL_WARDROBE_EFFECTS"),
        env_or_skip("QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MAIN"),
        env_or_skip("QT_FIXTURE_PASCAL_WARDROBE_EFFECTS_MOUNT"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("pascal-wardrobe-effects.json"))
            .expect("spec"),
    )
    .expect("parse spec");
    let oracle: Vec<OracleRow> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(
        oracle.len(),
        spec.steps.len(),
        "oracle step count — regenerate"
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-pwe-")
        .tempdir()
        .expect("scratch");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(&fx_main, &main).expect("copy main");
    std::fs::copy(&fx_mount, &mount).expect("copy mount");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.pepper,
    )
    .expect("open the fixture copy");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // [P4.D316] P4.D315's one v5-side plant, on this per-case copy (R-S).
    rt.block_on(db.write(|w| {
        quilltap_core::test_support::ensure_chat_wardrobe_history_on(w.main().connection());
        Ok(())
    }))
    .expect("plant chat_wardrobe_history");

    let mut failures: Vec<String> = Vec::new();
    let (mut applied_rows, mut module_lines, mut committed_lines) = (0usize, 0usize, 0usize);
    for (step, want) in spec.steps.iter().zip(&oracle) {
        assert_eq!(step.name, want.step, "oracle step order");
        let start = spec.states.get(&step.start).expect("start state").clone();
        let outfit = json!({ spec.character_id.clone(): start }).to_string();
        let chat_id = spec.chat_id.clone();
        rt.block_on(db.write(move |w| {
            w.main()
                .connection()
                .execute(
                    r#"UPDATE "chats" SET "equippedOutfit" = ?1 WHERE "id" = ?2"#,
                    rusqlite::params![outfit, chat_id],
                )
                .map(|_| ())
                .map_err(Into::into)
        }))
        .expect("plant the starting outfit");

        let character_id = match &step.character_id {
            None => Some(spec.character_id.clone()),
            Some(c) => c.clone(),
        };
        // [P4.D316] both turn containers from ONE factory, or neither.
        let containers = step.with_set.then(new_wardrobe_turn_containers);
        let set: Option<Arc<Mutex<HashSet<String>>>> = containers.as_ref().map(|c| c.0.clone());
        let result = run_result(&spec.tool_name, &step.gestures);

        logs.lock().unwrap().clear();
        let applied = rt.block_on(apply_effects_for_run(
            &db,
            &spec.user_id,
            &spec.chat_id,
            &result,
            None,
            character_id,
            &Map::new(),
            0,
            set.as_ref(),
            containers.as_ref().map(|c| &c.1),
            // P4.D329: no bag, no pending inventory map (no inventory gesture).
            None,
            None,
        ));
        let lines: Vec<String> = std::mem::take(&mut *logs.lock().unwrap());

        let mut check = |what: &str, got: String, expected: String| {
            if got != expected {
                failures.push(format!(
                    "step {} — {what}\n  rust:   {got}\n  oracle: {expected}",
                    step.name
                ));
            }
        };

        // The applied records (v4's `applied` halves, in order).
        let want_returned: Value = serde_json::from_str(&want.returned).expect("returned json");
        let want_applied: Vec<Value> = want_returned
            .as_array()
            .expect("returned array")
            .iter()
            .map(|e| e["applied"].clone())
            .collect();
        if !want_applied.is_empty() {
            applied_rows += 1;
        }
        check(
            "applied",
            serde_json::to_string(&applied).unwrap(),
            serde_json::to_string(&want_applied).unwrap(),
        );

        // The outfit after.
        let equipped = db
            .read_main(|c| {
                Ok(ChatOutfitsRepository::new(c)
                    .get_equipped_outfit_for_character(&spec.chat_id, &spec.character_id))
            })
            .expect("read the outfit")
            .unwrap_or(Value::Null);
        check(
            "equippedOutfit",
            serde_json::to_string(&equipped).unwrap(),
            serde_json::to_string(&want.equipped).unwrap(),
        );

        check(
            "wardrobe_wear_stats",
            serde_json::to_string(&dump_wear(&db)).unwrap(),
            serde_json::to_string(&want.wear).unwrap(),
        );

        let got_set = match &set {
            Some(s) => {
                let mut v: Vec<String> = s.lock().unwrap().iter().cloned().collect();
                v.sort();
                json!(v)
            }
            None => Value::Null,
        };
        check(
            "the announcement set",
            serde_json::to_string(&got_set).unwrap(),
            serde_json::to_string(&want.set).unwrap(),
        );

        check(
            "background_jobs",
            serde_json::to_string(&dump_jobs(&db)).unwrap(),
            serde_json::to_string(&want.jobs).unwrap(),
        );

        // [P4.D316] the turn's history Map and the history table.
        let got_map = match &containers {
            Some(c) => {
                let mut v: Vec<(String, Value)> =
                    c.1.lock()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
                        .collect();
                v.sort_by(|a, b| a.0.cmp(&b.0));
                json!(v)
            }
            None => Value::Null,
        };
        check(
            "the history map",
            serde_json::to_string(&got_map).unwrap(),
            serde_json::to_string(&want.map).unwrap(),
        );
        check(
            "chat_wardrobe_history",
            serde_json::to_string(&dump_history(&db)).unwrap(),
            serde_json::to_string(&want.history).unwrap(),
        );

        // The lines: the module's whole, the chokepoint's by its tail.
        let mut messages: Vec<&str> = MODULE_MESSAGES.to_vec();
        messages.push(COMMITTED);
        let got_lines: Vec<Line> = lines
            .iter()
            .filter_map(|l| rust_line(l, &messages))
            .filter(|(_, m, f)| {
                m == COMMITTED || f.iter().any(|(k, v)| k == "context" && v == MODULE_CONTEXT)
            })
            .map(|l| {
                if l.1 == COMMITTED {
                    committed_tail(l)
                } else {
                    l
                }
            })
            .collect();
        let want_lines: Vec<Line> = want
            .logs
            .iter()
            .map(oracle_line)
            .map(|l| {
                if l.1 == COMMITTED {
                    committed_tail(l)
                } else {
                    l
                }
            })
            .collect();
        module_lines += want_lines.iter().filter(|l| l.1 != COMMITTED).count();
        committed_lines += want_lines.iter().filter(|l| l.1 == COMMITTED).count();
        check("lines", format!("{got_lines:?}"), format!("{want_lines:?}"));
    }

    assert!(
        failures.is_empty(),
        "{} comparand(s) diverged:\n{}",
        failures.len(),
        failures.join("\n")
    );

    // ---- v5-only legs (the branches a real-DB oracle cannot script) --------

    // The MODEL entrance end to end (P4.D306 R-1 / Tier 1 item 12): the
    // production `BuiltInToolRunner` over ONE turn context, `run_custom` on the
    // vault's wear-effect tool and then the model's own `wardrobe_wear` — the
    // executor hands `run_custom` the SAME set the wardrobe tools borrow, so
    // the character joins it ONCE and nothing is enqueued until the
    // orchestrator's drain, which enqueues exactly one announcement. Pascal's
    // posted row carries the record: `previous` present, the five-key slots,
    // no `tier`.
    {
        let empty = spec.states["empty"].clone();
        let outfit = json!({ spec.character_id.clone(): empty }).to_string();
        let chat_id = spec.chat_id.clone();
        rt.block_on(db.write(move |w| {
            let c = w.main().connection();
            c.execute(
                r#"UPDATE "chats" SET "equippedOutfit" = ?1 WHERE "id" = ?2"#,
                rusqlite::params![outfit, chat_id],
            )?;
            c.execute(r#"DELETE FROM "background_jobs""#, [])?;
            Ok(())
        }))
        .expect("reset the outfit and the job table");

        let runner = BuiltInToolRunner::new(
            db.clone(),
            SelfInventoryEnv {
                version: String::new(),
                runtime_mode: "local-dev".to_string(),
                client_shell: ClientShell::Browser,
                mount_index_degraded: false,
                release_notes: None,
                changelog: None,
                model_info: Vec::new(),
                fallback_pricing: Vec::new(),
                registry_default_context: 8192,
            },
        );
        let mut ctx = create_tool_context(
            spec.chat_id.clone(),
            spec.user_id.clone(),
            spec.character_id.clone(),
            spec.participant_id.clone(),
            None,
            None,
            None,
            None,
            None,
        );
        ctx.character_ids = Some(vec![spec.character_id.clone()]);
        let history_before = dump_history(&db);
        let roll = rt.block_on(runner.run(
            &ToolCall {
                name: "run_custom".into(),
                arguments: json!({ "tool": "regalia" }),
                call_id: Some("call-1".into()),
            },
            &ctx,
        ));
        assert!(roll.success, "run_custom failed: {:?}", roll.error);
        let members = |ctx: &ToolExecutionContext| {
            let mut v: Vec<String> = ctx
                .pending_wardrobe_announcements
                .as_ref()
                .expect("create_tool_context mints the turn's set")
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .collect();
            v.sort();
            v
        };
        assert_eq!(
            members(&ctx),
            vec![spec.character_id.clone()],
            "joined the turn's set"
        );
        let jobs = |db: &Db| dump_jobs(db).as_array().map(Vec::len).unwrap_or(0);
        assert_eq!(jobs(&db), 0, "nothing is enqueued inside the turn");

        // The posted row's record.
        let meta: Value = db
            .read_main(|c| {
                c.query_row(
                    r#"SELECT "pascalMeta" FROM "chat_messages" WHERE "chatId" = ?1 AND "pascalMeta" IS NOT NULL ORDER BY "createdAt" DESC LIMIT 1"#,
                    [&spec.chat_id],
                    |r| r.get::<_, String>(0),
                )
                .map_err(Into::into)
            })
            .map(|t| serde_json::from_str(&t).expect("pascalMeta json"))
            .expect("Pascal posted a row");
        let cloak = spec
            .items
            .iter()
            .find(|i| i["title"] == "Cloak of Night")
            .expect("the cloak")["id"]
            .as_str()
            .unwrap()
            .to_string();
        let empty_slots = r#"{"top":[],"bottom":[],"footwear":[],"accessories":[],"hair":[]}"#;
        let worn = format!(
            r#"{{"top":["{cloak}"],"bottom":[],"footwear":[],"accessories":["{cloak}"],"hair":[]}}"#
        );
        assert_eq!(
            serde_json::to_string(&meta["effects"]).unwrap(),
            format!(
                r#"[{{"target":"wardrobe.wear","previous":{empty_slots},"next":{{"item":{{"id":"{cloak}","title":"Cloak of Night"}},"slotsAffected":["top","accessories"],"effect":"replaced","slots":{worn}}}}}]"#
            ),
            "pascalMeta.effects on Pascal's row"
        );

        let wear = rt.block_on(runner.run(
            &ToolCall {
                name: "wardrobe_wear".into(),
                arguments: json!({ "operations": [{ "item_title": "Linen Shirt" }] }),
                call_id: Some("call-2".into()),
            },
            &ctx,
        ));
        assert!(wear.success, "wardrobe_wear failed: {:?}", wear.error);
        assert_eq!(
            members(&ctx),
            vec![spec.character_id.clone()],
            "still ONE entry"
        );
        assert_eq!(jobs(&db), 0, "the tool's change joined the same set");

        // The orchestrator's drain at the end of the turn.
        // [P4.D316] inside the turn the history WAITS: Pascal's intent (the
        // rolling character + the tool's name) was REPLACED by the model's own
        // `wardrobe_wear` — the last intent wins.
        let intents = ctx
            .pending_wardrobe_history
            .as_ref()
            .expect("create_tool_context mints the turn's map")
            .lock()
            .unwrap()
            .clone();
        assert_eq!(intents.len(), 1, "{intents:?}");
        assert_eq!(
            serde_json::to_string(&intents[&spec.character_id]).unwrap(),
            format!(
                r#"{{"kind":"append","provenance":{{"source":"character","actorCharacterId":"{}"}}}}"#,
                spec.character_id
            ),
            "the model's wear replaced Pascal's intent"
        );
        assert_eq!(
            dump_history(&db),
            history_before,
            "nothing settled inside the turn"
        );
        rt.block_on(flush_pending_wardrobe_announcements(
            &db,
            &spec.user_id,
            &spec.chat_id,
            ctx.pending_wardrobe_announcements.as_ref(),
            ctx.pending_wardrobe_history.as_ref(),
        ));
        assert_eq!(jobs(&db), 1, "ONE Aurora announcement for the turn");
        assert!(members(&ctx).is_empty(), "the drain empties the set");
        let settled = dump_history(&db);
        let (before, after) = (
            history_before.as_array().unwrap().len(),
            settled.as_array().unwrap().len(),
        );
        let current: Vec<&Value> = settled
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["chatId"] == spec.chat_id.as_str() && r["isCurrent"] == 1)
            .collect();
        assert!(
            after == before + 1
                && current.len() == 1
                && current[0]["source"] == "character"
                && current[0]["actorToolName"].is_null(),
            "ONE entry for the turn, the last intent's: {settled}"
        );
    }
    let gestures = |wear: &str| {
        vec![
            json!({ "index": 0, "wardrobe": { "action": "wear", "items": [wear] } }),
            json!({ "index": 1, "wardrobe": { "action": "clear_slot", "items": [], "slot": "hair" } }),
        ]
    };
    let plant = |db: &Db, slots: Value| {
        let outfit = json!({ spec.character_id.clone(): slots }).to_string();
        let chat_id = spec.chat_id.clone();
        rt.block_on(db.write(move |w| {
            w.main()
                .connection()
                .execute(
                    r#"UPDATE "chats" SET "equippedOutfit" = ?1 WHERE "id" = ?2"#,
                    rusqlite::params![outfit, chat_id],
                )
                .map(|_| ())
                .map_err(Into::into)
        }))
        .expect("plant");
    };
    let ribbon_on = spec.states["ribbon-on"].clone();

    // v4's `disk on fire`: a commit that FAILS is dropped per item with the
    // WARN, and the run goes on. The wear ledger gone, a put-on's credit write
    // fails; the clear (a take-off — no credit) still lands.
    plant(&db, ribbon_on.clone());
    rt.block_on(db.write(|w| {
        w.main()
            .connection()
            .execute_batch(r#"DROP TABLE "wardrobe_wear_stats""#)
            .map_err(Into::into)
    }))
    .expect("drop the wear ledger");
    logs.lock().unwrap().clear();
    let applied = rt.block_on(apply_effects_for_run(
        &db,
        &spec.user_id,
        &spec.chat_id,
        &run_result(&spec.tool_name, &gestures("Linen Shirt")),
        None,
        Some(spec.character_id.clone()),
        &Map::new(),
        0,
        None,
        None,
        // P4.D329: no bag, no pending inventory map.
        None,
        None,
    ));
    let lines: Vec<String> = std::mem::take(&mut *logs.lock().unwrap());
    let targets: Vec<&str> = applied.iter().map(|a| a.target.as_str()).collect();
    assert_eq!(
        targets,
        vec!["wardrobe.clear_slot"],
        "the run continued past the failure"
    );
    let failed: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("Custom tool wardrobe effect failed; that change was dropped"))
        .collect();
    assert_eq!(failed.len(), 1, "{lines:?}");
    for field in [
        "context=pascal.wardrobe-effects",
        "effectIndex=0",
        "item=Linen Shirt",
        "error=no such table: wardrobe_wear_stats",
    ] {
        assert!(failed[0].contains(field), "missing {field}: {}", failed[0]);
    }
    assert!(
        failed[0].starts_with("WARN quilltap::pascal"),
        "{}",
        failed[0]
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Custom tool wardrobe effects applied")
                && l.contains(r#"appliedJson=["wardrobe.clear_slot"]"#)
                && l.contains("announcement=enqueued now")),
        "the after-commit DEBUG names the change that landed: {lines:?}"
    );

    // A degraded open (no mount-index writer): v5's home for v4's
    // `loadToolPool` catch — the WARN, nothing changes, no notify.
    drop(db);
    let degraded_dir = tempfile::Builder::new()
        .prefix("qt-pwe-degraded-")
        .tempdir()
        .expect("scratch");
    let degraded_main = degraded_dir.path().join("main.db");
    std::fs::copy(&fx_main, &degraded_main).expect("copy main");
    let degraded = Db::open(
        DbPaths {
            main: degraded_main,
            mount_index: None,
            llm_logs: None,
        },
        &spec.pepper,
    )
    .expect("open main alone");
    plant(&degraded, ribbon_on);
    logs.lock().unwrap().clear();
    let applied = rt.block_on(apply_effects_for_run(
        &degraded,
        &spec.user_id,
        &spec.chat_id,
        &run_result(&spec.tool_name, &gestures("Linen Shirt")),
        None,
        Some(spec.character_id.clone()),
        &Map::new(),
        0,
        None,
        None,
        // P4.D329: no bag, no pending inventory map.
        None,
        None,
    ));
    let lines: Vec<String> = std::mem::take(&mut *logs.lock().unwrap());
    assert!(applied.is_empty(), "{applied:?}");
    let unread: Vec<&String> = lines
        .iter()
        .filter(|l| {
            l.contains("Custom tool wardrobe effects skipped — the wardrobe could not be read")
        })
        .collect();
    assert_eq!(unread.len(), 1, "{lines:?}");
    assert!(
        unread[0].starts_with("WARN quilltap::pascal")
            && unread[0].contains("characterId=c1000000-0000-4000-8000-000000000001")
            && unread[0].contains("error=wardrobe effects require the mount-index database"),
        "{}",
        unread[0]
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Custom tool wardrobe effects applied")
                || l.contains("Wardrobe outfit announcement scheduled")),
        "nothing landed, so nothing is announced: {lines:?}"
    );
    // Floors: a corpus that quietly stopped applying, logging or committing
    // would otherwise pass (memory note `tier-r-vacuous-equal-rows`).
    assert!(
        applied_rows >= 12,
        "only {applied_rows} steps landed a change"
    );
    assert!(
        module_lines >= 25,
        "only {module_lines} module lines compared"
    );
    assert!(
        committed_lines >= 20,
        "only {committed_lines} chokepoint commits compared"
    );
    eprintln!(
        "pascal_wardrobe_effects_equivalence: {} steps, {applied_rows} landing a change, \
         {module_lines} module lines, {committed_lines} chokepoint commits",
        spec.steps.len()
    );
}
