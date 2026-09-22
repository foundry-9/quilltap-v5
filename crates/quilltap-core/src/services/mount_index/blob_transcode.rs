//! Port of v4 `lib/mount-index/blob-transcode.ts` — the Scriptorium blob
//! upload's WebP normalisation. Non-image MIME types pass through untouched;
//! decodable bitmaps transcode to WebP via the [`WebpTranscoder`] seam. v4's
//! `transcodeToWebP` falls back to the ORIGINAL bytes on any encode failure —
//! the refusing default seam takes that same fallback arm loudly (the
//! production codec is deferred machinery of P4.6v/P4.6y tier 3: a real encoder
//! can never be byte-identical to sharp's WebP anyway, so the fallback arm is
//! the differential-pinnable one).
//!
//! ## Already-WebP uploads (v4 `186eb09cb`, bug 159)
//!
//! A *lossy* WebP is stored as-is: re-encoding lossy→lossy is generation loss
//! for a modest saving, so it is never worth it.
//!
//! A *lossless* WebP (a `VP8L` chunk) is a different animal and IS re-encoded
//! once it exceeds [`LOSSLESS_WEBP_REENCODE_MIN_BYTES`]. Lossless WebP of a
//! photographic image runs ~7× the size of the same picture at quality 85 —
//! v4 measured 1.9 MB vs 0.29 MB on real 1536×1024 story backgrounds. The size
//! floor spares small lossless assets (icons, diagrams, screenshots with hard
//! edges) where lossless is the right encoding and the saving is trivial.
//!
//! ## The reclamation migration: recorded, not run (P4.D209 item 6)
//!
//! `186eb09cb` ships more than the write-side fix. It also ships a ONE-WAY
//! migration, `recompress-oversized-mount-blobs-v1` (`dependsOn:
//! ['sqlite-initial-schema-v1']`), which sweeps the blobs already in a store:
//! every row whose `storedMimeType` is one of png/jpeg/jpg/gif/heic/heif/tiff/
//! avif, plus up to `SAMPLE_LIMIT = 64` sniffed WebP rows, re-encoded in a
//! per-candidate transaction that rewrites `doc_mount_blobs`
//! (`data, sha256, sizeBytes, storedMimeType, updatedAt`), then
//! `doc_mount_files` (`sha256, fileSizeBytes, updatedAt`), then — best-effort,
//! in the MAIN db, keyed on the OLD hash — `files.sha256`, which is bug 117's
//! invariant. `relativePath` is deliberately left alone.
//!
//! **v5 runs no reclamation this round, by decision.** Three reasons, in order
//! of weight: (1) on any instance shared with v4 the migration has ALREADY run,
//! so a v5 pass would find nothing and could only do harm; (2) it is one-way
//! and rewrites the content-addressed hashes that dedup and every storage key
//! are built on, which is not a thing to land beside the write-side fix it
//! depends on; (3) v5's migration runner is itself still deferred, so there is
//! no ledger-guarded place to put it. A future order may re-home it as a
//! boot heal behind bug 117's `files.sha256` invariant — at which point the
//! first question is whether the ledger already records v4 having run it.
//!
//! **The decode side needs no new host seam** — measured, not assumed: the
//! existing [`WebpTranscoder`] impl (`quilltap-host::HostImageCodec`) decodes
//! through the `image` crate's format sniffer, and `image-webp` handles a VP8L
//! bitstream, so a lossless WebP reaches the same lossy encoder every bitmap
//! does. The work order's "fifth host image seam" is therefore NOT taken; the
//! fourth one already answers.

use sha2::{Digest, Sha256};

/// MIME types v4's sharp path transcodes (image/webp deliberately absent:
/// already-WebP uploads store as-is).
const TRANSCODABLE_MIME_TYPES: [&str; 8] = [
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/gif",
    "image/heic",
    "image/heif",
    "image/tiff",
    "image/avif",
];

/// Lossless WebP inputs at or above this many bytes are re-encoded to lossy
/// WebP (v4 `186eb09cb`). Below it, the saving does not justify discarding a
/// deliberate lossless encoding (icons, diagrams, hard-edged screenshots).
pub const LOSSLESS_WEBP_REENCODE_MIN_BYTES: usize = 512 * 1024;

/// Report whether a buffer is a lossless WebP — v4 `isLosslessWebP`
/// (`blob-transcode.ts:63`, `186eb09cb`).
///
/// A WebP file is a RIFF container: `RIFF` + u32 size + `WEBP`, then a chunk
/// sequence. Lossless image data lives in a `VP8L` chunk; lossy lives in
/// `VP8 `. A `VP8X` (extended) file declares flags and then carries one of
/// those two, possibly behind `ICCP`/`ANIM`/`ALPH` chunks, so the whole chunk
/// list is walked rather than only the first fourcc being read.
///
/// Returns false for anything that is not a well-formed WebP, so a malformed
/// or truncated buffer is left alone rather than re-encoded on a guess.
pub fn is_lossless_webp(input: &[u8]) -> bool {
    if input.len() < 16 {
        return false;
    }
    if &input[0..4] != b"RIFF" {
        return false;
    }
    if &input[8..12] != b"WEBP" {
        return false;
    }

    // Walk the chunk list. Each chunk is: fourcc (4) + u32 payload size +
    // payload, padded to an even length.
    let mut offset: usize = 12;
    while offset + 8 <= input.len() {
        let fourcc = &input[offset..offset + 4];
        if fourcc == b"VP8L" {
            return true;
        }
        if fourcc == b"VP8 " {
            return false;
        }
        let size = u32::from_le_bytes([
            input[offset + 4],
            input[offset + 5],
            input[offset + 6],
            input[offset + 7],
        ]) as usize;
        // A size that overflows the buffer means a malformed file — stop walking.
        // (v4 compares against `input.length`, not the remaining bytes; kept.)
        //
        // ⚠ This arm has NO observable effect on the verdict, measured rather
        // than assumed: `size > input.len()` implies `offset + 8 + size >
        // input.len()`, so the very next loop condition fails and the walk
        // returns `false` anyway. The named mutation proof for it SURVIVED, and
        // the reason is a proof rather than a corpus hole — no input can exist
        // where an over-length chunk is followed by a reachable `VP8L`. It is
        // kept because it is v4's line and because it ends the walk in O(1)
        // instead of one more iteration. The corpus still carries `size_past_end`
        // and `size_max_u32`, which pin the VERDICT on both sides.
        if size > input.len() {
            return false;
        }
        // `8 + size + (size % 2)` — the header, the payload, and RIFF's odd-size
        // pad byte. Saturating so a `size` near `usize::MAX` cannot wrap the
        // cursor back into the buffer (JS's doubles cannot overflow here; Rust's
        // usize can, and the guard above only bounds `size` by the LENGTH).
        offset = offset
            .saturating_add(8)
            .saturating_add(size)
            .saturating_add(size % 2);
    }
    false
}

/// v4 `TranscodeResult`, plus one field v4 does not need.
pub struct TranscodeResult {
    pub data: Vec<u8>,
    pub stored_mime_type: String,
    pub size_bytes: i64,
    pub sha256: String,
    /// Whether the bytes actually MOVED. v4's callers ask this by REFERENCE
    /// identity (`transcoded.data === input.data`), which sharp's passthrough
    /// preserves and a Rust copy cannot; this flag carries the same answer
    /// explicitly. `false` on every passthrough and on the encode-failure
    /// fallback.
    pub transcoded: bool,
}

/// The bitmap→WebP encode seam (v4 `sharp(input).webp({quality, effort})`).
/// `Err` = could not encode (v4's catch) → the caller stores the original.
pub trait WebpTranscoder: Send + Sync {
    fn encode_webp(&self, bytes: &[u8], quality: u8) -> Result<Vec<u8>, String>;
}

/// The default seam: refuses loudly and lets the caller take v4's
/// store-original fallback arm — never a silent behavioral fork.
pub struct RefusingWebpTranscoder;

impl WebpTranscoder for RefusingWebpTranscoder {
    fn encode_webp(&self, _bytes: &[u8], _quality: u8) -> Result<Vec<u8>, String> {
        Err(
            "WebP transcoding is not available in this build (the production codec is \
             deferred by work order P4.6y); storing the original bytes — v4's own \
             fallback arm for an encode failure"
                .to_string(),
        )
    }
}

fn passthrough(input: &[u8], mime: &str) -> TranscodeResult {
    TranscodeResult {
        data: input.to_vec(),
        stored_mime_type: mime.to_string(),
        size_bytes: input.len() as i64,
        sha256: hex::encode(Sha256::digest(input)),
        transcoded: false,
    }
}

/// v4 `transcodeToWebP(input, originalMimeType)` (default quality 85).
pub fn transcode_to_webp(
    input: &[u8],
    original_mime_type: &str,
    transcoder: &dyn WebpTranscoder,
) -> TranscodeResult {
    // v4 `originalMimeType.trim().toLowerCase().split(';')[0]` — the parameters
    // and the casing come off BEFORE the set lookup, so `Image/PNG; charset=…`
    // is a PNG. (v5 used to lowercase only; a mime with parameters fell through
    // to passthrough.)
    let mime = original_mime_type
        .trim()
        .to_lowercase()
        .split(';')
        .next()
        .unwrap_or("")
        .to_string();

    // A large lossless WebP is re-encoded even though image/webp is not in the
    // transcodable set — see the module doc. A lossy WebP falls through to the
    // passthrough below and is never re-encoded.
    let reencode_lossless = mime == "image/webp"
        && input.len() >= LOSSLESS_WEBP_REENCODE_MIN_BYTES
        && is_lossless_webp(input);

    if !reencode_lossless && !TRANSCODABLE_MIME_TYPES.contains(&mime.as_str()) {
        return passthrough(input, original_mime_type);
    }
    match transcoder.encode_webp(input, 85) {
        Ok(webp) => {
            tracing::debug!(
                target: "quilltap::mount_index",
                original_mime_type = %mime,
                input_bytes = input.len(),
                output_bytes = webp.len(),
                quality = 85,
                reason = if reencode_lossless {
                    "lossless-webp-reencode"
                } else {
                    "bitmap-transcode"
                },
                "Transcoded blob to WebP",
            );
            TranscodeResult {
                sha256: hex::encode(Sha256::digest(&webp)),
                size_bytes: webp.len() as i64,
                stored_mime_type: "image/webp".to_string(),
                data: webp,
                transcoded: true,
            }
        }
        Err(msg) => {
            tracing::warn!(
                target: "quilltap::mount_index",
                original_mime_type = %mime,
                error = %msg,
                "Failed to transcode blob to WebP; storing original bytes",
            );
            passthrough(input, original_mime_type)
        }
    }
}

/// v4 `normaliseBlobRelativePath` — rewrite the extension to `.webp` when the
/// stored MIME became `image/webp`, so persisted Markdown references resolve.
pub fn normalise_blob_relative_path(relative_path: &str, stored_mime_type: &str) -> String {
    if stored_mime_type != "image/webp" {
        return relative_path.to_string();
    }
    if relative_path.to_lowercase().ends_with(".webp") {
        return relative_path.to_string();
    }
    let last_dot = relative_path.rfind('.');
    let last_slash = relative_path.rfind('/').map(|i| i as isize).unwrap_or(-1);
    match last_dot {
        Some(d) if (d as isize) > last_slash => {
            format!("{}.webp", &relative_path[..d])
        }
        _ => format!("{relative_path}.webp"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_blob_relative_path_rewrites_extension() {
        assert_eq!(
            normalise_blob_relative_path("images/portrait.png", "image/webp"),
            "images/portrait.webp"
        );
        assert_eq!(
            normalise_blob_relative_path("images/portrait.webp", "image/webp"),
            "images/portrait.webp"
        );
        assert_eq!(
            normalise_blob_relative_path("images/noext", "image/webp"),
            "images/noext.webp"
        );
        assert_eq!(
            normalise_blob_relative_path("a.b/noext", "image/webp"),
            "a.b/noext.webp"
        );
        assert_eq!(
            normalise_blob_relative_path("doc.pdf", "application/pdf"),
            "doc.pdf"
        );
    }
}
