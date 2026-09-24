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
//! **P4.108 — the animated-input decline (ruled by the human, 2026-09-23).**
//! Seven more rows over committed animated fixtures (`fixtures/normalize-blob-
//! image/generate.py` makes them). Two are a RULED DIVERGENCE
//! ([`RULED_ANIMATED_DECLINE`]): v4's `sharp(input, { animated: true })` keeps
//! both frames as an animated WebP, and v5's single-frame host codec declines
//! instead, so the store-original fallback keeps both frames as the input. They
//! are pinned in BOTH directions: v4 must show an animated WebP (`pages > 1`,
//! an oracle field kept OUT of `Row`'s equality), v5 must show the input bytes
//! exactly, and the caller's store-original WARN must fire once. The other
//! five (a WebP passthrough, a one-`ANMF` still, two still GIFs, an APNG sharp
//! reads as a still) must MATCH whole-row — they pin that detection counts
//! FRAMES and never over-reaches. This family exercises the codec's
//! `BlobWebpTranscoder` seam; the `PixelCodec` animated seam is proven by the
//! host codec's unit tests.
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
    // P4.108 — the animated-input decline. The two `_declined` rows are the
    // RULED divergence ([`RULED_ANIMATED_DECLINE`]); the other five MATCH.
    CaseSpec {
        name: "anim_gif_declined",
        file: "anim-2frame.gif",
        relative_path: "art/loop.gif",
        file_name: "loop.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
    CaseSpec {
        // An animated WebP reaches the encoder ONLY under a mislabelled mime:
        // `image/webp` is outside the transcodable set, and the lossless check
        // reads top-level chunks only (the bitstream sits inside `ANMF`).
        name: "anim_webp_mislabelled_declined",
        file: "anim-2frame.webp",
        relative_path: "art/loop.gif",
        file_name: "loop.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "anim_webp_passthrough",
        file: "anim-2frame.webp",
        relative_path: "art/loop.webp",
        file_name: "loop.webp",
        stored_mime_type: "image/webp",
        normalize_images: Some(true),
    },
    CaseSpec {
        // The animation bit with ONE `ANMF`: sharp reads one page, so no frame
        // is at stake — detection is by FRAME COUNT, not the bit.
        name: "one_anmf_still",
        file: "one-anmf.webp",
        relative_path: "art/still.gif",
        file_name: "still.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "still_gif_transcoded",
        file: "still-large.gif",
        relative_path: "art/stripes.gif",
        file_name: "stripes.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
    CaseSpec {
        // One frame behind a comment full of 0x2C, the descriptor introducer.
        name: "commas_gif_transcoded",
        file: "still-with-commas.gif",
        relative_path: "art/commas.gif",
        file_name: "commas.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
    CaseSpec {
        // sharp reads an APNG as a still PNG, so v4 loses these frames too and
        // v5 must never decline one.
        name: "apng_still",
        file: "anim-2frame.apng",
        relative_path: "art/loop.png",
        file_name: "loop.png",
        stored_mime_type: "image/png",
        normalize_images: Some(true),
    },
    // P4.112 — two frames, the SECOND corrupt (P4.108's recorded nit). A
    // measurement of sharp, pinned as measured ([`MEASURED_CORRUPT_SECOND_FRAME`]).
    CaseSpec {
        name: "corrupt_second_frame_gif",
        file: "anim-corrupt2.gif",
        relative_path: "art/broken.gif",
        file_name: "broken.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
    CaseSpec {
        name: "corrupt_second_frame_webp_mislabelled",
        file: "anim-corrupt2.webp",
        relative_path: "art/broken.gif",
        file_name: "broken.gif",
        stored_mime_type: "image/gif",
        normalize_images: Some(true),
    },
];

/// P4.108 — rows where v5 deliberately does NOT match v4, with the ruling.
///
/// **Ruled by the human, 2026-09-23 (the `a2db63da7` unification):** the host
/// WebP encoder is single-frame (the `webp` crate, no libwebpmux), so rather
/// than store an animated GIF/WebP as its first frame it DECLINES the encode,
/// and v4's own store-original fallback keeps every frame. v4's
/// `sharp(input, { animated: true })` keeps the frames and re-encodes to an
/// animated WebP. The divergence is therefore on mime, path, name, sha and
/// size — never on a frame. Pinned in BOTH directions below: a row where v4
/// and v5 agree is a "VANISHED" failure, a row whose difference is not this
/// one is a "WRONG SHAPE" failure, and every row named here must have run.
const RULED_ANIMATED_DECLINE: &[(&str, &str)] = &[
    (
        "anim_gif_declined",
        "a two-frame GIF: v4 writes an animated WebP; v5 stores the GIF unchanged",
    ),
    (
        "anim_webp_mislabelled_declined",
        "a two-frame WebP stored as image/gif: v4 re-encodes it animated; v5 stores it unchanged",
    ),
];

/// P4.112 — rows MEASURED against sharp, not ruled: a two-frame input whose
/// SECOND frame cannot decode (P4.108's recorded nit). Measured at the
/// `00c290c9a` pin, 2026-09-23: sharp's `{ animated: true }` transcode THROWS
/// on both inputs, so v4's store-original fallback keeps the input bytes —
/// both frames' worth (sharp's own metadata reads the GIF as `pages: 2`; the
/// WebP it cannot read at all). v5's frame counter stops at the first frame
/// that fails to decode, counts ONE, and the host encoder writes that first
/// frame as a still WebP — so **v5 drops the second frame's bytes that v4
/// keeps**. That is the order's STOP condition: no behaviour change here; the
/// ruling is the human's (P4.112's lane record). Pinned in BOTH directions
/// like the ruled rows: equal → "VANISHED", a different difference → "WRONG
/// SHAPE", every declared row must have run.
const MEASURED_CORRUPT_SECOND_FRAME: &[(&str, &str)] = &[
    (
        "corrupt_second_frame_gif",
        "a GIF whose second frame is corrupt: v4 stores it unchanged (sharp: pages 2, transcode \
         throws); v5 writes the first frame as a still WebP",
    ),
    (
        "corrupt_second_frame_webp_mislabelled",
        "a two-ANMF WebP (stored as image/gif) whose second frame is corrupt: v4 stores it \
         unchanged (sharp cannot read it); v5 writes the first frame as a still WebP",
    ),
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

/// The raw oracle rows, for fields deliberately kept OUT of [`Row`]'s
/// whole-row equality (P4.108's `pages`, read only on the ruled rows).
#[derive(Deserialize)]
struct RawOracle {
    results: Vec<serde_json::Value>,
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
    let text = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle: Oracle = serde_json::from_str(text.trim()).expect("parse oracle");
    let raw: RawOracle = serde_json::from_str(text.trim()).expect("parse oracle (raw)");
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
    // Per case: were the stored bytes the input bytes ("every frame kept"),
    // and how many times did the caller's store-original WARN fire.
    let mut kept_input: Vec<bool> = Vec::new();
    let mut store_original_warns: Vec<usize> = Vec::new();
    // Per case: does v5's OUTPUT carry an animation frame chunk (`ANMF`)?
    let mut out_has_anmf: Vec<bool> = Vec::new();

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
        let (out, lines) = quilltap_core::test_support::captured_with(|| {
            normalize_link_blob_image(&input, normalize, Some(&codec))
        });
        kept_input.push(out.data == data);
        out_has_anmf.push(out.data.windows(4).any(|w| w == b"ANMF"));
        store_original_warns.push(
            lines
                .iter()
                .filter(|l| {
                    l.starts_with("WARN ")
                        && l.contains("Failed to transcode blob to WebP; storing original bytes")
                })
                .count(),
        );

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
    let mut seen_ruled: Vec<&str> = Vec::new();
    let mut seen_measured: Vec<&str> = Vec::new();
    for (i, (theirs, ours)) in oracle.results.iter().zip(ours.iter()).enumerate() {
        assert_eq!(
            theirs.name, ours.name,
            "the oracle case list and this test's CASES are out of ORDER — \
             they are transcribed twice on purpose and must agree"
        );
        let case = &CASES[i];
        if let Some((_, why)) = RULED_ANIMATED_DECLINE
            .iter()
            .find(|(n, _)| *n == theirs.name)
        {
            seen_ruled.push(case.name);
            if theirs == ours {
                mismatches.push(format!(
                    "  {}: the ruled divergence VANISHED — v4 and v5 agree ({why}); \
                     if the host codec now encodes animation, retire the ruling's row\n    \
                     both {ours:?}",
                    theirs.name
                ));
                continue;
            }
            let v4_pages = raw.results[i].get("pages").and_then(|p| p.as_i64());
            // v4: kept the frames and re-encoded them as an animated WebP.
            let v4_shape = theirs.changed
                && theirs.sha_changed
                && theirs.stored_mime_type == "image/webp"
                && theirs.relative_path.ends_with(".webp")
                && v4_pages.is_some_and(|p| p > 1);
            // v5: declined — the input exactly, every frame kept.
            let v5_shape = !ours.changed
                && !ours.sha_changed
                && ours.stored_mime_type == case.stored_mime_type
                && ours.relative_path == case.relative_path
                && ours.file_name == case.file_name
                && ours.bytes_grew_or_shrank == "same"
                && kept_input[i];
            let same_picture = theirs.width == ours.width && theirs.height == ours.height;
            if !(v4_shape && v5_shape && same_picture) {
                mismatches.push(format!(
                    "  {}: the ruled divergence has the WRONG SHAPE ({why}) — \
                     v4 animated={v4_shape} (pages {v4_pages:?}), v5 declined={v5_shape} \
                     (bytes kept {}), same dimensions={same_picture}\n    v4 {theirs:?}\n    v5 {ours:?}",
                    theirs.name, kept_input[i]
                ));
            }
            // The decline is the codec's `Err`; the caller's WARN is what
            // says so out loud — once per declined write.
            if store_original_warns[i] != 1 {
                mismatches.push(format!(
                    "  {}: the store-original WARN fired {} times, want exactly 1",
                    theirs.name, store_original_warns[i]
                ));
            }
            continue;
        }
        if let Some((_, why)) = MEASURED_CORRUPT_SECOND_FRAME
            .iter()
            .find(|(n, _)| *n == theirs.name)
        {
            seen_measured.push(case.name);
            if theirs == ours {
                mismatches.push(format!(
                    "  {}: the measured divergence VANISHED — v4 and v5 agree ({why}); record \
                     what moved and retire the row\n    both {ours:?}",
                    theirs.name
                ));
                continue;
            }
            let v4_pages = raw.results[i].get("pages").and_then(|p| p.as_i64());
            // v4: sharp threw, the store-original fallback kept the input.
            let v4_kept = !theirs.changed
                && !theirs.sha_changed
                && theirs.stored_mime_type == case.stored_mime_type
                && theirs.relative_path == case.relative_path
                && theirs.file_name == case.file_name
                && theirs.bytes_grew_or_shrank == "same"
                // The GIF: sharp still counts both frames on what it kept.
                && (case.file != "anim-corrupt2.gif" || v4_pages == Some(2));
            // v5: counted one frame and encoded it — a still WebP of the
            // first frame's size, no decline.
            let v5_first_frame = ours.changed
                && ours.sha_changed
                && ours.stored_mime_type == "image/webp"
                && ours.relative_path.ends_with(".webp")
                && ours.width == Some(32)
                && ours.height == Some(24)
                && !kept_input[i]
                && !out_has_anmf[i]
                && store_original_warns[i] == 0;
            if !(v4_kept && v5_first_frame) {
                mismatches.push(format!(
                    "  {}: the measured divergence has the WRONG SHAPE ({why}) — v4 kept={v4_kept} \
                     (pages {v4_pages:?}), v5 first-frame webp={v5_first_frame} (anmf {}, \
                     warns {})\n    v4 {theirs:?}\n    v5 {ours:?}",
                    theirs.name, out_has_anmf[i], store_original_warns[i]
                ));
            }
            continue;
        }
        if store_original_warns[i] != 0 {
            mismatches.push(format!(
                "  {}: the store-original WARN fired {} times on a row that is not declined",
                theirs.name, store_original_warns[i]
            ));
        }
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

    // Every declared ruled row must have run, or deleting it from the corpus
    // would silently retire the divergence's measurement.
    for (name, why) in RULED_ANIMATED_DECLINE {
        assert!(
            seen_ruled.contains(name),
            "the corpus carries no '{name}' row, so the ruled divergence ({why}) is unproven — \
             regenerate the oracle from the case file that defines it"
        );
    }

    for (name, why) in MEASURED_CORRUPT_SECOND_FRAME {
        assert!(
            seen_measured.contains(name),
            "the corpus carries no '{name}' row, so the measured divergence ({why}) is \
             unproven — regenerate the oracle from the case file that defines it"
        );
    }

    // Both decisions in quantity, counted on v5's side: the two ruled rows
    // count as UNCHANGED here (v5 declines them), and the row-level diff above
    // is what proves each one.
    let changed = ours.iter().filter(|r| r.changed).count();
    assert!(
        changed >= 4 && changed < CASES.len(),
        "the corpus must carry both decisions in quantity: {changed} changed of {}",
        CASES.len()
    );
    println!(
        "OK: normalize_link_blob_image matched v4 on {} cases ({} normalized, {} left \
         alone) — decision, mime, path, name, size direction and decoded dimensions — \
         diverged as RULED on the {} animated-decline rows, and as MEASURED on the {} \
         corrupt-second-frame rows.",
        CASES.len() - RULED_ANIMATED_DECLINE.len() - MEASURED_CORRUPT_SECOND_FRAME.len(),
        // v5 normalizes the measured rows, so they come off `changed` here.
        changed - MEASURED_CORRUPT_SECOND_FRAME.len(),
        // Left alone AMONG the matched rows: v5 leaves the ruled rows alone
        // too, so they come off this count as well as off the total (the
        // `00c290c9a` unification's review — "9 + 7" of 14 did not add up).
        CASES.len() - changed - RULED_ANIMATED_DECLINE.len(),
        RULED_ANIMATED_DECLINE.len(),
        MEASURED_CORRUPT_SECOND_FRAME.len()
    );
}

/// P4.104 (Tier 2 item 6) — the REPOSITORY path, not the function. The same
/// decodable PNG goes through `DocMountBlobsRepository::with_blob_codec(…)
/// .create(…)` → `DocMountFileLinksRepository::link_blob_content` over a real
/// connection, and what the row reads back as must equal v4's
/// `png_drags_mime_path_name_hash` row. This is what proves the facade's
/// readback (it answers the `.webp` row normalization moved the write to — the
/// P4.104 unit 1 fix) and that the normalized sha reaches BOTH
/// `doc_mount_files.sha256` and `doc_mount_blobs.sha256`, describing the stored
/// bytes.
#[test]
fn the_repository_path_matches_the_oracle_png_row() {
    use quilltap_core::db::doc_mount_blobs::{CreateBlobInput, DocMountBlobsRepository};

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
    let want = oracle
        .results
        .iter()
        .find(|r| r.name == "png_drags_mime_path_name_hash")
        .expect("the oracle carries the PNG row");

    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL, sha256 TEXT NOT NULL,
            fileSizeBytes REAL, fileType TEXT NOT NULL, source TEXT NOT NULL,
            createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);
         CREATE TABLE doc_mount_folders (
            id TEXT PRIMARY KEY NOT NULL, mountPointId TEXT NOT NULL, path TEXT NOT NULL,
            name TEXT NOT NULL, parentId TEXT, createdAt TEXT NOT NULL,
            updatedAt TEXT NOT NULL);
         CREATE TABLE doc_mount_file_links (
            id TEXT PRIMARY KEY NOT NULL, fileId TEXT NOT NULL, linkGroupId TEXT,
            mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL, fileName TEXT NOT NULL,
            folderId TEXT, originalFileName TEXT, originalMimeType TEXT,
            description TEXT, descriptionUpdatedAt TEXT, conversionStatus TEXT NOT NULL,
            conversionError TEXT, plainTextLength REAL, extractedText TEXT,
            extractedTextSha256 TEXT, extractionStatus TEXT NOT NULL, extractionError TEXT,
            chunkCount REAL NOT NULL DEFAULT 0, allowEmbed REAL NOT NULL DEFAULT 1,
            allowCharacterRead REAL NOT NULL DEFAULT 1,
            allowCharacterWrite REAL NOT NULL DEFAULT 1,
            lastModified TEXT NOT NULL, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
    )
    .unwrap();

    let data = std::fs::read(fixtures_dir().join("photo.png")).unwrap();
    let sha = hex::encode(Sha256::digest(&data));
    let codec = HostImageCodec;
    let found = DocMountBlobsRepository::with_blob_codec(&conn, &codec)
        .create(&CreateBlobInput {
            mount_point_id: "mp-1".to_string(),
            relative_path: "art/photo.png".to_string(),
            original_file_name: Some("photo.png".to_string()),
            original_mime_type: Some("image/png".to_string()),
            stored_mime_type: "image/png".to_string(),
            sha256: sha.clone(),
            data: data.clone(),
            description: None,
            file_name: Some("photo.png".to_string()),
            file_type: None,
            normalize_images: true,
        })
        .expect("the normalized write reads back through the facade");

    let stored = DocMountBlobsRepository::new(&conn)
        .read_data_by_file_id(&found.file_id)
        .unwrap()
        .expect("the stored bytes");
    let (width, height) = codec.measure(&stored);
    let ours = Row {
        name: want.name.clone(),
        changed: found.sha256 != sha,
        stored_mime_type: found.stored_mime_type.clone(),
        relative_path: found.relative_path.clone(),
        file_name: found.file_name.clone(),
        sha_changed: found.sha256 != sha,
        bytes_grew_or_shrank: match stored.len().cmp(&data.len()) {
            std::cmp::Ordering::Equal => "same",
            std::cmp::Ordering::Less => "smaller",
            std::cmp::Ordering::Greater => "larger",
        }
        .to_string(),
        width,
        height,
    };
    assert_eq!(
        &ours, want,
        "the repository path diverged from v4's function row"
    );

    let stored_sha = hex::encode(Sha256::digest(&stored));
    let file_sha: String = conn
        .query_row(
            "SELECT sha256 FROM doc_mount_files WHERE id = ?1",
            [&found.file_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        found.sha256, stored_sha,
        "doc_mount_blobs.sha256 hashes the stored bytes"
    );
    assert_eq!(
        file_sha, stored_sha,
        "doc_mount_files.sha256 hashes the stored bytes"
    );
    println!("OK: the repository path matched v4's PNG row and propagated the stored sha.");
}
