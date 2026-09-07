//! Tier-1 differential: the subprompts PURE helpers (v4 `2f4254b42`,
//! `lib/subprompts/subprompts.ts` — `quilltap_core::subprompts`, P4.D163
//! unit 2).
//!
//! Drives v5's `is_valid_subprompt_id_value`, `slugify_subprompt_title`,
//! `compose_subprompt_content`, `parse_subprompt_content` and
//! `subprompt_path_for_id` over the committed corpus
//! (`harness/oracle/fixtures/subprompts-helpers.json`) and compares, field for
//! field and byte for byte, against the NDJSON v4's REAL exports emitted from
//! the SAME corpus. Coverage is asserted by shape: every corpus row must have
//! an oracle row of its kind + name and vice versa.
//!
//! What the corpus asks that a transcription cannot answer alone: UTF-16
//! `.length` vs code points on the 120-cap (astral ids), the JS `trim`
//! whitespace set (U+00A0 / U+3000 / U+FEFF — Rust's `str::trim` keeps
//! U+FEFF), `normalize('NFKD')` + the U+0300–036F strip (fullwidth, ligatures,
//! Turkish İ vs ı, ß, the Kelvin/Ångström signs), JS `toLowerCase` over the
//! folded form, the 60-byte slice landing on a hyphen and its re-trim, and
//! `parseFrontmatter`'s UTF-16 `bodyStartOffset` on astral files — plus the
//! CRLF file that is NOT frontmatter to either side, and the leading-NBSP body
//! that `replace(/^\n+/, '')` leaves alone while `trimEnd` strips a trailing
//! one.
//!
//! Generate the oracle output (Node 24, from the v4 checkout — a pinned
//! worktree while v4 HEAD is past the baseline; the sweep driver rewrites the
//! `cd`):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/subprompts-helpers.ts \
//!     > /tmp/oracle-subprompts-helpers.ndjson
//! Run:
//!   QT_ORACLE_SUBPROMPTS_HELPERS=/tmp/oracle-subprompts-helpers.ndjson \
//!     cargo test -p quilltap-harness --test subprompts_helpers_equivalence

use std::collections::BTreeMap;
use std::path::PathBuf;

use quilltap_core::subprompts::{
    compose_subprompt_content, is_valid_subprompt_id_value, parse_subprompt_content,
    slugify_subprompt_title, subprompt_path_for_id,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Corpus {
    ids: Vec<IdCase>,
    slugs: Vec<SlugCase>,
    compose: Vec<ComposeCase>,
    parse: Vec<ParseCase>,
    paths: Vec<PathCase>,
}
#[derive(Deserialize)]
struct IdCase {
    name: String,
    input: Value,
}
#[derive(Deserialize)]
struct SlugCase {
    name: String,
    title: String,
}
#[derive(Deserialize)]
struct ComposeCase {
    name: String,
    title: String,
    content: String,
}
#[derive(Deserialize)]
struct ParseCase {
    name: String,
    id: String,
    raw: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}
#[derive(Deserialize)]
struct PathCase {
    name: String,
    id: String,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/subprompts-helpers.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// Minimum row count per kind — a corpus that silently shrank cannot pass.
const FLOORS: [(&str, usize); 5] = [
    ("id", 40),
    ("slug", 30),
    ("compose", 20),
    ("parse", 30),
    ("path", 4),
];

#[test]
fn subprompts_helpers_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_SUBPROMPTS_HELPERS") else {
        return;
    };
    let corpus: Corpus =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("parse corpus");
    let oracle: BTreeMap<(String, String), Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle row");
            (
                (
                    v["kind"].as_str().unwrap().to_string(),
                    v["name"].as_str().unwrap().to_string(),
                ),
                v,
            )
        })
        .collect();
    assert!(
        !oracle.is_empty(),
        "empty oracle file (the empty-file trap)"
    );

    let mut driven: Vec<(String, String)> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut check = |kind: &str, name: &str, got: Value| {
        driven.push((kind.to_string(), name.to_string()));
        let Some(want) = oracle.get(&(kind.to_string(), name.to_string())) else {
            failed.push(format!("{kind}/{name}: MISSING FROM ORACLE"));
            return;
        };
        // Every row is a flat object of strings/bools/values; compare the
        // OUTPUT keys (everything but kind/name and the echoed inputs).
        for (k, wv) in want.as_object().unwrap() {
            if k == "kind" || k == "name" {
                continue;
            }
            let gv = got.get(k).cloned().unwrap_or(Value::Null);
            if gv != *wv {
                failed.push(format!(
                    "{kind}/{name}: `{k}` differs\n    v5: {}\n    v4: {}",
                    serde_json::to_string(&gv).unwrap(),
                    serde_json::to_string(wv).unwrap()
                ));
            }
        }
    };

    for c in &corpus.ids {
        check(
            "id",
            &c.name,
            json!({ "input": c.input, "valid": is_valid_subprompt_id_value(&c.input) }),
        );
    }
    for c in &corpus.slugs {
        check(
            "slug",
            &c.name,
            json!({ "title": c.title, "slug": slugify_subprompt_title(&c.title) }),
        );
    }
    for c in &corpus.compose {
        check(
            "compose",
            &c.name,
            json!({
                "title": c.title,
                "content": c.content,
                "composed": compose_subprompt_content(&c.title, &c.content),
            }),
        );
    }
    for c in &corpus.parse {
        let p = parse_subprompt_content(&c.id, &c.raw, &c.updated_at);
        check(
            "parse",
            &c.name,
            json!({
                "id": c.id,
                "raw": c.raw,
                "updatedAt": c.updated_at,
                "parsed": {
                    "id": p.id, "path": p.path, "title": p.title,
                    "content": p.content, "updatedAt": p.updated_at,
                },
            }),
        );
    }
    for c in &corpus.paths {
        check(
            "path",
            &c.name,
            json!({ "id": c.id, "path": subprompt_path_for_id(&c.id) }),
        );
    }

    // Shape: every oracle row was driven, every corpus row has an oracle row.
    let mut undriven: Vec<&(String, String)> =
        oracle.keys().filter(|k| !driven.contains(k)).collect();
    undriven.sort();
    assert!(
        undriven.is_empty(),
        "oracle rows never driven: {undriven:?}"
    );
    for (kind, floor) in FLOORS {
        let n = driven.iter().filter(|(k, _)| k == kind).count();
        assert!(n >= floor, "kind `{kind}`: {n} rows < floor {floor}");
    }

    assert!(
        failed.is_empty(),
        "{} row(s) differ from v4:\n{}",
        failed.len(),
        failed.join("\n")
    );
    eprintln!(
        "OK: subprompts helpers matched oracle ({} rows).",
        driven.len()
    );
}
