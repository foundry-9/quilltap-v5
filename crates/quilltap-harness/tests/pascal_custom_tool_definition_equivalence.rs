//! Tier-1 differential (P4.6ay unit 1): the Pascal custom-tool DEFINITION
//! format — the accept/reject verdict, the parsed data, the unknown-key report,
//! and the full rejection SENTENCE, byte-exact against v4's real Zod schema.
//!
//! The sentence is compared rather than regex-matched because it is payload:
//! `loadToolsFromMount` stores `formatDefinitionIssues(...)` as a load error's
//! `reason`, and the custom-tools GET route returns it verbatim in `errors[]`.
//! v4's own test only `toMatch(/…/)`s these strings, so this is a strictly
//! stronger bar — and the one the route differential needs.
//!
//! The `gate` rows (P4.d19, v4 `6864bf0e`) ride in the same file: the verdict
//! `evaluateToolGate` returns for one definition against one fact sheet, driven
//! directly because the roster only ever shows a verdict's consequence, and the
//! asymmetry that matters — an empty sheet fails every `availableWhen` and
//! satisfies no `withheldWhen` — is invisible from there.
//!
//! Generate the oracle output (v4 @ 231be14c, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`; the pinned detached worktree):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx \
//!     <V5W>/harness/oracle/cases/pascal-custom-tool-definition.ts \
//!     > /tmp/oracle-pascal-definition.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_DEFINITION=/tmp/oracle-pascal-definition.ndjson \
//!     cargo test -p quilltap-harness --test pascal_custom_tool_definition_equivalence

use quilltap_core::pascal::custom_tool_types::{
    collect_unknown_keys, display_title, format_definition_issues, safe_parse,
};
use quilltap_core::pascal::tool_gate::{evaluate_tool_gate, has_tool_gate};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "title")]
    Title {
        id: String,
        input: TitleInput,
        out: String,
    },
    #[serde(rename = "definition")]
    Definition {
        id: String,
        /// The definition's BYTES — never a re-stringified structure. Both sides
        /// parse this with their own JSON parser, exactly as `readToolFile` does.
        #[serde(rename = "inputJson")]
        input_json: String,
        success: bool,
        reason: Option<String>,
        data: Option<String>,
        #[serde(rename = "unknownKeys")]
        unknown_keys: Vec<String>,
    },
    /// The availability gate (P4.d19, v4 `6864bf0e`): the verdict
    /// `evaluateToolGate` returns for one definition against one fact sheet,
    /// serialized — so `withheldBy`'s ABSENCE on an available verdict is part
    /// of what is compared.
    #[serde(rename = "gate")]
    Gate {
        id: String,
        #[serde(rename = "inputJson")]
        input_json: String,
        metadata: Option<Map<String, Value>>,
        #[serde(rename = "hasGate")]
        has_gate: bool,
        verdict: String,
    },
}

#[derive(Deserialize)]
struct TitleInput {
    name: String,
    title: Option<String>,
}

#[test]
fn pascal_custom_tool_definition_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_DEFINITION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_DEFINITION to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut titles, mut defs, mut gates) = (0usize, 0usize, 0usize);
    // Coverage, not a magic row count: a green gate run proves nothing if every
    // row happens to be an ungated definition. Both clauses, both verdicts, and
    // both `hasToolGate` answers must appear.
    let mut seen_clauses: Vec<String> = Vec::new();
    let mut seen_available: Vec<bool> = Vec::new();
    let mut seen_has_gate: Vec<bool> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Title { id, input, out } => {
                let got = display_title(&input.name, input.title.as_deref());
                assert_eq!(got, out, "displayTitle '{id}'");
                titles += 1;
            }
            Row::Definition {
                id,
                input_json,
                success,
                reason,
                data,
                unknown_keys,
            } => {
                let raw: Value = serde_json::from_str(&input_json)
                    .unwrap_or_else(|e| panic!("case '{id}': oracle input is not JSON: {e}"));

                assert_eq!(
                    collect_unknown_keys(&raw),
                    unknown_keys,
                    "collectUnknownKeys '{id}'"
                );

                match safe_parse(&raw) {
                    Ok(tool) => {
                        assert!(
                            success,
                            "case '{id}': v5 accepted, v4 rejected with: {reason:?}"
                        );
                        let got = serde_json::to_string(&tool).unwrap();
                        assert_eq!(
                            got,
                            data.unwrap_or_default(),
                            "case '{id}': parsed data differs (key order included)"
                        );
                    }
                    Err(issues) => {
                        let got = format_definition_issues(&issues);
                        assert!(
                            !success,
                            "case '{id}': v5 rejected with '{got}', v4 accepted"
                        );
                        assert_eq!(
                            got,
                            reason.unwrap_or_default(),
                            "case '{id}': rejection differs"
                        );
                    }
                }
                defs += 1;
            }
            Row::Gate {
                id,
                input_json,
                metadata,
                has_gate,
                verdict,
            } => {
                let raw: Value = serde_json::from_str(&input_json)
                    .unwrap_or_else(|e| panic!("gate case '{id}': oracle input is not JSON: {e}"));
                let tool = safe_parse(&raw).unwrap_or_else(|e| {
                    panic!("gate case '{id}': definition does not load: {e:?}")
                });

                assert_eq!(has_tool_gate(&tool), has_gate, "hasToolGate '{id}'");

                let got = serde_json::to_string(&evaluate_tool_gate(&tool, metadata.as_ref()))
                    .expect("a verdict serializes");
                assert_eq!(
                    got, verdict,
                    "gate case '{id}': verdict differs (withheldBy's absence included)"
                );

                let seen: Value = serde_json::from_str(&verdict).expect("a verdict is JSON");
                if let Some(clause) = seen.get("withheldBy").and_then(Value::as_str) {
                    if !seen_clauses.iter().any(|c| c == clause) {
                        seen_clauses.push(clause.to_string());
                    }
                }
                for (bucket, flag) in [
                    (
                        &mut seen_available,
                        seen["available"].as_bool().unwrap_or(false),
                    ),
                    (&mut seen_has_gate, has_gate),
                ] {
                    if !bucket.contains(&flag) {
                        bucket.push(flag);
                    }
                }
                gates += 1;
            }
        }
    }

    assert!(
        titles > 0 && defs > 0 && gates > 0,
        "oracle file looks empty: {titles} titles, {defs} definitions, {gates} gates"
    );
    seen_clauses.sort();
    assert_eq!(
        seen_clauses,
        vec!["availableWhen".to_string(), "withheldWhen".to_string()],
        "the gate corpus must withhold by BOTH clauses (saw {seen_clauses:?})"
    );
    seen_available.sort();
    seen_has_gate.sort();
    assert_eq!(
        (seen_available, seen_has_gate),
        (vec![false, true], vec![false, true]),
        "the gate corpus must cover available AND withheld, gated AND ungated"
    );
    eprintln!(
        "OK: pascal custom-tool definition matched oracle \
         ({titles} titles, {defs} definitions, {gates} gate verdicts)."
    );
}
