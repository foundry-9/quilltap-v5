//! Differential test (W4.1a sub-unit 2): the RNG tool executor
//! (`quilltap_core::tools::rng::execute_rng_tool` + `format_rng_results` — v4
//! `executeRngTool` + `formatRngResults`). Both sides read a COPY of one seed
//! fixture (the executor never writes) and run the same case list, feeding an
//! identical committed byte sequence to the injected randomness source. The
//! `RngToolOutput` (serialized to JSON, order-independent) + the formatted string
//! are diffed exactly, and each case is asserted to consume EXACTLY the committed
//! bytes — proving the rejection-sampling loop draws the stream identically to v4.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   TMPO=/tmp/qt-rng-executor-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5/harness/oracle/cases/rng-executor.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5/harness/oracle/fixtures/rng-executor.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-rng-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-rng-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-rng-executor-fixture.ts
//!   QT_FIXTURE_RNG_MAIN=/tmp/qt-rng-main.db QT_FIXTURE_RNG_MOUNT=/tmp/qt-rng-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-rng-executor.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/harness/oracle/cases" -- rng-executor
//! Run:
//!   QT_ORACLE_RNG_EXECUTOR=/tmp/oracle-rng-executor.ndjson \
//!   QT_FIXTURE_RNG_MAIN=/tmp/qt-rng-main.db QT_FIXTURE_RNG_MOUNT=/tmp/qt-rng-mount.db \
//!     cargo test -p quilltap-harness --test rng_executor_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::tools::rng::{execute_rng_tool, format_rng_results, FixedBytes, RngToolContext};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct CaseSpec {
    name: String,
    input: Value,
    #[serde(rename = "chatId")]
    chat_id: String,
    bytes: Vec<u8>,
}
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
struct OracleLine {
    kind: String,
    name: String,
    output: Value,
    formatted: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/rng-executor.json")
}

#[test]
fn rng_executor_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_RNG_EXECUTOR") else {
        eprintln!("QT_ORACLE_RNG_EXECUTOR not set; skipping");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_RNG_MAIN") else {
        eprintln!("QT_FIXTURE_RNG_MAIN not set; skipping");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_RNG_MOUNT") else {
        eprintln!("QT_FIXTURE_RNG_MOUNT not set; skipping");
        return;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");

    // Parse the oracle NDJSON into a name -> (output, formatted) map.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let mut want: HashMap<String, (Value, String)> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleLine = serde_json::from_str(line).expect("oracle line parses");
        assert_eq!(row.kind, "case");
        want.insert(row.name, (row.output, row.formatted));
    }

    // Copy the fixture DBs to a scratch dir (the executor is read-only, but open
    // a copy per convention).
    let scratch = std::env::temp_dir().join(format!("qt-rng-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("rng-main.db");
    let work_mount = scratch.join("rng-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture instance");

    for case in &spec.cases {
        let (want_output, want_formatted) = want
            .get(&case.name)
            .unwrap_or_else(|| panic!("oracle missing case {}", case.name));

        let mut rng = FixedBytes::new(case.bytes.clone());
        let ctx = RngToolContext {
            user_id: spec.user_id.clone(),
            chat_id: case.chat_id.clone(),
        };
        let output = execute_rng_tool(&db, &case.input, &ctx, &mut rng).expect("execute");

        // Byte-consumption parity: the rejection-sampling loop must draw exactly the
        // committed sequence (the same count v4's mock served).
        assert_eq!(
            rng.consumed(),
            case.bytes.len(),
            "case {}: consumed {} bytes, committed {}",
            case.name,
            rng.consumed(),
            case.bytes.len()
        );

        let got_output = serde_json::to_value(&output).expect("serialize output");
        assert_eq!(
            &got_output, want_output,
            "output mismatch for case {}",
            case.name
        );

        let got_formatted = format_rng_results(&output);
        assert_eq!(
            &got_formatted, want_formatted,
            "formatted mismatch for case {}",
            case.name
        );
    }

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
    eprintln!(
        "OK: rng-executor matched oracle ({} cases).",
        spec.cases.len()
    );
}
