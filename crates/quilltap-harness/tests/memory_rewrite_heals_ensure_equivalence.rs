//! P4.D288 (C1-2 slots 4 / 5, R-E) tier-2 differential: `db::memory_rewrite_heals`
//! against v4's REAL `recompute-reinforced-importance-v1` and
//! `rezone-legacy-memory-occurred-at-v1`, each through v4's narrowed REAL
//! `MigrationRunner` so the STAMP — `(id, itemsAffected, message)` — is
//! byte-compared with v4's row.
//!
//! - **recompute:** a stored value NULL / within the 1e-9 band / beyond it;
//!   count NULL / 0 / 1 / 2 / 1e6; importance NULL; the ledger present
//!   (`AlreadyCompleted`, nothing touched); a 0-row stamp; no table
//!   (`NotNeeded`, nothing stamped).
//! - **rezone:** the zone sources (the UTC fallback; the FIRST valid
//!   `chat_settings.timezone` in rowid order past a NULL and an invalid one;
//!   east of Greenwich); `…T00:00:00Z` vs `.000Z` in UTC; an offset suffix; a
//!   noon; a pre-1970 midnight; DST-gap midnights in `America/Santiago` and
//!   `America/Havana` (v4's two-pass algorithm, P4.D289's
//!   `zoned_time_to_utc_ms`); an unparseable candidate; the ledger present; no
//!   table. The recipe pins `TZ=UTC QUILLTAP_TIMEZONE=UTC`; this side passes
//!   `UTC` as the host fallback.
//!
//! Regenerate + run (one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5} ; N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   for F in recompute-importance rezone-occurred-at; do
//!     rm -rf /tmp/qt-p4d288-ensures/$F /tmp/oracle-p4d288-ensure-$F.ndjson
//!     TZ=UTC QUILLTAP_TIMEZONE=UTC QT_P4D288_ENSURE_FAMILY=$F \
//!       QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!       $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts > /tmp/oracle-p4d288-ensure-$F.ndjson
//!   done
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_RECOMPUTE_IMPORTANCE=/tmp/oracle-p4d288-ensure-recompute-importance.ndjson \
//!   QT_ORACLE_P4D288_ENSURE_REZONE_OCCURRED_AT=/tmp/oracle-p4d288-ensure-rezone-occurred-at.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test memory_rewrite_heals_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};
use quilltap_core::db::memory_rewrite_heals::{
    recompute_reinforced_importance, rezone_legacy_memory_occurred_at, LedgeredHealOutcome,
    RECOMPUTE_MIGRATION_ID, REZONE_MIGRATION_ID,
};

const NOW: &str = "2026-10-14T00:00:00.000Z";

#[test]
fn recompute_reinforced_importance_matches_v4() {
    run_family(
        "recompute-importance",
        &["memories"],
        &["counts", "ledgered", "zero-rows", "no-table"],
        &[],
        |c| match recompute_reinforced_importance(c, NOW).expect("heal") {
            LedgeredHealOutcome::Ran { .. } => vec![RECOMPUTE_MIGRATION_ID],
            _ => vec![],
        },
    );
}

#[test]
fn rezone_legacy_memory_occurred_at_matches_v4() {
    run_family(
        "rezone-occurred-at",
        &["memories", "chat_settings"],
        &[
            "utc-fallback",
            "chicago-after-an-invalid-zone",
            "santiago-dst-gap",
            "havana-dst-gap",
            "tokyo-east",
            "ledgered",
            "no-table",
        ],
        &[],
        |c| match rezone_legacy_memory_occurred_at(c, "UTC", NOW).expect("heal") {
            LedgeredHealOutcome::Ran { .. } => vec![REZONE_MIGRATION_ID],
            _ => vec![],
        },
    );
}
