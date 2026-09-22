//! Blob image normalization — the write-side chokepoint (v4
//! `lib/mount-index/normalize-blob-image.ts`, `186eb09cb`, bug 159).
//!
//! Every byte that reaches `doc_mount_blobs` passes through `link_blob_content`,
//! and `link_blob_content` passes it through here first. That is deliberate:
//! transcoding used to be a courtesy each call site could decline, and most of
//! them did.
//!
//! On v4's Friday instance that left 68 untranscoded PNGs (96.5 MB, one avatar
//! at 6.77 MB where the shipped WebP is ~50 KB) and 71 oversized lossless WebP
//! (112 MB). Re-encoded at quality 85 those measure ~7% and ~15% of their stored
//! size respectively. The call sites that skipped transcoding were the
//! filesystem→DB cutover, copy/move in `file_ops`, the sync applier, and the
//! three photo-gallery services.
//!
//! Normalizing here rather than at each caller means a NEW write path cannot
//! reintroduce the problem by forgetting. The one sanctioned way out is
//! `normalize_images: false`, for byte-fidelity restores only.
//!
//! Rewriting the bytes also rewrites `stored_mime_type`, `relative_path` and
//! `file_name`, so the row never claims to hold a PNG while holding WebP — the
//! mismatch that [`normalise_blob_relative_path`] exists to prevent, now applied
//! on every path instead of two.
//!
//! **The seam.** v4 imports `sharp` at module scope; v5 injects the encoder
//! ([`WebpTranscoder`]). A repository built without one normalizes NOTHING and
//! says so once per write — the same code path v4 takes when `sharp` throws
//! (it hands the original bytes back). Only READ/DELETE constructions and the
//! one byte-fidelity write (below) are built that way.
//!
//! **Every write site carries an encoder (P4.104, closing the P4.D209 OPEN
//! note).** v4's `normalize-blob-image.ts:11-15` names the paths that skipped
//! transcoding; v5's census (`blob_write_sites_census`) enumerates every
//! `link_blob_content` / blob `create` call in the core and asserts each builds
//! `with_blob_codec`, bar ONE named exception. The sites and where each gets
//! its encoder:
//!
//! | site | encoder |
//! |---|---|
//! | the sync applier (`sync/apply_store.rs`) | the engine's `blob_webp` (P4.D209) |
//! | `store_mount_file` | its `webp` argument — the engine's `blob_webp` |
//! | `file_ops` copy / move / write | a `webp` argument — the engine's `blob_webp` through the four mount-file arms |
//! | `save_to_character_gallery` (+ the save-by-link leg, the avatar-roll album save) | a `blob_webp` argument — the engine's `blob_webp`, or the byte store's |
//! | `save_to_user_gallery`, `save_image_to_album` | the byte store's [`crate::photos::save_image_to_album::FileBytesStore::blob_webp`] |
//! | `doc_write_blob` | the tool context's `blob_webp` — the tool runner's byte store |
//! | `write_character_avatar_to_vault`, `write_lantern_background_to_mount_store` | a `blob_webp` argument — the engine's, the job handlers', or [`PixelCodecWebp`] over the generate route's codec |
//! | `write_main_avatar_to_vault`, `store_mount_blob` | [`PixelCodecWebp`] over the pixel codec the site already transcodes with |
//! | `save_generated_image` | the image-generation deps' `blob_webp` |
//! | **the `.qtap` import's document stores** | **none — `normalize_images: false`, the ONE byte-fidelity write** |
//!
//! Where a site's chain already carried a pixel codec, the normalization runs
//! through THAT codec ([`PixelCodecWebp`]) rather than a second encoder: v4 has
//! one `sharp`, used by both the bridges' pre-transcode and the normalization,
//! and on every production path the codec is the host's `HostImageCodec` — the
//! same encoder the engine's `blob_webp` holds.
//!
//! **No encoder wired** (a host that supplies none, a canned test store) is
//! decided ONCE for every site: [`blob_codec_or_refusing`] hands over the
//! [`RefusingWebpTranscoder`], whose `Err` takes v4's `sharp`-threw arm — the
//! original bytes stored, the reason said out loud. v4 has no encoder-less
//! write path at all, so there is no quieter faithful arm to choose.
//!
//! **Pre-transcode AND normalize (measured at `f45a517a9`).** v4's
//! `storeMountFile` transcodes when `transcodeImages` (every bridge passes
//! `true`) and `linkBlobContent` then normalizes whatever arrives; the doc-edit
//! tool transcodes and then `docMountBlobs.create` normalizes. v5 keeps both,
//! in that order, through one encoder. After a lossy encode the normalization
//! declines (lossy → lossy is generation loss); it moves bytes where the
//! pre-transcode did not run or passed them through.
//!
//! **Deferred, loudly.** The image re-encode MIGRATION stays deferred
//! reclamation (P4.D203 / P4.D209): a v5 boot never re-encodes existing rows.
//! v4's `mount-index/conversion.ts` (the filesystem → DB cutover, the source of
//! v4's 2026-04-23 batch) has no v5 twin — `db/conversion` was never ported.

use super::blob_transcode::{
    normalise_blob_relative_path, transcode_to_webp, RefusingWebpTranscoder, WebpTranscoder,
};

/// The one refusing encoder every write site falls back to when its host
/// wired none (P4.104). A unit struct, so one static serves them all.
static REFUSING: RefusingWebpTranscoder = RefusingWebpTranscoder;

/// The encoder a blob-write site hands `with_blob_codec`: the host's when it
/// wired one, else [`RefusingWebpTranscoder`] — decided ONCE for every site
/// (P4.104), following the `api/mount_files.rs` precedent. v4 has no
/// encoder-less write path at all (`sharp` is a module-scope import), so there
/// is no faithful "normalize nothing quietly" arm to choose; the refusing
/// encoder takes v4's `sharp`-threw arm instead — the original bytes stored,
/// with the reason said out loud by the encoder's own error.
pub fn blob_codec_or_refusing(codec: Option<&dyn WebpTranscoder>) -> &dyn WebpTranscoder {
    codec.unwrap_or(&REFUSING)
}

/// An owned, optional host encoder for a context struct that derives `Clone`
/// + `Debug` (the doc-edit tool context — P4.104). `Default` is "none wired".
#[derive(Clone, Default)]
pub struct SharedBlobWebp(pub Option<std::sync::Arc<dyn WebpTranscoder>>);

impl SharedBlobWebp {
    /// The encoder a write site hands `with_blob_codec` — see
    /// [`blob_codec_or_refusing`].
    pub fn get(&self) -> &dyn WebpTranscoder {
        blob_codec_or_refusing(self.0.as_deref())
    }
}

impl std::fmt::Debug for SharedBlobWebp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "SharedBlobWebp(wired)"
        } else {
            "SharedBlobWebp(none)"
        })
    }
}

/// A [`crate::services::file_storage::PixelCodec`] seen as the blob
/// [`WebpTranscoder`] (P4.104). v4 has ONE `sharp`: the bridges' pre-transcode
/// (`storeMountFile`'s `transcodeToWebP`) and `linkBlobContent`'s normalization
/// both encode through it, as `sharp(input, { animated: true }).webp({ quality,
/// effort: 4 })` (`blob-transcode.ts`). A write site whose chain already carries a pixel
/// codec therefore normalizes through THAT codec — the same encoder as its own
/// pre-transcode, never a second one that could disagree with it. On every
/// production path the codec is the host's `HostImageCodec`, the same encoder
/// the engine's `blob_webp` holds.
pub struct PixelCodecWebp<P>(pub P);

impl<P> WebpTranscoder for PixelCodecWebp<P>
where
    P: std::ops::Deref + Send + Sync,
    P::Target: crate::services::file_storage::PixelCodec,
{
    fn encode_webp(&self, bytes: &[u8], quality: u8) -> Result<Vec<u8>, String> {
        // v4 `blob-transcode.ts`: `sharp(input, { animated: true })
        // .webp({ quality, effort: 4 })`.
        crate::services::file_storage::PixelCodec::encode_webp(
            &*self.0,
            bytes,
            quality as i64,
            Some(4),
            true,
        )
    }
}

/// The subset of a blob-write input this module reads and may rewrite (v4
/// `NormalizableBlobInput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizableBlob {
    pub relative_path: String,
    pub file_name: String,
    pub stored_mime_type: String,
    pub sha256: String,
    pub data: Vec<u8>,
}

/// Transcode an image blob input to WebP where worthwhile, returning an input
/// with `data`, `sha256`, `stored_mime_type`, `relative_path` and `file_name`
/// updated to agree with each other — v4 `normalizeLinkBlobImage`.
///
/// Returns the input unchanged when normalization is disabled, when the bytes
/// are not a transcodable image, or when `transcode_to_webp` declines (a lossy
/// WebP, a small lossless one, or an encoder failure — all of which it reports
/// by handing the original bytes back).
///
/// `normalize_images` is the caller's flag, NOT an `Option`: v4's
/// `normalizeImages === false` reads an omitted flag as `true`, and
/// `LinkBlobInput` resolves that at its own boundary so the default cannot be
/// lost here.
pub fn normalize_link_blob_image(
    input: &NormalizableBlob,
    normalize_images: bool,
    transcoder: Option<&dyn WebpTranscoder>,
) -> NormalizableBlob {
    if !normalize_images {
        return input.clone();
    }
    let Some(transcoder) = transcoder else {
        // No encoder wired on this host. v4's own catch arm — store the original
        // bytes — with the reason said out loud so a normalization that silently
        // did not happen is never mistaken for one that declined on the merits.
        tracing::debug!(
            target: "quilltap::mount_index",
            relative_path = %input.relative_path,
            stored_mime_type = %input.stored_mime_type,
            "Blob image normalization skipped: no WebP encoder is wired on this host",
        );
        return input.clone();
    };

    // The STORED type is what the serving routes trust, so it — not the original
    // upload's claim — is what decides whether there is work to do.
    let transcoded = transcode_to_webp(&input.data, &input.stored_mime_type, transcoder);
    if !transcoded.transcoded {
        // v4 declines by REFERENCE identity (`transcoded.data === input.data`),
        // which sharp's passthrough preserves and a Rust copy cannot. The
        // `transcoded` flag is that identity, carried explicitly.
        return input.clone();
    }

    let relative_path =
        normalise_blob_relative_path(&input.relative_path, &transcoded.stored_mime_type);
    // `file_name` must track the path, or the link row advertises the old
    // extension while the path carries the new one.
    let file_name = if relative_path == input.relative_path {
        input.file_name.clone()
    } else {
        posix_basename(&relative_path).to_string()
    };

    tracing::debug!(
        target: "quilltap::mount_index",
        relative_path = %input.relative_path,
        final_relative_path = %relative_path,
        from_mime_type = %input.stored_mime_type,
        to_mime_type = %transcoded.stored_mime_type,
        from_bytes = input.data.len(),
        to_bytes = transcoded.size_bytes,
        "Normalized blob image before storage",
    );

    NormalizableBlob {
        relative_path,
        file_name,
        stored_mime_type: transcoded.stored_mime_type,
        sha256: transcoded.sha256,
        data: transcoded.data,
    }
}

/// Node `path.posix.basename` — everything after the last `/`.
fn posix_basename(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

#[cfg(test)]
mod encoder_choice_tests {
    use super::*;
    use crate::services::file_storage::PixelCodec;
    use std::sync::Mutex;

    /// Records what the adapter hands the pixel codec.
    #[derive(Default)]
    struct Recording(Mutex<Vec<(i64, Option<i64>, bool)>>);
    impl PixelCodec for Recording {
        fn encode_webp(
            &self,
            _bytes: &[u8],
            quality: i64,
            effort: Option<i64>,
            animated: bool,
        ) -> Result<Vec<u8>, String> {
            self.0.lock().unwrap().push((quality, effort, animated));
            Ok(b"ENCODED".to_vec())
        }
        fn measure(&self, _bytes: &[u8]) -> (Option<i64>, Option<i64>) {
            (None, None)
        }
    }

    /// v4 `blob-transcode.ts`: `sharp(input, { animated: true }).webp({
    /// quality, effort: 4 })` — the adapter must pass exactly that, whatever
    /// the pixel codec's other callers ask of it.
    #[test]
    fn the_pixel_codec_adapter_encodes_as_v4s_blob_transcode_does() {
        let codec = Recording::default();
        let adapter = PixelCodecWebp(&codec as &dyn PixelCodec);
        assert_eq!(adapter.encode_webp(b"x", 85).unwrap(), b"ENCODED");
        assert_eq!(*codec.0.lock().unwrap(), vec![(85, Some(4), true)]);
    }

    /// No encoder wired → the refusing encoder, whose `Err` takes v4's
    /// `sharp`-threw arm; a wired encoder is handed through untouched.
    #[test]
    fn no_encoder_falls_back_to_the_loud_refusal() {
        assert!(blob_codec_or_refusing(None).encode_webp(b"x", 85).is_err());
        let codec = Recording::default();
        let adapter = PixelCodecWebp(&codec as &dyn PixelCodec);
        assert!(blob_codec_or_refusing(Some(&adapter))
            .encode_webp(b"x", 85)
            .is_ok());
        assert!(SharedBlobWebp::default()
            .get()
            .encode_webp(b"x", 85)
            .is_err());
        assert_eq!(
            format!("{:?}", SharedBlobWebp::default()),
            "SharedBlobWebp(none)"
        );
    }
}
