//! Tier-1 differential (P4.D380 item 2): v4 `4310d435f`'s resolved-file
//! helpers — `statResolvedFile` / `moveResolvedFile` / `deleteResolvedFile`
//! (`lib/doc-edit/path-resolver.ts:785-824` at `624d0d25c`) against
//! `quilltap_core::doc_edit::path_resolver::{stat_resolved_file,
//! move_resolved_file, delete_resolved_file}`.
//!
//! Every oracle row carries its own tree (`setup`) and op; this side rebuilds
//! the SAME tree in a fresh temp dir and replays the SAME op, then compares the
//! result (a stat's `{isFile, size}`, `null`, or `"ok"`), the rethrown error's
//! Node `code` (round ruling R-4: only `ENOENT` / `ENOTDIR` read as absent —
//! the EACCES rows prove every other class bubbles), the post-op existence of
//! the row's probe paths, and v4's logger lines byte-for-byte as v5 renders
//! them (`module` is v4's logger-child tag; v5 carries it as the tracing
//! target).
//!
//! Generate the oracle output (a NON-ROOT user — the EACCES rows chmod 000):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   LOG_LEVEL=debug $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-resolved-file.ts \
//!     > /tmp/oracle-doc-resolved-file.ndjson
//! Run:
//!   QT_ORACLE_DOC_RESOLVED_FILE=/tmp/oracle-doc-resolved-file.ndjson \
//!     cargo test -p quilltap-harness --test doc_resolved_file_equivalence

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use quilltap_core::doc_edit::path_resolver::{
    delete_resolved_file, io_error_code, move_resolved_file, stat_resolved_file, ResolvedPath,
};
use quilltap_core::doc_edit::DocEditScope;
use serde_json::{json, Value};

fn resolved_at(root: &Path, rel: &str, mount_type: Option<&str>, empty_abs: bool) -> ResolvedPath {
    ResolvedPath {
        absolute_path: if empty_abs {
            String::new()
        } else {
            root.join(rel).to_string_lossy().into_owned()
        },
        scope: DocEditScope::General,
        mount_point_id: mount_type.map(|_| "mp-1".to_string()),
        mount_point_name: None,
        mount_type: mount_type.map(str::to_string),
        base_path: root.to_string_lossy().into_owned(),
        relative_path: rel.to_string(),
    }
}

/// Render one v4 logger line as the v5 capture rig renders the port's line.
fn v5_line(v4: &Value) -> String {
    let level = v4["level"].as_str().unwrap().to_uppercase();
    let mut s = format!(
        "{level} quilltap_core::doc_edit::path_resolver {}",
        v4["message"].as_str().unwrap()
    );
    for (k, v) in v4["fields"].as_object().unwrap() {
        if k == "module" {
            continue;
        }
        s.push_str(&format!(" {k}={}", v.as_str().unwrap()));
    }
    s
}

fn run_row(row: &Value) -> (Value, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut locked = Vec::new();
    for step in row["setup"].as_array().unwrap() {
        if let Some(f) = step.get("file").and_then(Value::as_str) {
            std::fs::write(root.join(f), step["content"].as_str().unwrap()).unwrap();
        } else if let Some(d) = step.get("dir").and_then(Value::as_str) {
            std::fs::create_dir_all(root.join(d)).unwrap();
        } else if let Some(l) = step.get("symlink").and_then(Value::as_str) {
            std::os::unix::fs::symlink(step["target"].as_str().unwrap(), root.join(l)).unwrap();
        } else {
            let p = root.join(step["chmod"].as_str().unwrap());
            let mode = step["mode"].as_u64().unwrap() as u32;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
            locked.push(p);
        }
    }
    let op = &row["op"];
    let kind = op["kind"].as_str().unwrap();
    let (outcome, lines) =
        quilltap_core::test_support::captured_with(|| -> Result<Value, String> {
            match kind {
                "stat" => {
                    let r = resolved_at(
                        root,
                        op["path"].as_str().unwrap(),
                        op.get("mountType").and_then(Value::as_str),
                        op.get("emptyAbsolute").and_then(Value::as_bool) == Some(true),
                    );
                    stat_resolved_file(&r)
                        .map(|s| match s {
                            None => Value::Null,
                            Some(s) => {
                                json!({"isFile": s.is_file, "size": s.size, "mtimeIsDate": true})
                            }
                        })
                        .map_err(|e| io_error_code(&e))
                }
                "move" => move_resolved_file(
                    &resolved_at(root, op["from"].as_str().unwrap(), None, false),
                    &resolved_at(root, op["to"].as_str().unwrap(), None, false),
                )
                .map(|_| json!("ok"))
                .map_err(|e| io_error_code(&e)),
                "delete" => delete_resolved_file(&resolved_at(
                    root,
                    op["path"].as_str().unwrap(),
                    None,
                    false,
                ))
                .map(|_| json!("ok"))
                .map_err(|e| io_error_code(&e)),
                other => panic!("unknown op {other}"),
            }
        });
    for p in &locked {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let after: Vec<Value> = row["probes"]
        .as_array()
        .map(|a| a.to_vec())
        .unwrap_or_default()
        .iter()
        .map(|p| {
            let p = p.as_str().unwrap();
            json!({"path": p, "exists": std::fs::symlink_metadata(root.join(p)).is_ok()})
        })
        .collect();
    let mut out = json!({"id": row["id"]});
    match outcome {
        Ok(v) => out["result"] = v,
        Err(code) => out["errorCode"] = json!(code),
    }
    out["after"] = json!(after);
    (out, lines)
}

#[test]
fn resolved_file_helpers_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_DOC_RESOLVED_FILE") else {
        eprintln!("SKIP: QT_ORACLE_DOC_RESOLVED_FILE unset");
        return;
    };
    // SAFETY-free probe: the EACCES rows need a non-root user on this side too.
    assert_ne!(
        std::env::var("USER").as_deref(),
        Ok("root"),
        "the EACCES rows are meaningless as root"
    );
    let text = std::fs::read_to_string(&path).expect("oracle NDJSON");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(rows.len() >= 21, "oracle has {} rows", rows.len());
    let mut failures = Vec::new();
    for row in &rows {
        let probes = row.get("probes").cloned().unwrap_or_else(|| {
            json!(row["after"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["path"].clone())
                .collect::<Vec<_>>())
        });
        let mut replay = row.clone();
        replay["probes"] = probes;
        let (got, lines) = run_row(&replay);
        let mut want = json!({"id": row["id"]});
        if row.get("errorCode").is_some() {
            want["errorCode"] = row["errorCode"].clone();
        } else {
            want["result"] = row["result"].clone();
        }
        want["after"] = row["after"].clone();
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want).unwrap() {
            failures.push(format!("{}:\n  v5 {got}\n  v4 {want}", row["id"]));
        }
        let want_lines: Vec<String> = row["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(v5_line)
            .collect();
        if lines != want_lines {
            failures.push(format!(
                "{} lines:\n  v5 {lines:?}\n  v4 {want_lines:?}",
                row["id"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
