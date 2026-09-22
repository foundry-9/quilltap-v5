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
//! says so once per write — which is byte-for-byte v4's own behaviour when
//! `sharp` throws (it hands the original bytes back), so the un-wired default is
//! a faithful arm rather than a hole.

use super::blob_transcode::{normalise_blob_relative_path, transcode_to_webp, WebpTranscoder};

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
