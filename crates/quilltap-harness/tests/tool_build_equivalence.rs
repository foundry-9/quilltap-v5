//! W4.1g differential: the **tool slate** in isolation
//! (`quilltap_core::services::tool_build::build_tools` — v4 `buildTools` + the
//! built-in half of `buildToolsForProvider`). Drives v4's REAL `buildTools` over
//! a flag matrix (harness/oracle/cases/tool-build.test.ts) and diffs the returned
//! `{ tools, modelSupportsNativeTools, useNativeWebSearch }` byte-exact (tools
//! compared as JSON arrays — canonicalized/sorted by name on both sides).
//!
//! `checkModelSupportsTools` + `provider.supportsWebSearch` are the injected
//! registry-seam inputs (mocked in the oracle; `BuildToolsInput` fields here).
//! The autonomous-room destructive-tool filter (an orchestrator post-step, not
//! part of buildTools) is replicated on both sides.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//! (The worktree path is under `/.claude/`, in v4 jest's
//! `testPathIgnorePatterns`, so the oracle `.test.ts` + spec are copied to a temp
//! `cases/`+`fixtures/` sibling pair OUTSIDE `.claude` first — the tsx builder +
//! cargo run fine from the worktree.)
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
//!   TMPO=/tmp/qt-oracle-w41g; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5/harness/oracle/cases/tool-build.test.ts $TMPO/cases/
//!   cp $V5/harness/oracle/fixtures/tool-build.json $TMPO/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-tool-build.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-tool-build-fixture.ts
//!   QT_ORACLE_OUT=/tmp/oracle-tool-build.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" \
//!       --roots "$TMPO/cases" -- tool-build
//! Run:
//!   QT_ORACLE_TOOL_BUILD=/tmp/oracle-tool-build.ndjson \
//!   QT_FIXTURE_TOOL_BUILD=/tmp/qt-tool-build.db \
//!     cargo test -p quilltap-harness --test tool_build_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::tool_build::{
    self, build_tools, BuildToolsInput, ImageProviderConstraints,
};
use serde::Deserialize;
use serde_json::Value;

fn default_true() -> bool {
    true
}
fn default_provider() -> String {
    "OPENAI".to_string()
}
fn default_model() -> String {
    "gpt-4o".to_string()
}
fn default_chat_type() -> String {
    "chat".to_string()
}
fn default_policy() -> String {
    "opt_in_per_room".to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default = "default_model")]
    #[allow(dead_code)]
    model_name: String,
    #[serde(default)]
    use_native_web_search: bool,
    #[serde(default)]
    allow_tool_use: Option<bool>,
    #[serde(default)]
    allow_web_search: bool,
    #[serde(default)]
    image_profile_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    request_full_context: bool,
    #[serde(default)]
    disabled_tools: Vec<String>,
    #[serde(default)]
    disabled_tools_undefined: bool,
    #[serde(default)]
    disabled_tool_groups: Vec<String>,
    #[serde(default)]
    agent_mode_enabled: bool,
    #[serde(default)]
    is_multi_character: bool,
    #[serde(default)]
    help_tools_enabled: bool,
    #[serde(default = "default_true")]
    can_dress_themselves: bool,
    #[serde(default = "default_true")]
    can_create_outfits: bool,
    #[serde(default)]
    document_editing_enabled: bool,
    #[serde(default)]
    ask_carina_enabled: bool,
    #[serde(default = "default_true")]
    include_workspace_tools: bool,
    #[serde(default)]
    exclude_memory_search: bool,
    #[serde(default)]
    sql_access: bool,
    #[serde(default = "default_true")]
    model_supports_native_tools: bool,
    #[serde(default)]
    provider_supports_web_search: bool,
    #[serde(default = "default_chat_type")]
    chat_type: String,
    #[serde(default)]
    run_destructive_tools_allowed: i64,
    #[serde(default = "default_policy")]
    destructive_tool_policy: String,
}

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    cases: Vec<CaseW>,
}

#[derive(Deserialize)]
struct OracleRow {
    #[allow(dead_code)]
    kind: String,
    name: String,
    tools: Value,
    #[serde(rename = "modelSupportsNativeTools")]
    model_supports_native_tools: bool,
    #[serde(rename = "useNativeWebSearch")]
    use_native_web_search: bool,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/tool-build.json")
}

#[test]
fn tool_build_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_TOOL_BUILD") else {
        eprintln!("QT_ORACLE_TOOL_BUILD not set; skipping");
        return;
    };
    let Ok(fixture) = std::env::var("QT_FIXTURE_TOOL_BUILD") else {
        eprintln!("QT_FIXTURE_TOOL_BUILD not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");

    let mut want: HashMap<String, OracleRow> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .expect("oracle readable")
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: OracleRow = serde_json::from_str(line).expect("oracle row parses");
        want.insert(row.name.clone(), row);
    }

    // Copy the fixture to scratch and open it.
    let scratch = std::env::temp_dir().join(format!("qt-tool-build-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch");
    let work = scratch.join("tb.db");
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).expect("copy fixture");
    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture");

    let mut failures: Vec<String> = Vec::new();

    for c in &spec.cases {
        let disabled_tools: Option<&[String]> = if c.disabled_tools_undefined {
            None
        } else {
            Some(&c.disabled_tools)
        };
        let built = build_tools(
            &db,
            &spec.user_id,
            &BuildToolsInput {
                provider: &c.provider,
                use_native_web_search: c.use_native_web_search,
                allow_tool_use: c.allow_tool_use,
                allow_web_search: c.allow_web_search,
                image_profile_id: c.image_profile_id.as_deref(),
                image_provider_constraints: None::<ImageProviderConstraints>,
                project_id: c.project_id.as_deref(),
                request_full_context: c.request_full_context,
                disabled_tools,
                disabled_tool_groups: &c.disabled_tool_groups,
                agent_mode_enabled: c.agent_mode_enabled,
                is_multi_character: c.is_multi_character,
                help_tools_enabled: c.help_tools_enabled,
                can_dress_themselves: c.can_dress_themselves,
                can_create_outfits: c.can_create_outfits,
                document_editing_enabled: c.document_editing_enabled,
                ask_carina_enabled: c.ask_carina_enabled,
                include_workspace_tools: c.include_workspace_tools,
                exclude_memory_search: c.exclude_memory_search,
                sql_access: c.sql_access,
                model_supports_native_tools: c.model_supports_native_tools,
                provider_supports_web_search: c.provider_supports_web_search,
                // The tool-build corpus has no Pascal roster; no run_custom.
                custom_tool_context: None,
            },
        )
        .expect("build_tools");

        let mut tools = built.tools;

        // Autonomous-room destructive filter (orchestrator post-step; replicated
        // both sides).
        if c.chat_type == "autonomous" {
            let allowed_at_room = c.run_destructive_tools_allowed == 1;
            if !tool_build::destructive_allowed(&c.destructive_tool_policy, allowed_at_room) {
                tool_build::filter_destructive_tools(&mut tools);
            }
        }

        let got_tools = Value::Array(tools);
        let want_row = want
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle row missing for {}", c.name));

        if got_tools != want_row.tools {
            let g = tool_names(&got_tools);
            let w = tool_names(&want_row.tools);
            failures.push(format!(
                "case {}: tools mismatch\n  got:  {:?}\n  want: {:?}",
                c.name, g, w
            ));
        }
        if built.model_supports_native_tools != want_row.model_supports_native_tools {
            failures.push(format!(
                "case {}: modelSupportsNativeTools {} != {}",
                c.name, built.model_supports_native_tools, want_row.model_supports_native_tools
            ));
        }
        if built.use_native_web_search != want_row.use_native_web_search {
            failures.push(format!(
                "case {}: useNativeWebSearch {} != {}",
                c.name, built.use_native_web_search, want_row.use_native_web_search
            ));
        }
    }

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn tool_names(tools: &Value) -> Vec<String> {
    tools
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    t.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default()
}
