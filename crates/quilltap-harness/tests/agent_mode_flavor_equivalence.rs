//! Tier-1 differential (P4.D370, v4 `96156566b` at `624d0d25c`): Agent Mode
//! flavours — the cascade decides whether Agent Mode is on, the responding
//! character's kind decides what it means.
//!
//! Drives v4's REAL `resolveAgentModeSetting` / `buildAgentModeInstructions` /
//! `buildForceFinalMessage` and compares, per row:
//!   - `resolve`: the `ResolvedAgentMode` (`flavor` by its wire spelling) AND
//!     the `[AgentMode] Resolved agent mode` INFO — message, target
//!     `quilltap::agent_mode`, keys in v4's ORDER (`chatId` absent for a null
//!     chat), captured on this thread;
//!   - `instructions` / `force_final`: the exact prompt strings.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/agent-mode-flavor.ts \
//!     > /tmp/oracle-agent-mode-flavor.ndjson
//! Run:
//!   QT_ORACLE_AGENT_MODE_FLAVOR=/tmp/oracle-agent-mode-flavor.ndjson \
//!     cargo test -p quilltap-harness --test agent_mode_flavor_equivalence

use quilltap_core::characters::kind::AgentModeFlavor;
use quilltap_core::services::agent_mode::{
    build_agent_mode_instructions, build_force_final_message, global_agent_mode_settings,
    resolve_agent_mode_setting, ResolvedAgentMode,
};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Value};

/// NON_BOOLEAN_CHARACTER_AGENT_DEFAULT — a ruled divergence (the order's item
/// 6: "a non-boolean is 'unset', as today's callers' `as_bool` already read
/// it"). v4's cascade tests `!== null && !== undefined`, so a stored
/// `defaultAgentModeEnabled: "yes"` becomes `enabled: "yes"` with
/// `enabledSource: 'character'`; v5's resolver reads only a boolean, so the
/// character tier is unset and the lower tier stands. Pinned BOTH ways: v4 must
/// still show its shape (else the row is retired) and v5 must show the
/// unset-tier shape.
const NON_BOOLEAN_CHARACTER_AGENT_DEFAULT: &[&str] = &[
    "roleplay--character-yes",
    "pre-kind--character-yes",
    "assistant--character-yes",
    "robot--character-yes",
    "capital-assistant--character-yes",
    "kind-null--character-yes",
    "kind-five--character-yes",
];

/// ONE parse of a wire flavour onto C5's `AgentModeFlavor`; an unknown
/// spelling PANICS (never a default).
fn flavor_of(v: &Value, id: &str) -> AgentModeFlavor {
    match v.as_str() {
        Some("research") => AgentModeFlavor::Research,
        Some("in-character") => AgentModeFlavor::InCharacter,
        other => panic!("row '{id}': unknown flavor {other:?}"),
    }
}

fn resolved_json(r: &ResolvedAgentMode) -> Value {
    json!({
        "enabled": r.enabled,
        "maxTurns": r.max_turns,
        "enabledSource": r.enabled_source.as_str(),
        "flavor": r.flavor.as_str(),
    })
}

/// A v4 captured line as the v5 capture rig renders it:
/// `INFO <target> <message> k=v …`, keys in v4's order.
fn render_v4_line(line: &Value) -> String {
    let mut out = format!(
        "INFO quilltap::agent_mode {}",
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["context"].as_object().unwrap() {
        let v = v
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string());
        out.push_str(&format!(" {k}={v}"));
    }
    out
}

#[test]
fn agent_mode_flavor_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_AGENT_MODE_FLAVOR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_AGENT_MODE_FLAVOR to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    let mut diverged = 0usize;
    let mut red: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        match kind {
            "resolve" => {
                let chat = &row["chat"];
                let chat_id = chat.get("id").and_then(Value::as_str);
                let chat_enabled = chat.get("agentModeEnabled").and_then(Value::as_bool);
                let project_default = row["project"]
                    .get("defaultAgentModeEnabled")
                    .and_then(Value::as_bool);
                let character = (!row["character"].is_null()).then_some(&row["character"]);
                let settings = (!row["globalSettings"].is_null()).then_some(&row["globalSettings"]);
                let (got, lines) = captured_with(|| {
                    resolve_agent_mode_setting(
                        chat_id,
                        chat_enabled,
                        project_default,
                        character,
                        global_agent_mode_settings(settings),
                    )
                });
                let got_json = resolved_json(&got);
                let want = &row["out"];
                let v4_lines: Vec<String> = row["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(render_v4_line)
                    .collect();
                if NON_BOOLEAN_CHARACTER_AGENT_DEFAULT.contains(&id) {
                    // v4's side of the divergence: the non-boolean as `enabled`.
                    if want["enabled"] != json!("yes")
                        || want["enabledSource"] != json!("character")
                    {
                        red.push(format!(
                            "{id}: v4 no longer shows the divergence — retire it ({want})"
                        ));
                    }
                    // v5's side: the character tier unset — this corpus's
                    // `character-yes` cascade has no project / chat level, so the
                    // global default (off) stands.
                    if got.enabled_source.as_str() != "global" || got.enabled {
                        red.push(format!("{id}: v5 read a non-boolean default ({got_json})"));
                    }
                    if got_json["flavor"] != want["flavor"]
                        || got_json["maxTurns"] != want["maxTurns"]
                    {
                        red.push(format!("{id}: flavor/maxTurns {got_json} vs {want}"));
                    }
                    diverged += 1;
                } else {
                    // Bytes, not `Map ==` (key order is part of the comparand).
                    if serde_json::to_string(&got_json).unwrap()
                        != serde_json::to_string(want).unwrap()
                    {
                        red.push(format!("{id}: out v5 {got_json} vs v4 {want}"));
                    }
                    if lines != v4_lines {
                        red.push(format!("{id}: lines v5 {lines:?} vs v4 {v4_lines:?}"));
                    }
                }
                // Exactly one line per resolve, both sides (the silence leg is
                // the count: never a second line, never none).
                if lines.len() != 1 || v4_lines.len() != 1 {
                    red.push(format!(
                        "{id}: expected ONE line each, v5 {} v4 {}",
                        lines.len(),
                        v4_lines.len()
                    ));
                }
            }
            "instructions" => {
                let flavor = flavor_of(&row["flavor"], id);
                let got = build_agent_mode_instructions(row["maxTurns"].as_i64().unwrap(), flavor);
                if Value::String(got) != row["out"] {
                    red.push(format!("instructions/{id}"));
                }
            }
            "force_final" => {
                let flavor = flavor_of(&row["flavor"], id);
                if Value::String(build_force_final_message(flavor)) != row["out"] {
                    red.push(format!("force_final/{id}"));
                }
            }
            other => panic!("unknown kind '{other}' in row '{id}'"),
        }
        count += 1;
    }
    for r in &red {
        eprintln!("RED {r}");
    }
    assert!(red.is_empty(), "{} red of {count} rows", red.len());
    assert!(count >= 60, "oracle file looks short: {count}");
    assert_eq!(
        diverged,
        NON_BOOLEAN_CHARACTER_AGENT_DEFAULT.len(),
        "every named divergence row must be present in the oracle"
    );
    eprintln!("OK: agent-mode-flavor matched oracle ({count} rows, {diverged} named divergences).");
}
