//! Tier-1 differential: the deterministic day-reference resolver
//! (`quilltap_core::day_references` vs v4 `lib/memory/day-references.ts`,
//! v4 `505dcb1f` — P4.d26).
//!
//! Drives v5's `resolve_day_reference` over the committed corpus
//! (`harness/oracle/fixtures/day-references.json`) and compares, field for
//! field, against the NDJSON v4's REAL `resolveDayReference` emitted from the
//! SAME corpus: the window bytes (`from`/`to` as `toISOString()` renders them),
//! `pastPointing`, and the normalized `matched` phrase.
//!
//! **TWO ZONE LEGS, and they are the point.** The module resolves calendar
//! boundaries in the SERVER-LOCAL zone, and the live bug it fixes is exactly a
//! UTC-vs-local difference — under `TZ=UTC` local *is* UTC, so a single-zone
//! differential cannot tell the fix from the bug. The oracle therefore runs the
//! whole corpus twice (`TZ=UTC` and `TZ=America/Chicago`, the diagnosed
//! incident's zone, which carries both DST transitions), each row recording its
//! zone; v5 takes the zone as an explicit IANA parameter (core never reads the
//! process zone) and is fed the row's own. The assertions below REQUIRE both
//! legs, require the non-UTC leg to be DST-carrying, and require the two legs
//! to actually disagree somewhere — so a regenerate that silently loses a leg
//! fails instead of passing on half the evidence.
//!
//! Generate the oracle output (both legs, one go — the second APPENDS):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   cd ~/source/quilltap-server           # or a detached worktree pinned at
//!                                         # the baseline when v4 is dirty
//!   TZ=UTC $N/node --import tsx $W/harness/oracle/cases/day-references.ts \
//!     > /tmp/oracle-day-references.ndjson
//!   TZ=America/Chicago $N/node --import tsx $W/harness/oracle/cases/day-references.ts \
//!     >> /tmp/oracle-day-references.ndjson
//! Run:
//!   QT_ORACLE_DAY_REFERENCES=/tmp/oracle-day-references.ndjson \
//!     cargo test -p quilltap-harness --test day_references_equivalence -- --nocapture

use std::collections::BTreeSet;

use quilltap_core::day_references::resolve_day_reference;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct CaseSpec {
    id: String,
    text: String,
    /// `null` is v4's Invalid Date arm.
    now: Option<String>,
}

#[derive(Deserialize)]
struct Spec {
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
struct OracleRow {
    id: String,
    tz: String,
    resolution: Value,
}

/// Zones the oracle legs MUST cover: UTC (where local == UTC) and a
/// DST-carrying non-UTC zone (where they differ, twice a year by an hour).
const REQUIRED_ZONES: [&str; 2] = ["UTC", "America/Chicago"];

#[test]
fn day_references_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DAY_REFERENCES") else {
        eprintln!("QT_ORACLE_DAY_REFERENCES not set — skipping day-references differential");
        return;
    };
    let oracle_raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let oracle_rows: Vec<OracleRow> = oracle_raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/day-references.json"
        ))
        .expect("read corpus"),
    )
    .expect("parse corpus");

    // ---- coverage, asserted as SHAPE (never a hand-written count) ----
    let zones: BTreeSet<&str> = oracle_rows.iter().map(|r| r.tz.as_str()).collect();
    for required in REQUIRED_ZONES {
        assert!(
            zones.contains(required),
            "oracle is missing the {required} leg — regenerate BOTH legs (see the header); \
             a UTC-only run cannot distinguish local-calendar math from UTC math"
        );
    }
    assert_eq!(
        oracle_rows.len(),
        spec.cases.len() * zones.len(),
        "oracle row count != corpus cases x zone legs — a truncated or stale NDJSON"
    );

    let mut resolved = 0usize;
    let mut null_resolutions = 0usize;
    // id -> the resolution each zone leg produced (for the divergence check).
    let mut by_case: std::collections::BTreeMap<String, Vec<Value>> = Default::default();

    for (i, oracle) in oracle_rows.iter().enumerate() {
        // The oracle walks the corpus in order, once per leg.
        let case = &spec.cases[i % spec.cases.len()];
        assert_eq!(case.id, oracle.id, "case order mismatch at row {i}");
        let label = format!("{} @ {}", case.id, oracle.tz);

        // v4 builds `new Date(Date.parse(now))`, so an absent `now` is NaN.
        let now_ms = match &case.now {
            Some(iso) => quilltap_core::episodic::event_time_ms(Some(iso))
                .unwrap_or_else(|| panic!("{label}: corpus `now` is unparsable")),
            None => f64::NAN,
        };

        let got = match resolve_day_reference(&case.text, now_ms, &oracle.tz) {
            Some(r) => json!({
                "timeRange": { "from": r.time_range.from, "to": r.time_range.to },
                "pastPointing": r.past_pointing,
                "matched": r.matched,
            }),
            None => Value::Null,
        };
        assert_eq!(got, oracle.resolution, "{label}: resolution diverges");

        if oracle.resolution.is_null() {
            null_resolutions += 1;
        } else {
            resolved += 1;
        }
        by_case
            .entry(case.id.clone())
            .or_default()
            .push(oracle.resolution.clone());
    }

    // The zone legs must actually disagree somewhere; if they agreed everywhere,
    // the corpus would be blind to the very bias the drift fixes (every `now` an
    // instant whose UTC and local days happen to coincide).
    let cases = spec.cases.len();
    let differing_legs = by_case
        .values()
        .filter(|legs| legs.iter().any(|v| v != &legs[0]))
        .count();
    assert!(
        differing_legs >= 10,
        "only {differing_legs} corpus cases differ between the zone legs — the \
         local-vs-UTC crux is under-covered"
    );
    assert!(resolved > 0 && null_resolutions > 0, "corpus lost an arm");

    println!(
        "day_references_equivalence: {} rows green ({} zone legs x {cases} cases; \
         {resolved} resolved, {null_resolutions} null, {differing_legs} zone-sensitive)",
        oracle_rows.len(),
        zones.len()
    );
}
