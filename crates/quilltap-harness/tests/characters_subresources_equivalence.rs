//! P4.6f characters sub-resource-mutation differential: the prompts / scenarios /
//! plugin-data mutation handlers (`api::characters::character_{prompt,scenario,
//! plugin_data}_*`) vs v4's REAL route handlers. The underlying repo ops are
//! already proven (characters_arrays_tier2 / character_plugin_data_upsert_tier2);
//! this pins the HANDLER assembly. Each case runs on a FRESH copy of the
//! committed fixture; update/delete/set-default target an existing baked sub-item
//! resolved by name (stable across copies). The response body is diffed after
//! normalizing the minted `id`/`createdAt`/`updatedAt` on any prompt/scenario/
//! plugin-data object (the vault overlay + upsert mint these).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-characters-subresources.ndjson npx jest -- characters-subresources
//! Run:
//!   QT_ORACLE_CHARACTERS_SUBRES=/tmp/oracle-characters-subresources.ndjson \
//!     cargo test -p quilltap-harness --test characters_subresources_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::characters;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

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

/// Blank the minted `id`/`createdAt`/`updatedAt` on the sub-item object nested
/// under `prompt`/`scenario`/`pluginData` (the vault overlay / upsert mint them).
fn strip_subitem_ts(body: &mut Value) {
    if let Some(o) = body.as_object_mut() {
        for key in ["prompt", "scenario", "pluginData"] {
            if let Some(item) = o.get_mut(key).and_then(Value::as_object_mut) {
                for f in ["id", "createdAt", "updatedAt"] {
                    if item.contains_key(f) {
                        item.insert(f.into(), Value::String(format!("<{f}>")));
                    }
                }
            }
        }
    }
}

fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-char-subres-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// Resolve a baked sub-item id by name off Aria's overlaid character.
fn resolve_sub_id(db: &Db, field: &str, name_field: &str, name: &str) -> String {
    let field = field.to_string();
    let name_field = name_field.to_string();
    let name = name.to_string();
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let ch = quilltap_core::db::characters_read::find_by_id(main, mount, ARIA)?
                .unwrap_or(Value::Null);
            let id = ch
                .get(&field)
                .and_then(Value::as_array)
                .and_then(|a| {
                    a.iter()
                        .find(|e| e.get(&name_field).and_then(Value::as_str) == Some(name.as_str()))
                })
                .and_then(|e| e.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Ok(id)
        })
    })
    .unwrap()
}

#[test]
fn characters_subresources_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_CHARACTERS_SUBRES") else {
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
    let mut failed: Vec<String> = Vec::new();

    let check = |name: &str, resp: Response, failed: &mut Vec<String>| {
        let mut got = response_data(&resp);
        let mut want = oracle[name]["body"].clone();
        strip_subitem_ts(&mut got);
        strip_subitem_ts(&mut want);
        if norm(&got) != norm(&want) {
            eprintln!(
                "[{name}] MISMATCH:\n  GOT : {}\n  WANT: {}",
                norm(&got),
                norm(&want)
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };

    {
        let db = fresh_db(&spec, "pc");
        let r = rt.block_on(characters::character_prompt_create(
            &db,
            &uid,
            ARIA,
            "Extra",
            "A third prompt.",
            false,
        ));
        check("prompt_create", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "pu");
        let id = resolve_sub_id(&db, "systemPrompts", "name", "Backup");
        let r = rt.block_on(characters::character_prompt_update(
            &db,
            &uid,
            ARIA,
            &id,
            None,
            Some("Revised backup prompt."),
            None,
        ));
        check("prompt_update", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "pd");
        let id = resolve_sub_id(&db, "systemPrompts", "name", "Backup");
        let r = rt.block_on(characters::character_prompt_delete(&db, &uid, ARIA, &id));
        check("prompt_delete", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sc");
        let r = rt.block_on(characters::character_scenario_create(
            &db,
            &uid,
            ARIA,
            "Epilogue",
            "The voyage ends.",
            None,
        ));
        check("scenario_create", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "su");
        let id = resolve_sub_id(&db, "scenarios", "title", "Prologue");
        let r = rt.block_on(characters::character_scenario_update(
            &db,
            &uid,
            ARIA,
            &id,
            None,
            Some("A revised prologue."),
            None,
        ));
        check("scenario_update", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sd");
        let id = resolve_sub_id(&db, "scenarios", "title", "Interlude");
        let r = rt.block_on(characters::character_scenario_delete(&db, &uid, ARIA, &id));
        check("scenario_delete", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "pue");
        let r = rt.block_on(characters::character_plugin_data_upsert(
            &db,
            &uid,
            ARIA,
            "com.example.notes",
            json!({ "color": "crimson" }),
        ));
        check("plugin_upsert_existing", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "pun");
        let r = rt.block_on(characters::character_plugin_data_upsert(
            &db,
            &uid,
            ARIA,
            "com.example.flags",
            json!({ "beta": true }),
        ));
        check("plugin_upsert_new", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "pdel");
        let r = rt.block_on(characters::character_plugin_data_delete(
            &db,
            &uid,
            ARIA,
            "com.example.notes",
        ));
        check("plugin_delete", r, &mut failed);
    }

    // ── P4.D120 / v4 `d25dacc1` — the archived tri-state on the RETURNED object.
    // `characters_arrays_tier2` cannot see it: a vault-linked character keeps an
    // EMPTY `scenarios` DB column and `build_scenario_file` never emits
    // `archived: false`, so an intermediate `false` is invisible on disk. The
    // RESPONSE is the only place the key's presence and value are observable —
    // and v4's `updateScenario` genuinely DOES echo `archived: false` on a
    // restore (measured, not reasoned about).
    {
        let db = fresh_db(&spec, "sc_arch");
        let r = rt.block_on(characters::character_scenario_create(
            &db,
            &uid,
            ARIA,
            "Mothballed",
            "A scene put away.",
            Some(true),
        ));
        check("scenario_create_archived", r, &mut failed);
    }
    {
        let db = fresh_db(&spec, "sc_act");
        let r = rt.block_on(characters::character_scenario_create(
            &db,
            &uid,
            ARIA,
            "Explicit",
            "Not archived.",
            Some(false),
        ));
        check("scenario_create_explicitly_active", r, &mut failed);
    }
    for (name, tag, pre_archived, archived, content) in [
        (
            "scenario_update_archives",
            "su_arch",
            None,
            Some(true),
            None,
        ),
        (
            "scenario_update_restores",
            "su_rest",
            None,
            Some(false),
            None,
        ),
        // Archive FIRST, then edit without mentioning `archived`: without the
        // pre-call the target is ACTIVE and "preserve" is indistinguishable from
        // "default to false".
        (
            "scenario_update_omitting_archived",
            "su_omit",
            Some(true),
            None,
            Some("Untouched flag."),
        ),
    ] {
        let db = fresh_db(&spec, tag);
        let id = resolve_sub_id(&db, "scenarios", "title", "Prologue");
        if let Some(pre) = pre_archived {
            let _ = rt.block_on(characters::character_scenario_update(
                &db,
                &uid,
                ARIA,
                &id,
                None,
                None,
                Some(pre),
            ));
        }
        let r = rt.block_on(characters::character_scenario_update(
            &db, &uid, ARIA, &id, None, content, archived,
        ));
        check(name, r, &mut failed);
    }

    // Every oracle row must have been DRIVEN — a case added to the corpus and
    // not to this list would otherwise pass in silence.
    assert_eq!(
        oracle.len(),
        14,
        "the shared corpus grew; add the new case(s) to this list"
    );
    assert!(
        failed.is_empty(),
        "characters-subresources FAILED: {failed:?}"
    );
}
