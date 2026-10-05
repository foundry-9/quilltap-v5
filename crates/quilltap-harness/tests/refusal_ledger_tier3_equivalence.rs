//! P4.D225 — the refusal ledger (v4 `recordModerationRefusal` /
//! `maybeAutoSwitchAfterRefusal`, `lib/services/dangerous-content/
//! refusal-ledger.ts`, NEW at `49059fb14`) against v4's REAL module on a real
//! encrypted fixture.
//!
//! Per op: the result (`{count, switched}` / `{switched}`) and every line the
//! ledger logged, rendered through the capture rig and compared to v4's
//! recorded `ConciergeRefusalLedger` lines in order. After the ops: the
//! `chats` dump (the ledger columns, the auto-switch's flag + category stamp)
//! and the `chat_messages` dump (the Concierge's auto-flag bubble, one per
//! switch — the concurrent pair must post exactly ONE).
//!
//! Arms: below / at / over the threshold, a per-user threshold of 3, the
//! auto-switch set to 0 on duty, the Concierge OFF DUTY at a threshold of 2
//! (v4 `3b463d6b1`, #76: the policy folds off duty into a threshold of 0, so
//! the ledger counts and nothing ever switches — ONE "off for this chat"
//! line), a vouched chat and an already-flagged one (recorded, never
//! switched), the re-read race (an operator vouch planted mid-check →
//! abandoned), `inferred` and an absent evidence (never recorded), an empty
//! chat id, a missing chat (the repository's `0`, then "chat not found"), a
//! direct check with no last refusal (the bubble without a provider), two
//! checks landing together, a user with no settings row (v4's defaults: on
//! duty at a threshold of 2).
//!
//! Regenerate the oracle (Node 24, from the v4 checkout; stage the case OUTSIDE
//! any `.claude/` path — v4's jest ignores `/\.claude/`). While v4 HEAD is past
//! the oracle baseline this needs a PINNED worktree; that is the sweep driver's
//! `--v4`, never a path in this header. The jest filter is ANCHORED: a bare
//! `refusal-ledger` also runs v4's own `refusal-ledger.test.ts`.
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-refusal-ledger
//!   cd ~/source/quilltap-server
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/refusal-ledger.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/refusal-ledger.json" "$TMPO/fixtures/"
//!   rm -f /tmp/qt-refusal-ledger.db /tmp/oracle-refusal-ledger.ndjson
//!   QT_FIXTURE_OUT=/tmp/qt-refusal-ledger.db \
//!     $N/npx tsx "$V5W/harness/oracle/fixtures/build-refusal-ledger-fixture.ts"
//!   QT_FIXTURE_REFUSAL_LEDGER=/tmp/qt-refusal-ledger.db \
//!   QT_ORACLE_OUT=/tmp/oracle-refusal-ledger.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/refusal-ledger\.test\.ts$"
//! Run:
//!   QT_ORACLE_REFUSAL_LEDGER=/tmp/oracle-refusal-ledger.ndjson \
//!   QT_FIXTURE_REFUSAL_LEDGER=/tmp/qt-refusal-ledger.db \
//!     cargo test -p quilltap-harness --test refusal_ledger_tier3_equivalence -- --nocapture

use std::sync::Mutex;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::dangerous_content::refusal::RefusalEvidence;
use quilltap_core::services::dangerous_content::refusal_ledger::{
    maybe_auto_switch_after_refusal, maybe_auto_switch_after_refusal_with,
    record_moderation_refusal_with, AutoSwitchProbe, LastRefusal, RefusalKind, RefusalPurpose,
    RefusalRecord,
};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Value};

const SEED_TS: &str = "2020-01-01T00:00:00.000Z";
const TARGET: &str = "quilltap::concierge_refusal_ledger";

/// The operator's vouch, planted between the check's first read and its
/// re-read (v4's case plants it inside the settings read — observationally the
/// same point: nothing between the two looks at the chat).
struct RacePlant {
    chat_id: Mutex<Option<String>>,
    /// P4.D226 (v4 `4d370a90f`): the `conciergeMode` the operator's
    /// mid-check move writes (was the legacy `conciergeOverride`).
    mode_value: String,
}

impl AutoSwitchProbe for RacePlant {
    async fn before_reread(&self, db: &Db, _chat_id: &str) {
        let armed = self.chat_id.lock().unwrap().take();
        if let Some(id) = armed {
            let value = self.mode_value.clone();
            db.write(move |w| {
                w.main().connection().execute(
                    "UPDATE chats SET \"conciergeMode\" = ?1 WHERE id = ?2",
                    rusqlite::params![value, id],
                )?;
                Ok(())
            })
            .await
            .expect("plant the operator vouch");
        }
    }
}

fn record_of(v: &Value) -> RefusalRecord {
    let s = |k: &str| {
        v[k].as_str()
            .unwrap_or_else(|| panic!("record.{k}"))
            .to_string()
    };
    RefusalRecord {
        chat_id: s("chatId"),
        kind: match v["kind"].as_str().unwrap() {
            "text" => RefusalKind::Text,
            "image" => RefusalKind::Image,
            other => panic!("kind {other}"),
        },
        purpose: match v["purpose"].as_str().unwrap() {
            "chat" => RefusalPurpose::Chat,
            "cheap" => RefusalPurpose::Cheap,
            "tool" => RefusalPurpose::Tool,
            "lantern" => RefusalPurpose::Lantern,
            "avatar" => RefusalPurpose::Avatar,
            "dialog" => RefusalPurpose::Dialog,
            other => panic!("purpose {other}"),
        },
        refused_profile_id: s("refusedProfileId"),
        refused_profile_name: s("refusedProfileName"),
        provider: s("provider"),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .map(str::to_string),
        evidence: v
            .get("evidence")
            .and_then(Value::as_str)
            .map(|e| RefusalEvidence::from_wire(e).unwrap_or_else(|| panic!("evidence {e}"))),
        rerouted: v["rerouted"].as_bool().unwrap(),
    }
}

fn last_refusal_of(v: &Value) -> Option<LastRefusal> {
    if v.is_null() {
        return None;
    }
    Some(LastRefusal {
        provider: v["provider"].as_str().unwrap().to_string(),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// v4's recorded line in the capture rig's rendering.
fn render_v4(log: &Value) -> String {
    let level = log["level"].as_str().unwrap().to_uppercase();
    let mut line = format!("{level} {TARGET} {}", log["message"].as_str().unwrap());
    for (k, v) in log["bag"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {}={rendered}", snake(k)));
    }
    line
}

fn normalize_chat_rows(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        for col in [
            "updatedAt",
            "lastMessageAt",
            "dangerClassifiedAt",
            "lastModerationRefusalAt",
        ] {
            let minted = obj
                .get(col)
                .and_then(Value::as_str)
                .is_some_and(|s| s != SEED_TS);
            if minted {
                obj.insert(col.into(), Value::String("<ts>".into()));
            }
        }
    }
}

fn normalize_message_rows(rows: &mut [Value]) {
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        obj.insert("id".into(), Value::String("<id>".into()));
        if obj.get("createdAt").is_some_and(|v| !v.is_null()) {
            obj.insert("createdAt".into(), Value::String("<ts>".into()));
        }
    }
}

fn canon(mut v: Value) -> Value {
    fn walk(v: &mut Value) {
        match v {
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    if n.is_f64() && f.fract() == 0.0 && f.abs() < 9.007e15 {
                        *v = Value::Number(serde_json::Number::from(f as i64));
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(walk),
            Value::Object(o) => o.values_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut v);
    v
}

fn rows_of(dump: &Value) -> Vec<Value> {
    dump["rows"].as_array().expect("dump rows").clone()
}

#[test]
fn refusal_ledger_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_REFUSAL_LEDGER") else {
        eprintln!("SKIP: set QT_ORACLE_REFUSAL_LEDGER to the tier-3 NDJSON (see header).");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_REFUSAL_LEDGER") else {
        eprintln!("SKIP: set QT_FIXTURE_REFUSAL_LEDGER to the seed fixture .db (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/refusal-ledger.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-refusal-ledger-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).expect("copy fixture");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let db = rt.block_on(async { Db::open_main(&work, &pepper).expect("open fixture copy") });

    let plants: Vec<(String, i64, String)> = spec["ledgerPlants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["chatId"].as_str().unwrap().to_string(),
                p["count"].as_i64().unwrap(),
                p["lastAt"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    rt.block_on(db.write(move |w| {
        // v5's boot ensure: a no-op on a fixture v4's migration already widened.
        quilltap_core::test_support::ensure_p4d225_columns(w.main().connection());
        for (id, count, at) in &plants {
            w.main().connection().execute(
                "UPDATE chats SET \"moderationRefusalCount\" = ?1, \"lastModerationRefusalAt\" = ?2 WHERE id = ?3",
                rusqlite::params![count, at, id],
            )?;
        }
        Ok(())
    }))
    .expect("plant the ledgers");

    let race = RacePlant {
        chat_id: Mutex::new(None),
        mode_value: spec["raceMode"].as_str().unwrap().to_string(),
    };
    let ops = spec["ops"].as_array().unwrap();
    assert_eq!(oracle.len(), ops.len() + 2, "one row per op + two dumps");

    let mut switches = 0usize;
    for (op, want) in ops.iter().zip(&oracle) {
        let id = op["id"].as_str().unwrap();
        assert_eq!(want["id"], op["id"], "oracle rows in spec order");
        let (got, lines) = captured_with(|| {
            rt.block_on(async {
                match op["kind"].as_str().unwrap() {
                    "record" => {
                        let rec = record_of(&op["record"]);
                        if op["racePlant"].as_bool() == Some(true) {
                            *race.chat_id.lock().unwrap() = Some(rec.chat_id.clone());
                        }
                        let r = record_moderation_refusal_with(&db, &rec, &race).await;
                        json!({ "count": r.count, "switched": r.switched })
                    }
                    "check" => {
                        let last = last_refusal_of(&op["lastRefusal"]);
                        let s = maybe_auto_switch_after_refusal(
                            &db,
                            op["chatId"].as_str().unwrap(),
                            last.as_ref(),
                        )
                        .await;
                        json!({ "switched": s })
                    }
                    "concurrentChecks" => {
                        let last = last_refusal_of(&op["lastRefusal"]);
                        let chat_id = op["chatId"].as_str().unwrap();
                        let (a, b) = tokio::join!(
                            maybe_auto_switch_after_refusal_with(
                                &db,
                                chat_id,
                                last.as_ref(),
                                &race
                            ),
                            maybe_auto_switch_after_refusal_with(
                                &db,
                                chat_id,
                                last.as_ref(),
                                &race
                            ),
                        );
                        json!([{ "switched": a }, { "switched": b }])
                    }
                    // P4.D226 (v4 `4d370a90f`): the operator's own move through
                    // the real chokepoint — a return to Moderated empties the
                    // ledger, so the next refusal starts afresh.
                    "operatorFlip" => {
                        let chat_id = op["chatId"].as_str().unwrap();
                        let chat = db
                            .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, chat_id))
                            .unwrap()
                            .unwrap();
                        let requested = quilltap_core::services::dangerous_content::chat_override::ConciergeState::from_wire(
                            op["requested"].as_str().unwrap(),
                        )
                        .unwrap();
                        let r = quilltap_core::services::dangerous_content::manual_flip::apply_concierge_flip(
                            &db,
                            &quilltap_core::services::dangerous_content::manual_flip::RealConciergeAnnouncer { db: &db },
                            chat_id,
                            requested,
                            &chat,
                        )
                        .await
                        .unwrap();
                        json!({ "newState": r.new_state.as_str(), "changed": r.changed })
                    }
                    other => panic!("op kind {other}"),
                }
            })
        });
        assert!(
            race.chat_id.lock().unwrap().is_none(),
            "op {id}: the race plant never fired"
        );
        assert_eq!(got, want["result"], "op {id}: result");
        let got_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.split(' ').nth(1) == Some(TARGET))
            .collect();
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        assert_eq!(
            got_lines.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            want_lines.iter().map(String::as_str).collect::<Vec<_>>(),
            "op {id}: the ledger's log lines"
        );
        switches += want_lines
            .iter()
            .filter(|l| l.contains("switched a chat to Unmoderated"))
            .count();
    }
    assert!(
        switches >= 5,
        "the corpus must exercise the switch ({switches})"
    );

    let (got_chats, got_msgs) = (
        db.read_main(|c| dump_table_json_conn(c, "chats", "id"))
            .expect("dump chats"),
        db.read_main(|c| dump_table_json_conn(c, "chat_messages", "chatId"))
            .expect("dump chat_messages"),
    );
    drop(db);
    drop(rt);

    let tables = &oracle[ops.len()..];
    let (mut want_chats, mut want_msgs) = (rows_of(&tables[0]), rows_of(&tables[1]));
    let (mut got_chats, mut got_msgs) = (rows_of(&got_chats), rows_of(&got_msgs));
    normalize_chat_rows(&mut want_chats);
    normalize_chat_rows(&mut got_chats);
    normalize_message_rows(&mut want_msgs);
    normalize_message_rows(&mut got_msgs);
    let c = |rows: Vec<Value>| rows.into_iter().map(canon).collect::<Vec<_>>();
    assert_eq!(c(got_chats), c(want_chats), "chats rows diverge");
    let operator_flips = ops
        .iter()
        .filter(|o| o["kind"].as_str() == Some("operatorFlip"))
        .count();
    assert_eq!(
        want_msgs.len(),
        switches + operator_flips,
        "one Concierge bubble per switch (and per operator flip) on v4's side"
    );
    assert_eq!(c(got_msgs), c(want_msgs), "chat_messages rows diverge");

    eprintln!(
        "OK: refusal ledger matched v4 ({} ops, {switches} switches).",
        ops.len()
    );
}
