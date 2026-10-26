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
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::test_support::global_capture;
use quilltap_core::weighted_random::DrawSource;
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

/// Seed the Host turn-pass records (explicit ids + createdAt) via the ported
/// `add_messages`, mirroring the oracle's ONE `repos.chats.addMessages` batch.
///
/// ⚠ Batched on purpose (the `53294163f` unification's catch). The oracle seeds
/// both rows in a single `addMessages` call, which announces the transcript
/// change ONCE; this helper used to call `add_message` per row and so bumped
/// `chats.transcriptVersion` TWICE. Invisible while the committed pair lacked
/// the column (both sides' bumps failed soft, `a-widened-shared-column-breaks-
/// sibling-fixtures-invisibly`); P4.94's widen made the count a comparand and
/// the `skip_refusal` chats dump read 2 against v4's 1. A harness seeding-shape
/// divergence, not a port one — the production skip path writes one message.
fn seed_turn_passes(db: &Db, rows: &[(&str, &str, &str)]) {
    let events: Vec<quilltap_core::db::chats_messages::ChatEventInput> = rows
        .iter()
        .map(|(id, participant_id, created_at)| {
            serde_json::from_value(json!({
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
            .expect("seed event")
        })
        .collect();
    db.write_blocking(move |w| w.main().chat_messages().add_messages(GROUP, &events))
        .expect("seed write");
}

/// P4.D335: the interrupt cases — `(name, chat id, plant SQL)` for the stops,
/// `(name, raised tri-state)` for the raises. The plants are the oracle's SQL
/// verbatim.
const SOLO: &str = "c1000000-0000-4000-8000-000000000001";
const UNKNOWN: &str = "c1000000-0000-4000-8000-0000000000ff";

/// A dispatch `Response` as v4's HTTP pair: success → `(200, data)`; an error
/// → its status with v4's `{error}` body (+ `details` when set — v4's
/// `validationError`).
fn as_http(r: &Response) -> (u16, Value) {
    match r {
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::NotFound => 404,
                ErrorKind::BadRequest => 400,
                _ => 500,
            };
            let mut body = json!({ "error": e.message });
            if let Some(d) = &e.details {
                body["details"] = (**d).clone();
            }
            (status, body)
        }
        other => (200, response_data(other)),
    }
}

/// v4's `{level, message, fields}` in the capture rig's line shape (target
/// dropped), keys snake_cased in v4's order.
fn v4_log_line(l: &Value) -> String {
    let mut s = format!(
        "{} {}",
        l["level"].as_str().unwrap().to_ascii_uppercase(),
        l["message"].as_str().unwrap()
    );
    for (k, v) in l["fields"].as_object().unwrap() {
        let key: String = k
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['_', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        let v = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        s.push_str(&format!(" {key}={v}"));
    }
    s
}

fn v5_log_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains("[Interrupt]") || l.contains("[TurnControl]"))
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap_or_default();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or_default())
        })
        .collect()
}

#[test]
fn salon_skip_matches_oracle() {
    // P4.D335: the interrupt cases capture through the process-global rig.
    global_capture::install();
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
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-salon-skip-{case}-"))
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
            &spec.test_pepper_base64,
        )
        .expect("open db");
        // P4.D172: the committed salon fixture predates the two `78b381a96`
        // schema moves, and this path now WRITES both columns. P4.D171's
        // repaired-at-boot heal is what the many committed fixtures use; the v4
        // side of this family gets the same ALTERs in its own jest setup.
        db.write_blocking(|w| {
            quilltap_core::test_support::ensure_p4d171_columns(w.main().connection());
            Ok(())
        })
        .expect("heal the fixture copy");
        if seed {
            seed_turn_passes(
                &db,
                &[
                    (SEED_ID_1, ARIA_P, "2026-02-02T00:00:00.000Z"),
                    (SEED_ID_2, BRAM_P, "2026-02-03T00:00:00.000Z"),
                ],
            );
        }
        let body = response_data(&rt.block_on(salon::turn_action(
            &db,
            GROUP,
            "skipUserTurn",
            Some(USER_P),
            &DrawSource::sequence(vec![0.42]),
        )));
        let chats = db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .unwrap();
        let msgs = db
            .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
            .unwrap();
        drop(db);
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

    // --- P4.D335: the interrupt actions (no stream open in this process) ---
    let open_copy = |case: &str| -> (tempfile::TempDir, Db) {
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-salon-interrupt-{case}-"))
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
            &spec.test_pepper_base64,
        )
        .expect("open db");
        db.write_blocking(|w| {
            quilltap_core::test_support::ensure_p4d171_columns(w.main().connection());
            Ok(())
        })
        .expect("heal the fixture copy");
        (scratch, db)
    };
    let stops: &[(&str, &str, Option<String>)] = &[
        ("stop_no_stream_group", GROUP, None),
        (
            "stop_no_stream_already_paused",
            GROUP,
            Some(format!(r#"UPDATE chats SET "isPaused" = 1 WHERE id = '{GROUP}'"#)),
        ),
        (
            "stop_no_stream_single",
            SOLO,
            Some(format!(
                "UPDATE chats SET participants = json_set(participants, '$[0].status', 'removed') WHERE id = '{SOLO}'"
            )),
        ),
        ("stop_no_stream_solo_pair", SOLO, None),
        (
            "stop_no_stream_autonomous",
            GROUP,
            Some(format!(r#"UPDATE chats SET "chatType" = 'autonomous' WHERE id = '{GROUP}'"#)),
        ),
        ("stop_unknown_chat", UNKNOWN, None),
    ];
    // `None` = the key absent; `Some(None)` = an explicit `null`.
    let raises: Vec<(&str, &str, Option<Option<Value>>)> = vec![
        ("raise_absent_body", GROUP, None),
        ("raise_true", GROUP, Some(Some(json!(true)))),
        ("raise_false", GROUP, Some(Some(json!(false)))),
        ("raise_null", GROUP, Some(None)),
        ("raise_string", GROUP, Some(Some(json!("true")))),
        ("raise_number", GROUP, Some(Some(json!(1)))),
        ("raise_object", GROUP, Some(Some(json!({})))),
        // Extra keys are stripped (no `.strict()`): the dispatch variant has
        // no slot for them, so `raised: true` is the whole request.
        ("raise_extra_keys", GROUP, Some(Some(json!(true)))),
        ("raise_unknown_chat", UNKNOWN, Some(None)),
    ];
    let mut interrupt_rows = 0usize;
    let mut compare =
        |case: &str, got: (u16, Value), lines: Vec<String>, chats: Value, msgs: Value| {
            let want = oracle
                .get(case)
                .unwrap_or_else(|| panic!("{case}: no oracle row — regenerate"));
            interrupt_rows += 1;
            if u64::from(got.0) != want["status"].as_u64().unwrap() {
                eprintln!("[{case}] status v5 {} != v4 {}", got.0, want["status"]);
                failed.push(format!("{case}/status"));
            }
            if serde_json::to_string(&got.1).unwrap()
                != serde_json::to_string(&want["body"]).unwrap()
            {
                eprintln!("[{case}] body v5 {} != v4 {}", got.1, want["body"]);
                failed.push(format!("{case}/body"));
            }
            let want_logs: Vec<String> = want["logs"]
                .as_array()
                .expect("logs")
                .iter()
                .map(v4_log_line)
                .collect();
            let got_logs = v5_log_lines(&lines);
            if got_logs != want_logs {
                eprintln!("[{case}] logs\n  v5 {got_logs:#?}\n  v4 {want_logs:#?}");
                failed.push(format!("{case}/logs"));
            }
            for (section, g, w) in [
                (
                    "chats",
                    norm_chats(&chats),
                    norm_chats(&want["tables"]["chats"]),
                ),
                (
                    "chat_messages",
                    norm_msgs(&msgs),
                    norm_msgs(&want["tables"]["chatMessages"]),
                ),
            ] {
                let (g, w) = (norm(&g), norm(&w));
                if g != w {
                    eprintln!(
                        "[{case}] section `{section}` MISMATCH:\n{}",
                        first_diff(&g, &w)
                    );
                    failed.push(format!("{case}/{section}"));
                }
            }
        };
    let dump = |db: &Db| {
        (
            db.read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
                .unwrap(),
            db.read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
                .unwrap(),
        )
    };
    for (case, chat_id, plant) in stops {
        let (_scratch, db) = open_copy(case);
        if let Some(sql) = plant {
            let sql = sql.clone();
            db.write_blocking(move |w| {
                w.main().connection().execute_batch(&sql)?;
                Ok(())
            })
            .expect("plant");
        }
        let (resp, lines) = rt.block_on(global_capture::capture_async(
            quilltap_core::api::chat_interrupt::chat_stop_turn(&db, chat_id),
        ));
        let (chats, msgs) = dump(&db);
        compare(case, as_http(&resp), lines, chats, msgs);
    }
    for (case, chat_id, raised) in &raises {
        let (_scratch, db) = open_copy(case);
        let (resp, lines) = rt.block_on(global_capture::capture_async(
            quilltap_core::api::chat_interrupt::chat_raise_hand(&db, chat_id, raised),
        ));
        let (chats, msgs) = dump(&db);
        compare(case, as_http(&resp), lines, chats, msgs);
    }
    // `raise_unparsable_body` is the REST edge's (a non-JSON body never reaches
    // a dispatch `Request`): `quilltap-web`'s `chat_interrupt_rest_routes`.
    assert!(
        oracle.contains_key("raise_unparsable_body"),
        "the REST-only row must still be in the oracle"
    );
    assert_eq!(
        interrupt_rows, 15,
        "every dispatchable interrupt row compared"
    );
    eprintln!("[interrupts] {interrupt_rows} rows done.");

    assert!(failed.is_empty(), "salon-skip FAILED: {failed:?}");
}
