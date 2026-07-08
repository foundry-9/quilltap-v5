//! Tier-1 differential: the enclave cron slot computation (U4.2).
//!
//! v4 evaluates autonomous-room schedules with croner 10.0.1 in the system
//! local timezone (`new Cron(expr).nextRun(anchor)`, try/catch → null); the
//! port is `quilltap_core::enclave::cron::next_occurrence` with the timezone
//! injected. The oracle replays a committed corpus once per timezone (Node
//! caches TZ at startup, so each zone is a separate run) and every row is
//! diffed exactly: `Some(ms)` is rendered back through the same
//! `iso_from_unix_ms` v4's `.toISOString()` maps to, `None` must line up with
//! the oracle's `null`.
//!
//! Generate the oracle output (croner resolves from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   TZ=America/Chicago npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-cron.ts \
//!     > /tmp/oracle-enclave-cron-chicago.ndjson
//!   TZ=Asia/Kolkata npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-cron.ts \
//!     > /tmp/oracle-enclave-cron-kolkata.ndjson
//! Run:
//!   QT_ORACLE_ENCLAVE_CRON=/tmp/oracle-enclave-cron-chicago.ndjson \
//!   QT_ORACLE_ENCLAVE_CRON_KOLKATA=/tmp/oracle-enclave-cron-kolkata.ndjson \
//!     cargo test -p quilltap-harness --test enclave_cron_equivalence
//!
//! (The no-oracle self-test lives in `quilltap-core`'s `enclave::cron` unit
//! tests — a pinned subset of this corpus verified against real croner.)

use quilltap_core::clock::{iso_from_unix_ms, iso_to_ms};
use quilltap_core::enclave::cron::next_occurrence;
use serde::Deserialize;

/// The croner version the port transcribes; a v4 bump must fail loudly here.
const EXPECTED_CRONER: &str = "10.0.1";

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "probe")]
    Probe { tz: String, croner: String },
    #[serde(rename = "row")]
    Case {
        id: String,
        expr: String,
        #[serde(rename = "anchorIso")]
        anchor_iso: String,
        tz: String,
        #[serde(rename = "nextIso")]
        next_iso: Option<String>,
    },
}

fn run_file(path: &str, expected_tz: &[&str]) -> usize {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());

    // The probe row must come first: it records the timezone Node actually
    // resolved (TZ pinning is unreliable mid-process) and the croner version.
    let probe: Row = serde_json::from_str(lines.next().expect("oracle file is empty"))
        .expect("first oracle row must parse");
    match probe {
        Row::Probe { tz, croner } => {
            // Intl canonicalizes some zones to their legacy tzdb alias
            // (TZ=Asia/Kolkata resolves to "Asia/Calcutta"); jiff follows the
            // same links, so either spelling of the same zone is acceptable.
            assert!(
                expected_tz.contains(&tz.as_str()),
                "oracle {path} was generated under TZ={tz}, expected one of {expected_tz:?} — \
                 regenerate with the TZ env var set (see test header)"
            );
            assert_eq!(
                croner, EXPECTED_CRONER,
                "v4's installed croner is {croner}, but the port transcribes {EXPECTED_CRONER} — \
                 re-audit the port against the new croner before bumping this pin"
            );
        }
        Row::Case { .. } => panic!("oracle {path}: first row must be the probe"),
    }

    let mut count = 0usize;
    for line in lines {
        let row: Row =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("bad row: {e}\n{line}"));
        let Row::Case {
            id,
            expr,
            anchor_iso,
            tz,
            next_iso,
        } = row
        else {
            panic!("unexpected second probe row in {path}");
        };
        let anchor_ms = iso_to_ms(&anchor_iso)
            .unwrap_or_else(|| panic!("case '{id}': unparseable anchor {anchor_iso}"));
        let got = next_occurrence(&expr, anchor_ms, &tz).map(iso_from_unix_ms);
        assert_eq!(
            got, next_iso,
            "case '{id}' (expr {expr:?}, anchor {anchor_iso}, tz {tz}): rust={got:?} oracle={next_iso:?}"
        );
        count += 1;
    }
    assert!(
        count >= 50,
        "oracle file {path} looks truncated: only {count} case rows"
    );
    count
}

#[test]
fn enclave_cron_matches_oracle_chicago() {
    let path = match std::env::var("QT_ORACLE_ENCLAVE_CRON") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ENCLAVE_CRON to the America/Chicago oracle NDJSON.");
            return;
        }
    };
    let n = run_file(&path, &["America/Chicago"]);
    eprintln!("OK: enclave-cron matched oracle (America/Chicago, {n} rows).");
}

#[test]
fn enclave_cron_matches_oracle_kolkata() {
    let path = match std::env::var("QT_ORACLE_ENCLAVE_CRON_KOLKATA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_ENCLAVE_CRON_KOLKATA to the Asia/Kolkata oracle NDJSON."
            );
            return;
        }
    };
    let n = run_file(&path, &["Asia/Kolkata", "Asia/Calcutta"]);
    eprintln!("OK: enclave-cron matched oracle (Asia/Kolkata, {n} rows).");
}
