//! W4.1g differential: the **tool slate** in isolation
//! (`quilltap_core::services::tool_build::build_tools` — v4 `buildTools` + the
//! built-in half of `buildToolsForProvider`). Drives v4's REAL `buildTools` over
//! a flag matrix (harness/oracle/cases/tool-build.test.ts) and diffs the returned
//! `{ tools, modelSupportsNativeTools, useNativeWebSearch }` byte-exact (tools
//! compared as JSON arrays — canonicalized/sorted by name on both sides).
//!
//! `checkModelSupportsTools` + `provider.supportsWebSearch` are the injected
//! registry-seam inputs (mocked in the oracle; `BuildToolsInput` fields here).
//! The turn's effect ceiling is computed by each side's real resolver and
//! passed into `build_tools` (P4.D343 below).
//!
//! P4.D216 (v4 `d1c06cd9d`): the corpus grows the `docToolsMode` cases (the
//! read-only five; explicit `full`/`off`), the `extras` bag (`documentsOnlySearch`
//! → the Scenario Builder `search` variant, incl. its precedence over
//! `excludeMemorySearch`; `webSearch: false` withholding and `webSearch: true`
//! unable to GRANT — narrowing only), the two allowlist cases (NEUTRAL BY
//! CONSTRUCTION on both sides — neither builds plugin tools here), and the two
//! Scenario Builder slates. The oracle shapes the index-13 argument by what the
//! pinned tree exports, so the same case runs at a pre-`d1c06cd9d` pin too:
//! there the 27 pre-existing cases are byte-identical to the target's, and
//! exactly the six moved cases differ (measured).
//!
//! P4.D234 (v4 `39bc98ffc` + `12c336fad`): every workspace slate trades
//! `list_email` for `list_mail` + `read_mail` + `discard_mail` (34 of 40 cases
//! move between the `acadcc7cd` and `12c336fad` pins; the six without the
//! workspace bundle stay byte-identical).
//!
//! P4.D343 (v4 `b15fe9404` + `a8214b253` at the pin `236c04e35`): the case is
//! RE-AIMED — `b15fe9404` deleted the curated destructive list (the case died at
//! import) and `a8214b253` made `buildTools` take ONE options object (the old
//! positional call ran garbled). The autonomous-room filter is no longer a
//! post-step on either side: the turn's EFFECT CEILING (v4's REAL
//! `resolveTurnEffectCeiling` / v5's `resolve_turn_effect_ceiling`) is passed
//! INTO `build_tools`, which applies it last, and each row compares
//! `effectCeiling` plus the two effect maps. The withheld set widens from three
//! to every `destroy` tool on the slate (`delete_annotation` joins — it is
//! always on); the old `p4d234_destructive_autonomous_mail_only` row, whose
//! premise ("discard_mail is the ONE destructive tool") is false at the pin, is
//! renamed `effect_ceiling_autonomous_withholds_destroy` and pins the measured
//! set. The `p4d343_storyteller_ceiling_*` rows run a storyteller ceiling of
//! each level over a full slate (the computed tools' notes included). The
//! `disabled_tools_undefined` row now builds the slate: `a8214b253` dropped
//! v4's legacy `disabledTools === undefined` bail (`disabledTools = []`).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//! (The worktree path is under `/.claude/`, in v4 jest's
//! `testPathIgnorePatterns`, so the oracle `.test.ts` + spec are copied to a temp
//! `cases/`+`fixtures/` sibling pair OUTSIDE `.claude` first — the tsx builder +
//! cargo run fine from the worktree.)
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-oracle-w41g; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
//!   cp $V5W/harness/oracle/cases/tool-build.test.ts $TMPO/cases/
//!   cp $V5W/harness/oracle/fixtures/tool-build.json $TMPO/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-tool-build.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-tool-build-fixture.ts
//!   QT_ORACLE_OUT=/tmp/oracle-tool-build.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" \
//!       --testTimeout=180000 --runTestsByPath "$TMPO/cases/tool-build.test.ts"
//! Run:
//!   QT_ORACLE_TOOL_BUILD=/tmp/oracle-tool-build.ndjson \
//!   QT_FIXTURE_TOOL_BUILD=/tmp/qt-tool-build.db \
//!     cargo test -p quilltap-harness --test tool_build_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::pascal::roster::RosterContext;
use quilltap_core::services::tool_build::{
    self, build_tools, BuildToolsExtras, BuildToolsInput, DocToolsMode, ImageProviderConstraints,
};
use quilltap_core::tools::effects::ToolEffect;
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
    /// P4.D328 (v4 `e1872c94f`): the positional `canManageInventory` at index
    /// 12. Absent and `null` both read ON (the oracle passes `true` / `null`).
    #[serde(default)]
    can_manage_inventory: Option<bool>,
    #[serde(default)]
    document_editing_enabled: bool,
    /// P4.D216 (v4 `d1c06cd9d`): `"off"`/`"read"`/`"full"`; absent → derived
    /// from `documentEditingEnabled` exactly as every v4 caller maps it.
    #[serde(default)]
    doc_tools_mode: Option<DocToolsMode>,
    /// P4.D216 (v4 `d1c06cd9d`): `buildTools`' trailing `extras` bag.
    #[serde(default)]
    extras: Option<ExtrasW>,
    #[serde(default)]
    ask_carina_enabled: bool,
    #[serde(default = "default_true")]
    include_workspace_tools: bool,
    #[serde(default)]
    exclude_memory_search: bool,
    #[serde(default)]
    sql_access: bool,
    /// P4.D332 (v4 `ea2cb60ab`): a non-null `customToolContext` — the roster
    /// SWITCH. The fixture instance has no custom tools, so `run_custom` stays
    /// withheld and `edit_custom_tool` rides the switch alone.
    #[serde(default)]
    custom_tool_context: bool,
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
    /// P4.D343: a storyteller turn's ceiling, fed through the resolver.
    #[serde(default)]
    storyteller_ceiling: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExtrasW {
    #[serde(default)]
    plugin_tool_allowlist: Option<Vec<String>>,
    #[serde(default)]
    documents_only_search: bool,
    #[serde(default)]
    web_search: Option<bool>,
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
    #[serde(rename = "effectCeiling")]
    effect_ceiling: Option<String>,
    #[serde(rename = "customToolEffects")]
    custom_tool_effects: Value,
    #[serde(rename = "pluginToolEffects")]
    plugin_tool_effects: Value,
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
    let scratch = tempfile::Builder::new()
        .prefix("qt-tool-build-")
        .tempdir()
        .expect("scratch");
    let work = scratch.path().join("tb.db");
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
    let (mut withheld_rows, mut allowed_autonomous_rows, mut ceiling_rows) = (0, 0, 0);

    for c in &spec.cases {
        let disabled_tools: Option<&[String]> = if c.disabled_tools_undefined {
            None
        } else {
            Some(&c.disabled_tools)
        };
        // The turn's effect ceiling, through the PRODUCTION resolver the
        // orchestrator calls (its DEBUG captured for the withheld rows).
        let storyteller = c
            .storyteller_ceiling
            .as_deref()
            .map(|e| ToolEffect::from_str(e).expect("ceiling"));
        let (effect_ceiling, resolve_lines) = quilltap_core::test_support::captured_with(|| {
            tool_build::resolve_turn_effect_ceiling(
                &c.chat_type,
                &c.destructive_tool_policy,
                c.run_destructive_tools_allowed == 1,
                storyteller,
                &c.name,
            )
        });
        let withheld_line = resolve_lines
            .iter()
            .filter(|l| l.contains("destructive tools withheld by effect ceiling"))
            .count();
        let (built, build_lines) = quilltap_core::test_support::captured_with(|| {
            build_tools(
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
                    can_manage_inventory: c.can_manage_inventory != Some(false),
                    doc_tools_mode: c.doc_tools_mode.unwrap_or(if c.document_editing_enabled {
                        DocToolsMode::Full
                    } else {
                        DocToolsMode::Off
                    }),
                    extras: c
                        .extras
                        .as_ref()
                        .map(|e| BuildToolsExtras {
                            plugin_tool_allowlist: e.plugin_tool_allowlist.clone(),
                            documents_only_search: e.documents_only_search,
                            web_search: e.web_search,
                        })
                        .unwrap_or_default(),
                    ask_carina_enabled: c.ask_carina_enabled,
                    include_workspace_tools: c.include_workspace_tools,
                    exclude_memory_search: c.exclude_memory_search,
                    sql_access: c.sql_access,
                    model_supports_native_tools: c.model_supports_native_tools,
                    provider_supports_web_search: c.provider_supports_web_search,
                    // The tool-build corpus has no Pascal roster, so no run_custom;
                    // a case may still flip the SWITCH (`edit_custom_tool`, P4.D332).
                    custom_tool_context: c.custom_tool_context.then(|| RosterContext {
                        user_id: spec.user_id.clone(),
                        chat_id: "chat-tb".to_string(),
                        ..Default::default()
                    }),
                    effect_ceiling,
                },
            )
        });
        let built = built.expect("build_tools");
        let ceiling_line = build_lines
            .iter()
            .filter(|l| l.contains("Effect ceiling applied to tool list"))
            .count();
        // The DEBUG fires exactly when a ceiling reached a built slate.
        let expect_ceiling_line =
            usize::from(effect_ceiling.is_some() && c.allow_tool_use != Some(false));
        assert_eq!(
            ceiling_line, expect_ceiling_line,
            "{}: {build_lines:?}",
            c.name
        );
        if effect_ceiling.is_some() {
            ceiling_rows += 1;
        }

        let tools = built.tools.clone();
        let names_now: Vec<String> = tool_names(&Value::Array(tools.clone()));
        if c.name == "effect_ceiling_autonomous_withholds_destroy" {
            // The measured withheld set at the pin: every `destroy` tool this
            // slate would carry — `delete_annotation` (Scriptorium, always on)
            // beside `discard_mail`; no `doc_*` tools ride this row.
            assert_eq!(withheld_line, 1, "{}: {resolve_lines:?}", c.name);
            for gone in ["delete_annotation", "discard_mail"] {
                assert!(
                    !names_now.contains(&gone.to_string()),
                    "{}: {gone} kept",
                    c.name
                );
            }
            withheld_rows += 1;
        } else if c.chat_type == "autonomous"
            && c.destructive_tool_policy != "always_refuse"
            && c.run_destructive_tools_allowed == 1
        {
            // Silence leg: an allowed room lowers nothing and says nothing.
            assert_eq!(withheld_line, 0, "{}: {resolve_lines:?}", c.name);
            allowed_autonomous_rows += 1;
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
        let got_ceiling = effect_ceiling.map(|e| e.as_str().to_string());
        if got_ceiling != want_row.effect_ceiling {
            failures.push(format!(
                "case {}: effectCeiling {:?} != {:?}",
                c.name, got_ceiling, want_row.effect_ceiling
            ));
        }
        let entries = |m: &std::collections::BTreeMap<String, ToolEffect>| {
            Value::Array(
                m.iter()
                    .map(|(k, v)| serde_json::json!([k, v.as_str()]))
                    .collect(),
            )
        };
        for (label, got, want) in [
            (
                "customToolEffects",
                entries(&built.custom_tool_effects),
                &want_row.custom_tool_effects,
            ),
            (
                "pluginToolEffects",
                entries(&built.plugin_tool_effects),
                &want_row.plugin_tool_effects,
            ),
        ] {
            if &got != want {
                failures.push(format!("case {}: {label} {got} != {want}", c.name));
            }
        }
        if built.use_native_web_search != want_row.use_native_web_search {
            failures.push(format!(
                "case {}: useNativeWebSearch {} != {}",
                c.name, built.use_native_web_search, want_row.use_native_web_search
            ));
        }
    }

    drop(db);
    drop(scratch);

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    eprintln!("tool_build_equivalence: {} cases", spec.cases.len());
    assert_eq!(withheld_rows, 1, "the withheld-set pin ran");
    assert!(allowed_autonomous_rows > 0, "the silence leg ran");
    assert!(ceiling_rows >= 8, "the ceiling rows ran ({ceiling_rows})");
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
