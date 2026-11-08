//! P4.6y mount-INDEX differential (scan / reindex / embed / semantic-search):
//! the `api::mount_files` indexing variants vs v4's REAL action-dispatch
//! handlers, over per-case FRESH copies of the committed mounts fixture. Both
//! sides' RAW eight-table dumps (+ response bodies) are normalized by the ONE
//! shared algorithm in `tests/mount_common/mod.rs`.
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror; jest
//! ignores .claude/ paths):
//!   TMPO=/tmp/qt-mount-index-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp harness/oracle/cases/mount-index.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/mounts.json "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MOUNTS_MAIN=<v5>/crates/quilltap-web/tests/fixtures/mounts-main.db \
//!   QT_FIXTURE_MOUNTS_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/mounts-mount.db \
//!   QT_MOUNTS_FS_TREE=<v5>/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
//!   QT_ORACLE_OUT=/tmp/oracle-mount-index.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- mount-index
//! Run:
//!   QT_ORACLE_MOUNT_INDEX=/tmp/oracle-mount-index.ndjson \
//!     cargo test -p quilltap-harness --test mount_index_equivalence

#[path = "mount_common/mod.rs"]
mod mount_common;

use mount_common::*;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::ErasedEmbeddingProvider;
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use serde_json::{json, Value};

// === P4.D381 (v4 bf7a457f6): the scan's lines + the in-cloud plant ===

/// The lines this family compares, by message (the oracle's `SCAN_LINES`).
const SCAN_LINES: &[&str] = &[
    "Starting mount point scan",
    "Mount point scan completed",
    "Left cloud-evicted files unread; they are indexed once downloaded",
    "Starting database-backed mount point rescan",
    "Database-backed rescan failed",
    "Error processing file during mount scan",
    "Mount point scan failed",
    "Unable to read directory during walk",
    "[Mount Points v1] Scan completed",
    "Enqueuing embedding jobs for mount chunks",
    "Embedding job enqueueing complete",
];

/// A PROCESS-GLOBAL capture: the scan runs inside `Db::write`, on the WRITER
/// thread, which a thread-scoped capture cannot see. This binary runs ONE
/// test, so the buffer is never shared.
fn all_thread_lines() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    static BUF: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<String>>>> =
        std::sync::OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// The message a captured line carries, when it is one of [`SCAN_LINES`].
fn scan_message(rest: &str) -> Option<&'static str> {
    SCAN_LINES
        .iter()
        .copied()
        .find(|m| rest == *m || rest.starts_with(&format!("{m} ")))
}

/// Mask the per-side values: the fs-tree copy's path, and an OS error text
/// (Node's `EACCES: …` vs Rust's `Permission denied (os error 13)`).
fn mask_line(line: &str) -> String {
    let mut out = String::new();
    for (i, tok) in line.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let masked = if let Some((k, v)) = tok.split_once('=') {
            if (k == "basePath" || k == "path") && v.contains("fs-tree") {
                let tail = &v[v.find("fs-tree").unwrap() + "fs-tree".len()..];
                format!("{k}=<tree>{tail}")
            } else {
                tok.to_string()
            }
        } else {
            tok.to_string()
        };
        out.push_str(&masked);
    }
    match out.find(" error=") {
        Some(i) => format!("{} error=<err>", &out[..i]),
        None => out,
    }
}

/// v5's captured lines for this case, `LEVEL message k=v…` (target dropped).
fn v5_scan_lines(captured: &[String]) -> Vec<String> {
    captured
        .iter()
        .filter_map(|l| {
            let mut it = l.splitn(3, ' ');
            let (level, _target, rest) = (it.next()?, it.next()?, it.next()?);
            scan_message(rest)?;
            Some(mask_line(&format!("{level} {rest}")))
        })
        .collect()
}

/// v4's `{level, message, fields}` rendered the same way.
fn v4_scan_lines(row: &Value) -> Vec<String> {
    row["logs"]
        .as_array()
        .unwrap_or_else(|| panic!("oracle row {} has no logs", row["name"]))
        .iter()
        .map(|l| {
            let mut s = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            if let Some(m) = l["fields"].as_object() {
                for (k, v) in m {
                    let val = match v {
                        Value::String(x) => x.clone(),
                        other => other.to_string(),
                    };
                    s.push_str(&format!(" {k}={val}"));
                }
            }
            mask_line(&s)
        })
        .collect()
}

/// A scan result's `errors` entries keep v4's `<relativePath>: ` prefix; the
/// OS message after it is per-runtime text (masked both sides).
fn mask_scan_errors(body: &mut Value) {
    if let Some(errs) = body
        .get_mut("scanResult")
        .and_then(|r| r.get_mut("errors"))
        .and_then(Value::as_array_mut)
    {
        for e in errs {
            if let Some(s) = e.as_str() {
                let prefix = s.split_once(": ").map(|(p, _)| p).unwrap_or(s);
                *e = Value::String(format!("{prefix}: <err>"));
            }
        }
    }
}

/// Reports `blocks: 0` for ONE absolute path once armed — the oracle's
/// `mockEvictedStat` (armed after the first, indexing scan).
struct EvictOnce {
    path: std::sync::Mutex<Option<String>>,
}
impl quilltap_core::services::mount_index::scanner::AllocationProbe for EvictOnce {
    fn allocation(
        &self,
        p: &str,
        meta: &std::fs::Metadata,
    ) -> Option<quilltap_core::services::mount_index::cloud_placeholder::AllocationStat> {
        if self.path.lock().unwrap().as_deref() == Some(p) {
            return Some(
                quilltap_core::services::mount_index::cloud_placeholder::AllocationStat {
                    size: meta.len(),
                    blocks: 0,
                },
            );
        }
        quilltap_core::services::mount_index::scanner::HostAllocationProbe.allocation(p, meta)
    }
}

/// The fs-tree copy `fresh_db` made for this case.
fn tree_of(scratch: &std::path::Path) -> std::path::PathBuf {
    scratch.join("fs-tree")
}

fn chmod(p: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
}
thread_local! {
    /// The current case's scratch dir (its fs-tree copy is under it).
    static SCRATCH: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn scratch_of(_db: &Db) -> std::path::PathBuf {
    SCRATCH.with(|s| s.borrow().clone().expect("scratch set"))
}
// === /P4.D381 ===

// ─── the runner ──────────────────────────────────────────────────────────────

#[test]
fn mount_index_matches_oracle() {
    let Some(oracle) = load_oracle("QT_ORACLE_MOUNT_INDEX") else {
        return;
    };
    let pepper = test_pepper();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    use quilltap_core::api::mount_files as mf;

    let _ = all_thread_lines(); // === P4.D381: install before any callsite ===
    type CaseFn = Box<dyn Fn(&Db, &tokio::runtime::Runtime) -> Response>;
    fn provider(db: &Db) -> ErasedEmbeddingProvider {
        ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
            db.clone(),
            CannedWireTransport::new(),
        ))
    }
    let cases: Vec<(&str, CaseFn)> = vec![
        // ── scan (unit F) ──
        (
            "scan_fs",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, MP_FS))),
        ),
        (
            "scan_obs",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, MP_OBS))),
        ),
        (
            "scan_db",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, MP_DB))),
        ),
        (
            "scan_404",
            Box::new(|db, rt| rt.block_on(mf::mount_scan(db, BOGUS))),
        ),
        // ── reindex / embed / semantic-search (unit E) ──
        (
            "reindex_db_default",
            Box::new(|db, rt| rt.block_on(mf::mount_reindex(db, MP_DB, None, None))),
        ),
        (
            "reindex_db_force",
            Box::new(|db, rt| rt.block_on(mf::mount_reindex(db, MP_DB, None, Some(true)))),
        ),
        (
            "reindex_db_scoped",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_reindex(
                    db,
                    MP_DB,
                    Some("notes/".to_string()),
                    Some(true),
                ))
            }),
        ),
        (
            "reindex_404",
            Box::new(|db, rt| rt.block_on(mf::mount_reindex(db, BOGUS, None, None))),
        ),
        (
            "embed_db_default",
            Box::new(|db, rt| rt.block_on(mf::mount_embed(db, MP_DB, None, None))),
        ),
        (
            "embed_db_scoped_force",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_embed(
                    db,
                    MP_DB,
                    Some("reference.txt".to_string()),
                    Some(true),
                ))
            }),
        ),
        (
            "embed_404",
            Box::new(|db, rt| rt.block_on(mf::mount_embed(db, BOGUS, None, None))),
        ),
        (
            "search_basic",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_semantic_search(
                    db,
                    &provider(db),
                    SINGLE_USER_ID,
                    json!({"query": "reference alpha", "threshold": 0, "top": 10}),
                ))
            }),
        ),
        (
            "search_scoped",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_semantic_search(
                    db,
                    &provider(db),
                    SINGLE_USER_ID,
                    json!({
                        "query": "body line",
                        "mountPointIds": [MP_DB],
                        "pathPrefix": "notes/",
                        "threshold": 0
                    }),
                ))
            }),
        ),
        // === P4.D381 (v4 bf7a457f6) ===
        (
            "scan_fs_in_cloud",
            Box::new(|db, rt| {
                use quilltap_core::services::mount_index::scanner::AllocationProbe as _;
                let probe = std::sync::Arc::new(EvictOnce {
                    path: std::sync::Mutex::new(None),
                });
                let _ = rt.block_on(mf::mount_scan_with_probe(db, MP_FS, probe.clone()));
                let alpha = tree_of(&scratch_of(db)).join("notes/alpha.md");
                *probe.path.lock().unwrap() = Some(alpha.to_str().unwrap().to_string());
                chmod(&alpha, 0o000);
                let r = rt.block_on(mf::mount_scan_with_probe(db, MP_FS, probe));
                chmod(&alpha, 0o644);
                r
            }),
        ),
        (
            "scan_fs_unreadable_file",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                let alpha = tree_of(&scratch_of(db)).join("notes/alpha.md");
                chmod(&alpha, 0o000);
                let r = rt.block_on(mf::mount_scan(db, MP_FS));
                chmod(&alpha, 0o644);
                r
            }),
        ),
        (
            "scan_fs_unreadable_dir",
            Box::new(|db, rt| {
                let sub = tree_of(&scratch_of(db)).join("notes/sub");
                chmod(&sub, 0o000);
                let r = rt.block_on(mf::mount_scan(db, MP_FS));
                chmod(&sub, 0o755);
                r
            }),
        ),
        // === /P4.D381 ===
        (
            "search_bad_body",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_semantic_search(
                    db,
                    &provider(db),
                    SINGLE_USER_ID,
                    json!({}),
                ))
            }),
        ),
    ];

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut lines_compared = 0usize; // === P4.D381 ===
    for (name, run) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("no oracle row {name}"));
        let (db, scratch) = fresh_db(&pepper, name);
        // === P4.D381 ===
        SCRATCH.with(|s| *s.borrow_mut() = Some(scratch.clone()));
        all_thread_lines().lock().unwrap().clear();
        // === /P4.D381 ===
        let resp = run(&db, &rt);
        let (status, mut body) = response_to_status_body(resp);
        let tables = dump_tables(&db);
        // === P4.D381: the lines (in ORDER), then the row without them ===
        let captured = all_thread_lines().lock().unwrap().clone();
        let (got_lines, want_lines) = (v5_scan_lines(&captured), v4_scan_lines(want));
        if got_lines != want_lines {
            failures.push(format!(
                "case {name} lines:\n  v4 {want_lines:#?}\n  v5 {got_lines:#?}"
            ));
        }
        lines_compared += want_lines.len();
        let mut want = want.clone();
        want.as_object_mut().unwrap().remove("logs");
        mask_scan_errors(&mut body);
        if let Some(b) = want.get_mut("body") {
            mask_scan_errors(b);
        }
        let want = &want;
        // === /P4.D381 ===
        let got = json!({ "name": name, "status": status, "body": body, "tables": tables });
        // Error cases carry a tables dump too — the refusals must leave the
        // fixture untouched on both sides.
        // P4.D381: collect every red row by id (the red-first record needs
        // the whole set, not the first divergence).
        if norm(&got) != norm(want) {
            failures.push(format!(
                "case {name}
{}",
                diff_first_line(&norm(&got), &norm(want))
            ));
        }
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
        checked += 1;
    }

    assert!(
        failures.is_empty(),
        "{} row(s) diverge:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 17, "expected the 17 mount-index cases");
    // === P4.D381: the log comparand was not vacuous ===
    assert!(lines_compared >= 20, "only {lines_compared} lines compared");
    eprintln!("OK: mount-index matched oracle ({checked} cases, {lines_compared} lines).");
}
