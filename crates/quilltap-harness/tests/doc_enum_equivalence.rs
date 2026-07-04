//! Differential: the doc-edit ENUMERATION tool handlers (`doc_grep` /
//! `doc_list_files`; v4 `lib/tools/handlers/doc-edit/text-handlers.ts`
//! `handleGrep` / `handleListFiles`), ported as `execute_doc_edit_tool` /
//! `format_doc_edit_results` in `quilltap_core::tools::doc_edit`.
//!
//! Both sides run the SAME op sequence (`doc-enum.json`) on a fresh copy of the
//! pre-seeded two-database fixture (a character with a full vault + a corpus of
//! text/markdown files, a subfolder, a `character_read:false` blocked file, an
//! auto-image + an OS-cruft file, a project with an official store). grep +
//! doc_list_files are READ-ONLY, so nothing mutates — each op's Output +
//! `formatDocEditResults` string are compared against v4's; there is NO table dump.
//!
//! DETERMINISM: grep matches carry stable `path`/`mount_point`/`uri`/`line_number`/
//! `match` (+ optional context) — no minted values. list files carry a `modified`
//! integer (`new Date(link.lastModified).getTime()`), which is baked into the
//! shared fixture and read identically by both sides, so it is compared EXACTLY
//! (proving list-modified correctness). The `uri` values are `qtap://self/…` /
//! `qtap://<store>/…` / `qtap://project/…` — stable text, no UUIDs. The shared
//! `normalize` (positional UUID remap + ISO-timestamp collapse, from the doc-text
//! differential) is therefore a no-op on these values but is retained verbatim so
//! any stray minted id/timestamp would surface identically on both sides.
//!
//! ORDERING: matches[] / files[] follow v4's mount-iteration order
//! (`getAccessibleMountPoints`) then per-mount DB row order — the ported
//! `resolve_tiered_mount_pool` + the SQL natural order. The arrays are compared
//! AS-IS (unsorted); a genuine enumeration divergence would be a real bug.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DEN_MAIN=/tmp/qt-den-main.db QT_FIXTURE_DEN_MOUNT=/tmp/qt-den-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-doc-enum-fixture.ts
//!   QT_FIXTURE_DEN_MAIN=/tmp/qt-den-main.db QT_FIXTURE_DEN_MOUNT=/tmp/qt-den-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-doc-enum.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- doc-enum
//! Run:
//!   QT_ORACLE_DEN=/tmp/oracle-doc-enum.ndjson \
//!   QT_FIXTURE_DEN_MAIN=/tmp/qt-den-main.db QT_FIXTURE_DEN_MOUNT=/tmp/qt-den-mount.db \
//!     cargo test -p quilltap-harness --test doc_enum_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::db::Writer;
use quilltap_core::tools::doc_edit::{
    execute_doc_edit_tool, format_doc_edit_results, DocEditToolContext,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    tool: String,
    args: Value,
}

/// Recursively remap every UUID (positional `<id-N>`) and collapse every ISO
/// timestamp to `<ts>`, using one shared map per top-level value. Applied to both
/// sides so a differently-minted value maps to the same token. (A no-op on the
/// grep/list output here — kept verbatim from the doc-text differential.)
fn normalize(v: &Value) -> Value {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut counter = 0usize;
    walk(v, &mut map, &mut counter)
}

fn walk(v: &Value, map: &mut HashMap<String, String>, counter: &mut usize) -> Value {
    match v {
        Value::String(s) => Value::String(normalize_text(s, map, counter)),
        Value::Array(a) => Value::Array(a.iter().map(|e| walk(e, map, counter)).collect()),
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            for (k, val) in o {
                m.insert(k.clone(), walk(val, map, counter));
            }
            Value::Object(m)
        }
        other => other.clone(),
    }
}

/// Replace UUID / ISO-timestamp substrings inside a string (positional tokens for
/// UUIDs, `<ts>` for timestamps).
fn normalize_text(s: &str, map: &mut HashMap<String, String>, counter: &mut usize) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some(len) = iso_ts_len(&bytes[i..]) {
            out.push_str("<ts>");
            i += len;
        } else if uuid_at(&bytes[i..]) {
            let raw = &s[i..i + 36];
            let token = map
                .entry(raw.to_string())
                .or_insert_with(|| {
                    let t = format!("<id-{}>", *counter);
                    *counter += 1;
                    t
                })
                .clone();
            out.push_str(&token);
            i += 36;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// A UUID (`8-4-4-4-12` hex) starting at `b[0]`.
fn uuid_at(b: &[u8]) -> bool {
    if b.len() < 36 {
        return false;
    }
    let dashes = [8, 13, 18, 23];
    for (i, &c) in b[..36].iter().enumerate() {
        if dashes.contains(&i) {
            if c != b'-' {
                return false;
            }
        } else if !is_hex(c) {
            return false;
        }
    }
    if b.len() > 36 && (is_hex(b[36]) || b[36] == b'-') {
        return false;
    }
    true
}

/// The length of an ISO-8601 `YYYY-MM-DDTHH:MM:SS.sssZ` timestamp at `b[0]`, else
/// `None`.
fn iso_ts_len(b: &[u8]) -> Option<usize> {
    const LEN: usize = 24;
    if b.len() < LEN {
        return None;
    }
    let d = |i: usize| b[i].is_ascii_digit();
    let ok = d(0)
        && d(1)
        && d(2)
        && d(3)
        && b[4] == b'-'
        && d(5)
        && d(6)
        && b[7] == b'-'
        && d(8)
        && d(9)
        && b[10] == b'T'
        && d(11)
        && d(12)
        && b[13] == b':'
        && d(14)
        && d(15)
        && b[16] == b':'
        && d(17)
        && d(18)
        && b[19] == b'.'
        && d(20)
        && d(21)
        && d(22)
        && b[23] == b'Z';
    if ok {
        Some(LEN)
    } else {
        None
    }
}

#[test]
fn doc_enum_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DEN") else {
        eprintln!("SKIP: set QT_ORACLE_DEN to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_DEN_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_DEN_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_DEN_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_DEN_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/doc-enum.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle_ops = parse_oracle(&oracle_text);
    assert_eq!(
        oracle_ops.len(),
        spec.ops.len(),
        "oracle op count {} != spec op count {}",
        oracle_ops.len(),
        spec.ops.len()
    );

    // Copy the fixtures and open two writers (both connections held together).
    let scratch = std::env::temp_dir().join(format!("qt-den-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("den-main.db");
    let work_mount = scratch.join("den-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let main = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");

    let ctx = DocEditToolContext {
        chat_id: spec.chat_id.clone(),
        user_id: spec.user_id.clone(),
        project_id: Some(spec.project_id.clone()),
        character_id: Some(spec.character_id.clone()),
        operator_override: false,
    };

    for (i, op) in spec.ops.iter().enumerate() {
        let result = execute_doc_edit_tool(
            main.connection(),
            mount.connection(),
            &op.tool,
            &op.args,
            &ctx,
        );
        let formatted = format_doc_edit_results(&result);
        let output = serde_json::to_value(&result).expect("serialize result");

        let got = normalize(&json!({
            "name": op.name,
            "tool": op.tool,
            "output": output,
            "formatted": formatted,
        }));
        let want = normalize(&oracle_ops[i]);

        assert_eq!(
            got, want,
            "op[{i}] {} diverged\n  rust:   {got}\n  oracle: {want}",
            op.name
        );
    }

    drop(main);
    drop(mount);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    eprintln!(
        "OK: doc-enum handlers matched oracle ({} ops).",
        spec.ops.len()
    );
}

/// Extract the `ops` array from the single-line oracle NDJSON.
fn parse_oracle(text: &str) -> Vec<Value> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if let Some(r) = v.get("ops") {
            return r.as_array().expect("ops array").clone();
        }
    }
    panic!("oracle missing ops");
}
