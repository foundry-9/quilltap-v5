//! Tier-2 (D19-shaped) differential: the blob-image write-side chokepoint
//! (P4.D209 / v4 bug 159, `186eb09cb`).
//!
//! **What is compared, and what deliberately is not.** v4 encodes through
//! `sharp`; v5 through the `image` + `webp` crates in `quilltap-host`. Two
//! different encoders cannot produce byte-identical WebP, so D19 stands: the
//! comparand is the DECISION (did the bytes move?), the `storedMimeType`, the
//! `relativePath`, the `fileName`, whether the sha CHANGED, the direction the
//! size moved, and the DECODED pixel dimensions of whatever came out. Never the
//! encoded bytes, and never the sha's value.
//!
//! The dimensions are what keeps that from being a decision-only test: a port
//! that "transcoded" by handing back a 1×1 placeholder would satisfy every
//! other field. The lossless case is 620×440 and must stay 620×440 through a
//! decode-and-re-encode that changes every byte.
//!
//! The cases are v4's own six plus three the module doc implies: an OMITTED
//! flag must still normalize (the default is `true`, and a skip would silently
//! reintroduce the whole defect), a mime carrying parameters and capitals is
//! still a PNG, and an extension-less path gains `.webp` rather than losing its
//! leaf.
//!
//! **The fifth host seam the work order expected is NOT taken, by measurement.**
//! `HostImageCodec`'s existing `WebpTranscoder::encode_webp` decodes through the
//! `image` crate's format sniffer, and `image-webp` handles a VP8L bitstream, so
//! a lossless WebP already reaches the same lossy encoder every bitmap does.
//! `large_lossless_reencoded_not_renamed` is the proof: it runs through that
//! seam untouched and comes back 620×440 and smaller.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_NORMALIZE_BLOB_IMAGE=$V5W/harness/oracle/fixtures/normalize-blob-image \
//!     $N/npx tsx $V5W/harness/oracle/cases/normalize-blob-image.ts \
//!     > /tmp/oracle-normalize-blob-image.ndjson
//! Run:
//!   QT_ORACLE_NORMALIZE_BLOB_IMAGE=/tmp/oracle-normalize-blob-image.ndjson \
//!     cargo test -p quilltap-harness --test normalize_blob_image_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::services::file_storage::PixelCodec;
use quilltap_core::services::mount_index::normalize_blob_image::{
    normalize_link_blob_image, NormalizableBlob,
};
use quilltap_host::HostImageCodec;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The corpus, kept in lockstep with the oracle case's `CASES` by name and
/// ORDER (the test asserts both).
struct CaseSpec {
    name: &'static str,
    file: &'static str,
    relative_path: &'static str,
    file_name: &'static str,
    stored_mime_type: &'static str,
    /// `None` = the flag is OMITTED, which v4 reads as `true`.
    normalize_images: Option<bool>,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        name: "png_drags_mime_path_name_hash",
        file: "photo.png",
        relative_path: "art/photo.png",
        file_name: "photo.png",
        stored_mime_type: "image/png",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "large_lossless_reencoded_not_renamed",
        file: "photo-lossless.webp",
        relative_path: "art/plate.webp",
        file_name: "plate.webp",
        stored_mime_type: "image/webp",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "small_lossless_under_the_floor_untouched",
        file: "icon-lossless.webp",
        relative_path: "art/icon.webp",
        file_name: "icon.webp",
        stored_mime_type: "image/webp",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "lossy_untouched",
        file: "photo-lossy.webp",
        relative_path: "art/snap.webp",
        file_name: "snap.webp",
        stored_mime_type: "image/webp",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "non_image_untouched",
        file: "notes.txt",
        relative_path: "docs/notes.txt",
        file_name: "notes.txt",
        stored_mime_type: "text/plain",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "flag_false_honoured",
        file: "photo.png",
        relative_path: "art/photo.png",
        file_name: "photo.png",
        stored_mime_type: "image/png",
        normalize_images: Some(false),
    },
    CaseSpec {
        name: "omitted_flag_still_normalizes",
        file: "photo.png",
        relative_path: "art/photo.png",
        file_name: "photo.png",
        stored_mime_type: "image/png",
        normalize_images: None,
    },
    CaseSpec {
        name: "mime_with_parameters_and_case",
        file: "photo.png",
        relative_path: "art/photo.png",
        file_name: "photo.png",
        stored_mime_type: "Image/PNG; charset=binary",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "extensionless_path_gains_webp",
        file: "photo.png",
        relative_path: "art/plate",
        file_name: "plate",
        stored_mime_type: "image/png",
        normalize_images: Some(true),
    },
];

#[derive(Deserialize, Debug, PartialEq)]
struct Row {
    name: String,
    changed: bool,
    #[serde(rename = "storedMimeType")]
    stored_mime_type: String,
    #[serde(rename = "relativePath")]
    relative_path: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "shaChanged")]
    sha_changed: bool,
    #[serde(rename = "bytesGrewOrShrank")]
    bytes_grew_or_shrank: String,
    width: Option<i64>,
    height: Option<i64>,
}

#[derive(Deserialize)]
struct Oracle {
    results: Vec<Row>,
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/normalize-blob-image")
}

#[test]
fn normalize_link_blob_image_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_NORMALIZE_BLOB_IMAGE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_NORMALIZE_BLOB_IMAGE to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");
    assert_eq!(
        oracle.results.len(),
        CASES.len(),
        "oracle covers {} cases, this test has {} — regenerate",
        oracle.results.len(),
        CASES.len()
    );

    let dir = fixtures_dir();
    let codec = HostImageCodec;
    let mut ours: Vec<Row> = Vec::new();

    for case in CASES {
        let data = std::fs::read(dir.join(case.file))
            .unwrap_or_else(|e| panic!("read fixture {}: {e}", case.file));
        let sha = hex::encode(Sha256::digest(&data));
        let input = NormalizableBlob {
            relative_path: case.relative_path.to_string(),
            file_name: case.file_name.to_string(),
            stored_mime_type: case.stored_mime_type.to_string(),
            sha256: sha.clone(),
            data: data.clone(),
        };
        // An omitted flag is `true` — `LinkBlobInput` resolves the default at
        // its own boundary, so the function takes a plain bool.
        let normalize = case.normalize_images.unwrap_or(true);
        let out = normalize_link_blob_image(&input, normalize, Some(&codec));

        let (width, height) = if out.data == data && case.file == "notes.txt" {
            (None, None)
        } else {
            let (w, h) = codec.measure(&out.data);
            (w, h)
        };

        ours.push(Row {
            name: case.name.to_string(),
            changed: out.sha256 != sha,
            stored_mime_type: out.stored_mime_type,
            relative_path: out.relative_path,
            file_name: out.file_name,
            sha_changed: out.sha256 != sha,
            bytes_grew_or_shrank: match out.data.len().cmp(&data.len()) {
                std::cmp::Ordering::Equal => "same",
                std::cmp::Ordering::Less => "smaller",
                std::cmp::Ordering::Greater => "larger",
            }
            .to_string(),
            width,
            height,
        });
    }

    let mut mismatches: Vec<String> = Vec::new();
    for (theirs, ours) in oracle.results.iter().zip(ours.iter()) {
        assert_eq!(
            theirs.name, ours.name,
            "the oracle case list and this test's CASES are out of ORDER — \
             they are transcribed twice on purpose and must agree"
        );
        if theirs != ours {
            mismatches.push(format!(
                "  {}\n    v4 {theirs:?}\n    v5 {ours:?}",
                theirs.name
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "normalize_link_blob_image diverged on {} of {} cases:\n{}",
        mismatches.len(),
        CASES.len(),
        mismatches.join("\n")
    );

    let changed = ours.iter().filter(|r| r.changed).count();
    assert!(
        changed >= 4 && changed < CASES.len(),
        "the corpus must carry both decisions in quantity: {changed} changed of {}",
        CASES.len()
    );
    println!(
        "OK: normalize_link_blob_image matched v4 on {} cases ({changed} normalized, {} left \
         alone) — decision, mime, path, name, size direction and decoded dimensions.",
        CASES.len(),
        CASES.len() - changed
    );
}
