//! Tier-1 differential: the Scenario Builder's two prompt builders — v4
//! `buildScenarioBuilderSystemPrompt` + `buildScenarioBuilderUserMessage`
//! (`lib/scenario-builder/system-prompt.ts`, `d1c06cd9d`) against v5's
//! `quilltap_core::services::scenario_builder::system_prompt`.
//!
//! Exact on every output byte. The system rows carry `(epochMs, tz)`: v4 set
//! `process.env.TZ` per row and passed `new Date(epochMs)`; v5 builds a
//! `jiff::Zoned` from the same pair, so the IANA zone → offset resolution is
//! compared along with `formatIsoWithOffset` (Chicago/Tokyo for the sign,
//! Kolkata/St John's/Kathmandu for the `% 60` remainder across DST, UTC for
//! `+00:00`-never-`Z`, and a 1999 instant for a year boundary crossing zones).
//! The full `real`/`in-world` × `webAvailable` × tool-instructions grid rides
//! the first zone and instant.
//!
//! The user rows pin the padded `currentScenario` row as TRIMMED (the P4.D217
//! order's §R.4(d) said v4 interpolates it untrimmed — measured false; see
//! `system_prompt.rs`'s module header), the padded revise pair as UNTRIMMED,
//! and the `!= null` revise gate with empty strings.
//!
//! ⚠ PIN REQUIRED at the TARGET `d1c06cd9d`: the module does not exist at the
//! `00c290c9a` baseline, so a baseline-pinned run fails to IMPORT — that failure
//! is the pin verification.
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-scenario-builder-prompts.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/scenario-builder-prompts.ts \
//!     > /tmp/oracle-scenario-builder-prompts.ndjson
//!   cd $V5W
//!   QT_ORACLE_SCENARIO_BUILDER_PROMPTS=/tmp/oracle-scenario-builder-prompts.ndjson \
//!     cargo test -p quilltap-harness --test scenario_builder_prompts_equivalence -- --nocapture

use quilltap_core::services::scenario_builder::request_schema::ScenarioBuilderMode;
use quilltap_core::services::scenario_builder::system_prompt::{
    build_scenario_builder_system_prompt, build_scenario_builder_user_message, zoned_at,
    ScenarioBuilderUserMessageInput, SCENARIO_TARGET_TOKENS,
};
use serde_json::Value;

fn mode_of(s: &str) -> ScenarioBuilderMode {
    match s {
        "real" => ScenarioBuilderMode::Real,
        "in-world" => ScenarioBuilderMode::InWorld,
        other => panic!("unknown mode {other} in the corpus"),
    }
}

#[test]
fn scenario_builder_prompts_match_oracle() {
    let path = match std::env::var("QT_ORACLE_SCENARIO_BUILDER_PROMPTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_SCENARIO_BUILDER_PROMPTS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    assert!(!text.trim().is_empty(), "{path} is EMPTY");

    let (mut systems, mut users, mut consts) = (0usize, 0usize, 0usize);
    let mut zones = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        let id = row["id"].as_str().unwrap().to_string();
        match row["kind"].as_str().unwrap() {
            "const" => {
                consts += 1;
                assert_eq!(
                    row["value"].as_u64(),
                    Some(u64::from(SCENARIO_TARGET_TOKENS))
                );
            }
            "system" => {
                systems += 1;
                let tz = row["tz"].as_str().unwrap();
                zones.insert(tz.to_string());
                let now = zoned_at(row["epochMs"].as_i64().unwrap(), tz)
                    .unwrap_or_else(|| panic!("{id}: jiff cannot resolve {tz}"));
                let got = build_scenario_builder_system_prompt(
                    mode_of(row["mode"].as_str().unwrap()),
                    row["webAvailable"].as_bool().unwrap(),
                    row["toolInstructions"].as_str().unwrap(),
                    &now,
                );
                let want = row["out"].as_str().unwrap();
                if got != want {
                    failures.push(format!("{id} ({tz}):\n--- v4\n{want}\n--- v5\n{got}"));
                }
            }
            "user" => {
                users += 1;
                let input = &row["input"];
                let s = |k: &str| input.get(k).and_then(Value::as_str);
                let got = build_scenario_builder_user_message(&ScenarioBuilderUserMessageInput {
                    mode: mode_of(s("mode").unwrap()),
                    location: s("location").unwrap(),
                    time: s("time").unwrap(),
                    details: s("details").unwrap(),
                    current_scenario: s("currentScenario"),
                    context_summary: s("contextSummary"),
                    prior_draft: s("priorDraft"),
                    revision: s("revision"),
                });
                let want = row["out"].as_str().unwrap();
                if got != want {
                    failures.push(format!("{id}:\n--- v4\n{want}\n--- v5\n{got}"));
                }
            }
            other => panic!("unknown row kind {other}"),
        }
    }
    eprintln!(
        "scenario_builder_prompts: {systems} system rows over {} zones, {users} user rows, {consts} const",
        zones.len()
    );
    assert_eq!(consts, 1);
    assert!(systems >= 20 && zones.len() >= 6, "the zone grid shrank");
    assert!(users >= 15, "the user-message corpus shrank");
    assert!(
        failures.is_empty(),
        "{} row(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
