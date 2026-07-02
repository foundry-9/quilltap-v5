//! Tier-2 differential test: the turn-orchestration decision core (v4
//! `lib/services/chat-message/turn-orchestrator.service.ts` `shouldChainNext` /
//! `persistTurnParticipantId` + the `actions/turn.ts` `handleTurnAction` mutation
//! core), ported as `quilltap_core::services::turn_orchestrator`.
//!
//! Both sides run the SAME op sequence (`turn-orchestrator-tier2.json`) on a fresh
//! copy of the two-database seed fixture (a main DB with chats + messages +
//! slim/null-mount characters, and an empty-but-valid mount-index DB), then diff
//! (a) each op's returned decision object and (b) the final `chats` table dump.
//!
//! This proves: the guard order (not-found → paused → depth → time), the all-LLM
//! auto-pause WRITE (isPaused=true even on a "don't chain" return), the queue pop
//! (skipping the immediate last speaker) + its write-back, the weighted selection
//! (RNG injected), the user-turn stop that surfaces the user character's name, and
//! the five turn actions (nudge/queue/dequeue/skipUserTurn/query) with their
//! turnQueue + lastTurnParticipantId (+ spokenThisCycleParticipantIds on skip)
//! writes.
//!
//! NORMALIZATION: none. The seed pins every id + timestamp; no op mints a chat
//! `updatedAt` (v4 `_update` preserves it, and none of these ops pass one), so the
//! final `chats` dump and every decision object are compared exactly.
//!
//! INJECTION: `Math.random` is pinned to each op's `random01` on the v4 side; the
//! identical value is threaded into the Rust `select_next_speaker`. The wall clock
//! in `shouldChainNext` is passed explicitly (`nowMs` / `chainStartTimeMs`).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-turnorch-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-turnorch-mount.db \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-turn-orchestrator-fixture.ts
//!   QT_FIXTURE_TURNORCH=/tmp/qt-turnorch-main.db \
//!   QT_FIXTURE_TURNORCH_MOUNT=/tmp/qt-turnorch-mount.db \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/turn-orchestrator-tier2.ts > /tmp/oracle-turnorch.ndjson
//! Run:
//!   QT_ORACLE_TURNORCH=/tmp/oracle-turnorch.ndjson \
//!   QT_FIXTURE_TURNORCH=/tmp/qt-turnorch-main.db \
//!   QT_FIXTURE_TURNORCH_MOUNT=/tmp/qt-turnorch-mount.db \
//!     cargo test -p quilltap-harness --test turn_orchestrator_tier2_equivalence

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::turn_orchestrator::{
    handle_turn_action, should_chain_next, ChainConfig, ChainGuards, TurnAction,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "shouldChainNext")]
    ShouldChainNext {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "userParticipantId")]
        user_participant_id: Option<String>,
        #[serde(rename = "chainDepth")]
        chain_depth: i64,
        #[serde(rename = "nowMs")]
        now_ms: i64,
        #[serde(rename = "chainStartTimeMs")]
        chain_start_time_ms: i64,
        random01: f64,
    },
    #[serde(rename = "turnAction")]
    TurnAction {
        #[serde(rename = "chatId")]
        chat_id: String,
        action: String,
        #[serde(rename = "participantId")]
        participant_id: Option<String>,
        random01: f64,
    },
}

fn parse_action(s: &str) -> TurnAction {
    match s {
        "nudge" => TurnAction::Nudge,
        "queue" => TurnAction::Queue,
        "dequeue" => TurnAction::Dequeue,
        "skipUserTurn" => TurnAction::SkipUserTurn,
        "query" => TurnAction::Query,
        other => panic!("unknown action: {other}"),
    }
}

#[tokio::test]
async fn turn_orchestrator_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TURNORCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TURNORCH to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TURNORCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_TURNORCH to the main seed fixture .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_TURNORCH_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_TURNORCH_MOUNT to the mount seed fixture .db.");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/turn-orchestrator-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");
    let oracle_results = oracle["results"].as_array().expect("oracle results array");

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-turnorch-rust-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-turnorch-rust-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture, &work_main).unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(&fixture_mount, &work_mount)
        .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let config = ChainConfig::default();
    let guards = ChainGuards::default();

    let mut got_results: Vec<Value> = Vec::new();

    for op in &spec.ops {
        match op {
            Op::ShouldChainNext {
                chat_id,
                user_participant_id,
                chain_depth,
                now_ms,
                chain_start_time_ms,
                random01,
            } => {
                let decision = should_chain_next(
                    &db,
                    chat_id,
                    user_participant_id.as_deref(),
                    *chain_depth,
                    *now_ms,
                    *chain_start_time_ms,
                    &config,
                    guards,
                    *random01,
                )
                .await
                .unwrap_or_else(|e| panic!("shouldChainNext {chat_id}: {e:?}"));
                got_results.push(json!({
                    "op": "shouldChainNext",
                    "chatId": chat_id,
                    "chain": decision.chain,
                    "participantId": decision.participant_id,
                    "characterName": decision.character_name,
                    "reason": decision.reason.as_str(),
                }));
            }
            Op::TurnAction {
                chat_id,
                action,
                participant_id,
                random01,
            } => {
                let r = handle_turn_action(
                    &db,
                    chat_id,
                    parse_action(action),
                    participant_id.as_deref(),
                    *random01,
                )
                .await
                .unwrap_or_else(|e| panic!("turnAction {action} {chat_id}: {e:?}"));
                got_results.push(json!({
                    "op": "turnAction",
                    "chatId": chat_id,
                    "action": action,
                    "nextSpeakerId": r.next_speaker_id,
                    "nextSpeakerName": r.next_speaker_name,
                    "nextSpeakerControlledBy": r.next_speaker_controlled_by,
                    "reason": r.reason,
                    "explanation": r.explanation,
                    "cycleComplete": r.cycle_complete,
                    "isUsersTurn": r.is_users_turn,
                    "queue": r.queue,
                    "queuePosition": r.queue_position,
                }));
            }
        }
    }

    // Compare each op's decision object.
    assert_eq!(
        got_results.len(),
        oracle_results.len(),
        "op-count mismatch — regenerate the oracle NDJSON"
    );
    for (i, (got, want)) in got_results.iter().zip(oracle_results.iter()).enumerate() {
        assert_eq!(
            got, want,
            "op {i} decision diverged\n  rust:   {got}\n  oracle: {want}"
        );
    }

    // Compare the final chats dump (zero-normalization).
    let got_dump = db
        .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
        .expect("dump chats");
    let oracle_dump = &oracle["dump"];

    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    assert_eq!(got_dump["table"], oracle_dump["table"], "table name");
    assert_eq!(
        got_dump["columns"], oracle_dump["columns"],
        "column set / order"
    );
    assert_eq!(
        got_dump["rows"], oracle_dump["rows"],
        "chats row state diverged\n  rust:   {}\n  oracle: {}",
        got_dump["rows"], oracle_dump["rows"]
    );

    eprintln!(
        "OK: turn-orchestrator tier-2 matched oracle ({} ops, {} chats).",
        got_results.len(),
        got_dump["rows"].as_array().map(|a| a.len()).unwrap_or(0)
    );
}
