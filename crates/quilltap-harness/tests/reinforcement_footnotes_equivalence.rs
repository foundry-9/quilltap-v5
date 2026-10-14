//! Tier-1 differential: v4's reinforcement footnotes at the pin (`83334b9a5`)
//! — `crate::reinforcement_footnotes` (P4.D289 unit 5, C1-A item 5) against
//! v4's REAL `lib/memory/reinforcement-footnotes.ts`:
//! `MAX_REINFORCEMENT_FOOTNOTES`, `countReinforcementFootnotes`,
//! `appendCappedFootnotes(content, details, cap)`.
//!
//! Rows: the 0 / 7 / 8 / 9-footnote × 0 / 1 / 3-detail grid at the default
//! cap (v4's omitted `cap` — the Rust side passes
//! `MAX_REINFORCEMENT_FOOTNOTES`), bugs 231 / 234's skip-present (a present
//! detail, all present, a present detail not spending room, prose and
//! no-space lines that are NOT footnotes) and within-call dedupe, room 0, a
//! cap below the present count, cap 0 / 1 / 100, `\r\n` content (whose
//! `[+] X\r` lines never match `X`), `[+]x` without the space, empty details /
//! content / detail string, a leading / indented footnote, non-BMP details.
//! Strings compare exactly.
//!
//! Generate the oracle output, then run:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/reinforcement-footnotes.ts" > /tmp/oracle-reinforcement-footnotes.ndjson
//!   cd "$V5W"
//!   QT_ORACLE_REINFORCEMENT_FOOTNOTES=/tmp/oracle-reinforcement-footnotes.ndjson \
//!     cargo test -p quilltap-harness --test reinforcement_footnotes_equivalence -- --nocapture

use quilltap_core::reinforcement_footnotes::{
    append_capped_footnotes, count_reinforcement_footnotes, MAX_REINFORCEMENT_FOOTNOTES,
};
use serde_json::Value;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn reinforcement_footnotes_match_oracle() {
    let path = match std::env::var("QT_ORACLE_REINFORCEMENT_FOOTNOTES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_REINFORCEMENT_FOOTNOTES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut constants, mut appends) = (0usize, 0usize);
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        match row["kind"].as_str().unwrap() {
            "constant" => {
                if row["out"].as_u64().unwrap() as usize != MAX_REINFORCEMENT_FOOTNOTES {
                    failures.push(format!(
                        "{id}: {MAX_REINFORCEMENT_FOOTNOTES} vs {}",
                        row["out"]
                    ));
                }
                constants += 1;
            }
            "append" => {
                let content = row["content"].as_str().unwrap();
                let details = strings(&row["details"]);
                // `null` = the call omitted `cap` (v4's default).
                let cap = row["cap"]
                    .as_u64()
                    .map(|c| c as usize)
                    .unwrap_or(MAX_REINFORCEMENT_FOOTNOTES);
                let count = count_reinforcement_footnotes(content);
                if count as u64 != row["count"].as_u64().unwrap() {
                    failures.push(format!("{id}: count {count} vs {}", row["count"]));
                }
                let (got_content, got_appended) = append_capped_footnotes(content, &details, cap);
                let out = &row["out"];
                if got_content != out["content"].as_str().unwrap()
                    || got_appended != strings(&out["appended"])
                {
                    failures.push(format!(
                        "{id}: rust ({got_content:?}, {got_appended:?}) vs oracle {out}"
                    ));
                }
                appends += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    assert!(failures.is_empty(), "diverged:\n{}", failures.join("\n"));
    // Corpus drift guard: the case's exact row counts.
    assert_eq!(
        (constants, appends),
        (1, 35),
        "row counts (constant, append)"
    );
    eprintln!(
        "OK: reinforcement-footnotes matched oracle ({constants} constant, {appends} append rows)."
    );
}
