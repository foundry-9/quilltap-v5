//! P4.167 (P4.D317 Tier 3 #30) differential: the `.qtap` NDJSON reader's log
//! lines.
//!
//! v4's `assembleExportFromStream` (`lib/import/quilltap-import-stream.ts:
//! 175-492` at `322a5488e`) logs SIX WARNs and one INFO as it reassembles a
//! streamed bundle: a non-object record, a second envelope, an orphaned
//! `wardrobe_item` / `character_plugin_data` / `chat_message`, each unknown
//! kind, and — after the envelope and truncation checks — the unknown-kind
//! count. v5's reader logged none of them. Both sides read the SAME committed
//! hand-built bundle (`harness/oracle/fixtures/qtap-ndjson-reader-lines.qtap`)
//! through the whole upload path (v4 `readNdjsonLines` +
//! `assembleExportFromStream`; v5 `load_qtap_from_upload`), and every line is
//! compared IN ORDER: level, message, and the context keys in order (`module`
//! dropped on v4's side, as the file envelope does). The reassembled shape
//! (the first envelope's `exportType`, the kept character with only its kept
//! wardrobe item) is compared too.
//!
//! The ARRAY record is the subtle row: an array is an object to JS, so v4's
//! `typeof raw !== 'object'` guard lets it through and its `undefined` kind
//! lands in the unknown-kind arm (`kind: "undefined"`), where `5` / `null` /
//! a string / `true` / `false` are skipped as non-objects.
//!
//! Generate the oracle (see `harness/oracle/cases/qtap-ndjson-reader-lines.ts`):
//!   cd ~/source/quilltap-server
//!   LOG_LEVEL=error npx tsx ~/source/quilltap-v5/harness/oracle/cases/qtap-ndjson-reader-lines.ts \
//!     > /tmp/oracle-qtap-ndjson-reader-lines.ndjson
//! Run:
//!   QT_ORACLE_QTAP_NDJSON_READER_LINES=/tmp/oracle-qtap-ndjson-reader-lines.ndjson \
//!     cargo test -p quilltap-harness --test qtap_ndjson_reader_lines_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::services::quilltap_import::ndjson::load_qtap_from_upload;
use serde_json::{json, Map, Value};

/// The reader's messages (v4's literals).
const MESSAGES: &[&str] = &[
    "Skipping non-object NDJSON record",
    "Second envelope line ignored — only the first is used",
    "wardrobe_item record with unknown parent character — skipping",
    "character_plugin_data with unknown parent character — skipping",
    "chat_message with unknown parent chat — skipping",
    "Unknown NDJSON record kind — skipping",
    "Some NDJSON records were skipped due to unknown kind",
];

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/qtap-ndjson-reader-lines.qtap")
}

/// v5's captured lines (`LEVEL target message k=v …`) whose message is one of
/// [`MESSAGES`], as ordered `[key, value]` pairs.
fn v5_records(lines: &[String]) -> Vec<Value> {
    let mut out = Vec::new();
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let (Some(level), Some(_target), Some(rest)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(message) = MESSAGES
            .iter()
            .find(|m| rest == **m || rest.starts_with(&format!("{m} ")))
        else {
            continue;
        };
        let mut pairs = vec![
            json!(["level", level.to_lowercase()]),
            json!(["message", message]),
        ];
        for kv in rest[message.len()..].split_whitespace() {
            if let Some((k, v)) = kv.split_once('=') {
                pairs.push(json!([k, v]));
            }
        }
        out.push(Value::Array(pairs));
    }
    out
}

/// v4's recorded lines as the same ordered pairs (values rendered as text,
/// the way the capture renders them).
fn v4_records(lines: &[Value]) -> Vec<Value> {
    lines
        .iter()
        .map(|r| {
            Value::Array(
                r.as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| {
                        json!([
                            k,
                            match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            }
                        ])
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn qtap_ndjson_reader_lines_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_QTAP_NDJSON_READER_LINES") else {
        eprintln!("SKIP: QT_ORACLE_QTAP_NDJSON_READER_LINES unset (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the oracle");
    let case: Value =
        serde_json::from_str(text.lines().next().expect("one case line")).expect("oracle JSON");
    assert_eq!(case["name"], "reader_lines");
    let v4_lines = case["lines"].as_array().expect("lines");
    assert_eq!(
        v4_lines.len(),
        14,
        "the fixture fires fourteen v4 reader lines — regenerate or re-measure"
    );

    let bytes = std::fs::read(fixture()).expect("read the fixture");
    let (got, lines) = quilltap_core::test_support::captured_with(|| load_qtap_from_upload(&bytes));
    let export = got.expect("the bundle reassembles on v5");

    let (g, w) = (v5_records(&lines), v4_records(v4_lines));
    assert_eq!(
        g, w,
        "the reader lines differ\n  rust:   {g:?}\n  oracle: {w:?}"
    );

    // The reassembled shape: the FIRST envelope's manifest, the kept
    // character with only its kept wardrobe item, no plugin data.
    assert_eq!(export.manifest["exportType"], case["exportType"]);
    let characters: Vec<Value> = export.data["characters"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            let mut m = Map::new();
            m.insert("id".into(), c["id"].clone());
            m.insert(
                "wardrobeItems".into(),
                Value::Array(
                    c["wardrobeItems"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|w| w["id"].clone())
                        .collect(),
                ),
            );
            m.insert(
                "pluginData".into(),
                c.get("pluginData").cloned().unwrap_or(Value::Null),
            );
            Value::Object(m)
        })
        .collect();
    assert_eq!(Value::Array(characters), case["characters"]);
    println!("OK reader_lines: {} lines, in order", g.len());
}
