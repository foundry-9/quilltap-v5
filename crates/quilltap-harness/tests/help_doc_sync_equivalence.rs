//! P4.1b differential — the help-docs disk sync (v4 `lib/help/help-doc-sync.ts`
//! `syncHelpDocs` vs `quilltap_core::services::help_doc_sync::sync_help_docs`).
//!
//! Both sides COPY the same pre-seeded fixture DB, walk the SAME committed
//! fixture help tree (`harness/oracle/fixtures/help-sync/help/`; the Rust side
//! through the production host walker), run the sync, and diff:
//!   - the returned summary (counts + the changed docs as sorted PATHS —
//!     walk order is readdir-dependent, paths are the stable identity);
//!   - the full `help_docs` dump ordered by path (embedding as hex).
//!
//! Normalization is sentinel-aware: minted ids collapse to `<minted>`, minted
//! timestamps to `<ts>`, while the seeded `2020-…` sentinel must SURVIVE —
//! proving the unchanged row (matching contentHash) kept its title, embedding
//! and timestamps byte-for-byte, and that the updated row kept its seeded
//! `createdAt` while clearing its embedding.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this tree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-help-sync-fixture.ts
//!   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/help-sync-tier2.ts > /tmp/oracle-help-sync.ndjson
//! Run:
//!   QT_ORACLE_HELP_SYNC=/tmp/oracle-help-sync.ndjson \
//!   QT_FIXTURE_HELP_MAIN=/tmp/qt-help-sync-main.db \
//!     cargo test -p quilltap-harness --test help_doc_sync_equivalence

use serde::Deserialize;
use serde_json::{json, Value};

use quilltap_core::db::runtime::Db;
use quilltap_core::services::help_doc_sync::sync_help_docs;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedSentinel")]
    seed_sentinel: String,
    seed: Vec<SeedRow>,
    /// P4.D77 — the pre-existing chunk rows (replaced / survived / pruned).
    #[serde(rename = "helpDocChunkSeed")]
    help_doc_chunk_seed: Vec<SeedRow>,
}

#[derive(Deserialize)]
struct SeedRow {
    id: String,
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// Normalize a dump row: seeded ids stay literal, minted ids → `<minted>`;
/// the seed-sentinel timestamps stay literal, minted → `<ts>`.
fn normalize_rows(rows: &[Value], seeded_ids: &[String], sentinel: &str) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let mut out = row.clone();
            let obj = out.as_object_mut().expect("row object");
            if let Some(Value::String(id)) = obj.get("id") {
                if !seeded_ids.contains(id) {
                    obj.insert("id".into(), Value::String("<minted>".into()));
                }
            }
            for ts_col in ["createdAt", "updatedAt"] {
                if let Some(Value::String(ts)) = obj.get(ts_col) {
                    if ts != sentinel {
                        obj.insert(ts_col.into(), Value::String("<ts>".into()));
                    }
                }
            }
            out
        })
        .collect()
}

#[test]
fn help_doc_sync_matches_oracle() {
    let (oracle_path, fixture_main) = match (
        std::env::var("QT_ORACLE_HELP_SYNC"),
        std::env::var("QT_FIXTURE_HELP_MAIN"),
    ) {
        (Ok(o), Ok(f)) => (o, f),
        _ => {
            eprintln!(
                "skipping help_doc_sync_matches_oracle: set QT_ORACLE_HELP_SYNC + \
                 QT_FIXTURE_HELP_MAIN to run the differential"
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("help-sync.json")).expect("spec"),
    )
    .expect("parse spec");
    let seeded_ids: Vec<String> = spec.seed.iter().map(|s| s.id.clone()).collect();
    let seeded_chunk_ids: Vec<String> = spec
        .help_doc_chunk_seed
        .iter()
        .map(|s| s.id.clone())
        .collect();

    // Copy the fixture DB.
    let tmp = tempfile::tempdir().expect("tempdir");
    let work_main = tmp.path().join("help-sync-main.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy fixture");

    let db = Db::open_main(&work_main, &spec.test_pepper_base64).expect("open db");

    // Walk the SAME committed help tree through the production host walker.
    let fixture_root = fixtures_dir().join("help-sync");
    let files = quilltap_host::files_store::load_help_source_files(&fixture_root);

    let files_for_sync = files.clone();
    let result = db
        .write_blocking(move |ws| Ok(sync_help_docs(ws.main().connection(), &files_for_sync)))
        .expect("sync");

    // Dump the table (same SELECT/order as the oracle).
    let rows: Vec<Value> = db
        .read_main(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, title, path, url, content, contentHash, \
                     hex(embedding) AS embeddingHex, createdAt, updatedAt \
                     FROM help_docs ORDER BY path ASC",
                )
                .map_err(quilltap_core::db::DbError::from)?;
            let mapped = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "title": r.get::<_, String>(1)?,
                        "path": r.get::<_, String>(2)?,
                        "url": r.get::<_, String>(3)?,
                        "content": r.get::<_, String>(4)?,
                        "contentHash": r.get::<_, String>(5)?,
                        // SQLite hex(NULL) = '' — matches the oracle's SELECT.
                        "embeddingHex": r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                        "createdAt": r.get::<_, String>(7)?,
                        "updatedAt": r.get::<_, String>(8)?,
                    }))
                })
                .map_err(quilltap_core::db::DbError::from)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(quilltap_core::db::DbError::from)?;
            Ok(mapped)
        })
        .expect("dump");

    // Build the summary with changedIds mapped to sorted paths.
    let path_by_id: std::collections::HashMap<&str, &str> = rows
        .iter()
        .map(|r| {
            (
                r.get("id").and_then(Value::as_str).unwrap(),
                r.get("path").and_then(Value::as_str).unwrap(),
            )
        })
        .collect();
    let mut changed_paths: Vec<String> = result
        .changed_ids
        .iter()
        .map(|id| {
            path_by_id
                .get(id.as_str())
                .map(|p| p.to_string())
                .unwrap_or_else(|| format!("<unknown:{id}>"))
        })
        .collect();
    changed_paths.sort();

    let rust_summary = json!({
        "kind": "summary",
        "totalOnDisk": result.total_on_disk,
        "created": result.created,
        "updated": result.updated,
        "unchanged": result.unchanged,
        "deleted": result.deleted,
        "failed": result.failed,
        "chunksWritten": result.chunks_written,
        "changedPaths": changed_paths,
    });

    // P4.D77 — the section chunks the sync wrote. Chunk ids and timestamps are
    // minted on BOTH sides, so the row identity is (doc PATH, chunkIndex).
    // `chunkIndex` reads as `f64`: on this `generateDDL`-shaped fixture the
    // column is REAL, and SQLite's REAL affinity forces the bound integer into
    // floating point (see `db::help_doc_chunks::HelpDocChunkRow`).
    #[allow(clippy::type_complexity)]
    let chunk_rows: Vec<(String, String, f64, Option<String>, String, String, String)> = db
        .read_main(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, docId, chunkIndex, heading, content, \
                     hex(embedding) AS embeddingHex, updatedAt FROM help_doc_chunks",
                )
                .map_err(quilltap_core::db::DbError::from)?;
            let mapped = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, f64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        r.get::<_, String>(6)?,
                    ))
                })
                .map_err(quilltap_core::db::DbError::from)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(quilltap_core::db::DbError::from)?;
            Ok(mapped)
        })
        .expect("dump help_doc_chunks");

    let mut chunks: Vec<Value> = chunk_rows
        .iter()
        .map(|(id, doc_id, idx, heading, content, emb_hex, updated_at)| {
            json!({
                "id": if seeded_chunk_ids.contains(id) { id.as_str() } else { "<minted>" },
                "docPath": path_by_id
                    .get(doc_id.as_str())
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| format!("<unknown:{doc_id}>")),
                "chunkIndex": if idx.fract() == 0.0 { Value::from(*idx as i64) } else { Value::from(*idx) },
                "heading": heading,
                "content": content,
                "hasEmbedding": !emb_hex.is_empty(),
                "updatedAt": if updated_at == &spec.seed_sentinel { "<sentinel>" } else { "<ts>" },
            })
        })
        .collect();
    chunks.sort_by(|a, b| {
        a["docPath"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["docPath"].as_str().unwrap_or_default())
            .then(
                a["chunkIndex"]
                    .as_i64()
                    .unwrap_or_default()
                    .cmp(&b["chunkIndex"].as_i64().unwrap_or_default()),
            )
    });

    // The prune's cascade into embedding_status (same SELECT/order as the oracle).
    let status_rows: Vec<Value> = db
        .read_main(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, entityType, entityId, profileId, status \
                     FROM embedding_status ORDER BY id ASC",
                )
                .map_err(quilltap_core::db::DbError::from)?;
            let mapped = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "entityType": r.get::<_, String>(1)?,
                        "entityId": r.get::<_, String>(2)?,
                        "profileId": r.get::<_, String>(3)?,
                        "status": r.get::<_, String>(4)?,
                    }))
                })
                .map_err(quilltap_core::db::DbError::from)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(quilltap_core::db::DbError::from)?;
            Ok(mapped)
        })
        .expect("dump embedding_status");

    // Parse the oracle NDJSON.
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");
    let mut oracle_summary = None;
    let mut oracle_rows: Vec<Value> = Vec::new();
    let mut oracle_status_rows: Option<Vec<Value>> = None;
    let mut oracle_chunks: Option<Vec<Value>> = None;
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("summary") => oracle_summary = Some(v),
            Some("help_docs") => {
                oracle_rows = v
                    .get("rows")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            }
            Some("embedding_status") => {
                oracle_status_rows = Some(
                    v.get("rows")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            Some("help_doc_chunks") => {
                oracle_chunks = Some(
                    v.get("rows")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                )
            }
            other => panic!("unexpected oracle line kind: {other:?}"),
        }
    }
    let oracle_summary = oracle_summary.expect("oracle summary line");
    let oracle_status_rows = oracle_status_rows.expect("oracle embedding_status line");
    let oracle_chunks = oracle_chunks.expect("oracle help_doc_chunks line — regenerate the oracle");

    assert_eq!(
        rust_summary, oracle_summary,
        "help-sync summary diverged\nrust:   {rust_summary}\noracle: {oracle_summary}"
    );

    assert_eq!(
        Value::Array(chunks.clone()),
        Value::Array(oracle_chunks.clone()),
        "help_doc_chunks diverged\nrust:   {chunks:?}\noracle: {oracle_chunks:?}"
    );

    let rust_norm = normalize_rows(&rows, &seeded_ids, &spec.seed_sentinel);
    let oracle_norm = normalize_rows(&oracle_rows, &seeded_ids, &spec.seed_sentinel);
    assert_eq!(
        rust_norm.len(),
        oracle_norm.len(),
        "row count diverged: rust {} vs oracle {}",
        rust_norm.len(),
        oracle_norm.len()
    );
    for (r, o) in rust_norm.iter().zip(oracle_norm.iter()) {
        assert_eq!(r, o, "help_docs row diverged\nrust:   {r}\noracle: {o}");
    }

    // Belt-and-braces pins: the unchanged row kept its seeded embedding + the
    // sentinel timestamps; the updated row cleared its embedding but kept its
    // seeded createdAt.
    let unchanged = rust_norm
        .iter()
        .find(|r| r.get("path").and_then(Value::as_str) == Some("help/no-frontmatter.md"))
        .expect("unchanged row");
    assert_eq!(
        unchanged.get("title").and_then(Value::as_str),
        Some("Stale Title Untouched")
    );
    assert_ne!(
        unchanged.get("embeddingHex").and_then(Value::as_str),
        Some("")
    );
    assert_eq!(
        unchanged.get("updatedAt").and_then(Value::as_str),
        Some(spec.seed_sentinel.as_str())
    );
    let updated = rust_norm
        .iter()
        .find(|r| r.get("path").and_then(Value::as_str) == Some("help/getting-started.md"))
        .expect("updated row");
    assert_eq!(
        updated.get("embeddingHex").and_then(Value::as_str),
        Some("")
    );
    assert_eq!(
        updated.get("createdAt").and_then(Value::as_str),
        Some(spec.seed_sentinel.as_str())
    );
    assert_eq!(
        updated.get("updatedAt").and_then(Value::as_str),
        Some("<ts>")
    );

    // ---- the P4.d6 prune (v4 551f090b) ----
    assert_eq!(
        status_rows, oracle_status_rows,
        "embedding_status diverged after the prune\nrust:   {status_rows:?}\noracle: {oracle_status_rows:?}"
    );

    // Belt-and-braces pins, so a shared regression in BOTH sides' dumps can't
    // pass as agreement: the retired doc's row is gone from help_docs, its TWO
    // status rows (one per profile — deleteByEntity is a deleteMany) went with
    // it, and the surviving doc's status row did NOT.
    assert!(
        !rust_norm
            .iter()
            .any(|r| r.get("path").and_then(Value::as_str) == Some("help/retired.md")),
        "help/retired.md has no file on disk and must have been pruned"
    );
    let retired_id = "aaaaaaaa-0000-4000-8000-000000000003";
    assert!(
        !status_rows
            .iter()
            .any(|r| r.get("entityId").and_then(Value::as_str) == Some(retired_id)),
        "the pruned doc's embedding_status rows must be deleted (both profiles)"
    );
    let survivor_id = "aaaaaaaa-0000-4000-8000-000000000001";
    assert!(
        status_rows
            .iter()
            .any(|r| r.get("entityId").and_then(Value::as_str) == Some(survivor_id)),
        "the surviving doc's embedding_status row must NOT be collateral damage"
    );

    // ---- P4.D77 — the section slicing (v4 24633026) ----
    //
    // Pinned against the ORACLE, so two identically-broken sides cannot pass as
    // agreement.
    let oracle_chunk_paths: Vec<&str> = oracle_chunks
        .iter()
        .filter_map(|c| c["docPath"].as_str())
        .collect();
    let oracle_chunk = |path: &str| oracle_chunks.iter().find(|c| c["docPath"] == path);
    // The UPDATED doc: its seeded chunk is REPLACED, not diffed — a fresh id and
    // a cleared vector, because boundaries move when the prose above them does.
    let replaced =
        oracle_chunk("help/getting-started.md").expect("the UPDATED doc must be re-sliced");
    assert_eq!(
        replaced["id"], "<minted>",
        "replaceForDoc is delete-then-insert: the seeded chunk row must be GONE, not \
         updated in place (its embedding no longer describes its content)"
    );
    assert_eq!(replaced["hasEmbedding"], false);
    // The UNCHANGED doc: byte-untouched, seeded id, seeded vector, sentinel
    // timestamp. This is the gap the upgrade backfill exists to close — the
    // content-hash check returns before the slice.
    let survivor =
        oracle_chunk("help/no-frontmatter.md").expect("the UNCHANGED doc's chunk must SURVIVE");
    assert_ne!(
        survivor["id"], "<minted>",
        "the unchanged doc's chunk must keep its seeded id"
    );
    assert_eq!(survivor["hasEmbedding"], true);
    assert_eq!(survivor["updatedAt"], "<sentinel>");
    assert!(
        !oracle_chunk_paths.contains(&"help/close-no-newline.md"),
        "help/close-no-newline.md has an EMPTY body (its frontmatter fence eats the \
         whole file), so it slices to nothing — the zero-chunk arm"
    );
    assert!(
        !oracle_chunk_paths.contains(&"help/retired.md")
            && !oracle_chunks.iter().any(|c| c["docPath"]
                .as_str()
                .is_some_and(|p| p.starts_with("<unknown:"))),
        "the pruned doc's SEEDED chunk must go with it — the prune deletes chunks \
         explicitly, because this fixture's table is the generateDDL shape and carries \
         no FK cascade to do it (an orphan would surface as <unknown:…> here)"
    );
    assert!(
        oracle_chunks
            .iter()
            .filter(|c| c["id"] == "<minted>")
            .all(|c| c["hasEmbedding"] == false),
        "replaceForDoc writes every chunk with a NULL vector; the HELP_DOC job fills them"
    );
    // A doc that changed and DID slice, so `chunksWritten` is measuring something.
    assert!(
        rust_summary["chunksWritten"].as_u64().unwrap_or(0) > 0,
        "the sync wrote no chunks at all — the whole arm is vacuous"
    );
}
