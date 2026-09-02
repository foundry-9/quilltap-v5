//! P4.D143 differential: **`triggerChatDangerClassification`'s gate chain**
//! (v4 `lib/services/chat-message/memory-trigger.service.ts:157-200`) —
//! v4's own `chat-danger-trigger.test.ts` corpus, case for case, plus the two
//! operator arms `c43d3b1b4` added.
//!
//! Until this family the gate chain was only ever inferred from the spine
//! dumps: the tier-3 finalizer and orchestrator corpora prove the enqueue
//! happens or does not, but they exercise one or two gates per op. Here every
//! gate has its own case.
//!
//! **The two sides are shaped differently on purpose.** v4's oracle is DB-free
//! (the route-guard-oracle idiom — three `jest.doMock`s, mocked repos) and
//! emits the ENQUEUE CALLS the function made; the v5 side seeds ONE chat row
//! into a copy of the committed `chat-scenario-main.db` (a CURRENT-vintage
//! `chats` DDL — `chats_read`'s SELECT names ~60 columns, so an older fixture
//! dies on `no such column`, which is how `chat-send-main.db` was ruled out —
//! plus the `background_jobs` table the enqueue writes) and reads the rows back.
//! Both are reduced to the same `{userId, chatId, connectionProfileId}` list.
//!
//! **Two of v4's observables have NO v5 counterpart, and the corpus says so.**
//! v4 resolves the danger mode INSIDE the function, so it can assert that an
//! operator override bails "before any setting lookup at all"; v5's
//! `danger_mode_off` is computed by the two producers (`orchestrator`,
//! `courier_transport`) BEFORE the call. So `chatSettingsLookedUp` is recorded
//! in the NDJSON and never compared, and `settings_lookup_throws` is skipped by
//! name — there is no settings lookup in the v5 function to throw. The
//! observable this port pins is the enqueue itself.
//!
//! Generate the oracle (Node 24, from the v4 checkout — cp to a /tmp mirror;
//! jest ignores .claude/ paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   TMPO=/tmp/qt-danger-trigger-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/danger-trigger.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-danger-trigger.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- "danger-trigger\.test\.ts$"
//! Run:
//!   QT_ORACLE_DANGER_TRIGGER=/tmp/oracle-danger-trigger.ndjson \
//!     cargo test -p quilltap-harness --test danger_trigger_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::message_finalizer::trigger_chat_danger_classification;
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// v4's corpus ids, verbatim — the oracle's payloads carry them.
const CHAT_ID: &str = "chat-1";
const USER_ID: &str = "user-1";
const PROFILE_ID: &str = "profile-1";
/// The one v4 case whose observable has no v5 counterpart (see the header).
const NO_COUNTERPART: &[&str] = &["settings_lookup_throws"];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[derive(Deserialize)]
struct OracleCase {
    name: String,
    /// The `chats.findById` answer v4 was handed — `null` = chat not found.
    chat: Option<Value>,
    mode: String,
    enqueued: Vec<EnqueueCall>,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct EnqueueCall {
    user_id: String,
    chat_id: String,
    connection_profile_id: String,
}

/// SQL-literal a JSON value the way the corresponding column stores it:
/// `NULL` for JSON null/absent, `0`/`1` for a bool, a quoted string otherwise.
fn lit(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "NULL".into(),
        Some(Value::Bool(b)) => if *b { "1" } else { "0" }.into(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(other) => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

#[test]
fn danger_trigger_gate_chain_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_DANGER_TRIGGER") else {
        eprintln!("SKIP: set QT_ORACLE_DANGER_TRIGGER to the NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let cases: Vec<OracleCase> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle case"))
        .collect();
    assert!(!cases.is_empty(), "danger-trigger oracle looks empty");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed: Vec<String> = Vec::new();
    let mut compared = 0usize;
    let mut skipped = 0usize;

    for case in &cases {
        if NO_COUNTERPART.contains(&case.name.as_str()) {
            eprintln!(
                "[{}] NO v5 COUNTERPART (recorded, not compared).",
                case.name
            );
            skipped += 1;
            continue;
        }

        // A fresh copy of the committed fixture per case — the real `chats` DDL
        // (a reduced hand-rolled one would silently drift from the reader's
        // 60-column SELECT) plus the `background_jobs` table the enqueue writes.
        let scratch = std::env::temp_dir().join(format!(
            "qt-danger-trigger-{}-{}",
            std::process::id(),
            case.name
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main = scratch.join("main.db");
        std::fs::copy(fixtures_dir().join("chat-scenario-main.db"), &main).unwrap();
        let db = Db::open(
            DbPaths {
                main,
                mount_index: None,
                llm_logs: None,
            },
            TEST_PEPPER,
        )
        .expect("open db");

        // Clear the fixture's own rows and seed exactly v4's mocked chat (or
        // nothing at all, for the chat-not-found arm).
        let seed = match &case.chat {
            None => {
                "DELETE FROM \"background_jobs\"; DELETE FROM \"chat_messages\"; \
                 DELETE FROM \"chats\";"
                    .to_string()
            }
            Some(chat) => format!(
                "DELETE FROM \"background_jobs\"; DELETE FROM \"chat_messages\"; \
                 DELETE FROM \"chats\"; \
                 INSERT INTO \"chats\" (\"id\", \"userId\", \"title\", \"createdAt\", \"updatedAt\", \
                 \"contextSummary\", \"messageCount\", \"isDangerousChat\", \"dangerClassifiedAt\", \
                 \"dangerClassifiedAtMessageCount\", \"conciergeOverride\") VALUES \
                 ('{CHAT_ID}', '{USER_ID}', 'Trigger corpus', '2026-01-01T00:00:00.000Z', \
                 '2026-01-01T00:00:00.000Z', {}, {}, {}, {}, {}, {});",
                lit(chat.get("contextSummary")),
                lit(chat.get("messageCount")),
                lit(chat.get("isDangerousChat")),
                lit(chat.get("dangerClassifiedAt")),
                lit(chat.get("dangerClassifiedAtMessageCount")),
                lit(chat.get("conciergeOverride")),
            ),
        };
        rt.block_on(db.write(move |w| {
            w.main().connection().execute_batch(&seed)?;
            Ok(())
        }))
        .expect("seed the corpus chat");

        // v5's `danger_mode_off` is the resolved mode, computed by the callers.
        let danger_mode_off = case.mode == "OFF";
        rt.block_on(trigger_chat_danger_classification(
            &db,
            CHAT_ID,
            USER_ID,
            PROFILE_ID,
            danger_mode_off,
        ))
        .expect("trigger");

        let rows: Vec<(String, String)> = rt
            .block_on(async {
                db.read_main(|conn| {
                    let mut st = conn.prepare(
                        "SELECT \"userId\", \"payload\" FROM \"background_jobs\" \
                         WHERE \"type\" = 'CHAT_DANGER_CLASSIFICATION' ORDER BY \"id\"",
                    )?;
                    let out = st
                        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(out)
                })
            })
            .expect("read background_jobs");
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        let got: Vec<EnqueueCall> = rows
            .into_iter()
            .map(|(user_id, payload)| {
                let p: Value = serde_json::from_str(&payload).expect("payload json");
                EnqueueCall {
                    user_id,
                    chat_id: p["chatId"].as_str().unwrap_or_default().to_string(),
                    connection_profile_id: p["connectionProfileId"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                }
            })
            .collect();

        if got != case.enqueued {
            eprintln!(
                "[{}] MISMATCH:\n  got  {got:?}\n  want {:?}",
                case.name, case.enqueued
            );
            failed.push(case.name.clone());
        } else {
            eprintln!("[{}] OK ({} enqueued).", case.name, got.len());
        }
        compared += 1;
    }

    // Shape guards: a stale oracle (regenerated before the operator arms
    // existed) would silently take the whole point of `c43d3b1b4` dark, and a
    // corpus in which nothing ever enqueues would pass on an always-bail port.
    assert!(
        cases
            .iter()
            .any(|c| c.name == "skips_when_operator_uncensored"),
        "the oracle predates c43d3b1b4's operator arms — regenerate it"
    );
    assert!(
        cases.iter().any(|c| !c.enqueued.is_empty()),
        "no case in the corpus enqueues anything — an always-bail port would pass"
    );
    assert!(failed.is_empty(), "danger-trigger FAILED: {failed:?}");
    eprintln!("OK: danger trigger gate chain matched oracle ({compared} compared, {skipped} no-counterpart).");
}
