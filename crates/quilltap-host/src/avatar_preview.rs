//! The host avatar-preview renderer (P4.6bf) — the production implementation of
//! the core's [`AvatarPreviewRenderer`] seam
//! (`quilltap_core::api::wardrobe`). The core handler
//! (`wardrobe_preview_avatar`) already does parse + guards + prompt build +
//! persist; this renderer is ONLY the render step of v4's
//! `app/api/v1/wardrobe/preview-avatar/route.ts`: ONE raw portrait provider
//! call → base64 decode → the ported `convertToWebP` policy over
//! [`HostImageCodec`] → the `avatar_preview_<safeName>_<Date.now()>.<ext>`
//! filename (rewritten to `.webp` by the transcode).
//!
//! **Wiring this makes the wardrobe dialog's out-of-chat Preview button cost
//! real money** — an image-provider generation per click.
//!
//! The provider call rides the same W4.7f [`RealImageProvider`] machinery the
//! `imageProfileGenerate` runner uses (`HostImageGenerationRunner`): the
//! renderer rebuilds `RealImageProvider::with_bytes_fetch(…)` per
//! request, exactly as that runner rebuilds its deps per run. The composition
//! (provider → decode → transcode → filename) is factored into
//! [`render_over_provider`] so the host integration tests can drive it over a
//! stubbed [`ImageProvider`] with a fixed clock.

use std::future::Future;
use std::pin::Pin;

use base64::Engine as _;

use quilltap_core::api::wardrobe::{
    AvatarPreviewError, AvatarPreviewImage, AvatarPreviewRenderRequest, AvatarPreviewRenderer,
};
use quilltap_core::clock::now_unix_ms;
use quilltap_core::model::image::ImageProvider;
use quilltap_core::model::image_dialects::RealImageProvider;
use quilltap_core::services::file_storage::convert_to_webp;

use crate::image_codec::HostImageCodec;
use crate::wire::{ReqwestImageBytes, ReqwestWireTransport};

/// The production avatar-preview renderer. Stateless; share via `Arc`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostAvatarPreviewRenderer;

impl AvatarPreviewRenderer for HostAvatarPreviewRenderer {
    fn render<'a>(
        &'a self,
        req: &'a AvatarPreviewRenderRequest,
    ) -> Pin<Box<dyn Future<Output = Result<AvatarPreviewImage, AvatarPreviewError>> + Send + 'a>>
    {
        Box::pin(async move {
            // Rebuild the provider per request (the HostImageGenerationRunner
            // idiom — no shared client state to carry).
            let provider = RealImageProvider::with_bytes_fetch(
                ReqwestWireTransport::new(),
                ReqwestImageBytes::new(),
            );
            render_over_provider(&provider, req, now_unix_ms()).await
        })
    }
}

/// v4 `wardrobePreviewAvatar`'s render step, factored over the [`ImageProvider`]
/// seam and an injected `now_ms` clock so the host tests can drive it
/// deterministically. Mirrors `preview-avatar/route.ts` line-for-line:
///
/// - the provider call, whose params the CORE builds through the shared
///   `buildImageGenParams` (v4 `84f33ce94`) and hands over on the request — the
///   host no longer knows the shape. The dangerous-content classifier is still
///   deliberately skipped (v4's comment: an explicit operator-chosen one-shot);
/// - `rawData = imageData?.data || imageData?.b64Json` → the model folds
///   `b64_json` into `data`, so an empty/absent `data` is the same
///   `NoImageData` refusal;
/// - `providerMimeType = imageData.mimeType || 'image/png'`;
///   `providerExt = providerMimeType.split('/')[1] || 'png'`;
/// - `safeName = character.name.replace(/[^a-zA-Z0-9]/g, '_')`;
/// - `providerFilename = avatar_preview_<safeName>_<Date.now()>.<ext>`;
/// - `convertToWebP(rawBuffer, providerMimeType, providerFilename)` — the ported
///   policy over [`HostImageCodec`] (SVG/WebP passthrough, convertible mimes to
///   WebP with the extension rewritten, fallback-to-original on encode failure).
pub async fn render_over_provider<P: ImageProvider>(
    provider: &P,
    req: &AvatarPreviewRenderRequest,
    now_ms: i64,
) -> Result<AvatarPreviewImage, AvatarPreviewError> {
    // v4: an uncaught provider throw → the middleware generic 500 (mapped to
    // `Failed` upstream, which the handler renders as "Internal server error").
    let response = provider
        .generate_image(&req.provider, &req.api_key, &req.params)
        .await
        .map_err(|e| AvatarPreviewError::Failed(e.message))?;

    // `imageData = generationResponse.images?.[0]`; `rawData = data || b64Json`
    // (empty string is falsy) → `NoImageData` when absent.
    let image = response.images.into_iter().next();
    let (raw_data, provider_mime, revised_prompt) = match image {
        Some(img) => (
            img.data.filter(|s| !s.is_empty()),
            img.mime_type.filter(|s| !s.is_empty()),
            // v4 stores `imageData.revisedPrompt || null` — empty → None.
            img.revised_prompt.filter(|s| !s.is_empty()),
        ),
        None => (None, None, None),
    };
    let Some(raw_data) = raw_data else {
        return Err(AvatarPreviewError::NoImageData);
    };

    // `Buffer.from(rawData, 'base64')`. A malformed base64 is v4's synchronous
    // throw inside the handler body → the generic 500 (`Failed`).
    let raw_buffer = base64::engine::general_purpose::STANDARD
        .decode(raw_data.as_bytes())
        .map_err(|e| AvatarPreviewError::Failed(format!("base64 decode failed: {e}")))?;

    let provider_mime = provider_mime.unwrap_or_else(|| "image/png".to_string());
    let provider_ext = provider_mime
        .split('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("png");
    let safe_name: String = req
        .character_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let provider_filename = format!("avatar_preview_{safe_name}_{now_ms}.{provider_ext}");

    let converted = convert_to_webp(
        &HostImageCodec,
        &raw_buffer,
        &provider_mime,
        &provider_filename,
    );
    Ok(AvatarPreviewImage {
        buffer: converted.buffer,
        mime_type: converted.mime_type,
        filename: converted.filename,
        revised_prompt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat};
    use quilltap_core::model::image::{GeneratedImageData, ImageGenError, ImageGenResponse};

    /// A stub [`ImageProvider`] returning a fixed outcome — the host-test
    /// analogue of v4's mocked `createImageProvider` (the model boundary).
    struct StubProvider(Result<ImageGenResponse, String>);

    impl ImageProvider for StubProvider {
        fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &quilltap_core::model::image::ImageGenParams,
        ) -> impl std::future::Future<Output = Result<ImageGenResponse, ImageGenError>> + Send
        {
            let out = match &self.0 {
                Ok(r) => Ok(r.clone()),
                Err(m) => Err(ImageGenError::new(m.clone())),
            };
            async move { out }
        }
    }

    fn png_base64(w: u32, h: u32) -> String {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([40, 90, 160]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        base64::engine::general_purpose::STANDARD.encode(out)
    }

    fn req(name: &str) -> AvatarPreviewRenderRequest {
        AvatarPreviewRenderRequest {
            provider: "OPENAI".into(),
            params: quilltap_core::model::image::ImageGenParams {
                prompt: "a brass-goggled aviatrix".into(),
                model: "dall-e-3".into(),
                n: Some(1.0),
                quality: Some("hd".into()),
                style: Some("natural".into()),
                ..Default::default()
            },
            api_key: "sk-test".into(),
            character_name: name.into(),
        }
    }

    fn ok_response(b64: String, mime: Option<&str>, revised: Option<&str>) -> ImageGenResponse {
        ImageGenResponse {
            images: vec![GeneratedImageData {
                data: Some(b64),
                url: None,
                mime_type: mime.map(str::to_string),
                revised_prompt: revised.map(str::to_string),
            }],
        }
    }

    #[tokio::test]
    async fn png_transcodes_to_webp_with_minted_filename_and_revised_prompt() {
        let provider = StubProvider(Ok(ok_response(
            png_base64(64, 112),
            Some("image/png"),
            Some("a revised brass aviatrix"),
        )));
        // Note the non-alphanumerics in the name → underscores in safeName.
        let image = render_over_provider(&provider, &req("Aria O'Malley-7"), 1_234_567_890)
            .await
            .unwrap();
        assert_eq!(image.mime_type, "image/webp");
        assert_eq!(
            image.filename,
            "avatar_preview_Aria_O_Malley_7_1234567890.webp"
        );
        assert_eq!(
            image.revised_prompt.as_deref(),
            Some("a revised brass aviatrix")
        );
        assert_eq!(
            image::guess_format(&image.buffer).unwrap(),
            ImageFormat::WebP
        );
        assert_eq!(
            image::load_from_memory(&image.buffer)
                .unwrap()
                .to_rgb8()
                .dimensions(),
            (64, 112)
        );
    }

    #[tokio::test]
    async fn provider_mime_defaults_png_when_absent() {
        // No mimeType → v4 `|| 'image/png'`; still convertible → webp output.
        let provider = StubProvider(Ok(ok_response(png_base64(20, 20), None, None)));
        let image = render_over_provider(&provider, &req("Nix"), 42)
            .await
            .unwrap();
        assert_eq!(image.mime_type, "image/webp");
        assert_eq!(image.filename, "avatar_preview_Nix_42.webp");
        assert!(image.revised_prompt.is_none());
    }

    #[tokio::test]
    async fn empty_revised_prompt_collapses_to_none() {
        let provider = StubProvider(Ok(ok_response(
            png_base64(10, 10),
            Some("image/png"),
            Some(""),
        )));
        let image = render_over_provider(&provider, &req("X"), 7).await.unwrap();
        assert!(image.revised_prompt.is_none());
    }

    #[tokio::test]
    async fn no_image_data_arm() {
        // images present but data empty → NoImageData (v4 `!rawData`).
        let provider = StubProvider(Ok(ok_response(String::new(), Some("image/png"), None)));
        let err = render_over_provider(&provider, &req("X"), 1)
            .await
            .unwrap_err();
        assert!(matches!(err, AvatarPreviewError::NoImageData));

        // no images at all → NoImageData too.
        let empty = StubProvider(Ok(ImageGenResponse { images: vec![] }));
        let err2 = render_over_provider(&empty, &req("X"), 1)
            .await
            .unwrap_err();
        assert!(matches!(err2, AvatarPreviewError::NoImageData));
    }

    #[tokio::test]
    async fn provider_throw_maps_to_failed() {
        let provider = StubProvider(Err("400 rejected by the safety system.".into()));
        let err = render_over_provider(&provider, &req("X"), 1)
            .await
            .unwrap_err();
        match err {
            AvatarPreviewError::Failed(m) => assert!(m.contains("safety system")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// A stubbed [`WireTransport`] returning a fixed 2xx body regardless of the
    /// request — the W4.7f "stub provider" flavor, so the FULL production path
    /// (real dialect build → parse → extract → transcode) runs, only the socket
    /// is canned.
    struct AlwaysOkTransport {
        body: String,
    }
    impl quilltap_core::model::wire::WireTransport for AlwaysOkTransport {
        fn send(
            &self,
            _method: &str,
            _url: &str,
            _headers: &[(String, String)],
            _body: &str,
        ) -> impl std::future::Future<
            Output = Result<quilltap_core::model::wire::WireResponse, String>,
        > + Send {
            let body = self.body.clone();
            async move {
                Ok(quilltap_core::model::wire::WireResponse {
                    status: 200,
                    status_text: "OK".into(),
                    body,
                })
            }
        }
    }

    #[tokio::test]
    async fn real_openai_dialect_over_stubbed_wire() {
        // The real OpenAI image dialect parses `data[].b64_json` +
        // `revised_prompt` from a 200 body.
        let png = png_base64(48, 84);
        let body = serde_json::json!({
            "data": [{ "b64_json": png, "revised_prompt": "a revised aviatrix" }]
        })
        .to_string();
        let provider = RealImageProvider::new(AlwaysOkTransport { body });
        let image = render_over_provider(&provider, &req("Aria"), 999)
            .await
            .unwrap();
        assert_eq!(image.mime_type, "image/webp");
        assert_eq!(image.filename, "avatar_preview_Aria_999.webp");
        assert_eq!(image.revised_prompt.as_deref(), Some("a revised aviatrix"));
        assert_eq!(
            image::guess_format(&image.buffer).unwrap(),
            ImageFormat::WebP
        );
        assert_eq!(
            image::load_from_memory(&image.buffer)
                .unwrap()
                .to_rgb8()
                .dimensions(),
            (48, 84)
        );
    }
}
