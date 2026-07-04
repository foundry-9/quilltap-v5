//! Tier-1 differential (W4.1d batch 3a): the `qtap://` URI codec — exact
//! field-by-field equivalence against the v4 oracle. Diffs parse parts (or the
//! thrown error code + byte-exact message), the re-emitted canonical URI, the
//! resolver triple, `isQtapUri`, `formatQtapUri` from parts, and the producer
//! strings. Exercises the JS→Rust traps: `encodeURIComponent`/`decodeURIComponent`
//! exact character sets, the last-`:` fragment split, BAD_LEVEL bounds, the
//! encoded-slash segment, non-ASCII round-trips, and malformed percent escapes.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   LOG_LEVEL=error npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/qtap-uri.ts \
//!     > /tmp/oracle-qtap-uri.ndjson
//! Run:
//!   QT_ORACLE_QTAP_URI=/tmp/oracle-qtap-uri.ndjson \
//!     cargo test -p quilltap-harness --test qtap_uri_equivalence

use quilltap_core::doc_edit::qtap_uri::{
    format_doc_store_uri, format_qtap_uri, format_scoped_uri, format_self_uri, is_qtap_uri,
    parse_qtap_uri, qtap_uri_to_resolver_input, QtapUriParts, ScopedAuthority,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Row {
    kind: String,
    id: String,
    #[serde(default)]
    input: Value,
    #[serde(rename = "isQtapUri", default)]
    is_qtap_uri: bool,
    #[serde(default)]
    parse: Value,
    #[serde(default)]
    format: Value,
    #[serde(default)]
    result: Value,
}

/// Build the v4 `qtapUriToResolverInput` JSON (`{ scope, mount_point?, path }`).
fn resolver_json(parts: &QtapUriParts) -> Value {
    let (scope, mp, path) = qtap_uri_to_resolver_input(parts);
    let mut obj = serde_json::Map::new();
    obj.insert("scope".into(), json!(scope.as_str()));
    if let Some(mp) = mp {
        obj.insert("mount_point".into(), json!(mp));
    }
    obj.insert("path".into(), json!(path));
    Value::Object(obj)
}

#[test]
fn qtap_uri_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_QTAP_URI") else {
        eprintln!("SKIP: set QT_ORACLE_QTAP_URI to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("oracle line parses");
        match row.kind.as_str() {
            "parse" => {
                let input = row.input.as_str().unwrap();
                assert_eq!(
                    is_qtap_uri(input),
                    row.is_qtap_uri,
                    "isQtapUri mismatch for '{}' ({input:?})",
                    row.id
                );
                let want = &row.parse;
                match parse_qtap_uri(input) {
                    Ok(parts) => {
                        assert_eq!(
                            want["ok"],
                            json!(true),
                            "'{}': expected error, got Ok({:?})",
                            row.id,
                            parts
                        );
                        assert_eq!(
                            serde_json::to_value(&parts).unwrap(),
                            want["parts"],
                            "parse parts mismatch for '{}' ({input:?})",
                            row.id
                        );
                        assert_eq!(
                            json!(format_qtap_uri(&parts)),
                            want["format"],
                            "round-trip format mismatch for '{}' ({input:?})",
                            row.id
                        );
                        assert_eq!(
                            resolver_json(&parts),
                            want["resolver"],
                            "resolver triple mismatch for '{}' ({input:?})",
                            row.id
                        );
                    }
                    Err(e) => {
                        assert_eq!(
                            want["ok"],
                            json!(false),
                            "'{}': expected Ok, got Err({})",
                            row.id,
                            e.message
                        );
                        assert_eq!(
                            json!(e.code.as_str()),
                            want["code"],
                            "error code mismatch for '{}' ({input:?})",
                            row.id
                        );
                        assert_eq!(
                            json!(e.message),
                            want["message"],
                            "error message mismatch for '{}' ({input:?})",
                            row.id
                        );
                    }
                }
            }
            "format" => {
                let parts: QtapUriParts =
                    serde_json::from_value(row.input.clone()).expect("format input parses");
                assert_eq!(
                    json!(format_qtap_uri(&parts)),
                    row.format,
                    "formatQtapUri mismatch for '{}'",
                    row.id
                );
            }
            "producer" => {
                let got = match row.id.as_str() {
                    "scoped-project" => {
                        format_scoped_uri(ScopedAuthority::Project, "Outline.md", None, None)
                    }
                    "scoped-general" => {
                        format_scoped_uri(ScopedAuthority::General, "a/b.md", None, None)
                    }
                    "self-plain" => format_self_uri("Mail/x.md", None, None),
                    "self-heading" => format_self_uri("Backstory.md", Some("Childhood"), Some(2)),
                    "ds-name" => {
                        format_doc_store_uri("My Store", "uuid-1", "a.md", false, None, None)
                    }
                    "ds-ambiguous" => {
                        format_doc_store_uri("My Store", "uuid-1", "a.md", true, None, None)
                    }
                    "ds-reserved-self" => {
                        format_doc_store_uri("self", "uuid-c", "a.md", false, None, None)
                    }
                    "ds-reserved-Project" => {
                        format_doc_store_uri("Project", "uuid-c", "a.md", false, None, None)
                    }
                    "ds-reserved-GENERAL" => {
                        format_doc_store_uri("GENERAL", "uuid-c", "a.md", false, None, None)
                    }
                    "ds-heading" => format_doc_store_uri(
                        "My Store",
                        "uuid-1",
                        "a.md",
                        false,
                        Some("Sec: 1"),
                        Some(2),
                    ),
                    "ds-empty-name" => format_doc_store_uri("", "uuid-e", "a.md", true, None, None),
                    other => panic!("unknown producer case '{other}'"),
                };
                assert_eq!(json!(got), row.result, "producer mismatch for '{}'", row.id);
            }
            other => panic!("unknown row kind '{other}'"),
        }
        count += 1;
    }
    assert!(count >= 40, "expected the full corpus, saw {count} rows");
    eprintln!("qtap_uri: {count} rows matched the oracle.");
}
