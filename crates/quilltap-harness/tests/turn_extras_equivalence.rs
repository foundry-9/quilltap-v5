//! Tier-1 differential test (P4.D82): turn extras — the parts of an outgoing
//! payload that are not context (v4 `lib/services/chat-message/turn-extras.ts`,
//! `f933ba9c`, bug 70).
//!
//! Covers extractToolNames / buildToolChangeNotice / collectTurnExtras. Strings
//! exact (the tool-change notice is a contractual sentence the model reads, and
//! the agent-mode block is a prompt), token counts exact.
//!
//! The oracle drives v4's REAL collectTurnExtras, which in turn pulls v4's real
//! buildAgentModeInstructions and its real estimator — so the reservation
//! arithmetic is compared end to end. Every row uses the OPENAI provider, whose
//! estimator rate is the default 3.5 this side injects.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-extras.ts \
//!     > /tmp/oracle-turn-extras.ndjson
//! Run:
//!   QT_ORACLE_TURN_EXTRAS=/tmp/oracle-turn-extras.ndjson \
//!     cargo test -p quilltap-harness --test turn_extras_equivalence

use quilltap_core::services::turn_extras::{
    build_tool_change_notice, collect_turn_extras, extract_tool_names, TurnExtrasOptions,
};
use quilltap_core::token_estimation::DEFAULT_CHARS_PER_TOKEN;
use serde::Deserialize;
use serde_json::Value;

const CPT: f64 = DEFAULT_CHARS_PER_TOKEN;

#[derive(Deserialize)]
struct ExtrasOut {
    #[serde(rename = "agentModeInstructions")]
    agent_mode_instructions: Option<String>,
    #[serde(rename = "toolChangeNotice")]
    tool_change_notice: Option<String>,
    #[serde(rename = "toolSchemaTokens")]
    tool_schema_tokens: i64,
    #[serde(rename = "reservedTokens")]
    reserved_tokens: i64,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "names")]
    Names {
        id: String,
        tools: Vec<Value>,
        out: Vec<String>,
    },
    #[serde(rename = "notice")]
    Notice {
        id: String,
        #[serde(rename = "toolNames")]
        tool_names: Vec<String>,
        out: String,
    },
    #[serde(rename = "extras")]
    Extras {
        id: String,
        tools: Vec<Value>,
        #[serde(rename = "agentModeEnabled")]
        agent_mode_enabled: bool,
        #[serde(rename = "agentModeMaxTurns")]
        agent_mode_max_turns: i64,
        #[serde(rename = "toolSettingsChanged")]
        tool_settings_changed: bool,
        out: ExtrasOut,
    },
}

#[test]
fn turn_extras_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TURN_EXTRAS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TURN_EXTRAS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 3];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Names { id, tools, out } => {
                assert_eq!(extract_tool_names(&tools), out, "names '{id}'");
                counts[0] += 1;
            }
            OracleRow::Notice {
                id,
                tool_names,
                out,
            } => {
                assert_eq!(build_tool_change_notice(&tool_names), out, "notice '{id}'");
                counts[1] += 1;
            }
            OracleRow::Extras {
                id,
                tools,
                agent_mode_enabled,
                agent_mode_max_turns,
                tool_settings_changed,
                out,
            } => {
                let got = collect_turn_extras(TurnExtrasOptions {
                    tools: &tools,
                    agent_mode_enabled,
                    agent_mode_max_turns,
                    tool_settings_changed,
                    chars_per_token: CPT,
                });
                assert_eq!(
                    got.agent_mode_instructions, out.agent_mode_instructions,
                    "extras '{id}' agentModeInstructions"
                );
                assert_eq!(
                    got.tool_change_notice, out.tool_change_notice,
                    "extras '{id}' toolChangeNotice"
                );
                assert_eq!(
                    got.tool_schema_tokens, out.tool_schema_tokens,
                    "extras '{id}' toolSchemaTokens"
                );
                assert_eq!(
                    got.reserved_tokens, out.reserved_tokens,
                    "extras '{id}' reservedTokens"
                );
                counts[2] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: turn-extras matched oracle (counts {counts:?}).");
}
