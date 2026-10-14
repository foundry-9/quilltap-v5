//! Tier-1 differential: the read-side day-reference resolver at the pin
//! (`83334b9a5`) — `quilltap_core::memory_time::resolve_day_reference` vs v4
//! `lib/memory/time/clock.ts`'s `resolveDayReference(text, now, timeZone)`
//! (P4.D289 re-aim; first ported as P4.d26 against `505dcb1f`'s
//! `lib/memory/day-references.ts`, deleted by v4 `e0ac7493a` — bug 212).
//!
//! Drives v5's resolver over the committed corpus
//! (`harness/oracle/fixtures/day-references.json`) and compares, field for
//! field, against the NDJSON v4's REAL `resolveDayReference` emitted from the
//! SAME corpus: the window bytes (`from`/`to` as `toISOString()` renders them —
//! every `to` one millisecond before the next boundary), `pastPointing`, and
//! the normalized `matched` phrase.
//!
//! **ZONES ARE EXPLICIT, and two of them are the point.** At the pin the
//! resolver takes the memory-calendar zone as an ARGUMENT, so the case walks
//! the corpus once per zone of an explicit list (`UTC`, `America/Chicago`,
//! `Pacific/Chatham` — one block each, in order), passing it on both sides; the
//! process zone plays no part (the P4.d26 version ran two `TZ` legs because
//! v4 then read the process zone). The local-vs-UTC crux is still the guard:
//! the assertions below REQUIRE the UTC and DST-carrying Chicago blocks and
//! require the zone blocks to actually disagree somewhere — so a regenerate
//! that silently loses a zone fails instead of passing on half the evidence.
//!
//! Red-first (P4.D289, recorded in the lane record): the re-aimed case against
//! the UNCHANGED P4.d26 test reddened 84 of 165 rows — 81 (27 cases × 3 zones)
//! where only `to` moved 1 ms earlier, and `casefold-kelvin-before-word-char`
//! × 3 (the old Unicode `(?i)` folded KELVIN SIGN to `k`; v4's scan has no
//! `u` flag — LR-9).
//!
//! Run from inside a v4 checkout (or the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/day-references.ts" > /tmp/oracle-day-references.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_DAY_REFERENCES=/tmp/oracle-day-references.ndjson \
//!     cargo test -p quilltap-harness --test day_references_equivalence -- --nocapture

use std::collections::BTreeSet;

use quilltap_core::memory_time::resolve_day_reference;
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

/// Zones the oracle's explicit list MUST cover: UTC (where local == UTC) and a
/// DST-carrying non-UTC zone (where they differ, twice a year by an hour).
const REQUIRED_ZONES: [&str; 2] = ["UTC", "America/Chicago"];

#[test]
fn day_references_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DAY_REFERENCES") else {
        eprintln!("SKIP: set QT_ORACLE_DAY_REFERENCES to the oracle NDJSON (see test header).");
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
            "oracle is missing the {required} zone block — regenerate (see the header); \
             a UTC-only run cannot distinguish local-calendar math from UTC math"
        );
    }
    assert_eq!(
        oracle_rows.len(),
        spec.cases.len() * zones.len(),
        "oracle row count != corpus cases x zones — a truncated or stale NDJSON"
    );

    let mut resolved = 0usize;
    let mut null_resolutions = 0usize;
    // id -> the resolution each zone produced (for the divergence check).
    let mut by_case: std::collections::BTreeMap<String, Vec<Value>> = Default::default();

    for (i, oracle) in oracle_rows.iter().enumerate() {
        // The oracle walks the corpus in order, once per zone.
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

    // The zone blocks must actually disagree somewhere; if they agreed everywhere,
    // the corpus would be blind to the very bias the drift fixes (every `now` an
    // instant whose UTC and local days happen to coincide).
    let cases = spec.cases.len();
    let differing_legs = by_case
        .values()
        .filter(|legs| legs.iter().any(|v| v != &legs[0]))
        .count();
    assert!(
        differing_legs >= 10,
        "only {differing_legs} corpus cases differ between the zones — the \
         local-vs-UTC crux is under-covered"
    );
    assert!(resolved > 0 && null_resolutions > 0, "corpus lost an arm");

    println!(
        "day_references_equivalence: {} rows green ({} zones x {cases} cases; \
         {resolved} resolved, {null_resolutions} null, {differing_legs} zone-sensitive)",
        oracle_rows.len(),
        zones.len()
    );
}
