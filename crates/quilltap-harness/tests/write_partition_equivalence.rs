//! Tier-1 differential test #5: write-batch partitioning + folder-conflict remap.
//!
//! Covers classify / partition / main-primary / rewriteFolderRefs /
//! isUniqueConstraintError. The two arbitrary-JSON-shaped functions (rewrite,
//! uniqueErr) follow the recall-history pattern: each oracle row carries BOTH
//! input and expected output, and the Rust port is fed the same bytes
//! (serde_json::Value) so there's no second transcription of subtle inputs.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/write-partition.ts \
//!     > /tmp/oracle-write-partition.ndjson
//! Run:
//!   QT_ORACLE_WRITE_PARTITION=/tmp/oracle-write-partition.ndjson \
//!     cargo test -p quilltap-harness --test write_partition_equivalence

use std::collections::HashMap;

use quilltap_core::write_apply::{apply_writes, ApplyError, ApplyHost};
use quilltap_core::write_partition::{
    classify_write_target, is_main_primary_job_type, is_unique_constraint_error, partition_writes,
    rewrite_folder_refs, ChildWritePayload, WriteDbTarget,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "classify")]
    Classify {
        id: String,
        method: String,
        out: String,
    },
    #[serde(rename = "partition")]
    Partition {
        id: String,
        writes: Vec<ChildWritePayload>,
        out: PartitionOut,
    },
    #[serde(rename = "mainPrimary")]
    MainPrimary {
        id: String,
        #[serde(rename = "jobType")]
        job_type: Option<String>,
        out: bool,
    },
    #[serde(rename = "rewrite")]
    Rewrite {
        id: String,
        write: ChildWritePayload,
        remap: HashMap<String, String>,
        out: ChildWritePayload,
    },
    #[serde(rename = "uniqueErr")]
    UniqueErr { id: String, err: Value, out: bool },
}

#[derive(Deserialize)]
struct PartitionOut {
    main: Vec<ChildWritePayload>,
    #[serde(rename = "mountIndex")]
    mount_index: Vec<ChildWritePayload>,
    #[serde(rename = "llmLogs")]
    llm_logs: Vec<ChildWritePayload>,
}

#[test]
fn write_partition_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_WRITE_PARTITION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_WRITE_PARTITION to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 5];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Classify { id, method, out } => {
                assert_eq!(
                    classify_write_target(&method).as_str(),
                    out,
                    "classify '{id}'"
                );
                counts[0] += 1;
            }
            OracleRow::Partition { id, writes, out } => {
                let got = partition_writes(&writes);
                assert_eq!(got.main, out.main, "partition '{id}' main");
                assert_eq!(
                    got.mount_index, out.mount_index,
                    "partition '{id}' mountIndex"
                );
                assert_eq!(got.llm_logs, out.llm_logs, "partition '{id}' llmLogs");
                counts[1] += 1;
            }
            OracleRow::MainPrimary { id, job_type, out } => {
                assert_eq!(
                    is_main_primary_job_type(job_type.as_deref()),
                    out,
                    "mainPrimary '{id}'"
                );
                counts[2] += 1;
            }
            OracleRow::Rewrite {
                id,
                write,
                remap,
                out,
            } => {
                let got = rewrite_folder_refs(&write, &remap);
                assert_eq!(got, out, "rewrite '{id}'");
                counts[3] += 1;
            }
            OracleRow::UniqueErr { id, err, out } => {
                assert_eq!(is_unique_constraint_error(&err), out, "uniqueErr '{id}'");
                counts[4] += 1;
            }
        }
    }

    // Floors: the two group-store keys P4.D221 added are the 14th/15th classify
    // rows and the 3rd partition row — a regen that lost them must not pass.
    assert!(
        counts[0] >= 15 && counts[1] >= 3,
        "the corpus lost rows (classify {}, partition {}; floors 15/3)",
        counts[0],
        counts[1]
    );
    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!(
        "OK: write-partition matched oracle ({} classify, {} partition, {} mainPrimary, {} rewrite, {} uniqueErr).",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    );
}

/// Minimal [`ApplyHost`] recorder: tracks which partition's transaction is
/// currently open (set on `BEGIN IMMEDIATE`, cleared on `COMMIT`/`ROLLBACK`)
/// and, for every dispatched write, which partition it landed under.
struct Recorder {
    current: Option<WriteDbTarget>,
    dispatched: Vec<(WriteDbTarget, String)>,
}

impl ApplyHost for Recorder {
    fn conn_available(&self, _partition: WriteDbTarget) -> bool {
        true
    }

    fn conn_exec(&mut self, partition: WriteDbTarget, sql: &str) -> Result<(), ApplyError> {
        match sql {
            "BEGIN IMMEDIATE" => self.current = Some(partition),
            "COMMIT" | "ROLLBACK" => self.current = None,
            _ => {}
        }
        Ok(())
    }

    fn dispatch(&mut self, write: &ChildWritePayload) -> Result<(), ApplyError> {
        let partition = self
            .current
            .expect("dispatch invoked outside an open transaction");
        self.dispatched.push((partition, write.method.clone()));
        Ok(())
    }

    fn find_folder(
        &mut self,
        _mount_point_id: &str,
        _path: &str,
    ) -> Result<Option<String>, ApplyError> {
        Ok(None)
    }

    fn finalize_file(
        &mut self,
        _final_dir: &str,
        _staging_path: &str,
        _final_path: &str,
    ) -> Result<(), ApplyError> {
        Ok(())
    }

    fn undo_finalize(&mut self, _final_path: &str, _staging_path: &str) {}

    fn cleanup_staging_dir(&mut self, _staging_root: &str) {}

    fn dispatch_invalidations(
        &mut self,
        _vector_store_keys: &[String],
        _mount_point_keys: &[String],
    ) {
    }
}

/// v4 `ad1c4c37f`: `MOUNT_INDEX_REPO_KEYS` gained `groupDocMountLinks` +
/// `groupCharacterMembers` — before the fix, `classify_write_target` routed
/// these two repos to `Main` (the default fallback), so their buffered writes
/// would have committed inside the MAIN transaction against the wrong
/// connection. This drives the real applier orchestration end to end
/// (`classify_write_target` -> `partition_writes` -> `apply_writes`), not just
/// the pure classifier, proving a `groupCharacterMembers` write lands inside
/// the MOUNT-INDEX transaction while an ordinary main-DB write stays in Main.
#[test]
fn group_character_members_write_applies_inside_mount_index_transaction() {
    let mut host = Recorder {
        current: None,
        dispatched: Vec::new(),
    };
    let writes = vec![
        ChildWritePayload {
            method: "chats.update".to_string(),
            args: vec![json!({ "id": "c1" })],
        },
        ChildWritePayload {
            method: "groupCharacterMembers.create".to_string(),
            args: vec![json!({ "id": "g1" })],
        },
        ChildWritePayload {
            method: "groupDocMountLinks.create".to_string(),
            args: vec![json!({ "id": "g2" })],
        },
    ];

    apply_writes(&mut host, "job-p4d221", &writes, None).expect("apply_writes");

    let partition_of = |method: &str| -> WriteDbTarget {
        host.dispatched
            .iter()
            .find(|(_, m)| m == method)
            .unwrap_or_else(|| panic!("{method} was never dispatched"))
            .0
    };

    assert_eq!(
        partition_of("groupCharacterMembers.create"),
        WriteDbTarget::MountIndex,
        "groupCharacterMembers must apply inside the mount-index transaction, not main"
    );
    assert_eq!(
        partition_of("groupDocMountLinks.create"),
        WriteDbTarget::MountIndex,
        "groupDocMountLinks must apply inside the mount-index transaction, not main"
    );
    assert_eq!(
        partition_of("chats.update"),
        WriteDbTarget::Main,
        "an ordinary main-DB write must stay in the main transaction"
    );
}
