//! Tier-1 EXACT differential — every v5 display-date helper that renders in
//! the HOST's zone, against v4's real helpers run under TWO zones (P4.119,
//! dogfood #121, ruled (a) 2026-09-29).
//!
//! v4 formats the Post Office's letter dates and reply prefaces, the Suparṇā
//! whisper and mail context, the web-search `Published:` dates and the
//! Almanack's dates with zone-less `toLocale*` calls — the Node process's
//! zone. The harness used to run v4 under `TZ=UTC` only, so a port that always
//! rendered UTC was indistinguishable from one that renders the host zone; this
//! family is the first that can see the difference. The oracle
//! (`harness/oracle/cases/host-zone-dates.ts`, tsx — v4's jest config forces
//! `TZ=UTC`) records the zone it ran under in its first row, and v5 is fed that
//! zone BY ARGUMENT through the `_in_zone` / explicit-zone entry points: nothing
//! here reads this machine's zone, so both arms are green on any host.
//!
//! The corpus (`harness/oracle/fixtures/host-zone-dates.json`) carries the walk's
//! D2 instant (19:40Z → `02:40 PM` in Chicago), a winter instant at the same UTC
//! hour (per-instant offsets, not one offset at `now`), both 2026 US DST
//! transitions, instants that straddle local-but-not-UTC midnight, and — for
//! the Almanack — a SQLite space-form stamp and a zone-less `T` stamp, which
//! V8 reads as LOCAL (Mandate 3: their wall digits print back unchanged).
//!
//! Generate (Node 24, from the v4 checkout or a pinned worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/node --import tsx $V5W/harness/oracle/cases/host-zone-dates.ts \
//!     > /tmp/oracle-host-zone-dates.ndjson
//!   TZ=America/Chicago $N/node --import tsx $V5W/harness/oracle/cases/host-zone-dates.ts \
//!     > /tmp/oracle-host-zone-dates-chicago.ndjson
//!   TZ=XST-9 $N/node --import tsx $V5W/harness/oracle/cases/host-zone-dates.ts \
//!     > /tmp/oracle-host-zone-dates-posix.ndjson
//! Run:
//!   QT_ORACLE_HOST_ZONE_DATES=/tmp/oracle-host-zone-dates.ndjson \
//!   QT_ORACLE_HOST_ZONE_DATES_CHICAGO=/tmp/oracle-host-zone-dates-chicago.ndjson \
//!   QT_ORACLE_HOST_ZONE_DATES_POSIX=/tmp/oracle-host-zone-dates-posix.ndjson \
//!     cargo test -p quilltap-harness --test host_zone_dates_equivalence
//!
//! **The POSIX arm (P4.140):** `TZ=XST-9` is a POSIX rule with no IANA name and
//! no daylight saving — the one rule shape v4's ICU honours (a rule WITH DST is
//! ignored by ICU, which renders the host's `/etc/localtime` instead, so that
//! arm would not be machine-independent; v5 honours every rule, the ruled
//! shape). ICU resolves no zone name there, so the oracle's `tz` is absent and
//! the arm checks the recorded `envTz`; v5 builds the zone with
//! `TimeZone::posix` — the VALUE the host reads from such a `TZ`.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use quilltap_core::almanack::{render_almanack_markdown, AlmanackReportData};
use quilltap_core::format_time::{format_date_time, MonthStyle};
use quilltap_core::host_zone::TimeZone;
use quilltap_core::post_office::instructions::{format_letter_date, format_letter_heading};
use quilltap_core::post_office::mailbox::{build_reply_preface, DeliveredLetterSummary};
use quilltap_core::services::suparna_mail::build_suparna_mail_llm_context;
use quilltap_core::services::suparna_notifications::build_suparna_mail_whisper_in_zone;
use quilltap_core::tools::web_search::{format_web_search_results_in_zone, WebSearchResult};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LetterSeed {
    path: String,
    from: String,
    sent_at: String,
    body: String,
    alerted: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    instants: Vec<String>,
    letters: Vec<LetterSeed>,
    search_results: Vec<WebSearchResult>,
}

fn corpus() -> Corpus {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/host-zone-dates.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("the corpus is readable"))
        .expect("the corpus parses")
}

/// The oracle rows, keyed `(op, label)`.
fn oracle(var: &str) -> Option<HashMap<(String, String), Value>> {
    let path = match std::env::var(var) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set {var} to the host-zone-dates oracle NDJSON (see header).");
            return None;
        }
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    assert!(
        !raw.trim().is_empty(),
        "{var}={path} is EMPTY — the generation step failed (the empty-file trap)"
    );
    let mut map = HashMap::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("each oracle line is JSON");
        let key = (
            row["op"].as_str().expect("op").to_string(),
            row["label"].as_str().expect("label").to_string(),
        );
        assert!(
            map.insert(key.clone(), row).is_none(),
            "duplicate row {key:?}"
        );
    }
    Some(map)
}

/// Which run of the oracle an arm reads: the IANA zone ICU resolved (`tz`) and
/// the `TZ` the process ran under (`envTz`, recorded since P4.140 — the only
/// identifier a POSIX-rule run has).
struct Arm<'a> {
    tz: Option<&'a str>,
    env_tz: &'a str,
    zone: TimeZone,
}

fn run(oracle: &HashMap<(String, String), Value>, arm: Arm<'_>) -> usize {
    let corpus = corpus();
    let mut checked = 0usize;

    // The zone the oracle ran under; a file generated under the wrong `TZ`
    // fails here rather than in a date diff.
    let row = oracle
        .get(&("hostZone".to_string(), "tz".to_string()))
        .expect("the oracle records its zone");
    assert_eq!(
        row["tz"].as_str(),
        arm.tz,
        "oracle generated under a different zone: {row}"
    );
    assert_eq!(
        row["envTz"].as_str(),
        Some(arm.env_tz),
        "oracle generated under a different TZ: {row}"
    );
    checked += 1;
    let zone = arm.zone;
    let expected_tz = arm.env_tz;

    let mut check = |op: &str, label: &str, got: &str| {
        let want = oracle
            .get(&(op.to_string(), label.to_string()))
            .unwrap_or_else(|| panic!("the oracle has no ({op}, {label}) row — regenerate"));
        assert_eq!(
            Some(got),
            want["result"].as_str(),
            "{op}/{label} diverges under {expected_tz}"
        );
        checked += 1;
    };

    for s in &corpus.instants {
        check(
            "formatDateTime",
            s,
            &format_date_time(Some(s), MonthStyle::Long, &zone),
        );
        check("formatLetterDate", s, &format_letter_date(s, &zone));
        check(
            "buildReplyPreface",
            s,
            &build_reply_preface("Line one\n\nLine two", s, &zone),
        );
    }
    check("formatLetterDate", "", &format_letter_date("", &zone));
    check(
        "buildReplyPreface",
        "",
        &build_reply_preface("Hi.", "", &zone),
    );

    let letters: Vec<DeliveredLetterSummary> = corpus
        .letters
        .iter()
        .map(|l| DeliveredLetterSummary {
            path: l.path.clone(),
            from: l.from.clone(),
            sent_at: l.sent_at.clone(),
            body: l.body.clone(),
            alerted: l.alerted,
            in_reply_to: None,
        })
        .collect();
    for (i, letter) in letters.iter().enumerate() {
        check(
            "formatLetterHeading",
            &letter.path,
            &format_letter_heading(letter, i + 1, &zone),
        );
    }
    check(
        "suparnaWhisper",
        "all",
        &build_suparna_mail_whisper_in_zone(&letters, &zone),
    );
    check(
        "suparnaLlmContext",
        "all",
        &build_suparna_mail_llm_context(&letters, &zone),
    );

    check(
        "formatWebSearchResults",
        "all",
        &format_web_search_results_in_zone(&corpus.search_results, &zone),
    );

    // The Almanack renders v4's OWN data object (planted dates included), so
    // the model round-trip is part of the proof.
    let row = oracle
        .get(&("almanackRender".to_string(), "planted".to_string()))
        .expect("the almanack row");
    let data: AlmanackReportData =
        serde_json::from_value(row["data"].clone()).expect("v4's data round-trips the model");
    check(
        "almanackRender",
        "planted",
        &render_almanack_markdown(&data, &zone),
    );

    assert_eq!(
        checked,
        oracle.len(),
        "every oracle row must be consumed — a corpus row the family forgot is a silent gap"
    );
    checked
}

#[test]
fn host_zone_dates_match_oracle_utc() {
    let Some(oracle) = oracle("QT_ORACLE_HOST_ZONE_DATES") else {
        return;
    };
    let n = run(
        &oracle,
        Arm {
            tz: Some("UTC"),
            env_tz: "UTC",
            zone: TimeZone::UTC,
        },
    );
    println!("host_zone_dates (UTC): {n} rows OK");
}

#[test]
fn host_zone_dates_match_oracle_second_zone() {
    let Some(oracle) = oracle("QT_ORACLE_HOST_ZONE_DATES_CHICAGO") else {
        return;
    };
    let n = run(
        &oracle,
        Arm {
            tz: Some("America/Chicago"),
            env_tz: "America/Chicago",
            zone: TimeZone::get("America/Chicago").expect("a zone jiff knows"),
        },
    );
    println!("host_zone_dates (America/Chicago): {n} rows OK");
}

/// P4.140 (Tier 2 item 14): a POSIX `TZ` rule with no IANA name — the host
/// zone VALUE Option V threads — renders exactly as v4 does where ICU honours
/// the rule.
#[test]
fn host_zone_dates_match_oracle_posix_rule() {
    let Some(oracle) = oracle("QT_ORACLE_HOST_ZONE_DATES_POSIX") else {
        return;
    };
    let zone = TimeZone::posix("XST-9").expect("jiff parses the rule");
    assert_eq!(zone.iana_name(), None, "a POSIX rule has no IANA name");
    let n = run(
        &oracle,
        Arm {
            tz: None,
            env_tz: "XST-9",
            zone,
        },
    );
    println!("host_zone_dates (POSIX XST-9): {n} rows OK");
}
