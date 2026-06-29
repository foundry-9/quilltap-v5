//! Tier-1 differential test: the Markdown frontmatter parser + its hand-rolled
//! YAML reader (`parse_frontmatter`).
//!
//! Exact-equality against the v4 oracle (which drives eemeli/yaml's `YAML.parse`).
//! Covers the structural delimiter/offset logic (presence, `---\n`-only opener,
//! CRLF non-recognition, missing close, empty/comments-only → `{}`, array/scalar
//! → null, dup-key → null, UTF-16 offsets incl. a multibyte case) and the YAML
//! 1.2 core-schema subset (scalar resolution, quoting + escapes, comments, flow
//! and block sequences, multi-key + realistic prompt/wardrobe frontmatter).
//!
//! `data` is compared as a structural `serde_json::Value` (key order is
//! irrelevant — the vault reads keys by name), with integer-valued floats
//! canonicalized so `42`/`1.5` compare across the JS-number boundary.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/markdown-frontmatter.ts \
//!     > /tmp/oracle-markdown-frontmatter.ndjson
//! Run:
//!   QT_ORACLE_MARKDOWN_FRONTMATTER=/tmp/oracle-markdown-frontmatter.ndjson \
//!     cargo test -p quilltap-harness --test markdown_frontmatter_equivalence

use quilltap_core::markdown::parse_frontmatter;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    content: String,
    data: Value,
    #[serde(rename = "bodyStartLine")]
    body_start_line: usize,
    #[serde(rename = "bodyStartOffset")]
    body_start_offset: usize,
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64
                {
                    *v = Value::from(f as i64);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

fn canon(mut v: Value) -> Value {
    canon_numbers(&mut v);
    v
}

#[test]
fn frontmatter_parser_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MARKDOWN_FRONTMATTER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MARKDOWN_FRONTMATTER to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        let got = parse_frontmatter(&row.content);
        let got_data = got.data.unwrap_or(Value::Null);
        assert_eq!(
            canon(got_data),
            canon(row.data),
            "[{}] frontmatter data",
            row.id
        );
        assert_eq!(
            got.body_start_line, row.body_start_line,
            "[{}] bodyStartLine",
            row.id
        );
        assert_eq!(
            got.body_start_offset, row.body_start_offset,
            "[{}] bodyStartOffset",
            row.id
        );
        n += 1;
    }
    assert!(n >= 52, "expected the full corpus, saw {n} rows");
    eprintln!("OK: frontmatter parser matched oracle on {n} cases.");
}
