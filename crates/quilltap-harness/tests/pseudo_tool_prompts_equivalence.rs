//! Tier-1 differential: pseudo-tool prompt builders + option determination
//! (wave 4 / W4.1b.1 — prompts half). The simple-json builder renders signatures
//! from the b.3 tool definitions, so this proves the prompt over the REAL ported
//! catalog. Exact string equality.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/pseudo-tool-prompts.ts \
//!     > /tmp/oracle-pseudo-tool-prompts.ndjson
//! Run:
//!   QT_ORACLE_PSEUDO_TOOL_PROMPTS=/tmp/oracle-pseudo-tool-prompts.ndjson \
//!     cargo test -p quilltap-harness --test pseudo_tool_prompts_equivalence

use quilltap_core::services::pseudo_tool::{self, TextBlockEnabledToolOptions};
use quilltap_core::tools::native_tool_prompt::build_native_tool_instructions;
use quilltap_core::tools::simple_json_prompt::{
    build_simple_json_tool_instructions, SimpleJsonPromptOptions,
};
use quilltap_core::tools::text_block_prompt::{
    build_text_block_instructions, TextBlockPromptOptions,
};
use serde_json::Value;

#[test]
fn pseudo_tool_prompts_match_oracle() {
    let path = match std::env::var("QT_ORACLE_PSEUDO_TOOL_PROMPTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PSEUDO_TOOL_PROMPTS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let f = row["fn"].as_str().unwrap();
        let id = row["id"].as_str().unwrap_or("<no-id>");
        let want = &row["out"];

        let got: Value = match f {
            "build_native" => Value::String(build_native_tool_instructions(
                row["hasTools"].as_bool().unwrap(),
            )),
            "build_native_system" => {
                Value::String(pseudo_tool::build_native_tool_system_instructions())
            }
            "build_text_block" => {
                let opts: TextBlockPromptOptions =
                    serde_json::from_value(row["options"].clone()).unwrap();
                Value::String(build_text_block_instructions(&opts))
            }
            "build_simple_json" => {
                let opts: SimpleJsonPromptOptions =
                    serde_json::from_value(row["options"].clone()).unwrap();
                Value::String(build_simple_json_tool_instructions(&opts))
            }
            "determine_enabled" => {
                let img = row["imageProfileId"].as_str();
                let allow = row["allowWebSearch"].as_bool().unwrap();
                serde_json::to_value(pseudo_tool::determine_enabled_tool_options(img, allow))
                    .unwrap()
            }
            "determine_text_block" => {
                let c = &row["input"];
                let opts = pseudo_tool::determine_text_block_tool_options(
                    c["imageProfileId"].as_str(),
                    c["allowWebSearch"].as_bool().unwrap(),
                    c["isMultiCharacter"].as_bool().unwrap(),
                    c["hasProject"].as_bool().unwrap(),
                    c.get("helpToolsEnabled").and_then(Value::as_bool),
                    c.get("canDressThemselves").and_then(Value::as_bool),
                    c.get("canCreateOutfits").and_then(Value::as_bool),
                );
                serde_json::to_value(opts).unwrap()
            }
            "build_text_block_system" => {
                let opts: TextBlockEnabledToolOptions =
                    serde_json::from_value(row["enabledOptions"].clone()).unwrap();
                Value::String(pseudo_tool::build_text_block_system_instructions(&opts))
            }
            "build_simple_json_system" => {
                let opts: TextBlockEnabledToolOptions =
                    serde_json::from_value(row["enabledOptions"].clone()).unwrap();
                Value::String(pseudo_tool::build_simple_json_system_instructions(&opts))
            }
            other => panic!("unknown fn '{other}' in case '{id}'"),
        };

        assert_eq!(&got, want, "fn '{f}' case '{id}'");
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: pseudo-tool-prompts matched oracle ({count} rows).");
}
