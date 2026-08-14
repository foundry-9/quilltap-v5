//! P4.9H2A EMBEDDING-REAPPLY differential: `services::embedding_reapply_profile`
//! vs v4's REAL `reapplyEmbeddingProfile`, over a FRESH copy of the committed
//! `embedding-profiles-{main,mount}.db` fixture for the Reapply Target profile
//! (trunc=4, over 8-dim vectors). Both sides quantize with the same codec, so the
//! rewritten blobs are BYTE-IDENTICAL — every embedding-bearing table's
//! `{id, embedding-hex}` is compared byte-for-byte, plus the result's per-table
//! counts + targetDimensions + totalTruncated (backup paths + durationMs
//! normalized out on the oracle side).
//!
//! ⚠ MOUNT (`doc_mount_chunks`) — v4 and v5 AGREE byte-for-byte, but neither
//! slices it IN THE SANDBOX (the fixture's one 8-dim mount chunk stays 8-dim on
//! both sides). v4's `reapply-profile.ts` opens the mount via a fresh
//! `getMountIndexDatabasePath()` connection whose second read returns 0 rows in
//! jest; v5's writer connection likewise reads 0 rows for the mount reapply after
//! the preceding partition ops (a writer-connection interaction reproducible only
//! in this harness — an isolated writer read sees the chunk). The SLICING LOGIC
//! itself is proven by the four MAIN tables here (byte-identical) + the
//! `services::embedding_reapply_profile` unit tests. That production reapply
//! slices the mount partition is the OWED real-instance proof (the CLAUDE.md
//! crypto/mount flag) — the differential locks the two ports to identical
//! observable behavior meanwhile.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-ep-reapply.ndjson npx jest -- embedding-reapply
//! Run:
//!   QT_ORACLE_EP_REAPPLY=/tmp/oracle-ep-reapply.ndjson \
//!     cargo test -p quilltap-harness --test embedding_reapply_equivalence

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::embedding_reapply_profile::reapply_embedding_profile;
use serde_json::{json, Value};

const EP_REAPPLY: &str = "e0000000-0000-4000-8000-000000000005";
const PEPPER: &str = "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `[{id, embedding: <hex>}]` for one table, ordered by id (v4's `dumpTables`).
fn dump_table(db: &Db, mount: bool, table: &str) -> Value {
    let table = table.to_string();
    let reader = move |conn: &rusqlite::Connection| {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, embedding FROM \"{table}\" WHERE embedding IS NOT NULL ORDER BY id"
        ))?;
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let id: String = r.get(0)?;
                let blob: Vec<u8> = r.get(1)?;
                Ok(json!({ "id": id, "embedding": hex(&blob) }))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Value::Array(rows))
    };
    if mount {
        db.read_mount_index(reader).unwrap()
    } else {
        db.read_main(reader).unwrap()
    }
}

#[test]
fn embedding_reapply_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EP_REAPPLY") else {
        eprintln!("SKIP: set QT_ORACLE_EP_REAPPLY (see test header).");
        return;
    };
    let oracle: Value =
        serde_json::from_str(std::fs::read_to_string(&oracle_path).unwrap().trim()).unwrap();

    let scratch = std::env::temp_dir().join(format!("qt-ep-reapply-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("embedding-profiles-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("embedding-profiles-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // Reapply Target: trunc=4, normalizeL2=true.
    let result = rt
        .block_on(reapply_embedding_profile(
            &db,
            EP_REAPPLY,
            4,
            true,
            "2026-05-05T00:00:00.000Z",
            1234,
        ))
        .expect("reapply");

    let mut failed = false;

    // ── every table: byte-for-byte against v4 ───────────────────────────────
    let want_tables = &oracle["tables"];
    for t in [
        "memories",
        "vector_entries",
        "conversation_chunks",
        "help_docs",
        // P4.D77 — the fifth main table (v4 `24633026`). Its one seeded chunk
        // carries an 8-dimension vector against this profile's trunc=4, so the
        // table shows a real `truncated` count in `perTable` below rather than
        // being silently skipped as absent on both sides.
        "help_doc_chunks",
        "doc_mount_chunks",
    ] {
        let is_mount = t == "doc_mount_chunks";
        let got = dump_table(&db, is_mount, t);
        let want = &want_tables[t];
        if &got != want {
            eprintln!("[{t}] MISMATCH:\n  got:  {got}\n  want: {want}");
            failed = true;
        } else {
            eprintln!(
                "[{t}] OK ({} rows).",
                got.as_array().map(|a| a.len()).unwrap_or(0)
            );
        }
    }

    // ── result: perTable + targetDimensions + totalTruncated ────────────────
    let got_per: Vec<Value> = result
        .per_table
        .iter()
        .map(|p| {
            json!({
                "table": p.table,
                "truncated": p.truncated,
                "alreadyAtTarget": p.already_at_target,
                "shorterThanTarget": p.shorter_than_target,
                "degenerate": p.degenerate,
            })
        })
        .collect();
    if json!(got_per) != oracle["result"]["perTable"] {
        eprintln!(
            "[perTable] MISMATCH:\n  got:  {}\n  want: {}",
            json!(got_per),
            oracle["result"]["perTable"]
        );
        failed = true;
    } else {
        eprintln!("[perTable] OK.");
    }
    assert_eq!(
        result.target_dimensions,
        oracle["result"]["targetDimensions"].as_i64().unwrap(),
        "targetDimensions"
    );
    assert_eq!(
        result.total_truncated as i64,
        oracle["result"]["totalTruncated"].as_i64().unwrap(),
        "totalTruncated"
    );
    assert!(result.backup_path.is_some(), "main backup must be taken");

    let _ = std::fs::remove_dir_all(&scratch);
    assert!(!failed, "reapply differential mismatched");
    eprintln!("OK: embedding_reapply matched oracle (all tables byte-identical).");
}
