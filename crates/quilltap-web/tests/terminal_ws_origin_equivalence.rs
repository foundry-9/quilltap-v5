//! P4.D124 tier-1 differential: the WebSocket upgrade ORIGIN gate, diffed
//! against v4's REAL `authenticateUpgrade` (`lib/realtime/upgrade-auth.ts`,
//! `f3892158d`).
//!
//! The oracle mocks away v4's other two checks (live session, not locked) —
//! neither ports; see `upgrade_auth`'s module header — so what is compared is
//! exactly the arm v5 implements, including the refusal SENTENCES. Those
//! sentences are internal to v4 (the socket only ever closes with the fixed
//! `Unauthorized`), but comparing them is free and catches an arm that refuses
//! for the wrong reason.
//!
//! ⚠ Note what `unparseable_origin_scheme_only` and its neighbours pin: v4
//! reaches `new URL(origin).host` and gets an empty host or a throw, and v5's
//! `url` crate must land on the same verdict. This is why `check_origin` uses a
//! real WHATWG parser rather than string surgery — `https://x:443` and `https://x`
//! are the SAME origin, and only a spec parser knows that.
//!
//! Regenerate + run:
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   TMPO=/tmp/qt-upgrade-origin-oracle
//!   rm -f /tmp/oracle-upgrade-origin.ndjson
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/upgrade-origin.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-upgrade-origin.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=60000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- upgrade-origin
//!   cd $V5W
//!   QT_ORACLE_UPGRADE_ORIGIN=/tmp/oracle-upgrade-origin.ndjson \
//!     cargo test -p quilltap-web --test terminal_ws_origin_equivalence -- --nocapture

use quilltap_web::upgrade_auth::check_origin;
use serde_json::Value;

#[test]
fn upgrade_origin_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_UPGRADE_ORIGIN") else {
        eprintln!("SKIP: set QT_ORACLE_UPGRADE_ORIGIN to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle line");
        let name = row["name"].as_str().unwrap();
        // An ABSENT key is the header not being sent at all.
        let origin = row["input"].get("origin").and_then(Value::as_str);
        let host = row["input"].get("host").and_then(Value::as_str);

        let got = check_origin(origin, host);
        let expected_ok = row["ok"].as_bool().unwrap();
        let expected_reason = row.get("reason").and_then(Value::as_str);

        match (&got, expected_ok) {
            (None, true) => {}
            (Some(reason), false) if Some(reason.as_str()) == expected_reason => {}
            _ => failed.push(format!(
                "{name}: rust {got:?} != oracle ok={expected_ok} reason={expected_reason:?} \
                 (origin={origin:?} host={host:?})"
            )),
        }
        ran += 1;
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert!(ran >= 19, "the corpus is too thin: {ran}");
}
