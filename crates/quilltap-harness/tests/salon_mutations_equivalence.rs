//! P4.6a mutation-surface differential: the Salon mutation handlers
//! (`api::salon::{chat_impersonate, chat_stop_impersonate, chat_set_active_speaker,
//! turn_action, message_edit, message_delete, message_swipe_switch}`) vs v4's REAL
//! route handlers (the impersonation verbs, the turn action, and the message
//! edit/delete/swipe). Both sides run each case over a FRESH copy of the committed
//! salon fixture; the response body + the affected MAIN-db tables (`chats` /
//! `chat_messages`) are diffed byte-for-byte. Almost every case is zero-mint
//! (skipUserTurn is excluded); the exception is impersonate/stop-impersonate,
//! whose bug-27 `controlledBy` flip mints the target participant's `updatedAt`
//! (normalized to `<ts>` by [`scrub_minted_timestamps`] — the only non-seed
//! timestamps in the corpus).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-salon-mutations.ndjson npx jest -- salon-mutations
//! Run:
//!   QT_ORACLE_SALON_MUTATIONS=/tmp/oracle-salon-mutations.ndjson \
//!     cargo test -p quilltap-harness --test salon_mutations_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::salon;
use quilltap_core::api::types::Response;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

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
/// The fixture's seed timestamp (`salon.json` `seedTimestamp`). Every seed row
/// carries it; a value that ISN'T it was minted at write time.
const SEED_TS: &str = "2026-02-01T00:00:00.000Z";

/// Replace any ISO-8601 timestamp that is NOT the seed sentinel with `<ts>`.
///
/// Bug 44 (`62c63dc3`): impersonate/stop-impersonate are now a PURE OVERLAY — the
/// target participant's `controlledBy` is no longer flipped, so the participant's
/// `updatedAt` inside the `participants` JSON is NOT minted (verified: the
/// impersonate/stop cases keep the seed sentinel on the participant row). What
/// still mints is the CHAT row's own `updatedAt`: `add_impersonation` /
/// `remove_impersonation` (and the other chat-writing verbs — set-active-speaker,
/// turn, message edit/delete/swipe) write the chat, bumping `updatedAt` (and
/// `lastMessageAt`) to wall-clock on BOTH sides. So the scrubber stays load-bearing.
/// A non-seed value on ONE side only still diverges (`<ts>` vs the literal seed),
/// so "who minted" stays diffable — which is exactly what proves the participant
/// row is now UNTOUCHED by the impersonation verbs (no one-sided mint on it).
fn scrub_minted_timestamps(s: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap());
    re.replace_all(s, |caps: &regex::Captures| {
        let m = &caps[0];
        if m == SEED_TS {
            m.to_string()
        } else {
            "<ts>".to_string()
        }
    })
    .into_owned()
}

fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    scrub_minted_timestamps(&serde_json::to_string_pretty(&sorted(&v)).unwrap())
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
/// Every id baked into the salon fixture carries this middle (`…-0000-4000-8000-…`),
/// so an id WITHOUT it is a row the run minted — today, only the P4.D141 Concierge
/// manual bubble. Minted ids differ between the two implementations by
/// construction, so they are placeholdered; minted rows also sort LAST rather than
/// wherever their random first group happens to fall, which would otherwise make
/// the row ORDER differ between sides.
const SEEDED_ID_MIDDLE: &str = "-0000-4000-8000-";

fn is_minted_id(row: &Value) -> bool {
    row.get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.contains(SEEDED_ID_MIDDLE))
}

/// Extract the `rows` array (sorted by id, `renderedHtml` stripped) from a
/// dump/oracle `{table, columns, rows}` object.
fn table_rows(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.sort_by_key(|r| {
        (
            is_minted_id(r),
            r.get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        )
    });
    for r in &mut rows {
        let minted = is_minted_id(r);
        if let Some(o) = r.as_object_mut() {
            o.remove("renderedHtml");
            if minted {
                o.insert("id".into(), Value::String("<id>".into()));
            }
        }
    }
    Value::Array(rows)
}

fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

#[test]
fn salon_mutations_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SALON_MUTATIONS") else {
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

    let uid = spec.user_id.clone();
    const GROUP: &str = "c1000000-0000-4000-8000-000000000002";
    const USER_P: &str = "b2000000-0000-4000-8000-000000000003";
    const LLM_P: &str = "b2000000-0000-4000-8000-000000000001";
    const EDIT_MSG: &str = "d2000000-0000-4000-8000-000000000002";
    const SWIPE_MSG: &str = "d2000000-0000-4000-8000-000000000005";

    // Run one case over a fresh fixture copy; return (body, chats_rows, msgs_rows).
    let run = |case: &str, f: &dyn Fn(&Db) -> Response| -> (Value, Value, Value) {
        let scratch =
            std::env::temp_dir().join(format!("qt-salon-mut-{}-{}", std::process::id(), case));
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
        let body = response_data(&f(&db));
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

    // (name, run closure)
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, Box<dyn Fn(&Db) -> Response>)> = vec![
        (
            "impersonate",
            Box::new(|db: &Db| rt.block_on(salon::chat_impersonate(db, GROUP, USER_P))),
        ),
        (
            "stop_impersonate",
            Box::new(|db: &Db| rt.block_on(salon::chat_stop_impersonate(db, GROUP, USER_P, None))),
        ),
        (
            "set_active_speaker",
            Box::new(|db: &Db| rt.block_on(salon::chat_set_active_speaker(db, GROUP, USER_P))),
        ),
        (
            "turn_query",
            Box::new(|db: &Db| rt.block_on(salon::turn_action(db, GROUP, "query", None, 0.42))),
        ),
        (
            "turn_nudge",
            Box::new(|db: &Db| {
                rt.block_on(salon::turn_action(db, GROUP, "nudge", Some(LLM_P), 0.42))
            }),
        ),
        (
            "message_edit",
            Box::new(|db: &Db| {
                rt.block_on(salon::message_edit(
                    db,
                    &uid,
                    EDIT_MSG,
                    "I stride toward the ridge, resolute.",
                ))
            }),
        ),
        (
            "message_delete_confirm",
            Box::new(|db: &Db| rt.block_on(salon::message_delete(db, &uid, EDIT_MSG, None, false))),
        ),
        (
            "message_delete_swipe",
            Box::new(|db: &Db| {
                rt.block_on(salon::message_delete(db, &uid, SWIPE_MSG, None, false))
            }),
        ),
        (
            "message_swipe_switch",
            Box::new(|db: &Db| salon::message_swipe_switch(db, &uid, SWIPE_MSG, 1)),
        ),
        (
            "chat_update",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "isPaused": true, "title": "Paused Expedition" }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_broad",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({
                        "title": "Broadly Reconfigured",
                        "contextSummary": "A terse recap.",
                        "isManuallyRenamed": true,
                        "documentEditingMode": true,
                        "allowCrossCharacterVaultReads": true,
                        "coreWhisperEnabled": false,
                        "coreWhisperInterval": 5,
                        "turnSkippingEnabled": false,
                        "showThinking": true,
                        "answerConfirmationOverride": "OFF",
                        "documentMode": "split",
                        "dividerPosition": 60,
                        "terminalMode": "focus",
                        "activeTerminalSessionId": null,
                        "rightPaneVerticalSplit": 40,
                        "alertCharactersOfLanternImages": true,
                        "imageProfileId": null,
                    }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_roleplay_404",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "roleplayTemplateId": "99999999-9999-4999-8999-999999999999" }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_project_404",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "projectId": "99999999-9999-4999-8999-999999999999" }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_timeline_set",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "timelineMode": "narrative" }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_timeline_null",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "timelineMode": null }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_timeline_invalid",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "timelineMode": "dreamtime" }),
                    None, // conciergeState (P4.D141) — its own cases below
                    // P4.9E1A: the bag's three participant families (unused here).
                    None,
                    None,
                    None,
                ))
            }),
        ),
        // ── P4.D141: the `conciergeState` arm of the chat PUT ──
        (
            "chat_update_concierge_flagged",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&serde_json::json!("flagged")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_vouched",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&serde_json::json!("vouched")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_uncensored",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&serde_json::json!("uncensored")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_noop",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&serde_json::json!("monitored")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_invalid",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&serde_json::json!("off")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_null",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&Value::Null),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_wrong_type",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({}),
                    Some(&serde_json::json!(42)),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_with_bag",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "title": "Vouched And Renamed" }),
                    Some(&serde_json::json!("vouched")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "chat_update_concierge_invalid_with_bag",
            Box::new(|db: &Db| {
                rt.block_on(salon::chat_update(
                    db,
                    &uid,
                    GROUP,
                    &serde_json::json!({ "title": "Should Not Land" }),
                    Some(&serde_json::json!("off")),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "message_delete_cascade",
            Box::new(|db: &Db| {
                rt.block_on(salon::message_delete(
                    db,
                    &uid,
                    EDIT_MSG,
                    Some("DELETE_MEMORIES"),
                    false,
                ))
            }),
        ),
    ];

    let mut failed = Vec::new();
    for (name, f) in &cases {
        let want = &oracle[*name];
        let (got_body, got_chats, got_msgs) = run(name, f.as_ref());
        // Body: v4 error responses are `{ error: msg }`; the v5 typed shape is
        // `{ kind, message }`. Compare the copy when the oracle body is an error.
        if let Some(err) = want["body"].get("error").and_then(Value::as_str) {
            let got_msg = got_body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("");
            if got_msg != err {
                eprintln!(
                    "[{name}] section `body` error-copy MISMATCH:\n  GOT : {got_msg}\n  WANT: {err}"
                );
                failed.push(format!("{name}/body"));
            }
        } else {
            let g = norm(&got_body);
            let w = norm(&want["body"]);
            if g != w {
                eprintln!("[{name}] section `body` MISMATCH:\n{}", first_diff(&g, &w));
                failed.push(format!("{name}/body"));
            }
        }
        let got_chat_rows = table_rows(&got_chats);
        let want_chat_rows = table_rows(&want["tables"]["chats"]);
        let sections: [(&str, Value, Value); 2] = [
            ("chats", got_chat_rows, want_chat_rows),
            (
                "chat_messages",
                table_rows(&got_msgs),
                table_rows(&want["tables"]["chatMessages"]),
            ),
        ];
        for (section, got, want_v) in &sections {
            let g = norm(got);
            let w = norm(want_v);
            if g != w {
                eprintln!(
                    "[{name}] section `{section}` MISMATCH:\n{}",
                    first_diff(&g, &w)
                );
                failed.push(format!("{name}/{section}"));
            }
        }
        eprintln!("[{name}] done.");
    }
    assert!(failed.is_empty(), "salon-mutations FAILED: {failed:?}");
}
