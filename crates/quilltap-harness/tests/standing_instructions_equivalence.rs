//! Tier-1/tier-2 differential (P4.D103 unit 1, v4 `8f868109`): standing
//! instructions — `quilltap_core::standing_instructions` vs v4's REAL
//! `lib/chat/context/standing-instructions.ts`.
//!
//! Two row kinds, both over the SAME baked fixture pair:
//!   - `resolve` drives `resolveStandingInstructions` AND the one-shot
//!     `resolveStandingInstructionsSection`, so the resolved SOURCE LIST (kind,
//!     name, instructions, IN ORDER) is a comparand alongside the rendered
//!     section. A port that renders correctly from a wrongly-ordered resolve
//!     still reddens.
//!   - `render` drives `renderStandingInstructionsSection` on hand-built
//!     sources the resolver can never produce (untrimmed bodies, all-whitespace
//!     entries, em dashes in names).
//!
//! Load-bearing arms, each mutation-proven red-first:
//!   - the group sort is by `localeCompare(name)` — membership INSERT order in
//!     the fixture is Zephyr/Gamma/Beacon, the answer is Beacon/Gamma/Zephyr;
//!   - `apple` sorts BEFORE `Banana` (ICU en-US, not byte order);
//!   - two groups named `Mirror` tie-break on `instructions`, not id/rowid;
//!   - the project source ALWAYS precedes the group sources;
//!   - a dangling membership (group row absent) is skipped, not fatal.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SI_MAIN=/tmp/qt-si-main.db QT_FIXTURE_SI_MOUNT=/tmp/qt-si-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-standing-instructions-fixture.ts
//!   QT_FIXTURE_SI_MAIN=/tmp/qt-si-main.db QT_FIXTURE_SI_MOUNT=/tmp/qt-si-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/standing-instructions.ts \
//!     > /tmp/oracle-standing-instructions.ndjson
//! Run:
//!   QT_ORACLE_STANDING_INSTRUCTIONS=/tmp/oracle-standing-instructions.ndjson \
//!   QT_FIXTURE_SI_MAIN=/tmp/qt-si-main.db QT_FIXTURE_SI_MOUNT=/tmp/qt-si-mount.db \
//!     cargo test -p quilltap-harness --test standing_instructions_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::standing_instructions::{
    render_standing_instructions_section, resolve_standing_instructions,
    resolve_standing_instructions_section, StandingInstructionsKind, StandingInstructionsSource,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

#[derive(Deserialize)]
struct WireSource {
    kind: String,
    name: String,
    instructions: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        #[serde(rename = "projectId")]
        project_id: Option<String>,
        #[serde(rename = "characterId")]
        character_id: Option<String>,
        sources: Vec<WireSource>,
        section: Option<String>,
    },
    #[serde(rename = "render")]
    Render {
        id: String,
        sources: Vec<WireSource>,
        out: Option<String>,
    },
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/standing-instructions.json")
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

fn to_kind(s: &str) -> StandingInstructionsKind {
    match s {
        "project" => StandingInstructionsKind::Project,
        "group" => StandingInstructionsKind::Group,
        other => panic!("unknown source kind: {other}"),
    }
}

fn to_source(w: &WireSource) -> StandingInstructionsSource {
    StandingInstructionsSource {
        kind: to_kind(&w.kind),
        name: w.name.clone(),
        instructions: w.instructions.clone(),
    }
}

fn describe(sources: &[StandingInstructionsSource]) -> Vec<(String, String, String)> {
    sources
        .iter()
        .map(|s| {
            (
                match s.kind {
                    StandingInstructionsKind::Project => "project".to_string(),
                    StandingInstructionsKind::Group => "group".to_string(),
                },
                s.name.clone(),
                s.instructions.clone(),
            )
        })
        .collect()
}

fn describe_wire(sources: &[WireSource]) -> Vec<(String, String, String)> {
    sources
        .iter()
        .map(|s| (s.kind.clone(), s.name.clone(), s.instructions.clone()))
        .collect()
}

#[test]
fn standing_instructions_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_STANDING_INSTRUCTIONS"),
        env_or_skip("QT_FIXTURE_SI_MAIN"),
        env_or_skip("QT_FIXTURE_SI_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let pid = std::process::id();
    let scratch = std::env::temp_dir().join(format!("qt-si-rust-{pid}"));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let main_work = scratch.join("si-main.db");
    let mount_work = scratch.join("si-mount.db");
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let db = Db::open(
        DbPaths {
            main: main_work,
            mount_index: Some(mount_work),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open db: {e}"));

    let text = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut resolve_rows = 0usize;
    let mut render_rows = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("row parses") {
            Row::Resolve {
                id,
                project_id,
                character_id,
                sources,
                section,
            } => {
                let got = resolve_standing_instructions(
                    &db,
                    project_id.as_deref(),
                    character_id.as_deref(),
                );
                assert_eq!(
                    describe(&got),
                    describe_wire(&sources),
                    "resolve '{id}' sources"
                );
                let got_section = resolve_standing_instructions_section(
                    &db,
                    project_id.as_deref(),
                    character_id.as_deref(),
                );
                assert_eq!(got_section, section, "resolve '{id}' section");
                resolve_rows += 1;
            }
            Row::Render { id, sources, out } => {
                let ports: Vec<StandingInstructionsSource> =
                    sources.iter().map(to_source).collect();
                let got = render_standing_instructions_section(&ports);
                assert_eq!(got, out, "render '{id}'");
                render_rows += 1;
            }
        }
    }

    // Shape floors — a truncated / stale oracle must not pass by carrying less.
    assert!(
        resolve_rows >= 14,
        "expected at least 14 resolve rows, got {resolve_rows} — regenerate the oracle"
    );
    assert!(
        render_rows >= 8,
        "expected at least 8 render rows, got {render_rows} — regenerate the oracle"
    );
    eprintln!(
        "OK: standing-instructions matched oracle ({resolve_rows} resolve, {render_rows} render)."
    );
}
