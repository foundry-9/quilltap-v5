//! Differential: the doc-edit TEXT/MARKDOWN tool handlers (W4.1d batch 3b — the
//! FIRST doc-edit differential; v4 `lib/tools/handlers/doc-edit-handler.ts` +
//! `doc-edit/{text,markdown}-handlers.ts`), ported as
//! `quilltap_core::tools::doc_edit::{execute_doc_edit_tool,
//! format_doc_edit_results}`.
//!
//! Both sides run the SAME op sequence (`doc-text.json`) on a fresh copy of the
//! pre-seeded two-database fixture (a character with a full vault + a project with
//! an official store + a corpus of text/json/markdown files + a chat with the
//! character as a participant), in order (state accumulates — the insert.md ops
//! chain). Each op's Output + `formatDocEditResults` string are compared against
//! v4's; then three tables are dumped and diffed: the two mount-index content
//! tables (`doc_mount_documents` / `doc_mount_file_links`) plus the MAIN-db
//! `chat_messages`, which holds the Group 6 Librarian doc-save write announcements
//! (`change:{diff}`/`{created,body}`) each mutating write handler now emits. The
//! Rust port threads the announcement out of the sync write closure and posts it
//! via `post_librarian_write_announcement_conn` (the executor spine posts the async
//! sibling); the v4 oracle drives the REAL `postLibrarianWriteAnnouncement`.
//!
//! NORMALIZATION: every UUID string is remapped to a positional `<id-N>` token
//! (first-appearance order, one map per compared value) and every ISO-8601
//! timestamp collapsed to `<ts>`, applied identically to both sides. Write ops
//! mint a fresh integer `mtime` (ms) that the ISO normalizer won't catch, so for
//! the five write tools the op output's `mtime` (and nested `result.mtime`) is
//! replaced with a `"<mtime>"` placeholder on both sides before comparison
//! (`doc_read_file`'s mtime is deterministic — both read the same seeded file — so
//! it is NOT placeholdered, proving read-mtime correctness). The V8 `JSON.parse`
//! message text (op `write_json_invalid`) is a documented normalized seam →
//! collapsed to `<ERR>` on both sides.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DT_MAIN=/tmp/qt-dt-main.db QT_FIXTURE_DT_MOUNT=/tmp/qt-dt-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-doc-text-fixture.ts
//!   QT_FIXTURE_DT_MAIN=/tmp/qt-dt-main.db QT_FIXTURE_DT_MOUNT=/tmp/qt-dt-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-doc-text.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- doc-text
//! Run:
//!   QT_ORACLE_DT=/tmp/oracle-doc-text.ndjson \
//!   QT_FIXTURE_DT_MAIN=/tmp/qt-dt-main.db QT_FIXTURE_DT_MOUNT=/tmp/qt-dt-mount.db \
//!     cargo test -p quilltap-harness --test doc_text_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::db::{dump_table_json_conn, Writer};
use quilltap_core::tools::doc_edit::post_pending_librarian_announcement_conn;
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

/// The five write tools that mint a fresh non-deterministic `mtime` (integer ms).
const WRITE_TOOLS: &[&str] = &[
    "doc_write_file",
    "doc_str_replace",
    "doc_insert_text",
    "doc_update_frontmatter",
    "doc_update_heading",
];

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
            // Copy one UTF-8 char.
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
    // Not part of a longer hex run (avoid clipping).
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

/// For write tools, replace any `mtime` key (top-level and under `result`) with a
/// string placeholder — the minted integer ms is non-deterministic and not caught
/// by the ISO normalizer.
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

/// For write tools, collapse the minted `mtime: <digits>` that the human
/// `formattedText` strings embed to `mtime: <mtime>` (the structured `mtime` key is
/// placeholdered separately by [`placeholder_mtime`]). Walks every string in the
/// op record.
fn collapse_mtime_strings(v: &mut Value) {
    fn collapse(s: &str) -> String {
        let needle = "mtime: ";
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(pos) = rest.find(needle) {
            out.push_str(&rest[..pos + needle.len()]);
            let after = &rest[pos + needle.len()..];
            let digits: usize = after.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 {
                out.push_str("<mtime>");
                rest = &after[digits..];
            } else {
                rest = after;
            }
        }
        out.push_str(rest);
        out
    }
    match v {
        Value::String(s) => *s = collapse(s),
        Value::Array(a) => a.iter_mut().for_each(collapse_mtime_strings),
        Value::Object(o) => o.values_mut().for_each(collapse_mtime_strings),
        _ => {}
    }
}

/// Collapse a `Invalid JSON: <v8 message>` / `Invalid JSONL: ...` /
/// `Parse Error: ...` tail to `<ERR>` (the documented V8 `JSON.parse` message
/// seam). Applied to the `error` + `formatted` fields of every op record so the
/// message text divergence never fails the diff (structure/prefix still checked).
fn collapse_json_errors(v: &mut Value) {
    fn collapse_str(s: &str) -> Option<String> {
        for prefix in ["Invalid JSONL: ", "Invalid JSON: ", "Parse Error: "] {
            if let Some(pos) = s.find(prefix) {
                return Some(format!("{}{prefix}<ERR>", &s[..pos]));
            }
        }
        None
    }
    match v {
        Value::String(s) => {
            if let Some(rep) = collapse_str(s) {
                *s = rep;
            }
        }
        Value::Array(a) => a.iter_mut().for_each(collapse_json_errors),
        Value::Object(o) => o.values_mut().for_each(collapse_json_errors),
        _ => {}
    }
}

#[test]
fn doc_text_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_DT") else {
        eprintln!("SKIP: set QT_ORACLE_DT to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_DT_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_DT_MAIN to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_DT_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_DT_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/doc-text.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (oracle_ops, oracle_dumps) = parse_oracle(&oracle_text);

    // Copy the fixtures and open two writers (both connections held together).
    let scratch = std::env::temp_dir().join(format!("qt-dt-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("dt-main.db");
    let work_mount = scratch.join("dt-mount.db");
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
        // Group 6: post the Librarian doc-save announcement over the held RW main
        // connection — the executor spine does this async after the write closure;
        // here (driving the sync handler directly) the sync sibling poster stands in.
        if let Some(ann) = &result.pending_librarian_announcement {
            post_pending_librarian_announcement_conn(main.connection(), ann);
        }
        let formatted = format_doc_edit_results(&result);
        let mut output = serde_json::to_value(&result).expect("serialize result");
        placeholder_mtime(&op.tool, &mut output);

        let mut got = json!({
            "name": op.name,
            "tool": op.tool,
            "output": output,
            "formatted": formatted,
        });
        collapse_json_errors(&mut got);
        if WRITE_TOOLS.contains(&op.tool.as_str()) {
            collapse_mtime_strings(&mut got);
        }
        let got = normalize(&got);

        let mut want = oracle_ops[i].clone();
        // Placeholder the oracle's minted mtime the same way.
        if let Some(o) = want.as_object_mut() {
            if let Some(out) = o.get_mut("output") {
                placeholder_mtime(&op.tool, out);
            }
        }
        collapse_json_errors(&mut want);
        if WRITE_TOOLS.contains(&op.tool.as_str()) {
            collapse_mtime_strings(&mut want);
        }
        let want = normalize(&want);

        assert_eq!(
            got, want,
            "op[{i}] {} diverged\n  rust:   {got}\n  oracle: {want}",
            op.name
        );
    }

    // Dump the two content tables and diff. Order by remap-invariant keys (a
    // content-derived sha / the stable relativePath) so the positional-UUID remap
    // assigns identical tokens on both sides.
    let documents =
        dump_table_json_conn(mount.connection(), "doc_mount_documents", "contentSha256")
            .expect("dump documents");
    let file_links =
        dump_table_json_conn(mount.connection(), "doc_mount_file_links", "relativePath")
            .expect("dump file_links");
    // P4.32: the chunk ROWS themselves, ordered by the remap-invariant `content`.
    let chunks = dump_table_json_conn(mount.connection(), "doc_mount_chunks", "content")
        .expect("dump chunks");
    // The Group 6 Librarian write-announcement rows live in the MAIN db. Order by
    // `content` (remap-invariant — no minted uuid/timestamp in the persona body).
    let chat_messages = dump_table_json_conn(main.connection(), "chat_messages", "content")
        .expect("dump chat_messages");

    let got_raw = json!({
        "documents": documents,
        "fileLinks": file_links,
        "chunks": chunks,
        "chatMessages": chat_messages,
    });
    let got_dumps = normalize(&got_raw);
    assert_chunk_pass_ran(&got_raw, &oracle_dumps);
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
        "OK: doc-text handlers matched oracle ({} ops + 4 table dumps incl. doc_mount_chunks + Librarian chat_messages).",
        spec.ops.len()
    );
}

/// The database-store paths this corpus WRITES — every one of which the chunk pass
/// must leave chunked. Deliberately NOT "any link with `chunkCount > 0`: the fixture
/// seeds four already-chunked vault files (`metadata.json` / `properties.json` /
/// `state.json` / `physical-prompts.json`), and an "at least one nonzero" arm sailed
/// through on those alone while the ops chunked nothing at all — the mutation proof
/// caught exactly that. These are the rows that moved 0 -> 1 the moment the oracle's
/// `reindexSingleFile` mock came off (P4.32).
const OP_CHUNKED_PATHS: &[&str] = &[
    "Notes/accent.md",
    "Notes/created.md",
    "Notes/edit.md",
    "Notes/fmrep.md",
    "Notes/fmupd.md",
    "Notes/headupd.md",
    "Notes/insert.md",
    "data/out.json",
];

/// P4.32 — the POSITIVE chunk-pass arm.
///
/// The dump equality below is an EQUALITY: it passes just as happily with
/// `chunkCount` 0 on both sides, which is precisely the state that held while the
/// oracle mocked `reindexSingleFile` away — v4's own P4.6BK chunk-on-write silenced
/// alongside the tool-level reindex seam it was aimed at. (What that concealed was
/// the opposite: v5 chunked, v4 did not, and the family sat stale-RED.) So assert
/// the pass actually RAN and left rows behind:
///
///   1. per-path `chunkCount` agrees side-for-side;
///   2. every path in [`OP_CHUNKED_PATHS`] carries a NONZERO count on BOTH sides —
///      an all-zero equality must not pass silently;
///   3. the `doc_mount_chunks` rows physically present equal the sum of the
///      rollups, checked on each side independently — so a bumped counter with no
///      chunk row behind it fails.
///
/// Mutation-proven: suppressing `insert_chunks` in `services::mount_index::
/// reindex_file` fires (3); zeroing the chunk vector on both sides fires (2).
fn assert_chunk_pass_ran(got: &Value, want: &Value) {
    fn num(v: &Value) -> i64 {
        v.as_i64()
            .or_else(|| v.as_f64().map(|f| f as i64))
            .unwrap_or(0)
    }
    fn counts(dumps: &Value) -> Vec<(String, i64)> {
        dumps["fileLinks"]["rows"]
            .as_array()
            .expect("fileLinks rows")
            .iter()
            .map(|r| {
                (
                    r["relativePath"].as_str().unwrap_or_default().to_string(),
                    num(&r["chunkCount"]),
                )
            })
            .collect()
    }
    fn chunk_rows(dumps: &Value) -> i64 {
        dumps["chunks"]["rows"]
            .as_array()
            .expect("chunks rows")
            .len() as i64
    }
    fn lookup(counts: &[(String, i64)], path: &str) -> i64 {
        counts
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, n)| *n)
            .unwrap_or_else(|| {
                panic!("pinned path `{path}` has no doc_mount_file_links row — the corpus moved; re-pin OP_CHUNKED_PATHS")
            })
    }

    let got_counts = counts(got);
    let want_counts = counts(want);
    assert_eq!(
        got_counts, want_counts,
        "per-path chunkCount diverged\n  rust:   {got_counts:?}\n  oracle: {want_counts:?}"
    );

    for path in OP_CHUNKED_PATHS {
        let (g, w) = (lookup(&got_counts, path), lookup(&want_counts, path));
        assert!(
            g > 0 && w > 0,
            "the chunk pass did NOT run for the written path `{path}`: chunkCount rust={g} \
             oracle={w}. An all-zero equality is exactly what the mocked oracle used to pass \
             with — see P4.32."
        );
    }

    let total: i64 = got_counts.iter().map(|(_, n)| *n).sum();
    assert_eq!(
        chunk_rows(got),
        total,
        "rust: doc_mount_chunks row count != sum(chunkCount)"
    );
    assert_eq!(
        chunk_rows(want),
        total,
        "oracle: doc_mount_chunks row count != sum(chunkCount)"
    );
    eprintln!(
        "chunk pass proven: {} written path(s) pinned nonzero, {total} chunk row(s), equal on both sides.",
        OP_CHUNKED_PATHS.len()
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
