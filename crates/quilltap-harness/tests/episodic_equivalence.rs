//! Tier-1 differential: the episodic-spine pure helpers vs v4's REAL
//! `lib/memory/episodic.ts` (P4.d12 unit 4, v4 baseline `8bf3cb5f`).
//!
//! The corpus here mirrors `harness/oracle/cases/episodic.ts` case-for-case
//! (same ids, same inputs); the oracle drives v4's real module and this test
//! drives `quilltap_core::episodic`, comparing exactly — strings byte-equal,
//! `eventReferenceTimeMs` numerically exact (integer milliseconds), `null`s
//! exact.
//!
//! Generate the oracle (Node 24, from the v4 checkout — **TZ=UTC is
//! mandatory**: the absolute-ISO branch parses zone-less datetimes in the
//! process zone; the Rust port implements UTC semantics):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx <worktree>/harness/oracle/cases/episodic.ts > /tmp/oracle-episodic.ndjson
//! Run:
//!   QT_ORACLE_EPISODIC=/tmp/oracle-episodic.ndjson \
//!     cargo test -p quilltap-harness --test episodic_equivalence

use std::collections::HashMap;

use quilltap_core::episodic::{
    build_memory_anchor_line, build_memory_embedding_text, event_reference_time_ms,
    resolve_when_phrase, EpisodicAnchorView,
};
use serde_json::{json, Value};

/// The fixed write/reinforce clock for the eventReferenceTimeMs cases.
const WRITE_CLOCK_MS: f64 = 1_783_000_000_000.0;

/// Anchor A: Tuesday 2026-07-14T09:30:00.000Z. Anchor B: Monday 2026-01-05.
const A: &str = "2026-07-14T09:30:00.000Z";
const B: &str = "2026-01-05T00:00:00.000Z";

fn view(
    occurred_at: Option<&str>,
    narrative_time: Option<&str>,
    entities: &[&str],
) -> EpisodicAnchorView {
    EpisodicAnchorView {
        occurred_at: occurred_at.map(str::to_string),
        narrative_time: narrative_time.map(str::to_string),
        entities: entities.iter().map(|s| s.to_string()).collect(),
    }
}

fn anchor_cases() -> Vec<(&'static str, EpisodicAnchorView)> {
    vec![
        ("anchor-empty", view(None, None, &[])),
        ("anchor-all-null", view(None, None, &[])),
        ("anchor-when-date", view(Some("2026-07-14"), None, &[])),
        (
            "anchor-when-datetime",
            view(Some("2026-07-14T09:30:00.000Z"), None, &[]),
        ),
        (
            "anchor-when-padded",
            view(Some("  2026-07-14T09:30:00.000Z  "), None, &[]),
        ),
        ("anchor-when-invalid", view(Some("not-a-date-x"), None, &[])),
        ("anchor-when-short", view(Some("2026-7-4"), None, &[])),
        ("anchor-when-empty", view(Some("   "), None, &[])),
        (
            "anchor-story",
            view(None, Some("the third night at sea"), &[]),
        ),
        (
            "anchor-story-padded",
            view(None, Some("  the third night at sea  "), &[]),
        ),
        ("anchor-story-empty", view(None, Some("   "), &[])),
        (
            "anchor-entities-one",
            view(None, None, &["Lighthouse Point"]),
        ),
        (
            "anchor-entities-cap",
            view(None, None, &["A", "B", "C", "D", "E", "F"]),
        ),
        (
            "anchor-entities-trim-filter",
            view(None, None, &["  Paris  ", "", "   ", "Bob"]),
        ),
        (
            "anchor-all",
            view(
                Some("2026-07-14T21:00:00.000Z"),
                Some("the third night at sea"),
                &["Lighthouse Point", "Marta"],
            ),
        ),
        (
            "anchor-story-and-place",
            view(None, Some("midwinter"), &["The Salon"]),
        ),
    ]
}

struct EmbedCase {
    id: &'static str,
    summary: &'static str,
    content: &'static str,
    anchors: Option<EpisodicAnchorView>,
}

fn embed_cases() -> Vec<EmbedCase> {
    vec![
        EmbedCase {
            id: "embed-no-anchors-arg",
            summary: "S",
            content: "C",
            anchors: None,
        },
        EmbedCase {
            id: "embed-null-anchors",
            summary: "S",
            content: "C",
            anchors: None,
        },
        EmbedCase {
            id: "embed-empty-anchors",
            summary: "Sum",
            content: "Body text",
            anchors: Some(view(None, None, &[])),
        },
        EmbedCase {
            id: "embed-null-fields",
            summary: "Sum",
            content: "Body text",
            anchors: Some(view(None, None, &[])),
        },
        EmbedCase {
            id: "embed-with-anchor",
            summary: "Cafe meeting",
            content: "Alice met Bob at the cafe",
            anchors: Some(view(Some("2026-07-14T09:30:00.000Z"), None, &["Paris"])),
        },
        EmbedCase {
            id: "embed-empty-strings",
            summary: "",
            content: "",
            anchors: Some(view(None, Some("dawn"), &[])),
        },
    ]
}

fn when_cases() -> Vec<(&'static str, Option<&'static str>, &'static str)> {
    vec![
        ("when-null", None, A),
        ("when-undefined", None, A),
        ("when-empty", Some(""), A),
        ("when-blank", Some("   "), A),
        ("when-bad-anchor", Some("yesterday"), "not-an-anchor"),
        ("when-iso-date", Some("2026-07-10"), A),
        ("when-iso-datetime-z", Some("2026-07-10T18:45:00.000Z"), A),
        ("when-iso-datetime-lower", Some("2026-07-10t18:45:00z"), A),
        (
            "when-iso-datetime-offset",
            Some("2026-07-10T18:45:00+02:00"),
            A,
        ),
        (
            "when-iso-datetime-offset-nocolon",
            Some("2026-07-10T18:45:00+0200"),
            A,
        ),
        ("when-iso-datetime-nozone", Some("2026-07-10T18:45:00"), A),
        ("when-iso-bad-month", Some("2026-13-05"), A),
        ("when-iso-bad-day", Some("2026-02-31"), A),
        ("when-month-day-year", Some("July 10, 2026"), A),
        ("when-month-day", Some("July 10"), A),
        ("when-month-day-future-rolls-back", Some("December 25"), A),
        ("when-month-day-on", Some("on July 10"), A),
        ("when-month-day-ordinal", Some("July 4th"), A),
        ("when-day-month", Some("10 July"), A),
        ("when-day-month-year", Some("10 July 2025"), A),
        ("when-day-of-month", Some("3rd of March"), A),
        ("when-month-day-overflow", Some("February 31"), A),
        ("when-month-unknown", Some("Smarch 5"), A),
        ("when-month-day-zero", Some("July 0"), A),
        ("when-today", Some("today"), A),
        ("when-tonight", Some("tonight"), A),
        ("when-this-morning", Some("this morning"), A),
        ("when-earlier", Some("earlier"), A),
        ("when-earlier-today", Some("earlier today"), A),
        ("when-just-now", Some("just now"), A),
        ("when-now", Some("now"), A),
        ("when-mixed-case", Some("ToDaY"), A),
        ("when-yesterday", Some("yesterday"), A),
        ("when-last-night", Some("last night"), A),
        ("when-3-days-ago", Some("3 days ago"), A),
        ("when-1-day-ago", Some("1 day ago"), A),
        ("when-a-week-ago", Some("a week ago"), A),
        ("when-two-months-ago", Some("two months ago"), A),
        ("when-ten-years-back", Some("ten years back"), A),
        ("when-about-2-weeks-ago", Some("about 2 weeks ago"), A),
        (
            "when-around-five-days-earlier",
            Some("around five days earlier"),
            A,
        ),
        ("when-some-days-before", Some("some three days before"), A),
        ("when-couple-days-ago", Some("couple days ago"), A),
        ("when-a-couple-of-days-ago", Some("a couple of days ago"), A),
        ("when-a-few-weeks-back", Some("a few weeks back"), A),
        ("when-zero-days-ago", Some("0 days ago"), A),
        ("when-10000-days-ago", Some("10000 days ago"), A),
        ("when-9999-days-ago", Some("9999 days ago"), A),
        ("when-unknown-word-days-ago", Some("several days ago"), A),
        ("when-last-week", Some("last week"), A),
        ("when-last-month", Some("last month"), A),
        ("when-last-year", Some("last year"), A),
        ("when-other-day", Some("the other day"), A),
        ("when-last-friday", Some("last friday"), A),
        ("when-last-tuesday", Some("last tuesday"), A),
        ("when-last-monday-from-monday", Some("last monday"), B),
        ("when-last-sunday-from-monday", Some("last sunday"), B),
        ("when-last-spring", Some("last spring"), A),
        ("when-last-autumn", Some("last autumn"), A),
        ("when-tomorrow", Some("tomorrow"), A),
        ("when-next-week", Some("next week"), A),
        ("when-gibberish", Some("when the moon was full"), A),
        ("when-dec-from-jan", Some("December 31"), B),
        ("when-jan-from-jan", Some("January 5"), B),
        ("when-jan-6-from-jan-5", Some("January 6"), B),
    ]
}

fn ref_cases() -> Vec<(&'static str, Option<&'static str>)> {
    vec![
        ("ref-null", None),
        ("ref-undefined", None),
        ("ref-empty", Some("")),
        ("ref-garbage", Some("not-a-date-at-all")),
        ("ref-iso-datetime", Some("2026-07-14T09:30:00.000Z")),
        ("ref-iso-date", Some("2026-07-14")),
        ("ref-iso-seconds", Some("2020-01-01T00:00:00Z")),
    ]
}

#[test]
fn episodic_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EPISODIC") else {
        eprintln!("SKIP: set QT_ORACLE_EPISODIC to the oracle NDJSON (see header).");
        return;
    };
    let raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let mut oracle: HashMap<(String, String), Value> = HashMap::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line parses");
        let kind = v["kind"].as_str().expect("kind").to_string();
        let id = v["id"].as_str().expect("id").to_string();
        oracle.insert((kind, id), v["result"].clone());
    }

    let mut checked = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    let mut check = |kind: &str, id: &str, got: Value| {
        let key = (kind.to_string(), id.to_string());
        match oracle.get(&key) {
            None => mismatches.push(format!("{kind}/{id}: MISSING from oracle")),
            Some(want) => {
                if *want != got {
                    mismatches.push(format!("{kind}/{id}: oracle={want} port={got}"));
                }
            }
        }
        checked += 1;
    };

    for (id, v) in anchor_cases() {
        check("anchorLine", id, json!(build_memory_anchor_line(&v)));
    }
    for c in embed_cases() {
        check(
            "embeddingText",
            c.id,
            json!(build_memory_embedding_text(
                c.summary,
                c.content,
                c.anchors.as_ref()
            )),
        );
    }
    for (id, phrase, anchor) in when_cases() {
        check("whenPhrase", id, json!(resolve_when_phrase(phrase, anchor)));
    }
    for (id, occurred_at) in ref_cases() {
        let ms = event_reference_time_ms(occurred_at, WRITE_CLOCK_MS);
        // The oracle emits whole milliseconds as JSON integers; serde_json
        // integers and floats never compare equal, so render the JS way.
        let got = if ms.fract() == 0.0 && ms.abs() < 9.0e15 {
            json!(ms as i64)
        } else {
            json!(ms)
        };
        check("eventReferenceTimeMs", id, got);
    }

    assert!(
        mismatches.is_empty(),
        "episodic mismatches ({} of {checked}):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    assert_eq!(checked, oracle.len(), "corpus size differs from oracle");
    println!("OK: episodic matched oracle ({checked} cases).");
}
