//! Differential: the doc-edit HOST-FILESYSTEM tool branches (P4.6bg unit 4). Both
//! sides run the SAME op sequence (`doc-fs.json`) — general scope, filesystem-backed
//! mounts, the legacy `<filesDir>/<projectId>` project fallback, and the new-blank
//! document path — through v4's REAL `executeDocEditTool` (oracle) vs
//! `quilltap_core::tools::doc_edit::execute_doc_edit_tool` (Rust), over a copy of
//! the doc-fs fixture DBs + an IDENTICAL temp fs tree both sides materialize under a
//! CANONICAL scratch root. The fs-mount store's sentinel basePath is rewritten to
//! `<scratch>/mount`; the resolver is given `files_dir = Some(<scratch>/files)`.
//!
//! After the ops it diffs each op's Output + `formatDocEditResults` string, then the
//! resulting fs tree (files/ + mount/ + outside/, byte-for-byte) and the empty
//! doc_mount_documents / doc_mount_file_links (fs ops never write DB rows).
//!
//! NORMALIZATION (applied identically to both sides): every minted UUID → a
//! positional `<id-N>` token and every ISO timestamp → `<ts>`; every fs-derived
//! `mtime`/`modified` → `<mtime>` (on-disk/minted, non-deterministic); the tree is
//! sorted by a UUID-collapsed key (new-blank filenames are random). `doc_list_files`
//! / `doc_grep` walk directories in OS-dependent `readdir` order, so their result
//! arrays are sorted and the human `formatted` string (which keeps walk order) is
//! dropped — the structured entry SET + contents are what the diff asserts. The
//! Librarian announcements are a documented seam: mocked to no-ops in the oracle and
//! NOT posted here, so `chat_messages` stays untouched (not dumped).
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DFS_MAIN=/tmp/qt-dfs-main.db QT_FIXTURE_DFS_MOUNT=/tmp/qt-dfs-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-doc-fs-fixture.ts
//!   QT_FIXTURE_DFS_MAIN=/tmp/qt-dfs-main.db QT_FIXTURE_DFS_MOUNT=/tmp/qt-dfs-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-doc-fs.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- doc-fs
//! Run:
//!   QT_ORACLE_DFS=/tmp/oracle-doc-fs.ndjson \
//!   QT_FIXTURE_DFS_MAIN=/tmp/qt-dfs-main.db QT_FIXTURE_DFS_MOUNT=/tmp/qt-dfs-mount.db \
//!     cargo test -p quilltap-harness --test doc_fs_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::{dump_table_json_conn, Writer};
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
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "legacyProjectId")]
    legacy_project_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    tool: String,
    args: Value,
    ctx: OpCtx,
}

#[derive(Deserialize, Default)]
struct OpCtx {
    #[serde(rename = "characterId", default)]
    character_id: Option<String>,
    #[serde(rename = "projectId", default)]
    project_id: Option<String>,
    #[serde(rename = "operatorOverride", default)]
    operator_override: bool,
}

/// Build the host-filesystem tree both sides materialize identically under a
/// CANONICAL scratch root (mirrors the oracle's `materializeTree`). Returns
/// `<root>/mount`, the fs-mount base.
fn materialize_tree(root: &Path, legacy_project_id: &str) -> PathBuf {
    let general = root.join("files").join("_general");
    let legacy = root.join("files").join(legacy_project_id);
    let mount = root.join("mount");
    let outside = root.join("outside");
    std::fs::create_dir_all(&general).unwrap();
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::create_dir_all(mount.join("docs")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(general.join("existing.md"), "# existing general\n").unwrap();
    std::fs::write(legacy.join("draft.md"), "# draft\n\ndraft body\n").unwrap();
    std::fs::write(mount.join("docs").join("note.md"), "# note\n\nnote body\n").unwrap();
    std::fs::write(outside.join("secret.md"), "secret\n").unwrap();
    std::os::unix::fs::symlink(&outside, mount.join("escape")).unwrap();
    mount
}

/// Collapse every UUID to a constant so the tree sort order is remap-invariant.
fn collapse_uuids(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if uuid_at(&b[i..]) {
            out.push_str("<uuid>");
            i += 36;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Recursively dump a directory tree into {path, kind, content?} entries, sorted by
/// a UUID-collapsed key (matches the oracle's `dumpTree`).
fn dump_tree(root: &Path, label: &str) -> Vec<Value> {
    fn walk(dir: &Path, root: &Path, label: &str, out: &mut Vec<Value>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.filter_map(Result::ok) {
            let full = e.path();
            let rel = format!(
                "{label}/{}",
                full.strip_prefix(root).unwrap().to_string_lossy()
            );
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                out.push(json!({ "path": rel, "kind": "symlink" }));
            } else if ft.is_dir() {
                out.push(json!({ "path": rel, "kind": "dir" }));
                walk(&full, root, label, out);
            } else if ft.is_file() {
                let content = std::fs::read_to_string(&full).unwrap_or_default();
                out.push(json!({ "path": rel, "kind": "file", "content": content }));
            }
        }
    }
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, label, &mut out);
    }
    out.sort_by(|a, b| {
        let ak = collapse_uuids(a["path"].as_str().unwrap_or(""));
        let bk = collapse_uuids(b["path"].as_str().unwrap_or(""));
        ak.cmp(&bk)
    });
    out
}

// ── normalization (shared with the other doc-edit differentials) ──

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
        } else if !c.is_ascii_hexdigit() {
            return false;
        }
    }
    if b.len() > 36 && (b[36].is_ascii_hexdigit() || b[36] == b'-') {
        return false;
    }
    true
}

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

/// Replace every `mtime`/`modified` value (recursively) with a `<mtime>`
/// placeholder — on-disk/minted times are non-deterministic.
fn placeholder_fs_times(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, val) in o.iter_mut() {
                if (k == "mtime" || k == "modified") && (val.is_number() || val.is_string()) {
                    *val = Value::String("<mtime>".into());
                } else {
                    placeholder_fs_times(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(placeholder_fs_times),
        _ => {}
    }
}

/// Collapse the minted `mtime: <digits>` a `formatted` string embeds to
/// `mtime: <mtime>`.
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

/// The listing/grep tools walk directories in OS-dependent `readdir` order. Sort the
/// structured result arrays by a stable key and DROP the `formatted` string (whose
/// line order follows the walk) — the entry SET + contents are asserted, the order
/// is a documented readdir seam.
fn normalize_walk_op(tool: &str, record: &mut Value) {
    let Some(o) = record.as_object_mut() else {
        return;
    };
    let array_key = match tool {
        "doc_list_files" => "files",
        "doc_grep" => "matches",
        _ => return,
    };
    if let Some(arr) = o
        .get_mut("output")
        .and_then(|out| out.get_mut("result"))
        .and_then(|r| r.get_mut(array_key))
        .and_then(Value::as_array_mut)
    {
        arr.sort_by_key(sort_key);
    }
    // The human formatted string (top-level + the result's own `formattedText`)
    // follows the walk order — drop it; the sorted entry SET is what we assert.
    o.insert("formatted".into(), Value::Null);
    if let Some(out) = o.get_mut("output").and_then(Value::as_object_mut) {
        if out.contains_key("formattedText") {
            out.insert("formattedText".into(), Value::Null);
        }
    }
}

/// A stable sort key for a listing/grep entry: `path` + line number.
fn sort_key(entry: &Value) -> String {
    let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
    let line = entry
        .get("line_number")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    format!("{path}\u{0}{line:012}")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see header).");
            None
        }
    }
}

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

#[test]
fn doc_fs_matches_oracle() {
    let (Some(oracle_path), Some(fixture_main), Some(fixture_mount)) = (
        env_or_skip("QT_ORACLE_DFS"),
        env_or_skip("QT_FIXTURE_DFS_MAIN"),
        env_or_skip("QT_FIXTURE_DFS_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/doc-fs.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (oracle_ops, oracle_dumps) = parse_oracle(&oracle_text);

    let pid = std::process::id();
    let scratch_raw = std::env::temp_dir().join(format!("qt-dfs-rust-{pid}"));
    let _ = std::fs::remove_dir_all(&scratch_raw);
    std::fs::create_dir_all(&scratch_raw).unwrap();
    let root = std::fs::canonicalize(&scratch_raw).unwrap();
    let fs_mount_base = materialize_tree(&root, &spec.legacy_project_id);
    let files_dir = root.join("files");

    let work_main = root.join("dfs-main.db");
    let work_mount = root.join("dfs-mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let main_w = Writer::open_writable(&work_main, &spec.test_pepper_base64).expect("open main");
    let mount_w = Writer::open_writable(&work_mount, &spec.test_pepper_base64).expect("open mount");
    let main = main_w.connection();
    let mount = mount_w.connection();

    mount
        .execute(
            "UPDATE doc_mount_points SET basePath = ?1 WHERE mountType = 'filesystem'",
            rusqlite::params![fs_mount_base.to_str().unwrap()],
        )
        .expect("rewrite fs basePath");

    let root_str = root.to_string_lossy().to_string();
    let sentinelize = |v: &Value| -> Value {
        let s = serde_json::to_string(v)
            .unwrap()
            .replace(&root_str, "__ROOT__");
        serde_json::from_str(&s).unwrap()
    };

    for (i, op) in spec.ops.iter().enumerate() {
        let ctx = DocEditToolContext {
            chat_id: spec.chat_id.clone(),
            user_id: spec.user_id.clone(),
            project_id: op.ctx.project_id.clone(),
            character_id: op.ctx.character_id.clone(),
            operator_override: op.ctx.operator_override,
            files_dir: Some(files_dir.clone()),
        };
        // The Librarian announcement is a documented seam — NOT posted (the oracle
        // mocks its poster to a no-op), so chat_messages stays untouched.
        let result = execute_doc_edit_tool(main, mount, &op.tool, &op.args, &ctx);
        let formatted = format_doc_edit_results(&result);
        let output = serde_json::to_value(&result).expect("serialize result");

        let mut got = json!({
            "name": op.name,
            "tool": op.tool,
            "output": output,
            "formatted": formatted,
        });
        got = sentinelize(&got);
        placeholder_fs_times(&mut got);
        collapse_mtime_strings(&mut got);
        normalize_walk_op(&op.tool, &mut got);
        let got = normalize(&got);

        let mut want = oracle_ops[i].clone();
        placeholder_fs_times(&mut want);
        collapse_mtime_strings(&mut want);
        normalize_walk_op(&op.tool, &mut want);
        let want = normalize(&want);

        assert_eq!(
            got, want,
            "op[{i}] {} diverged\n  rust:   {got}\n  oracle: {want}",
            op.name
        );
    }

    // Dump the resulting fs tree + the (empty) DB content tables.
    let tree: Vec<Value> = dump_tree(&files_dir, "files")
        .into_iter()
        .chain(dump_tree(&root.join("mount"), "mount"))
        .chain(dump_tree(&root.join("outside"), "outside"))
        .collect();
    let documents = dump_table_json_conn(mount, "doc_mount_documents", "contentSha256")
        .expect("dump documents");
    let file_links =
        dump_table_json_conn(mount, "doc_mount_file_links", "relativePath").expect("dump links");

    let got_dumps = normalize(&json!({
        "tree": tree,
        "documents": documents,
        "fileLinks": file_links,
    }));
    let want_dumps = normalize(&oracle_dumps);
    assert_eq!(
        got_dumps, want_dumps,
        "dumps diverged\n  rust:   {got_dumps}\n  oracle: {want_dumps}"
    );

    drop(main_w);
    drop(mount_w);
    let _ = std::fs::remove_dir_all(&root);

    eprintln!(
        "OK: doc-fs host-filesystem branches matched oracle ({} ops + fs-tree + 2 table dumps).",
        spec.ops.len()
    );
}
