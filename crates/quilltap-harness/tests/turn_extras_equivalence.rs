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
//! arithmetic is compared end to end. Each row carries its provider; this side
//! maps it through the manifest registry (the same read the orchestrator does),
//! so the `google-rate-*` row pins the per-provider 3.8 figure while the rest
//! ride the default 3.5.
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
use serde::Deserialize;
use serde_json::Value;

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
        /// §3 of the 93ed8abf round: the estimator rate is per provider (the
        /// `google-rate-*` row pins 3.8). Absent on older oracles → OPENAI.
        #[serde(default)]
        provider: Option<String>,
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
                provider,
                out,
            } => {
                // The seam: v4 passes the Provider into its estimator; v5
                // injects the rate. Map the row's provider through the SAME
                // registry the orchestrator reads, so the google-rate row
                // pins the per-provider figure end to end.
                let cpt = quilltap_core::provider_manifest::Registry::built_in()
                    .chars_per_token(provider.as_deref().unwrap_or("OPENAI"));
                let got = collect_turn_extras(TurnExtrasOptions {
                    tools: &tools,
                    agent_mode_enabled,
                    agent_mode_max_turns,
                    tool_settings_changed,
                    chars_per_token: cpt,
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
