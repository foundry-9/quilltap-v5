//! P4.6m (unit 2) tier-2 differential: `save_to_character_gallery` (the bytes
//! write spine the quilltap-web multipart-upload leg calls) vs v4's REAL
//! `saveToCharacterGallery`. Each case runs on a FRESH copy of the committed
//! characters fixture, under a fixed `kept_at`, and diffs the freshly-written
//! `photos/` link dump (OK cases) or the thrown message (refusal/400 arms).
//!
//! Pins the UPLOAD-specific filename→path branches (dotless timestamped slug vs
//! the dotted sanitize) plus the dedup guard + the two 400-keyword arms; the
//! shared write spine itself is also proven by `photo_save_link`.
//!
//! P4.104: `upload_real_png` hands the gallery a DECODABLE PNG (the
//! `normalize-blob-image` seed) and compares its stored row's D19
//! `imageFacts` (`blob_image_facts/mod.rs`) — v4 normalizes it to WebP inside
//! `linkBlobContent` with real sharp, v5 through the host encoder this family
//! hands `save_to_character_gallery` ([`BLOB_WEBP`]). Red-first on the
//! un-wired tree (the refusing encoder): `photos/Portrait.png`, `image/png`,
//! the sha unchanged, where v4 answers `.webp` / `image/webp` / changed.
//!
//! Generate the oracle (see the .ts header) then run:
//!   QT_ORACLE_PHOTO_UPLOAD=/tmp/oracle-photo-upload.ndjson \
//!     cargo test -p quilltap-harness --test character_photo_upload_tier2_equivalence

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::photos::character_gallery_service::{save_to_character_gallery, GalleryError};
use serde::Deserialize;
use serde_json::{json, Value};

#[path = "blob_image_facts/mod.rs"]
mod blob_image_facts;

/// The encoder every write in this family normalizes through — the host's,
/// as production wires it (P4.104).
const BLOB_WEBP: &dyn quilltap_core::services::mount_index::blob_transcode::WebpTranscoder =
    &quilltap_host::HostImageCodec;

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const FIXED_KEPT_AT: &str = "2026-04-01T12:00:00.000Z";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    name: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    saved_links: Value,
    #[serde(default)]
    image_facts: Option<Value>,
}

// The case inputs — kept identical to `character-photo-upload-tier2.test.ts`.
struct UploadCase {
    name: &'static str,
    data: Vec<u8>,
    filename: &'static str,
    mime: &'static str,
    caption: Option<&'static str>,
    tags: Vec<String>,
    twice: bool,
    /// P4.104: compare the stored `photos/` blobs' D19 facts.
    image_facts: bool,
}

fn cases() -> Vec<UploadCase> {
    vec![
        UploadCase {
            name: "upload_dotless",
            data: b"upload-image-bytes-Alpha".to_vec(),
            filename: "freshupload",
            mime: "image/png",
            caption: Some("A fine portrait"),
            tags: vec!["airship".into(), "adventure".into()],
            twice: false,
            image_facts: false,
        },
        UploadCase {
            name: "upload_dotted",
            data: b"upload-image-bytes-Bravo".to_vec(),
            filename: "Snapshot.PNG",
            mime: "image/png",
            caption: None,
            tags: vec![],
            twice: false,
            image_facts: false,
        },
        UploadCase {
            name: "upload_dedup",
            data: b"upload-image-bytes-Charlie".to_vec(),
            filename: "dup.png",
            mime: "image/png",
            caption: None,
            tags: vec![],
            twice: true,
            image_facts: false,
        },
        UploadCase {
            name: "err_empty",
            data: vec![],
            filename: "empty.png",
            mime: "image/png",
            caption: None,
            tags: vec![],
            twice: false,
            image_facts: false,
        },
        UploadCase {
            name: "err_nonimage",
            data: b"upload-image-bytes-Delta".to_vec(),
            filename: "notes.txt",
            mime: "text/plain",
            caption: None,
            tags: vec![],
            twice: false,
            image_facts: false,
        },
        // P4.104: a DECODABLE image — normalized to WebP by `linkBlobContent`.
        UploadCase {
            name: "upload_real_png",
            data: blob_image_facts::seed_image("photo.png"),
            filename: "Portrait.png",
            mime: "image/png",
            caption: None,
            tags: vec![],
            twice: false,
            image_facts: true,
        },
    ]
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/characters.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-photo-upload-{}-{}", tag, std::process::id()));
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

/// The mount's `photos/` links (raw columns; content-addressed fileId is
/// deterministic), in the oracle's array shape.
fn dump_saved_photo_links(db: &Db) -> Value {
    db.read_mount_index(|mount| {
        let mut stmt = mount.prepare(
            "SELECT relativePath, fileId, originalMimeType, extractedText, description \
             FROM doc_mount_file_links WHERE relativePath LIKE 'photos/%' ORDER BY relativePath",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(json!({
                "relativePath": r.get::<_, String>(0)?,
                "fileId": r.get::<_, String>(1)?,
                "originalMimeType": r.get::<_, Option<String>>(2)?,
                "extractedText": r.get::<_, Option<String>>(3)?,
                "description": r.get::<_, Option<String>>(4)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(Value::Array(out))
    })
    .unwrap()
}

#[test]
fn photo_upload_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PHOTO_UPLOAD") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PHOTO_UPLOAD to the oracle NDJSON (see header).");
            return;
        }
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let mut oracle: std::collections::HashMap<String, Row> = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        oracle.insert(row.name.clone(), row);
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut failures = Vec::new();

    for c in cases() {
        let want = oracle
            .get(c.name)
            .unwrap_or_else(|| panic!("oracle missing {}", c.name));
        let db = fresh_db(&spec, c.name);
        let data = c.data.clone();
        let filename = c.filename.to_string();
        let mime = c.mime.to_string();
        let caption = c.caption.map(str::to_string);
        let tags = c.tags.clone();
        let twice = c.twice;
        let result: Result<Value, GalleryError> = rt
            .block_on(db.write(move |writers| {
                let mount = writers.mount_index().unwrap().connection();
                let main = writers.main().connection();
                if twice {
                    let _ = save_to_character_gallery(
                        main,
                        mount,
                        ARIA,
                        &data,
                        &filename,
                        &mime,
                        caption.as_deref(),
                        &tags,
                        FIXED_KEPT_AT,
                        BLOB_WEBP,
                    );
                }
                Ok(save_to_character_gallery(
                    main,
                    mount,
                    ARIA,
                    &data,
                    &filename,
                    &mime,
                    caption.as_deref(),
                    &tags,
                    FIXED_KEPT_AT,
                    BLOB_WEBP,
                ))
            }))
            .unwrap();

        // Error / body diff.
        match (&result, &want.error) {
            (Err(GalleryError::BadRequest(msg)), Some(want_msg)) => {
                if msg != want_msg {
                    failures.push(format!("[{}] error: got {msg:?} want {want_msg:?}", c.name));
                }
            }
            (Ok(mut_body), None) => {
                let mut got = mut_body.clone();
                got["linkId"] = Value::String("<linkId>".into());
                let want_body = want.body.clone().unwrap_or(Value::Null);
                if got != want_body {
                    failures.push(format!("[{}] body: got {} want {}", c.name, got, want_body));
                }
            }
            (got, want_err) => {
                failures.push(format!(
                    "[{}] outcome mismatch: got {got:?}, oracle error {want_err:?}",
                    c.name
                ));
            }
        }

        // The photos/ link dump matches (deterministic — save the fresh blob's
        // MINTED fileId, blanked on both sides; every fresh upload is new bytes,
        // so linkBlobContent mints a fresh uuid rather than reusing an existing
        // content row).
        let mut got_links = dump_saved_photo_links(&db);
        let mut want_links = want.saved_links.clone();
        blank_file_ids(&mut got_links);
        blank_file_ids(&mut want_links);
        if got_links != want_links {
            failures.push(format!(
                "[{}] savedLinks: got {} want {}",
                c.name, got_links, want.saved_links
            ));
        } else {
            eprintln!("[{}] OK", c.name);
        }

        // P4.104: the D19 image comparand for the decodable row.
        if c.image_facts {
            let got = db
                .read_mount_index(|mount| {
                    Ok(blob_image_facts::blob_image_facts(
                        &blob_image_facts::stored_blob_rows(
                            mount,
                            "WHERE l.relativePath LIKE 'photos/%' ORDER BY l.relativePath",
                            &[],
                        ),
                        &c.data,
                    ))
                })
                .unwrap();
            let got = Value::Array(got);
            match &want.image_facts {
                Some(want_facts) if *want_facts == got => {
                    eprintln!("[{}] imageFacts OK: {got}", c.name)
                }
                Some(want_facts) => failures.push(format!(
                    "[{}] imageFacts: got {got} want {want_facts}",
                    c.name
                )),
                None => failures.push(format!(
                    "[{}] the oracle carries no imageFacts — regenerate",
                    c.name
                )),
            }
        }
    }

    assert!(failures.is_empty(), "mismatches:\n{}", failures.join("\n"));
    eprintln!(
        "OK: photo-upload matched oracle on {} cases.",
        cases().len()
    );
}

/// Blank the minted `fileId` (a fresh uuid per never-before-seen blob) in every
/// link object so the deterministic remainder can be diffed.
fn blank_file_ids(links: &mut Value) {
    if let Some(arr) = links.as_array_mut() {
        for link in arr {
            if let Some(obj) = link.as_object_mut() {
                if obj.contains_key("fileId") {
                    obj.insert("fileId".into(), Value::String("<fileId>".into()));
                }
            }
        }
    }
}
