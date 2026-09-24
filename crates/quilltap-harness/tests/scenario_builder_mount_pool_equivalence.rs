//! Tier-2 differential: the Scenario Builder mount pool — v4
//! `resolveScenarioBuilderMountPool` (`lib/scenario-builder/mount-pool.ts`,
//! `d1c06cd9d`) against v5's
//! `quilltap_core::services::scenario_builder::mount_pool`.
//!
//! Both sides run over the SAME real two-partition fixture — the doc-opacity
//! fixture, built by its own UNCHANGED builder (P4.D216's), copied per run —
//! with the SAME plants from `harness/oracle/fixtures/scenario-builder-mount-
//! pool.json` applied IN ORDER through each side's own SQL connection (v4's
//! `rawQuery`, v5's writable `Writer`): Leilani's raw row cloned into an
//! ARCHIVED member, a FOREIGN member, a member whose vault IS the group's
//! official store, a vault-LESS member, a TWIN sharing Leilani's vault and an
//! UNREADABLE member (a BLOB `name`); then General's setting deleted; then the
//! settings table dropped. v4's `mount-pool.test.ts` mocks every repository, so
//! its eight shapes are this family's arm NAMES, planted rather than mocked.
//!
//! **Comparand, per arm:** the five pool fields (arrays in v4's order — the
//! object's KEY order is not compared: v4 spreads the deduped triple and then
//! appends `participantMountPointIds`, a Rust struct serializes in declaration
//! order) and the `ScenarioBuilderMountPool` logger's lines, level + message +
//! every field, `error` by presence (v4's `error.message` vs v5's `Display`).
//! v4's repository-level `safeQuery` fallback line — which the UNREADABLE arm
//! reaches in v4 on a logger the oracle does not record — is pinned on the v5
//! side alone: exactly once on that arm, never elsewhere (see `mount_pool.rs`).
//!
//! **P4.113:** plus v4's `[InstanceSettings] Failed to read setting` WARN
//! (`readSetting`'s catch, on the plain `@/lib/logger`) — exactly one on
//! `general-read-fails`, silence on every other arm (`general-absent`'s missing
//! row is SILENT) — through this file's OWN capture of
//! `quilltap_core::db::instance_settings`; and v4's backend `Raw query failed`
//! ERROR on that same arm, a recorded v4-only line pinned both ways.
//!
//! ⚠ PIN REQUIRED at the TARGET `d1c06cd9d` (the module does not exist at the
//! `00c290c9a` baseline — the jest import fails there, the pin proof). The
//! fixture pair is MINTED — rebuild, regenerate, THEN `cargo test` against that
//! SAME build, in that order. Stage the case OUTSIDE `.claude/` (v4's jest
//! ignores those paths).
//!
//!     N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
//!     STAGE=/tmp/qt-oracle-stage-sb-pool
//!     rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!     cp $W/harness/oracle/cases/scenario-builder-mount-pool.test.ts $STAGE/harness/oracle/cases/
//!     cp $W/harness/oracle/fixtures/scenario-builder-mount-pool.json $STAGE/harness/oracle/fixtures/
//!     cd ~/source/quilltap-server
//!     rm -f /tmp/qt-sbpool-main.db /tmp/qt-sbpool-mount.db
//!     QT_FIXTURE_DOPA_MAIN=/tmp/qt-sbpool-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-sbpool-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
//!     QT_FIXTURE_SBPOOL_MAIN=/tmp/qt-sbpool-main.db QT_FIXTURE_SBPOOL_MOUNT=/tmp/qt-sbpool-mount.db \
//!     QT_ORACLE_OUT=/tmp/oracle-scenario-builder-mount-pool.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!     --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "scenario-builder-mount-pool\.test\.ts$"
//!
//! Then:
//!
//!     QT_ORACLE_SBPOOL=/tmp/oracle-scenario-builder-mount-pool.ndjson \
//!     QT_FIXTURE_SBPOOL_MAIN=/tmp/qt-sbpool-main.db QT_FIXTURE_SBPOOL_MOUNT=/tmp/qt-sbpool-mount.db \
//!     cargo test -p quilltap-harness --test scenario_builder_mount_pool_equivalence -- --nocapture

mod scenario_builder_capture;

use quilltap_core::db::tiered_mount_pool::TieredMountPool;
use quilltap_core::db::Writer;
use quilltap_core::services::scenario_builder::mount_pool::resolve_scenario_builder_mount_pool;
use scenario_builder_capture::{normalize, v4_lines, StructuralCapture};
use serde::Deserialize;
use serde_json::Value;

const TARGETS: &[&str] = &["quilltap_core::services::scenario_builder::mount_pool"];

/// P4.113 — the `read_setting` WARN's target. Captured by this file's OWN
/// layer ([`SettingsCapture`]), never by widening the shared
/// `scenario_builder_capture` module (P4.114's this round, §R.10(c)).
const SETTINGS_TARGET: &str = "quilltap_core::db::instance_settings";

/// v4's backend ERROR logged before `readSetting`'s catch — v4-only (v5 has
/// no `rawQuery` layer). Pinned both ways: v4 must still log it exactly on
/// the arms named here (else VANISHED), v5 must never.
const RAW_QUERY_FAILED: &str = "Raw query failed";
const RAW_QUERY_FAILED_ARMS: &[&str] = &["general-read-fails"];

/// One captured `instance_settings` line: level, message, `key`, and whether
/// a non-empty `error` field was present.
type SettingsLine = (String, String, Option<String>, bool);

#[derive(Clone, Default)]
struct SettingsCapture(std::sync::Arc<std::sync::Mutex<Vec<SettingsLine>>>);

#[derive(Default)]
struct SettingsFields {
    message: String,
    key: Option<String>,
    has_error: bool,
}

impl tracing::field::Visit for SettingsFields {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        match f.name() {
            "key" => self.key = Some(v.to_string()),
            "error" => self.has_error = !v.is_empty(),
            "message" => self.message = v.to_string(),
            _ => {}
        }
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        let rendered = format!("{v:?}");
        match f.name() {
            "message" => self.message = rendered,
            "error" => self.has_error = !rendered.is_empty(),
            "key" => self.key = Some(rendered.trim_matches('"').to_string()),
            _ => {}
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SettingsCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if !event.metadata().target().starts_with(SETTINGS_TARGET) {
            return;
        }
        let mut f = SettingsFields::default();
        event.record(&mut f);
        self.0.lock().unwrap().push((
            event.metadata().level().to_string(),
            f.message,
            f.key,
            f.has_error,
        ));
    }
}

/// v4's `settingsLogs` for one arm, as [`SettingsLine`]s — the `[InstanceSettings]`
/// lines only (the `Raw query failed` ERROR is the recorded divergence,
/// counted separately).
fn v4_settings_lines(want: &Value) -> Vec<SettingsLine> {
    want["settingsLogs"]
        .as_array()
        .expect("the oracle carries settingsLogs — regenerate it (P4.113)")
        .iter()
        .filter(|l| l["message"].as_str() != Some(RAW_QUERY_FAILED))
        .map(|l| {
            (
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap().to_string(),
                l["key"].as_str().map(str::to_string),
                l["hasError"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[derive(Deserialize)]
struct Statement {
    sql: String,
    params: Vec<Option<String>>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Step {
    Plant {
        #[allow(dead_code)]
        name: String,
        statements: Vec<Statement>,
    },
    #[serde(rename_all = "camelCase")]
    Arm {
        name: String,
        user_id: String,
        project_id: Option<String>,
        character_ids: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    leilani_id: String,
    steps: Vec<Step>,
}

fn pool_fields(p: &TieredMountPool) -> Value {
    serde_json::json!({
        "characterMountPointId": p.character_mount_point_id,
        "participantMountPointIds": p.participant_mount_point_ids,
        "groupMountPointIds": p.group_mount_point_ids,
        "projectMountPointIds": p.project_mount_point_ids,
        "globalMountPointId": p.global_mount_point_id,
    })
}

fn v4_pool_fields(p: &Value) -> Value {
    serde_json::json!({
        "characterMountPointId": p["characterMountPointId"],
        "participantMountPointIds": p["participantMountPointIds"],
        "groupMountPointIds": p["groupMountPointIds"],
        "projectMountPointIds": p["projectMountPointIds"],
        "globalMountPointId": p["globalMountPointId"],
    })
}

#[test]
fn scenario_builder_mount_pool_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SBPOOL") else {
        eprintln!("SKIP: set QT_ORACLE_SBPOOL to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_SBPOOL_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_SBPOOL_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_SBPOOL_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_SBPOOL_MOUNT to the seed mount-index .db (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/scenario-builder-mount-pool.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    let arms = spec
        .steps
        .iter()
        .filter(|s| matches!(s, Step::Arm { .. }))
        .count();
    assert_eq!(oracle.len(), arms, "oracle arm count != spec arm count");
    assert!(
        arms >= 8,
        "the eight mount-pool.test.ts shapes must all be armed"
    );

    let scratch = std::env::temp_dir().join(format!("qt-sbpool-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("main.db");
    let work_mount = scratch.join("mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount");
    let main_w = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount_w = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");
    let main = main_w.connection();
    let mount = mount_w.connection();

    let leilani_vault: String = main
        .query_row(
            "SELECT \"characterDocumentMountPointId\" FROM \"characters\" WHERE \"id\" = ?1",
            [&spec.leilani_id],
            |r| r.get(0),
        )
        .expect("leilani vault minted");
    let sub = |p: &Option<String>| -> Option<String> {
        p.as_ref().map(|s| {
            if s == "{{leilaniVaultId}}" {
                leilani_vault.clone()
            } else {
                s.clone()
            }
        })
    };

    use tracing_subscriber::layer::SubscriberExt;
    let (layer, captured) = StructuralCapture::new(TARGETS);
    let settings = SettingsCapture::default();
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(layer)
            .with(settings.clone()),
    );
    let mut raw_query_failed_seen = 0usize;
    let mut settings_warns_seen = 0usize;

    let mut failures: Vec<String> = Vec::new();
    let mut idx = 0usize;
    let mut saw_archived_line = false;
    for step in &spec.steps {
        match step {
            Step::Plant { statements, .. } => {
                for st in statements {
                    let params: Vec<Option<String>> = st.params.iter().map(sub).collect();
                    main.execute(&st.sql, rusqlite::params_from_iter(params.iter()))
                        .unwrap_or_else(|e| panic!("plant `{}`: {e}", st.sql));
                }
            }
            Step::Arm {
                name,
                user_id,
                project_id,
                character_ids,
            } => {
                let want = &oracle[idx];
                idx += 1;
                assert_eq!(want["arm"].as_str(), Some(name.as_str()), "arm order");
                captured.lock().unwrap().clear();
                settings.0.lock().unwrap().clear();
                let pool = resolve_scenario_builder_mount_pool(
                    main,
                    mount,
                    user_id,
                    project_id.as_deref(),
                    character_ids,
                );
                // v4's `safeQuery` fallback line (`Error finding entity by ID`)
                // lives on the REPOSITORY's logger, which the oracle does not
                // record; v5 emits it from this module (`mount_pool.rs`'s read
                // loop). It is split out of the pool-logger comparand and pinned
                // on its own: exactly once, on the unreadable arm, never else.
                let (safe_query, got_lines): (Vec<_>, Vec<_>) = captured
                    .lock()
                    .unwrap()
                    .clone()
                    .into_iter()
                    .partition(|l| l.message == "Error finding entity by ID");
                let want_safe_query = usize::from(name == "unreadable-member-warns");
                if safe_query.len() != want_safe_query {
                    failures.push(format!(
                        "{name}: expected {want_safe_query} safeQuery fallback line(s), got {}",
                        safe_query.len()
                    ));
                }
                let (g, w) = (pool_fields(&pool), v4_pool_fields(&want["pool"]));
                if g != w {
                    failures.push(format!("{name}: POOL differs\n  v4: {w}\n  v5: {g}"));
                }
                let (gl, wl) = (
                    normalize(&got_lines, &["error"]),
                    normalize(&v4_lines(&want["logs"]), &["error"]),
                );
                if gl != wl {
                    failures.push(format!(
                        "{name}: LOG lines differ\n  v4: {wl:#?}\n  v5: {gl:#?}"
                    ));
                }
                // P4.113: v4's `[InstanceSettings] Failed to read setting`
                // WARN (`readSetting`'s catch) — exactly the dropped-table arm;
                // silence on every other (a missing row is SILENT).
                let got_settings = settings.0.lock().unwrap().clone();
                let want_settings = v4_settings_lines(want);
                if got_settings != want_settings {
                    failures.push(format!(
                        "{name}: [InstanceSettings] lines differ\n  v4: {want_settings:?}\n  v5: {got_settings:?}"
                    ));
                }
                settings_warns_seen += want_settings.len();
                // The recorded v4-only `Raw query failed` ERROR, both ways.
                let v4_raw = want["settingsLogs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|l| l["message"].as_str() == Some(RAW_QUERY_FAILED))
                    .count();
                let want_raw = usize::from(RAW_QUERY_FAILED_ARMS.contains(&name.as_str()));
                if v4_raw != want_raw {
                    failures.push(format!(
                        "{name}: v4 `{RAW_QUERY_FAILED}` count {v4_raw}, recorded {want_raw} — \
                         the divergence VANISHED or moved; re-survey the backend"
                    ));
                }
                raw_query_failed_seen += v4_raw;
                if got_settings.iter().any(|l| l.1 == RAW_QUERY_FAILED) {
                    failures.push(format!(
                        "{name}: v5 logged `{RAW_QUERY_FAILED}` — WRONG SHAPE (v5 has no rawQuery layer)"
                    ));
                }
                if wl
                    .iter()
                    .any(|l| l.1 == "Archived cast member contributes nothing to the pool")
                {
                    saw_archived_line = true;
                }
            }
        }
    }
    drop(main_w);
    drop(mount_w);
    let _ = std::fs::remove_dir_all(&scratch);
    eprintln!("scenario_builder_mount_pool: {arms} arms");
    assert!(saw_archived_line, "no arm pins the archived-member DEBUG");
    assert_eq!(
        (settings_warns_seen, raw_query_failed_seen),
        (1, RAW_QUERY_FAILED_ARMS.len()),
        "the [InstanceSettings] WARN pin and the v4-only `{RAW_QUERY_FAILED}` pin must each be \
         exercised exactly once"
    );
    assert!(
        failures.is_empty(),
        "{} arm(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
