//! Tier-1 differential (P4.D381, v4 `bf7a457f6`): v4's REAL
//! `lib/files/cloud-placeholder.ts` (`STAT_BLOCK_BYTES`,
//! `isNotFullyMaterialized`, `isEvictedCloudPlaceholder(stat, platform)`)
//! against `quilltap_core::services::mount_index::cloud_placeholder`, over
//! v4's ten test vectors plus a size × blocks × platform grid. Exact.
//!
//! `freebsd` maps to `Platform::Other` — every platform v4 treats as having
//! no placeholders. The production reader's non-unix arm
//! (`PLACEHOLDER_CHECK_UNIX_ONLY`, R-3) is not exercised here: this family
//! compares the RULE, which v5 carries whole including the `win32` arm.
//!
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/cloud-placeholder.ts \
//!     > /tmp/oracle-cloud-placeholder.ndjson
//! Run:
//!   QT_ORACLE_CLOUD_PLACEHOLDER=/tmp/oracle-cloud-placeholder.ndjson \
//!     cargo test -p quilltap-harness --test cloud_placeholder_equivalence -- --nocapture

use quilltap_core::services::mount_index::cloud_placeholder::{
    is_evicted_cloud_placeholder, is_not_fully_materialized, AllocationStat, Platform,
    STAT_BLOCK_BYTES,
};
use serde_json::Value;

fn platform(p: &str) -> Platform {
    match p {
        "darwin" => Platform::Darwin,
        "win32" => Platform::Win32,
        "linux" => Platform::Linux,
        _ => Platform::Other,
    }
}

#[test]
fn cloud_placeholder_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CLOUD_PLACEHOLDER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CLOUD_PLACEHOLDER to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let (mut rows, mut saw_const, mut saw_win32_evicted) = (0usize, false, false);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        let id = v["id"].as_str().unwrap();
        if id == "const" {
            assert_eq!(v["statBlockBytes"].as_u64(), Some(STAT_BLOCK_BYTES));
            saw_const = true;
            continue;
        }
        let stat = AllocationStat {
            size: v["size"].as_u64().unwrap(),
            blocks: v["blocks"].as_u64().unwrap(),
        };
        let p = v["platform"].as_str().unwrap();
        let evicted = is_evicted_cloud_placeholder(stat, platform(p));
        assert_eq!(
            Some(evicted),
            v["evicted"].as_bool(),
            "{id}: isEvictedCloudPlaceholder"
        );
        assert_eq!(
            Some(is_not_fully_materialized(stat)),
            v["notFullyMaterialized"].as_bool(),
            "{id}: isNotFullyMaterialized"
        );
        saw_win32_evicted |= p == "win32" && evicted;
        rows += 1;
    }
    assert!(saw_const, "the STAT_BLOCK_BYTES row is missing");
    assert!(
        saw_win32_evicted,
        "no win32 evicted row: the grid did not exercise the Windows arm"
    );
    assert_eq!(rows, 10 + 7 * 4 * 4, "row count");
    eprintln!("OK: cloud-placeholder matched oracle ({rows} rows).");
}
