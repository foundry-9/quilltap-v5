//! Differential: the doc-edit DOCUMENT-UI tool handlers (W4.1d batch 3b) — v4
//! `lib/tools/handlers/doc-edit/document-ui-handlers.ts` (`doc_open_document` /
//! `doc_close_document` / `doc_focus`), ported as
//! `quilltap_core::tools::doc_edit::document_ui`.
//!
//! Both sides run the SAME op sequence (`doc-ui.json`) on a fresh copy of the
//! pre-seeded two-database fixture (a transparent character with a full vault + a
//! project + a chat with `documentMode: 'normal'`), in order (state accumulates —
//! open A, open B, focus×4, close×3). Each op's Output + `formatDocEditResults`
//! string are compared against v4's; then `chat_documents` (all cols) + a `chats`
//! subset (`id` / `documentMode` / `updatedAt`) + `chat_messages` (the Librarian
//! open-announcement rows, W4.6c) are dumped and diffed. The `chats` dump PROVES
//! the documentMode transitions (normal → split on open, back to normal on the last
//! close). NOTE (W4.6c): the open announcement is now LIVE — posting it is an
//! actual `type:'message'` event, so it DOES bump the chat's `updatedAt` (on both
//! sides identically); the earlier "updatedAt never bumped by open/close" pin is
//! therefore gone (the announcement legitimately bumps it, and the `chat_messages`
//! dump proves the announcement posted on both sides byte-for-byte).
//!
//! NORMALIZATION: every UUID string is remapped to a positional `<id-N>` token
//! (first-appearance order, one map per compared value) and every ISO-8601
//! timestamp collapsed to `<ts>`, applied identically to both sides. The open ops
//! mint a fresh integer `mtime` (ms) the ISO normalizer won't catch, so the
//! `mtime` key in the open output is replaced with `"<mtime>"` on both sides.
//!
//! Regenerate the fixture + oracle (Node 24, from the v4 checkout). STAGE the case
//! OUTSIDE `.claude/` — v4's jest ignores those paths in BOTH testPathIgnorePatterns
//! and modulePathIgnorePatterns, so a `--roots` into a worktree matches ZERO tests,
//! leaves the previous NDJSON in place, and the family then passes against a stale
//! oracle. That is how the P4.32 stale-RED stayed invisible; the recipe below runs
//! verbatim (`harness/tools/recipe_sweep.py --run doc_ui_equivalence`).
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   STAGE=/tmp/qt-oracle-stage-doc-ui
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $W/harness/oracle/cases/doc-ui.test.ts $STAGE/harness/oracle/cases/
//!   cp $W/harness/oracle/fixtures/doc-ui.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server        # or a worktree pinned at the baseline
//!   QT_FIXTURE_DUI_MAIN=/tmp/qt-dui-main.db QT_FIXTURE_DUI_MOUNT=/tmp/qt-dui-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-doc-ui-fixture.ts
//!   QT_FIXTURE_DUI_MAIN=/tmp/qt-dui-main.db QT_FIXTURE_DUI_MOUNT=/tmp/qt-dui-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-doc-ui.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-ui\.test\.ts$"
//!
//! The fixture pair is NOT committed: the builder MINTS it (fresh UUIDs every run),
//! so the oracle and `cargo test` must both point at the SAME build — rebuild, then
//! regenerate, then run, in that order. The jest filter is ANCHORED so a sibling
//! `doc-f*` case can never be picked up silently (`oracle-regen-silent-stale-pass`).
//! Run:
//!   QT_ORACLE_DUI=/tmp/oracle-doc-ui.ndjson \
//!   QT_FIXTURE_DUI_MAIN=/tmp/qt-dui-main.db QT_FIXTURE_DUI_MOUNT=/tmp/qt-dui-mount.db \
//!     cargo test -p quilltap-harness --test doc_ui_equivalence -- --nocapture

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

/// The length of an ISO-8601 `YYYY-MM-DDTHH:MM:SS.sssZ` timestamp at `b[0]`.
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

/// Replace the open op's minted integer `mtime` (top-level + nested under
/// `result`) with a `"<mtime>"` placeholder — the minted ms is non-deterministic
/// and not caught by the ISO normalizer. Only `doc_open_document` mints one.
fn placeholder_mtime(tool: &str, output: &mut Value) {
    if tool != "doc_open_document" {
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
fn doc_ui_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DUI") else {
        eprintln!("SKIP: set QT_ORACLE_DUI to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_DUI_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_DUI_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_DUI_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_DUI_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/doc-ui.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (oracle_ops, oracle_dumps) = parse_oracle(&oracle_text);

    // Copy the fixtures and open two writers (both connections held together).
    let scratch = std::env::temp_dir().join(format!("qt-dui-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("dui-main.db");
    let work_mount = scratch.join("dui-mount.db");
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
        files_dir: None,
    };

    for (i, op) in spec.ops.iter().enumerate() {
        let result = execute_doc_edit_tool(
            main.connection(),
            mount.connection(),
            &op.tool,
            &op.args,
            &ctx,
        );
        // W4.6c: post the Librarian open announcement over the held RW main
        // connection (the sync sibling of the executor's async post). Only
        // `doc_open_document` produces one.
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

    // Dump chat_documents (all cols) + the chats subset. Order chat_documents by
    // the stable relativePath-equivalent (filePath) so the positional-UUID remap
    // assigns identical tokens on both sides; chats by id.
    let chat_documents =
        dump_table_json_conn(main.connection(), "chat_documents", "filePath").expect("dump cd");
    let chats_full = dump_table_json_conn(main.connection(), "chats", "id").expect("dump chats");
    let chats = project_chats_subset(&chats_full);
    // W4.6c: the Librarian open-announcement rows live in the MAIN db. Order by
    // `content` — remap-invariant (no minted uuid/timestamp in the persona body).
    let chat_messages = dump_table_json_conn(main.connection(), "chat_messages", "content")
        .expect("dump chat_messages");

    let got_dumps = json!({
        "chatDocuments": chat_documents,
        "chats": chats,
        "chatMessages": chat_messages,
    });

    let got_dumps = normalize(&got_dumps);
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
        "OK: doc-ui handlers matched oracle ({} ops + chat_documents/chats/chat_messages dumps).",
        spec.ops.len()
    );
}

/// Project a full `chats` dump (from [`dump_table_json_conn`]) down to the
/// `id` / `documentMode` / `updatedAt` subset the oracle emits, keeping the same
/// `{ table, columns, rows }` shape and the id ordering (the full dump is already
/// ordered by `id`).
fn project_chats_subset(full: &Value) -> Value {
    let rows: Vec<Value> = full
        .get("rows")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("chats dump has no rows"))
        .iter()
        .map(|row| {
            let pick = |k: &str| row.get(k).cloned().unwrap_or(Value::Null);
            json!({
                "id": pick("id"),
                "documentMode": pick("documentMode"),
                "updatedAt": pick("updatedAt"),
            })
        })
        .collect();
    json!({
        "table": "chats",
        "columns": ["id", "documentMode", "updatedAt"],
        "rows": rows,
    })
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
