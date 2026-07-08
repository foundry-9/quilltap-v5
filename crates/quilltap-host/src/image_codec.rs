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
//!   `resizeImageForProvider` decision loop drives (sharp inventory row 4);
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
//! * **Animated GIF → WebP** (`transcodeToWebP`'s `{animated: true}`): the
//!   `webp` crate encodes single frames only (no libwebpmux), so an animated
//!   GIF degrades to its FIRST FRAME (a quality divergence, not a policy one —
//!   the mime/extension/sha policy is identical).
//! * **AVIF / HEIC / HEIF decode** are not wired (they need dav1d / libheif C
//!   stacks): those inputs fail decode and take v4's own failure-passthrough
//!   branch (stored as-is with the original mime — v4 with a working sharp
//!   would have transcoded; flagged in the module docs of the core policy).
//! * `resize_step` is infallible by trait shape; a decode/encode failure
//!   returns the ORIGINAL bytes (no resize), where v4's sharp would throw and
//!   the caller would skip the file — a documented divergence reachable only
//!   when a decodable-claimed image fails to decode.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader};

use quilltap_core::files::image_processing::{
    ImageMetadata, ImageTranscoder as ResizeTranscoder, OutputFormat,
};
use quilltap_core::model::image::{
    ImageTranscoder as WebpTranscoder, TranscodeInput, TranscodeOutput,
};
use quilltap_core::services::file_storage::{convert_to_webp, PixelCodec, THUMBNAIL_QUALITY};

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
        _animated: bool,
    ) -> Result<Vec<u8>, String> {
        // `effort` (libwebp method) is not exposed by the `webp` crate's
        // simple encoder — an encoding-speed/size knob, not a policy one.
        // `animated: true` degrades to first-frame (module docs).
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
    fn thumbnail_is_square_webp_cover() {
        let codec = HostImageCodec;
        let jpeg = jpeg_bytes(120, 40);
        let thumb = codec.thumbnail_webp(&jpeg, 30).unwrap();
        assert_eq!(format_of(&thumb), ImageFormat::WebP);
        assert_eq!(dims_of(&thumb), (30, 30)); // cover crop, not letterboxed
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
