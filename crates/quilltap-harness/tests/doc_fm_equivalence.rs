//! Differential: the doc-edit FILE-MANAGEMENT tool handlers (W4.1d batch 3b;
//! v4 `lib/tools/handlers/doc-edit-handler.ts` +
//! `doc-edit/file-management-handlers.ts`), ported as
//! `quilltap_core::tools::doc_edit::file_management`
//! (`doc_move_file` / `doc_copy_file` / `doc_delete_file` / `doc_create_folder` /
//! `doc_delete_folder` / `doc_move_folder`), reached via
//! `execute_doc_edit_tool` + `format_doc_edit_results`.
//!
//! Both sides run the SAME op sequence (`doc-fm.json`) on a fresh copy of the
//! pre-seeded two-database fixture (a transparent character with a full vault + a
//! project-linked SECOND store for the cross-store copy + a corpus of files and
//! folders), in order (state accumulates). Each op's Output +
//! `formatDocEditResults` string are compared against v4's; then three content
//! tables (`doc_mount_file_links` / `doc_mount_folders` / `doc_mount_documents`)
//! plus the MAIN-db `chat_messages` (the W4.6c Librarian move/copy/delete/folder
//! announcement rows) are dumped and diffed.
//!
//! NORMALIZATION: every UUID string is remapped to a positional `<id-N>` token
//! (first-appearance order, one map per compared value) and every ISO-8601
//! timestamp collapsed to `<ts>`, applied identically to both sides. Only
//! `doc_copy_file` mints a fresh integer `mtime` (ms) — move/delete/folder ops
//! return no `mtime` — so that ONE tool's op output `mtime` (top-level + under
//! `result`) is placeholdered on both sides before comparison.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DFM_MAIN=/tmp/qt-dfm-main.db QT_FIXTURE_DFM_MOUNT=/tmp/qt-dfm-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-doc-fm-fixture.ts
//!   (copy the oracle+spec to a temp cases/+fixtures/ outside .claude and --roots there)
//! Run:
//!   QT_ORACLE_DFM=/tmp/oracle-doc-fm.ndjson \
//!   QT_FIXTURE_DFM_MAIN=/tmp/qt-dfm-main.db QT_FIXTURE_DFM_MOUNT=/tmp/qt-dfm-mount.db \
//!     cargo test -p quilltap-harness --test doc_fm_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::db::{dump_table_json_conn, Writer};
use quilltap_core::tools::doc_edit::{
    execute_doc_edit_tool, format_doc_edit_results, post_pending_librarian_announcement_conn,
    DocEditToolContext,
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

/// The only file-management tool that mints a fresh non-deterministic `mtime`
/// (integer ms). Move/delete/folder ops return no `mtime`.
const WRITE_TOOLS: &[&str] = &["doc_copy_file"];

/// Recursively remap every UUID (positional `<id-N>`) and collapse every ISO
/// timestamp to `<ts>`, using one shared map per top-level value. Applied to both
/// sides so a differently-minted value maps to the same token.
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
/// UUIDs, `<ts>` for timestamps). Handles standalone ids AND ids embedded in
/// rendered text.
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

/// For the copy tool, replace any `mtime` key (top-level and under `result`) with
/// a string placeholder — the minted integer ms is non-deterministic and not
/// caught by the ISO normalizer.
fn placeholder_mtime(tool: &str, output: &mut Value) {
    if !WRITE_TOOLS.contains(&tool) {
        return;
    }
    if let Value::Object(o) = output {
        if o.contains_key("mtime") {
            o.insert("mtime".into(), Value::String("<mtime>".into()));
        }
        if let Some(Value::Object(r)) = o.get_mut("result") {
            if r.contains_key("mtime") {
                r.insert("mtime".into(), Value::String("<mtime>".into()));
            }
        }
    }
}

#[test]
fn doc_fm_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DFM") else {
        eprintln!("SKIP: set QT_ORACLE_DFM to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_DFM_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_DFM_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_DFM_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_DFM_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/doc-fm.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (oracle_ops, oracle_dumps) = parse_oracle(&oracle_text);

    // Copy the fixtures and open two writers (both connections held together).
    let scratch = std::env::temp_dir().join(format!("qt-dfm-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("dfm-main.db");
    let work_mount = scratch.join("dfm-mount.db");
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
        // W4.6c: post the Librarian move/copy/delete/folder announcement over the
        // held RW main connection — the executor spine does this async after the
        // write closure; here (driving the sync handler directly) the sync sibling
        // dispatcher stands in.
        if let Some(ann) = &result.pending_librarian_announcement {
            post_pending_librarian_announcement_conn(main.connection(), ann);
        }
        let formatted = format_doc_edit_results(&result);
        let mut output = serde_json::to_value(&result).expect("serialize result");
        placeholder_mtime(&op.tool, &mut output);

        let got = json!({
            "name": op.name,
            "tool": op.tool,
            "output": output,
            "formatted": formatted,
        });
        let got = normalize(&got);

        let mut want = oracle_ops[i].clone();
        if let Some(o) = want.as_object_mut() {
            if let Some(out) = o.get_mut("output") {
                placeholder_mtime(&op.tool, out);
            }
        }
        let want = normalize(&want);

        assert_eq!(
            got, want,
            "op[{i}] {} diverged\n  rust:   {got}\n  oracle: {want}",
            op.name
        );
    }

    // Dump the three content tables (+ the Librarian chat_messages, below) and
    // diff. Order by remap-invariant keys.
    let file_links =
        dump_table_json_conn(mount.connection(), "doc_mount_file_links", "relativePath")
            .expect("dump file_links");
    let folders = dump_table_json_conn(mount.connection(), "doc_mount_folders", "path")
        .expect("dump folders");
    let documents =
        dump_table_json_conn(mount.connection(), "doc_mount_documents", "contentSha256")
            .expect("dump documents");
    // W4.6c: the Librarian move/copy/delete/folder announcement rows live in the
    // MAIN db. Order by `content` — a remap-invariant key (the persona body has no
    // minted uuid/timestamp; each op yields distinct content) so the positional
    // UUID/timestamp remap aligns rows across sides.
    let chat_messages = dump_table_json_conn(main.connection(), "chat_messages", "content")
        .expect("dump chat_messages");

    let got_dumps = normalize(&json!({
        "fileLinks": file_links,
        "folders": folders,
        "documents": documents,
        "chatMessages": chat_messages,
    }));
    let want_dumps = normalize(&oracle_dumps);
    assert_eq!(
        got_dumps, want_dumps,
        "table dumps diverged\n  rust:   {got_dumps}\n  oracle: {want_dumps}"
    );

    drop(main);
    drop(mount);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    eprintln!(
        "OK: doc-fm handlers matched oracle ({} ops + 4 table dumps incl. Librarian chat_messages).",
        spec.ops.len()
    );
}

/// Extract the `ops` array (line 1) + the `dumps` object (line 2) from the oracle
/// NDJSON.
fn parse_oracle(text: &str) -> (Vec<Value>, Value) {
    let mut ops = None;
    let mut dumps = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if let Some(r) = v.get("ops") {
            ops = Some(r.as_array().expect("ops array").clone());
        }
        if let Some(r) = v.get("dumps") {
            dumps = Some(r.clone());
        }
    }
    (
        ops.expect("oracle missing ops"),
        dumps.expect("oracle missing dumps"),
    )
}
