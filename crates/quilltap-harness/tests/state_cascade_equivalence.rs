//! Differential test (P4.d10 units 1–2): the state-family modules —
//! `quilltap_core::state::{paths, cascade}` + `services::mount_index::general_state`
//! — vs v4's REAL `lib/state/state-paths.ts` + `state-cascade.ts` +
//! `lib/mount-index/general-state.ts` at the round baseline `7e6d13e5`.
//!
//! Four sections mirroring the oracle case
//! (`harness/oracle/cases/state-cascade.test.ts`):
//!   - paths   (pure, no DB): parse / get / set / delete corpora incl. the
//!     root-set throw, the array splice, and the regex-grammar edge cases.
//!   - general (fresh three-DB copies per case): seeded read, ensure
//!     (existing / absent / unprovisioned), write round-trip, corrupt / array /
//!     null bodies, missing doc, the unprovisioned write error VERBATIM.
//!   - cascade: precedence, 0/1/2-group tiers, participants-union (removed +
//!     non-CHARACTER skips, controlledBy NOT filtered), degradation.
//!   - groupref: all four `StateGroupResolutionError` codes + id-first +
//!     case-insensitive-name policy with VERBATIM messages.
//!
//! Comparison: each case rebuilds the oracle row's `result` and compares the
//! SERIALIZED string (`serde_json::to_string`) against the re-serialized oracle
//! value — preserve_order makes that a key-order-exact comparison; the cascade
//! rows carry `resultJson` (v4 `JSON.stringify`) compared byte-for-byte.
//!
//! Generate the fixture + oracle (Node 24, from the PINNED v4 worktree; stage
//! the case outside `.claude/` first):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; WT=<v5 worktree> ; STAGE=/tmp/qt-oracle-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $WT/harness/oracle/cases/state-cascade.test.ts $STAGE/harness/oracle/cases/
//!   cp $WT/harness/oracle/fixtures/state-sql-tools.json $STAGE/harness/oracle/fixtures/
//!   cd /private/tmp/qt-v4-pin-7e6d13e5
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
//!     $N/node --import tsx $WT/harness/oracle/fixtures/build-state-sql-tools-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db QT_ORACLE_OUT=/tmp/oracle-state-cascade.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- state-cascade
//! Run:
//!   QT_ORACLE_STATE_CASCADE=/tmp/oracle-state-cascade.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!     cargo test -p quilltap-harness --test state_cascade_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::chats_read;
use quilltap_core::db::database_store::{delete_database_document, write_database_document};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::mount_index::general_state::{
    ensure_general_state_file, read_general_state, write_general_state,
};
use quilltap_core::state::cascade::{
    resolve_group_candidates, resolve_group_for_context, resolve_state_cascade, GroupScope,
};
use quilltap_core::state::paths::{delete_at_path, get_at_path, parse_path, set_at_path, PathKey};
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "chatProjectId")]
    chat_project_id: String,
    #[serde(rename = "chatSoloId")]
    chat_solo_id: String,
    #[serde(rename = "chatUnionId")]
    chat_union_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "charBId")]
    char_b_id: String,
    #[serde(rename = "charDId")]
    char_d_id: String,
    #[serde(rename = "groupTwin2Id")]
    group_twin2_id: String,
    #[serde(rename = "generalMountPointId")]
    general_mount_point_id: String,
}

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    kind: String,
    result: Value,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/state-sql-tools.json")
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

fn load_oracle(path: &str) -> HashMap<String, OracleRow> {
    let mut map = HashMap::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read oracle {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: OracleRow = serde_json::from_str(line).expect("oracle line parses");
        map.insert(row.label.clone(), row);
    }
    map
}

/// Compare a rebuilt `result` against the oracle row — string-compare of both
/// serializations (preserve_order → key-order-exact).
fn assert_result(oracle: &HashMap<String, OracleRow>, label: &str, kind: &str, got: &Value) {
    let want = oracle
        .get(label)
        .unwrap_or_else(|| panic!("oracle missing case {label}"));
    assert_eq!(want.kind, kind, "kind mismatch for {label}");
    assert_eq!(
        serde_json::to_string(got).unwrap(),
        serde_json::to_string(&want.result).unwrap(),
        "result diverged for {label}"
    );
}

/// JS-`undefined` made JSON-visible (the oracle's `definedProbe`).
fn defined_probe(v: Option<Value>) -> Value {
    json!({ "defined": v.is_some(), "value": v.unwrap_or(Value::Null) })
}

fn path_keys_to_value(keys: &[PathKey]) -> Value {
    Value::Array(
        keys.iter()
            .map(|k| match k {
                PathKey::Prop(s) => Value::String(s.clone()),
                PathKey::Index(n) => json!(n),
            })
            .collect(),
    )
}

// ── Section A: pure paths ────────────────────────────────────────────────────

fn run_paths_cases(oracle: &HashMap<String, OracleRow>) {
    let parse = |p: Option<&str>| path_keys_to_value(&parse_path(p));
    let cases: Vec<(&str, Value)> = vec![
        ("parse_undefined", parse(None)),
        ("parse_empty", parse(Some(""))),
        ("parse_whitespace", parse(Some("   "))),
        ("parse_dots", parse(Some("player.health"))),
        ("parse_index", parse(Some("inventory[0].name"))),
        ("parse_double_index", parse(Some("a[2][3]"))),
        ("parse_underscore", parse(Some("_secret.x"))),
        ("parse_hyphen_splits", parse(Some("weird-key"))),
        ("parse_space_splits", parse(Some("a b"))),
        ("parse_alpha_index", parse(Some("x[abc]"))),
        ("parse_unclosed_index", parse(Some("x[1"))),
        ("parse_bare_index", parse(Some("[0]"))),
        ("parse_non_ascii", parse(Some("é.x"))),
        (
            "get_root",
            defined_probe(get_at_path(&json!({"a": 1}), &[])),
        ),
        (
            "get_nested",
            defined_probe(get_at_path(
                &json!({"player": {"inv": [{"name": "sword"}]}}),
                &parse_path(Some("player.inv[0].name")),
            )),
        ),
        (
            "get_missing",
            defined_probe(get_at_path(&json!({"a": 1}), &parse_path(Some("a.b")))),
        ),
        (
            "get_null_leaf",
            defined_probe(get_at_path(
                &json!({"a": {"b": null}}),
                &parse_path(Some("a.b")),
            )),
        ),
        (
            "get_null_mid",
            defined_probe(get_at_path(&json!({"a": null}), &parse_path(Some("a.b")))),
        ),
        (
            "get_string_key_on_array",
            defined_probe(get_at_path(
                &json!({"l": [1, 2]}),
                &parse_path(Some("l.foo")),
            )),
        ),
        ("set_nested_create", {
            let mut obj = json!({});
            set_at_path(&mut obj, &parse_path(Some("player.stats.hp")), json!(5)).unwrap();
            obj
        }),
        ("set_array_create", {
            let mut obj = json!({});
            set_at_path(&mut obj, &parse_path(Some("list[2]")), json!("x")).unwrap();
            obj
        }),
        ("set_overwrites_primitive_mid_path", {
            let mut obj = json!({"a": 5});
            set_at_path(&mut obj, &parse_path(Some("a.b")), json!(1)).unwrap();
            obj
        }),
        ("set_root_object", {
            let mut obj = json!({"a": 1});
            set_at_path(&mut obj, &[], json!({"b": 2})).unwrap();
            obj
        }),
        ("set_root_non_object_throws", {
            let mut obj = json!({});
            match set_at_path(&mut obj, &[], json!(5)) {
                Ok(()) => json!({"threw": false}),
                Err(m) => json!({"threw": true, "message": m}),
            }
        }),
        ("set_root_array_throws", {
            let mut obj = json!({});
            match set_at_path(&mut obj, &[], json!([1, 2])) {
                Ok(()) => json!({"threw": false}),
                Err(m) => json!({"threw": true, "message": m}),
            }
        }),
        ("delete_object_key", {
            let mut obj = json!({"player": {"health": 10, "mana": 3}});
            let deleted = delete_at_path(&mut obj, &parse_path(Some("player.mana")));
            json!({"deleted": deleted, "obj": obj})
        }),
        ("delete_middle_key_order", {
            // Pins JS `delete`'s order preservation (shift-remove, not swap-remove).
            let mut obj = json!({"a": 1, "b": 2, "c": 3, "d": 4});
            let deleted = delete_at_path(&mut obj, &parse_path(Some("b")));
            json!({"deleted": deleted, "obj": obj})
        }),
        ("delete_array_splices", {
            let mut obj = json!({"list": ["a", "b", "c"]});
            let deleted = delete_at_path(&mut obj, &parse_path(Some("list[1]")));
            json!({"deleted": deleted, "obj": obj})
        }),
        ("delete_missing", {
            let mut obj = json!({"a": 1});
            let deleted = delete_at_path(&mut obj, &parse_path(Some("b")));
            json!({"deleted": deleted, "obj": obj})
        }),
        ("delete_root_refused", {
            let mut obj = json!({"a": 1});
            let deleted = delete_at_path(&mut obj, &[]);
            json!({"deleted": deleted, "obj": obj})
        }),
        ("delete_primitive_mid_path", {
            let mut obj = json!({"a": 1});
            let deleted = delete_at_path(&mut obj, &parse_path(Some("a.deep")));
            json!({"deleted": deleted, "obj": obj})
        }),
        ("delete_string_key_on_array", {
            let mut obj = json!({"l": [1, 2]});
            let deleted = delete_at_path(&mut obj, &parse_path(Some("l.foo")));
            json!({"deleted": deleted, "obj": obj})
        }),
    ];
    for (label, got) in &cases {
        assert_result(oracle, label, "paths", got);
    }
}

// ── DB plumbing ──────────────────────────────────────────────────────────────

fn fresh_copy(main_fx: &str, mount_fx: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let main = dir.join(format!("qt-cascade-main-rust-{pid}-{tag}.db"));
    let mount = dir.join(format!("qt-cascade-mount-rust-{pid}-{tag}.db"));
    for p in [&main, &mount] {
        clear(p);
    }
    std::fs::copy(main_fx, &main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fx, &mount).unwrap_or_else(|e| panic!("copy mount: {e}"));
    (main, mount)
}

fn clear(p: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
    }
}

fn open_two_db(main: &Path, mount: &Path, pepper: &str) -> Db {
    Db::open(
        DbPaths {
            main: main.to_path_buf(),
            mount_index: Some(mount.to_path_buf()),
            llm_logs: None,
        },
        pepper,
    )
    .unwrap_or_else(|e| panic!("open two-db: {e}"))
}

/// Run `body` on a fresh fixture copy inside one write closure and return its
/// rebuilt result Value.
async fn with_fresh_dbs<F>(spec: &Spec, main_fx: &str, mount_fx: &str, tag: &str, body: F) -> Value
where
    F: FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Value + Send + 'static,
{
    let (main, mount) = fresh_copy(main_fx, mount_fx, tag);
    let db = open_two_db(&main, &mount, &spec.test_pepper_base64);
    let got = db
        .write(move |writers| {
            let mount_conn = writers
                .mount_index()
                .expect("mount-index present")
                .connection();
            let main_conn = writers.main().connection();
            Ok(body(main_conn, mount_conn))
        })
        .await
        .expect("case body");
    drop(db);
    clear(&main);
    clear(&mount);
    got
}

// ── Section B: general-state ─────────────────────────────────────────────────

async fn run_general_cases(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fx: &str,
    mount_fx: &str,
) {
    let mp = spec.general_mount_point_id.clone();

    type CaseBody = Box<dyn FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Value + Send>;
    let cases: Vec<(&str, CaseBody)> = vec![
        (
            "gen_read_seeded",
            Box::new(|main, mount| read_general_state(main, Some(mount))),
        ),
        (
            "gen_ensure_existing",
            Box::new(
                |main, mount| json!({"created": ensure_general_state_file(main, mount).expect("ensure")}),
            ),
        ),
        ("gen_ensure_absent_seeds", {
            let mp = mp.clone();
            Box::new(move |main, mount| {
                delete_database_document(mount, &mp, "state.json").expect("delete doc");
                let created = ensure_general_state_file(main, mount).expect("ensure");
                let read_back = read_general_state(main, Some(mount));
                json!({"created": created, "readBack": read_back})
            })
        }),
        (
            "gen_write_roundtrip",
            Box::new(|main, mount| {
                write_general_state(
                    main,
                    mount,
                    &json!({"weather_default": "fog", "depth": {"z": 3}}),
                )
                .expect("write");
                read_general_state(main, Some(mount))
            }),
        ),
        ("gen_corrupt_body", {
            let mp = mp.clone();
            Box::new(move |main, mount| {
                write_database_document(mount, &mp, "state.json", "{ not json").expect("write");
                read_general_state(main, Some(mount))
            })
        }),
        ("gen_array_body", {
            let mp = mp.clone();
            Box::new(move |main, mount| {
                write_database_document(mount, &mp, "state.json", "[1,2,3]").expect("write");
                read_general_state(main, Some(mount))
            })
        }),
        ("gen_null_body", {
            let mp = mp.clone();
            Box::new(move |main, mount| {
                write_database_document(mount, &mp, "state.json", "null").expect("write");
                read_general_state(main, Some(mount))
            })
        }),
        ("gen_missing_doc", {
            let mp = mp.clone();
            Box::new(move |main, mount| {
                delete_database_document(mount, &mp, "state.json").expect("delete doc");
                read_general_state(main, Some(mount))
            })
        }),
        (
            "gen_unprovisioned",
            Box::new(|main, mount| {
                main.execute(
                    "DELETE FROM \"instance_settings\" WHERE \"key\" = ?1",
                    ["generalMountPointId"],
                )
                .expect("clear pointer");
                let ensured = ensure_general_state_file(main, mount).expect("ensure");
                let read = read_general_state(main, Some(mount));
                let write_error = match write_general_state(main, mount, &json!({"a": 1})) {
                    Ok(()) => Value::Null,
                    Err(e) => Value::String(e.to_string()),
                };
                json!({"ensured": ensured, "read": read, "writeError": write_error})
            }),
        ),
    ];

    for (label, body) in cases {
        let got = with_fresh_dbs(spec, main_fx, mount_fx, label, body).await;
        assert_result(oracle, label, "general", &got);
    }
}

// ── Section C: cascade ───────────────────────────────────────────────────────

/// Serialize a `StateCascadeResult` exactly as v4's return-literal
/// `JSON.stringify` does (projectId omitted when undefined).
fn cascade_result_json(r: &quilltap_core::state::cascade::StateCascadeResult) -> String {
    let mut out = Map::new();
    out.insert("chatState".into(), r.chat_state.clone());
    out.insert("projectState".into(), r.project_state.clone());
    out.insert("groupState".into(), r.group_state.clone());
    out.insert("generalState".into(), r.general_state.clone());
    out.insert("merged".into(), r.merged.clone());
    out.insert(
        "groupTier".into(),
        serde_json::to_value(&r.group_tier).expect("groupTier"),
    );
    if let Some(pid) = &r.project_id {
        out.insert("projectId".into(), Value::String(pid.clone()));
    }
    serde_json::to_string(&Value::Object(out)).expect("cascade json")
}

async fn run_cascade_cases(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fx: &str,
    mount_fx: &str,
) {
    enum ChatSource {
        Db(String),
        Synthetic(Value),
    }
    let cases: Vec<(&str, ChatSource, GroupScope)> = vec![
        (
            "cascade_charA_single",
            ChatSource::Db(spec.chat_project_id.clone()),
            GroupScope::Character {
                character_id: spec.char_a_id.clone(),
            },
        ),
        (
            "cascade_charB_ambiguous",
            ChatSource::Db(spec.chat_project_id.clone()),
            GroupScope::Character {
                character_id: spec.char_b_id.clone(),
            },
        ),
        (
            "cascade_charD_no_groups",
            ChatSource::Db(spec.chat_project_id.clone()),
            GroupScope::Character {
                character_id: spec.char_d_id.clone(),
            },
        ),
        (
            "cascade_none_scope",
            ChatSource::Db(spec.chat_project_id.clone()),
            GroupScope::None,
        ),
        (
            "cascade_union_ambiguous",
            ChatSource::Db(spec.chat_union_id.clone()),
            GroupScope::ParticipantsUnion,
        ),
        (
            "cascade_union_single_no_project",
            ChatSource::Db(spec.chat_solo_id.clone()),
            GroupScope::ParticipantsUnion,
        ),
        (
            "cascade_synthetic_type_skip",
            ChatSource::Synthetic(json!({
                "id": "synthetic-1",
                "state": {"s": 1},
                "participants": [
                    {"type": "USER", "characterId": spec.char_b_id, "status": "active"},
                    {"type": "CHARACTER", "characterId": spec.char_a_id, "status": "active"},
                ],
            })),
            GroupScope::ParticipantsUnion,
        ),
        (
            "cascade_missing_project",
            ChatSource::Synthetic(json!({
                "id": "synthetic-2",
                "state": {"c": 1},
                "projectId": "deadbeef-0000-4000-8000-000000000000",
                "participants": [],
            })),
            GroupScope::None,
        ),
    ];

    for (label, source, scope) in cases {
        let got = with_fresh_dbs(spec, main_fx, mount_fx, label, move |main, mount| {
            let chat = match source {
                ChatSource::Db(id) => chats_read::find_by_id(main, &id)
                    .expect("chat read")
                    .unwrap_or_else(|| panic!("fixture chat missing: {id}")),
                ChatSource::Synthetic(v) => v,
            };
            let result = resolve_state_cascade(main, Some(mount), &chat, &scope);
            json!({"resultJson": cascade_result_json(&result)})
        })
        .await;
        assert_result(oracle, label, "cascade", &got);
    }
}

// ── Section D: group-ref policy ──────────────────────────────────────────────

async fn run_groupref_cases(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fx: &str,
    mount_fx: &str,
) {
    let cases: Vec<(&str, String, Option<String>)> = vec![
        ("ref_sole_omitted", spec.char_a_id.clone(), None),
        ("ref_required", spec.char_b_id.clone(), None),
        (
            "ref_by_id",
            spec.char_b_id.clone(),
            Some(spec.group_twin2_id.clone()),
        ),
        (
            "ref_by_name_ci",
            spec.char_a_id.clone(),
            Some("alpha lodge".to_string()),
        ),
        (
            "ref_ambiguous_name",
            spec.char_b_id.clone(),
            Some("TWIN LODGE".to_string()),
        ),
        (
            "ref_not_found",
            spec.char_b_id.clone(),
            Some("Zed".to_string()),
        ),
        ("ref_no_groups", spec.char_d_id.clone(), None),
        (
            "ref_whitespace",
            spec.char_b_id.clone(),
            Some("   ".to_string()),
        ),
    ];

    for (label, character_id, group_ref) in cases {
        let chat_id = spec.chat_project_id.clone();
        let got = with_fresh_dbs(spec, main_fx, mount_fx, label, move |main, mount| {
            let chat = chats_read::find_by_id(main, &chat_id)
                .expect("chat read")
                .expect("fixture chat present");
            let candidates = resolve_group_candidates(
                main,
                Some(mount),
                &chat,
                &GroupScope::Character { character_id },
            );
            match resolve_group_for_context(group_ref.as_deref(), &candidates) {
                Ok(g) => {
                    // v4: `picked.state ?? {}` — nullish coalescing.
                    let state = match g.get("state") {
                        Some(Value::Null) | None => json!({}),
                        Some(v) => v.clone(),
                    };
                    json!({"ok": true, "group": {
                        "id": g.get("id").cloned().unwrap_or(Value::Null),
                        "name": g.get("name").cloned().unwrap_or(Value::Null),
                        "state": state,
                    }})
                }
                Err(e) => json!({
                    "ok": false,
                    "code": e.code.as_str(),
                    "message": e.message,
                    "candidates": serde_json::to_value(&e.candidates).expect("candidates"),
                }),
            }
        })
        .await;
        assert_result(oracle, label, "groupref", &got);
    }
}

#[tokio::test]
async fn state_cascade_matches_oracle() {
    let (Some(oracle_path), Some(main_fx), Some(mount_fx)) = (
        env_or_skip("QT_ORACLE_STATE_CASCADE"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle = load_oracle(&oracle_path);

    run_paths_cases(&oracle);
    run_general_cases(&spec, &oracle, &main_fx, &mount_fx).await;
    run_cascade_cases(&spec, &oracle, &main_fx, &mount_fx).await;
    run_groupref_cases(&spec, &oracle, &main_fx, &mount_fx).await;

    eprintln!(
        "OK: state-cascade differential matched the oracle ({} rows).",
        oracle.len()
    );
}
