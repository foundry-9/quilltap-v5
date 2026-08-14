//! Differential test (W4.1d4): the four search/introspection tool handlers
//! (`quilltap_core::tools::{project_info, request_full_context, help_search,
//! search}`) vs v4's REAL handlers.
//!
//! COMPARISON STRATEGY (float-safe): the oracle emits, per case, one NDJSON line
//! `{ label, resultJson: JSON.stringify(handlerOutput), formatted: format*(...) }`.
//! This test builds the SAME handler output, does `serde_json::to_string(&output)`,
//! and asserts it equals `resultJson`; and asserts the Rust `format*` equals
//! `formatted`. Identical f64 bits render identically under ryu (serde) and V8
//! (JSON.stringify), and the handlers already apply js-number rendering +
//! preserve_order key ordering, so the strings match. `request_full_context`
//! additionally dumps the full `chats` row (via `dump_table_json_conn`) and compares
//! it against the oracle's canonicalized row — proving the flag flips to 1 and every
//! other column (updatedAt included) is preserved.
//!
//! Each case runs on a FRESH copy of the two-DB fixture (search + request_full_context
//! WRITE). The `search` + `help_search` cases inject the corpus's canned query
//! embeddings via a `CannedEmbeddingProvider`, matching the oracle's jest.mock of
//! `generateEmbeddingForUser`. `now_ms` is pinned identically on both sides.
//!
//! Generate the fixture + oracles (Node 24, from the v4 checkout). The oracle case
//! files MUST be staged OUTSIDE any `.claude/` path: v4's jest config lists both
//! `/\.claude/` in `testPathIgnorePatterns` AND `modulePathIgnorePatterns`, so a case
//! file living under this worktree (`.../.claude/worktrees/.../harness/oracle/cases`)
//! is invisible to jest. Copy the case + fixture spec into a sibling
//! `cases/`+`fixtures/` tree under a non-`.claude` dir (the oracle reads its spec via
//! `../fixtures/search-tools.json`) and point `--roots` there. (Once these files are
//! merged to the main `~/source/quilltap-v5` checkout the STAGE step is unnecessary —
//! that path has no `.claude` segment.)
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   WT=<this worktree root>
//!   STAGE=/tmp/qt-oracle-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $WT/harness/oracle/cases/search-tools-*.test.ts $STAGE/harness/oracle/cases/
//!   cp $WT/harness/oracle/fixtures/search-tools.json    $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
//!     $N/node --import tsx $WT/harness/oracle/fixtures/build-search-tools-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-search-tools-readwrite.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- search-tools-readwrite
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-search-tools-search.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- search-tools-search
//! Run:
//!   QT_ORACLE_SEARCH_RW=/tmp/oracle-search-tools-readwrite.ndjson \
//!   QT_ORACLE_SEARCH=/tmp/oracle-search-tools-search.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-search-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-search-mount.db \
//!     cargo test -p quilltap-harness --test search_tools_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::tools::help_search::{execute_help_search, format_help_search};
use quilltap_core::tools::project_info::{
    execute_project_info, format_project_info, ProjectInfoContext,
};
use quilltap_core::tools::request_full_context::{
    execute_request_full_context, format_request_full_context,
};
use quilltap_core::tools::search::{
    execute_search_scriptorium, format_search_scriptorium_results, SearchContext,
};
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
    #[serde(rename = "embeddingProfileId")]
    embedding_profile_id: Option<String>,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "minimalProjectId")]
    minimal_project_id: String,
    #[serde(rename = "bogusProjectId")]
    bogus_project_id: String,
    #[serde(rename = "chatAId")]
    chat_a_id: String,
    #[serde(rename = "charAId")]
    char_a_id: String,
    #[serde(rename = "$nowMs")]
    now_ms: f64,
    #[serde(rename = "cannedEmbeddings")]
    canned_embeddings: HashMap<String, Vec<f32>>,
    #[serde(rename = "cannedFailures")]
    canned_failures: Vec<String>,
}

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    #[serde(rename = "resultJson")]
    result_json: String,
    formatted: String,
    #[serde(default, rename = "chatRow")]
    chat_row: Option<Value>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/search-tools.json")
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

/// Fresh copies of the two-DB fixture (per case). Returns (main_work, mount_work).
fn fresh_copy(main_fixture: &str, mount_fixture: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let main_work = dir.join(format!(
        "qt-search-main-rust-{}-{tag}.db",
        std::process::id()
    ));
    let mount_work = dir.join(format!(
        "qt-search-mount-rust-{}-{tag}.db",
        std::process::id()
    ));
    for p in [&main_work, &mount_work] {
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
        }
    }
    std::fs::copy(main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));
    (main_work, mount_work)
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

fn cleanup(main: &Path, mount: &Path) {
    for p in [main, mount] {
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
        }
    }
}

#[tokio::test]
async fn search_tools_matches_oracle() {
    let (Some(rw_oracle), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_SEARCH_RW"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };
    let search_oracle = match std::env::var("QT_ORACLE_SEARCH") {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("NOTE: QT_ORACLE_SEARCH unset — running only the readwrite (project_info / request_full_context) cases.");
            None
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    // ---- request_full_context + project_info ----
    let rw = load_oracle(&rw_oracle);
    run_readwrite(&spec, &rw, &main_fixture, &mount_fixture).await;

    // ---- help_search + search ----
    if let Some(search_oracle) = search_oracle {
        let so = load_oracle(&search_oracle);
        run_search(&spec, &so, &main_fixture, &mount_fixture).await;
    }

    eprintln!("OK: search-tools differential matched the oracle(s).");
}

/// project_info (read) + request_full_context (write). Field-order-exact JSON string
/// compare + format compare; rfc additionally proves the chats-row effect.
async fn run_readwrite(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fixture: &str,
    mount_fixture: &str,
) {
    // ---- project_info cases (read-only, but we still copy per case for isolation) ----
    struct PiCase {
        label: &'static str,
        action: Value,
        project_id: String,
    }
    let pi_cases = vec![
        PiCase {
            label: "pi_get_info_rich",
            action: serde_json::json!({ "action": "get_info" }),
            project_id: spec.project_id.clone(),
        },
        PiCase {
            label: "pi_get_info_minimal",
            action: serde_json::json!({ "action": "get_info" }),
            project_id: spec.minimal_project_id.clone(),
        },
        PiCase {
            label: "pi_get_instructions_present",
            action: serde_json::json!({ "action": "get_instructions" }),
            project_id: spec.project_id.clone(),
        },
        PiCase {
            label: "pi_get_instructions_none",
            action: serde_json::json!({ "action": "get_instructions" }),
            project_id: spec.minimal_project_id.clone(),
        },
        PiCase {
            label: "pi_project_not_found",
            action: serde_json::json!({ "action": "get_info" }),
            project_id: spec.bogus_project_id.clone(),
        },
        PiCase {
            label: "pi_invalid_action",
            action: serde_json::json!({ "action": "nope" }),
            project_id: spec.project_id.clone(),
        },
        PiCase {
            label: "pi_invalid_nonobject",
            action: Value::String("not-an-object".into()),
            project_id: spec.project_id.clone(),
        },
    ];

    for c in &pi_cases {
        let (main_work, mount_work) = fresh_copy(main_fixture, mount_fixture, c.label);
        let db = open_two_db(&main_work, &mount_work, &spec.test_pepper_base64);
        let ctx = ProjectInfoContext {
            user_id: spec.user_id.clone(),
            project_id: c.project_id.clone(),
            embedding_profile_id: None,
        };
        let out = execute_project_info(&db, &ctx, &c.action);
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = format_project_info(&out);

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(
            got_json, want.result_json,
            "resultJson diverged for {}",
            c.label
        );
        assert_eq!(
            got_fmt, want.formatted,
            "formatted diverged for {}",
            c.label
        );

        drop(db);
        cleanup(&main_work, &mount_work);
    }

    // ---- request_full_context cases (write) ----
    struct RfcCase {
        label: &'static str,
        args: Value,
    }
    let rfc_cases = vec![
        RfcCase {
            label: "rfc_write",
            args: serde_json::json!({}),
        },
        RfcCase {
            label: "rfc_invalid_nonobject",
            args: Value::String("nope".into()),
        },
    ];

    for c in &rfc_cases {
        let (main_work, mount_work) = fresh_copy(main_fixture, mount_fixture, c.label);
        let db = open_two_db(&main_work, &mount_work, &spec.test_pepper_base64);
        let out = execute_request_full_context(&db, &spec.chat_a_id, &c.args).await;
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = format_request_full_context(&out);

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(
            got_json, want.result_json,
            "resultJson diverged for {}",
            c.label
        );
        assert_eq!(
            got_fmt, want.formatted,
            "formatted diverged for {}",
            c.label
        );

        // Dump the single chats row and compare against the oracle's canonicalized row.
        let dump = db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .expect("dump chats");
        let rows = dump
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let got_row = rows
            .into_iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(spec.chat_a_id.as_str()))
            .unwrap_or(Value::Null);
        let want_row = want.chat_row.clone().unwrap_or(Value::Null);
        assert_eq!(got_row, want_row, "chats row diverged for {}", c.label);

        drop(db);
        cleanup(&main_work, &mount_work);
    }
}

/// help_search + search cases (canned embeddings). Each case runs on a fresh copy
/// (search's memory branch bumps lastAccessedAt). The `now_ms` matches the oracle's
/// frozen Date.now(); the canned provider mirrors the oracle's generateEmbeddingForUser
/// jest.mock.
async fn run_search(
    spec: &Spec,
    oracle: &HashMap<String, OracleRow>,
    main_fixture: &str,
    mount_fixture: &str,
) {
    let provider = build_provider(spec);

    // ---- help_search cases ----
    struct HsCase {
        label: &'static str,
        args: Value,
    }
    let hs_cases = vec![
        HsCase {
            label: "hs_semantic_hit",
            args: serde_json::json!({ "query": "how do I get started" }),
        },
        HsCase {
            label: "hs_semantic_limit",
            args: serde_json::json!({ "query": "how do I get started", "limit": 1 }),
        },
        HsCase {
            label: "hs_keyword_fallback",
            args: serde_json::json!({ "query": "wardrobe outfits clothing help" }),
        },
        HsCase {
            label: "hs_keyword_miss",
            args: serde_json::json!({ "query": "xyzzy plugh frobnitz quux" }),
        },
        HsCase {
            label: "hs_invalid_empty",
            args: serde_json::json!({ "query": "" }),
        },
        // P4.D77 — the whole ranking in one case, so every section arm is
        // visible: the doc that beats its own section, the tie that still
        // attaches, the doc whose section LIFTS it from last place to joint
        // first, the dimension-mismatched section that is skipped, and the docs
        // with no sections at all. The default limit of 3 hides the last three.
        HsCase {
            label: "hs_semantic_sections_all",
            args: serde_json::json!({ "query": "how do I get started", "limit": 10 }),
        },
    ];
    // This list is HAND-MAINTAINED against the oracle case file, and a label
    // present there but missing here is silently unrun — which is exactly how
    // P4.D77's first dimension-guard mutation came back green. Fail instead.
    {
        let declared: std::collections::BTreeSet<&str> = hs_cases.iter().map(|c| c.label).collect();
        let from_oracle: Vec<&String> = oracle.keys().filter(|k| k.starts_with("hs_")).collect();
        for label in &from_oracle {
            assert!(
                declared.contains(label.as_str()),
                "the oracle carries help_search case {label} that this test never runs — \
                 add it to `hs_cases`"
            );
        }
    }
    for c in &hs_cases {
        let (main_work, mount_work) = fresh_copy(main_fixture, mount_fixture, c.label);
        let db = open_two_db(&main_work, &mount_work, &spec.test_pepper_base64);
        let out = execute_help_search(&db, &provider, &spec.user_id, &c.args).await;
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = format_help_search(out.results.as_deref().unwrap_or(&[]));

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(
            got_json, want.result_json,
            "resultJson diverged for {}",
            c.label
        );
        assert_eq!(
            got_fmt, want.formatted,
            "formatted diverged for {}",
            c.label
        );

        drop(db);
        cleanup(&main_work, &mount_work);
    }

    // ---- search cases ----
    struct SearchCase {
        label: &'static str,
        args: Value,
        operator_surface: bool,
        with_character: bool,
        wrong_user: bool,
    }
    let s_cases = vec![
        SearchCase {
            label: "search_memories_only",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"] }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_conversations_only",
            args: serde_json::json!({ "query": "what did we say about the ledger", "sources": ["conversations"] }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_documents_only",
            args: serde_json::json!({ "query": "guide to celestial mechanics", "sources": ["documents"] }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_knowledge_only",
            args: serde_json::json!({ "query": "guide to celestial mechanics", "sources": ["knowledge"] }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_combined",
            args: serde_json::json!({ "query": "guide to celestial mechanics" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_empty_result",
            args: serde_json::json!({ "query": "unrelated query that matches nothing" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_limit_cap",
            args: serde_json::json!({ "query": "guide to celestial mechanics", "limit": 2 }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_truncation",
            args: serde_json::json!({ "query": "a passage with a very long body", "sources": ["documents"] }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_operator_surface",
            args: serde_json::json!({ "query": "guide to celestial mechanics" }),
            operator_surface: true,
            with_character: false,
            wrong_user: false,
        },
        SearchCase {
            label: "search_invalid_empty",
            args: serde_json::json!({ "query": "" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        // ---- episodic recall (P4.d13 unit 6): since/until + aboutCharacter ----
        SearchCase {
            label: "search_window_dateonly",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "since": "2025-12-01", "until": "2025-12-31" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_until_createdat_fallback",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "until": "2025-06-01" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_since_full_iso",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "since": "2025-12-20T00:00:00.000Z" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_window_empty",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "since": "2020-01-01", "until": "2020-12-31" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_about_resolved",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "aboutCharacter": "Bram Roster" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_about_alias",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "aboutCharacter": "the quartermaster" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_about_trimmed_case",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "aboutCharacter": "  BRAM roster  " }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_about_unresolved",
            args: serde_json::json!({ "query": "recall the star navigation notes", "sources": ["memories"], "aboutCharacter": "Nobody Atall" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_conversations_window_in",
            args: serde_json::json!({ "query": "what did we say about the ledger", "sources": ["conversations"], "since": "2025-05-01", "until": "2025-07-01" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_conversations_window_out",
            args: serde_json::json!({ "query": "what did we say about the ledger", "sources": ["conversations"], "since": "2020-01-01", "until": "2020-12-31" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
        SearchCase {
            label: "search_invalid_since",
            args: serde_json::json!({ "query": "recall the star navigation notes", "since": "last week" }),
            operator_surface: false,
            with_character: true,
            wrong_user: false,
        },
    ];
    for c in &s_cases {
        let (main_work, mount_work) = fresh_copy(main_fixture, mount_fixture, c.label);
        let db = open_two_db(&main_work, &mount_work, &spec.test_pepper_base64);
        let ctx = SearchContext {
            user_id: if c.wrong_user {
                spec.wrong_user_id.clone()
            } else {
                spec.user_id.clone()
            },
            character_id: if c.with_character {
                Some(spec.char_a_id.clone())
            } else {
                None
            },
            embedding_profile_id: spec.embedding_profile_id.clone(),
            project_id: Some(spec.project_id.clone()),
            operator_surface: c.operator_surface,
        };
        let out = execute_search_scriptorium(&db, &provider, &ctx, &c.args, spec.now_ms).await;
        let got_json = serde_json::to_string(&out).unwrap();
        let got_fmt = format_search_scriptorium_results(out.results.as_deref().unwrap_or(&[]));

        let want = oracle
            .get(c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));
        assert_eq!(
            got_json, want.result_json,
            "resultJson diverged for {}",
            c.label
        );
        assert_eq!(
            got_fmt, want.formatted,
            "formatted diverged for {}",
            c.label
        );

        drop(db);
        cleanup(&main_work, &mount_work);
    }
}

fn build_provider(spec: &Spec) -> CannedEmbeddingProvider {
    let mut provider = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        provider = provider.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        provider = provider.with_failure(text.clone());
    }
    provider
}
