//! P4.9c DATA-DIRECTORY differential: `services::data_dir::data_dir_info` vs
//! v4's REAL `lib/paths.ts` family (`getBaseDataDirWithSource`, `getPlatform`,
//! `isDockerEnvironment`, `isLimaEnvironment`, `isElectronShell`,
//! `getElectronShellVersion`, `getShellCapabilities`, `getHostDataDir`) across
//! an environment matrix.
//!
//! Tier 1 (exact strings). Each oracle row carries BOTH the ambient snapshot
//! v4's functions read AND the resulting payload, so this side replays the
//! inputs verbatim — there is no duplicated case list to drift, and a
//! disagreement can only be about the LOGIC.
//!
//! Generate the oracle (Node 24, from the PINNED v4 worktree — see the .ts):
//!   cd /tmp/qt-v4-baseline
//!   QT_ORACLE_OUT=/tmp/oracle-data-dir.ndjson \
//!     $N/node --import tsx $V5W/harness/oracle/cases/data-dir-paths.ts
//! Run:
//!   QT_ORACLE_DATA_DIR=/tmp/oracle-data-dir.ndjson \
//!     cargo test -p quilltap-harness --test data_dir_paths_equivalence -- --nocapture

use quilltap_core::services::data_dir::{data_dir_info, DataDirEnv};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    lima_container: Option<String>,
    docker_container: Option<String>,
    quilltap_data_dir: Option<String>,
    quilltap_host_data_dir: Option<String>,
    quilltap_shell: Option<String>,
    quilltap_shell_capabilities: Option<String>,
    appdata: Option<String>,
    home_dir: String,
    os_platform: String,
    dockerenv_exists: bool,
    app_is_dir: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    path: String,
    source: String,
    source_description: String,
    platform: String,
    is_docker: bool,
    #[serde(rename = "isVM")]
    is_vm: bool,
    is_electron_shell: bool,
    shell_version: Option<String>,
    shell_capabilities: Vec<String>,
    can_open: bool,
    host_path: String,
}

#[derive(Deserialize)]
struct Row {
    name: String,
    snapshot: Snapshot,
    info: Info,
}

impl From<Snapshot> for DataDirEnv {
    fn from(s: Snapshot) -> Self {
        DataDirEnv {
            lima_container: s.lima_container,
            docker_container: s.docker_container,
            quilltap_data_dir: s.quilltap_data_dir,
            quilltap_host_data_dir: s.quilltap_host_data_dir,
            quilltap_shell: s.quilltap_shell,
            quilltap_shell_capabilities: s.quilltap_shell_capabilities,
            appdata: s.appdata,
            home_dir: s.home_dir,
            os_platform: s.os_platform,
            dockerenv_exists: s.dockerenv_exists,
            app_is_dir: s.app_is_dir,
        }
    }
}

#[test]
fn data_dir_paths_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DATA_DIR") else {
        eprintln!("SKIP: set QT_ORACLE_DATA_DIR (see test header).");
        return;
    };

    let raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let rows: Vec<Row> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert!(!rows.is_empty(), "oracle NDJSON is empty");

    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let case_count = rows.len();

    let mut failed: Vec<String> = Vec::new();
    for row in rows {
        let name = row.name;
        let env: DataDirEnv = row.snapshot.into();
        let got = data_dir_info(&env);
        let want = &row.info;

        let mut diffs: Vec<String> = Vec::new();
        let mut cmp = |field: &str, got: String, want: &str| {
            if got != want {
                diffs.push(format!("  {field}: got {got:?} want {want:?}"));
            }
        };
        cmp("path", got.path.clone(), &want.path);
        cmp("source", got.source.as_str().to_string(), &want.source);
        cmp(
            "sourceDescription",
            got.source_description.clone(),
            &want.source_description,
        );
        cmp(
            "platform",
            got.platform.as_str().to_string(),
            &want.platform,
        );
        cmp(
            "isDocker",
            got.is_docker.to_string(),
            &want.is_docker.to_string(),
        );
        cmp("isVM", got.is_vm.to_string(), &want.is_vm.to_string());
        cmp(
            "isElectronShell",
            got.is_electron_shell.to_string(),
            &want.is_electron_shell.to_string(),
        );
        cmp(
            "shellVersion",
            format!("{:?}", got.shell_version),
            &format!("{:?}", want.shell_version),
        );
        cmp(
            "shellCapabilities",
            format!("{:?}", got.shell_capabilities),
            &format!("{:?}", want.shell_capabilities),
        );
        cmp(
            "canOpen",
            got.can_open.to_string(),
            &want.can_open.to_string(),
        );
        cmp("hostPath", got.host_path.clone(), &want.host_path);

        if diffs.is_empty() {
            eprintln!("[{name}] OK.");
        } else {
            eprintln!("[{name}] MISMATCH:\n{}", diffs.join("\n"));
            failed.push(name);
        }
    }

    eprintln!("data-dir differential: {case_count} cases.");
    assert!(failed.is_empty(), "cases diverged from v4: {failed:?}");

    // Corpus-coverage guard: a thinned oracle must fail loudly rather than pass
    // on a smaller world (the workbench-fixture lesson).
    for required in [
        "default_darwin",
        "default_linux",
        "default_win32_no_appdata",
        "override_tilde",
        "override_empty_string",
        "docker_dockerenv_marker",
        "docker_app_dir_marker",
        "docker_ignores_override",
        "lima_beats_docker_markers",
        "shell_version_and_capabilities",
        "host_path_inside_docker",
        "host_path_outside_container",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "oracle is missing the {required:?} case"
        );
    }
}
