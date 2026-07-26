//! P4.9E1A chat CAST + AVATAR-OVERRIDE route-surface differential:
//! `api::chat_cast::*` + `api::salon::chat_update`'s participant families vs
//! v4's REAL chat action handlers (`actions/participants.ts`,
//! `actions/avatars.ts`, `actions/toggle-avatar-generation.ts`) and its `PUT`
//! bag (`helpers.ts` `processChatUpdates`). Both sides read a FRESH copy of the
//! committed `chat-cast-{main,mount}.db` fixture per case (baked ids identical),
//! then the response body AND the post-mutation table dumps are diffed.
//!
//! Coverage is asserted by SHAPE, not a hand-written count: every case name the
//! oracle emitted must have been driven here and vice versa
//! (`harness-corpus-shape-constants-rot`).
//!
//! ## Normalization (applied identically to both sides)
//!
//! Anything the mutating cases MINT is tokenized: a uuid that is not one of the
//! fixture's pinned ids becomes `<u0>`, `<u1>`, … in traversal order (so a
//! minted participant id still links its `impersonatingParticipantIds` /
//! `activeTypingParticipantId` references), and an ISO timestamp that is not the
//! seed sentinel becomes `<ts>`. A timestamp EQUAL to the sentinel is diffed
//! exactly — that is what proves `createdAt` preservation and the absence of a
//! stray mint.
//!
//! ## Three recorded, direction-asserted differences
//!
//! 1. **[`V4_CREATED_CASES`]** — v4 answers **201** on a fresh
//!    `add-participant`. The dispatch boundary carries no per-verb success
//!    status (every success is 200 at the HTTP edge — the standing `ChatCreate`
//!    precedent), so those cases assert v4==201 ∧ v5==200. The BODY is compared
//!    byte-for-byte.
//! 2. **[`VALIDATION_DETAILS_GAP`]** — v4's Zod arm answers `{error:
//!    'Validation error', details: […]}`; v5's error envelope has no `details`
//!    array (the standing, named P4.6bb deferral). Asserted in both directions,
//!    then dropped before the body diff.
//! 3. ~~`PARTICIPANT_NULL_COLLAPSE`~~ — **CLOSED at this round's unification.**
//!    v4's `ChatParticipantBaseSchema` marks `joinScenario`, `talkativeness` and
//!    `roleplayTemplateId` `.nullable().optional()`, so an explicit `null`
//!    survives its parse and `JSON.stringify`; v5's `db::chats::ChatParticipant`
//!    modelled them as plain `Option<T>` and dropped the key. The lane escalated
//!    it rather than reaching into `db/chats.rs` (P4.9E3A's file that round);
//!    the unifier made all three double-`Option`s, and the both-directions
//!    tripwire that guarded the gap FIRED on the first run, exactly as designed.
//!    Every body and table now matches without reconciliation.
//!
//! ## The two entrances land the same state
//!
//! [`ENTRANCE_PAIRS`] names the (`?action=` case, chat-PUT-bag case) pairs that
//! perform the SAME mutation, and the test asserts their `chat` dumps are
//! identical — the order's "one implementation, two entrances" requirement. The
//! `add` pair is compared with `equippedOutfit` dropped: the action verb dresses
//! the newcomer and `processChatUpdates` deliberately does not.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-cast.ndjson npx jest -- chat-cast-routes
//! Run:
//!   QT_ORACLE_CHAT_CAST=/tmp/oracle-chat-cast.ndjson \
//!     cargo test -p quilltap-harness --test chat_cast_routes_equivalence -- --nocapture

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::chat_cast;
use quilltap_core::api::salon;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::chat_participants::{ParticipantAddData, ParticipantUpdateData};
use serde::Deserialize;
use serde_json::{json, Value};

const SEED_TS: &str = "2026-05-01T00:00:00.000Z";
const NOW_MS: i64 = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z (the oracle's frozen clock)

/// v4 answers 201; the dispatch boundary answers 200.
const V4_CREATED_CASES: &[&str] = &[
    "add_fresh_defaults",
    "add_explicit_fields",
    "add_history_access_suppresses_scenario",
    "add_outfit_none",
    "add_outfit_manual",
    "add_user_controlled",
    "add_stale_profile_falls_back",
];

/// Cases whose 400 body carries v4's Zod `details` array.
const VALIDATION_DETAILS_GAP: &[&str] =
    &["update_invalid_status", "update_talkativeness_out_of_range"];

/// (`?action=` case, chat-PUT-bag case) pairs that perform the same mutation.
/// The third element drops `equippedOutfit` before comparing (see the header).
const ENTRANCE_PAIRS: &[(&str, &str, bool)] = &[
    ("update_display_order", "bag_update_participant", false),
    ("add_fresh_defaults", "bag_add_participant", true),
    ("remove_ok", "bag_remove_participant", false),
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    ids: BTreeMap<String, String>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/chat-cast.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-cast-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-cast-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-cast-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn looks_like_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn looks_like_iso(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24 && b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[23] == b'Z'
}

/// Tokenize every MINTED uuid (first-appearance order) and every non-sentinel
/// timestamp. Pinned fixture ids and the seed sentinel pass through untouched,
/// so their linkage and preservation stay diffable.
struct Tokenizer<'a> {
    pinned: &'a BTreeSet<String>,
    map: BTreeMap<String, String>,
}

impl Tokenizer<'_> {
    fn walk(&mut self, v: &mut Value) {
        match v {
            Value::String(s) => {
                if looks_like_uuid(s) && !self.pinned.contains(s.as_str()) {
                    let next = self.map.len();
                    let tok = self
                        .map
                        .entry(s.clone())
                        .or_insert_with(|| format!("<u{next}>"))
                        .clone();
                    *s = tok;
                } else if looks_like_iso(s) && s != SEED_TS {
                    *s = "<ts>".to_string();
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.walk(x)),
            Value::Object(o) => {
                // KEYS are tokenized too: `compiledIdentityStacks` is keyed by
                // participant id, and a freshly-added participant's is minted.
                let mut next_map = serde_json::Map::new();
                for (k, mut v) in std::mem::take(o) {
                    self.walk(&mut v);
                    let mut key = Value::String(k);
                    self.walk(&mut key);
                    next_map.insert(key.as_str().unwrap().to_string(), v);
                }
                *o = next_map;
            }
            _ => {}
        }
    }
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

fn norm(v: &Value, pinned: &BTreeSet<String>) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    let mut t = Tokenizer {
        pinned,
        map: BTreeMap::new(),
    };
    t.walk(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// The web-edge (status, body) for a Response.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatCast(v) => (200, v.clone()),
        Response::Chat(w) => (200, json!({ "chat": w.chat })),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

// ---------------------------------------------------------------------------
// Table dumps (mirror the oracle's readers exactly)
// ---------------------------------------------------------------------------

fn dump_chat_row(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    db.read_main(move |c| {
        use rusqlite::OptionalExtension;
        let row = c
            .query_row(
                "SELECT id, participants, impersonatingParticipantIds, activeTypingParticipantId, \
                 tags, equippedOutfit, avatarGenerationEnabled, compiledIdentityStacks, updatedAt \
                 FROM chats WHERE id = ?1",
                rusqlite::params![cid],
                |r| {
                    let j = |s: Option<String>| -> Value {
                        s.filter(|x| !x.is_empty())
                            .and_then(|x| serde_json::from_str::<Value>(&x).ok())
                            .unwrap_or(Value::Null)
                    };
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "participants": j(r.get::<_, Option<String>>(1)?),
                        "impersonatingParticipantIds": j(r.get::<_, Option<String>>(2)?),
                        "activeTypingParticipantId": r.get::<_, Option<String>>(3)?,
                        "tags": j(r.get::<_, Option<String>>(4)?),
                        "equippedOutfit": j(r.get::<_, Option<String>>(5)?),
                        // Dumped RAW (the oracle reads the column straight out of
                        // SQLite, where the boolean is stored as 0/1).
                        "avatarGenerationEnabled": r.get::<_, Option<i64>>(6)?,
                        "compiledIdentityStacks": j(r.get::<_, Option<String>>(7)?),
                        "updatedAt": r.get::<_, String>(8)?,
                    }))
                },
            )
            .optional()?;
        Ok(row.unwrap_or(Value::Null))
    })
    .unwrap()
}

/// Every chat message event, re-sorted deterministically — the SAME re-sort the
/// oracle applies. `getMessages` is `ORDER BY createdAt ASC`, but the two sides'
/// `createdAt` values are not comparable: the oracle freezes its clock (one
/// timestamp for every announcement of a case, SQLite's tie-break deciding the
/// order) while this side's real clock advances and orders by insertion. Both
/// are normalized to `<ts>` anyway, so the timestamp cannot participate in the
/// sort; re-sorting by `(type, systemKind, body)` makes the diff compare the SET
/// of announcements, which is the actual claim.
fn dump_messages(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    let mut rows = db
        .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
        .unwrap();
    let key = |m: &Value| -> String {
        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let body = if m.get("content").is_some() {
            s("content")
        } else {
            s("description")
        };
        format!("{}\u{0}{}\u{0}{}", s("type"), s("systemKind"), body)
    };
    rows.sort_by_key(key);
    Value::Array(rows)
}

fn dump_avatar_overrides(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare("SELECT id, avatarOverrides FROM characters ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| {
                let raw: Option<String> = r.get(1)?;
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "avatarOverrides": raw
                        .filter(|x| !x.is_empty())
                        .and_then(|x| serde_json::from_str::<Value>(&x).ok())
                        .unwrap_or_else(|| Value::Array(Vec::new())),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

fn dump_jobs(db: &Db) -> Value {
    db.read_main(|c| {
        // ORDER BY the payload's characterId, NOT the row id: the id is a fresh
        // uuid on both sides, so ordering by it is ordering by randomness.
        let mut stmt = c.prepare(
            "SELECT id, userId, type, status, priority, attempts, maxAttempts, payload \
             FROM background_jobs ORDER BY type, json_extract(payload, '$.characterId')",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let payload: String = r.get(7)?;
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "userId": r.get::<_, String>(1)?,
                    "type": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?,
                    "priority": r.get::<_, Option<f64>>(4)?,
                    "attempts": r.get::<_, Option<f64>>(5)?,
                    "maxAttempts": r.get::<_, Option<f64>>(6)?,
                    "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// Request-bag builders (the same coercion the chat-PUT bag entrance uses, so
// the two entrances are driven from ONE description of each mutation).
// ---------------------------------------------------------------------------

fn add_data(v: Value) -> ParticipantAddData {
    ParticipantAddData::from_value(&v).expect("valid add bag")
}
fn update_data(v: Value) -> ParticipantUpdateData {
    // The rejection cases go through the same parser; a parse failure is the
    // 400 v4 answers, so surface it as a synthetic bag with the raw values.
    ParticipantUpdateData::from_value(&v).unwrap_or(ParticipantUpdateData {
        participant_id: v
            .get("participantId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: v.get("status").and_then(Value::as_str).map(str::to_string),
        talkativeness: v.get("talkativeness").map(|t| t.as_f64()),
        ..Default::default()
    })
}

#[test]
fn chat_cast_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHAT_CAST") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let i = &spec.ids;
    let mut pinned: BTreeSet<String> = i.values().cloned().collect();
    pinned.insert(spec.user_id.clone());
    // The character vaults are minted at fixture-build time; they are stable
    // across both sides (same .db file) but not listed in the spec.
    let meta: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("chat-cast-main.db.meta.json")).unwrap(),
    )
    .unwrap();
    if let Some(v) = meta.get("vaults").and_then(Value::as_object) {
        for value in v.values() {
            if let Some(s) = value.as_str() {
                pinned.insert(s.to_string());
            }
        }
    }

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();
    // The `chats` dumps of each case, for the two-entrances assertion.
    let mut chat_dumps: HashMap<String, Value> = HashMap::new();

    {
        let pinned = &pinned;
        let oracle = &oracle;
        let failed = &mut failed;
        let driven = &mut driven;
        let chat_dumps = &mut chat_dumps;

        let mut check = |name: &str, resp: &Response, tables: Option<Value>| {
            driven.insert(name.to_string());
            let Some(want) = oracle.get(name) else {
                failed.push(format!("{name}_MISSING_FROM_ORACLE"));
                return;
            };
            let (status, body) = status_body(resp);
            let want_status = want["status"].as_u64().unwrap() as u16;
            let mut want_body = want["body"].clone();

            if V4_CREATED_CASES.contains(&name) {
                if want_status != 201 || status != 200 {
                    eprintln!(
                        "[{name}] the recorded 201-vs-200 difference no longer holds \
                         (v4={want_status}, v5={status}) — re-rule it or drop the entry"
                    );
                    failed.push(format!("{name}_status"));
                }
            } else if status != want_status {
                eprintln!("[{name}] STATUS {status} != {want_status}");
                failed.push(format!("{name}_status"));
            }

            if VALIDATION_DETAILS_GAP.contains(&name) {
                let v4_ok = want_body
                    .get("details")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty());
                let v5_has = body.get("details").is_some();
                if !v4_ok || v5_has {
                    eprintln!(
                        "[{name}] the recorded validation-details gap no longer holds \
                         (v4 details={v4_ok}, v5 details={v5_has}) — re-rule it or drop the entry"
                    );
                    failed.push(format!("{name}_details_gap"));
                }
                if let Some(o) = want_body.as_object_mut() {
                    o.remove("details");
                }
            }

            if norm(&body, pinned) != norm(&want_body, pinned) {
                eprintln!(
                    "[{name}] BODY MISMATCH:\n{}",
                    first_diff(&norm(&body, pinned), &norm(&want_body, pinned))
                );
                failed.push(name.to_string());
            } else {
                eprintln!("[{name}] body OK.");
            }

            if let Some(mut got_tables) = tables {
                let want_tables = want["tables"].clone();
                if let Some(chat) = got_tables.get("chat") {
                    chat_dumps.insert(name.to_string(), chat.clone());
                }
                canon_numbers(&mut got_tables);
                if norm(&got_tables, pinned) != norm(&want_tables, pinned) {
                    eprintln!(
                        "[{name} tables] MISMATCH:\n{}",
                        first_diff(&norm(&got_tables, pinned), &norm(&want_tables, pinned))
                    );
                    failed.push(format!("{name}_tables"));
                } else {
                    eprintln!("[{name} tables] OK.");
                }
            }
        };

        let id = |k: &str| i.get(k).expect("spec id").clone();
        let (chat_main, chat_quiet, chat_solo) = (id("chatMain"), id("chatQuiet"), id("chatSolo"));
        let missing = id("missing");

        let cast_tables = |db: &Db, chat: &str| {
            json!({
                "chat": dump_chat_row(db, chat),
                "messages": dump_messages(db, chat),
            })
        };

        // ── ?action=add-participant ───────────────────────────────────────
        let mut add = |tag: &str, name: &str, chat: &str, bag: Value, dump: bool| {
            let db = fresh_db(&spec, tag);
            let data = add_data(bag);
            let r = rt.block_on(chat_cast::chat_add_participant(
                &db,
                &spec.user_id,
                chat,
                &data,
            ));
            let tables = dump.then(|| cast_tables(&db, chat));
            check(name, &r, tables);
        };
        add(
            "a1",
            "add_fresh_defaults",
            &chat_main,
            json!({ "type": "CHARACTER", "characterId": id("dora") }),
            true,
        );
        add(
            "a2",
            "add_explicit_fields",
            &chat_quiet,
            json!({
                "type": "CHARACTER",
                "characterId": id("dora"),
                "connectionProfileId": id("connB"),
                "imageProfileId": id("imageProfile"),
                "displayOrder": 7,
                "hasHistoryAccess": false,
                "joinScenario": "arrives by airship, soaked through",
                "controlledBy": "llm",
            }),
            true,
        );
        add(
            "a3",
            "add_history_access_suppresses_scenario",
            &chat_quiet,
            json!({
                "type": "CHARACTER",
                "characterId": id("dora"),
                "joinScenario": "arrives by airship, soaked through",
                "hasHistoryAccess": true,
            }),
            true,
        );
        add(
            "a4",
            "add_outfit_none",
            &chat_quiet,
            json!({
                "type": "CHARACTER",
                "characterId": id("dora"),
                "outfitSelection": { "characterId": id("dora"), "mode": "none" },
            }),
            true,
        );
        add(
            "a5",
            "add_outfit_manual",
            &chat_quiet,
            json!({
                "type": "CHARACTER",
                "characterId": id("dora"),
                "outfitSelection": {
                    "characterId": id("dora"),
                    "mode": "manual",
                    "slots": { "top": [id("wDoraTop")], "bottom": [], "footwear": [], "accessories": [] },
                },
            }),
            true,
        );
        add(
            "a6",
            "add_user_controlled",
            &chat_quiet,
            json!({ "type": "CHARACTER", "characterId": id("dora"), "controlledBy": "user" }),
            true,
        );
        add(
            "a7",
            "add_stale_profile_falls_back",
            &chat_quiet,
            json!({
                "type": "CHARACTER",
                "characterId": id("dora"),
                "connectionProfileId": missing,
            }),
            true,
        );
        add(
            "a8",
            "add_reactivates_removed",
            &chat_main,
            json!({ "type": "CHARACTER", "characterId": id("eris") }),
            true,
        );
        add(
            "a9",
            "add_reactivates_removed_with_outfit",
            &chat_main,
            json!({
                "type": "CHARACTER",
                "characterId": id("eris"),
                "controlledBy": "user",
                "outfitSelection": { "characterId": id("eris"), "mode": "default" },
            }),
            true,
        );
        add(
            "a10",
            "add_already_present",
            &chat_main,
            json!({ "type": "CHARACTER", "characterId": id("bram") }),
            false,
        );
        add(
            "a11",
            "add_already_present_deactivated",
            &chat_solo,
            json!({ "type": "CHARACTER", "characterId": id("cleo") }),
            false,
        );
        add(
            "a12",
            "add_character_missing",
            &chat_main,
            json!({ "type": "CHARACTER", "characterId": missing }),
            false,
        );
        add(
            "a13",
            "add_chat_missing",
            &missing,
            json!({ "type": "CHARACTER", "characterId": id("dora") }),
            false,
        );

        // ── ?action=update-participant ────────────────────────────────────
        let mut upd = |tag: &str, name: &str, chat: &str, bag: Value, dump: bool| {
            let db = fresh_db(&spec, tag);
            let data = update_data(bag);
            let r = rt.block_on(chat_cast::chat_update_participant(&db, chat, &data));
            let tables = dump.then(|| cast_tables(&db, chat));
            check(name, &r, tables);
        };
        let p_bram = id("pBram");
        let p_cleo = id("pCleo");
        let p_aria = id("pAria");
        let p_gail = id("pGail");
        let q_bram = id("qBram");
        let q_cleo = id("qCleo");
        upd(
            "u1",
            "update_display_order",
            &chat_main,
            json!({ "participantId": p_bram, "displayOrder": 5 }),
            true,
        );
        upd(
            "u2",
            "update_talkativeness_set",
            &chat_main,
            json!({ "participantId": p_bram, "talkativeness": 0.4 }),
            true,
        );
        upd(
            "u3",
            "update_talkativeness_null",
            &chat_main,
            json!({ "participantId": p_bram, "talkativeness": null }),
            true,
        );
        upd(
            "u4",
            "update_talkativeness_absent",
            &chat_main,
            json!({ "participantId": p_bram, "hasHistoryAccess": true }),
            true,
        );
        upd(
            "u5",
            "update_image_profile_set",
            &chat_main,
            json!({ "participantId": p_bram, "imageProfileId": id("imageProfile") }),
            true,
        );
        upd(
            "u6",
            "update_image_profile_null",
            &chat_main,
            json!({ "participantId": p_bram, "imageProfileId": null }),
            true,
        );
        upd(
            "u7",
            "update_join_scenario_set",
            &chat_main,
            json!({ "participantId": p_bram, "joinScenario": "was already at the table" }),
            true,
        );
        upd(
            "u8",
            "update_join_scenario_null",
            &chat_main,
            json!({ "participantId": p_bram, "joinScenario": null }),
            true,
        );
        upd(
            "u9",
            "update_selected_prompt_change",
            &chat_main,
            json!({ "participantId": p_bram, "selectedSystemPromptId": id("bramPromptAlt") }),
            true,
        );
        upd(
            "u10",
            "update_selected_prompt_null",
            &chat_main,
            json!({ "participantId": p_bram, "selectedSystemPromptId": null }),
            true,
        );
        upd(
            "u11",
            "update_selected_prompt_unchanged",
            &chat_main,
            json!({ "participantId": p_bram, "selectedSystemPromptId": id("bramPrompt") }),
            true,
        );
        upd(
            "u12",
            "update_connection_profile_change",
            &chat_main,
            json!({ "participantId": p_bram, "connectionProfileId": id("connB") }),
            true,
        );
        upd(
            "u13",
            "update_connection_profile_same",
            &chat_main,
            json!({ "participantId": p_bram, "connectionProfileId": id("connA") }),
            true,
        );
        upd(
            "u14",
            "update_connection_profile_missing",
            &chat_main,
            json!({ "participantId": p_bram, "connectionProfileId": missing }),
            false,
        );
        upd(
            "u15",
            "update_image_profile_missing",
            &chat_main,
            json!({ "participantId": p_bram, "imageProfileId": missing }),
            false,
        );
        upd(
            "u16",
            "update_controlled_by_user_no_typist",
            &chat_quiet,
            json!({ "participantId": q_bram, "controlledBy": "user" }),
            true,
        );
        upd(
            "u17",
            "update_controlled_by_user_typist_set",
            &chat_main,
            json!({ "participantId": p_bram, "controlledBy": "user" }),
            true,
        );
        upd(
            "u18",
            "update_controlled_by_llm_promotes",
            &chat_main,
            json!({ "participantId": p_aria, "controlledBy": "llm" }),
            true,
        );
        upd(
            "u19",
            "update_controlled_by_llm_not_typist",
            &chat_main,
            json!({ "participantId": p_gail, "controlledBy": "llm" }),
            true,
        );
        upd(
            "u20",
            "update_controlled_by_with_status_early_return",
            &chat_main,
            json!({ "participantId": p_bram, "controlledBy": "user", "status": "silent" }),
            true,
        );
        upd(
            "u21",
            "update_status_silent",
            &chat_main,
            json!({ "participantId": p_bram, "status": "silent" }),
            true,
        );
        upd(
            "u22",
            "update_status_exit_silent",
            &chat_quiet,
            json!({ "participantId": q_cleo, "status": "active" }),
            true,
        );
        upd(
            "u23",
            "update_status_absent",
            &chat_main,
            json!({ "participantId": p_cleo, "status": "absent" }),
            true,
        );
        upd(
            "u24",
            "update_status_removed",
            &chat_main,
            json!({ "participantId": p_cleo, "status": "removed" }),
            true,
        );
        upd(
            "u25",
            "update_is_active_false",
            &chat_main,
            json!({ "participantId": p_bram, "isActive": false }),
            true,
        );
        upd(
            "u26",
            "update_is_active_true_noop",
            &chat_main,
            json!({ "participantId": p_bram, "isActive": true }),
            true,
        );
        upd(
            "u27",
            "update_participant_missing",
            &chat_main,
            json!({ "participantId": missing, "displayOrder": 3 }),
            false,
        );
        upd(
            "u28",
            "update_chat_missing",
            &missing,
            json!({ "participantId": p_bram, "displayOrder": 3 }),
            false,
        );
        upd(
            "u29",
            "update_invalid_status",
            &chat_main,
            json!({ "participantId": p_bram, "status": "lurking" }),
            false,
        );
        upd(
            "u30",
            "update_talkativeness_out_of_range",
            &chat_main,
            json!({ "participantId": p_bram, "talkativeness": 4 }),
            false,
        );

        // ── ?action=remove-participant ────────────────────────────────────
        let mut rem = |tag: &str, name: &str, chat: &str, participant: &str, dump: bool| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_cast::chat_remove_participant(&db, chat, participant));
            let tables = dump.then(|| cast_tables(&db, chat));
            check(name, &r, tables);
        };
        rem("r1", "remove_ok", &chat_main, &p_cleo, true);
        rem(
            "r2",
            "remove_impersonating_promotes",
            &chat_main,
            &p_aria,
            true,
        );
        rem(
            "r3",
            "remove_impersonating_not_typist",
            &chat_main,
            &p_gail,
            true,
        );
        rem(
            "r4",
            "remove_last_character",
            &chat_solo,
            &id("sBram"),
            false,
        );
        rem(
            "r5",
            "remove_participant_missing",
            &chat_main,
            &missing,
            false,
        );
        rem("r6", "remove_chat_missing", &missing, &p_cleo, false);

        // ── ?action=rebuild-system-prompt ─────────────────────────────────
        let mut rb = |tag: &str, name: &str, chat: &str, participant: &str, dump: bool| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_cast::chat_rebuild_system_prompt(
                &db,
                chat,
                participant,
            ));
            let tables = dump.then(|| cast_tables(&db, chat));
            check(name, &r, tables);
        };
        rb("b1", "rebuild_ok", &chat_main, &p_bram, true);
        rb("b2", "rebuild_user_controlled", &chat_main, &p_aria, false);
        rb(
            "b3",
            "rebuild_participant_missing",
            &chat_main,
            &missing,
            false,
        );
        rb("b4", "rebuild_chat_missing", &missing, &p_bram, false);
        rb("b5", "rebuild_no_participant_id", &chat_main, "", false);

        // ── the avatar-override family ────────────────────────────────────
        {
            let db = fresh_db(&spec, "v1");
            let r = chat_cast::chat_get_avatars(&db, &spec.user_id, &chat_main);
            check("get_avatars", &r, None);
        }
        {
            let db = fresh_db(&spec, "v2");
            let r = chat_cast::chat_get_avatars(&db, &spec.user_id, &missing);
            check("get_avatars_chat_missing", &r, None);
        }
        let mut set_avatar = |tag: &str, name: &str, character: &str, image: &str, dump: bool| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_cast::chat_set_avatar(
                &db, &chat_main, character, image,
            ));
            let tables = dump.then(|| json!({ "overrides": dump_avatar_overrides(&db) }));
            check(name, &r, tables);
        };
        set_avatar("v3", "set_avatar_new", &id("dora"), &id("imgA"), true);
        set_avatar("v4", "set_avatar_replace", &id("bram"), &id("imgB"), true);
        set_avatar(
            "v5",
            "set_avatar_character_missing",
            &missing,
            &id("imgA"),
            false,
        );
        set_avatar(
            "v6",
            "set_avatar_image_missing",
            &id("bram"),
            &missing,
            false,
        );
        let mut remove_avatar = |tag: &str, name: &str, character: &str, dump: bool| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_cast::chat_remove_avatar(&db, &chat_main, character));
            let tables = dump.then(|| json!({ "overrides": dump_avatar_overrides(&db) }));
            check(name, &r, tables);
        };
        remove_avatar("v7", "remove_avatar_ok", &id("bram"), true);
        remove_avatar("v8", "remove_avatar_no_override", &id("dora"), true);
        remove_avatar("v9", "remove_avatar_character_missing", &missing, false);
        let mut toggle = |tag: &str, name: &str, chat: &str, dump: bool| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(chat_cast::chat_toggle_avatar_generation(
                &db,
                &spec.user_id,
                chat,
            ));
            let tables =
                dump.then(|| json!({ "chat": dump_chat_row(&db, chat), "jobs": dump_jobs(&db) }));
            check(name, &r, tables);
        };
        toggle("v10", "toggle_avatar_generation_on", &chat_main, true);
        toggle("v11", "toggle_avatar_generation_off", &chat_quiet, true);
        toggle(
            "v12",
            "toggle_avatar_generation_chat_missing",
            &missing,
            false,
        );

        // ── the chat-PUT bag entrance ─────────────────────────────────────
        let mut bag = |tag: &str,
                       name: &str,
                       chat: &str,
                       chat_bag: Value,
                       upd: Option<Value>,
                       add: Option<Value>,
                       rm: Option<&str>,
                       dump: bool| {
            let db = fresh_db(&spec, tag);
            let r = rt.block_on(salon::chat_update(
                &db,
                &spec.user_id,
                chat,
                &chat_bag,
                upd.as_ref(),
                add.as_ref(),
                rm,
            ));
            let tables = dump.then(|| cast_tables(&db, chat));
            check(name, &r, tables);
        };
        bag(
            "g1",
            "bag_update_participant",
            &chat_main,
            json!({}),
            Some(json!({ "participantId": p_bram, "displayOrder": 5 })),
            None,
            None,
            true,
        );
        bag(
            "g2",
            "bag_add_participant",
            &chat_main,
            json!({}),
            None,
            Some(json!({ "type": "CHARACTER", "characterId": id("dora") })),
            None,
            true,
        );
        bag(
            "g3",
            "bag_remove_participant",
            &chat_main,
            json!({}),
            None,
            None,
            Some(&p_cleo),
            true,
        );
        bag(
            "g4",
            "bag_remove_last_participant",
            &chat_solo,
            json!({}),
            None,
            None,
            Some(&id("sBram")),
            false,
        );
        bag(
            "g5",
            "bag_update_and_chat_fields",
            &chat_main,
            json!({ "title": "The Salon, Reconvened" }),
            Some(json!({ "participantId": p_bram, "status": "silent" })),
            None,
            None,
            true,
        );
        bag(
            "g6",
            "bag_update_participant_missing",
            &chat_main,
            json!({}),
            Some(json!({ "participantId": missing, "displayOrder": 5 })),
            None,
            None,
            false,
        );
    }

    // The two entrances land the same DB state.
    for (action_case, bag_case, drop_outfit) in ENTRANCE_PAIRS {
        let (Some(a), Some(b)) = (chat_dumps.get(*action_case), chat_dumps.get(*bag_case)) else {
            failed.push(format!("{action_case}_vs_{bag_case}_MISSING_DUMP"));
            continue;
        };
        let strip = |v: &Value| -> Value {
            let mut v = v.clone();
            if *drop_outfit {
                if let Some(o) = v.as_object_mut() {
                    o.remove("equippedOutfit");
                }
            }
            v
        };
        if norm(&strip(a), &pinned) != norm(&strip(b), &pinned) {
            eprintln!(
                "[entrances {action_case} vs {bag_case}] MISMATCH:\n{}",
                first_diff(&norm(&strip(a), &pinned), &norm(&strip(b), &pinned))
            );
            failed.push(format!("{action_case}_vs_{bag_case}"));
        } else {
            eprintln!("[entrances {action_case} vs {bag_case}] identical.");
        }
    }

    // Shape assertion: the driven set and the oracle's set must be identical.
    let recorded: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = recorded.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&recorded).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case coverage drifted — oracle-only: {missing:?}, rust-only: {extra:?}"
    );
    eprintln!("chat-cast-routes: {} cases driven.", driven.len());

    assert!(failed.is_empty(), "chat-cast-routes FAILED: {failed:?}");
}

/// A key-order assertion on the enriched-participant body (order tier-1 item 6:
/// typed structs in v4's object-literal order, never sorted). `preserve_order`
/// is on, so `serde_json` keeps struct field order end-to-end.
#[test]
fn enriched_participant_key_order_matches_v4() {
    let p = json!({
        "id": "e1000000-0000-4000-8000-000000000003",
        "type": "CHARACTER",
        "characterId": "a1000000-0000-4000-8000-000000000002",
        "controlledBy": "llm",
        "displayOrder": 2,
        "isActive": true,
        "status": "active",
        "hasHistoryAccess": false,
        "joinScenario": Value::Null,
        "createdAt": SEED_TS,
        "updatedAt": SEED_TS,
    });
    let enriched = quilltap_core::services::chat_participants::EnrichedParticipant {
        id: "e1".into(),
        kind: "CHARACTER".into(),
        controlled_by: "llm".into(),
        character_id: Some("a1".into()),
        display_order: 2,
        is_active: true,
        status: "active".into(),
        removed_at: None,
        has_history_access: false,
        join_scenario: p
            .get("joinScenario")
            .map(|v| v.as_str().map(str::to_string)),
        character: None,
        connection_profile: None,
        created_at: SEED_TS.into(),
        updated_at: SEED_TS.into(),
    };
    let v = serde_json::to_value(&enriched).unwrap();
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        vec![
            "id",
            "type",
            "controlledBy",
            "characterId",
            "displayOrder",
            "isActive",
            "status",
            "removedAt",
            "hasHistoryAccess",
            "joinScenario",
            "character",
            "connectionProfile",
            "createdAt",
            "updatedAt",
        ],
        "the enriched-participant key order drifted from v4's `enrichParticipant` literal"
    );
    // The frozen clock the oracle uses, kept referenced so a change to one side
    // is a compile-visible change to both.
    assert_eq!(NOW_MS, 1_777_939_200_000);
}
