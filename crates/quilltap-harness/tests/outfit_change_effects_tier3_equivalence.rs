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
//! Generate the fixture + oracle (Node 24, from the v4 checkout / a 4a38222e6 pin):
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

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::outfit_change_effects::{
    flush_pending_wardrobe_announcements, notify_wardrobe_changed,
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
    context: Option<String>,
}

#[derive(Deserialize)]
struct OracleRow {
    step: String,
    set: Vec<String>,
    jobs: Value,
    logs: Vec<Value>,
}

/// The module's own messages — the only lines this family compares.
const CAPTURED: [&str; 2] = [
    "Wardrobe outfit announcement scheduled",
    "Failed to schedule wardrobe outfit announcement",
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
    (db, scratch)
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

    // The one shared turn set (v4's `Set`), as the tool context carries it.
    let turn_set: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut jobs_gone = false;
    let mut failures = Vec::new();
    let mut compared_lines = 0usize;
    for (step, want) in spec.steps.iter().zip(&oracle) {
        assert_eq!(step.name, want.step, "oracle step order");
        let context: &'static str = Box::leak(step.context.clone().unwrap_or_default().into());
        let ((), lines) = rt.block_on(global_capture::capture_async(async {
            match step.op.as_str() {
                "notify" => {
                    let (chat, character) = (chat_of(&step.chat), character_of(&step.character));
                    let ctx = OutfitChangeContext {
                        user_id: &spec.user_id,
                        chat_id: &chat,
                        character_id: &character,
                        pending_wardrobe_announcements: step.with_set.then_some(&turn_set),
                    };
                    notify_wardrobe_changed(&db, &ctx, context).await;
                }
                "record" => {
                    let (chat, character) = (chat_of(&step.chat), character_of(&step.character));
                    let ctx = OutfitChangeContext {
                        user_id: &spec.user_id,
                        chat_id: &chat,
                        character_id: &character,
                        pending_wardrobe_announcements: step.with_set.then_some(&turn_set),
                    };
                    record_pending_wardrobe_announcement(&db, &ctx, context).await;
                }
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
                    )
                    .await;
                }
                "flushNoSet" => {
                    flush_pending_wardrobe_announcements(
                        &db,
                        &spec.user_id,
                        &chat_of(&step.chat),
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
        }));

        let mut got_set: Vec<String> = turn_set.lock().unwrap().iter().cloned().collect();
        got_set.sort();
        let got_jobs = dump_jobs(&db, jobs_gone);
        let mut got_lines: Vec<Line> = lines.iter().filter_map(|l| rust_line(l)).collect();
        let mut want_lines: Vec<Line> = want.logs.iter().map(oracle_line).collect();
        // The recorded divergence: v4 drains in insertion order, v5 sorted.
        // Compared as sorted lists; `flush_two_ids` below pins v4's own order.
        got_lines.sort();
        want_lines.sort();
        compared_lines += want_lines.len();

        let ok_set = got_set == want.set;
        let ok_jobs = got_jobs == want.jobs;
        let ok_lines = got_lines == want_lines;
        if !(ok_set && ok_jobs && ok_lines) {
            failures.push(format!(
                "{}: set {ok_set} jobs {ok_jobs} lines {ok_lines}\n  got set   {got_set:?}\n want set   {:?}\n  got jobs  {got_jobs}\n want jobs  {}\n  got lines {got_lines:?}\n want lines {want_lines:?}",
                step.name, want.set, want.jobs
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
    assert_eq!(warns.len(), 3, "three steps take the catch leg: {warns:?}");
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
        6,
        "six announcements: on/A, on/B, off/A, autonomous/B, on2/A, off/B (reuses collapse)"
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

    // Silence: a flag-OFF refresh, an empty flush, an absent-set flush, and a
    // record INTO the set log none of the module's lines.
    let ((), lines) = rt.block_on(global_capture::capture_async(async {
        refresh_avatar_for_outfit(&db, &spec.user_id, &off, &spec.character_a, "silent").await;
        flush_pending_wardrobe_announcements(&db, &spec.user_id, &on, Some(&set)).await;
        flush_pending_wardrobe_announcements(&db, &spec.user_id, &on, None).await;
        let ctx = OutfitChangeContext {
            user_id: &spec.user_id,
            chat_id: &on,
            character_id: &spec.character_b,
            pending_wardrobe_announcements: Some(&set),
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
        flush_pending_wardrobe_announcements(&db, &spec.user_id, &on, Some(&set)).await;
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
