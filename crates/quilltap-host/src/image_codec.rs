//! The host image codec (P4.1b) — the production implementation of the core's
//! pixel seams over the `image` (+ `webp` for lossy WebP encoding) crates.
//!
//! One codec type implements ALL of:
//!
//! * [`quilltap_core::services::file_storage::PixelCodec`] — the low-level
//!   WebP encode + dimensions probe the core WebP POLICIES
//!   (`convert_to_webp` / `transcode_to_webp`) drive;
//! * [`quilltap_core::files::image_processing::ImageTranscoder`] — the
//!   metadata probe + one resize-and-encode step the ported
//!   `resizeImageForProvider` decision loop drives (sharp inventory row 4),
//!   PLUS the fit-inside WebP shrink the ported `shrinkImageForLlmTransport`
//!   ladder drives (P4.D198, v4 `bcd7e4852`, bug 151 — and the ONE seam method
//!   here that is fallible, deliberately: see its doc);
//! * [`quilltap_core::model::image::ImageTranscoder`] — the whole
//!   `convertToWebP` (sharp inventory row 3), composed from the core policy
//!   over this codec's own pixel seam;
//!
//! plus the thumbnail op (sharp inventory row 2: `resize(size, size,
//! {fit:'cover', position:'center'}).webp({quality: 80})`).
//!
//! **D19:** operations are ported, encoded BYTE parity with sharp is NOT
//! required — dimension/format/alpha POLICY parity is (asserted in the tests
//! below). Documented degradations:
//!
//! * **Animated input on the two ANIMATED encode seams is DECLINED** (P4.108,
//!   ruled by the human 2026-09-23). The `webp` crate encodes single frames
//!   only (no libwebpmux), and v4 has exactly ONE sharp call that keeps frames
//!   — `transcodeToWebP`'s `sharp(input, { animated: true })`
//!   (`lib/mount-index/blob-transcode.ts:121`). Its two v5 twins —
//!   [`PixelCodec::encode_webp`] with `animated: true` and
//!   [`BlobWebpTranscoder::encode_webp`] — answer the cannot-transcode `Err`
//!   when `is_multi_frame` says the input has two or more frames, so the
//!   caller takes v4's own store-original fallback and EVERY FRAME is kept (a
//!   recorded D19 divergence on mime, path, name and sha — v4 writes an
//!   animated WebP — never a lost frame). Detection is by FRAME COUNT, not
//!   the WebP animation bit: a one-`ANMF` WebP is a one-page still to sharp
//!   too. APNG is never declined — sharp reads it as a still PNG, so v4 keeps
//!   only its first frame as well. Every OTHER codec method here decodes the
//!   FIRST FRAME of an animated input, which is PARITY: every other v4 sharp
//!   call is `sharp(buffer)` (default `pages: 1`) — `convertToWebP`, the
//!   thumbnail, the LLM-transport shrink, the provider resize.
//! * **AVIF / HEIC / HEIF decode** are not wired (they need dav1d / libheif C
//!   stacks): those inputs fail decode and take v4's own failure-passthrough
//!   branch (stored as-is with the original mime — v4 with a working sharp
//!   would have transcoded; flagged in the module docs of the core policy).
//! * `resize_step` is infallible by trait shape; a decode/encode failure
//!   returns the ORIGINAL bytes (no resize), where v4's sharp would throw and
//!   the caller would skip the file — a documented divergence reachable only
//!   when a decodable-claimed image fails to decode.
//!   `shrink_to_webp` is the exception and returns `Err` — the budget module
//!   reads that `Result` to tell v4's `catch` arm apart from a no-op encode,
//!   so handing back the input there would collapse two different facts.

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder};
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, GenericImageView, ImageFormat, ImageReader};

use quilltap_core::files::image_processing::{
    ImageMetadata, ImageTranscoder as ResizeTranscoder, OutputFormat,
};
use quilltap_core::model::image::{
    ImageTranscoder as WebpTranscoder, TranscodeInput, TranscodeOutput,
};
use quilltap_core::services::file_storage::{convert_to_webp, PixelCodec, THUMBNAIL_QUALITY};
use quilltap_core::services::mount_index::blob_transcode::WebpTranscoder as BlobWebpTranscoder;

/// The production image codec. Stateless; share via `Arc`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostImageCodec;

fn decode(bytes: &[u8]) -> Result<DynamicImage, String> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("image format probe failed: {e}"))?
        .decode()
        .map_err(|e| format!("image decode failed: {e}"))
}

/// Does this input carry TWO OR MORE frames? (P4.108 — the animated-input
/// decline; see the module docs.)
///
/// GIF and WebP only, counted through the `image` crate's own animation
/// decoders (a byte scan for image-descriptor `0x2C` bytes would miscount a
/// GIF whose comment or palette carries that byte). Everything else — APNG
/// included, which sharp reads as a still PNG — is `false`, and so is any
/// decoder error: an input this cannot read falls through to [`decode`],
/// which fails or succeeds exactly as it always did. `take(2)` stops the
/// count at the answer, so a long animation is never decoded whole.
fn is_multi_frame(bytes: &[u8]) -> bool {
    fn at_least_two<'a>(frames: image::Frames<'a>) -> bool {
        frames.take(2).take_while(|f| f.is_ok()).count() >= 2
    }
    match image::guess_format(bytes) {
        Ok(ImageFormat::Gif) => GifDecoder::new(Cursor::new(bytes))
            .map(|d| at_least_two(d.into_frames()))
            .unwrap_or(false),
        // A still WebP reports zero animation frames, so its iterator is
        // empty; the animation bit with ONE `ANMF` counts one.
        Ok(ImageFormat::WebP) => WebPDecoder::new(Cursor::new(bytes))
            .map(|d| at_least_two(d.into_frames()))
            .unwrap_or(false),
        _ => false,
    }
}

/// The cannot-transcode answer for an animated input (P4.108). The caller's
/// `Err` arm is v4's store-original fallback, which keeps every frame.
const ANIMATED_DECLINED: &str = "animated input: the host WebP encoder is single-frame \
     (ruled 2026-09-23 — decline rather than keep only the first frame); \
     storing the original bytes";

/// Encode a decoded image as lossy WebP at `quality` (0–100) via libwebp.
fn encode_webp_image(img: &DynamicImage, quality: i64) -> Result<Vec<u8>, String> {
    // libwebp accepts RGB8/RGBA8; normalize the color type first.
    let rgba = DynamicImage::ImageRgba8(img.to_rgba8());
    let encoder =
        webp::Encoder::from_image(&rgba).map_err(|e| format!("webp encoder init failed: {e}"))?;
    let quality = quality.clamp(0, 100) as f32;
    Ok(encoder.encode(quality).to_vec())
}

impl PixelCodec for HostImageCodec {
    fn encode_webp(
        &self,
        bytes: &[u8],
        quality: i64,
        _effort: Option<i64>,
        animated: bool,
    ) -> Result<Vec<u8>, String> {
        // `effort` (libwebp method) is not exposed by the `webp` crate's
        // simple encoder — an encoding-speed/size knob, not a policy one.
        // `animated: true` is v4's `sharp(input, { animated: true })`: a
        // multi-frame input is DECLINED (module docs). `animated: false` is
        // `sharp(buffer)` — the first frame, as v4.
        if animated && is_multi_frame(bytes) {
            return Err(ANIMATED_DECLINED.to_string());
        }
        let img = decode(bytes)?;
        encode_webp_image(&img, quality)
    }

    fn measure(&self, bytes: &[u8]) -> (Option<i64>, Option<i64>) {
        // Dimensions-only probe (no full decode) — v4's sharp `.metadata()`.
        match ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
        {
            Some((w, h)) => (Some(w as i64), Some(h as i64)),
            None => (None, None),
        }
    }
}

impl ResizeTranscoder for HostImageCodec {
    fn metadata(&self, buffer: &[u8]) -> ImageMetadata {
        // The resize loop needs hasAlpha too → a full decode. Failure → the
        // default (no dims, no alpha), matching sharp's catch-→-{} probes.
        match decode(buffer) {
            Ok(img) => {
                let (w, h) = img.dimensions();
                ImageMetadata {
                    width: Some(w as i64),
                    height: Some(h as i64),
                    has_alpha: img.color().has_alpha(),
                }
            }
            Err(_) => ImageMetadata::default(),
        }
    }

    fn resize_step(
        &self,
        buffer: &[u8],
        target_width: i64,
        format: OutputFormat,
        quality: i64,
    ) -> Vec<u8> {
        let Ok(img) = decode(buffer) else {
            return buffer.to_vec(); // documented infallible-trait degrade
        };
        let (w, _h) = img.dimensions();
        // sharp `.resize({width, fit:'inside', withoutEnlargement:true})`:
        // scale down to the target width preserving aspect; never enlarge.
        let resized = if target_width > 0 && (target_width as u32) < w {
            img.resize(target_width as u32, u32::MAX, FilterType::Lanczos3)
        } else {
            img
        };
        encode_output(&resized, format, quality).unwrap_or_else(|_| buffer.to_vec())
    }

    /// v4's LLM-transport shrink step (bug 151, `bcd7e4852`):
    /// `sharp(buffer).resize({ width: max_edge, height: max_edge, fit:
    /// 'inside', withoutEnlargement: true }).webp({ quality }).toBuffer()`.
    ///
    /// `fit: 'inside'` scales the image down until BOTH edges fit the box
    /// (unlike [`Self::resize_step`]'s width-only `resize({width})`, which is
    /// what v4's `resizeImageForProvider` asks for), and `withoutEnlargement`
    /// means an image already inside the box is re-encoded at its own size.
    /// `image::DynamicImage::resize` is exactly that contract — it fits the
    /// aspect ratio inside `(nw, nh)` — so the whole op is one call plus the
    /// already-small guard.
    ///
    /// **Lanczos3** is the filter: sharp's own default kernel is `lanczos3`
    /// (`sharp.kernel.lanczos3`), and every other resize in this codec already
    /// uses it. D19 stands — the ported OPERATION is fit-inside downscale +
    /// lossy WebP, and encoded byte parity with sharp is not required (nor
    /// achievable: libwebp's rate control and sharp's premultiply differ). What
    /// the tests below assert is the POLICY: output dimensions, format, the
    /// never-enlarge rule and the byte budget the ladder is walking toward.
    ///
    /// Unlike [`Self::resize_step`], a decode or encode failure is an `Err`,
    /// NEVER the input bytes: the budget module reads this `Result` to tell
    /// v4's `catch` arm (warn, send stored bytes) apart from a successful
    /// encode that did not help (silence, send stored bytes). Handing back the
    /// input here would collapse the two into one and re-create the
    /// `try_downsize` anti-pattern the trait's doc names.
    fn shrink_to_webp(
        &self,
        buffer: &[u8],
        max_edge: i64,
        quality: i64,
    ) -> Result<Vec<u8>, String> {
        let img = decode(buffer)?;
        let (w, h) = img.dimensions();
        let edge = max_edge.max(0) as u32;
        // `withoutEnlargement: true` + `fit: 'inside'`: an image already inside
        // the box keeps its size (and is still re-encoded — v4 re-encodes it
        // too; the budget module's early return is what skips the pointless
        // work, not this seam).
        let fitted = if edge > 0 && (w > edge || h > edge) {
            img.resize(edge, edge, FilterType::Lanczos3)
        } else {
            img
        };
        encode_webp_image(&fitted, quality)
    }
}

/// Encode per v4 `determineOutputFormat`'s selected sharp pipeline:
/// `.jpeg({quality, mozjpeg:true})` / `.webp({quality})` /
/// `.png({compressionLevel: 9})` / `.gif()`.
fn encode_output(
    img: &DynamicImage,
    format: OutputFormat,
    quality: i64,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    match format {
        OutputFormat::Jpeg => {
            // JPEG has no alpha channel; flatten to RGB8 (mozjpeg parity not
            // required — D19).
            let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
            let encoder =
                JpegEncoder::new_with_quality(Cursor::new(&mut out), quality.clamp(1, 100) as u8);
            rgb.write_with_encoder(encoder)
                .map_err(|e| format!("jpeg encode failed: {e}"))?;
        }
        OutputFormat::Webp => {
            out = encode_webp_image(img, quality)?;
        }
        OutputFormat::Png => {
            let encoder = PngEncoder::new_with_quality(
                Cursor::new(&mut out),
                CompressionType::Best, // sharp compressionLevel: 9
                PngFilterType::Adaptive,
            );
            img.write_with_encoder(encoder)
                .map_err(|e| format!("png encode failed: {e}"))?;
        }
        OutputFormat::Gif => {
            img.write_to(&mut Cursor::new(&mut out), ImageFormat::Gif)
                .map_err(|e| format!("gif encode failed: {e}"))?;
        }
    }
    Ok(out)
}

impl WebpTranscoder for HostImageCodec {
    /// v4 `convertToWebP` (quality 90) — the core POLICY over this codec's
    /// pixel seam (skip SVG/WebP, rewrite mime + `.webp` extension, measure the
    /// output, keep the original on failure).
    fn transcode(&self, input: &TranscodeInput) -> TranscodeOutput {
        let r = convert_to_webp(self, &input.bytes, &input.mime_type, &input.filename);
        TranscodeOutput {
            bytes: r.buffer,
            mime_type: r.mime_type,
            filename: r.filename,
            width: r.width.map(|w| w as f64),
            height: r.height.map(|h| h as f64),
        }
    }
}

impl BlobWebpTranscoder for HostImageCodec {
    /// The Scriptorium blob-upload pixel seam (v4 `transcodeToWebP`'s
    /// `sharp(input, { animated: true }).webp({quality, effort:4})`,
    /// `blob-transcode.ts:121`). Decode then lossy-encode at `quality`; an
    /// undecodable input is an `Err` the caller turns into v4's store-original
    /// fallback arm, and so — ALWAYS, since this seam is v4's animated call —
    /// is a multi-frame input (P4.108, the module docs).
    ///
    /// v4's `effort: 4` is a libwebp encoder-cost knob with no policy surface
    /// (the `webp` crate's simple encoder does not expose it) — dropped, exactly
    /// as the [`PixelCodec::encode_webp`] `_effort` argument is (D19: policy
    /// parity, never byte parity).
    fn encode_webp(&self, bytes: &[u8], quality: u8) -> Result<Vec<u8>, String> {
        if is_multi_frame(bytes) {
            return Err(ANIMATED_DECLINED.to_string());
        }
        let img = decode(bytes)?;
        encode_webp_image(&img, quality as i64)
    }
}

impl HostImageCodec {
    /// v4 `generateThumbnail`'s sharp pipeline (`thumbnail-utils.ts:110`):
    /// `resize(size, size, {fit:'cover', position:'center'}).webp({quality:80})`.
    /// The caller clamps `size` to `MAX_THUMBNAIL_SIZE` (v4 does the clamp in
    /// `generateThumbnail`, not here).
    pub fn thumbnail_webp(&self, bytes: &[u8], size: i64) -> Result<Vec<u8>, String> {
        let img = decode(bytes)?;
        let side = size.max(1) as u32;
        // `fit: 'cover', position: 'center'` == resize_to_fill (center crop).
        let thumb = img.resize_to_fill(side, side, FilterType::Lanczos3);
        encode_webp_image(&thumb, THUMBNAIL_QUALITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quilltap_core::files::image_processing::resize_image_for_provider;
    use quilltap_core::services::file_storage::{
        convert_to_webp, transcode_to_webp, TRANSCODE_WEBP_QUALITY,
    };

    /// A `w`×`h` PNG with (optionally) a transparent pixel.
    fn png_bytes(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
        if alpha {
            img.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        }
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([10, 120, 240]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
            .unwrap();
        out
    }

    fn gif_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([1, 2, 3, 255]));
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Gif)
            .unwrap();
        out
    }

    fn dims_of(bytes: &[u8]) -> (u32, u32) {
        image::load_from_memory(bytes).unwrap().dimensions()
    }

    /// A `w`x`h` deterministic-NOISE WebP at quality 90 — v4's own bug-151
    /// fixture shape (`sharp({create: {…, noise: {type: 'gaussian', mean: 128,
    /// sigma: 70}}}).webp({quality: 90})`). Noise matters: flat colour
    /// compresses to nothing and cannot reproduce a defect that was entirely
    /// about measured byte sizes. An LCG stands in for sharp's gaussian — D19,
    /// and the only property either fixture needs is incompressibility.
    fn noise_webp(w: u32, h: u32) -> Vec<u8> {
        let mut state: u32 = 0x1234_5678;
        let mut img = image::RgbImage::new(w, h);
        for px in img.pixels_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = state.to_le_bytes();
            *px = image::Rgb([b[0], b[1], b[2]]);
        }
        encode_webp_image(&DynamicImage::ImageRgb8(img), 90).unwrap()
    }

    fn format_of(bytes: &[u8]) -> ImageFormat {
        image::guess_format(bytes).unwrap()
    }

    #[test]
    fn measure_and_metadata_probe() {
        let codec = HostImageCodec;
        let png = png_bytes(20, 12, true);
        assert_eq!(PixelCodec::measure(&codec, &png), (Some(20), Some(12)));
        let meta = codec.metadata(&png);
        assert_eq!(meta.width, Some(20));
        assert_eq!(meta.height, Some(12));
        assert!(meta.has_alpha);
        // Non-image bytes: nothing measured, default metadata.
        assert_eq!(PixelCodec::measure(&codec, b"not an image"), (None, None));
        assert_eq!(codec.metadata(b"nope"), ImageMetadata::default());
    }

    #[test]
    fn convert_to_webp_policy_over_real_codec() {
        let codec = HostImageCodec;
        // PNG → converted to WebP, extension rewritten, OUTPUT dims measured.
        let png = png_bytes(16, 8, false);
        let r = convert_to_webp(&codec, &png, "image/png", "pic.png");
        assert!(r.was_converted);
        assert_eq!(r.mime_type, "image/webp");
        assert_eq!(r.filename, "pic.webp");
        assert_eq!(format_of(&r.buffer), ImageFormat::WebP);
        assert_eq!((r.width, r.height), (Some(16), Some(8)));
        // Already-WebP passes through unconverted but measured.
        let r2 = convert_to_webp(&codec, &r.buffer, "image/webp", "pic.webp");
        assert!(!r2.was_converted);
        assert_eq!((r2.width, r2.height), (Some(16), Some(8)));
        // SVG passes through unmeasured.
        let r3 = convert_to_webp(&codec, b"<svg/>", "image/svg+xml", "pic.svg");
        assert!(!r3.was_converted);
        assert_eq!(r3.width, None);
    }

    #[test]
    fn blob_transcode_policy_over_real_codec() {
        let codec = HostImageCodec;
        // GIF → WebP at quality 85 (first-frame; policy: mime + sha over output).
        let gif = gif_bytes(9, 7);
        let r = transcode_to_webp(&codec, &gif, "image/gif", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r.stored_mime_type, "image/webp");
        assert_eq!(format_of(&r.data), ImageFormat::WebP);
        assert_eq!(dims_of(&r.data), (9, 7));
        // AVIF (decode not wired — real AVIF bytes fail to decode) → the
        // failure-passthrough branch: original bytes + original mime (v4's
        // sharp-catch shape). A minimal `ftyp avif` box, undecodable here.
        let avif: Vec<u8> = [
            &[0, 0, 0, 0x1C][..],
            b"ftypavif",
            &[0, 0, 0, 0],
            b"avifmif1",
            b"not-actually-decodable",
        ]
        .concat();
        let r2 = transcode_to_webp(&codec, &avif, "image/avif", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r2.stored_mime_type, "image/avif");
        assert_eq!(r2.data, avif);
        // Non-image mime → stored as-is.
        let r3 = transcode_to_webp(&codec, b"%PDF", "application/pdf", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r3.stored_mime_type, "application/pdf");
    }

    #[test]
    fn resize_step_shapes() {
        let codec = HostImageCodec;
        let jpeg = jpeg_bytes(100, 50);
        // Downscale to width 40 → 40×20, JPEG output.
        let out = codec.resize_step(&jpeg, 40, OutputFormat::Jpeg, 85);
        assert_eq!(format_of(&out), ImageFormat::Jpeg);
        assert_eq!(dims_of(&out), (40, 20));
        // withoutEnlargement: target ≥ width → no upscale (still re-encoded).
        let out2 = codec.resize_step(&jpeg, 400, OutputFormat::Jpeg, 85);
        assert_eq!(dims_of(&out2), (100, 50));
        // Alpha PNG → PNG output keeps alpha.
        let png = png_bytes(60, 30, true);
        let out3 = codec.resize_step(&png, 30, OutputFormat::Png, 85);
        assert_eq!(format_of(&out3), ImageFormat::Png);
        assert_eq!(dims_of(&out3), (30, 15));
        assert!(image::load_from_memory(&out3).unwrap().color().has_alpha());
        // WebP output.
        let out4 = codec.resize_step(&jpeg, 25, OutputFormat::Webp, 85);
        assert_eq!(format_of(&out4), ImageFormat::WebP);
        assert_eq!(dims_of(&out4), (25, 13)); // round(50 * 25/100) = 13 (12.5 → image rounds)
    }

    #[test]
    fn resize_loop_shrinks_below_provider_cap() {
        // Drive the REAL codec through the ported decision half
        // (`resize_image_for_provider`): a deterministic-noise JPEG whose
        // base64 exceeds the 4 MiB default cap (an unknown provider) must come
        // out resized with its base64 size under the cap; a small image
        // early-returns untouched.
        let codec = HostImageCodec;
        let small = jpeg_bytes(200, 100);
        let r = resize_image_for_provider("OPENAI", &small, "image/jpeg", 85, &codec);
        assert!(!r.was_resized); // tiny file fits the 20 MiB cap
        assert_eq!(r.final_size, small.len());
        let s1 = codec.resize_step(&small, 160, OutputFormat::Jpeg, 85);
        let s2 = codec.resize_step(&small, 80, OutputFormat::Jpeg, 85);
        assert!(dims_of(&s1).0 > dims_of(&s2).0);
        assert!(s2.len() < small.len());

        // Incompressible (LCG noise) 2000×2000 JPEG at q100 → over the
        // 4 MiB default cap for an unknown provider.
        let mut state: u32 = 0x1234_5678;
        let mut noise = image::RgbImage::new(2000, 2000);
        for px in noise.pixels_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let b = state.to_le_bytes();
            *px = image::Rgb([b[0], b[1], b[2]]);
        }
        let mut big = Vec::new();
        let enc = JpegEncoder::new_with_quality(Cursor::new(&mut big), 100);
        DynamicImage::ImageRgb8(noise)
            .write_with_encoder(enc)
            .unwrap();
        let cap = quilltap_core::files::image_processing::DEFAULT_MAX_BASE64_SIZE;
        let base64_size = |len: usize| ((len as i64) * 4 + 2) / 3;
        assert!(base64_size(big.len()) > cap, "fixture must exceed the cap");

        let r = resize_image_for_provider("BOGUS_PROVIDER", &big, "image/jpeg", 85, &codec);
        assert!(r.was_resized);
        assert!(
            base64_size(r.final_size) <= cap,
            "resized base64 {} must be within the {} cap",
            base64_size(r.final_size),
            cap
        );
        assert!(r.width.unwrap() < 2000);
    }

    #[test]
    fn blob_webp_transcoder_seam() {
        use quilltap_core::services::mount_index::blob_transcode::{
            transcode_to_webp as blob_transcode_to_webp, WebpTranscoder as BlobWebpTranscoder,
        };
        let codec = HostImageCodec;
        // A decodable PNG → WebP bytes that decode back with IDENTICAL dims
        // (D19: policy parity — dims, not bytes).
        let png = png_bytes(24, 18, true);
        let webp = BlobWebpTranscoder::encode_webp(&codec, &png, 85).unwrap();
        assert_eq!(format_of(&webp), ImageFormat::WebP);
        assert_eq!(dims_of(&webp), (24, 18));
        // JPEG too, at a different quality.
        let jpeg = jpeg_bytes(30, 20);
        let webp2 = BlobWebpTranscoder::encode_webp(&codec, &jpeg, 90).unwrap();
        assert_eq!(format_of(&webp2), ImageFormat::WebP);
        assert_eq!(dims_of(&webp2), (30, 20));
        // Undecodable input errors — the caller's store-original fallback arm.
        assert!(BlobWebpTranscoder::encode_webp(&codec, b"not an image", 85).is_err());

        // Through the ported blob POLICY: a GIF becomes image/webp; a non-image
        // mime passes through untouched (the transcodable-set gate).
        let gif = gif_bytes(9, 7);
        let r = blob_transcode_to_webp(&gif, "image/gif", &codec);
        assert_eq!(r.stored_mime_type, "image/webp");
        assert_eq!(format_of(&r.data), ImageFormat::WebP);
        assert_eq!(dims_of(&r.data), (9, 7));
        let r2 = blob_transcode_to_webp(b"%PDF", "application/pdf", &codec);
        assert_eq!(r2.stored_mime_type, "application/pdf");
        assert_eq!(r2.data, b"%PDF");
    }

    #[test]
    fn thumbnail_is_square_webp_cover() {
        let codec = HostImageCodec;
        let jpeg = jpeg_bytes(120, 40);
        let thumb = codec.thumbnail_webp(&jpeg, 30).unwrap();
        assert_eq!(format_of(&thumb), ImageFormat::WebP);
        assert_eq!(dims_of(&thumb), (30, 30)); // cover crop, not letterboxed
    }

    /// P4.D198 (v4 `bcd7e4852`, bug 151) — the transport shrink seam's POLICY:
    /// fit INSIDE the box on both edges, never enlarge, WebP out, and an `Err`
    /// (never the input bytes) on failure.
    #[test]
    fn shrink_to_webp_fits_inside_the_box() {
        let codec = HostImageCodec;

        // A 1024x1536 portrait avatar — the exact shape bug 151 reported.
        // `fit: 'inside'` scales until BOTH edges fit: 1536 -> 1024, so
        // 1024 -> 683 (v4's own test asserts 683x1024 from sharp).
        let portrait = noise_webp(1024, 1536);
        let out = ResizeTranscoder::shrink_to_webp(&codec, &portrait, 1024, 78).unwrap();
        assert_eq!(format_of(&out), ImageFormat::WebP);
        assert_eq!(dims_of(&out), (683, 1024), "portrait fit-inside dimensions");

        // The landscape twin — a 1536x1024 story background -> 1024x683.
        let landscape = noise_webp(1536, 1024);
        let out2 = ResizeTranscoder::shrink_to_webp(&codec, &landscape, 1024, 78).unwrap();
        assert_eq!(
            dims_of(&out2),
            (1024, 683),
            "landscape fit-inside dimensions"
        );

        // `withoutEnlargement: true`: an image already inside the box keeps its
        // own size and is still re-encoded as WebP.
        let small = jpeg_bytes(320, 240);
        let out3 = ResizeTranscoder::shrink_to_webp(&codec, &small, 1024, 78).unwrap();
        assert_eq!(dims_of(&out3), (320, 240), "never enlarged");
        assert_eq!(format_of(&out3), ImageFormat::WebP);

        // The ladder's premise: a lower quality really is fewer bytes.
        let q78 = ResizeTranscoder::shrink_to_webp(&codec, &portrait, 1024, 78).unwrap();
        let q45 = ResizeTranscoder::shrink_to_webp(&codec, &portrait, 1024, 45).unwrap();
        assert!(
            q45.len() < q78.len(),
            "quality 45 ({}) must undercut quality 78 ({})",
            q45.len(),
            q78.len()
        );

        // And the byte target the whole feature exists for. MEASURED here
        // (2026-09-17, `image` 0.25 + `webp` 0.3): the stored q90 1024x1536
        // noise avatar is 1,287,400 B = 1,716,534 B of base64, and one turn of
        // this seam takes it to 342,376 B (456,502 B base64) at quality 78 and
        // 242,366 B (323,155 B base64) at 45. v4's sharp reached ~98 KB of
        // base64 on its own gaussian-noise fixture, so an LCG is the less
        // compressible of the two — D19, and the POLICY is what is asserted:
        // the stored avatar starts OVER the 500 KiB transport ceiling and UNDER
        // the 4 MB per-image provider cap (bug 151's exact precondition — which
        // is why nothing resized it), and the ladder clears the ceiling.
        let base64_size = |len: usize| ((len as i64) * 4 + 2) / 3;
        assert!(
            base64_size(portrait.len()) > 500 * 1024,
            "the fixture must start over the transport ceiling: {}",
            base64_size(portrait.len())
        );
        assert!(
            base64_size(portrait.len()) < 4 * 1024 * 1024,
            "…and under the per-image provider cap that left it untouched: {}",
            base64_size(portrait.len())
        );
        assert!(
            base64_size(q45.len()) <= 500 * 1024,
            "the ladder's bottom rung must clear the 500 KiB ceiling: {}",
            base64_size(q45.len())
        );
    }

    /// P4.D198 Tier-1 item 2, the WHOLE transport budget over the REAL codec —
    /// v4's `llm-image-budget.test.ts` shapes ("caps the long edge at 1024 for a
    /// portrait avatar", "caps a landscape story background", "brings the
    /// payload under the 500K base64 ceiling"), driven through
    /// `shrink_image_for_llm_transport` rather than the seam method alone, so
    /// the ladder's break-on-fit and the reported dimensions are measured
    /// against `image` + `webp` and not a script. Landed at the `bcd7e4852`
    /// unification (the §3 review found only the seam-level half here).
    #[test]
    fn the_transport_budget_over_the_real_codec_takes_v4s_shape() {
        use quilltap_core::files::image_processing::calculate_base64_size;
        use quilltap_core::files::llm_image_budget::{
            shrink_image_for_llm_transport, LLM_TRANSPORT_MAX_EDGE, LLM_TRANSPORT_TARGET_BASE64,
        };
        let codec = HostImageCodec;
        let t: &dyn quilltap_core::files::image_processing::ImageTranscoder = &codec;

        let portrait = noise_webp(1024, 1536);
        let r = shrink_image_for_llm_transport(t, &portrait, "image/webp", Some("NANOGPT"), None);
        assert!(r.was_shrunk, "a 1024x1536 avatar is shrunk");
        assert_eq!(r.mime_type, "image/webp");
        assert_eq!(
            r.width.unwrap_or(0).max(r.height.unwrap_or(0)),
            LLM_TRANSPORT_MAX_EDGE,
            "the long edge is capped at 1024"
        );
        assert_eq!(
            (r.width, r.height),
            (Some(683), Some(1024)),
            "aspect preserved"
        );
        assert!(
            calculate_base64_size(r.final_size) <= LLM_TRANSPORT_TARGET_BASE64,
            "the payload clears the 500 KiB ceiling: {}",
            calculate_base64_size(r.final_size)
        );
        assert_eq!(r.original_size, portrait.len());

        let landscape = noise_webp(1536, 1024);
        let r2 = shrink_image_for_llm_transport(t, &landscape, "image/webp", Some("NANOGPT"), None);
        assert!(r2.was_shrunk);
        assert_eq!(
            (r2.width, r2.height),
            (Some(1024), Some(683)),
            "landscape twin"
        );
    }

    /// v4 "keeps two shrunk avatars inside the per-turn budget that bug 151
    /// blew" — the reported turn reproduced over the real codec: two unseen
    /// avatars on one request sum under `LANTERN_IMAGE_BASE64_BUDGET`, and
    /// under the 4.52 MB NanoGPT answered with 413.
    #[test]
    fn two_shrunk_avatars_fit_the_per_turn_budget_bug_151_blew() {
        use quilltap_core::files::image_processing::calculate_base64_size;
        use quilltap_core::files::llm_image_budget::{
            shrink_image_for_llm_transport, LANTERN_IMAGE_BASE64_BUDGET,
        };
        let codec = HostImageCodec;
        let t: &dyn quilltap_core::files::image_processing::ImageTranscoder = &codec;
        let a = shrink_image_for_llm_transport(
            t,
            &noise_webp(1024, 1536),
            "image/webp",
            Some("NANOGPT"),
            None,
        );
        let b = shrink_image_for_llm_transport(
            t,
            &noise_webp(1024, 1536),
            "image/webp",
            Some("NANOGPT"),
            None,
        );
        let on_the_wire = calculate_base64_size(a.final_size) + calculate_base64_size(b.final_size);
        assert!(
            on_the_wire <= LANTERN_IMAGE_BASE64_BUDGET as i64,
            "two shrunk avatars must fit the per-turn budget: {on_the_wire}"
        );
        // 4.52 MB was what NanoGPT answered with 413.
        assert!(on_the_wire < (4.52 * 1024.0 * 1024.0) as i64);
    }

    /// A decode failure is an `Err`, and specifically NOT the input bytes —
    /// the collapse `resize_step`'s infallible shape forces and this seam
    /// forbids (M6 targets exactly this assertion).
    #[test]
    fn shrink_to_webp_errs_rather_than_handing_back_the_input() {
        let codec = HostImageCodec;
        let junk = b"not an image at all";
        let r = ResizeTranscoder::shrink_to_webp(&codec, junk, 1024, 78);
        assert!(
            r.is_err(),
            "undecodable bytes must Err, got {} bytes",
            r.map(|b| b.len()).unwrap_or(0)
        );
        // The default trait body is the not-configured answer; the real codec's
        // failure must name the decode, not that.
        let msg = ResizeTranscoder::shrink_to_webp(&codec, junk, 1024, 78).unwrap_err();
        assert!(
            msg.contains("decode") || msg.contains("probe"),
            "unexpected: {msg}"
        );
    }

    /// P4.108 — the committed animated-input fixtures (and their generator).
    fn anim_fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../harness/oracle/fixtures/normalize-blob-image")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    /// P4.108 Tier-1 item 2 — the helper's classification over the six
    /// fixtures, which sharp 0.35.4 reads as pages 2 / 1 / 1 / 2 / 1 / none.
    #[test]
    fn is_multi_frame_counts_frames_not_flags() {
        for (file, want) in [
            ("anim-2frame.gif", true),
            ("still-large.gif", false),
            // A comment full of 0x2C (the descriptor introducer) is not a frame.
            ("still-with-commas.gif", false),
            ("anim-2frame.webp", true),
            // The VP8X animation bit with ONE `ANMF`: a still to sharp.
            ("one-anmf.webp", false),
            // sharp reads APNG as a still PNG — never declined.
            ("anim-2frame.apng", false),
        ] {
            assert_eq!(is_multi_frame(&anim_fixture(file)), want, "{file}");
        }
        // Stills of every other kind, and bytes that are no image at all.
        assert!(!is_multi_frame(&gif_bytes(9, 7)));
        assert!(!is_multi_frame(&png_bytes(4, 4, false)));
        assert!(!is_multi_frame(&noise_webp(8, 8)));
        assert!(!is_multi_frame(b"GIF89a truncated"));
        assert!(!is_multi_frame(b"not an image"));
    }

    /// P4.108 Tier-1 item 3 — the two ANIMATED seams decline a multi-frame
    /// input with the ruling's message, and encode everything else as before.
    #[test]
    fn the_two_animated_seams_decline_a_multi_frame_input() {
        let codec = HostImageCodec;
        for file in ["anim-2frame.gif", "anim-2frame.webp"] {
            let bytes = anim_fixture(file);
            let e = PixelCodec::encode_webp(&codec, &bytes, 85, Some(4), true).unwrap_err();
            assert!(e.contains("ruled 2026-09-23"), "{file}: {e}");
            let e = BlobWebpTranscoder::encode_webp(&codec, &bytes, 85).unwrap_err();
            assert_eq!(e, ANIMATED_DECLINED, "{file}");
        }
        for file in [
            "still-large.gif",
            "still-with-commas.gif",
            "one-anmf.webp",
            "anim-2frame.apng",
        ] {
            let bytes = anim_fixture(file);
            let a = PixelCodec::encode_webp(&codec, &bytes, 85, Some(4), true).unwrap();
            assert_eq!(format_of(&a), ImageFormat::WebP, "{file}");
            let b = BlobWebpTranscoder::encode_webp(&codec, &bytes, 85).unwrap();
            assert_eq!(format_of(&b), ImageFormat::WebP, "{file}");
        }
    }

    /// P4.108 Tier-1 item 3 — every path where v4 itself encodes only the
    /// FIRST frame (`sharp(buffer)`, default `pages: 1`) is UNCHANGED: an
    /// animated GIF still comes out as its first frame there.
    #[test]
    fn the_first_frame_paths_still_encode_an_animated_gif() {
        let codec = HostImageCodec;
        let anim = anim_fixture("anim-2frame.gif");

        // `PixelCodec::encode_webp` with `animated: false` — v4 `convertToWebP`.
        let direct = PixelCodec::encode_webp(&codec, &anim, 90, None, false).unwrap();
        assert_eq!(format_of(&direct), ImageFormat::WebP);
        let r = convert_to_webp(&codec, &anim, "image/gif", "loop.gif");
        assert!(
            r.was_converted,
            "convertToWebP keeps v4's first-frame encode"
        );
        assert_eq!(r.mime_type, "image/webp");
        assert_eq!(r.filename, "loop.webp");
        assert_eq!((r.width, r.height), (Some(32), Some(24)));

        // The first frame is RED (the generator's frame 1), not the blue second.
        let px = image::load_from_memory(&r.buffer).unwrap().to_rgb8();
        let image::Rgb([red, _g, blue]) = *px.get_pixel(16, 12);
        assert!(red > 150 && blue < 100, "frame 1 is red: {red},{blue}");

        let shrunk = ResizeTranscoder::shrink_to_webp(&codec, &anim, 16, 78).unwrap();
        assert_eq!(dims_of(&shrunk), (16, 12));
        let thumb = codec.thumbnail_webp(&anim, 8).unwrap();
        assert_eq!(dims_of(&thumb), (8, 8));
        let resized = codec.resize_step(&anim, 16, OutputFormat::Webp, 85);
        assert_eq!(format_of(&resized), ImageFormat::WebP);
        assert_eq!(dims_of(&resized), (16, 12));
    }

    /// P4.108 Tier-2 item 7 — the `PixelCodec` animated seam through the
    /// policies that drive it with `animated: true`. The family
    /// (`normalize_blob_image_equivalence`) hands `HostImageCodec` to the
    /// chokepoint as a `BlobWebpTranscoder`, so it exercises THAT seam; this
    /// is the proof for the other one: `PixelCodecWebp` (the normalization
    /// over a site's pixel codec) and `file_storage::transcode_to_webp`
    /// (the gallery / avatar / image-job pre-transcode) both store an
    /// animated GIF's original bytes, and a still GIF is still transcoded.
    #[test]
    fn the_pixel_codec_animated_seam_keeps_every_frame_through_its_policies() {
        use quilltap_core::services::mount_index::normalize_blob_image::{
            normalize_link_blob_image, NormalizableBlob, PixelCodecWebp,
        };
        let codec = HostImageCodec;
        let anim = anim_fixture("anim-2frame.gif");

        let r = transcode_to_webp(&codec, &anim, "image/gif", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r.stored_mime_type, "image/gif");
        assert_eq!(r.data, anim, "every frame kept");

        let seam = PixelCodecWebp(&codec);
        let input = NormalizableBlob {
            relative_path: "art/loop.gif".into(),
            file_name: "loop.gif".into(),
            stored_mime_type: "image/gif".into(),
            sha256: "in".into(),
            data: anim.clone(),
        };
        let out = normalize_link_blob_image(&input, true, Some(&seam));
        assert_eq!(out, input, "declined: the input, unchanged");

        let still = anim_fixture("still-large.gif");
        let r2 = transcode_to_webp(&codec, &still, "image/gif", TRANSCODE_WEBP_QUALITY);
        assert_eq!(r2.stored_mime_type, "image/webp");
        let out2 = normalize_link_blob_image(
            &NormalizableBlob {
                data: still,
                ..input
            },
            true,
            Some(&seam),
        );
        assert_eq!(out2.stored_mime_type, "image/webp");
        assert_eq!(out2.relative_path, "art/loop.webp");
    }

    #[test]
    fn model_image_transcode_seam() {
        let codec = HostImageCodec;
        let png = png_bytes(10, 10, false);
        let out = WebpTranscoder::transcode(
            &codec,
            &TranscodeInput {
                bytes: png,
                mime_type: "image/png".into(),
                filename: "generated_1.png".into(),
            },
        );
        assert_eq!(out.mime_type, "image/webp");
        assert_eq!(out.filename, "generated_1.webp");
        assert_eq!(out.width, Some(10.0));
    }
}
