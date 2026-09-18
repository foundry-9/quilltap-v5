//! P4.D198 — two pins on the bug-151 loader path that no differential can see.
//!
//! Both are about v5's OWN composition, so neither has an oracle: v4's side of
//! each question is settled by reading `bcd7e4852`'s hunks, and what needs
//! proving is that this port's wiring agrees.
//!
//! 1. **The production transcoder SHRINKS** (`the_production_loader_arm_reaches_
//!    a_shrinking_transcoder`). `shrink_to_webp` has a default body returning
//!    `Err`, so every implementor compiles — which means a loader handed the
//!    wrong transcoder would silently send stored bytes forever, warning once
//!    per image and looking exactly like a provider that cannot resize. The
//!    P4.73 class ("every production call site had the not-configured codec")
//!    is the precedent.
//!
//! 2. **The shrink runs BEFORE the provider-ceiling backstop, and the backstop
//!    is handed the shrink's output mime** (`the_shrink_runs_before_the_
//!    backstop_...`). Both are corpus-invisible, MEASURED: mutating either one
//!    leaves `file_attachment_tier3` green, because every image in that corpus
//!    is under the 4 MB per-image provider cap and so the backstop never acts
//!    at all — which is v4's own observation about its new code ("this fires
//!    only for a format it had to pass through"). The pin below is the "count
//!    the seam calls" instrument the order named, made discriminating by giving
//!    the backstop something to do: an image whose ladder BOTTOM is still over
//!    the provider cap. The recorded format is what separates the two
//!    mutations, since `determine_output_format` answers `Webp` for the
//!    shrink's `image/webp` output and `Jpeg` for the stored `image/png`.
//!
//! Run standalone (no oracle — the comparand is v5's own composition):
//!   cargo test -p quilltap-harness --test lantern_loader_transcoder_wiring

use std::sync::Mutex;

use quilltap_core::db::files::FileEntry;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::files::image_processing::{
    ImageMetadata, ImageTranscoder, OutputFormat, DEFAULT_MAX_BASE64_SIZE,
};
use quilltap_core::services::chat_files::{
    load_chat_files_for_llm, FileBytesStore, LoadChatFilesOptions,
};
use quilltap_harness::scripted_transcoder::original_bytes;

/// The test pepper the committed fixtures are keyed with.
const PEPPER: &str = "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHktMzJieXRl";
/// A `files` row the committed images fixture seeds, with a non-empty
/// `storageKey` (the loader refuses a row without one; the BYTES come from the
/// injected store, so the key's target is never read).
const FILE_ID: &str = "f0000000-0000-4000-8000-000000000001";

// ===========================================================================
// 1. The production transcoder shrinks
// ===========================================================================

/// The loaders' transcoder in production comes from
/// `quilltap_host::spine::ChatSpine::image_transcoder`, which is typed as the
/// CONCRETE `Arc<HostImageCodec>` and threaded into `OrchestratorDeps` at both
/// `OrchestratorDeps` constructions, then into `chat_files`'s
/// `ProcessFilesDeps` / `RealMessageContextSeams`. Two halves, both checked:
/// the codec really shrinks (run here against real bytes), and the wire really
/// names it (a source census — the `db_error_key_guard` idiom, since the wire
/// is a type, not a value a test can read back).
#[test]
fn the_production_loader_arm_reaches_a_shrinking_transcoder() {
    use quilltap_host::image_codec::HostImageCodec;

    // --- the functional half: `HostImageCodec` is not the default `Err`. ---
    //
    // A 1400x900 binary PPM of deterministic noise. PPM rather than PNG or WebP
    // because the harness links no image codec of its own (and is not gaining
    // one for a test): `P6` is a header plus raw RGB, which the host codec's
    // `image`-crate decoder reads directly. Noise, not flat colour, so the
    // "it really shrank" assertion means something.
    let codec = HostImageCodec;
    let source = {
        let (w, h) = (1400usize, 900usize);
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        let mut state: u32 = 0xfeed_face;
        for _ in 0..(w * h) {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = state.to_le_bytes();
            out.extend_from_slice(&[b[0], b[1], b[2]]);
        }
        out
    };
    let shrunk = ImageTranscoder::shrink_to_webp(&codec, &source, 1024, 78)
        .expect("the production codec must implement the shrink, not inherit the default Err");
    assert!(
        shrunk.len() < source.len(),
        "the production codec's shrink must actually shrink: {} -> {}",
        source.len(),
        shrunk.len()
    );
    assert_eq!(
        &shrunk[0..4],
        b"RIFF",
        "the shrink's output must be WebP (a RIFF container)"
    );
    assert_eq!(&shrunk[8..12], b"WEBP");

    // And the whole budget, over the real codec end to end.
    let r = quilltap_core::files::llm_image_budget::shrink_image_for_llm_transport(
        &codec,
        &source,
        "image/png",
        Some("NANOGPT"),
        Some("wire.png"),
    );
    assert!(r.was_shrunk, "the budget over the real codec must shrink");
    assert_eq!(r.mime_type, "image/webp");
    assert_eq!(r.width.unwrap().max(r.height.unwrap()), 1024);

    // --- the wire half: the source census. ---
    let spine = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../quilltap-host/src/spine.rs"),
    )
    .expect("read the host spine");
    assert_eq!(
        spine
            .matches("pub image_transcoder: Arc<HostImageCodec>")
            .count(),
        1,
        "ChatSpine's transcoder field must stay the CONCRETE HostImageCodec — a \
         widening to `Arc<dyn ImageTranscoder>` would let a not-configured codec \
         reach the loaders without any test noticing"
    );
    assert_eq!(
        spine
            .matches("image_transcoder: Arc::new(HostImageCodec)")
            .count(),
        1,
        "the production spine must build its transcoder from HostImageCodec"
    );
    assert_eq!(
        spine
            .matches("image_transcoder: &*self.image_transcoder")
            .count(),
        2,
        "both OrchestratorDeps constructions in the spine must hand the loaders \
         the spine's own codec"
    );

    // Every `NotConfiguredTranscoder` in the core is TEST-only: measured at
    // P4.D198 as exactly two sites, one in `orchestrator.rs` and one in
    // `enclave/step.rs`, each below its file's first `#[cfg(test)]`.
    for rel in [
        "../quilltap-core/src/services/orchestrator.rs",
        "../quilltap-core/src/enclave/step.rs",
    ] {
        let src =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
                .unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let uses: Vec<usize> = src
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains("NotConfiguredTranscoder"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(uses.len(), 1, "{rel}: expected one NotConfiguredTranscoder");
        let first_test = src
            .lines()
            .position(|l| l.trim_start().starts_with("#[cfg(test)]"))
            .unwrap_or_else(|| panic!("{rel}: no #[cfg(test)] module"));
        assert!(
            uses[0] > first_test,
            "{rel}: a NotConfiguredTranscoder at line {} sits OUTSIDE the test \
             module (first #[cfg(test)] at {}) — a production loader would send \
             stored bytes and warn once per image",
            uses[0] + 1,
            first_test + 1
        );
    }
}

// ===========================================================================
// 2. The shrink runs first, and the backstop gets its output mime
// ===========================================================================

/// One seam call, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeamCall {
    /// `shrink_to_webp(max_edge, quality)` — the transport budget's ladder.
    Shrink { max_edge: i64, quality: i64 },
    /// `resize_step(target_width, format, quality)` — the provider-ceiling
    /// backstop. The FORMAT is what tells the two mutations apart.
    Resize {
        target_width: i64,
        format: OutputFormat,
        quality: i64,
    },
}

/// A transcoder that answers BOTH seams and records the order it was asked in.
struct RecordingTranscoder {
    /// The bytes every ladder rung returns — deliberately the input's length, so
    /// the ladder never fits the transport ceiling AND the backstop still has
    /// something over the provider cap to do.
    rung_len: usize,
    /// The bytes `resize_step` returns: small enough that the backstop's first
    /// iteration clears the cap and returns.
    resize_len: usize,
    log: Mutex<Vec<SeamCall>>,
}

impl ImageTranscoder for RecordingTranscoder {
    fn metadata(&self, buffer: &[u8]) -> ImageMetadata {
        // Keyed on CONTENT, not length: every ladder rung deliberately returns
        // a buffer the SAME length as the stored image (that is how the ladder
        // never fits and the backstop still has work), so a length test would
        // report the stored 4000x3000 for the shrunk image and the backstop's
        // first ×0.8 step would come out 3200 instead of 819. This fake's own
        // products are runs of `S` (a ladder rung) and `R` (a backstop step);
        // the stored original is `original_bytes`' step-7 pattern.
        match buffer.first() {
            Some(&b'S') | Some(&b'R') => ImageMetadata {
                // Post-shrink: the long edge is now at the transport cap.
                width: Some(1024),
                height: Some(768),
                has_alpha: false,
            },
            _ => ImageMetadata {
                // The stored image: over-1024 on its long edge, so the budget's
                // early return cannot fire whatever its byte size.
                width: Some(4000),
                height: Some(3000),
                has_alpha: false,
            },
        }
    }

    fn resize_step(
        &self,
        _buffer: &[u8],
        target_width: i64,
        format: OutputFormat,
        quality: i64,
    ) -> Vec<u8> {
        self.log.lock().unwrap().push(SeamCall::Resize {
            target_width,
            format,
            quality,
        });
        vec![b'R'; self.resize_len]
    }

    fn shrink_to_webp(
        &self,
        _buffer: &[u8],
        max_edge: i64,
        quality: i64,
    ) -> Result<Vec<u8>, String> {
        self.log
            .lock()
            .unwrap()
            .push(SeamCall::Shrink { max_edge, quality });
        Ok(vec![b'S'; self.rung_len])
    }
}

struct OneFileBytes(Vec<u8>);
impl FileBytesStore for OneFileBytes {
    fn download_file(&self, _entry: &FileEntry) -> Result<Vec<u8>, String> {
        Ok(self.0.clone())
    }
}

#[test]
fn the_shrink_runs_before_the_backstop_and_hands_it_the_shrunk_mime() {
    // A 3.2 MB stored image: base64 4,266,667 is over the 4 MiB default
    // per-image cap, which is the ONLY way to give the backstop work to do and
    // so the only way either mutation becomes observable.
    let stored = original_bytes(3_200_000);
    assert!(
        quilltap_core::files::image_processing::calculate_base64_size(stored.len())
            > DEFAULT_MAX_BASE64_SIZE,
        "the pin's premise: the stored image must exceed the provider cap"
    );

    let dir = std::env::temp_dir().join(format!("qt-p4d198-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.db");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../quilltap-web/tests/fixtures/images-main.db"),
        &main,
    )
    .unwrap();
    // The committed fixture's row is `image/webp`, which the shrink would leave
    // unchanged — and then the backstop's format would be `Webp` either way and
    // the mime mutation would stay invisible. Re-stamp it `image/png` in the
    // SCRATCH copy so the shrink's `image/webp` output is a real change. (A
    // plant in a scratch copy, not a differential fixture: this pin has no
    // oracle, and the comparand is v5's own call order.)
    {
        let w = quilltap_core::db::Writer::open_writable(&main, PEPPER).unwrap();
        w.connection()
            .execute(
                "UPDATE files SET mimeType = 'image/png' WHERE id = ?1",
                rusqlite::params![FILE_ID],
            )
            .unwrap();
    }

    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open the seeded fixture");

    let transcoder = RecordingTranscoder {
        rung_len: stored.len(),
        resize_len: 100,
        log: Mutex::new(Vec::new()),
    };
    let bytes = OneFileBytes(stored.clone());
    let attachments = load_chat_files_for_llm(
        &db,
        &bytes,
        &transcoder,
        &[FILE_ID.to_string()],
        &LoadChatFilesOptions::with_provider(Some("NANOGPT".to_string())),
    );

    let log = transcoder.log.lock().unwrap().clone();
    // The shrink's four rungs (nothing fits the 500 KiB ceiling), THEN one
    // backstop step — which is handed `Webp`, because `determine_output_format`
    // reads the shrink's `image/webp` output and not the stored `image/png`.
    assert_eq!(
        log,
        vec![
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 78
            },
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 65
            },
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 55
            },
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 45
            },
            // round(1024 * 0.8) = 819 — the backstop's first ×0.8 step off the
            // SHRUNK image's width, not the stored 4000.
            SeamCall::Resize {
                target_width: 819,
                format: OutputFormat::Webp,
                quality: 85
            },
        ],
        "the seam call order and the backstop's format. A shrink that ran AFTER \
         the backstop would put the Resize first AND ask for Jpeg (the stored \
         PNG has no alpha, so `determine_output_format` falls through to JPEG); \
         a backstop handed the STORED mime would ask for Jpeg in place. Both of \
         those mutations leave `file_attachment_tier3` green — this is the only \
         thing that sees them."
    );

    // And the composition's result, so the pin is not only about call order.
    assert_eq!(attachments.len(), 1);
    let a = &attachments[0];
    assert_eq!(
        a["mimeType"].as_str(),
        Some("image/webp"),
        "the backstop's output mime reaches the descriptor"
    );
    assert_eq!(
        a["size"].as_f64(),
        Some(2048.0),
        "the legacy path keeps the STORED `files.size` — v4 does not re-derive \
         it here even when `data` shrank (`chat-files-v2.ts` `loadChatFilesForLLM`)"
    );
    use base64::Engine;
    assert_eq!(
        a["data"].as_str().map(|d| d.len()),
        Some(
            base64::engine::general_purpose::STANDARD
                .encode(vec![b'R'; 100])
                .len()
        ),
        "the bytes on the wire are the backstop's 100-byte output, not the 3.2 MB stored ones"
    );

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The committed `images-mount.db` link whose file id is NOT in `images-main.db`'s
/// `files` (so `load_chat_files_for_llm` falls through to the MOUNT path) and
/// whose blob the scratch copy re-stamps: `in-use.webp`.
const MOUNT_LINK_ID: &str = "5349c92e-b239-420b-a8d8-7508b02cf7cd";
const MOUNT_FILE_ID: &str = "0dffc1e1-ba0a-4530-8ed0-3027da543650";

/// The MOUNT-path twin of the pin above — v4's own "where every character
/// avatar actually travels" (`chat-files-v2.ts` `loadMountFileAsAttachment`),
/// landed at the `bcd7e4852` unification (the §3 review found the legacy pin
/// alone: swapping the mount loader's two stages, or handing its backstop
/// `blob.stored_mime_type`, left `file_attachment_tier3` green because the
/// backstop never acts in that corpus). Same shape, same discriminators; the
/// bytes come from the blob row, not the byte store, and the descriptor's
/// `size` is the POST-processing length — the mount path's own quirk.
#[test]
fn the_mount_path_shrinks_before_its_backstop_too() {
    let stored = original_bytes(3_200_000);
    assert!(
        quilltap_core::files::image_processing::calculate_base64_size(stored.len())
            > DEFAULT_MAX_BASE64_SIZE,
        "the pin's premise: the stored blob must exceed the provider cap"
    );

    let dir = std::env::temp_dir().join(format!("qt-p4d198-mount-order-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures");
    let main = dir.join("main.db");
    let mount = dir.join("mount.db");
    std::fs::copy(fixtures.join("images-main.db"), &main).unwrap();
    std::fs::copy(fixtures.join("images-mount.db"), &mount).unwrap();
    // Re-stamp the committed 34-byte `image/webp` blob as a 3.2 MB `image/png`
    // in the SCRATCH copy — the same reason as the legacy pin: a stored WebP
    // would make the shrink's `image/webp` output invisible to the mime
    // mutation. (A plant in a scratch copy, not a differential fixture.)
    {
        let w = quilltap_core::db::Writer::open_writable(&mount, PEPPER).unwrap();
        let n = w
            .connection()
            .execute(
                "UPDATE doc_mount_blobs SET data = ?1, storedMimeType = 'image/png', sizeBytes = ?2 \
                 WHERE fileId = ?3",
                rusqlite::params![stored, stored.len() as i64, MOUNT_FILE_ID],
            )
            .unwrap();
        assert_eq!(n, 1, "the committed fixture's `in-use.webp` blob row");
    }

    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: Some(mount.clone()),
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open the seeded pair");

    let transcoder = RecordingTranscoder {
        rung_len: stored.len(),
        resize_len: 100,
        log: Mutex::new(Vec::new()),
    };
    // The byte store must NOT be consulted on the mount path; a store that
    // answers would mask a loader that resolved the wrong way.
    struct NoBytes;
    impl FileBytesStore for NoBytes {
        fn download_file(&self, e: &FileEntry) -> Result<Vec<u8>, String> {
            panic!(
                "the mount path must not read the byte store (asked for {})",
                e.id
            )
        }
    }
    let attachments = load_chat_files_for_llm(
        &db,
        &NoBytes,
        &transcoder,
        &[MOUNT_LINK_ID.to_string()],
        &LoadChatFilesOptions::with_provider(Some("NANOGPT".to_string())),
    );

    let log = transcoder.log.lock().unwrap().clone();
    assert_eq!(
        log,
        vec![
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 78
            },
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 65
            },
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 55
            },
            SeamCall::Shrink {
                max_edge: 1024,
                quality: 45
            },
            SeamCall::Resize {
                target_width: 819,
                format: OutputFormat::Webp,
                quality: 85
            },
        ],
        "the MOUNT loader's seam call order and the backstop's format: a shrink \
         that ran AFTER the backstop would put the Resize first AND ask for Jpeg; \
         a backstop handed the STORED `image/png` would ask for Jpeg in place."
    );

    assert_eq!(attachments.len(), 1);
    let a = &attachments[0];
    assert_eq!(a["id"].as_str(), Some(MOUNT_LINK_ID));
    assert_eq!(a["filename"].as_str(), Some("in-use.webp"));
    assert_eq!(
        a["mimeType"].as_str(),
        Some("image/webp"),
        "the backstop's output mime reaches the mount descriptor"
    );
    assert_eq!(
        a["size"]
            .as_u64()
            .or_else(|| a["size"].as_f64().map(|f| f as u64)),
        Some(100),
        "the mount path's `size` is the POST-processing length (v4 `:663`), unlike the legacy path"
    );
    use base64::Engine;
    assert_eq!(
        a["data"].as_str(),
        Some(
            base64::engine::general_purpose::STANDARD
                .encode(vec![b'R'; 100])
                .as_str()
        ),
        "the bytes on the wire are the backstop's 100-byte output"
    );

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}
