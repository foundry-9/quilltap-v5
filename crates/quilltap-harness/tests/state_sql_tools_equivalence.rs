//! Differential test (W4.1d5): the `state` + `run_sql` tool handlers
//! (`quilltap_core::tools::{state, run_sql}`) vs v4's REAL handlers.
//!
//! COMPARISON STRATEGY (float-safe, per the search_tools precedent): the oracle
//! emits one NDJSON line per case with `resultJson = JSON.stringify(handlerOutput)`
//! (+ `formatted`, `chatsDump`, `projectState` for state cases). This test builds
//! the same handler output, `serde_json::to_string`s it, and asserts equality —
//! identical f64 bits render identically under ryu and V8, and both sides apply
//! js-number + preserve_order.
//!
//! - `state` cases (tier-2): drive `execute_state_tool` + `format_state_results`,
//!   compare resultJson + formatted, dump the `chats` table (proves chat-state writes;
//!   no `updatedAt` mint → ZERO normalization), and read back the project's `state`
//!   (proves the project-state store-overlay write — the read-back approach per the
//!   `vault_wardrobe_public` precedent, since the overlay bytes are already proven
//!   by `projects_tier2`).
//! - `run_sql` cases (read-only): drive `execute_run_sql_tool`, diff resultJson.
//!
//! Each case runs on a FRESH copy of all three fixture DBs (state WRITES).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout). Case files under
//! any `.claude/` path are invisible to jest, so STAGE them outside first:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   WT=<this worktree root>
//!   STAGE=/tmp/qt-state-sql-tools-oracle
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $WT/harness/oracle/cases/state-sql-tools.test.ts $STAGE/harness/oracle/cases/
//!   cp $WT/harness/oracle/fixtures/state-sql-tools.json  $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
//!     $N/node --import tsx $WT/harness/oracle/fixtures/build-state-sql-tools-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db QT_ORACLE_OUT=/tmp/oracle-state-sql-tools.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- state-sql-tools
//! Run:
//!   QT_ORACLE_STATE_SQL=/tmp/oracle-state-sql-tools.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-state-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-state-mount.db \
//!   QT_FIXTURE_TMP_LLM=/tmp/qt-state-llm.db \
//!     cargo test -p quilltap-harness --test state_sql_tools_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::projects::ProjectsRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::tools::run_sql::execute_run_sql_tool;
use quilltap_core::tools::state::{execute_state_tool, format_state_results, StateToolContext};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "wrongUserId")]
    wrong_user_id: String,
    #[serde(rename = "chatProjectId")]
    chat_project_id: String,
    #[serde(rename = "chatSoloId")]
    chat_solo_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "charBId")]
    char_b_id: String,
    #[serde(rename = "charDId")]
    char_d_id: String,
    #[serde(rename = "groupAlphaId")]
    group_alpha_id: String,
    #[serde(rename = "groupTwin2Id")]
    group_twin2_id: String,
}

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    kind: String,
    #[serde(rename = "resultJson")]
    result_json: String,
    #[serde(default)]
    formatted: Option<String>,
    #[serde(default, rename = "chatsDump")]
    chats_dump: Option<Vec<Value>>,
    #[serde(default, rename = "projectState")]
    project_state: Option<Value>,
    #[serde(default, rename = "groupAlphaState")]
    group_alpha_state: Option<Value>,
    #[serde(default, rename = "generalState")]
    general_state: Option<Value>,
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

/// Fresh copies of the three-DB fixture (per case). Returns (main, mount, llm).
fn fresh_copy(
    main_fx: &str,
    mount_fx: &str,
    llm_fx: &str,
    tag: &str,
) -> (PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let main = dir.join(format!("qt-state-main-rust-{pid}-{tag}.db"));
    let mount = dir.join(format!("qt-state-mount-rust-{pid}-{tag}.db"));
    let llm = dir.join(format!("qt-state-llm-rust-{pid}-{tag}.db"));
    for p in [&main, &mount, &llm] {
        clear(p);
    }
    std::fs::copy(main_fx, &main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fx, &mount).unwrap_or_else(|e| panic!("copy mount: {e}"));
    std::fs::copy(llm_fx, &llm).unwrap_or_else(|e| panic!("copy llm: {e}"));
    (main, mount, llm)
}

fn clear(p: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
    }
}

fn open_three_db(main: &Path, mount: &Path, llm: &Path, pepper: &str) -> Db {
    Db::open(
        DbPaths {
            main: main.to_path_buf(),
            mount_index: Some(mount.to_path_buf()),
            llm_logs: Some(llm.to_path_buf()),
        },
        pepper,
    )
    .unwrap_or_else(|e| panic!("open three-db: {e}"))
}

struct StateCase {
    label: &'static str,
    chat_key: ChatKey,
    character_key: Option<CharKey>,
    wrong_user: bool,
    args: Value,
}
enum ChatKey {
    Project,
    Solo,
    Bogus,
}
#[derive(Clone, Copy)]
enum CharKey {
    A,
    B,
    D,
}
impl CharKey {
    fn resolve(self, spec: &Spec) -> String {
        match self {
            CharKey::A => spec.char_a_id.clone(),
            CharKey::B => spec.char_b_id.clone(),
            CharKey::D => spec.char_d_id.clone(),
        }
    }
}

struct SqlCase {
    label: &'static str,
    args: Value,
}

#[tokio::test]
async fn state_sql_tools_matches_oracle() {
    let (Some(oracle_path), Some(main_fx), Some(mount_fx), Some(llm_fx)) = (
        env_or_skip("QT_ORACLE_STATE_SQL"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
        env_or_skip("QT_FIXTURE_TMP_LLM"),
    ) else {
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle = load_oracle(&oracle_path);

    run_state_cases(&spec, &oracle, &main_fx, &mount_fx, &llm_fx).await;
    run_sql_cases(&spec, &oracle, &main_fx, &mount_fx, &llm_fx).await;

    eprintln!("OK: state-sql-tools differential matched the oracle.");
}

async fn run_state_cases(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fx: &str,
    mount_fx: &str,
    llm_fx: &str,
) {
    use serde_json::json;
    let cases = vec![
        StateCase {
            label: "fetch_chat_root",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "chat" }),
        },
        StateCase {
            label: "fetch_chat_path",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "chat", "path": "hp" }),
        },
        StateCase {
            label: "fetch_chat_nested",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "chat", "path": "flags.seen" }),
        },
        StateCase {
            label: "fetch_chat_missing",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "chat", "path": "nope" }),
        },
        StateCase {
            label: "fetch_project_path",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "project", "path": "weather" }),
        },
        StateCase {
            label: "fetch_merged_project_key",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "path": "difficulty" }),
        },
        StateCase {
            label: "fetch_merged_chat_key",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "path": "hp" }),
        },
        StateCase {
            label: "fetch_project_not_in_project",
            chat_key: ChatKey::Solo,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "project", "path": "x" }),
        },
        StateCase {
            label: "set_chat_new",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "context": "chat", "path": "score", "value": 42 }),
        },
        StateCase {
            label: "set_chat_overwrite",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "context": "chat", "path": "hp", "value": 5 }),
        },
        StateCase {
            label: "set_chat_nested_create",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "context": "chat", "path": "stats.level", "value": 3 }),
        },
        StateCase {
            label: "set_project",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "context": "project", "path": "weather", "value": "stormy" }),
        },
        StateCase {
            label: "set_underscore_refused",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "path": "_secret", "value": 1 }),
        },
        StateCase {
            label: "delete_chat",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "chat", "path": "mood" }),
        },
        StateCase {
            label: "delete_chat_missing",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "chat", "path": "nope" }),
        },
        StateCase {
            label: "delete_project",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "project", "path": "difficulty" }),
        },
        StateCase {
            label: "permission_denied",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: true,
            args: json!({ "operation": "fetch", "context": "chat" }),
        },
        StateCase {
            label: "chat_not_found",
            chat_key: ChatKey::Bogus,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "chat" }),
        },
        StateCase {
            label: "validation_bad_op",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "bogus" }),
        },
        StateCase {
            label: "validation_nonobject",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: Value::String("nope".into()),
        },
        // ── P4.d10: the four-tier families (f48f34dc) ──
        StateCase {
            label: "fetch_merged_zero_groups",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::D),
            wrong_user: false,
            args: json!({ "operation": "fetch", "path": "gscore" }),
        },
        StateCase {
            label: "fetch_merged_one_group",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "fetch", "path": "gscore" }),
        },
        StateCase {
            label: "fetch_merged_two_groups",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::B),
            wrong_user: false,
            args: json!({ "operation": "fetch", "path": "twin" }),
        },
        StateCase {
            label: "fetch_merged_general_key",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "fetch", "path": "gen_only" }),
        },
        StateCase {
            label: "fetch_group_sole",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "group", "path": "gscore" }),
        },
        StateCase {
            label: "fetch_group_by_id",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::B),
            wrong_user: false,
            args: Value::Null, // resolved below (group-by-id needs the spec)
        },
        StateCase {
            label: "fetch_group_by_name",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "group", "group": "alpha lodge", "path": "k" }),
        },
        StateCase {
            label: "fetch_group_ambiguous_name",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::B),
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "group", "group": "TWIN LODGE", "path": "twin" }),
        },
        StateCase {
            label: "fetch_group_ref_required",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::B),
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "group", "path": "twin" }),
        },
        StateCase {
            label: "fetch_group_no_character",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "group", "path": "twin" }),
        },
        StateCase {
            label: "fetch_general",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "general", "path": "gen_only" }),
        },
        StateCase {
            label: "fetch_general_root",
            chat_key: ChatKey::Solo,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "fetch", "context": "general" }),
        },
        StateCase {
            label: "set_group_sole",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "set", "context": "group", "path": "gscore", "value": 2 }),
        },
        StateCase {
            label: "set_group_underscore_refused",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "set", "context": "group", "path": "_hidden", "value": 1 }),
        },
        StateCase {
            label: "set_general_new_key",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "context": "general", "path": "new_flag", "value": "lit" }),
        },
        StateCase {
            label: "set_general_overwrite",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "set", "context": "general", "path": "k", "value": "general2" }),
        },
        StateCase {
            label: "delete_group_key",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "group", "path": "gscore" }),
        },
        StateCase {
            label: "delete_general_key",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "general", "path": "gen_only" }),
        },
        StateCase {
            label: "delete_general_underscore_refused",
            chat_key: ChatKey::Project,
            character_key: None,
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "general", "path": "_secret" }),
        },
        StateCase {
            label: "delete_group_missing_key",
            chat_key: ChatKey::Project,
            character_key: Some(CharKey::A),
            wrong_user: false,
            args: json!({ "operation": "delete", "context": "group", "path": "nope" }),
        },
    ];

    for c in &cases {
        let (main, mount, llm) = fresh_copy(main_fx, mount_fx, llm_fx, c.label);
        let db = open_three_db(&main, &mount, &llm, &spec.test_pepper_base64);
        // The one spec-dependent args literal (group-by-id).
        let args = if c.label == "fetch_group_by_id" {
            json!({ "operation": "fetch", "context": "group", "group": spec.group_twin2_id, "path": "twin" })
        } else {
            c.args.clone()
        };
        let chat_id = match c.chat_key {
            ChatKey::Project => spec.chat_project_id.clone(),
            ChatKey::Solo => spec.chat_solo_id.clone(),
            ChatKey::Bogus => "deadbeef-0000-4000-8000-000000000000".to_string(),
        };
        let ctx = StateToolContext {
            user_id: if c.wrong_user {
                spec.wrong_user_id.clone()
            } else {
                spec.user_id.clone()
            },
            chat_id,
            project_id: None,
            character_id: c.character_key.map(|k| k.resolve(spec)),
        };
        let out = execute_state_tool(&db, &ctx, &args).await;
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = format_state_results(&out);

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(want.kind, "state", "kind mismatch for {}", c.label);
        assert_eq!(
            got_json, want.result_json,
            "resultJson diverged for {}",
            c.label
        );
        assert_eq!(
            got_fmt,
            want.formatted.as_deref().unwrap_or_default(),
            "formatted diverged for {}",
            c.label
        );

        // Dump the chats table (zero normalization — no updatedAt mint).
        let dump = db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .expect("dump chats");
        let got_rows = dump
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let want_rows = want.chats_dump.clone().unwrap_or_default();
        if got_rows != want_rows {
            eprintln!("GOT:  {}", serde_json::to_string(&got_rows).unwrap());
            eprintln!("WANT: {}", serde_json::to_string(&want_rows).unwrap());
        }
        assert_eq!(got_rows, want_rows, "chats dump diverged for {}", c.label);

        // Read back the project state through the store overlay.
        let pid = spec.project_id.clone();
        let project_state = db
            .write(move |writers| {
                let mount_w = writers
                    .mount_index()
                    .expect("mount-index present")
                    .connection();
                let main_c = writers.main().connection();
                let repo = ProjectsRepository::new(main_c, mount_w);
                let p = repo
                    .find_by_id(&pid)
                    .map_err(|e| quilltap_core::db::DbError::Key(e.to_string()))?;
                Ok(p.and_then(|p| p.get("state").cloned())
                    .unwrap_or(Value::Null))
            })
            .await
            .expect("read back project state");
        let want_state = want.project_state.clone().unwrap_or(Value::Null);
        assert_eq!(
            project_state, want_state,
            "project state diverged for {}",
            c.label
        );

        // Read back the Alpha group state + the general state (the P4.d10
        // group/general-tier write proofs).
        let alpha_id = spec.group_alpha_id.clone();
        let (group_alpha_state, general_state) = db
            .write(move |writers| {
                let mount_c = writers
                    .mount_index()
                    .expect("mount-index present")
                    .connection();
                let main_c = writers.main().connection();
                let alpha = quilltap_core::db::groups::GroupsRepository::new(main_c, mount_c)
                    .find_by_id(&alpha_id)
                    .map_err(|e| quilltap_core::db::DbError::Key(e.to_string()))?;
                let alpha_state = alpha
                    .and_then(|g| match g.get("state") {
                        Some(Value::Null) | None => None,
                        Some(v) => Some(v.clone()),
                    })
                    .unwrap_or(Value::Null);
                let general =
                    quilltap_core::services::mount_index::general_state::read_general_state(
                        main_c,
                        Some(mount_c),
                    );
                Ok((alpha_state, general))
            })
            .await
            .expect("read back group/general state");
        assert_eq!(
            group_alpha_state,
            want.group_alpha_state.clone().unwrap_or(Value::Null),
            "group state diverged for {}",
            c.label
        );
        assert_eq!(
            general_state,
            want.general_state.clone().unwrap_or(Value::Null),
            "general state diverged for {}",
            c.label
        );

        drop(db);
        for p in [&main, &mount, &llm] {
            clear(p);
        }
    }
}

async fn run_sql_cases(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fx: &str,
    mount_fx: &str,
    llm_fx: &str,
) {
    use serde_json::json;
    let cases = vec![
        SqlCase {
            label: "sql_main_select",
            args: json!({ "sql": "SELECT id, title FROM chats ORDER BY id" }),
        },
        SqlCase {
            label: "sql_main_blob",
            args: json!({ "sql": "SELECT id, embedding FROM memories" }),
        },
        SqlCase {
            label: "sql_main_default_db",
            args: json!({ "sql": "SELECT 1 AS n" }),
        },
        SqlCase {
            label: "sql_mount",
            args: json!({ "sql": "SELECT name FROM doc_mount_points ORDER BY name", "database": "mount-index" }),
        },
        SqlCase {
            label: "sql_llm",
            args: json!({ "sql": "SELECT id, provider, modelName FROM llm_logs", "database": "llm-logs" }),
        },
        SqlCase {
            label: "sql_readonly_pragma",
            args: json!({ "sql": "PRAGMA table_info(chats)" }),
        },
        SqlCase {
            label: "sql_truncation",
            args: json!({ "sql": "SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3", "max_rows": 2 }),
        },
        SqlCase {
            label: "sql_write_refusal",
            args: json!({ "sql": "DELETE FROM chats" }),
        },
        SqlCase {
            label: "sql_write_in_cte",
            args: json!({ "sql": "WITH t AS (SELECT 1) DELETE FROM chats" }),
        },
        SqlCase {
            label: "sql_multi_statement",
            args: json!({ "sql": "SELECT 1; SELECT 2" }),
        },
        SqlCase {
            label: "sql_empty",
            args: json!({ "sql": "   " }),
        },
        SqlCase {
            label: "sql_prepare_error",
            args: json!({ "sql": "SELECT * FROM nonexistent_table" }),
        },
        SqlCase {
            label: "sql_mutating_pragma",
            args: json!({ "sql": "PRAGMA journal_mode = WAL" }),
        },
        SqlCase {
            label: "sql_validation_nonobject",
            args: Value::String("nope".into()),
        },
    ];

    for c in &cases {
        let (main, mount, llm) = fresh_copy(main_fx, mount_fx, llm_fx, c.label);
        let db = open_three_db(&main, &mount, &llm, &spec.test_pepper_base64);
        let out = execute_run_sql_tool(&db, &c.args, &spec.user_id).await;
        let got_json = serde_json::to_string(&out).unwrap();

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(want.kind, "run_sql", "kind mismatch for {}", c.label);
        assert_eq!(
            got_json, want.result_json,
            "resultJson diverged for {}",
            c.label
        );

        drop(db);
        for p in [&main, &mount, &llm] {
            clear(p);
        }
    }
}
