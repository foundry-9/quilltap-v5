//! Tier-1 differential test: character kind (v4 `lib/characters/kind.ts`,
//! ported as `quilltap_core::characters::kind`, P4.D369 `KIND`).
//!
//! Every row of the oracle is one call into v4's REAL module; the Rust port
//! must serialize the same `out` BYTES (`serde_json::to_string`, never
//! `Value ==` — key order of the `KindPolicy` rows matters). An absent `value`
//! key is v4's `undefined` (Rust `None`).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   N=$HOME/.nvm/versions/node/v24.13.1/bin
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/character-kind.ts \
//!     > /tmp/oracle-character-kind.ndjson
//! Run:
//!   QT_ORACLE_CHARACTER_KIND=/tmp/oracle-character-kind.ndjson \
//!     cargo test -p quilltap-harness --test character_kind_equivalence

use quilltap_core::characters::kind::{
    character_kind_policy, is_assistant_character, kind_or_default, kind_policy,
    resolve_character_kind, CHARACTER_KINDS, DEFAULT_CHARACTER_KIND,
};
use serde_json::Value;

fn port(func: &str, value: Option<&Value>) -> String {
    let j = |r: serde_json::Result<String>| r.expect("serialize");
    match func {
        "characterKindOrDefault" => j(serde_json::to_string(&kind_or_default(value))),
        "resolveCharacterKind" => j(serde_json::to_string(&resolve_character_kind(value))),
        "isAssistantCharacter" => j(serde_json::to_string(&is_assistant_character(value))),
        // v4's `kindPolicy` re-normalizes its argument through
        // `characterKindOrDefault`; the Rust signature takes the enum, so the
        // normalization happens at the call (a bad runtime value → roleplay).
        "kindPolicy" => j(serde_json::to_string(&kind_policy(kind_or_default(value)))),
        "characterKindPolicy" => j(serde_json::to_string(&character_kind_policy(value))),
        "CHARACTER_KINDS" => j(serde_json::to_string(&CHARACTER_KINDS)),
        "DEFAULT_CHARACTER_KIND" => j(serde_json::to_string(&DEFAULT_CHARACTER_KIND)),
        other => panic!("unknown fn {other}"),
    }
}

#[test]
fn character_kind_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARACTER_KIND") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHARACTER_KIND to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    let mut failures = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        // Read the row as a raw object so a JSON `null` value stays
        // `Some(Null)` while an ABSENT key (v4's `undefined`) is `None`.
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let id = row["id"].as_str().expect("id");
        let func = row["fn"].as_str().expect("fn");
        let want = serde_json::to_string(&row["out"]).unwrap();
        let got = port(func, row.get("value"));
        if got != want {
            failures.push(format!("{id}: v4 {want} / v5 {got}"));
        }
        n += 1;
    }
    assert!(n >= 40, "oracle too small ({n} rows) — regenerate it");
    assert!(
        failures.is_empty(),
        "{} of {n} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("character_kind: {n} rows equal");
}
