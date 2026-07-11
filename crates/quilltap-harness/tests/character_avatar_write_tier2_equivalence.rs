//! P4.6m (unit 4) tier-2 differential: `write_main_avatar_to_vault` (the
//! delete-then-insert main-avatar write the ST-import multipart leg performs) vs
//! v4's REAL `writeCharacterAvatarToVault({ kind: 'main' })`. Runs on a FRESH
//! copy of the characters fixture (Aria already has `images/avatar.webp`, so the
//! replacement path is exercised).
//!
//! The stored WebP BYTES are the declared codec seam (the host `image`/`webp`
//! stack vs sharp), so `sha256` is blanked; the link row's stable fields
//! (relativePath / fileName / originalMimeType / storedMimeType) and the blob's
//! decoded metadata (width / height / format / non-empty) are diffed exactly.
//!
//! Generate the oracle (see the .ts header) then run:
//!   QT_ORACLE_AVATAR_WRITE=/tmp/oracle-avatar-write.ndjson \
//!     cargo test -p quilltap-harness --test character_avatar_write_tier2_equivalence

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::file_storage::PixelCodec;
use quilltap_core::services::image_job_storage::write_main_avatar_to_vault;
use quilltap_host::HostImageCodec;
use serde::Deserialize;
use serde_json::{json, Value};

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const AVATAR_PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAAAFklEQVR4nGM4YaNBEmIY1TCqYfhqAAAeBCwQMwPSngAAAABJRU5ErkJggg==";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/characters.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-avatar-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// Blank the seam `sha256` in each link object.
fn blank_sha(links: &mut Value) {
    if let Some(arr) = links.as_array_mut() {
        for l in arr {
            if let Some(o) = l.as_object_mut() {
                if o.contains_key("sha256") {
                    o.insert("sha256".into(), Value::String("<sha256>".into()));
                }
            }
        }
    }
}

#[test]
fn avatar_write_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_AVATAR_WRITE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_AVATAR_WRITE to the oracle NDJSON (see header).");
            return;
        }
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .lines()
            .find(|l| !l.trim().is_empty())
            .expect("one oracle line"),
    )
    .expect("parse oracle");

    let png = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(AVATAR_PNG_B64)
            .unwrap()
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let db = fresh_db(&spec);

    // Drive the main-avatar write with the host codec.
    let written = rt
        .block_on(db.write(move |writers| {
            let mount = writers.mount_index().unwrap().connection();
            let main = writers.main().connection();
            Ok(write_main_avatar_to_vault(
                main,
                mount,
                &HostImageCodec,
                ARIA,
                "hero.png",
                &png,
                "image/png",
                None,
            ))
        }))
        .unwrap()
        .expect("avatar write ok");

    // Dump the link row(s) at the canonical path + the stored blob bytes.
    let (mut links, blob): (Value, Vec<u8>) = db
        .read_mount_index(|mount| {
            let mut stmt = mount.prepare(
                "SELECT l.relativePath, l.fileName, l.originalMimeType, b.storedMimeType, b.sha256, b.data \
                 FROM doc_mount_file_links l JOIN doc_mount_blobs b ON b.fileId = l.fileId \
                 WHERE l.relativePath = 'images/avatar.webp' ORDER BY l.id",
            )?;
            let mut rows = Vec::new();
            let mut blob = Vec::new();
            let mapped = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Vec<u8>>(5)?,
                ))
            })?;
            for row in mapped {
                let (rp, fname, omime, smime, sha, data) = row?;
                blob = data;
                rows.push(json!({
                    "relativePath": rp,
                    "fileName": fname,
                    "originalMimeType": omime,
                    "storedMimeType": smime,
                    "sha256": sha,
                }));
            }
            Ok((Value::Array(rows), blob))
        })
        .unwrap();

    // Metadata-level blob check (dims via the host codec; format from the mime).
    let (w, h) = HostImageCodec.measure(&blob);
    let meta = json!({
        "width": w,
        "height": h,
        "format": if written.stored_mime_type == "image/webp" { "webp" } else { "png" },
        "nonEmpty": !blob.is_empty(),
    });

    // Diff (sha256 blanked on both sides).
    let mut want_links = oracle["links"].clone();
    blank_sha(&mut links);
    blank_sha(&mut want_links);

    assert_eq!(links, want_links, "link rows differ");
    assert_eq!(
        links.as_array().unwrap().len() as i64,
        oracle["linkCount"].as_i64().unwrap(),
        "linkCount differs (delete-then-insert)"
    );
    assert_eq!(written.stored_mime_type, oracle["storedMimeType"]);
    assert_eq!(meta, oracle["meta"], "blob metadata differs");
    eprintln!("OK: write_main_avatar_to_vault matched oracle (16×16 webp, replaced).");
}
