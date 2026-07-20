//! P4.d10 §A state-verbs differential: `api::salon::chat_state_*` +
//! `api::groups::group_state_*` + `api::settings::general_state_*` vs v4's REAL
//! route modules (chats/[id] + groups/[id] `?action=*-state`, the
//! settings/general-state route) at `7e6d13e5`, over fresh copies of the
//! state-sql-tools fixture family.
//!
//! Comparison: `(status, body[, afterBody])` per case — bodies compared as
//! SERIALIZED strings (preserve_order → key-order-exact; the chat cascade
//! body's omit-when-empty arms included). Validation-failure bodies have the
//! Zod `details` array dropped oracle-side (the settings-routes precedent) —
//! the v5 envelope is the flat `{error}`.
//!
//! Generate (from the PINNED v4 worktree — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-state-routes.ndjson npx jest -- state-routes
//! Run:
//!   QT_ORACLE_STATE_ROUTES=/tmp/oracle-state-routes.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!     cargo test -p quilltap-harness --test state_routes_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::api::{groups, salon, settings};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    chat_project_id: String,
    chat_solo_id: String,
    chat_union_id: String,
    group_alpha_id: String,
}

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    status: i64,
    body: Value,
    #[serde(default, rename = "afterBody")]
    after_body: Option<Value>,
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

fn clear(p: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
    }
}

fn fresh_db(spec: &Spec, main_fx: &str, mount_fx: &str, tag: &str) -> (Db, PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let main = dir.join(format!("qt-stateroutes-main-{pid}-{tag}.db"));
    let mount = dir.join(format!("qt-stateroutes-mount-{pid}-{tag}.db"));
    for p in [&main, &mount] {
        clear(p);
    }
    std::fs::copy(main_fx, &main).unwrap();
    std::fs::copy(mount_fx, &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: Some(mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    (db, main, mount)
}

/// Map a `Response` to the v4 route `(status, body)` pair.
fn to_status_body(r: &Response) -> (i64, Value) {
    match r {
        Response::State(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response variant: {other:?}"),
    }
}

const BOGUS: &str = "deadbeef-0000-4000-8000-000000000000";

#[tokio::test]
async fn state_routes_match_oracle() {
    let (Some(oracle_path), Some(main_fx), Some(mount_fx)) = (
        env_or_skip("QT_ORACLE_STATE_ROUTES"),
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

    enum Op {
        ChatGet(String),
        ChatSet(String, Value),
        ChatReset(String),
        GroupGet(String),
        GroupSet(String, Value),
        GroupReset(String),
        GeneralGet,
        GeneralSet(Value),
        GeneralReset,
    }

    struct Case {
        label: &'static str,
        op: Op,
        clear_general_pointer: bool,
        after: bool,
    }

    let cases = vec![
        Case {
            label: "chat_get_state_full",
            op: Op::ChatGet(spec.chat_project_id.clone()),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "chat_get_state_solo",
            op: Op::ChatGet(spec.chat_solo_id.clone()),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "chat_get_state_union_ambiguous",
            op: Op::ChatGet(spec.chat_union_id.clone()),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "chat_get_state_missing",
            op: Op::ChatGet(BOGUS.into()),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "chat_set_state",
            op: Op::ChatSet(
                spec.chat_project_id.clone(),
                json!({ "hp": 1, "fresh": true }),
            ),
            clear_general_pointer: false,
            after: true,
        },
        Case {
            label: "chat_set_state_invalid",
            op: Op::ChatSet(spec.chat_project_id.clone(), json!(5)),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "chat_set_state_missing",
            op: Op::ChatSet(BOGUS.into(), json!({})),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "chat_reset_state",
            op: Op::ChatReset(spec.chat_project_id.clone()),
            clear_general_pointer: false,
            after: true,
        },
        Case {
            label: "group_get_state",
            op: Op::GroupGet(spec.group_alpha_id.clone()),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "group_get_state_missing",
            op: Op::GroupGet(BOGUS.into()),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "group_set_state",
            op: Op::GroupSet(spec.group_alpha_id.clone(), json!({ "banner": "up" })),
            clear_general_pointer: false,
            after: true,
        },
        Case {
            label: "group_set_state_invalid",
            op: Op::GroupSet(spec.group_alpha_id.clone(), json!([1, 2])),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "group_reset_state",
            op: Op::GroupReset(spec.group_alpha_id.clone()),
            clear_general_pointer: false,
            after: true,
        },
        Case {
            label: "general_get_state",
            op: Op::GeneralGet,
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "general_set_state",
            op: Op::GeneralSet(json!({ "fog": 3, "nested": { "a": 1 } })),
            clear_general_pointer: false,
            after: true,
        },
        Case {
            label: "general_set_state_invalid",
            op: Op::GeneralSet(json!("nope")),
            clear_general_pointer: false,
            after: false,
        },
        Case {
            label: "general_reset_state",
            op: Op::GeneralReset,
            clear_general_pointer: false,
            after: true,
        },
        Case {
            label: "general_set_unprovisioned",
            op: Op::GeneralSet(json!({ "a": 1 })),
            clear_general_pointer: true,
            after: false,
        },
        Case {
            label: "general_get_unprovisioned",
            op: Op::GeneralGet,
            clear_general_pointer: true,
            after: false,
        },
    ];

    for c in cases {
        let (db, main_p, mount_p) = fresh_db(&spec, &main_fx, &mount_fx, c.label);
        if c.clear_general_pointer {
            db.write(|writers| {
                writers.main().connection().execute(
                    "DELETE FROM \"instance_settings\" WHERE \"key\" = ?1",
                    ["generalMountPointId"],
                )?;
                Ok(())
            })
            .await
            .expect("clear pointer");
        }

        let resp = match &c.op {
            Op::ChatGet(id) => salon::chat_state_get(&db, id).await,
            Op::ChatSet(id, state) => salon::chat_state_set(&db, id, state.clone()).await,
            Op::ChatReset(id) => salon::chat_state_reset(&db, id).await,
            Op::GroupGet(id) => groups::group_state_get(&db, id),
            Op::GroupSet(id, state) => groups::group_state_set(&db, id, state.clone()).await,
            Op::GroupReset(id) => groups::group_state_reset(&db, id).await,
            Op::GeneralGet => settings::general_state_get(&db).await,
            Op::GeneralSet(state) => settings::general_state_set(&db, state.clone()).await,
            Op::GeneralReset => settings::general_state_reset(&db).await,
        };
        let (status, body) = to_status_body(&resp);

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(status, want.status, "status diverged for {}", c.label);
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            serde_json::to_string(&want.body).unwrap(),
            "body diverged for {}",
            c.label
        );

        if c.after {
            let after_resp = match &c.op {
                Op::ChatGet(id) | Op::ChatSet(id, _) | Op::ChatReset(id) => {
                    salon::chat_state_get(&db, id).await
                }
                Op::GroupGet(id) | Op::GroupSet(id, _) | Op::GroupReset(id) => {
                    groups::group_state_get(&db, id)
                }
                Op::GeneralGet | Op::GeneralSet(_) | Op::GeneralReset => {
                    settings::general_state_get(&db).await
                }
            };
            let (_, after_body) = to_status_body(&after_resp);
            let want_after = want
                .after_body
                .clone()
                .unwrap_or_else(|| panic!("oracle missing afterBody for {}", c.label));
            assert_eq!(
                serde_json::to_string(&after_body).unwrap(),
                serde_json::to_string(&want_after).unwrap(),
                "afterBody diverged for {}",
                c.label
            );
        }

        drop(db);
        clear(&main_p);
        clear(&mount_p);
    }

    eprintln!(
        "OK: state-routes differential matched the oracle ({} rows).",
        oracle.len()
    );
}
