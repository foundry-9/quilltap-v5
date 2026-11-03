//! Tier-2 differential: the storyteller round's PLAN and RUN (P4.D343, work
//! order item 24 — `plan_storyteller_run_equivalence`).
//!
//! Drives the SAME case sequence through v4's REAL `planStorytellerRun` +
//! `runStorytellerRound` (`lib/storyteller/round.service.ts`, the oracle case
//! `harness/oracle/cases/plan-storyteller-run.ts`) and through v5's
//! [`plan_storyteller_run`] + [`run_storyteller_round`], over copies of ONE
//! fixture pair built by v4's real code (`build-plan-storyteller-run-
//! fixture.ts`). `runTurn` is CANNED on both sides (the case's outcome per
//! participant), and every call it receives is recorded. The cases and their
//! inputs live in the ORACLE only: every NDJSON row carries its `input`, which
//! this side replays — in order, on one database copy, as the oracle did.
//!
//! Per case, compared byte for byte (`serde_json::to_string` — a `Map ==` is
//! key-order blind):
//!
//!   - **`plan`** — the returned round (v4's read shape) and the planned turns
//!     (`participantId`, `characterName`, `concurrent`, `profileKey`, `bid`),
//!     or the refusal's `{ message, status }`.
//!   - **`run`** — every `runTurn` call (`participantId` + the context:
//!     `concurrentFollower`, `bidRecord`, `effectCeiling`, `spokenSince`), the
//!     `onTurnStart` / `onTurnEnd` events with their depths, the ending, and
//!     the last outcome.
//!   - **`drawsUsed`** — how many draws the rotation consumed (`resolveCycle
//!     Order` draws only when no usable cycle is stored; `keepOrder` never
//!     consults it — the spy-count of `7d7e38d2f` row 2, measured on a chat
//!     with NO stored cycle so a consult would also PERSIST one).
//!   - **`roundRow`** — the raw `storyteller_rounds` row after the case
//!     (`markRunning` → `markClosed('ran')`: `status`, `bids` `[]`,
//!     `selection`, `closedReason`, `closedAt`).
//!   - **`chats`** — both chats' raw rotation cells (the drawn cycle PERSISTED).
//!   - **`logs`** — the pinned lines (`Storyteller run keeps the operator's
//!     order`, `Storyteller round running`, `Storyteller round closed`),
//!     rendered as the capture rig renders them, less the target (an array
//!     field renders through `Debug`, as v5's `?`-sigil fields do). Every one
//!     is logged on the CALLER, never inside a `Db::write` closure, so the
//!     thread-scoped `captured_with` sees them (the runtime is current-thread).
//!
//! ## Normalisation (both sides)
//!
//! v4 stamps `updatedAt` / `closedAt` / `selection.chosenAt` with the live
//! clock (`_update`, `markClosed`, `nowIso()`); v5 reads its clock. Any such
//! value NOT among the oracle header's pinned timestamps becomes `<ts>` —
//! presence and null-ness still compare.
//!
//! ## Named carves
//!
//! - `CYCLE_PERSIST_DEBUG_SILENT_IN_V4` — v4's `turnManagerLogger.debug` is a
//!   NO-OP on the server (`lib/chat/turn-manager/logger.ts`: "Debug logging
//!   disabled"), so `[Turn Manager] Cycle order persisted` never reaches v4's
//!   logger; v5's `cycle_order::log_cycle_order_persisted` logs it at DEBUG.
//!   The oracle pins the message (and records none); this side drops v5's
//!   line by name. The PERSIST itself compares through `chats`.
//! - `CONCURRENT_GROUP_IS_THE_HOSTS` — v5's [`run_storyteller_round`] runs the
//!   sequential part only (the per-profile semaphore and the concurrency are
//!   the host's, item 31). Cases whose plan holds a concurrent turn are
//!   compared on the PLAN only — the oracle runs none of them.
//!
//! Generate the fixture + oracle (Node 24, from the PINNED v4 worktree —
//! §R.3 PIN REQUIRED):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_PSR_MAIN=/tmp/qt-psr-main.db QT_FIXTURE_PSR_MOUNT=/tmp/qt-psr-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-plan-storyteller-run-fixture.ts
//!   TZ=UTC QT_FIXTURE_PSR_MAIN=/tmp/qt-psr-main.db QT_FIXTURE_PSR_MOUNT=/tmp/qt-psr-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/plan-storyteller-run.ts \
//!     > /tmp/oracle-plan-storyteller-run.ndjson
//! Run:
//!   QT_ORACLE_PLAN_STORYTELLER_RUN=/tmp/oracle-plan-storyteller-run.ndjson \
//!   QT_FIXTURE_PSR_MAIN=/tmp/qt-psr-main.db QT_FIXTURE_PSR_MOUNT=/tmp/qt-psr-mount.db \
//!     cargo test -p quilltap-harness --test plan_storyteller_run_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::storyteller_rounds::StorytellerRoundsRepository;
use quilltap_core::services::storyteller_rounds::{
    plan_storyteller_run, run_storyteller_round, RoundServiceError, RoundTurnRunner,
    StorytellerPlannedTurn, StorytellerTurnOutcome,
};
use quilltap_core::storyteller::types::{
    StorytellerBid, StorytellerOrder, StorytellerRoundStatus, StorytellerTurnContext,
};
use quilltap_core::test_support::captured_with;
use quilltap_core::weighted_random::DrawSource;
use serde_json::{json, Map, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const TRIGGER: &str = "d6000000-0000-4000-8000-0000000000c2";
const MIN_CASES: usize = 23;

const PINNED: &[&str] = &[
    "Storyteller run keeps the operator's order",
    "Storyteller round running",
    "Storyteller round closed",
    "[Turn Manager] Cycle order persisted",
];
/// `CYCLE_PERSIST_DEBUG_SILENT_IN_V4` (header).
const CYCLE_PERSIST_DEBUG_SILENT_IN_V4: &str = "[Turn Manager] Cycle order persisted";

fn mask_ts(v: &mut Value, pinned: &HashSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if (k == "updatedAt" || k == "closedAt" || k == "chosenAt")
                    && x.as_str().is_some_and(|s| !pinned.contains(s))
                {
                    *x = Value::String("<ts>".into());
                } else if k == "selection" && x.is_string() {
                    // The raw cell: JSON TEXT carrying `chosenAt`.
                    *x = Value::String(mask_chosen_at(x.as_str().unwrap(), pinned));
                } else {
                    mask_ts(x, pinned);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| mask_ts(x, pinned)),
        _ => {}
    }
}

/// `"chosenAt":"<iso>"` inside a stored JSON cell → `"chosenAt":"<ts>"`
/// unless pinned (a text edit — re-serialising would hide a key-order drift).
fn mask_chosen_at(cell: &str, pinned: &HashSet<String>) -> String {
    const KEY: &str = "\"chosenAt\":\"";
    let Some(start) = cell.find(KEY).map(|i| i + KEY.len()) else {
        return cell.to_string();
    };
    let Some(len) = cell[start..].find('"') else {
        return cell.to_string();
    };
    let value = &cell[start..start + len];
    if pinned.contains(value) {
        return cell.to_string();
    }
    format!("{}<ts>{}", &cell[..start], &cell[start + len..])
}

/// Raw rows, cells as stored — v4's `better-sqlite3` `.get()` / `.all()` shape.
fn raw_rows(conn: &rusqlite::Connection, sql: &str, args: &[&str]) -> Vec<Value> {
    let mut stmt = conn.prepare(sql).expect("prepare raw select");
    let names: Vec<String> = stmt.column_names().iter().map(|n| n.to_string()).collect();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            let mut m = Map::new();
            for (i, name) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                    rusqlite::types::ValueRef::Real(f) => Value::from(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        Value::String(String::from_utf8_lossy(t).into_owned())
                    }
                    rusqlite::types::ValueRef::Blob(b) => Value::from(b.to_vec()),
                };
                m.insert(name.clone(), v);
            }
            Ok(Value::Object(m))
        })
        .expect("query raw rows");
    rows.map(|r| r.expect("raw row")).collect()
}

/// The canned `runTurn` + hooks (the oracle's `spec.outcomes` / `stopDuring`).
struct Canned {
    outcomes: HashMap<String, Value>,
    stop_during: Option<String>,
    stopped: bool,
    calls: Vec<Value>,
    events: Vec<Value>,
}

fn context_json(c: &StorytellerTurnContext) -> Value {
    json!({
        "concurrentFollower": c.concurrent_follower,
        "bidRecord": {
            "roundId": c.bid_record.round_id,
            "tier": serde_json::to_value(c.bid_record.tier).expect("tier"),
            "plan": c.bid_record.plan,
            "toolsNamed": c.bid_record.tools_named,
        },
        "effectCeiling": c.effect_ceiling.as_str(),
        "spokenSince": c.spoken_since,
    })
}

fn outcome_json(o: &StorytellerTurnOutcome) -> Value {
    json!({ "spoke": o.spoke, "yielded": o.yielded, "messageId": o.message_id })
}

impl RoundTurnRunner for Canned {
    async fn run_turn(
        &mut self,
        turn: &StorytellerPlannedTurn,
        context: StorytellerTurnContext,
    ) -> StorytellerTurnOutcome {
        self.calls.push(json!({
            "participantId": turn.participant_id,
            "context": context_json(&context),
        }));
        if self.stop_during.as_deref() == Some(turn.participant_id.as_str()) {
            self.stopped = true;
        }
        let o = &self.outcomes[&turn.participant_id];
        StorytellerTurnOutcome {
            spoke: o["spoke"].as_bool().expect("spoke"),
            yielded: o["yielded"].as_bool().expect("yielded"),
            message_id: o["messageId"].as_str().map(str::to_string),
            has_scene_tracking: false,
        }
    }
    fn on_turn_start(&mut self, turn: &StorytellerPlannedTurn, depth: usize) {
        self.events
            .push(json!(["start", turn.participant_id, depth]));
    }
    fn on_turn_end(
        &mut self,
        turn: &StorytellerPlannedTurn,
        depth: usize,
        outcome: &StorytellerTurnOutcome,
    ) {
        self.events
            .push(json!(["end", turn.participant_id, depth, outcome.spoke]));
    }
    fn is_stopped(&self) -> bool {
        self.stopped
    }
}

/// One case through v5 → the oracle row's compared fields.
async fn run_case(db: &Db, i: usize, input: &Value, ts_for: &str) -> Value {
    if !input["round"].is_null() {
        let r = &input["round"];
        let status: StorytellerRoundStatus =
            serde_json::from_value(r["status"].clone()).expect("status");
        let bids: Vec<StorytellerBid> = serde_json::from_value(r["bids"].clone()).expect("bids");
        let (chat, id, ts) = (
            r["chatId"].as_str().unwrap().to_string(),
            r["id"].as_str().unwrap().to_string(),
            ts_for.to_string(),
        );
        db.write(move |w| {
            StorytellerRoundsRepository::new(w.main().connection()).create(
                &chat,
                Some(TRIGGER),
                status,
                bids,
                &id,
                &ts,
            )
        })
        .await
        .unwrap_or_else(|e| panic!("case {i}: plant round: {e}"));
    }

    let p = &input["plan"];
    let chat_id = p["chatId"].as_str().unwrap();
    let round_id = p["roundId"].as_str().unwrap();
    let ids: Vec<String> = p["participantIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let order: StorytellerOrder = serde_json::from_value(p["order"].clone()).expect("order");
    let keep_order = p["keepOrder"].as_bool().unwrap_or(false);

    // `DrawSource::sequence` semantics, counted.
    let seq: Vec<f64> = input["draws"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect())
        .unwrap_or_default();
    let used = Arc::new(AtomicUsize::new(0));
    let used_in = used.clone();
    let draws = DrawSource::from_fn(move || {
        let n = used_in.fetch_add(1, Ordering::SeqCst);
        if seq.is_empty() {
            0.0
        } else {
            seq[n.min(seq.len() - 1)]
        }
    });

    let planned =
        plan_storyteller_run(db, chat_id, round_id, &ids, order, keep_order, &draws).await;
    let draws_used = used.load(Ordering::SeqCst);
    let (plan, planned) = match planned {
        Ok((round, turns)) => {
            let turns_json: Vec<Value> = turns
                .iter()
                .map(|t| {
                    json!({
                        "participantId": t.participant_id,
                        "characterName": t.character_name,
                        "concurrent": t.concurrent,
                        "profileKey": t.profile_key,
                        "bid": serde_json::to_value(&t.bid).expect("bid"),
                    })
                })
                .collect();
            (
                json!({ "round": serde_json::to_value(&round).expect("round"), "turns": turns_json }),
                Some((round, turns)),
            )
        }
        Err(RoundServiceError::Refused(e)) => (
            json!({ "refused": { "message": e.message, "status": e.status } }),
            None,
        ),
        Err(RoundServiceError::Db(e)) => (json!({ "threw": e.to_string() }), None),
    };

    let mut run = Value::Null;
    if let (Some((round, turns)), Some(spec)) = (planned, input.get("run")) {
        assert!(
            turns.iter().all(|t| !t.concurrent),
            "case {i}: CONCURRENT_GROUP_IS_THE_HOSTS — the corpus runs no concurrent plan"
        );
        let mut canned = Canned {
            outcomes: spec["outcomes"]
                .as_object()
                .expect("outcomes")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            stop_during: spec["stopDuring"].as_str().map(str::to_string),
            stopped: false,
            calls: Vec::new(),
            events: Vec::new(),
        };
        let (ended, last) = run_storyteller_round(db, chat_id, &round, &turns, order, &mut canned)
            .await
            .unwrap_or_else(|e| panic!("case {i}: run: {e}"));
        run = json!({
            "calls": canned.calls,
            "events": canned.events,
            "ended": ended.as_str(),
            "lastOutcome": last.as_ref().map(outcome_json),
        });
    }

    let round_row = if input["round"].is_null() {
        Value::Null
    } else {
        let id = input["round"]["id"].as_str().unwrap().to_string();
        db.read_main(move |c| {
            Ok(raw_rows(
                c,
                r#"SELECT * FROM "storyteller_rounds" WHERE "id" = ?"#,
                &[&id],
            )
            .into_iter()
            .next()
            .unwrap_or(Value::Null))
        })
        .expect("round row")
    };
    let chats = db
        .read_main(|c| {
            Ok(raw_rows(
                c,
                r#"SELECT "id", "cycleOrderParticipantIds", "spokenThisCycleParticipantIds" FROM "chats" ORDER BY "id""#,
                &[],
            ))
        })
        .expect("chats");

    json!({
        "plan": plan,
        "run": run,
        "drawsUsed": draws_used,
        "roundRow": round_row,
        "chats": chats,
    })
}

/// A v4 log row → `<LEVEL> <message> k=v …` (the capture rig's rendering less
/// its target; an array renders as v5's `?`-sigil `Debug`).
fn v4_line(log: &Value) -> String {
    let mut line = format!(
        "{} {}",
        log["level"].as_str().unwrap_or("").to_ascii_uppercase(),
        log["message"].as_str().unwrap_or("")
    );
    for (k, v) in log["fields"].as_object().into_iter().flatten() {
        match v {
            Value::String(s) => line.push_str(&format!(" {k}={s}")),
            Value::Array(a) => {
                let items: Vec<String> = a
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .map(|s| format!("{s:?}"))
                            .unwrap_or(x.to_string())
                    })
                    .collect();
                line.push_str(&format!(" {k}=[{}]", items.join(", ")));
            }
            other => line.push_str(&format!(" {k}={other}")),
        }
    }
    line
}

/// A v5 capture line → the same shape (target dropped), when pinned.
fn v5_line(line: &str) -> Option<String> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, msg) = rest.split_once(' ')?;
    let pinned = PINNED
        .iter()
        .any(|p| msg == *p || msg.starts_with(&format!("{p} ")));
    (pinned && !msg.starts_with(CYCLE_PERSIST_DEBUG_SILENT_IN_V4)).then(|| format!("{level} {msg}"))
}

#[test]
fn plan_storyteller_run_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_PLAN_STORYTELLER_RUN") else {
        eprintln!("SKIP: set QT_ORACLE_PLAN_STORYTELLER_RUN (see header).");
        return;
    };
    let (Ok(main_src), Ok(mount_src)) = (
        std::env::var("QT_FIXTURE_PSR_MAIN"),
        std::env::var("QT_FIXTURE_PSR_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_FIXTURE_PSR_MAIN / QT_FIXTURE_PSR_MOUNT (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"));
    let mut rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    let header = rows.remove(0);
    assert_eq!(
        header["op"], "header",
        "the oracle's first row is its header"
    );
    let pinned_list: Vec<String> = header["pinnedTimestamps"]
        .as_array()
        .expect("pinnedTimestamps")
        .iter()
        .map(|v| v.as_str().expect("ts").to_string())
        .collect();
    let pinned: HashSet<String> = pinned_list.iter().cloned().collect();
    assert!(
        rows.len() >= MIN_CASES,
        "oracle carries {} case rows, expected at least {MIN_CASES} — stale or truncated",
        rows.len()
    );

    let dir = tempfile::Builder::new()
        .prefix("qt-psr-rust-")
        .tempdir()
        .expect("tempdir");
    let main = dir.path().join("main.db");
    let mount = dir.path().join("mount.db");
    std::fs::copy(&main_src, &main).unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(&mount_src, &mount).unwrap_or_else(|e| panic!("copy mount fixture: {e}"));
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let mut mismatches = Vec::new();
    for row in &rows {
        let i = row["i"].as_u64().expect("i") as usize;
        let name = row["name"].as_str().expect("name");
        let input = &row["input"];
        // The oracle plants case i's round at T(i) — `2026-05-02T11:<i>:00.000Z`.
        let ts = format!("2026-05-02T11:{i:02}:00.000Z");
        assert!(pinned.contains(&ts), "case {i}: T(i) is a pinned timestamp");

        let (mut got, lines) = captured_with(|| rt.block_on(run_case(&db, i, input, &ts)));
        for key in ["plan", "run", "drawsUsed", "roundRow", "chats"] {
            let mut g = got[key].take();
            let mut w = row[key].clone();
            mask_ts(&mut g, &pinned);
            mask_ts(&mut w, &pinned);
            let (gs, ws) = (
                serde_json::to_string(&g).unwrap(),
                serde_json::to_string(&w).unwrap(),
            );
            if gs != ws {
                mismatches.push(format!("case {i} {name} {key}\n   v5: {gs}\n   v4: {ws}"));
            }
        }
        let want: Vec<String> = row["logs"]
            .as_array()
            .expect("logs")
            .iter()
            .map(v4_line)
            .collect();
        let got_lines: Vec<String> = lines.iter().filter_map(|l| v5_line(l)).collect();
        if got_lines != want {
            mismatches.push(format!(
                "case {i} {name} LOGS\n   v5: {got_lines:#?}\n   v4: {want:#?}"
            ));
        }
    }

    eprintln!(
        "plan_storyteller_run_equivalence: {} cases compared, {} mismatches",
        rows.len(),
        mismatches.len()
    );
    assert!(
        mismatches.is_empty(),
        "{} mismatch(es):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
