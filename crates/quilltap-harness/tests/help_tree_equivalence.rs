//! P4.9I2A CONTENT ORACLE differential — the SHIPPED help tree through
//! `ensure_help_docs_synced` over the EMBEDDED table (`quilltap-host`'s
//! `build.rs` → `files_store::embedded_help_source_files`) vs v4's REAL
//! `ensureHelpDocsSynced()` walking `join(process.cwd(), 'help')` at the pin.
//!
//! One diff proves at once: the vendored `<repo>/help/` bytes, the front-matter
//! parser, the H1/`title:` rule, the `url:` read, the content hash, the section
//! chunker over all 120 real files, and the WALK ORDER (the `help_docs` rowid
//! order — v4's `readdirSync` order on this machine, which the build script's
//! mirrored walker must reproduce). A one-byte edit to ONE vendored file
//! reddens it (the mutation the lane record names).
//!
//! Both sides start from an EMPTY `help_docs` with no embedding profile, so the
//! embedding enqueue backs off and `jobs` is 0 on both sides. The Rust side
//! provisions a fresh instance (every table the ensure touches exists by
//! construction) and runs the ensure through the writer.
//!
//! Generate (Node 24, from the v4 checkout — the case is self-contained, no
//! fixture; `--v4 <pin>` on the sweep driver rewrites the `cd` while v4 HEAD is
//! past the baseline):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TMPO=/tmp/qt-help-tree-oracle; rm -rf $TMPO; mkdir -p $TMPO/cases
//!   cp $V5W/harness/oracle/cases/help-tree-sync.test.ts $TMPO/cases/
//!   QT_ORACLE_OUT=/tmp/oracle-help-tree.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots $TMPO/cases -- help-tree-sync
//! Run:
//!   QT_ORACLE_HELP_TREE=/tmp/oracle-help-tree.ndjson \
//!     cargo test -p quilltap-harness --test help_tree_equivalence

use serde::Deserialize;
use serde_json::{json, Value};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::help_doc_sync::ensure_help_docs_synced;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::files_store::embedded_help_source_files;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
struct OracleRow {
    count: usize,
    #[serde(rename = "walkOrder")]
    walk_order: Vec<String>,
    docs: Vec<Value>,
    chunks: Vec<Value>,
    jobs: i64,
}

fn first_diff(got: &[Value], want: &[Value], label: &str) -> Option<String> {
    for i in 0..got.len().max(want.len()) {
        let g = got.get(i);
        let w = want.get(i);
        if g != w {
            return Some(format!(
                "{label}[{i}] differs:\n  GOT : {}\n  WANT: {}",
                g.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into()),
                w.map(|v| v.to_string()).unwrap_or_else(|| "<none>".into())
            ));
        }
    }
    None
}

#[tokio::test]
async fn shipped_help_tree_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_TREE") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_TREE (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .expect("one row");
    let want: OracleRow = serde_json::from_str(line).expect("oracle row");

    // A fresh-provisioned instance: every table the ensure touches exists, and
    // there is no embedding profile (the enqueue backs off, as on the v4 side).
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let db = Db::open(
        DbPaths {
            main: data.join("quilltap.db"),
            mount_index: Some(data.join("quilltap-mount-index.db")),
            llm_logs: Some(data.join("quilltap-llm-logs.db")),
        },
        PEPPER,
    )
    .expect("open");

    // The v4 case's hand-built DB has NO embedding profile, so its enqueue backs
    // off; a fresh v5 instance seeds the builtin TF-IDF profile as default, which
    // would enqueue one HELP_DOC job per file. Drop it so both sides measure the
    // same thing here — the enqueue itself is `help_doc_ensure_equivalence`'s
    // comparand, not this family's.
    db.write(|ws| {
        ws.main()
            .connection()
            .execute_batch("DELETE FROM embedding_profiles")?;
        Ok(())
    })
    .await
    .expect("clear the seeded embedding profile");

    let files = embedded_help_source_files();
    assert_eq!(
        files.len(),
        want.count,
        "embedded file count vs the oracle's synced count"
    );
    let result = ensure_help_docs_synced(&db, &files)
        .await
        .expect("ensure")
        .expect("an empty table syncs");
    assert_eq!(
        result.created, want.count,
        "every file is a CREATE on an empty table"
    );
    assert_eq!(result.failed, 0);

    // The dumps — the same projections the oracle records.
    let (walk_order, docs, chunks, jobs) = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT id, path, title, url, content, contentHash, hex(embedding) \
                 FROM help_docs ORDER BY rowid ASC",
            )?;
            let mut walk = Vec::new();
            let mut docs = Vec::new();
            let mut path_by_id = std::collections::HashMap::new();
            for r in stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })? {
                let (id, path, title, url, content, hash, emb) = r?;
                walk.push(path.clone());
                path_by_id.insert(id, path.clone());
                docs.push(json!({
                    "path": path, "title": title, "url": url, "content": content,
                    "contentHash": hash,
                    "hasEmbedding": emb.map(|h| !h.is_empty()).unwrap_or(false),
                }));
            }
            docs.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
            let mut cstmt =
                c.prepare("SELECT docId, chunkIndex, heading, content FROM help_doc_chunks")?;
            let mut chunks = Vec::new();
            for r in cstmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })? {
                let (doc_id, idx, heading, content) = r?;
                chunks.push(json!({
                    "docPath": path_by_id.get(&doc_id).cloned().unwrap_or_else(|| format!("<unknown:{doc_id}>")),
                    "chunkIndex": idx as i64,
                    "heading": heading,
                    "content": content,
                }));
            }
            chunks.sort_by(|a, b| {
                a["docPath"]
                    .as_str()
                    .cmp(&b["docPath"].as_str())
                    .then(a["chunkIndex"].as_i64().cmp(&b["chunkIndex"].as_i64()))
            });
            let jobs: i64 = c.query_row("SELECT COUNT(*) FROM background_jobs", [], |r| r.get(0))?;
            Ok((walk, docs, chunks, jobs))
        })
        .expect("dump");

    let mut failed = Vec::new();
    if walk_order != want.walk_order {
        eprintln!(
            "WALK ORDER differs (first divergence at {:?})",
            walk_order
                .iter()
                .zip(want.walk_order.iter())
                .position(|(a, b)| a != b)
        );
        failed.push("walk_order");
    }
    if let Some(d) = first_diff(&docs, &want.docs, "docs") {
        eprintln!("{d}");
        failed.push("docs");
    } else {
        eprintln!("docs OK ({} rows).", docs.len());
    }
    if let Some(d) = first_diff(&chunks, &want.chunks, "chunks") {
        eprintln!("{d}");
        failed.push("chunks");
    } else {
        eprintln!("chunks OK ({} rows).", chunks.len());
    }
    if jobs != want.jobs {
        eprintln!("jobs {jobs} != {}", want.jobs);
        failed.push("jobs");
    }
    assert!(
        failed.is_empty(),
        "help-tree content oracle FAILED: {failed:?}"
    );
}
