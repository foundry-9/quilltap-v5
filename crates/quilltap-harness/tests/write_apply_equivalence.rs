//! Differential test for the partitioned write APPLIER orchestration
//! (`quilltap-core::write_apply`) vs v4's real `applyWritesUnsafe`.
//!
//! Tier-1-style TRACE differential (the apply path is orchestration; row writes
//! are delegated to repos, each tier-2-verified separately). Both sides run the
//! committed corpus (harness/oracle/fixtures/write-apply.json) and emit the same
//! observable trace per scenario: per-partition exec sequence (BEGIN IMMEDIATE /
//! COMMIT / ROLLBACK), the ordered ATTEMPTED repo dispatches (method + args, args
//! post folder-remap), the reconcile lookups, the `__finalizeFile` + post-commit
//! effects (fs renames incl. undo-on-rollback, mkdirs, staging-dir cleanup, and
//! the deduped cache-invalidation notifications), and the resolved/threw outcome.
//!
//! The oracle runs under v4's jest (the applier's `getRawDatabase()` /
//! `getRepositories()` singletons are `jest.mock`-injected). Generate it:
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-write-apply.ndjson \
//!     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- write-apply
//! Run:
//!   QT_ORACLE_WRITE_APPLY=/tmp/oracle-write-apply.ndjson \
//!     cargo test -p quilltap-harness --test write_apply_equivalence

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::write_apply::{apply_writes, ApplyError, ApplyHost};
use quilltap_core::write_partition::{ChildWritePayload, WriteDbTarget};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Corpus {
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    #[serde(rename = "jobType")]
    job_type: Option<String>,
    writes: Vec<ChildWritePayload>,
    #[serde(default)]
    fail: Vec<FailSpec>,
    #[serde(default, rename = "findFolder")]
    find_folder: Vec<FindFolderSpec>,
    #[serde(default)]
    unavailable: Vec<String>,
    #[serde(default, rename = "failCommit")]
    fail_commit: Vec<CommitFailSpec>,
}

#[derive(Deserialize)]
struct FailSpec {
    method: String,
    message: String,
    code: Option<String>,
}

#[derive(Deserialize)]
struct FindFolderSpec {
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
    path: String,
    result: Option<String>,
}

#[derive(Deserialize)]
struct CommitFailSpec {
    partition: String,
    message: String,
}

/// A recording [`ApplyHost`]: drives nothing real, records the orchestration's
/// observable effects, and injects the scenario's scripted failures/lookups.
struct RecHost {
    fail: HashMap<String, (String, Option<String>)>,
    find: HashMap<(String, String), Option<String>>,
    unavailable: HashSet<String>,
    commit_fail: HashMap<String, String>,
    exec_main: Vec<String>,
    exec_mount: Vec<String>,
    exec_llm: Vec<String>,
    dispatched: Vec<Value>,
    lookups: Vec<Value>,
    // Post-commit / filesystem effects (matched against v4's fs spies + notifyChild).
    renames: Vec<Value>, // {from, to} across finalize (forward) + undo (reverse)
    mkdirs: Vec<String>, // ensureDir(dirname(finalPath))
    rms: Vec<String>,    // cleanupStagingDirs -> rmSync(root)
    notifications: Vec<Value>, // {kind, key} from dispatchInvalidations
}

impl RecHost {
    fn new(sc: &Scenario) -> Self {
        Self {
            fail: sc
                .fail
                .iter()
                .map(|f| (f.method.clone(), (f.message.clone(), f.code.clone())))
                .collect(),
            find: sc
                .find_folder
                .iter()
                .map(|f| ((f.mount_point_id.clone(), f.path.clone()), f.result.clone()))
                .collect(),
            unavailable: sc.unavailable.iter().cloned().collect(),
            commit_fail: sc
                .fail_commit
                .iter()
                .map(|c| (c.partition.clone(), c.message.clone()))
                .collect(),
            exec_main: Vec::new(),
            exec_mount: Vec::new(),
            exec_llm: Vec::new(),
            dispatched: Vec::new(),
            lookups: Vec::new(),
            renames: Vec::new(),
            mkdirs: Vec::new(),
            rms: Vec::new(),
            notifications: Vec::new(),
        }
    }

    fn exec_vec(&mut self, partition: WriteDbTarget) -> &mut Vec<String> {
        match partition {
            WriteDbTarget::Main => &mut self.exec_main,
            WriteDbTarget::MountIndex => &mut self.exec_mount,
            WriteDbTarget::LlmLogs => &mut self.exec_llm,
        }
    }
}

impl ApplyHost for RecHost {
    fn conn_available(&self, partition: WriteDbTarget) -> bool {
        !self.unavailable.contains(partition.as_str())
    }

    fn conn_exec(&mut self, partition: WriteDbTarget, sql: &str) -> Result<(), ApplyError> {
        let key = partition.as_str().to_string();
        self.exec_vec(partition).push(sql.to_string());
        if sql == "COMMIT" {
            if let Some(msg) = self.commit_fail.get(&key) {
                return Err(ApplyError::msg(msg.clone()));
            }
        }
        Ok(())
    }

    fn dispatch(&mut self, write: &ChildWritePayload) -> Result<(), ApplyError> {
        self.dispatched.push(json!({
            "method": write.method,
            "args": write.args,
        }));
        if let Some((message, code)) = self.fail.get(&write.method) {
            return Err(ApplyError {
                message: message.clone(),
                code: code.clone(),
            });
        }
        Ok(())
    }

    fn find_folder(
        &mut self,
        mount_point_id: &str,
        path: &str,
    ) -> Result<Option<String>, ApplyError> {
        self.lookups.push(json!({
            "mountPointId": mount_point_id,
            "path": path,
        }));
        Ok(self
            .find
            .get(&(mount_point_id.to_string(), path.to_string()))
            .cloned()
            .flatten())
    }

    fn finalize_file(
        &mut self,
        final_dir: &str,
        staging_path: &str,
        final_path: &str,
    ) -> Result<(), ApplyError> {
        // v4: ensureDirSync(dirname(final)) then renameSync(staging -> final).
        self.mkdirs.push(final_dir.to_string());
        self.renames
            .push(json!({ "from": staging_path, "to": final_path }));
        Ok(())
    }

    fn undo_finalize(&mut self, final_path: &str, staging_path: &str) {
        // v4's reverse rename on rollback: renameSync(final -> staging).
        self.renames
            .push(json!({ "from": final_path, "to": staging_path }));
    }

    fn cleanup_staging_dir(&mut self, staging_root: &str) {
        self.rms.push(staging_root.to_string());
    }

    fn dispatch_invalidations(
        &mut self,
        vector_store_keys: &[String],
        mount_point_keys: &[String],
    ) {
        // v4 notifies all vectorStore keys, then all mountPoint keys, in order.
        for k in vector_store_keys {
            self.notifications
                .push(json!({ "kind": "vectorStore", "key": k }));
        }
        for k in mount_point_keys {
            self.notifications
                .push(json!({ "kind": "mountPoint", "key": k }));
        }
    }
}

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/write-apply.json")
}

#[test]
fn write_apply_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WRITE_APPLY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WRITE_APPLY to the oracle NDJSON (see test header).");
            return;
        }
    };

    let corpus: Corpus =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("parse corpus");

    // Oracle NDJSON -> name -> trace value.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let oracle: HashMap<String, Value> = oracle_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle line");
            (v["name"].as_str().expect("name").to_string(), v)
        })
        .collect();

    assert_eq!(
        oracle.len(),
        corpus.scenarios.len(),
        "oracle scenario count != corpus"
    );

    for sc in &corpus.scenarios {
        let mut host = RecHost::new(sc);
        let outcome = match apply_writes(&mut host, &sc.name, &sc.writes, sc.job_type.as_deref()) {
            Ok(()) => Value::String("resolved".into()),
            Err(e) => json!({ "threw": e.message }),
        };

        let got = json!({
            "case": "write-apply",
            "name": sc.name,
            "exec": {
                "main": host.exec_main,
                "mountIndex": host.exec_mount,
                "llmLogs": host.exec_llm,
            },
            "dispatched": host.dispatched,
            "lookups": host.lookups,
            "renames": host.renames,
            "mkdirs": host.mkdirs,
            "rms": host.rms,
            "notifications": host.notifications,
            "outcome": outcome,
        });

        let want = oracle
            .get(&sc.name)
            .unwrap_or_else(|| panic!("oracle missing scenario {}", sc.name));

        assert_eq!(
            &got, want,
            "trace diverged for `{}`\n  rust:   {got}\n  oracle: {want}",
            sc.name
        );
    }

    eprintln!(
        "OK: write-apply matched oracle ({} scenarios).",
        corpus.scenarios.len()
    );
}
