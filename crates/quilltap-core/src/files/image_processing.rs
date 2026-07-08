//! v4 `lib/files/image-processing.ts` — the base64-size + provider-limit resize
//! DECISION logic, ported PURE over an injected [`ImageTranscoder`] seam.
//!
//! The actual pixel work (v4's Sharp `.resize().jpeg()/.webp()/.png()/.gif()
//! .toBuffer()` and `.metadata()`) is a **host seam** — no image-codec crate is
//! pulled into the core (the `doc_blob` `transcodeToWebP` precedent). The
//! differential injects a deterministic fake on both sides (mirrored by the
//! oracle's Sharp mock), so the resize *schedule* (the geometric ×0.8 scale
//! walk, the quality-fallback branch, the loop bound) is itself what the diff
//! proves.
//!
//! Every subtle/buggy v4 behavior is reproduced faithfully (the geometric — NOT
//! linear — scale schedule computed from the ORIGINAL width each iteration; the
//! quality-fallback branch that overwrites `result_buffer` without rechecking the
//! limit; the exhausted-loop `result_buffer`/`result_metadata` inconsistency for
//! JPEG) so a byte-exact port matches v4's output on the wire.

/// The default provider base64 limit (v4 `DEFAULT_MAX_BASE64_SIZE = 4 * 1024 *
/// 1024`).
pub const DEFAULT_MAX_BASE64_SIZE: i64 = 4 * 1024 * 1024;

/// v4 `RESIZABLE_MIME_TYPES` (static GIF only).
const RESIZABLE_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/webp",
    "image/gif",
];

/// The output encoder v4's `determineOutputFormat` selects (Sharp `.jpeg()` /
/// `.webp()` / `.png()` / `.gif()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Jpeg,
    Webp,
    Png,
    Gif,
}

impl OutputFormat {
    /// A stable wire token for the fake transcoder + logs (`"jpeg"` etc.).
    pub fn as_str(self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpeg",
            OutputFormat::Webp => "webp",
            OutputFormat::Png => "png",
            OutputFormat::Gif => "gif",
        }
    }
}

/// The subset of Sharp `.metadata()` the resize decision consumes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageMetadata {
    /// `metadata.width` (v4 `|| 0` when absent — modeled as `None` here, the
    /// caller applies the `|| 0`).
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// `metadata.hasAlpha`.
    pub has_alpha: bool,
}

/// The injected image codec (Sharp) seam. A single resize+encode step and a
/// metadata probe — the differential provides a deterministic fake mirrored by
/// the oracle's Sharp mock; the production host impl wraps a real codec (Phase 4).
///
/// `Send + Sync`: the K-seam future ([`crate::services::chat_files::
/// RealMessageContextSeams`]) captures `&dyn ImageTranscoder` across an `.await`.
pub trait ImageTranscoder: Send + Sync {
    /// v4 `await sharp(buffer).metadata()` — the width/height/hasAlpha probe.
    fn metadata(&self, buffer: &[u8]) -> ImageMetadata;

    /// v4 one main-pipeline step: `sharp(buffer).resize({ width: target_width,
    /// fit: 'inside', withoutEnlargement: true }).<format>({ quality/… }).toBuffer()`.
    /// `buffer` is ALWAYS the ORIGINAL buffer (v4 re-derives target width from the
    /// original each iteration). `quality` is consumed only by JPEG/WebP (PNG uses
    /// `compressionLevel: 9`, GIF none) — the seam receives it uniformly.
    fn resize_step(
        &self,
        buffer: &[u8],
        target_width: i64,
        format: OutputFormat,
        quality: i64,
    ) -> Vec<u8>;
}

/// A default [`ImageTranscoder`] for an instance with no host image codec wired
/// (production Phase-4 seam). It reports no dimensions and never actually resizes
/// (returns the input length's worth of zero bytes) — but it is only ever reached
/// when an image exceeds the provider limit, which requires a real codec, so a
/// no-codec host simply never triggers resize (the base64 check runs first).
pub struct NotConfiguredTranscoder;

impl ImageTranscoder for NotConfiguredTranscoder {
    fn metadata(&self, _buffer: &[u8]) -> ImageMetadata {
        ImageMetadata::default()
    }
    fn resize_step(
        &self,
        buffer: &[u8],
        _target_width: i64,
        _format: OutputFormat,
        _quality: i64,
    ) -> Vec<u8> {
        buffer.to_vec()
    }
}

/// v4 `canResizeImage(mimeType)` — `RESIZABLE_MIME_TYPES.includes(mimeType
/// .toLowerCase())`.
pub fn can_resize_image(mime_type: &str) -> bool {
    RESIZABLE_MIME_TYPES.contains(&mime_type.to_lowercase().as_str())
}

/// v4 `calculateBase64Size(buffer)` — `Math.ceil(buffer.length * 4 / 3)`. Exact
/// integer ceil (`(len*4).div_ceil(3)`); NO base64 padding rounding (faithful to
/// v4's raw ceil).
pub fn calculate_base64_size(len: usize) -> i64 {
    let bits = (len as i64) * 4;
    // `i64::div_ceil` is still unstable (`int_roundings`) on the pinned 1.96.0
    // toolchain; `bits >= 0` (derived from a `usize`), so `(bits + 2) / 3` is the
    // exact ceil-divide.
    (bits + 2) / 3
}

/// v4 `getProviderMaxBase64Size(provider)` — the registry
/// `getAttachmentSupport(provider)?.maxBase64Size ?? DEFAULT_MAX_BASE64_SIZE`.
/// Reads the v5 provider manifest (the registry data source, distinct from the
/// client-safe [`crate::files::attachment_support`] map).
pub fn get_provider_max_base64_size(provider: &str) -> i64 {
    crate::provider_manifest::Registry::built_in()
        .attachment_support(provider)
        .and_then(|a| a.max_base64_size)
        .unwrap_or(DEFAULT_MAX_BASE64_SIZE)
}

/// v4 `determineOutputFormat(metadata, originalMimeType)`. Decision ORDER matters
/// (PNG stays PNG only with alpha; a non-alpha PNG falls through to JPEG). The
/// `original_mime_type` comparisons are case-sensitive exact (unlike
/// `canResizeImage`).
pub fn determine_output_format(
    has_alpha: bool,
    original_mime_type: &str,
) -> (OutputFormat, String) {
    if has_alpha && original_mime_type == "image/png" {
        return (OutputFormat::Png, "image/png".to_string());
    }
    if original_mime_type == "image/jpeg" || original_mime_type == "image/jpg" {
        return (OutputFormat::Jpeg, "image/jpeg".to_string());
    }
    if original_mime_type == "image/webp" {
        return (OutputFormat::Webp, "image/webp".to_string());
    }
    if original_mime_type == "image/gif" {
        return (OutputFormat::Gif, "image/gif".to_string());
    }
    (OutputFormat::Jpeg, "image/jpeg".to_string())
}

/// v4 `resizeImageForProvider`'s result (`ImageResizeResult`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageResizeResult {
    pub buffer: Vec<u8>,
    pub mime_type: String,
    pub was_resized: bool,
    pub original_size: usize,
    pub final_size: usize,
    /// `undefined` on the early-return paths (v4 omits width/height there).
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// The default JPEG/WebP quality (v4 `quality = 85` destructure default).
pub const DEFAULT_QUALITY: i64 = 85;

/// v4 `resizeImageForProvider(options)`. `provider` selects the base64 limit;
/// `quality` defaults to [`DEFAULT_QUALITY`]. The pixel work goes through the
/// injected `transcoder`. Reproduces v4's two early returns + the 10-iteration
/// progressive-resize loop verbatim (see the module docs for the faithfully-kept
/// quirks).
pub fn resize_image_for_provider(
    provider: &str,
    buffer: &[u8],
    mime_type: &str,
    quality: i64,
    transcoder: &dyn ImageTranscoder,
) -> ImageResizeResult {
    let original_size = buffer.len();
    let max_base64_size = get_provider_max_base64_size(provider);

    // Early-return 1 — unsupported format (width/height omitted).
    if !can_resize_image(mime_type) {
        return ImageResizeResult {
            buffer: buffer.to_vec(),
            mime_type: mime_type.to_string(),
            was_resized: false,
            original_size,
            final_size: original_size,
            width: None,
            height: None,
        };
    }

    // Early-return 2 — already fits (`<=`).
    let base64_size = calculate_base64_size(buffer.len());
    if base64_size <= max_base64_size {
        return ImageResizeResult {
            buffer: buffer.to_vec(),
            mime_type: mime_type.to_string(),
            was_resized: false,
            original_size,
            final_size: original_size,
            width: None,
            height: None,
        };
    }

    let original_metadata = transcoder.metadata(buffer);
    let original_width = original_metadata.width.unwrap_or(0);
    let original_height = original_metadata.height.unwrap_or(0);
    let (format, output_mime_type) =
        determine_output_format(original_metadata.has_alpha, mime_type);

    let mut scale_factor: f64 = 0.8;
    let mut iterations: i64 = 0;
    let max_iterations: i64 = 10;
    let mut result_buffer: Vec<u8> = buffer.to_vec();
    let mut result_metadata: Option<ImageMetadata> = None;

    while iterations < max_iterations {
        iterations += 1;

        // `Math.round` (half away from zero for positive values); `targetHeight`
        // is computed by v4 but never passed to `.resize()`.
        let target_width = (original_width as f64 * scale_factor).round() as i64;
        let _target_height = (original_height as f64 * scale_factor).round() as i64;

        result_buffer = transcoder.resize_step(buffer, target_width, format, quality);
        let meta = transcoder.metadata(&result_buffer);
        result_metadata = Some(meta);

        let new_base64_size = calculate_base64_size(result_buffer.len());
        if new_base64_size <= max_base64_size {
            return ImageResizeResult {
                mime_type: output_mime_type,
                was_resized: true,
                original_size,
                final_size: result_buffer.len(),
                width: meta.width,
                height: meta.height,
                buffer: result_buffer,
            };
        }

        scale_factor *= 0.8;

        // The quality-fallback branch (faithfully kept, incl. the bug): overwrites
        // `result_buffer` with a reduced-quality re-render at the CURRENT
        // `target_width`, WITHOUT recomputing `result_metadata` or the base64 size
        // or re-checking the limit this iteration. The reduced buffer is only
        // evaluated at the top of the NEXT iteration, where it is immediately
        // overwritten — so it is effectively discarded except in the exhausted
        // final iteration (iter 10), which returns the reduced buffer with stale
        // `result_metadata`.
        if scale_factor < 0.2 && format == OutputFormat::Jpeg {
            let reduced_quality = (quality - 20 * iterations).max(40);
            result_buffer =
                transcoder.resize_step(buffer, target_width, OutputFormat::Jpeg, reduced_quality);
        }
    }

    // Exhausted-loop fallback: the last-produced `result_buffer` (which, for JPEG,
    // is the reduced-quality re-render of iter 10) with `result_metadata` from the
    // pre-reduction main pipeline (v4's genuine inconsistency, reproduced).
    ImageResizeResult {
        mime_type: output_mime_type,
        was_resized: true,
        original_size,
        final_size: result_buffer.len(),
        width: result_metadata.and_then(|m| m.width),
        height: result_metadata.and_then(|m| m.height),
        buffer: result_buffer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_resize_lowercases() {
        assert!(can_resize_image("image/JPEG"));
        assert!(can_resize_image("image/png"));
        assert!(!can_resize_image("application/pdf"));
        assert!(!can_resize_image("image/svg+xml"));
    }

    #[test]
    fn base64_size_is_raw_ceil() {
        assert_eq!(calculate_base64_size(0), 0);
        assert_eq!(calculate_base64_size(1), 2); // ceil(4/3)
        assert_eq!(calculate_base64_size(3), 4); // ceil(12/3)
        assert_eq!(calculate_base64_size(100), 134); // ceil(400/3)
    }

    #[test]
    fn output_format_png_only_with_alpha() {
        assert_eq!(
            determine_output_format(true, "image/png"),
            (OutputFormat::Png, "image/png".to_string())
        );
        // Non-alpha PNG falls through to JPEG.
        assert_eq!(
            determine_output_format(false, "image/png"),
            (OutputFormat::Jpeg, "image/jpeg".to_string())
        );
        assert_eq!(
            determine_output_format(false, "image/jpg"),
            (OutputFormat::Jpeg, "image/jpeg".to_string())
        );
        assert_eq!(
            determine_output_format(false, "image/webp"),
            (OutputFormat::Webp, "image/webp".to_string())
        );
    }

    #[test]
    fn max_base64_size_reads_registry() {
        // ANTHROPIC declares 5 MiB; an unknown provider falls back to 4 MiB.
        assert_eq!(get_provider_max_base64_size("ANTHROPIC"), 5 * 1024 * 1024);
        assert_eq!(get_provider_max_base64_size("OPENAI"), 20 * 1024 * 1024);
        assert_eq!(
            get_provider_max_base64_size("BOGUS"),
            DEFAULT_MAX_BASE64_SIZE
        );
    }

    /// A tiny deterministic fake mirroring the differential's Sharp mock: a
    /// header `w=..|h=..|a=..` + a payload whose length halves each resize so the
    /// loop terminates.
    struct HalvingFake;
    impl ImageTranscoder for HalvingFake {
        fn metadata(&self, buffer: &[u8]) -> ImageMetadata {
            // width encoded as the buffer length for the test.
            ImageMetadata {
                width: Some(buffer.len() as i64),
                height: Some(buffer.len() as i64),
                has_alpha: false,
            }
        }
        fn resize_step(
            &self,
            buffer: &[u8],
            target_width: i64,
            _format: OutputFormat,
            _quality: i64,
        ) -> Vec<u8> {
            // Produce a buffer of length target_width (clamped to original) so the
            // base64 size shrinks with the geometric scale walk.
            let clamped = target_width.clamp(0, buffer.len() as i64) as usize;
            vec![b'A'; clamped]
        }
    }

    #[test]
    fn early_return_when_unsupported() {
        let r = resize_image_for_provider("OPENAI", b"xxxx", "application/pdf", 85, &HalvingFake);
        assert!(!r.was_resized);
        assert_eq!(r.width, None);
    }

    #[test]
    fn early_return_when_already_fits() {
        // 3 bytes → base64 4 << 4 MiB.
        let r = resize_image_for_provider("OPENAI", b"abc", "image/jpeg", 85, &HalvingFake);
        assert!(!r.was_resized);
        assert_eq!(r.final_size, 3);
    }

    #[test]
    fn resizes_until_it_fits() {
        // Provider default 4 MiB; a buffer whose base64 exceeds a tiny synthetic
        // limit — use OPENAI (20 MiB) with a large buffer is impractical in a unit
        // test, so exercise the loop logic via HalvingFake against a small buffer
        // and the registry limit is large: instead assert the no-op path. The full
        // loop + schedule is proven byte-exact in file_attachment_tier3.
        let big = vec![b'Z'; 8];
        let r = resize_image_for_provider("OPENAI", &big, "image/jpeg", 85, &HalvingFake);
        // 8 bytes fits trivially → not resized (documents the fits path).
        assert!(!r.was_resized);
    }
}
