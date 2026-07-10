//! P4.6c skipUserTurn differential (the minted-values sibling of
//! `salon_mutations_equivalence`): `api::salon::turn_action` with `skipUserTurn`
//! vs v4's REAL `handleTurnAction`, over a fresh copy of the committed salon GROUP
//! fixture. Two branches:
//!   - `skip_success`: the skip applies in a genuine group scene → a Host turn-pass
//!     row lands (minted id + createdAt), `spokenThisCycle` folds, the queue +
//!     `lastTurnParticipantId` persist. Diffed body + `chats` + `chat_messages` with
//!     the minted turn-pass row's id → `<HOSTPASS>`, its createdAt + the chats
//!     lastMessageAt/updatedAt bumps → `<ts>`.
//!   - `skip_refusal`: two prior LLM turn-pass records are seeded → `all-others-skipped`
//!     → v4 refuses with the exact copy. The refusal writes nothing; the error
//!     MESSAGE (v4 `{error}` vs v5 `{kind,message}`) is compared byte-for-byte and
//!     the tables (seeded state, lastMessageAt/updatedAt placeholdered) are diffed.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-salon-skip.ndjson npx jest -- salon-skip
//! Run:
//!   QT_ORACLE_SALON_SKIP=/tmp/oracle-salon-skip.ndjson \
//!     cargo test -p quilltap-harness --test salon_skip_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::salon;
use quilltap_core::api::types::Response;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

const SEED_TS: &str = "2026-02-01T00:00:00.000Z";
const GROUP: &str = "c1000000-0000-4000-8000-000000000002";
const USER_P: &str = "b2000000-0000-4000-8000-000000000003"; // Cleo (controlledBy user)
const ARIA_P: &str = "b2000000-0000-4000-8000-000000000001"; // Aria (llm)
const BRAM_P: &str = "b2000000-0000-4000-8000-000000000002"; // Bram (llm)
const SEED_ID_1: &str = "aa000000-0000-4000-8000-000000000001";
const SEED_ID_2: &str = "aa000000-0000-4000-8000-000000000002";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/salon.json")
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
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
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
    "(identical)".to_string()
}

/// Normalize a `chats` dump: placeholder `lastMessageAt`/`updatedAt` when bumped
/// off the seed sentinel (both sides bump on skip/seed), strip `renderedHtml`.
fn norm_chats(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_string());
    for r in &mut rows {
        if let Some(o) = r.as_object_mut() {
            o.remove("renderedHtml");
            for k in ["lastMessageAt", "updatedAt"] {
                if o.get(k).and_then(Value::as_str) != Some(SEED_TS) {
                    o.insert(k.to_string(), json!("<ts>"));
                }
            }
        }
    }
    Value::Array(rows)
}

/// Normalize a `chat_messages` dump: sort by (createdAt, id); a minted turn-pass
/// row (systemKind='turn-pass', id not one of the fixed seeded ids) has its id →
/// `<HOSTPASS>` and createdAt → `<ts>` (the content stays exact — the Host writer
/// output is byte-diffed). `renderedHtml` stripped.
fn norm_msgs(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.sort_by(|a, b| {
        let ka = (
            a["createdAt"].as_str().unwrap_or(""),
            a["id"].as_str().unwrap_or(""),
        );
        let kb = (
            b["createdAt"].as_str().unwrap_or(""),
            b["id"].as_str().unwrap_or(""),
        );
        ka.cmp(&kb)
    });
    for r in &mut rows {
        if let Some(o) = r.as_object_mut() {
            o.remove("renderedHtml");
            let id = o.get("id").and_then(Value::as_str).unwrap_or("");
            let is_seeded = id == SEED_ID_1 || id == SEED_ID_2;
            let is_pass = o.get("systemKind").and_then(Value::as_str) == Some("turn-pass");
            if is_pass && !is_seeded {
                o.insert("id".into(), json!("<HOSTPASS>"));
                o.insert("createdAt".into(), json!("<ts>"));
            }
        }
    }
    // Re-sort so the placeholdered minted row lands deterministically last.
    rows.sort_by(|a, b| {
        let ka = (
            a["createdAt"].as_str().unwrap_or(""),
            a["id"].as_str().unwrap_or(""),
        );
        let kb = (
            b["createdAt"].as_str().unwrap_or(""),
            b["id"].as_str().unwrap_or(""),
        );
        ka.cmp(&kb)
    });
    Value::Array(rows)
}

fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

/// Seed a Host turn-pass record (explicit id + createdAt) via the ported
/// `add_message`, mirroring the oracle's `repos.chats.addMessages` seed.
fn seed_turn_pass(db: &Db, id: &str, participant_id: &str, created_at: &str) {
    let event: quilltap_core::db::chats_messages::ChatEventInput = serde_json::from_value(json!({
        "type": "message",
        "id": id,
        "role": "ASSISTANT",
        "content": "A pass, quietly noted.",
        "createdAt": created_at,
        "attachments": [],
        "systemSender": "host",
        "systemKind": "turn-pass",
        "hostEvent": { "participantId": participant_id },
    }))
    .expect("seed event");
    db.write_blocking(move |w| w.main().chat_messages().add_message(GROUP, &event))
        .expect("seed write");
}

#[test]
fn salon_skip_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SALON_SKIP") else {
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // Open a fresh fixture copy; seed if requested; run the skip; dump.
    let run = |case: &str, seed: bool| -> (Value, Value, Value) {
        let scratch =
            std::env::temp_dir().join(format!("qt-salon-skip-{}-{}", std::process::id(), case));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main = scratch.join("main.db");
        let mount = scratch.join("mount.db");
        std::fs::copy(fixtures_dir().join("salon-main.db"), &main).unwrap();
        std::fs::copy(fixtures_dir().join("salon-mount.db"), &mount).unwrap();
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .expect("open db");
        if seed {
            seed_turn_pass(&db, SEED_ID_1, ARIA_P, "2026-02-02T00:00:00.000Z");
            seed_turn_pass(&db, SEED_ID_2, BRAM_P, "2026-02-03T00:00:00.000Z");
        }
        let body = response_data(&rt.block_on(salon::turn_action(
            &db,
            GROUP,
            "skipUserTurn",
            Some(USER_P),
            0.42,
        )));
        let chats = db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .unwrap();
        let msgs = db
            .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
            .unwrap();
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
        (body, chats, msgs)
    };

    let mut failed = Vec::new();

    // --- skip_success: full body + tables diff (minted turn-pass normalized) ---
    {
        let want = &oracle["skip_success"];
        let (got_body, got_chats, got_msgs) = run("skip_success", false);
        let sections: [(&str, Value, Value); 3] = [
            ("body", got_body, want["body"].clone()),
            (
                "chats",
                norm_chats(&got_chats),
                norm_chats(&want["tables"]["chats"]),
            ),
            (
                "chat_messages",
                norm_msgs(&got_msgs),
                norm_msgs(&want["tables"]["chatMessages"]),
            ),
        ];
        for (section, got, want_v) in &sections {
            let g = norm(got);
            let w = norm(want_v);
            if g != w {
                eprintln!(
                    "[skip_success] section `{section}` MISMATCH:\n{}",
                    first_diff(&g, &w)
                );
                failed.push(format!("skip_success/{section}"));
            }
        }
        eprintln!("[skip_success] done.");
    }

    // --- skip_refusal: exact error copy + tables unchanged (seeded state) ---
    {
        let want = &oracle["skip_refusal"];
        let (got_body, got_chats, got_msgs) = run("skip_refusal", true);
        // v4 badRequest body `{error}` vs v5 `{kind, message}` — compare the copy.
        let got_msg = got_body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("");
        let got_kind = got_body.get("kind").and_then(Value::as_str).unwrap_or("");
        let want_msg = want["body"]
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("<none>");
        if got_kind != "bad-request" {
            eprintln!("[skip_refusal] expected bad-request, got kind=`{got_kind}`");
            failed.push("skip_refusal/kind".into());
        }
        if got_msg != want_msg {
            eprintln!("[skip_refusal] error copy MISMATCH:\n  GOT : {got_msg}\n  WANT: {want_msg}");
            failed.push("skip_refusal/message".into());
        }
        let sections: [(&str, Value, Value); 2] = [
            (
                "chats",
                norm_chats(&got_chats),
                norm_chats(&want["tables"]["chats"]),
            ),
            (
                "chat_messages",
                norm_msgs(&got_msgs),
                norm_msgs(&want["tables"]["chatMessages"]),
            ),
        ];
        for (section, got, want_v) in &sections {
            let g = norm(got);
            let w = norm(want_v);
            if g != w {
                eprintln!(
                    "[skip_refusal] section `{section}` MISMATCH:\n{}",
                    first_diff(&g, &w)
                );
                failed.push(format!("skip_refusal/{section}"));
            }
        }
        eprintln!("[skip_refusal] done.");
    }

    assert!(failed.is_empty(), "salon-skip FAILED: {failed:?}");
}
