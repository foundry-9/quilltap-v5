//! Port of v4 `lib/mount-index/blob-transcode.ts` — the Scriptorium blob
//! upload's WebP normalisation. Already-WebP uploads store as-is; non-image
//! MIME types pass through untouched; decodable bitmaps transcode to WebP via
//! the [`WebpTranscoder`] seam. v4's `transcodeToWebP` falls back to the
//! ORIGINAL bytes on any encode failure — the refusing default seam takes that
//! same fallback arm loudly (the production codec is deferred machinery of
//! P4.6v/P4.6y tier 3: a real encoder can never be byte-identical to sharp's
//! WebP anyway, so the fallback arm is the differential-pinnable one).

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

/// v4 `TranscodeResult`.
pub struct TranscodeResult {
    pub data: Vec<u8>,
    pub stored_mime_type: String,
    pub size_bytes: i64,
    pub sha256: String,
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
    }
}

/// v4 `transcodeToWebP(input, originalMimeType)` (default quality 85).
pub fn transcode_to_webp(
    input: &[u8],
    original_mime_type: &str,
    transcoder: &dyn WebpTranscoder,
) -> TranscodeResult {
    if !TRANSCODABLE_MIME_TYPES.contains(&original_mime_type.to_lowercase().as_str()) {
        return passthrough(input, original_mime_type);
    }
    match transcoder.encode_webp(input, 85) {
        Ok(webp) => TranscodeResult {
            sha256: hex::encode(Sha256::digest(&webp)),
            size_bytes: webp.len() as i64,
            stored_mime_type: "image/webp".to_string(),
            data: webp,
        },
        Err(msg) => {
            eprintln!("Failed to transcode blob to WebP; storing original bytes: {msg}");
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
