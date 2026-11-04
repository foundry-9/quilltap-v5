//! P4.D343 (items 12, 25, 26; contract W1) — the Storyteller-mode route
//! surface, tier 2: the chat PUT's `turnMode` sibling + the
//! `storytellerInviteFilter` bag key (`api::salon::chat_update`), the chat GET
//! wire's two keys after `isPaused` (`api::salon::chat_get` →
//! `assemble_chat_get`), and the four Storyteller actions
//! (`api::chat_storyteller::{chat_storyteller_round, chat_storyteller_lie,
//! chat_storyteller_run, chat_storyteller_ask}`) vs v4's REAL chat routes at
//! the pin (`handlers/put.ts` / `get.ts` / `post.ts`, `actions/storyteller.ts`,
//! `helpers.ts::processChatUpdates`).
//!
//! Each case runs over a FRESH copy of the committed salon fixture pair on
//! BOTH sides, with identical plants (raw `chats.turnMode` /
//! `storytellerInviteFilter` cells; rounds through each side's REAL
//! `StorytellerRoundsRepository.create` with a pinned id / createdAt, a closed
//! round then through the REAL `markClosed`). Compared per case:
//!
//! - the HTTP status, and the body BYTES (`serde_json::to_string`, key order
//!   significant) — except the PUT's `{chat}` echo, which is compared
//!   key-sorted (its field order is the whole-chat reader's, owned by sibling
//!   families) PLUS the ordered key window around `isPaused`; error bodies
//!   compare `error` (carve [`ZOD_DETAILS`]);
//! - the `chats` and `storyteller_rounds` dumps, minted timestamps
//!   placeholdered on both sides (NULL-ness kept);
//! - the turn-mode / Storyteller log lines, in order (v4's captured by
//!   splicing `logger.transports`, v5's through a process-global capture — the
//!   repository writes run on the writer thread).
//!
//! The run / ask rows are the REFUSAL arms only: v5's host driver
//! (`quilltap-host`'s `storyteller_spine.rs`) plans through
//! `plan_storyteller_run` / `prepare_ask_anyway` and maps a refusal through
//! the core's `From<RoundServiceError>`; [`RefusalDriver`] mirrors that and
//! PANICS if a row ever planned past the refusals (no row may reach a model).
//!
//! Generate the oracle (Node 24; jest ignores `.claude/`, so mirror the case
//! file; the fixture pair is copied first so the committed pair is never
//! written):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   M=/tmp/qt-chat-turn-mode-mirror; rm -rf $M; mkdir -p $M/cases $M/fixtures $M/lib $M/db
//!   cp $V5W/harness/oracle/cases/chat-turn-mode-routes.test.ts $M/cases/
//!   cp $V5W/harness/oracle/lib/p4d171-columns.ts $M/lib/
//!   cp $V5W/harness/oracle/fixtures/salon.json $M/fixtures/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/salon-main.db $M/db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/salon-mount.db $M/db/
//!   QT_FIXTURE_SALON_MAIN=$M/db/salon-main.db \
//!   QT_FIXTURE_SALON_MOUNT=$M/db/salon-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-chat-turn-mode-routes.ndjson \
//!     npx jest --silent --watchman=false --roots "$PWD" --roots "$M/cases" --testTimeout=180000 --runTestsByPath "$M/cases/chat-turn-mode-routes.test.ts"
//! Run:
//!   QT_ORACLE_CHAT_TURN_MODE_ROUTES=/tmp/oracle-chat-turn-mode-routes.ndjson \
//!     cargo test -p quilltap-harness --test chat_turn_mode_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::chat_storyteller::{
    self, StorytellerDriver, StorytellerDriverError, StorytellerFuture, StorytellerRunRequest,
};
use quilltap_core::api::salon;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::storyteller_rounds::StorytellerRoundsRepository;
use quilltap_core::host_zone::TimeZone;
use quilltap_core::services::storyteller_rounds::{plan_storyteller_run, prepare_ask_anyway};
use quilltap_core::storyteller::types::{
    StorytellerBid, StorytellerClosedReason, StorytellerRoundStatus,
};
use quilltap_core::weighted_random::DrawSource;
use serde::Deserialize;
use serde_json::{json, Value};

/// The corpus must not shrink silently (the oracle's `CASES`).
const MIN_ROWS: usize = 49;

/// CARVE `ZOD_DETAILS` — v4's 400 `Validation error` body carries Zod's
/// `details` issue array; v5 answers the bare `{error}` (the standing
/// project-wide error-envelope deferral, `salon.rs::chat_update`'s own
/// note). Only `error` is compared on an error body.
const ZOD_DETAILS: &str = "ZOD_DETAILS";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/salon.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

const SOLO: &str = "c1000000-0000-4000-8000-000000000001";
const GROUP: &str = "c1000000-0000-4000-8000-000000000002";
const MISSING_CHAT: &str = "c1000000-0000-4000-8000-0000000000ff";
const ARIA_P: &str = "b2000000-0000-4000-8000-000000000001";
const BRAM_P: &str = "b2000000-0000-4000-8000-000000000002";
const QUIET_P: &str = "b2000000-0000-4000-8000-000000000009";
const OPEN_ROUND: &str = "5e000000-0000-4000-8000-000000000001";
const CLOSED_ROUND: &str = "5e000000-0000-4000-8000-000000000002";
const SOLO_ROUND: &str = "5e000000-0000-4000-8000-000000000003";
const TRIGGER: &str = "d2000000-0000-4000-8000-000000000004";

/// Every timestamp the fixture seeds or the plants pin; any other is minted
/// by the run and placeholdered (`<ts>`) on both sides.
const PINNED_TS: &[&str] = &[
    "2026-02-01T00:00:00.000Z",
    "2026-02-02T00:00:00.000Z",
    "2026-02-03T00:00:00.000Z",
    "2026-02-03T00:00:01.000Z",
    "2026-02-03T00:00:02.000Z",
];

fn bids() -> Vec<StorytellerBid> {
    serde_json::from_value(json!([
        {
            "participantId": ARIA_P,
            "characterId": "a1000000-0000-4000-8000-000000000001",
            "characterName": "Aria",
            "status": "bid",
            "statedTier": "act",
            "tier": "destroy",
            "tierRaised": true,
            "tools": ["delete_file"],
            "plan": "Burn the map before Bram can read it.",
            "reason": null,
            "talkativeness": 0.7,
            "silent": false,
            "askedAt": "2026-02-03T00:00:00.000Z",
            "answeredAt": "2026-02-03T00:00:01.000Z"
        },
        {
            "participantId": BRAM_P,
            "characterId": "a1000000-0000-4000-8000-000000000002",
            "characterName": "Bram",
            "status": "bid",
            "statedTier": "simple",
            "tier": "simple",
            "tierRaised": false,
            "tools": [],
            "plan": "Grumble about the weather.",
            "reason": null,
            "talkativeness": 0.5,
            "silent": false,
            "askedAt": "2026-02-03T00:00:00.000Z",
            "answeredAt": "2026-02-03T00:00:02.000Z"
        },
        {
            "participantId": QUIET_P,
            "characterId": "a1000000-0000-4000-8000-000000000004",
            "characterName": "Dax",
            "status": "kept-quiet",
            "statedTier": null,
            "tier": null,
            "tierRaised": false,
            "tools": [],
            "plan": null,
            "reason": "Kept quiet this round (talkativeness)",
            "talkativeness": 0.1,
            "silent": true,
            "askedAt": null,
            "answeredAt": null
        }
    ]))
    .expect("the bid plant parses")
}

#[derive(Clone)]
struct RoundPlant {
    id: &'static str,
    chat_id: &'static str,
    created_at: &'static str,
    trigger: Option<&'static str>,
    close_as: Option<StorytellerClosedReason>,
}

fn open_round() -> RoundPlant {
    RoundPlant {
        id: OPEN_ROUND,
        chat_id: GROUP,
        created_at: "2026-02-03T00:00:00.000Z",
        trigger: Some(TRIGGER),
        close_as: None,
    }
}
fn closed_round() -> RoundPlant {
    RoundPlant {
        id: CLOSED_ROUND,
        chat_id: GROUP,
        created_at: "2026-02-02T00:00:00.000Z",
        trigger: Some(TRIGGER),
        close_as: Some(StorytellerClosedReason::LetLie),
    }
}
fn solo_round() -> RoundPlant {
    RoundPlant {
        id: SOLO_ROUND,
        chat_id: SOLO,
        ..open_round()
    }
}

enum Call {
    Put(Value),
    GetChat,
    GetRound,
    Lie(Value),
    Run(Value),
    Ask(Value),
}

struct Case {
    name: &'static str,
    chat_id: &'static str,
    call: Call,
    turn_mode: Option<Option<&'static str>>,
    invite_filter: Option<i64>,
    rounds: Vec<RoundPlant>,
}

fn case(name: &'static str, call: Call) -> Case {
    Case {
        name,
        chat_id: GROUP,
        call,
        turn_mode: None,
        invite_filter: None,
        rounds: Vec::new(),
    }
}
impl Case {
    fn mode(mut self, m: &'static str) -> Self {
        self.turn_mode = Some(Some(m));
        self
    }
    fn filter(mut self, f: i64) -> Self {
        self.invite_filter = Some(f);
        self
    }
    fn rounds(mut self, r: Vec<RoundPlant>) -> Self {
        self.rounds = r;
        self
    }
    fn on(mut self, chat_id: &'static str) -> Self {
        self.chat_id = chat_id;
        self
    }
}

/// The corpus, row for row the oracle's `CASES`.
fn cases() -> Vec<Case> {
    use Call::*;
    let mut null_trigger = open_round();
    null_trigger.trigger = None;
    let early_open = RoundPlant {
        created_at: "2026-02-01T00:00:00.000Z",
        ..open_round()
    };
    vec![
        case("put_storyteller", Put(json!({"chat": {}, "turnMode": "storyteller"}))),
        case("put_rotation_from_null", Put(json!({"chat": {}, "turnMode": "rotation"}))),
        case("put_rotation_from_storyteller", Put(json!({"chat": {}, "turnMode": "rotation"})))
            .mode("storyteller"),
        case("put_storyteller_again_keeps_round", Put(json!({"chat": {}, "turnMode": "storyteller"})))
            .mode("storyteller")
            .rounds(vec![open_round()]),
        case("put_leave_with_open_round", Put(json!({"chat": {}, "turnMode": "rotation"})))
            .mode("storyteller")
            .rounds(vec![closed_round(), open_round()]),
        case("put_leave_with_closed_round_only", Put(json!({"chat": {}, "turnMode": "rotation"})))
            .mode("storyteller")
            .rounds(vec![closed_round()]),
        case(
            "put_with_bag_and_mode",
            Put(json!({"chat": {"title": "Told By The Fire"}, "turnMode": "storyteller"})),
        ),
        case("put_null_refused", Put(json!({"chat": {}, "turnMode": null}))),
        case("put_unknown_refused", Put(json!({"chat": {}, "turnMode": "x"}))),
        case("put_number_refused", Put(json!({"chat": {}, "turnMode": 5}))),
        case(
            "put_null_with_bag_writes_nothing",
            Put(json!({"chat": {"title": "Should Not Land"}, "turnMode": null})),
        )
        .mode("storyteller")
        .rounds(vec![open_round()]),
        case("put_absent_no_change", Put(json!({"chat": {}}))).mode("storyteller"),
        case("bag_filter_true", Put(json!({"chat": {"storytellerInviteFilter": true}}))),
        case("bag_filter_false", Put(json!({"chat": {"storytellerInviteFilter": false}}))).filter(1),
        case("bag_filter_string_refused", Put(json!({"chat": {"storytellerInviteFilter": "yes"}}))),
        case("bag_filter_null_refused", Put(json!({"chat": {"storytellerInviteFilter": null}}))),
        case(
            "bag_filter_string_with_title_refused",
            Put(json!({"chat": {"title": "Should Not Land", "storytellerInviteFilter": "yes"}})),
        ),
        case("get_chat_null_mode", GetChat),
        case("get_chat_storyteller_filter_on", GetChat).mode("storyteller").filter(1),
        case("round_none", GetRound),
        case("round_open", GetRound).rounds(vec![open_round()]),
        case("round_latest_closed", GetRound).rounds(vec![closed_round()]),
        case("round_open_beats_later_closed", GetRound).rounds(vec![early_open, closed_round()]),
        case("round_other_chats_round_unseen", GetRound).rounds(vec![solo_round()]),
        case("round_null_trigger", GetRound).rounds(vec![null_trigger]),
        case("round_missing_chat", GetRound).on(MISSING_CHAT),
        case("lie_success", Lie(json!({"roundId": OPEN_ROUND}))).rounds(vec![open_round()]),
        case("lie_closed_round", Lie(json!({"roundId": CLOSED_ROUND}))).rounds(vec![closed_round()]),
        case("lie_missing_round", Lie(json!({"roundId": OPEN_ROUND}))),
        case("lie_other_chats_round", Lie(json!({"roundId": SOLO_ROUND}))).rounds(vec![solo_round()]),
        case("lie_no_round_id", Lie(json!({}))).rounds(vec![open_round()]),
        case("lie_round_id_number", Lie(json!({"roundId": 7}))).rounds(vec![open_round()]),
        case("lie_missing_chat", Lie(json!({"roundId": OPEN_ROUND}))).on(MISSING_CHAT),
        case(
            "run_closed_round",
            Run(json!({"roundId": CLOSED_ROUND, "participantIds": [BRAM_P]})),
        )
        .rounds(vec![closed_round()]),
        case("run_empty_selection", Run(json!({"roundId": OPEN_ROUND, "participantIds": []})))
            .rounds(vec![open_round()]),
        case("run_not_a_bid", Run(json!({"roundId": OPEN_ROUND, "participantIds": [QUIET_P]})))
            .rounds(vec![open_round()]),
        case("run_unknown_seat", Run(json!({"roundId": OPEN_ROUND, "participantIds": ["nobody"]})))
            .rounds(vec![open_round()]),
        case(
            "run_destroy_with_others",
            Run(json!({"roundId": OPEN_ROUND, "participantIds": [BRAM_P, ARIA_P], "order": "simultaneous", "keepOrder": true})),
        )
        .rounds(vec![open_round()]),
        case(
            "run_bad_order",
            Run(json!({"roundId": OPEN_ROUND, "participantIds": [BRAM_P], "order": "random"})),
        )
        .rounds(vec![open_round()]),
        case("run_missing_participant_ids", Run(json!({"roundId": OPEN_ROUND})))
            .rounds(vec![open_round()]),
        case(
            "run_keep_order_string",
            Run(json!({"roundId": OPEN_ROUND, "participantIds": [BRAM_P], "keepOrder": "yes"})),
        )
        .rounds(vec![open_round()]),
        // v4's schema: `.optional()`, not `.nullable()` — an explicit `null`
        // is the ZodError arm (the unify review's should-fix).
        case(
            "run_order_null",
            Run(json!({"roundId": OPEN_ROUND, "participantIds": [BRAM_P], "order": null})),
        )
        .rounds(vec![open_round()]),
        case(
            "run_keep_order_null",
            Run(json!({"roundId": OPEN_ROUND, "participantIds": [BRAM_P], "keepOrder": null})),
        )
        .rounds(vec![open_round()]),
        case("run_participant_id_number", Run(json!({"roundId": OPEN_ROUND, "participantIds": [1]})))
            .rounds(vec![open_round()]),
        case("run_missing_chat", Run(json!({"roundId": OPEN_ROUND, "participantIds": [BRAM_P]})))
            .on(MISSING_CHAT),
        case("ask_closed_round", Ask(json!({"roundId": CLOSED_ROUND, "participantId": QUIET_P})))
            .rounds(vec![closed_round()]),
        case("ask_seat_not_on_round", Ask(json!({"roundId": OPEN_ROUND, "participantId": "nobody"})))
            .rounds(vec![open_round()]),
        case("ask_missing_participant_id", Ask(json!({"roundId": OPEN_ROUND})))
            .rounds(vec![open_round()]),
        case("ask_missing_chat", Ask(json!({"roundId": OPEN_ROUND, "participantId": QUIET_P})))
            .on(MISSING_CHAT),
    ]
}

/// The host driver's refusal half (`quilltap-host`'s `storyteller_spine.rs`):
/// plan / prepare, map a refusal through the core's `From<RoundServiceError>`
/// (the host's `refusal` is the same two arms). A row that plans PAST the
/// refusals would reach a model — that is a corpus error, so it panics.
struct RefusalDriver<'a> {
    db: &'a Db,
}

impl StorytellerDriver for RefusalDriver<'_> {
    fn run(&self, req: StorytellerRunRequest) -> StorytellerFuture<'_, ()> {
        Box::pin(async move {
            let planned = plan_storyteller_run(
                self.db,
                &req.chat_id,
                &req.round_id,
                &req.participant_ids,
                req.order,
                req.keep_order,
                &DrawSource::constant(0.5),
            )
            .await;
            match planned {
                Err(e) => Err(StorytellerDriverError::from(e)),
                Ok(_) => panic!("a refusal row planned a run — it would reach a model"),
            }
        })
    }
    fn ask(
        &self,
        chat_id: String,
        round_id: String,
        participant_id: String,
    ) -> StorytellerFuture<'_, StorytellerBid> {
        Box::pin(async move {
            match prepare_ask_anyway(self.db, &chat_id, &round_id, &participant_id).await {
                Err(e) => Err(StorytellerDriverError::from(e)),
                Ok(_) => panic!("a refusal row prepared an ask — it would reach a model"),
            }
        })
    }
}

/// A PROCESS-GLOBAL capture: the repository writes run on the WRITER thread.
fn global_capture() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    static BUF: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<String>>>> =
        std::sync::OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// The oracle's `LINE_PREFIXES`.
const LINE_PREFIXES: &[&str] = &[
    "[Chats v1] Turn mode set",
    "Storyteller round closed unrun",
    "Storyteller round let lie",
    "[Storyteller] ",
];

/// The family's lines since the last call (`LEVEL message k=v`), buffer cleared.
fn take_lines() -> Vec<String> {
    let buf = global_capture();
    let mut b = buf.lock().unwrap();
    let out = b
        .iter()
        .filter_map(|l| {
            let (level, rest) = l.split_once(' ')?;
            let (_target, rest) = rest.split_once(' ')?;
            LINE_PREFIXES
                .iter()
                .any(|p| rest.starts_with(p))
                .then(|| format!("{level} {rest}"))
        })
        .collect();
    b.clear();
    out
}

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
fn scrub(s: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap());
    re.replace_all(s, |caps: &regex::Captures| {
        let m = &caps[0];
        if PINNED_TS.contains(&m) {
            m.to_string()
        } else {
            "<ts>".to_string()
        }
    })
    .into_owned()
}
/// Byte form: numbers canonicalised, KEY ORDER KEPT, minted stamps scrubbed.
fn bytes(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    scrub(&serde_json::to_string(&v).unwrap())
}
fn rows_of(dump: &Value) -> Value {
    let mut v = dump.get("rows").cloned().unwrap_or(Value::Array(vec![]));
    if let Value::Array(rows) = &mut v {
        rows.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_string());
        for r in rows.iter_mut() {
            if let Some(o) = r.as_object_mut() {
                o.remove("renderedHtml");
            }
        }
    }
    v
}
/// The ordered keys around `isPaused` (the W1 placement) with their values.
fn ispaused_window(chat: &Value) -> Value {
    let Some(o) = chat.as_object() else {
        return Value::Null;
    };
    let keys: Vec<&String> = o.keys().collect();
    let Some(i) = keys.iter().position(|k| *k == "isPaused") else {
        return json!("<no isPaused>");
    };
    let window: Vec<Value> = keys[i.saturating_sub(1)..(i + 4).min(keys.len())]
        .iter()
        .map(|k| json!([k, o[*k]]))
        .collect();
    Value::Array(window)
}

fn status_and_body(r: &Response) -> (i64, Value) {
    match r {
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked | ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => {
            let v = serde_json::to_value(other).unwrap();
            (200, v.get("data").cloned().unwrap_or(Value::Null))
        }
    }
}

fn fresh_db(pepper: &str, tag: &str) -> (tempfile::TempDir, Db) {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-turn-mode-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("salon-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("salon-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    db.write_blocking(|w| {
        quilltap_core::test_support::ensure_p4d171_columns(w.main().connection());
        Ok(())
    })
    .expect("heal the fixture copy");
    (scratch, db)
}

fn plant(db: &Db, c: &Case) {
    let chat_id = c.chat_id.to_string();
    let turn_mode = c.turn_mode;
    let filter = c.invite_filter;
    let rounds = c.rounds.clone();
    db.write_blocking(move |w| {
        let conn = w.main().connection();
        if let Some(m) = turn_mode {
            conn.execute(
                r#"UPDATE chats SET "turnMode" = ?1 WHERE id = ?2"#,
                rusqlite::params![m, chat_id],
            )?;
        }
        if let Some(f) = filter {
            conn.execute(
                r#"UPDATE chats SET "storytellerInviteFilter" = ?1 WHERE id = ?2"#,
                rusqlite::params![f, chat_id],
            )?;
        }
        let repo = StorytellerRoundsRepository::new(conn);
        for r in &rounds {
            repo.create(
                r.chat_id,
                r.trigger,
                StorytellerRoundStatus::Open,
                bids(),
                r.id,
                r.created_at,
            )?;
            if let Some(reason) = r.close_as {
                repo.mark_closed(r.id, reason, None, &quilltap_core::clock::now_iso())?;
            }
        }
        Ok(())
    })
    .expect("plant the case");
}

fn body_field(body: &Value, key: &str) -> Value {
    body.get(key).cloned().unwrap_or(Value::Null)
}
fn opt(body: &Value, key: &str) -> Option<Value> {
    body.get(key).cloned()
}

/// The transport's three-state decode of an `.optional()` (never
/// `.nullable()`) key: absent → `None`, `null` → `Some(None)`, a value →
/// `Some(Some(v))` — `api::types`' `double_option`.
fn opt_tri(body: &Value, key: &str) -> Option<Option<Value>> {
    body.get(key).map(|v| (!v.is_null()).then(|| v.clone()))
}

#[test]
fn chat_turn_mode_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHAT_TURN_MODE_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_CHAT_TURN_MODE_ROUTES (see test header).");
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    let corpus = cases();
    assert!(
        oracle.len() >= MIN_ROWS && corpus.len() >= MIN_ROWS,
        "the corpus shrank: oracle {} / v5 {} rows (minimum {MIN_ROWS})",
        oracle.len(),
        corpus.len()
    );
    assert_eq!(
        oracle.len(),
        corpus.len(),
        "oracle and v5 corpora differ in size"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let uid = spec.user_id.clone();
    global_capture();

    let mut failed: Vec<String> = Vec::new();
    let mut compared = 0usize;
    for c in &corpus {
        let want = oracle
            .get(c.name)
            .unwrap_or_else(|| panic!("oracle has no row `{}` — regenerate it", c.name));
        let (_scratch, db) = fresh_db(&spec.test_pepper_base64, c.name);
        plant(&db, c);
        take_lines();
        let resp = match &c.call {
            Call::Put(body) => rt.block_on(salon::chat_update(
                &db,
                &uid,
                c.chat_id,
                &body_field(body, "chat"),
                None,
                None,
                None,
                None,
                opt(body, "turnMode").as_ref(),
            )),
            Call::GetChat => {
                rt.block_on(salon::chat_get(&db, &uid, c.chat_id, None, &TimeZone::UTC))
            }
            Call::GetRound => chat_storyteller::chat_storyteller_round(&db, c.chat_id),
            Call::Lie(body) => rt.block_on(chat_storyteller::chat_storyteller_lie(
                &db,
                c.chat_id,
                &body_field(body, "roundId"),
            )),
            Call::Run(body) => {
                let driver = RefusalDriver { db: &db };
                rt.block_on(chat_storyteller::chat_storyteller_run(
                    &db,
                    Some(&driver),
                    c.chat_id,
                    &body_field(body, "roundId"),
                    &body_field(body, "participantIds"),
                    &opt_tri(body, "order"),
                    &opt_tri(body, "keepOrder"),
                ))
            }
            Call::Ask(body) => {
                let driver = RefusalDriver { db: &db };
                rt.block_on(chat_storyteller::chat_storyteller_ask(
                    &db,
                    Some(&driver),
                    c.chat_id,
                    &body_field(body, "roundId"),
                    &body_field(body, "participantId"),
                ))
            }
        };
        // The writer thread's lines land before the write's future resolves;
        // nothing is left in flight once the call returned.
        let got_lines = take_lines();
        let (got_status, got_body) = status_and_body(&resp);
        let chats = db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .unwrap();
        let rounds = db
            .read_main(|conn| dump_table_json_conn(conn, "storyteller_rounds", "id"))
            .unwrap();
        drop(db);

        let name = c.name;
        let mut fail = |what: &str, got: String, want: String| {
            eprintln!("[{name}] {what} MISMATCH:\n  GOT : {got}\n  WANT: {want}");
            failed.push(format!("{name}/{what}"));
        };
        let want_status = want["status"].as_i64().unwrap();
        if got_status != want_status {
            fail("status", got_status.to_string(), want_status.to_string());
        }
        let want_body = &want["body"];
        if want_body.get("error").is_some() {
            // [ZOD_DETAILS] — only `error` is compared.
            let g = bytes(&json!({ "error": got_body.get("error") }));
            let w = bytes(&json!({ "error": want_body.get("error") }));
            if g != w {
                fail("body", g, w);
            }
        } else if matches!(c.call, Call::Put(_)) {
            let g = &got_body["chat"];
            // (`SIBLING_DIAGRAM_STYLE` retired at the unification: v5's PUT
            // echo now carries the sibling lane's `diagramStyle`.)
            let w = &want_body["chat"];
            let (gs, ws) = (bytes(&sorted(g)), bytes(&sorted(w)));
            if gs != ws {
                fail("body.chat(sorted)", gs, ws);
            }
            let (gw, ww) = (bytes(&ispaused_window(g)), bytes(&ispaused_window(w)));
            if gw != ww {
                fail("body.chat(isPaused window)", gw, ww);
            }
        } else if matches!(c.call, Call::GetChat) {
            let (g, w) = (&got_body["chat"], &want_body["chat"]);
            let (gw, ww) = (bytes(&ispaused_window(g)), bytes(&ispaused_window(w)));
            if gw != ww {
                fail("body.chat(isPaused window)", gw, ww);
            }
            for key in ["turnMode", "storytellerInviteFilter"] {
                let (gk, wk) = (bytes(&g[key]), bytes(&w[key]));
                if gk != wk {
                    fail(&format!("body.chat.{key}"), gk, wk);
                }
            }
        } else {
            let (g, w) = (bytes(&got_body), bytes(want_body));
            if g != w {
                fail("body", g, w);
            }
        }
        for (section, got, want_dump) in [
            ("chats", &chats, &want["tables"]["chats"]),
            (
                "storyteller_rounds",
                &rounds,
                &want["tables"]["storytellerRounds"],
            ),
        ] {
            let (g, w) = (bytes(&rows_of(got)), bytes(&rows_of(want_dump)));
            if g != w {
                fail(section, g, w);
            }
        }
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        if got_lines != want_lines {
            fail("lines", format!("{got_lines:?}"), format!("{want_lines:?}"));
        }
        compared += 1;
    }
    eprintln!("chat_turn_mode_routes: {compared} rows compared; carves {ZOD_DETAILS}");
    assert!(
        failed.is_empty(),
        "chat_turn_mode_routes FAILED: {failed:#?}"
    );
}
