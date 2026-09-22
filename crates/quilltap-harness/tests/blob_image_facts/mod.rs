//! The D19 image comparand shared by every P4.104 family — the Rust half of
//! `harness/oracle/lib/blob-image-facts.ts` (read its header for WHY the
//! comparand is the decision, the landing path, the stored mime, whether the
//! sha changed, the size direction and the DECODED dimensions — never the
//! bytes, never the sha's value). Keep the two in step.
//!
//! Include with `#[path = "blob_image_facts/mod.rs"] mod blob_image_facts;`.

#![allow(dead_code)]

use quilltap_core::services::file_storage::PixelCodec;
use quilltap_host::HostImageCodec;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The joined link + blob read (append a `WHERE …` over `l.*`).
pub const STORED_BLOB_SELECT: &str = "SELECT l.relativePath, l.fileName, b.storedMimeType, b.data \
     FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId ";

/// One stored row: `(relativePath, fileName, storedMimeType, data)`.
pub type StoredBlobRow = (String, String, String, Vec<u8>);

/// Read the stored rows matching `where_clause` (bound to `params`).
pub fn stored_blob_rows(
    mount: &rusqlite::Connection,
    where_clause: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Vec<StoredBlobRow> {
    let sql = format!("{STORED_BLOB_SELECT}{where_clause}");
    let mut stmt = mount.prepare(&sql).expect("prepare stored-blob read");
    let rows = stmt
        .query_map(params, |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .expect("query stored blobs");
    rows.map(|r| r.expect("stored blob row")).collect()
}

/// The facts for each row, relative to the bytes the write was handed —
/// the same objects, field for field and in order, as the TS helper emits.
pub fn blob_image_facts(rows: &[StoredBlobRow], input: &[u8]) -> Vec<Value> {
    let input_sha = hex::encode(Sha256::digest(input));
    rows.iter()
        .map(|(relative_path, file_name, stored_mime_type, data)| {
            let sha = hex::encode(Sha256::digest(data));
            let (width, height) = HostImageCodec.measure(data);
            let size = match data.len().cmp(&input.len()) {
                std::cmp::Ordering::Less => "smaller",
                std::cmp::Ordering::Equal => "same",
                std::cmp::Ordering::Greater => "larger",
            };
            json!({
                "relativePath": relative_path,
                "fileName": file_name,
                "storedMimeType": stored_mime_type,
                "shaChanged": sha != input_sha,
                "size": size,
                "width": width,
                "height": height,
            })
        })
        .collect()
}

/// The seed images (the ONLY real images under `harness/oracle/fixtures/`).
pub fn seed_image(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/normalize-blob-image")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read seed image {}: {e}", path.display()))
}
