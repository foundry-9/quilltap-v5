//! P4.6z `systemBrowseDirectory` differential: `api::system::browse_directory`
//! vs v4's REAL `app/api/v1/system/browse-directory/route.ts` GET handler, over
//! the committed `tests/fixtures/browse-fs-tree/` directory. The route is
//! read-only and DB-free, so both sides point at the SAME committed tree; every
//! absolute occurrence of this side's tree root is sentinel-rewritten to
//! `<BASE>` before the compare (checkout/machine independence — the P4.6v
//! "fs basePath sentinel rewrite" gotcha).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see browse-directory.ts
//! header) then run:
//!   QT_ORACLE_BROWSE_DIR=/tmp/oracle-browse-directory.ndjson \
//!     cargo test -p quilltap-web --test browse_directory_equivalence

use std::path::PathBuf;

use quilltap_core::api::system::browse_directory;
use quilltap_core::api::types::{ErrorKind, Response};
use serde_json::{json, Value};

const SENTINEL: &str = "<BASE>";

fn tree_root() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/browse-fs-tree")
        .to_string_lossy()
        .to_string()
}

/// Map a dispatch `Response` to the `{status, body}` shape the v4 route emits
/// (badRequest → 400 `{error}`; success → 200 `{path, parent, directories(,error)}`).
fn to_status_body(resp: Response) -> (u16, Value) {
    match resp {
        Response::System(v) => (200, v),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                other => panic!("unexpected error kind {other:?}: {}", e.message),
            };
            (status, json!({ "error": e.message }))
        }
        other => panic!("unexpected response {other:?}"),
    }
}

/// Rewrite every occurrence of the tree root → the sentinel (matching the oracle).
fn sentinelize(value: &Value, root: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(root, SENTINEL)),
        Value::Array(a) => Value::Array(a.iter().map(|v| sentinelize(v, root)).collect()),
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            for (k, v) in o {
                m.insert(k.clone(), sentinelize(v, root));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

#[test]
fn browse_directory_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_BROWSE_DIR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_BROWSE_DIR to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut oracle: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        oracle.insert(row["name"].as_str().unwrap().to_string(), row);
    }

    let root = tree_root();
    // (name, absolute path fed to the handler)
    let cases: [(&str, String); 5] = [
        ("list-root", format!("{root}/root")),
        ("nested-cherry", format!("{root}/root/cherry")),
        ("deep", format!("{root}/root/cherry/deep")),
        ("nonexistent", format!("{root}/root/nope")),
        ("file-not-dir", format!("{root}/root/zeta.txt")),
    ];

    let mut checked = 0usize;
    for (name, p) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("no oracle row {name}"));
        let (status, body) = to_status_body(browse_directory(Some(p)));
        let got = json!({ "name": name, "status": status, "body": sentinelize(&body, &root) });
        assert_eq!(
            norm(&got),
            norm(want),
            "case {name}\n{}",
            diff(&norm(&got), &norm(want))
        );
        checked += 1;
    }
    assert_eq!(checked, 5, "expected 5 cases");
    eprintln!("OK: browse-directory matched oracle ({checked} cases).");
}

fn diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}
