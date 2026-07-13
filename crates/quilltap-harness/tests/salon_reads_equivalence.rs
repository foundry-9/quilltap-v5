//! P4.6a read-surface differential: the three Salon reads
//! (`api::salon::{chat_settings, list_chats, chat_get}`) vs v4's REAL route
//! handlers (the chat-settings GET, `handleList`, `handleGet`). Both sides read a
//! FRESH copy of the committed salon fixture (identical baked ids → no remap); the
//! response body is diffed after (a) number canonicalization + key sort and
//! (b) stripping `renderedHtml` from the v4 side (the port OMITS it — the locked
//! markdown-render divergence).
//!
//! The dispatch contract wraps differently than v4's REST route, so the harness
//! extracts the comparable payload from each side: settings → the object; list →
//! v4 `body.chats` vs the Rust array; get → `{chat}` both.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-salon-reads.ndjson npx jest -- salon-reads
//! Run:
//!   QT_ORACLE_SALON_READS=/tmp/oracle-salon-reads.ndjson \
//!     cargo test -p quilltap-harness --test salon_reads_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::salon;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    chats: Vec<ChatSpec>,
}
#[derive(Deserialize)]
struct ChatSpec {
    id: String,
    #[serde(default)]
    tags: Vec<String>,
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
/// Strip `renderedHtml` from every message in a `{chat:{messages:[…]}}` body
/// (the v4 side always emits it; the port omits it — the locked divergence).
fn strip_rendered_html(v: &mut Value) {
    if let Some(msgs) = v
        .get_mut("chat")
        .and_then(|c| c.get_mut("messages"))
        .and_then(Value::as_array_mut)
    {
        for m in msgs {
            if let Some(o) = m.as_object_mut() {
                o.remove("renderedHtml");
            }
        }
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    let sorted = sorted(&v);
    serde_json::to_string_pretty(&sorted).unwrap()
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

/// The Rust `data` payload (adjacently-tagged Response → `{type, data}`).
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

#[test]
fn salon_reads_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SALON_READS") else {
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

    let solo = &spec.chats[0].id;
    let group = &spec.chats[1].id;
    let tag = &spec.chats[0].tags[0];
    let uid = &spec.user_id;

    // A fresh Db over a copy of the committed fixture.
    let scratch = std::env::temp_dir().join(format!("qt-salon-reads-{}", std::process::id()));
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

    // (name, got payload, want payload)
    let re_ts = Regex::new(r"^ignore$").unwrap();
    let _ = re_ts;
    let mut cases: Vec<(String, Value, Value)> = Vec::new();

    // settings
    {
        let got = response_data(&salon::chat_settings(&db, uid));
        let want = oracle["settings"]["body"].clone();
        cases.push(("settings".into(), got, want));
    }
    // list_all
    {
        let got = response_data(&salon::list_chats(&db, uid, &[], None, false));
        let want = oracle["list_all"]["body"]["chats"].clone();
        cases.push(("list_all".into(), got, want));
    }
    // list_exclude_tag
    {
        let got = response_data(&salon::list_chats(
            &db,
            uid,
            std::slice::from_ref(tag),
            None,
            false,
        ));
        let want = oracle["list_exclude_tag"]["body"]["chats"].clone();
        cases.push(("list_exclude_tag".into(), got, want));
    }
    // list_limit1
    {
        let got = response_data(&salon::list_chats(&db, uid, &[], Some(1), false));
        let want = oracle["list_limit1"]["body"]["chats"].clone();
        cases.push(("list_limit1".into(), got, want));
    }
    // get_solo
    {
        // No probe: identical to the jest oracle's empty `ptyManager` map.
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, solo, None)));
        let mut want = oracle["get_solo"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_solo".into(), got, want));
    }
    // get_group
    {
        let got = response_data(&rt.block_on(salon::chat_get(&db, uid, group, None)));
        let mut want = oracle["get_group"]["body"].clone();
        strip_rendered_html(&mut want);
        cases.push(("get_group".into(), got, want));
    }

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    let mut failed = Vec::new();
    for (name, got, want) in &cases {
        let g = norm(got);
        let w = norm(want);
        if g != w {
            eprintln!("[{name}] MISMATCH:\n{}", first_diff(&g, &w));
            failed.push(name.clone());
        } else {
            eprintln!("[{name}] OK.");
        }
    }
    assert!(failed.is_empty(), "salon-reads FAILED: {failed:?}");
}
