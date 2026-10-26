//! Tier-1 differential (P4.D333, v4 `a53024d3c`): the fail-soft METADATA
//! comparison table, driven DIRECTLY — `metadata_comparator_holds` against v4's
//! real `metadataComparatorHolds`.
//!
//! `a53024d3c` gave the table a LIST rule (a key holding an array of
//! primitives compares entry by entry; an ordering key declines on it). The
//! entrance rows (the execution family's `matchesWhen`, the definition family's
//! gate verdicts) prove the verdicts; what only a direct drive can prove is the
//! OPERAND ORDER — the list path declines an ordering key BEFORE resolving any
//! operand, the scalar walk resolves first — because only a resolver that
//! records what it was asked, and fails on a scripted key, can tell the two
//! walks apart. Each row's `held` / `asked` / `declined` / `threw` is compared
//! as ONE JSON object's bytes.
//!
//! The comparator travels as JSON and is loaded here through the real
//! definition schema (as an outcome row's `metadata` test, with the two
//! parameters its `$param` operands name declared), so no row can pose a
//! comparator v5's loader would refuse.
//!
//! Generate the oracle output (v4 @ 322a5488e, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx \
//!     <V5W>/harness/oracle/cases/pascal-metadata-match.ts \
//!     > /tmp/oracle-pascal-metadata-match.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_METADATA_MATCH=/tmp/oracle-pascal-metadata-match.ndjson \
//!     cargo test -p quilltap-harness --test pascal_metadata_match_equivalence

use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::metadata_match::{js_primitive, metadata_comparator_holds};
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Row {
    id: String,
    #[serde(rename = "comparatorJson")]
    comparator_json: String,
    key: String,
    #[serde(rename = "sheetJson")]
    sheet_json: String,
    #[serde(rename = "scriptJson")]
    script_json: String,
    #[serde(rename = "resultJson")]
    result_json: String,
}

#[test]
fn pascal_metadata_match_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_METADATA_MATCH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PASCAL_METADATA_MATCH to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut rows, mut list_rows, mut throw_rows, mut silent_declines) =
        (0usize, 0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let comparator_raw: Value = serde_json::from_str(&row.comparator_json).unwrap();
        let sheet: Map<String, Value> = serde_json::from_str(&row.sheet_json).unwrap();
        let script: Value = serde_json::from_str(&row.script_json).unwrap();

        let definition = json!({
            "name": "probe",
            "description": "A probe.",
            "parameters": {
                "difficulty": { "type": "number", "default": 10 },
                "material": { "type": "string", "default": "brass" },
            },
            "outcomes": [
                { "when": { "metadata": { row.key.clone(): comparator_raw.clone() } }, "message": "-", "state": "info" },
                { "when": true, "message": "-", "state": "info" },
            ],
        });
        let tool = safe_parse(&definition)
            .unwrap_or_else(|e| panic!("case '{}': comparator does not load: {e:?}", row.id));
        let group = &tool.outcomes[0].when.when_groups()[0];
        let comparator = &group.metadata.as_ref().expect("a metadata test")[0].1;

        let throws = script.get("throws").and_then(Value::as_str);
        let operands = script.get("operands").and_then(Value::as_object);
        let mut asked: Vec<String> = Vec::new();
        let mut declined: Vec<String> = Vec::new();
        let outcome = metadata_comparator_holds::<String>(
            comparator,
            &row.key,
            &sheet,
            &mut |ck| {
                asked.push(ck.to_string());
                if throws == Some(ck) {
                    return Err(ck.to_string());
                }
                let raw = operands
                    .and_then(|o| o.get(ck))
                    .or_else(|| comparator_raw.get(ck))
                    .unwrap_or_else(|| panic!("case '{}': no operand for {ck}", row.id));
                Ok(js_primitive(raw).expect("a primitive operand"))
            },
            &mut |reason| declined.push(reason),
        );
        let (held, threw) = match outcome {
            Ok(held) => (Value::Bool(held), Value::Null),
            Err(key) => (Value::Null, Value::String(key)),
        };
        // v4's object literal order: held, asked, declined, threw.
        let got = format!(
            "{{\"held\":{},\"asked\":{},\"declined\":{},\"threw\":{}}}",
            serde_json::to_string(&held).unwrap(),
            serde_json::to_string(&asked).unwrap(),
            serde_json::to_string(&declined).unwrap(),
            serde_json::to_string(&threw).unwrap(),
        );
        assert_eq!(
            got, row.result_json,
            "case '{}': held / asked / declined / threw differ",
            row.id
        );

        if sheet.get(&row.key).is_some_and(Value::is_array) {
            list_rows += 1;
        }
        throw_rows += usize::from(!threw.is_null());
        // The order proof: a row that declined having asked for NOTHING.
        silent_declines += usize::from(asked.is_empty() && !declined.is_empty());
        rows += 1;
    }

    assert!(rows > 0, "oracle file looks empty");
    assert!(
        list_rows >= 12,
        "only {list_rows} rows pose a list — the list rule went missing"
    );
    assert!(
        throw_rows >= 2,
        "only {throw_rows} rows threw — the order proof went missing"
    );
    assert!(
        silent_declines >= 1,
        "no row declined before asking — the order proof went missing"
    );
    eprintln!(
        "OK: metadata comparison table matched oracle ({rows} rows, {list_rows} over lists)."
    );
}
