//! Read-differential test: the `self_inventory` tool handler (v4
//! `lib/tools/handlers/self-inventory-*`), ported as
//! `quilltap_core::tools::self_inventory`.
//!
//! Both sides READ a COPY of the SAME pre-baked three-database fixture (main +
//! mount-index + llm-logs) and run the SAME case corpus
//! (`self-inventory.json::cases`). The oracle drives v4's REAL
//! `executeSelfInventoryTool` + `formatSelfInventoryResults`; the Rust port runs
//! `execute_self_inventory` + `format_self_inventory`. Each case's Output JSON AND
//! the formatted markdown string are diffed **byte-for-byte** — no normalization
//! (nothing is mutated, so no minted timestamp/id surfaces; the vault files'
//! `lastModified`/`mountPointId` are baked into the shared fixture, identical on
//! both sides).
//!
//! The host-env seam is fed the same values v4's oracle produces: `version` = v4's
//! `packageJson.version` (read from the oracle output), `runtimeMode=local-dev`,
//! `clientShell=browser`, `mountIndexDegraded=false`, empty registry (the corpus's
//! `ANTHROPIC`/`claude-3-opus` resolves via `MODEL_CONTEXT_OVERRIDES`, a hardcoded
//! table both sides share, so no registry rows are needed → contextWindow 200000).
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MAIN=/tmp/qt-selfinv-main.db \
//!   QT_FIXTURE_MOUNT=/tmp/qt-selfinv-mount.db \
//!   QT_FIXTURE_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-self-inventory-fixture.ts
//!   QT_FIXTURE_SELFINV_MAIN=/tmp/qt-selfinv-main.db \
//!   QT_FIXTURE_SELFINV_MOUNT=/tmp/qt-selfinv-mount.db \
//!   QT_FIXTURE_SELFINV_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/self-inventory.ts > /tmp/oracle-selfinv.ndjson
//! Run:
//!   QT_ORACLE_SELFINV=/tmp/oracle-selfinv.ndjson \
//!   QT_FIXTURE_SELFINV_MAIN=/tmp/qt-selfinv-main.db \
//!   QT_FIXTURE_SELFINV_MOUNT=/tmp/qt-selfinv-mount.db \
//!   QT_FIXTURE_SELFINV_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
//!     cargo test -p quilltap-harness --test self_inventory_equivalence

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::tool_execution::{
    LoadedInterCharacterMemory, LoadedMemoriesContext, LoadedSemanticMemory,
};
use quilltap_core::tools::self_inventory::{
    execute_self_inventory, format_self_inventory, ClientShell, SelfInventoryEnv,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    chat: IdOnly,
    project: IdOnly,
    #[serde(rename = "loadedMemories")]
    loaded_memories: LoadedMemoriesSpec,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Deserialize)]
struct LoadedMemoriesSpec {
    semantic: Vec<SemSpec>,
    #[serde(rename = "interCharacter")]
    inter_character: Vec<InterSpec>,
    recap: String,
}

#[derive(Deserialize)]
struct SemSpec {
    summary: String,
    importance: f64,
    score: f64,
    #[serde(rename = "effectiveWeight")]
    effective_weight: f64,
}

#[derive(Deserialize)]
struct InterSpec {
    #[serde(rename = "aboutCharacterName")]
    about_character_name: String,
    summary: String,
    importance: f64,
}

#[derive(Deserialize)]
struct CaseSpec {
    name: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
    sections: Option<Vec<String>>,
    #[serde(rename = "includeAutomaticImages")]
    include_automatic_images: Option<bool>,
    #[serde(rename = "withLoadedMemories")]
    with_loaded_memories: Option<bool>,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    output: Value,
    formatted: String,
}

#[tokio::test]
async fn self_inventory_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SELFINV") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SELFINV to the oracle NDJSON (see header).");
            return;
        }
    };
    let (fx_main, fx_mount, fx_llm) = match (
        std::env::var("QT_FIXTURE_SELFINV_MAIN"),
        std::env::var("QT_FIXTURE_SELFINV_MOUNT"),
        std::env::var("QT_FIXTURE_SELFINV_LLMLOGS"),
    ) {
        (Ok(a), Ok(b), Ok(c)) => (a, b, c),
        _ => {
            eprintln!("SKIP: set QT_FIXTURE_SELFINV_MAIN / _MOUNT / _LLMLOGS (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/self-inventory.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle_rows: Vec<OracleRow> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    assert_eq!(
        oracle_rows.len(),
        spec.cases.len(),
        "oracle row count vs case count"
    );

    // v4's packageJson.version — the top-level `quilltapVersion` in the oracle's
    // first output. Feed it to the env so both sides emit the same version.
    let version = oracle_rows[0].output["quilltapVersion"]
        .as_str()
        .expect("oracle quilltapVersion")
        .to_string();

    // Copy the three fixtures to a private working set.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-selfinv-rust-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-selfinv-rust-mount-{pid}.db"));
    let work_llm = std::env::temp_dir().join(format!("qt-selfinv-rust-llm-{pid}.db"));
    for w in [&work_main, &work_mount, &work_llm] {
        let _ = std::fs::remove_file(w);
    }
    std::fs::copy(&fx_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fx_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));
    std::fs::copy(&fx_llm, &work_llm).unwrap_or_else(|e| panic!("copy llm: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: Some(work_llm.clone()),
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let env = SelfInventoryEnv {
        version,
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    };

    let loaded = LoadedMemoriesContext {
        semantic: spec
            .loaded_memories
            .semantic
            .iter()
            .map(|s| LoadedSemanticMemory {
                summary: s.summary.clone(),
                importance: s.importance,
                score: s.score,
                effective_weight: s.effective_weight,
            })
            .collect(),
        inter_character: spec
            .loaded_memories
            .inter_character
            .iter()
            .map(|s| LoadedInterCharacterMemory {
                about_character_name: s.about_character_name.clone(),
                summary: s.summary.clone(),
                importance: s.importance,
            })
            .collect(),
        recap: Some(spec.loaded_memories.recap.clone()),
    };

    for (case, oracle) in spec.cases.iter().zip(oracle_rows.iter()) {
        assert_eq!(case.name, oracle.name, "case name alignment");

        let mut args = serde_json::Map::new();
        if let Some(sections) = &case.sections {
            args.insert("sections".to_string(), serde_json::json!(sections));
        }
        if let Some(inc) = case.include_automatic_images {
            args.insert("includeAutomaticImages".to_string(), Value::Bool(inc));
        }
        let args = Value::Object(args);

        let loaded_arg = if case.with_loaded_memories.unwrap_or(false) {
            Some(&loaded)
        } else {
            None
        };

        let out = execute_self_inventory(
            &db,
            &env,
            &spec.user_id,
            &spec.chat.id,
            case.character_id.as_deref(),
            Some(&spec.project.id),
            None,
            loaded_arg,
            &args,
        )
        .await;

        let got_json = serde_json::to_value(&out).expect("serialize output");
        assert_eq!(
            got_json,
            oracle.output,
            "case {}: output JSON mismatch\n GOT: {}\n EXP: {}",
            case.name,
            serde_json::to_string_pretty(&got_json).unwrap(),
            serde_json::to_string_pretty(&oracle.output).unwrap(),
        );

        let got_formatted = format_self_inventory(&out);
        assert_eq!(
            got_formatted, oracle.formatted,
            "case {}: formatted string mismatch",
            case.name
        );
    }
}
