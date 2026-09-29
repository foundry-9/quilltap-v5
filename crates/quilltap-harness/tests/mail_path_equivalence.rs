//! P4.D234 tier-1 differential: the Post Office's letter-reference parser (v4
//! `lib/post-office/mailbox.ts` `resolveMailPath` + `letterFileName`, v4
//! `39bc98ffc`) vs `quilltap_core::post_office::mailbox::{resolve_mail_path,
//! letter_file_name}`.
//!
//! The corpus lives in the oracle case alone (each NDJSON row carries its own
//! `input`), and both results are compared EXACTLY — `null` against `None`.
//! Beside the diff, the rows that exist to pin an ORDER are asserted by name,
//! because "both sides agree" and "both sides agree AND the trap is in the
//! corpus" are different claims: the URI strip before the slash strip
//! (`/qtap://self/Mail/x` → null, `qtap://self//Mail/x` → `Mail/x.md`), the
//! single `Mail/` strip (`Mail/Mail/x` → null), and the case-insensitive `.md`
//! test (`x.MD` keeps its extension).
//!
//! Generate the oracle output (Node 24, from a v4 tree at or after `39bc98ffc`
//! — the functions do not exist before it, so a baseline run fails outright):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/mail-path.ts \
//!     > /tmp/oracle-mail-path.ndjson
//! Run:
//!   QT_ORACLE_MAIL_PATH=/tmp/oracle-mail-path.ndjson \
//!     cargo test -p quilltap-harness --test mail_path_equivalence

use quilltap_core::post_office::mailbox::{letter_file_name, resolve_mail_path};
use serde_json::Value;

#[test]
fn mail_path_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MAIL_PATH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MAIL_PATH to the oracle NDJSON (see header).");
            return;
        }
    };
    let body = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let rows: Vec<Value> = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    assert!(rows.len() >= 50, "oracle too small ({} rows)", rows.len());

    let mut failures = Vec::new();
    for row in &rows {
        let name = row["name"].as_str().expect("name");
        let input = row["input"].as_str().expect("input");
        let want_resolved = row["resolveMailPath"].as_str().map(str::to_string);
        let got_resolved = resolve_mail_path(input);
        if got_resolved != want_resolved {
            failures.push(format!(
                "{name}: resolveMailPath({input:?}) v4={want_resolved:?} v5={got_resolved:?}"
            ));
        }
        let want_name = row["letterFileName"].as_str().expect("letterFileName");
        let got_name = letter_file_name(input);
        if got_name != want_name {
            failures.push(format!(
                "{name}: letterFileName({input:?}) v4={want_name:?} v5={got_name:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n")
    );

    // The order traps are IN the corpus, with v4's answers.
    let by_name = |n: &str| {
        rows.iter()
            .find(|r| r["name"] == n)
            .unwrap_or_else(|| panic!("corpus row {n} missing"))
    };
    assert_eq!(by_name("uri_after_slash")["resolveMailPath"], Value::Null);
    assert_eq!(by_name("uri_double_slash")["resolveMailPath"], "Mail/x.md");
    assert_eq!(by_name("mail_mail")["resolveMailPath"], Value::Null);
    assert_eq!(by_name("ext_upper")["resolveMailPath"], "Mail/x.MD");
    assert_eq!(
        by_name("ws_nbsp")["resolveMailPath"],
        "Mail/111-from-ariadne.md"
    );
    assert_eq!(by_name("three_dots")["resolveMailPath"], "Mail/....md");
    println!("mail_path: {} rows, 0 divergences", rows.len());
}
