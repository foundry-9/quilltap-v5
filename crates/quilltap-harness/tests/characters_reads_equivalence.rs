//! P4.6f characters read-surface differential: the character reads
//! (`api::characters::{character_list, character_get, character_default_partner,
//! character_get_tags, character_prompt_list, character_scenario_list,
//! character_wardrobe_list, character_plugin_data_map, character_plugin_data_get}`)
//! vs v4's REAL characters route handlers. Both sides read a FRESH copy of the
//! committed characters fixture (identical baked ids → no remap); the response
//! `data` payload is diffed after number canonicalization + key sort.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-characters-reads.ndjson npx jest -- characters-reads
//! Run:
//!   QT_ORACLE_CHARACTERS_READS=/tmp/oracle-characters-reads.ndjson \
//!     cargo test -p quilltap-harness --test characters_reads_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::characters;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/characters.json")
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
    "(identical line-by-line)".to_string()
}
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

/// The overlaid character's `physicalDescription.createdAt`/`updatedAt` are
/// MINTED at read time (the vault overlay synthesizes a default block when the
/// character has none), so they legitimately differ between the jest oracle read
/// and the Rust read — the established characters-read normalization
/// (`[[project-store-fixture-needs-real-create]]`). Blank them on both sides.
fn strip_physical_ts(v: &mut Value) {
    if let Some(pd) = v
        .get_mut("character")
        .and_then(|c| c.get_mut("physicalDescription"))
        .and_then(Value::as_object_mut)
    {
        pd.remove("createdAt");
        pd.remove("updatedAt");
    }
}

#[test]
fn characters_reads_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHARACTERS_READS") else {
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

    let uid = &spec.user_id;
    let scratch = std::env::temp_dir().join(format!("qt-characters-reads-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");

    // (name, got payload, want payload). Reads carry the FULL body as the payload.
    let mut cases: Vec<(String, Value, Value)> = Vec::new();
    let mut push = |name: &str, got: Value| {
        let want = oracle[name]["body"].clone();
        cases.push((name.to_string(), got, want));
    };

    push(
        "list_all",
        response_data(&characters::character_list(&db, uid, None, None, None)),
    );
    push(
        "list_npc_true",
        response_data(&characters::character_list(
            &db,
            uid,
            None,
            Some("true"),
            None,
        )),
    );
    push(
        "list_npc_false",
        response_data(&characters::character_list(
            &db,
            uid,
            None,
            Some("false"),
            None,
        )),
    );
    push(
        "list_user",
        response_data(&characters::character_list(
            &db,
            uid,
            None,
            None,
            Some("user"),
        )),
    );
    push(
        "list_llm",
        response_data(&characters::character_list(
            &db,
            uid,
            None,
            None,
            Some("llm"),
        )),
    );
    // The archived chokepoint (P4.D63). `list_all` above is the EXCLUDE
    // default and is what proves a tombstone stays out of every picker.
    push(
        "list_archived_only",
        response_data(&characters::character_list(
            &db,
            uid,
            Some("only"),
            None,
            None,
        )),
    );
    push(
        "list_archived_include",
        response_data(&characters::character_list(
            &db,
            uid,
            Some("include"),
            None,
            None,
        )),
    );
    push(
        "list_archived_garbage",
        response_data(&characters::character_list(
            &db,
            uid,
            Some("nonsense"),
            None,
            None,
        )),
    );
    push(
        "list_archived_include_llm",
        response_data(&characters::character_list(
            &db,
            uid,
            Some("include"),
            None,
            Some("llm"),
        )),
    );
    push(
        "list_archived_only_npc_false",
        response_data(&characters::character_list(
            &db,
            uid,
            Some("only"),
            Some("false"),
            None,
        )),
    );
    push(
        "get_aria",
        response_data(&characters::character_get(&db, uid, ARIA)),
    );
    push(
        "default_partner",
        response_data(&characters::character_default_partner(&db, uid, ARIA)),
    );
    push(
        "get_tags",
        response_data(&characters::character_get_tags(&db, uid, ARIA)),
    );
    push(
        "prompts",
        response_data(&characters::character_prompt_list(&db, uid, ARIA)),
    );
    push(
        "scenarios",
        response_data(&characters::character_scenario_list(&db, uid, ARIA)),
    );
    push(
        "wardrobe",
        response_data(&characters::character_wardrobe_list(&db, uid, ARIA)),
    );
    push(
        "plugin_data_map",
        response_data(&characters::character_plugin_data_map(&db, uid, ARIA)),
    );
    push(
        "plugin_data_item",
        response_data(&characters::character_plugin_data_get(
            &db,
            uid,
            ARIA,
            "com.example.notes",
        )),
    );
    push(
        "stats",
        response_data(&characters::character_stats(&db, uid, ARIA)),
    );
    push(
        "depiction_guidelines",
        response_data(&characters::character_depiction_guidelines(&db, uid, ARIA)),
    );
    // P4.6i: the enriched recent-chats DTO (plain / search / pagination).
    push(
        "chats_plain",
        response_data(&characters::character_chats(
            &db, uid, ARIA, None, None, None,
        )),
    );
    push(
        "chats_search_title",
        response_data(&characters::character_chats(
            &db,
            uid,
            ARIA,
            Some("voyage"),
            None,
            None,
        )),
    );
    push(
        "chats_search_content",
        response_data(&characters::character_chats(
            &db,
            uid,
            ARIA,
            Some("traveller"),
            None,
            None,
        )),
    );
    push(
        "chats_search_miss",
        response_data(&characters::character_chats(
            &db,
            uid,
            ARIA,
            Some("zzzznope"),
            None,
            None,
        )),
    );
    push(
        "chats_limit0",
        response_data(&characters::character_chats(
            &db,
            uid,
            ARIA,
            None,
            Some(0),
            None,
        )),
    );
    push(
        "chats_offset1",
        response_data(&characters::character_chats(
            &db,
            uid,
            ARIA,
            None,
            None,
            Some(1),
        )),
    );
    // P4.6i: ST export (JSON leg).
    push(
        "export_json",
        response_data(&characters::character_export(&db, uid, ARIA, Some("json"))),
    );
    // P4.6i: the character photo gallery listing.
    push(
        "photo_list",
        response_data(&characters::character_photo_list(
            &db, uid, ARIA, None, None,
        )),
    );
    // P4.6i: the cascade-delete preview.
    push(
        "cascade_preview",
        response_data(&characters::character_cascade_preview(&db, uid, ARIA)),
    );

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    // Normalize the read-time-minted physicalDescription timestamps on the detail.
    for (name, got, want) in cases.iter_mut() {
        if name == "get_aria" {
            strip_physical_ts(got);
            strip_physical_ts(want);
        }
    }

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
    assert!(failed.is_empty(), "characters-reads FAILED: {failed:?}");
}
