//! Tier-1 differential: pseudo-tool parsers / strippers / conversions / mode
//! resolution (wave 4 / W4.1b.1 — parsers half). Pure functions; exact equality.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/pseudo-tool-parsers.ts \
//!     > /tmp/oracle-pseudo-tool-parsers.ndjson
//! Run:
//!   QT_ORACLE_PSEUDO_TOOL_PARSERS=/tmp/oracle-pseudo-tool-parsers.ndjson \
//!     cargo test -p quilltap-harness --test pseudo_tool_parsers_equivalence

use quilltap_core::services::pseudo_tool;
use quilltap_core::tools::pseudo_tool_support::{
    resolve_tool_mode, should_use_text_block_tools, ToolMode,
};
use quilltap_core::tools::simple_json_parser as sj;
use quilltap_core::tools::text_block_parser as tb;
use serde_json::{json, Value};

fn parsed_sj_to_value(p: &sj::ParsedSimpleJsonCall) -> Value {
    json!({
        "toolName": p.tool_name,
        "arguments": p.arguments,
        "fullMatch": p.full_match,
        "startIndex": p.start_index,
        "endIndex": p.end_index,
        "parserTier": p.parser_tier.as_str(),
    })
}

fn parsed_tb_to_value(p: &tb::ParsedTextBlock) -> Value {
    let mut params = serde_json::Map::new();
    for (k, v) in &p.params {
        params.insert(k.clone(), Value::String(v.clone()));
    }
    json!({
        "toolName": p.tool_name,
        "params": Value::Object(params),
        "content": p.content,
        "fullMatch": p.full_match,
        "startIndex": p.start_index,
        "endIndex": p.end_index,
    })
}

fn request_to_value(name: &str, arguments: &Value) -> Value {
    json!({ "name": name, "arguments": arguments })
}

fn override_from(row: &Value) -> Option<ToolMode> {
    match row.get("override") {
        Some(Value::String(s)) => ToolMode::from_str(s),
        _ => None,
    }
}

#[test]
fn pseudo_tool_parsers_match_oracle() {
    let path = match std::env::var("QT_ORACLE_PSEUDO_TOOL_PARSERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PSEUDO_TOOL_PARSERS to the oracle NDJSON (see header).");
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
            "parse_simple_json" => {
                let response = row["response"].as_str().unwrap();
                let parsed = sj::parse_simple_json_calls(response);
                let parsed_v: Vec<Value> = parsed.iter().map(parsed_sj_to_value).collect();
                let converted_v: Vec<Value> = parsed
                    .iter()
                    .map(|p| {
                        let r = sj::convert_simple_json_to_tool_call_request(p);
                        request_to_value(&r.name, &r.arguments)
                    })
                    .collect();
                json!({ "parsed": parsed_v, "converted": converted_v })
            }
            "strip_simple_json" => Value::String(sj::strip_simple_json_markers(
                row["response"].as_str().unwrap(),
            )),
            "has_simple_json" => Value::Bool(sj::has_simple_json_markers(
                row["response"].as_str().unwrap(),
            )),
            "map_simple_json_name" => {
                Value::String(sj::map_simple_json_tool_name(row["name"].as_str().unwrap()))
            }
            "escape_xml" => Value::String(sj::escape_xml_attribute(row["value"].as_str().unwrap())),
            "format_simple_json_result" => {
                Value::String(pseudo_tool::format_simple_json_tool_result(
                    row["toolName"].as_str().unwrap(),
                    row["content"].as_str().unwrap(),
                ))
            }
            "stop_sequences" => json!(sj::SIMPLE_JSON_STOP_SEQUENCES),
            "parse_text_block" => {
                let response = row["response"].as_str().unwrap();
                let parsed = tb::parse_text_block_calls(response);
                let parsed_v: Vec<Value> = parsed.iter().map(parsed_tb_to_value).collect();
                let converted_v: Vec<Value> = parsed
                    .iter()
                    .map(|p| {
                        let r = tb::convert_text_block_to_tool_call_request(p);
                        request_to_value(&r.name, &r.arguments)
                    })
                    .collect();
                json!({ "parsed": parsed_v, "converted": converted_v })
            }
            "strip_text_block" => Value::String(tb::strip_text_block_markers(
                row["response"].as_str().unwrap(),
            )),
            "has_text_block" => Value::Bool(tb::has_text_block_markers(
                row["response"].as_str().unwrap(),
            )),
            "map_text_block_name" => {
                Value::String(tb::map_text_block_tool_name(row["name"].as_str().unwrap()))
            }
            "resolve_tool_mode" => {
                let supports = row["supportsNative"].as_bool().unwrap();
                Value::String(
                    resolve_tool_mode(supports, override_from(&row))
                        .as_str()
                        .to_string(),
                )
            }
            "should_use_text_block" => {
                let supports = row["supportsNative"].as_bool().unwrap();
                Value::Bool(should_use_text_block_tools(supports, override_from(&row)))
            }
            other => panic!("unknown fn '{other}' in case '{id}'"),
        };

        assert_eq!(&got, want, "fn '{f}' case '{id}'");
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: pseudo-tool-parsers matched oracle ({count} rows).");
}
