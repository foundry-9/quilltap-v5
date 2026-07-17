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
//! Generate the oracle output (v4 @ e3593f75, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   npx tsx \
//!     ~/source/quilltap-v5/.claude/worktrees/pascal-custom-tools-porting-dd49d0/harness/oracle/cases/pascal-custom-tool-definition.ts \
//!     > /tmp/oracle-pascal-definition.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_DEFINITION=/tmp/oracle-pascal-definition.ndjson \
//!     cargo test -p quilltap-harness --test pascal_custom_tool_definition_equivalence

use quilltap_core::pascal::custom_tool_types::{
    collect_unknown_keys, display_title, format_definition_issues, safe_parse,
};
use serde::Deserialize;
use serde_json::Value;

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

    let (mut titles, mut defs) = (0usize, 0usize);
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
        }
    }

    assert!(
        titles > 0 && defs > 0,
        "oracle file looks empty: {titles} titles, {defs} definitions"
    );
    eprintln!(
        "OK: pascal custom-tool definition matched oracle ({titles} titles, {defs} definitions)."
    );
}
