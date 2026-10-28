//! Tier-3 differential: the outfit-change side effects (P4.D279 keystone C1 §7;
//! v4 `lib/wardrobe/outfit-change-effects.ts`, NEW at `4a38222e6` — the module
//! did not exist on main before this lane, so the red-first measurement is the
//! module's absence: `services::outfit_change_effects` was an empty stub).
//!
//! Both sides run the SAME scripted matrix (`outfit-change-effects-tier3.json`,
//! twenty steps over one shared per-turn set) against a copy of the fixture
//! `build-outfit-change-effects-fixture.ts` bakes. v4 drives its REAL module
//! (real `triggerAvatarGenerationIfEnabled`, real
//! `enqueueWardrobeOutfitAnnouncement`) under tsx; v5 drives
//! [`quilltap_core::services::outfit_change_effects`] over the same copy.
//! Compared per step: the turn set's contents (sorted — v4's `Set` and v5's
//! `HashSet` carry the same members), the whole `background_jobs` table
//! projected to the columns both sides decide (`type`, `status`, `userId`,
//! `priority`, `maxAttempts`, `attempts`, `payload`; minted ids and clock
//! timestamps are not dumped; `null` once the table is dropped), and the
//! module's OWN two logger lines — `Wardrobe outfit announcement scheduled`
//! (DEBUG) and `Failed to schedule wardrobe outfit announcement` (WARN) — with
//! every field (`context`, `chatId`, `characterId`, `error`). The catch leg is
//! reached through REAL code on both sides: the matrix drops `background_jobs`
//! and every later schedule fails with SQLite's own `no such table:
//! background_jobs` — v4's `backgroundJobs.create` throws it, v5's
//! `find_pending_for_chat` answers it, and the bare message is byte-equal.
//!
//! `status` IS compared here (unlike the avatar-trigger family): the oracle runs
//! with v4's job host flagged `shuttingDown`, so nothing claims a job and both
//! sides leave every row `PENDING`.
//!
//! RECORDED DIVERGENCE (the W4.1d2 decision, restated in the module doc): v4
//! drains its `Set` in insertion order; v5 sorts the drained `HashSet`. The
//! matrix inserts B before A so the two `scheduled` DEBUG lines of
//! `flush_two_ids` come out in opposite orders — each step's lines are compared
//! as a SORTED list, and the oracle's own order for that step is asserted to be
//! the insertion order, so the divergence stays measured rather than hidden.
//!
//! Lines of the layers beneath the module (`[WardrobeAnnouncement] …`,
//! `Background job enqueued`, the trigger's `Avatar image profile resolved`, the
//! repository fallback ERRORs) are not compared here — the avatar-trigger and
//! queue families own them. One of them is a known seam difference: on a
//! missing table v4's `findPendingForChat` is a fallback read (ERROR + `[]`,
//! then `create` throws) where v5's `enqueue_wardrobe_outfit_announcement`
//! returns the read's `Err` directly; the module's WARN is the same either way.
//!
//! P4.D316 (outfit-history RECORDING, v4 `05005c562` + `4ae20977f`, re-aimed at
//! the pin `e8e17b2d9`): every step also compares the turn's history MAP
//! (sorted, each intent byte-compared as v4's object JSON) and the whole
//! `chat_wardrobe_history` table (ids / createdAt not dumped), planted on each
//! side's per-case copy through P4.D315's helpers. The red-first measurement:
//! the UNCHANGED case regenerated at the pin turned 8 of 22 steps red — its
//! `record` op passed `(ctx, sourceContext)` where the pin wants `(repos, ctx,
//! sourceContext)`, so v4 read the ctx as `repos` and the turn set never
//! gained B. The captured allowlist grows to four: the settle guard's WARN
//! (asserted ABSENT on both sides — unreachable through v4's real repository,
//! pinned v5-side by the module's unit test) and the operator resolver's DEBUG
//! on the `equip` rows (a Speaking-As seat, an impersonated seat, no seat).
//! RECORDED (R-3): v5's sorted drain also orders the per-character settles; no
//! row differs.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout / an e8e17b2d9 pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUTFIT_EFFECTS_MAIN=/tmp/qt-outfit-effects-main.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-outfit-change-effects-fixture.ts
//!   QT_FIXTURE_OUTFIT_EFFECTS_MAIN=/tmp/qt-outfit-effects-main.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/outfit-change-effects-tier3.ts \
//!     > /tmp/oracle-outfit-change-effects.ndjson
//! Run:
//!   QT_ORACLE_OUTFIT_EFFECTS=/tmp/oracle-outfit-change-effects.ndjson \
//!   QT_FIXTURE_OUTFIT_EFFECTS_MAIN=/tmp/qt-outfit-effects-main.db \
//!     cargo test -p quilltap-harness --test outfit_change_effects_tier3_equivalence -- --nocapture

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::db::chat_wardrobe_history::{
    WardrobeHistoryIntent, WardrobeHistoryProvenance, WardrobeHistorySource,
};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::outfit_change_effects::{
    flush_pending_wardrobe_announcements, new_wardrobe_turn_containers, notify_wardrobe_changed,
    record_pending_wardrobe_announcement, refresh_avatar_for_outfit,
    schedule_wardrobe_announcement, OutfitChangeContext,
};
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    pepper: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "characterAId")]
    character_a: String,
    #[serde(rename = "characterBId")]
    character_b: String,
    chats: BTreeMap<String, ChatSpec>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
struct ChatSpec {
    id: String,
}

#[derive(Deserialize)]
struct Step {
    name: String,
    op: String,
    chat: Option<String>,
    character: Option<String>,
    #[serde(default, rename = "withSet")]
    with_set: bool,
    /// P4.D316: `false` passes the Set WITHOUT its history Map.
    #[serde(default, rename = "withMap")]
    with_map: Option<bool>,
    #[serde(default)]
    provenance: Option<Value>,
    #[serde(default, rename = "historyIntent")]
    history_intent: Option<Value>,
    /// `dress` steps: the slots object written as-is.
    #[serde(default)]
    slots: Option<Value>,
    context: Option<String>,
    /// `equip` steps (P4.D282): the route body minus `characterId`.
    #[serde(default)]
    body: Option<Value>,
}

#[derive(Deserialize)]
struct OracleRow {
    step: String,
    set: Vec<String>,
    map: Value,
    history: Value,
    jobs: Value,
    logs: Vec<Value>,
    /// The route's status, on `equip` rows only.
    #[serde(default)]
    status: Option<u16>,
}

/// The module's own messages — the only lines this family compares.
const CAPTURED: [&str; 4] = [
    "Wardrobe outfit announcement scheduled",
    "Failed to schedule wardrobe outfit announcement",
    "Failed to settle wardrobe history",
    "Resolved operator for an outfit change",
];

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn load_spec() -> Spec {
    serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("outfit-change-effects-tier3.json"))
            .expect("spec"),
    )
    .expect("parse spec")
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

/// A fixture copy in a scratch dir; dropping the returned `TempDir` (after the
/// `Db`) removes it, `-journal` files and all. Main only: the module never
/// touches the mount index.
fn open_copy(main: &str, pepper: &str) -> (Db, tempfile::TempDir) {
    let scratch = tempfile::Builder::new()
        .prefix("qt-outfit-effects-")
        .tempdir()
        .expect("scratch");
    let m = scratch.path().join("main.db");
    std::fs::copy(main, &m).expect("copy main");
    let db = Db::open(
        DbPaths {
            main: m,
            mount_index: None,
            llm_logs: None,
        },
        pepper,
    )
    .expect("open main");
    // P4.D315's one v5-side plant, on this per-case copy (R-S).
    db.write_blocking(|w| {
        quilltap_core::test_support::ensure_chat_wardrobe_history_on(w.main().connection());
        Ok(())
    })
    .expect("plant chat_wardrobe_history");
    (db, scratch)
}

/// v4's provenance object → the keystone's type (its keys are v4's `?:`).
fn provenance_of(v: &Value) -> WardrobeHistoryProvenance {
    let source = match v["source"].as_str().expect("provenance.source") {
        "operator" => WardrobeHistorySource::Operator,
        "character" => WardrobeHistorySource::Character,
        "pascal" => WardrobeHistorySource::Pascal,
        other => panic!("a runtime provenance never names {other}"),
    };
    let mut p = WardrobeHistoryProvenance::new(source);
    if let Some(a) = v["actorCharacterId"].as_str() {
        p = p.with_actor(a);
    }
    if let Some(t) = v["toolName"].as_str() {
        p = p.with_tool_name(t);
    }
    p
}

/// v4's intent object → the keystone's type.
fn intent_of(v: &Value) -> WardrobeHistoryIntent {
    match v["kind"].as_str().expect("intent.kind") {
        "cursor" => WardrobeHistoryIntent::Cursor {
            index: v["index"].as_i64().expect("cursor index"),
        },
        "append" => WardrobeHistoryIntent::Append {
            provenance: provenance_of(&v["provenance"]),
        },
        other => panic!("unknown intent kind {other}"),
    }
}

/// The history table exactly as the oracle's `dumpHistory` reads it.
fn dump_history(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            "SELECT \"chatId\", \"characterId\", \"entryIndex\", \"slots\", \"slotsHash\", \"source\", \"actorCharacterId\", \"actorToolName\", \"isCurrent\" FROM chat_wardrobe_history ORDER BY \"chatId\", \"characterId\", \"entryIndex\"",
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

/// The `background_jobs` table projected and sorted exactly as the oracle's
/// `dumpJobs` does: `type` then the payload's JSON text; `null` once the table
/// is gone.
fn dump_jobs(db: &Db, gone: bool) -> Value {
    if gone {
        return Value::Null;
    }
    let dump = db
        .read_main(|c| quilltap_core::db::dump_table_json_conn(c, "background_jobs", "id"))
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

type Line = (String, String, BTreeMap<String, String>);

/// v4's captured line → `(level, message, fields)`, field values as the rig
/// renders them.
fn oracle_line(v: &Value) -> Line {
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
        v["level"].as_str().unwrap().to_lowercase(),
        v["message"].as_str().unwrap().to_string(),
        fields,
    )
}

/// A rig line (`LEVEL target message k=v …`) → the same shape; only the
/// module's two messages are kept. Field VALUES may carry spaces (the `error`
/// text), so a field runs up to the next ` <ident>=`.
fn rust_line(line: &str) -> Option<Line> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start();
    let message = CAPTURED.iter().find(|m| rest.starts_with(**m))?;
    let mut tail = rest[message.len()..].trim_start();
    let mut fields = BTreeMap::new();
    while !tail.is_empty() {
        let (key, after) = tail.split_once('=')?;
        // The value ends where the next ` <ident>=` begins, or at the end.
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
        fields.insert(key.to_string(), after[..end].to_string());
        tail = after[end..].trim_start();
    }
    Some((level.to_lowercase(), message.to_string(), fields))
}

#[test]
fn outfit_change_effects_match_oracle() {
    global_capture::install();
    let (Some(oracle_path), Some(fx_main)) = (
        env_or_skip("QT_ORACLE_OUTFIT_EFFECTS"),
        env_or_skip("QT_FIXTURE_OUTFIT_EFFECTS_MAIN"),
    ) else {
        return;
    };
    let spec = load_spec();

    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let oracle: Vec<OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert_eq!(oracle.len(), spec.steps.len(), "oracle step count");

    let (db, scratch) = open_copy(&fx_main, &spec.pepper);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let character_of = |c: &Option<String>| -> String {
        match c.as_deref() {
            Some("A") => spec.character_a.clone(),
            Some("B") => spec.character_b.clone(),
            other => panic!("step needs a character, got {other:?}"),
        }
    };
    let chat_of = |c: &Option<String>| -> String {
        let key = c.as_deref().expect("step needs a chat");
        spec.chats
            .get(key)
            .unwrap_or_else(|| panic!("unknown chat {key}"))
            .id
            .clone()
    };

    // The one shared pair of turn containers (v4's `newWardrobeTurnContainers()`).
    let (turn_set, turn_map, _) = new_wardrobe_turn_containers();
    let mut jobs_gone = false;
    let mut failures = Vec::new();
    let mut compared_lines = 0usize;
    for (step, want) in spec.steps.iter().zip(&oracle) {
        assert_eq!(step.name, want.step, "oracle step order");
        let context: &'static str = Box::leak(step.context.clone().unwrap_or_default().into());
        let (status, lines) = rt.block_on(global_capture::capture_async(async {
            match step.op.as_str() {
                "equip" => {
                    // P4.D282 — the chat outfit route (`chat_equip`): its
                    // effects carry no turn set, so the announcement goes at once.
                    let mut body = serde_json::Map::new();
                    body.insert("characterId".into(), json!(character_of(&step.character)));
                    if let Some(Value::Object(rest)) = &step.body {
                        body.extend(rest.clone());
                    }
                    let r = quilltap_core::api::chat_outfits::chat_equip(
                        &db,
                        &spec.user_id,
                        &chat_of(&step.chat),
                        Value::Object(body),
                    )
                    .await;
                    return Some(match r {
                        quilltap_core::api::types::Response::ChatOutfit(_) => 200,
                        other => panic!("{}: the route failed: {other:?}", step.name),
                    });
                }
                "notify" | "record" => {
                    let (chat, character) = (chat_of(&step.chat), character_of(&step.character));
                    let provenance =
                        step.provenance
                            .as_ref()
                            .map(provenance_of)
                            .unwrap_or_else(|| {
                                WardrobeHistoryProvenance::new(WardrobeHistorySource::Character)
                                    .with_actor(character.clone())
                            });
                    let intent = step.history_intent.as_ref().map(intent_of);
                    let with_map = step.with_set && step.with_map != Some(false);
                    let ctx = OutfitChangeContext {
                        user_id: &spec.user_id,
                        chat_id: &chat,
                        character_id: &character,
                        provenance: &provenance,
                        history_intent: intent.as_ref(),
                        pending_wardrobe_announcements: step.with_set.then_some(&turn_set),
                        pending_wardrobe_history: with_map.then_some(&turn_map),
                    };
                    if step.op == "notify" {
                        notify_wardrobe_changed(&db, &ctx, context).await;
                    } else {
                        record_pending_wardrobe_announcement(&db, &ctx, context).await;
                    }
                }
                "dress" => {
                    let (chat, character) = (chat_of(&step.chat), character_of(&step.character));
                    let slots = step.slots.clone().expect("dress needs slots");
                    db.write(move |w| {
                        quilltap_core::db::chats_outfits::ChatOutfitsRepository::new(
                            w.main().connection(),
                        )
                        .set_equipped_outfit(&chat, &character, &slots)
                        .map(|_| ())
                    })
                    .await
                    .expect("dress");
                }
                "seedMap" => {
                    turn_map.lock().unwrap().insert(
                        character_of(&step.character),
                        intent_of(step.history_intent.as_ref().expect("seedMap intent")),
                    );
                }
                "clearMap" => turn_map.lock().unwrap().clear(),
                "refresh" => {
                    refresh_avatar_for_outfit(
                        &db,
                        &spec.user_id,
                        &chat_of(&step.chat),
                        &character_of(&step.character),
                        context,
                    )
                    .await;
                }
                "schedule" => {
                    schedule_wardrobe_announcement(
                        &db,
                        context,
                        &spec.user_id,
                        &chat_of(&step.chat),
                        &character_of(&step.character),
                    )
                    .await;
                }
                "flush" => {
                    flush_pending_wardrobe_announcements(
                        &db,
                        &spec.user_id,
                        &chat_of(&step.chat),
                        Some(&turn_set),
                        Some(&turn_map),
                    )
                    .await;
                }
                "flushNoSet" => {
                    flush_pending_wardrobe_announcements(
                        &db,
                        &spec.user_id,
                        &chat_of(&step.chat),
                        None,
                        None,
                    )
                    .await;
                }
                "dropBackgroundJobs" => {
                    db.write(|w| {
                        w.main()
                            .connection()
                            .execute_batch("DROP TABLE background_jobs;")
                            .map_err(quilltap_core::db::DbError::from)
                    })
                    .await
                    .expect("drop background_jobs");
                    jobs_gone = true;
                }
                other => panic!("unknown op {other}"),
            }
            None
        }));

        let mut got_set: Vec<String> = turn_set.lock().unwrap().iter().cloned().collect();
        got_set.sort();
        let got_jobs = dump_jobs(&db, jobs_gone);
        let mut got_map: Vec<(String, Value)> = turn_map
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
            .collect();
        got_map.sort_by(|a, b| a.0.cmp(&b.0));
        let got_map = serde_json::to_string(&got_map).unwrap();
        let want_map = serde_json::to_string(&want.map).unwrap();
        let got_history = dump_history(&db);
        let mut got_lines: Vec<Line> = lines.iter().filter_map(|l| rust_line(l)).collect();
        let mut want_lines: Vec<Line> = want.logs.iter().map(oracle_line).collect();
        // The recorded divergence: v4 drains in insertion order, v5 sorted.
        // Compared as sorted lists; `flush_two_ids` below pins v4's own order.
        got_lines.sort();
        want_lines.sort();
        compared_lines += want_lines.len();

        let ok_set = got_set == want.set;
        let ok_jobs = got_jobs == want.jobs;
        let ok_lines = got_lines == want_lines && status == want.status;
        // Byte-compared (serde_json's Map `==` ignores key order).
        let ok_map = got_map == want_map;
        let ok_history = serde_json::to_string(&got_history).unwrap()
            == serde_json::to_string(&want.history).unwrap();
        if !(ok_set && ok_jobs && ok_lines && ok_map && ok_history) {
            failures.push(format!(
                "{}: set {ok_set} jobs {ok_jobs} lines {ok_lines} map {ok_map} history {ok_history}\n  got set   {got_set:?}\n want set   {:?}\n  got map   {got_map}\n want map   {want_map}\n  got hist  {got_history}\n want hist  {}\n  got jobs  {got_jobs}\n want jobs  {}\n  got lines {got_lines:?}\n want lines {want_lines:?}",
                step.name, want.set, want.history, want.jobs
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));

    // Exercised-count guards: the matrix reached every leg it was written for.
    assert!(
        compared_lines >= 10,
        "the matrix compared only {compared_lines} module lines"
    );
    let warns: Vec<&Value> = oracle
        .iter()
        .flat_map(|r| r.logs.iter())
        .filter(|l| l["level"] == "warn")
        .collect();
    assert_eq!(warns.len(), 4, "four steps take the catch leg: {warns:?}");
    // P4.D316 — the settle guard never fires through v4's real repository.
    assert!(
        oracle
            .iter()
            .flat_map(|r| r.logs.iter())
            .all(|l| l["message"] != "Failed to settle wardrobe history"),
        "the settle guard's WARN is v5-only reachable"
    );
    // The recording reached each provenance the matrix was written for: the
    // caller's `character`, Pascal's rolling character + tool, the operator
    // speaking as U, impersonating B, and bare — and a cursor move back.
    let last_history = oracle.last().unwrap().history.as_array().unwrap().clone();
    let has = |chat: &str, source: &str, actor: Value, tool: Value| {
        last_history.iter().any(|r| {
            r["chatId"] == spec.chats[chat].id.as_str()
                && r["source"] == source
                && r["actorCharacterId"] == actor
                && r["actorToolName"] == tool
        })
    };
    assert!(has(
        "hist",
        "character",
        json!(spec.character_a),
        Value::Null
    ));
    assert!(has(
        "hist",
        "pascal",
        json!(spec.character_a),
        json!("Dressing Dice")
    ));
    assert!(has(
        "opTyping",
        "operator",
        json!("55555555-5555-4555-8555-555555555555"),
        Value::Null
    ));
    assert!(has(
        "opImp",
        "operator",
        json!(spec.character_b),
        Value::Null
    ));
    assert!(has("off", "operator", Value::Null, Value::Null));
    let cursor_back = oracle
        .iter()
        .find(|r| r.step == "hist_record_a_cursor_back_to_1")
        .expect("the cursor row");
    assert!(
        cursor_back
            .history
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["chatId"] == spec.chats["hist"].id.as_str()
                && r["characterId"] == spec.character_a.as_str()
                && r["entryIndex"] == 1
                && r["isCurrent"] == 1),
        "the cursor moved back to entry 1"
    );
    // P4.D282 — the route's effects really are the route's: the announcement
    // DEBUG and the catch WARN both carry `context: '[Chats v1] outfit-equip'`,
    // and the route answered 200 both times (an effect never fails it).
    for (name, level) in [
        ("equip_clear_slot_off_a_announces_at_once", "debug"),
        ("equip_after_drop_warns_and_still_answers", "warn"),
    ] {
        let row = oracle
            .iter()
            .find(|r| r.step == name)
            .unwrap_or_else(|| panic!("the {name} row"));
        assert_eq!(row.status, Some(200), "{name}");
        assert!(
            row.logs
                .iter()
                .any(|l| l["level"] == level && l["fields"]["context"] == "[Chats v1] outfit-equip"),
            "{name}: {:?}",
            row.logs
        );
    }
    assert!(
        warns
            .iter()
            .all(|l| l["fields"]["error"] == "no such table: background_jobs"),
        "the catch leg carries SQLite's bare message: {warns:?}"
    );
    // The flag-ON chat holds two avatar jobs; the OFF and autonomous chats none.
    let last_table = oracle
        .iter()
        .rev()
        .find_map(|r| r.jobs.as_array().cloned())
        .expect("a step with the table present");
    let avatar_chats: Vec<&str> = last_table
        .iter()
        .filter(|j| j["type"] == "CHARACTER_AVATAR_GENERATION")
        .filter_map(|j| j["payload"]["chatId"].as_str())
        .collect();
    assert_eq!(
        avatar_chats,
        vec![
            spec.chats["on"].id.as_str(),
            spec.chats["on"].id.as_str(),
            spec.chats["on2"].id.as_str()
        ],
        "avatar jobs: ON twice (A and B), the second ON once, never OFF / autonomous"
    );
    assert_eq!(
        last_table
            .iter()
            .filter(|j| j["type"] == "WARDROBE_OUTFIT_ANNOUNCEMENT")
            .count(),
        10,
        "ten announcements: on/A, on/B, on2/A, off/A, off/B, autonomous/B, hist/A, hist/B, opTyping/A, opImp/A (reuses collapse)"
    );
    // v4's own drain order for the two-id flush is INSERTION order (B then A),
    // the recorded divergence from v5's sorted drain.
    let flush = oracle
        .iter()
        .find(|r| r.step == "flush_two_ids")
        .expect("flush_two_ids");
    let order: Vec<&str> = flush
        .logs
        .iter()
        .filter_map(|l| l["fields"]["characterId"].as_str())
        .collect();
    assert_eq!(
        order,
        vec![spec.character_b.as_str(), spec.character_a.as_str()],
        "v4 drains its Set in insertion order (B was recorded first)"
    );

    drop(db);
    drop(scratch);
    eprintln!(
        "OK: outfit-change effects matched oracle ({} steps, {compared_lines} module lines).",
        spec.steps.len()
    );
}

/// The capture leg that needs no oracle: the module's two lines fire on their
/// own branches and stay silent on the others (a flag-OFF refresh, a flush of
/// an empty or absent set, a record INTO a set).
#[test]
fn module_lines_fire_on_their_branches_only() {
    global_capture::install();
    let Some(fx_main) = env_or_skip("QT_FIXTURE_OUTFIT_EFFECTS_MAIN") else {
        return;
    };
    let spec = load_spec();
    let (db, scratch) = open_copy(&fx_main, &spec.pepper);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let off = spec.chats["off"].id.clone();
    let on = spec.chats["on"].id.clone();
    let set: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let provenance = WardrobeHistoryProvenance::new(WardrobeHistorySource::Character)
        .with_actor(spec.character_b.clone());

    // Silence: a flag-OFF refresh, an empty flush, an absent-set flush, and a
    // record INTO the set log none of the module's lines.
    let ((), lines) = rt.block_on(global_capture::capture_async(async {
        refresh_avatar_for_outfit(&db, &spec.user_id, &off, &spec.character_a, "silent").await;
        flush_pending_wardrobe_announcements(&db, &spec.user_id, &on, Some(&set), None).await;
        flush_pending_wardrobe_announcements(&db, &spec.user_id, &on, None, None).await;
        let ctx = OutfitChangeContext {
            user_id: &spec.user_id,
            chat_id: &on,
            character_id: &spec.character_b,
            provenance: &provenance,
            history_intent: None,
            pending_wardrobe_announcements: Some(&set),
            // A set without its map settles the history at once — the
            // repository's lines, not the module's: still silent here.
            pending_wardrobe_history: None,
        };
        record_pending_wardrobe_announcement(&db, &ctx, "silent").await;
    }));
    assert!(
        lines.iter().all(|l| rust_line(l).is_none()),
        "silence leg: {lines:#?}"
    );
    assert_eq!(
        set.lock().unwrap().iter().cloned().collect::<Vec<_>>(),
        vec![spec.character_b.clone()]
    );

    // The DEBUG line, with v4's three fields in its order.
    let ((), lines) = rt.block_on(global_capture::capture_async(async {
        flush_pending_wardrobe_announcements(&db, &spec.user_id, &on, Some(&set), None).await;
    }));
    let got: Vec<Line> = lines.iter().filter_map(|l| rust_line(l)).collect();
    assert_eq!(got.len(), 1, "{lines:#?}");
    assert_eq!(got[0].0, "debug");
    assert_eq!(got[0].1, CAPTURED[0]);
    assert_eq!(
        got[0].2,
        BTreeMap::from([
            ("context".to_string(), "orchestrator-turn-end".to_string()),
            ("chatId".to_string(), on.clone()),
            ("characterId".to_string(), spec.character_b.clone()),
        ])
    );
    let raw = lines.iter().find(|l| l.contains(CAPTURED[0])).unwrap();
    assert!(
        raw.contains(&format!(
            "{} context=orchestrator-turn-end chatId={on} characterId={}",
            CAPTURED[0], spec.character_b
        )),
        "field order is v4's: {raw}"
    );
    assert!(set.lock().unwrap().is_empty(), "the flush cleared the set");

    // The WARN line: the enqueue's write failing, with the bare SQLite message.
    rt.block_on(db.write(|w| {
        w.main()
            .connection()
            .execute_batch("DROP TABLE background_jobs;")
            .map_err(quilltap_core::db::DbError::from)
    }))
    .expect("drop");
    let ((), lines) = rt.block_on(global_capture::capture_async(async {
        schedule_wardrobe_announcement(
            &db,
            "wardrobe-routes.equip",
            &spec.user_id,
            &on,
            &spec.character_a,
        )
        .await;
    }));
    let got: Vec<Line> = lines.iter().filter_map(|l| rust_line(l)).collect();
    assert_eq!(got.len(), 1, "{lines:#?}");
    assert_eq!(got[0].0, "warn");
    assert_eq!(got[0].1, CAPTURED[1]);
    assert_eq!(
        got[0].2,
        BTreeMap::from([
            ("context".to_string(), "wardrobe-routes.equip".to_string()),
            ("chatId".to_string(), on.clone()),
            ("characterId".to_string(), spec.character_a.clone()),
            (
                "error".to_string(),
                "no such table: background_jobs".to_string()
            ),
        ])
    );

    drop(db);
    drop(scratch);
}

#[test]
fn rust_line_parser_handles_spaced_values() {
    let line = "WARN quilltap_core::services::outfit_change_effects Failed to schedule wardrobe outfit announcement context=wardrobe-wear-handler chatId=c1 characterId=ch_1 error=no such table: background_jobs";
    let (level, message, fields) = rust_line(line).expect("parsed");
    assert_eq!(level, "warn");
    assert_eq!(message, CAPTURED[1]);
    assert_eq!(fields["context"], "wardrobe-wear-handler");
    assert_eq!(fields["chatId"], "c1");
    assert_eq!(fields["characterId"], "ch_1");
    assert_eq!(fields["error"], "no such table: background_jobs");
    assert!(rust_line("INFO quilltap_core::x Background job enqueued jobId=1").is_none());
}

/// P4.D316 — v4's outfit-history fence 4 ("no turn creates the announcement
/// set by hand", `outfit-history-fence.test.ts`): the per-turn containers are
/// minted ONLY by `new_wardrobe_turn_containers` — a set minted by hand would
/// arrive without its history map and settle every change at once instead of
/// at turn end. A source census over the core's production code: no
/// `Arc`-wrapped `HashSet::new()` (one line, or wrapped onto the next) outside
/// the factory. At `HISTORY` (`df642c2d6`) it counts exactly the five pre-round
/// hand mints — `tool_execution.rs:337`, `orchestrator.rs:3376`, both Brahma
/// surfaces and the scenario builder — all retired by this order.
#[test]
fn no_turn_mints_the_announcement_set_by_hand() {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push((p.clone(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut offenders = Vec::new();
    let mut factory_mints = 0;
    for (path, text) in &files {
        // Production code only: stop at the file's test module.
        let production = text.split("#[cfg(test)]").next().unwrap_or_default();
        let lines: Vec<&str> = production.lines().collect();
        let mut hits = 0;
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("HashSet::new()") {
                continue;
            }
            let window = format!("{} {}", if i > 0 { lines[i - 1] } else { "" }, line);
            if window.contains("Arc::new(") {
                hits += 1;
                offenders.push(format!("{}:{}", path.display(), i + 1));
            }
        }
        if path.ends_with("services/outfit_change_effects.rs") {
            factory_mints = hits;
            offenders.retain(|o| !o.contains("outfit_change_effects.rs"));
        }
    }
    assert!(
        offenders.is_empty(),
        "hand-minted announcement sets: {offenders:#?}"
    );
    assert_eq!(factory_mints, 1, "the factory is the one minter");
}
