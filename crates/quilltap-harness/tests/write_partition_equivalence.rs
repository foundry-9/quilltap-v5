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
//!   QT_ORACLE_WRITE_PARTITION=/tmp/oracle-write-partition.ndjson cargo test -p quilltap-harness

use std::collections::HashMap;

use quilltap_core::write_partition::{
    classify_write_target, is_main_primary_job_type, is_unique_constraint_error, partition_writes,
    rewrite_folder_refs, ChildWritePayload,
};
use serde::Deserialize;
use serde_json::Value;

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

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!(
        "OK: write-partition matched oracle ({} classify, {} partition, {} mainPrimary, {} rewrite, {} uniqueErr).",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    );
}
