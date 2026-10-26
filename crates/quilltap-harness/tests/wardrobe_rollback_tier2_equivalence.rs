//! Tier-2 differential (P4.D316 Tier 2 item 24): the ONE rollback — v4's REAL
//! `rollbackWardrobe` (`lib/wardrobe/history/rollback.ts`, `ebb480edd`, at the
//! pin `e8e17b2d9`) vs `quilltap_core::wardrobe::rollback::{rollback_in_write,
//! finish_rollback}` — driven ROUTE-FREE over seeded timelines, with the pool
//! passed both ways.
//!
//! Both sides run the SAME op list (`harness/oracle/fixtures/wardrobe-
//! rollback-tier2.json`) over ONE copy of the committed wardrobe-routes fixture
//! pair (`crates/quilltap-web/tests/fixtures/wardrobe-routes-{main,mount}.db`).
//! Setup: the project's `allowAnyCharacter` is set false through each side's
//! REAL projects repository (its roster is empty), so Aria is OFF chat1's
//! project roster — **T14**: the CHARACTER's pool (`loadToolPool` /
//! `load_tool_pool`) withholds the project tier, so the `Project scarf` is
//! `missing`, while the OPERATOR's pool (`loadWearablePool(…, { operator: true
//! })` / `load_wearable_pool` with `operator: true`) restores it.
//!
//! Per op each side clears the pair's `chat_wardrobe_history` rows and every
//! `background_jobs` row, plants the timeline through its REAL repositories
//! (`setEquippedOutfit` + `settle` append — the first dressed plant lands as
//! entry 1 behind v4's first-touch EMPTY `seeded` 0), then the optional cursor
//! / gap / live outfit, then rolls back with `source` `ui` or `tool` (`inTurn`
//! hands it a fresh pair of turn containers — the cursor waits in the Map,
//! nothing settled, nothing enqueued). Comparands per op: the outcome (the four
//! refusals' sentences; the gap's thrown text; `{previousIndex, restoredIndex,
//! slots, skipped}`), the chat's `equippedOutfit`, EVERY `wardrobe_wear_stats`
//! row minus the minted id / timestamps (the commit carries `wornBundles: []`
//! — only newly worn leaves are credited; `first/lastWornAt` collapse to
//! `<now>`), the pair's history rows in v4's column order (byte-compared), the
//! queued jobs (type / status / maxAttempts / payload parsed), the turn's Set +
//! Map, and the log lines (level, message, every field). The settle's own lines
//! run inside the writer closure where the capture cannot see them (memory note
//! `capture-rig-writer-thread`), so the compared set is the caller-thread lines
//! (the pool's, the stale-item INFO, the chokepoint's DEBUG — the in-write half
//! runs on the test thread's connection — the announcement DEBUG and `Rolled a
//! wardrobe back`).
//!
//! Generate (Node 24, from the v4 checkout or the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server   # (or the round's pin)
//!   cp $V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-main.db /tmp/qt-wrb-main.db
//!   cp $V5W/crates/quilltap-web/tests/fixtures/wardrobe-routes-mount.db /tmp/qt-wrb-mount.db
//!   QT_FIXTURE_WRB_MAIN=/tmp/qt-wrb-main.db QT_FIXTURE_WRB_MOUNT=/tmp/qt-wrb-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/wardrobe-rollback-tier2.ts \
//!     > /tmp/qt-wrb-oracle.ndjson
//! Run:
//!   QT_ORACLE_WRB=/tmp/qt-wrb-oracle.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_rollback_tier2_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};

use quilltap_core::db::chat_wardrobe_history::{
    ChatWardrobeHistoryRepository, WardrobeHistoryIntent, WardrobeHistoryProvenance,
    WardrobeHistorySource,
};
use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::projects::ProjectsRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::wardrobe_wear_stats::EquipSource;
use quilltap_core::db::Writer;
use quilltap_core::services::outfit_change_effects::{
    new_wardrobe_turn_containers, OutfitChangeContext,
};
use quilltap_core::test_support::global_capture;
use quilltap_core::tools::wardrobe_tool_table::load_tool_pool;
use quilltap_core::wardrobe::rollback::{
    finish_rollback, rollback_in_write, RollbackInput, RollbackOutcome, CONTEXT,
};
use quilltap_core::wearable_pool::{load_wearable_pool, LoadWearablePoolOptions};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Base {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    ids: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "operatorProvenance")]
    operator_provenance: Value,
    #[serde(rename = "toolProvenance")]
    tool_provenance: Value,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Plant {
    slots: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    provenance: Option<Value>,
}

#[derive(Deserialize)]
struct Rollback {
    #[serde(default)]
    index: Option<i64>,
    source: String,
    pool: String,
    #[serde(default, rename = "inTurn")]
    in_turn: bool,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    chat: String,
    who: String,
    plants: Vec<Plant>,
    #[serde(default)]
    cursor: Option<i64>,
    #[serde(default, rename = "dropIndex")]
    drop_index: Option<i64>,
    #[serde(default)]
    live: Option<BTreeMap<String, Vec<String>>>,
    rollback: Rollback,
}

/// The messages this family compares (the oracle captures the same set).
const CAPTURED: [&str; 9] = [
    "Rollback left a stale item off",
    "Rolled a wardrobe back",
    "Committed equipped outfit",
    "Equipped outfit write failed; no wears credited",
    "Wardrobe outfit announcement scheduled",
    "Failed to schedule wardrobe outfit announcement",
    "Failed to settle wardrobe history",
    "[WearablePool] Character off project roster — project wardrobe withheld",
    "[ProjectRoster] Tool access check",
];

type LogLine = (String, String, BTreeMap<String, String>);

/// A captured v5 line → `(level, message, fields)`. The rig renders
/// `LEVEL target message k=v k=v…`; the message is one of [`CAPTURED`].
fn parse_line(line: &str) -> Option<LogLine> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let message = CAPTURED.iter().find(|m| rest.starts_with(**m))?;
    let tail = rest[message.len()..].trim_start();
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i == 0 || bytes[i - 1] == b' ' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i && j < bytes.len() && bytes[j] == b'=' {
                keys.push((i, j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let mut fields = BTreeMap::new();
    for (n, (start, eq)) in keys.iter().enumerate() {
        let end = keys
            .get(n + 1)
            .map(|(next, _)| next - 1)
            .unwrap_or(tail.len());
        let key = &tail[*start..*eq];
        let key = key.strip_suffix("Json").unwrap_or(key);
        fields.insert(key.to_string(), tail[eq + 1..end].to_string());
    }
    Some((level.to_lowercase(), message.to_string(), fields))
}

/// v4's captured line → the same shape.
fn oracle_line(v: &Value) -> LogLine {
    let fields = v["fields"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, val)| {
            let rendered = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), rendered)
        })
        .collect();
    (
        v["level"].as_str().unwrap().to_string(),
        v["message"].as_str().unwrap().to_string(),
        fields,
    )
}

fn provenance_of(p: &Value, id: &dyn Fn(&str) -> String) -> WardrobeHistoryProvenance {
    let source: WardrobeHistorySource = serde_json::from_value(p["source"].clone()).unwrap();
    let mut prov = WardrobeHistoryProvenance::new(source);
    if let Some(a) = p["actorCharacterId"].as_str() {
        prov = prov.with_actor(id(a));
    }
    if let Some(t) = p["toolName"].as_str() {
        prov = prov.with_tool_name(t);
    }
    prov
}

fn full_slots(s: Option<&BTreeMap<String, Vec<String>>>, id: &dyn Fn(&str) -> String) -> Value {
    let mut out = json!({ "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": [] });
    if let Some(s) = s {
        for (slot, ids) in s {
            out[slot] = json!(ids.iter().map(|k| id(k)).collect::<Vec<_>>());
        }
    }
    out
}

fn ledger(main: &rusqlite::Connection) -> Value {
    let mut st = main
        .prepare(
            "SELECT \"itemId\", \"wearerCharacterId\", \"wearCount\", \"firstWornAt\", \"lastWornAt\", \"lastWornChatId\"
               FROM \"wardrobe_wear_stats\" ORDER BY \"itemId\", COALESCE(\"wearerCharacterId\", '')",
        )
        .unwrap();
    let rows: Vec<Value> = st
        .query_map([], |r| {
            Ok(json!({
                "itemId": r.get::<_, String>(0)?,
                "wearerCharacterId": r.get::<_, Option<String>>(1)?,
                "wearCount": r.get::<_, i64>(2)?,
                "firstWornAt": "<now>",
                "lastWornAt": "<now>",
                "lastWornChatId": r.get::<_, Option<String>>(5)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

fn history(c: &rusqlite::Connection, chat_id: &str, character_id: &str) -> Value {
    let mut stmt = c
        .prepare(
            r#"SELECT "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent" FROM chat_wardrobe_history WHERE "chatId" = ?1 AND "characterId" = ?2 ORDER BY "entryIndex""#,
        )
        .unwrap();
    let rows = stmt
        .query_map([chat_id, character_id], |r| {
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
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    Value::Array(rows)
}

fn jobs(c: &rusqlite::Connection) -> Value {
    let mut stmt = c
        .prepare(r#"SELECT "type", "status", "maxAttempts", "payload" FROM "background_jobs" ORDER BY rowid"#)
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "type": r.get::<_, String>(0)?,
                "status": r.get::<_, String>(1)?,
                // The routes pair's `background_jobs` predates the integer
                // column: REAL affinity stores v4's `3` as `3.0`, which
                // better-sqlite3 hands the oracle back as the number `3`.
                "maxAttempts": r.get::<_, f64>(2)? as i64,
                "payload": r.get::<_, String>(3)?,
            }))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    Value::Array(rows)
}

/// Jobs with their payload parsed (the two sides serialise one object; key
/// order is compared by the serialised value of the parsed object).
fn parsed_jobs(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .unwrap()
            .iter()
            .map(|j| {
                let mut j = j.clone();
                if let Some(p) = j["payload"].as_str() {
                    j["payload"] = serde_json::from_str(p).unwrap();
                }
                j
            })
            .collect(),
    )
}

/// Byte compare (memory note `serde-json-map-eq-ignores-order`).
fn same(a: &Value, b: &Value) -> bool {
    serde_json::to_string(a).unwrap() == serde_json::to_string(b).unwrap()
}

#[test]
fn wardrobe_rollback_matches_oracle() {
    global_capture::install();
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WRB") else {
        eprintln!("SKIP: set QT_ORACLE_WRB (see header).");
        return;
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = root.join("harness/oracle/fixtures");
    let base: Base = serde_json::from_str(
        &std::fs::read_to_string(fixtures.join("wardrobe-routes.json")).unwrap(),
    )
    .unwrap();
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures.join("wardrobe-rollback-tier2.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        oracle.len(),
        spec.ops.len(),
        "the oracle has {} rows for {} ops — regenerate it",
        oracle.len(),
        spec.ops.len()
    );
    let id = |k: &str| -> String { base.ids.get(k).cloned().unwrap_or_else(|| k.to_string()) };

    let pair = root.join("crates/quilltap-web/tests/fixtures");
    let scratch = tempfile::tempdir().unwrap();
    let work_main = scratch.path().join("wrb-main.db");
    let work_mount = scratch.path().join("wrb-mount.db");
    std::fs::copy(pair.join("wardrobe-routes-main.db"), &work_main).unwrap();
    std::fs::copy(pair.join("wardrobe-routes-mount.db"), &work_mount).unwrap();
    let pepper = &base.test_pepper_base64;
    let main = Writer::open_writable(&work_main, pepper).expect("open main");
    let mount = Writer::open_writable(&work_mount, pepper).expect("open mount");
    let (main_c, mount_c) = (main.connection(), mount.connection());
    // P4.D315's history plant; the routes pair predates the wear ledger too.
    quilltap_core::test_support::ensure_chat_wardrobe_history_on(main_c);
    quilltap_core::test_support::ensure_wear_ledger_on(main_c);
    // T14's setup: chat1's project closes its (empty) roster.
    let closed = ProjectsRepository::new(main_c, mount_c)
        .set_allow_any_character(&id("project"), false)
        .expect("close the roster")
        .expect("the project");
    assert_eq!(
        closed["allowAnyCharacter"],
        json!(false),
        "setup: the roster did not close"
    );
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed: Vec<String> = Vec::new();
    let mut lines_compared = 0;
    for (op, want) in spec.ops.iter().zip(&oracle) {
        assert_eq!(want["name"], json!(op.name), "oracle row order");
        let (chat_id, character_id) = (id(&op.chat), id(&op.who));
        main_c
            .execute(
                r#"DELETE FROM chat_wardrobe_history WHERE "chatId" = ?1 AND "characterId" = ?2"#,
                [&chat_id, &character_id],
            )
            .unwrap();
        main_c
            .execute(r#"DELETE FROM "background_jobs""#, [])
            .unwrap();
        let outfits = ChatOutfitsRepository::new(main_c);
        let history_repo = ChatWardrobeHistoryRepository::new(main_c);
        for plant in &op.plants {
            outfits
                .set_equipped_outfit(
                    &chat_id,
                    &character_id,
                    &full_slots(Some(&plant.slots), &id),
                )
                .expect("plant slots");
            let p = plant.provenance.as_ref().unwrap_or(&spec.tool_provenance);
            history_repo.settle(
                &chat_id,
                &character_id,
                &WardrobeHistoryIntent::Append {
                    provenance: provenance_of(p, &id),
                },
            );
        }
        if let Some(index) = op.cursor {
            history_repo.settle(
                &chat_id,
                &character_id,
                &WardrobeHistoryIntent::Cursor { index },
            );
        }
        if let Some(index) = op.drop_index {
            main_c
                .execute(
                    r#"DELETE FROM chat_wardrobe_history WHERE "chatId" = ?1 AND "characterId" = ?2 AND "entryIndex" = ?3"#,
                    rusqlite::params![chat_id, character_id, index],
                )
                .unwrap();
        }
        if let Some(live) = &op.live {
            outfits
                .set_equipped_outfit(&chat_id, &character_id, &full_slots(Some(live), &id))
                .expect("plant the live outfit");
        }

        let rb = &op.rollback;
        let (source, prov_spec) = match rb.source.as_str() {
            "ui" => (EquipSource::Ui, &spec.operator_provenance),
            "tool" => (EquipSource::Tool, &spec.tool_provenance),
            other => panic!("unknown source {other}"),
        };
        let provenance = provenance_of(prov_spec, &id);
        let turn = rb.in_turn.then(new_wardrobe_turn_containers);
        let (outcome, lines) = global_capture::capture(|| {
            let pool = match rb.pool.as_str() {
                "tool" => load_tool_pool(main_c, mount_c, &chat_id, &character_id),
                "operator" => load_wearable_pool(
                    main_c,
                    mount_c,
                    &character_id,
                    None,
                    LoadWearablePoolOptions {
                        operator: true,
                        chat_id: Some(&chat_id),
                        ..LoadWearablePoolOptions::default()
                    },
                ),
                other => panic!("unknown pool {other}"),
            };
            let staged = rollback_in_write(
                main_c,
                RollbackInput {
                    chat_id: &chat_id,
                    character_id: &character_id,
                    pool: &pool,
                    index: rb.index,
                    source,
                    source_context: CONTEXT,
                },
            );
            match staged {
                Ok(RollbackOutcome::Rolled(stage)) => {
                    rt.block_on(finish_rollback(
                        &db,
                        &OutfitChangeContext {
                            user_id: &base.user_id,
                            chat_id: &chat_id,
                            character_id: &character_id,
                            provenance: &provenance,
                            history_intent: None,
                            pending_wardrobe_announcements: turn.as_ref().map(|t| &t.0),
                            pending_wardrobe_history: turn.as_ref().map(|t| &t.1),
                        },
                        &stage,
                        CONTEXT,
                    ));
                    json!({
                        "ok": true,
                        "previousIndex": stage.previous_index,
                        "restoredIndex": stage.restored_index,
                        "slots": stage.slots.to_value(),
                        "skipped": stage.skipped.iter().map(|s| s.to_route_value()).collect::<Vec<_>>(),
                    })
                }
                Ok(RollbackOutcome::Refused(r)) => json!({
                    "ok": false,
                    "reason": r.reason.as_str(),
                    "message": r.message,
                }),
                Err(e) => json!({ "thrown": quilltap_core::db::fallback::error_text(&e) }),
            }
        });

        let mut got = json!({
            "outcome": outcome,
            "equippedOutfit": ChatOutfitsRepository::new(main_c)
                .get_equipped_outfit(&chat_id)
                .unwrap_or(Value::Null),
            "ledger": ledger(main_c),
            "history": history(main_c, &chat_id, &character_id),
            "jobs": parsed_jobs(&jobs(main_c)),
            "set": Value::Null,
            "map": Value::Null,
        });
        if let Some((set, map, _)) = &turn {
            let mut s: Vec<String> = set.lock().unwrap().iter().cloned().collect();
            s.sort();
            let mut m: Vec<(String, Value)> = map
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
                .collect();
            m.sort_by(|a, b| a.0.cmp(&b.0));
            got["set"] = json!(s);
            got["map"] = json!(m);
        }
        let mut want_cmp = json!({
            "outcome": want["outcome"],
            "equippedOutfit": want["equippedOutfit"],
            "ledger": want["ledger"].as_array().unwrap().iter().map(|r| {
                let mut r = r.clone();
                r["firstWornAt"] = json!("<now>");
                r["lastWornAt"] = json!("<now>");
                r
            }).collect::<Vec<_>>(),
            "history": want["history"],
            "jobs": parsed_jobs(&want["jobs"]),
            "set": want["set"],
            "map": want["map"],
        });
        // v4 returns the slots object it wrote — all five keys on both sides.
        if want_cmp["outcome"].get("slots").is_some() {
            let s = quilltap_core::wardrobe::Slots::from_value(Some(&want_cmp["outcome"]["slots"]));
            want_cmp["outcome"]["slots"] = s.to_value();
        }
        for key in [
            "outcome",
            "equippedOutfit",
            "ledger",
            "history",
            "jobs",
            "set",
            "map",
        ] {
            if !same(&got[key], &want_cmp[key]) {
                eprintln!(
                    "[{}] {key}\n   v5: {}\n   v4: {}",
                    op.name, got[key], want_cmp[key]
                );
                failed.push(format!("{}:{key}", op.name));
            }
        }

        let mut got_lines: Vec<LogLine> = lines.iter().filter_map(|l| parse_line(l)).collect();
        let mut want_lines: Vec<LogLine> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(oracle_line)
            .collect();
        got_lines.sort();
        want_lines.sort();
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            eprintln!(
                "[{}] LOGS\n   v5: {got_lines:#?}\n   v4: {want_lines:#?}",
                op.name
            );
            failed.push(format!("{}:logs", op.name));
        }
        if !failed
            .iter()
            .any(|f| f.starts_with(&format!("{}:", op.name)))
        {
            eprintln!("[{}] OK.", op.name);
        }
    }
    assert!(failed.is_empty(), "wardrobe-rollback FAILED: {failed:?}");

    // Shape assertions — every arm the corpus was written for is reached in
    // v4's own answers, so no row can go vacuous if a plant stops firing.
    let row = |name: &str| {
        oracle
            .iter()
            .find(|r| r["name"] == name)
            .unwrap_or_else(|| panic!("the {name} row"))
    };
    let scarf = id("pScarf");
    // T14: the tool path reports the project scarf `missing`; the operator
    // path restores it from the SAME timeline.
    assert_eq!(
        row("tool_pool_project_item_missing_t14")["outcome"]["skipped"],
        json!([{ "itemId": scarf, "title": null, "reason": "missing" }])
    );
    assert_eq!(
        row("operator_pool_project_item_restored_t14")["outcome"]["slots"]["accessories"],
        json!([scarf])
    );
    assert!(row("tool_archived_item_left_off")["outcome"]["skipped"][0]["reason"] == "archived");
    assert_eq!(
        row("ui_archived_item_still_worn_kept")["outcome"]["slots"]["bottom"],
        json!([id("gArch")])
    );
    for (name, reason) in [
        ("refuse_no_history", "no_history"),
        ("refuse_at_first", "at_first"),
        ("refuse_out_of_range", "out_of_range"),
        ("refuse_already_current", "already_current"),
    ] {
        let r = row(name);
        assert_eq!(r["outcome"]["reason"], json!(reason), "{r}");
        assert!(r["jobs"].as_array().unwrap().is_empty(), "{r}");
    }
    assert_eq!(
        row("gap_at_target_throws")["outcome"]["thrown"],
        json!("Cannot read properties of undefined (reading 'slots')")
    );
    // In a turn: the cursor waits in the Map; nothing enqueued, table untouched.
    let in_turn = row("tool_in_turn_cursor_deferred");
    assert_eq!(
        in_turn["map"][0][1],
        json!({ "kind": "cursor", "index": 1 })
    );
    assert!(in_turn["jobs"].as_array().unwrap().is_empty());
    // Outside a turn: the cursor moved and the announcement was enqueued.
    let outside = row("ui_step_back_operator_pool");
    assert_eq!(outside["history"][1]["isCurrent"], json!(1));
    assert_eq!(outside["jobs"].as_array().unwrap().len(), 1);
    assert!(lines_compared >= 40, "log lines compared: {lines_compared}");
    eprintln!(
        "OK: wardrobe-rollback matched oracle ({} ops; {lines_compared} log lines).",
        spec.ops.len()
    );
}
