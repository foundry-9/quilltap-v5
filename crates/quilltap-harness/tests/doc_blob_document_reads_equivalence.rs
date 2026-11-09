//! Tier-2 differential (P4.D380 item 3, v4 `a3d53eae1`): the blob-as-document
//! readers — `db::database_store::{read_database_blob_as_document,
//! read_database_blob_for_document}` vs v4's REAL `readDatabaseBlobAsDocument`
//! / `readDatabaseBlobForDocument` (`lib/mount-index/database-store.ts:112-171`
//! at `624d0d25c`).
//!
//! The oracle (`harness/oracle/cases/doc-blob-document-reads.ts`) BUILDS the
//! planted pair (minted per run: the committed mounts pair + uploads through
//! v4's REAL `storeMountFile` + fixed `lastModified` / `updatedAt` instants + a
//! converted PDF text) and reads twelve paths; this side opens the SAME planted
//! pair and reads the same paths. Compared byte-for-byte per path: both
//! answers (`{content, mtime, size}` / `+source` / `+storedMimeType` only for
//! `blob-text`) and the DEBUG `readDatabaseBlobAsDocument: reading an uploaded
//! text blob as a document` `{mountPointId, relativePath, size}` (on the test
//! thread — the readers are plain calls here).
//!
//! The corpus: a native `.md` (lossy UTF-8 with an invalid sequence, a KEPT
//! BOM, an upper-case `.MD`), a `.json`, a case-differing path lookup, a
//! non-normalised path (`/uploads//a.md` — the DEBUG carries the normalised
//! one), a non-native `.yaml`, a converted `.pdf` (`blob-text` + its
//! `storedMimeType`), an unconverted `.pdf`, a path with no blob.
//!
//! ⚖ `UNPARSEABLE_LAST_MODIFIED_READS_ZERO` (P4.D380 item 25, recorded — the
//! P4.D354 K12 NOTE): `uploads/bad-date.md`'s `lastModified` is `'not a date'`;
//! v4's `new Date(…).getTime()` is `NaN` (JSON `null`), v5's reader answers `0`
//! (the P4.D354 reader's `unwrap_or(0)` shape, kept). Asserted BOTH ways below,
//! then equal on every other key.
//!
//! Regenerate (Node 24, from the v4 pin worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DBDR_MAIN=/tmp/qt-dbdr-main.db QT_FIXTURE_DBDR_MOUNT=/tmp/qt-dbdr-mount.db \
//!   LOG_LEVEL=debug \
//!     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/cases/doc-blob-document-reads.ts \
//!     > /tmp/oracle-doc-blob-document-reads.ndjson
//! Run:
//!   QT_ORACLE_DBDR=/tmp/oracle-doc-blob-document-reads.ndjson \
//!   QT_FIXTURE_DBDR_MAIN=/tmp/qt-dbdr-main.db QT_FIXTURE_DBDR_MOUNT=/tmp/qt-dbdr-mount.db \
//!     cargo test -p quilltap-harness --test doc_blob_document_reads_equivalence -- --nocapture

#[path = "mount_common/mod.rs"]
mod mount_common;

use mount_common::*;
use quilltap_core::db::database_store::{
    read_database_blob_as_document, read_database_blob_for_document, BlobForDocument,
    DatabaseBlobDocument,
};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

const BAD_DATE: &str = "uploads/bad-date.md";

fn render_v4(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let mut out = format!(
        "{level} quilltap::documents {}",
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["fields"].as_object().unwrap() {
        if k == "module" {
            continue;
        }
        match v {
            Value::String(s) => out.push_str(&format!(" {k}={s}")),
            other => out.push_str(&format!(" {k}={other}")),
        }
    }
    out
}

fn as_doc_json(d: &Option<DatabaseBlobDocument>) -> Value {
    match d {
        None => Value::Null,
        Some(d) => json!({"content": d.content, "mtime": d.mtime_ms, "size": d.size}),
    }
}

fn for_doc_json(d: &Option<BlobForDocument>) -> Value {
    match d {
        None => Value::Null,
        Some(d) => {
            let mut v = json!({
                "content": d.content, "mtime": d.mtime_ms, "size": d.size, "source": d.source.as_str(),
            });
            if let Some(m) = &d.stored_mime_type {
                v["storedMimeType"] = json!(m);
            }
            v
        }
    }
}

/// `UNPARSEABLE_LAST_MODIFIED_READS_ZERO`: v4 `null` (NaN), v5 `0` — asserted
/// both ways, then lifted to equality.
fn carve_bad_date(path: &str, v4: &mut Value, v5: &mut Value) {
    if path != BAD_DATE {
        return;
    }
    for v in [&mut *v4, &mut *v5] {
        assert!(
            v.is_object(),
            "{path}: the carve row must read on both sides"
        );
    }
    assert_eq!(v4["mtime"], Value::Null, "{path}: v4's NaN mtime");
    assert_eq!(v5["mtime"], json!(0), "{path}: v5's unwrap_or(0) mtime");
    v4["mtime"] = json!("<NaN|0>");
    v5["mtime"] = json!("<NaN|0>");
}

#[test]
fn blob_document_readers_match_v4() {
    let (Ok(oracle), Ok(fix_main), Ok(fix_mount)) = (
        std::env::var("QT_ORACLE_DBDR"),
        std::env::var("QT_FIXTURE_DBDR_MAIN"),
        std::env::var("QT_FIXTURE_DBDR_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_DBDR + QT_FIXTURE_DBDR_MAIN/MOUNT (see header).");
        return;
    };
    let rows: Vec<Value> = std::fs::read_to_string(&oracle)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    assert_eq!(rows.len(), 12, "twelve read paths");
    let pepper = test_pepper();
    let _ = fix_main;
    let mount =
        Writer::open_writable(std::path::Path::new(&fix_mount), &pepper).expect("open mount");

    let mut red = Vec::new();
    for row in &rows {
        let path = row["path"].as_str().unwrap();
        let (as_doc, as_lines) = quilltap_core::test_support::captured_with(|| {
            read_database_blob_as_document(mount.connection(), MP_DB, path)
                .expect("as-document read")
        });
        let (for_doc, for_lines) = quilltap_core::test_support::captured_with(|| {
            read_database_blob_for_document(mount.connection(), MP_DB, path)
                .expect("for-document read")
        });
        let mut got =
            json!({"asDocument": as_doc_json(&as_doc), "forDocument": for_doc_json(&for_doc)});
        let mut want = json!({"asDocument": row["asDocument"], "forDocument": row["forDocument"]});
        {
            let (w, g) = (&mut want["asDocument"], &mut got["asDocument"]);
            carve_bad_date(path, w, g);
        }
        {
            let (w, g) = (&mut want["forDocument"], &mut got["forDocument"]);
            carve_bad_date(path, w, g);
        }
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want).unwrap() {
            red.push(format!("{path}:\n  v5 {got}\n  v4 {want}"));
        }
        let want_as: Vec<String> = row["asLines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        let want_for: Vec<String> = row["forLines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        if as_lines != want_as || for_lines != want_for {
            red.push(format!(
                "{path} lines:\n  v5 {as_lines:?} / {for_lines:?}\n  v4 {want_as:?} / {want_for:?}"
            ));
        }
    }
    assert!(
        red.is_empty(),
        "{} mismatches:\n{}",
        red.len(),
        red.join("\n")
    );
}
