//! Tier-1 differential: the document-store sync PLANNER (v4
//! `lib/mount-index/sync/planner.ts`, `23da0b322`) vs
//! `quilltap_core::services::mount_index::sync::planner::plan_sync`.
//!
//! The planner is pure and table-driven — two entry maps, a base, the options
//! and two booleans in; actions and warnings out — so the whole decision table
//! is one corpus. Each NDJSON row carries the INPUT as well as the output, so
//! this side replays exactly what v4's real `planSync` was given rather than
//! re-stating the corpus in Rust; the actions are then diffed as whole JSON
//! values, key by key, IN ORDER (the plan's order is a promise: parents before
//! children, store before disk, deletions children-first and last).
//!
//! What this family is the instrument for, beyond the decision table: the
//! stable-sort ties (a `create` and the `describe` pushed right after it share
//! path, side and entry kind), the `localeCompare` tie-break at one depth, and
//! the absent-vs-null distinction on `createdAt` — v4 writes `undefined` for a
//! disk touch on a platform that cannot set a birthtime and `null` for a file
//! neither side can date, and the two serialize differently.
//!
//! Regenerate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   npx tsx $V5W/harness/oracle/cases/sync-planner.ts > /tmp/oracle-sync-planner.ndjson
//! Run:
//!   QT_ORACLE_SYNC_PLANNER=/tmp/oracle-sync-planner.ndjson cargo test -p quilltap-harness --test sync_planner_equivalence -- --nocapture

use quilltap_core::services::mount_index::sync::planner::{plan_sync, PlanInput};
use quilltap_core::services::mount_index::sync::types::{
    ManifestEntry, OrderedMap, SyncEntry, SyncEntryMap, SyncOptions,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Row {
    id: String,
    input: Input,
    actions: Vec<Value>,
    warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    store: Vec<SyncEntry>,
    disk: Vec<SyncEntry>,
    /// A JS object literal in the corpus; the planner only ever looks keys up,
    /// so its own iteration order is not load-bearing — but the KEY SET order
    /// is, because the base's keys join the planner's key set after the two
    /// walks'. The corpus writes them in one order and `serde_json`'s
    /// `preserve_order` hands them back in it.
    base: serde_json::Map<String, Value>,
    options: SyncOptions,
    is_character_vault: bool,
    can_set_disk_birthtime: bool,
}

fn entry_map(entries: Vec<SyncEntry>) -> SyncEntryMap {
    let mut map = SyncEntryMap::new();
    for entry in entries {
        let key = entry.relative_path.to_lowercase();
        map.insert(key, entry);
    }
    map
}

fn base_map(raw: &serde_json::Map<String, Value>) -> OrderedMap<ManifestEntry> {
    let mut map = OrderedMap::new();
    for (key, value) in raw {
        let entry: ManifestEntry =
            serde_json::from_value(value.clone()).expect("parse a corpus manifest entry");
        map.insert(key.to_lowercase(), entry);
    }
    map
}

#[test]
fn sync_planner_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SYNC_PLANNER") else {
        eprintln!("SKIP: set QT_ORACLE_SYNC_PLANNER to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("cannot read oracle {oracle_path}: {e}"));

    let mut n = 0usize;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut mismatches: Vec<String> = Vec::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        n += 1;

        let store = entry_map(row.input.store);
        let disk = entry_map(row.input.disk);
        let base = base_map(&row.input.base);
        let out = plan_sync(&PlanInput {
            store: &store,
            disk: &disk,
            base: &base,
            options: &row.input.options,
            is_character_vault: row.input.is_character_vault,
            can_set_disk_birthtime: row.input.can_set_disk_birthtime,
        });

        for action in &out.actions {
            *kinds.entry(action.kind.as_str().to_string()).or_default() += 1;
        }

        let got: Vec<Value> = out
            .actions
            .iter()
            .map(|a| serde_json::to_value(a).expect("serialize an action"))
            .collect();
        if got != row.actions {
            mismatches.push(format!(
                "[{}] actions differ\n      v4: {}\n      v5: {}",
                row.id,
                serde_json::to_string(&row.actions).unwrap(),
                serde_json::to_string(&got).unwrap(),
            ));
        }
        if out.warnings != row.warnings {
            mismatches.push(format!(
                "[{}] warnings differ\n      v4: {:?}\n      v5: {:?}",
                row.id, row.warnings, out.warnings
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} disagreements over {n} cases:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    // EXACT, not a floor: a stale oracle regenerated before the corpus grew
    // would sail past a `>=` having measured none of the new shapes.
    assert_eq!(
        n, 75,
        "expected the full corpus (75 cases), saw {n} — stale oracle?"
    );
    // Every action kind the planner can emit must actually appear, or a whole
    // arm of the decision table is unmeasured.
    for kind in [
        "create", "modify", "delete", "touch", "describe", "mkdir", "rmdir", "conflict", "skip",
    ] {
        assert!(
            kinds.get(kind).copied().unwrap_or(0) > 0,
            "no `{kind}` action anywhere in the corpus — that arm is unmeasured ({kinds:?})"
        );
    }
    eprintln!("OK: plan_sync matched v4 on {n} cases; kinds seen: {kinds:?}");
}
