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
//! P4.D378 (v4 `0c47f4539`, the shared group walk `groupMountsFor`): the
//! fixture grows three NEW characters (Gale / Hale / Ivo — nothing pre-existing
//! moves but the Carina roster, which lists every character) and nine `pools_*`
//! cases over `vault.groups` / `vaultAccess.groups` / `context.groups` +
//! `context.project`:
//!   - `pools_shared_store_*` — Gale joins Weavers Guild THEN Cartographers
//!     Circle, which is LINKED to Weavers Guild's official store: v4 credits the
//!     shared store to the first-joined group only.
//!   - `pools_orphan_membership_*` — Hale's second membership names a group id
//!     with NO `groups` row but a `group_doc_mount_links` row to its own store:
//!     v4 lists it with name `''` (it sorts FIRST) and its store.
//!   - `pools_rf_group_unreadable_*` (R-F, item 13) — Ivo's group's
//!     `properties.json` is planted unreadable (`{`): the HYDRATED
//!     `groups.findById` fails on BOTH sides while the RAW row stays whole
//!     (`hydratedProbeGroupIds` — each side probes in-case and the probe rows
//!     compare byte-for-byte, plus a self-check that the plant took); v4 names
//!     the group from the raw row.
//!
//! Every case is checked; the red set is printed whole, then the test FAILS if
//! any case differs.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-selfinv-main.db /tmp/qt-selfinv-mount.db /tmp/qt-selfinv-llmlogs.db
//!   QT_FIXTURE_MAIN=/tmp/qt-selfinv-main.db \
//!   QT_FIXTURE_MOUNT=/tmp/qt-selfinv-mount.db \
//!   QT_FIXTURE_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-self-inventory-fixture.ts
//!   QT_FIXTURE_SELFINV_MAIN=/tmp/qt-selfinv-main.db \
//!   QT_FIXTURE_SELFINV_MOUNT=/tmp/qt-selfinv-mount.db \
//!   QT_FIXTURE_SELFINV_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/self-inventory.ts > /tmp/oracle-selfinv.ndjson
//! Run:
//!   cd $V5W
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
    /// `reinforcedImportance × decay` (v4 `e0ac7493a`); an archive row's is 0.
    salience: f64,
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
    /// P4.D350: raw SQL on the main working copy before / after the case.
    #[serde(rename = "plantSql")]
    plant_sql: Option<String>,
    #[serde(rename = "restoreSql")]
    restore_sql: Option<String>,
    /// P4.D378 R-F: groups whose hydrated read must fail while the raw row
    /// stays whole (probed in-case on both sides).
    #[serde(rename = "hydratedProbeGroupIds")]
    hydrated_probe_group_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    output: Value,
    formatted: String,
    /// P4.D350: the handler's own `self_inventory: …` DEBUG lines.
    logs: Vec<OracleLog>,
    /// P4.D378 R-F: v4's in-case hydrated-vs-raw probe rows.
    probe: Option<Value>,
}

#[derive(Deserialize)]
struct OracleLog {
    level: String,
    message: String,
    fields: serde_json::Map<String, Value>,
}

/// v4's line as v5's capture rig renders it after the target: the message,
/// then each field `key=value` in v4's order (strings bare, `null` as `null`).
fn render_v4_line(l: &OracleLog) -> String {
    let mut out = format!("{} {}", l.level.to_uppercase(), l.message);
    for (k, v) in &l.fields {
        let v = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {k}={v}"));
    }
    out
}

/// A v5 captured line without its target (`"<LEVEL> <target> <rest>"`).
fn strip_target(line: &str) -> String {
    let mut it = line.splitn(3, ' ');
    let level = it.next().unwrap_or_default();
    let _target = it.next();
    format!("{level} {}", it.next().unwrap_or_default())
}

/// The frozen roster clock (v4 reads `Date.now()`; the corpus's one custom
/// tool carries no time gate, so any fixed value answers the same).
const NOW_MS: i64 = 1_767_323_045_000;

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

    // Copy the three fixtures to a private working set (a scratch dir removed
    // on drop).
    let scratch = tempfile::Builder::new()
        .prefix("qt-selfinv-rust-")
        .tempdir()
        .expect("tempdir");
    let work_main = scratch.path().join("main.db");
    let work_mount = scratch.path().join("mount.db");
    let work_llm = scratch.path().join("llm.db");
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
                salience: s.salience,
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

    quilltap_core::test_support::global_capture::install();
    let mut failures: Vec<String> = Vec::new();
    for (case, oracle) in spec.cases.iter().zip(oracle_rows.iter()) {
        assert_eq!(case.name, oracle.name, "case name alignment");
        if let Some(sql) = &case.plant_sql {
            let sql = sql.clone();
            db.write(move |ws| Ok(ws.main().connection().execute_batch(&sql)?))
                .await
                .unwrap_or_else(|e| panic!("plant: {e}"));
        }

        // P4.D378 R-F: the hydrated read FAILS and the raw row is whole — on
        // v5, compared with v4's in-case probe byte-for-byte.
        let mut case_failures: Vec<String> = Vec::new();
        if let Some(ids) = &case.hydrated_probe_group_ids {
            let got_probe: Vec<Value> = ids
                .iter()
                .map(|group_id| {
                    let hydrated_threw = db
                        .read_main(|main| {
                            db.read_mount_index(|mount| {
                                Ok(
                                    quilltap_core::db::groups::GroupsRepository::new(main, mount)
                                        .find_by_id(group_id)
                                        .is_err(),
                                )
                            })
                        })
                        .expect("hydrated probe");
                    let raw = db
                        .read_main(|main| {
                            quilltap_core::db::groups::find_name_and_official_mount_point_id_raw(
                                main, group_id,
                            )
                        })
                        .expect("raw probe");
                    serde_json::json!({
                        "groupId": group_id,
                        "hydratedThrew": hydrated_threw,
                        "rawName": raw.as_ref().map(|(n, _)| n.clone()),
                        "rawHasOfficialStore": raw.as_ref().is_some_and(|(_, o)| o.is_some()),
                    })
                })
                .collect();
            let want_probe = oracle
                .probe
                .clone()
                .unwrap_or_else(|| panic!("case {}: oracle carries no probe", case.name));
            // The plant must have taken on v4's side (else the row proves nothing).
            for p in want_probe.as_array().expect("probe array") {
                assert_eq!(
                    p["hydratedThrew"],
                    Value::Bool(true),
                    "case {}: v4 plant",
                    case.name
                );
                assert!(p["rawName"].is_string(), "case {}: v4 raw row", case.name);
            }
            let got_probe = Value::Array(got_probe);
            if serde_json::to_string(&got_probe).unwrap()
                != serde_json::to_string(&want_probe).unwrap()
            {
                case_failures.push(format!("R-F probe\n GOT: {got_probe}\n EXP: {want_probe}"));
            }
        }

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

        let (out, lines) =
            quilltap_core::test_support::global_capture::capture_async(execute_self_inventory(
                &db,
                &env,
                &spec.user_id,
                &spec.chat.id,
                case.character_id.as_deref(),
                Some(&spec.project.id),
                None,
                loaded_arg,
                &args,
                NOW_MS,
            ))
            .await;
        if let Some(sql) = &case.restore_sql {
            let sql = sql.clone();
            db.write(move |ws| Ok(ws.main().connection().execute_batch(&sql)?))
                .await
                .unwrap_or_else(|e| panic!("restore: {e}"));
        }

        // The handler's own DEBUG lines (P4.D350), each with its silence leg:
        // a case whose arm logs nothing must log nothing here too.
        let got_lines: Vec<String> = lines
            .iter()
            .filter(|l| l.contains(" self_inventory: "))
            .map(|l| strip_target(l))
            .collect();
        let want_lines: Vec<String> = oracle.logs.iter().map(render_v4_line).collect();
        if got_lines != want_lines {
            case_failures.push(format!(
                "self_inventory lines\n GOT: {got_lines:?}\n EXP: {want_lines:?}"
            ));
        }

        // BYTES, never `Value ==` (key order is the contract).
        let got_json = serde_json::to_value(&out).expect("serialize output");
        if serde_json::to_string(&got_json).unwrap()
            != serde_json::to_string(&oracle.output).unwrap()
        {
            case_failures.push(format!(
                "output JSON mismatch\n GOT: {}\n EXP: {}",
                serde_json::to_string_pretty(&got_json).unwrap(),
                serde_json::to_string_pretty(&oracle.output).unwrap(),
            ));
        }

        let got_formatted = format_self_inventory(&out);
        if got_formatted != oracle.formatted {
            case_failures.push(format!(
                "formatted string mismatch\n GOT: {got_formatted}\n EXP: {}",
                oracle.formatted
            ));
        }

        if case_failures.is_empty() {
            eprintln!("OK {}", case.name);
        } else {
            eprintln!("RED {}", case.name);
            for f in &case_failures {
                failures.push(format!("case {}: {f}", case.name));
            }
        }
    }
    eprintln!(
        "self_inventory_equivalence: {} cases, {} failure(s)",
        spec.cases.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
