//! Tier-1 differential test #2: ranking blend, provider cosine floor, and the
//! string-valued relative-age label. Proves the harness pattern scales to a
//! second numeric function and to STRING equivalence (exact match, no epsilon).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/ranking-blend.ts \
//!     > /tmp/oracle-ranking.ndjson
//! Run:
//!   QT_ORACLE_RANKING=/tmp/oracle-ranking.ndjson cargo test -p quilltap-harness

use std::collections::HashMap;

use quilltap_core::memory_weighting::{
    compute_ranking_blend, default_min_cosine_for_provider, format_relative_age, MemoryInputs,
};
use quilltap_harness::NOW_MS;
use serde::Deserialize;

const EPS: f64 = 1e-12;
const MS_PER_DAY: f64 = 86_400_000.0;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "blend")]
    Blend { id: String, value: f64 },
    #[serde(rename = "cosine")]
    Cosine { id: String, value: f64 },
    #[serde(rename = "age")]
    Age { id: String, value: String },
}

fn age_mem(days_ago: f64) -> MemoryInputs {
    // Mirror the TS: createdAt = NOW - daysAgo*day.
    MemoryInputs {
        importance: 0.5,
        created_at_ms: NOW_MS - days_ago * MS_PER_DAY,
        ..Default::default()
    }
}

#[test]
fn ranking_blend_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_RANKING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RANKING to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    // Index oracle rows by (kind, id).
    let mut blend = HashMap::new();
    let mut cosine = HashMap::new();
    let mut age = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Blend { id, value } => { blend.insert(id, value); }
            OracleRow::Cosine { id, value } => { cosine.insert(id, value); }
            OracleRow::Age { id, value } => { age.insert(id, value); }
        }
    }

    // --- blend (must match the TS corpus exactly) ---
    for (id, c, r) in [
        ("zero", 0.0, 0.0),
        ("cos-only", 1.0, 0.0),
        ("raw-only", 0.0, 1.0),
        ("mixed", 0.62, 0.4),
        ("high-both", 0.95, 0.88),
    ] {
        let got = compute_ranking_blend(c, r);
        let want = *blend.get(id).unwrap_or_else(|| panic!("oracle missing blend '{id}'"));
        assert!((got - want).abs() <= EPS, "blend '{id}': rust={got} oracle={want}");
    }

    // --- provider cosine floor ---
    for (id, prov) in [
        ("builtin", Some("BUILTIN")),
        ("openai", Some("OPENAI")),
        ("ollama", Some("OLLAMA")),
        ("null", None),
        ("unknown", Some("WAT")),
    ] {
        let got = default_min_cosine_for_provider(prov);
        let want = *cosine.get(id).unwrap_or_else(|| panic!("oracle missing cosine '{id}'"));
        assert!((got - want).abs() <= EPS, "cosine '{id}': rust={got} oracle={want}");
    }

    // --- relative age (STRING equality, no epsilon) ---
    for (id, days_ago) in [
        ("today", 0.5),
        ("yesterday", 1.5),
        ("days-ago", 4.0),
        ("last-week", 10.0),
        ("weeks-ago", 21.0),
        ("last-month", 45.0),
        ("months-ago", 200.0),
        ("one-year", 400.0),
        ("multi-year", 800.0),
        ("future-clamped", -5.0),
    ] {
        let got = format_relative_age(&age_mem(days_ago), NOW_MS);
        let want = age.get(id).unwrap_or_else(|| panic!("oracle missing age '{id}'"));
        assert_eq!(&got, want, "age '{id}': rust={got:?} oracle={want:?}");
    }

    eprintln!(
        "OK: ranking-blend case matched oracle ({} blend, {} cosine, {} age).",
        blend.len(), cosine.len(), age.len()
    );
}
