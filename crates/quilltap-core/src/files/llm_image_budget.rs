//! What an image costs on the wire to an LLM — v4 `lib/files/llm-image-budget.ts`
//! (NEW at `bcd7e4852`, bug 151), ported whole.
//!
//! Storage and transport are different questions, and conflating them is what
//! bug 151 was. The bytes on disk are the *archive*: a `gpt-image-2.5` portrait
//! lands at 1024x1536 and ~1.7 MB of quality-90 WebP, and that is what the
//! gallery, the album and every export should keep. What a vision model needs
//! to read the same picture is far less — it downsamples on arrival anyway —
//! and every byte above that is paid twice, once on the wire and once in the
//! provider's request-size limit.
//!
//! So nothing here touches a stored file. [`shrink_image_for_llm_transport`] is
//! applied at the two points where bytes are loaded *for a model*
//! ([`crate::services::chat_files`]), and its output is thrown away after the
//! request.
//!
//! Two budgets, and both are needed:
//!
//!  - **Per image** ([`LLM_TRANSPORT_MAX_EDGE`], [`LLM_TRANSPORT_TARGET_BASE64`]).
//!    A long-edge cap plus a quality ladder down to a byte ceiling.
//!  - **Per turn** ([`LANTERN_IMAGE_BASE64_BUDGET`], spent by the Lantern walk in
//!    [`crate::services::message_context`]). The per-image cap cannot see how
//!    many images a turn carries, which is precisely how bug 151 happened: two
//!    avatars, each comfortably under the provider's 4 MB per-image limit,
//!    summed to 4.52 MB of base64 and NanoGPT answered the turn with
//!    `413 Request Entity Too Large`.
//!
//! The provider's own per-image limit still applies on top — this module lowers
//! the ceiling, it never raises it.
//!
//! ## What is a host seam here and what is not
//!
//! The DECISION (the arm order, the ceiling, the ladder walk, the grow-discard,
//! the never-throw contract) is pure and lives here. The pixel work is v4's
//! `sharp(buffer).resize({…}).webp({quality}).toBuffer()`, which reaches the
//! host through [`ImageTranscoder::shrink_to_webp`] — the one FALLIBLE method on
//! that seam, and deliberately so: v4 wraps its whole `try` in ONE `catch`, so
//! "sharp threw" and "the encode did not help" are different facts on that side
//! and stay different facts here (the warn arm belongs to the first, the silent
//! grow-discard to the second).
//!
//! **One mechanism difference from v4, agreeing in outcome.** v4's
//! `sharp(buffer).metadata()` REJECTS on undecodable bytes and lands in the
//! `catch`. v5's [`ImageTranscoder::metadata`] is infallible and answers
//! `width: None, height: None` instead, so an undecodable input reaches the same
//! place by a different route: `longest_edge` is 0, the early return's
//! `longest_edge > 0` conjunct fails, the ladder runs, and the FIRST
//! `shrink_to_webp` `Err` is what produces the warn. Both sides return the
//! stored bytes with `was_shrunk: false` and warn exactly once. The tier-1
//! differential drives that row (`undecodable`) and compares the OUTCOME.
//!
//! v4's final `sharp(best).metadata()` is inside the same `catch`; v5's is
//! infallible, so that one arm cannot fail here. It has no observable
//! consequence — a metadata probe of bytes this seam just produced.
//!
//! ## What reaches `combined.log` (P4.D198 Tier-2 item 12)
//!
//! Traced from `quilltap-web`'s `log_file.rs` rather than guessed:
//!
//! * **The DEBUG line does not reach the file at all** under the default
//!   filter. `tracing_filter_directive` falls back to `info` when `RUST_LOG` is
//!   unset, and the file layer sits under that same `EnvFilter` — which is
//!   exactly v4's posture, whose `CURRENT_LEVEL` defaults to INFO and drops
//!   `logger.debug`. So the success line is a `RUST_LOG`-raised diagnostic on
//!   both sides, and the WARN is the one an operator sees unasked.
//! * **`module` lands as v4's value, in v4's position.** The layer seeds
//!   `context.module` from the event TARGET and then overlays the event's own
//!   fields, so this module's `module` field overwrites it with
//!   `files:llm-image-budget` — and `serde_json`'s `preserve_order` keeps a
//!   re-inserted key in its original slot, so it stays FIRST, which is also
//!   where v4's object literal puts it.
//! * **Two PRE-EXISTING, repo-wide shape divergences apply here**, neither
//!   introduced by this module and neither worth a one-off deviation:
//!   (a) field names are snake_case (`original_size` against v4's
//!   `originalSize`), the convention every ported bag follows — e.g.
//!   `provider_failover.rs`'s `chat_id`/`response_length`; and (b) a field
//!   named `error` is HOISTED by the layer out of `context` into the record's
//!   own `error` envelope as `{"name": "Error", "message": …}`, whereas v4's
//!   `logger.warn` has no third argument and leaves `error` as an ordinary
//!   CONTEXT key. 188 sites in this crate already spell it `error = %e`, so
//!   this module follows them.
//! * **P4.91's `…Json` convention applies to no field here** — checked: every
//!   field is a scalar (five strings and four integers), and that convention
//!   exists only for a v4 bag value that is an ARRAY or object.

use crate::files::image_processing::{
    calculate_base64_size, can_resize_image, get_provider_max_base64_size, ImageTranscoder,
};

/// Longest edge, in pixels, of an image sent to an LLM.
///
/// 1024 is the figure the major vision stacks converge on for a single-tile
/// read, and it is the size the operator asked for: a 1536x1024 story
/// background becomes 1024x683, a 1024x1536 portrait becomes 683x1024. Neither
/// loses anything a model was going to use.
pub const LLM_TRANSPORT_MAX_EDGE: i64 = 1024;

/// Byte ceiling for one image's base64 payload — the operator's "under 500K,
/// and still carries enough information to be useful".
///
/// Reached by stepping down [`LLM_TRANSPORT_QUALITY_LADDER`] after the resize,
/// not by refusing the image. An image that cannot make the ceiling even at the
/// bottom of the ladder is still sent, at the smallest encoding we got.
pub const LLM_TRANSPORT_TARGET_BASE64: i64 = 500 * 1024;

/// WebP qualities tried in order until the encoding fits
/// [`LLM_TRANSPORT_TARGET_BASE64`].
///
/// Starts well below storage's quality 90 — at 1024px a model is reading shape,
/// colour and text, none of which survive differently at 78 than at 90 — and
/// stops at 45, below which artefacts start costing the model information
/// rather than saving it bytes.
pub const LLM_TRANSPORT_QUALITY_LADDER: &[i64] = &[78, 65, 55, 45];

/// Ceiling on the *total* base64 an unseen-image walk may add to one turn.
///
/// ~2 MB is four images at the per-image ceiling, which is more pictures than a
/// turn has ever usefully carried, and leaves a wide margin under the narrowest
/// provider body limit we have met (NanoGPT's, which bug 151 found somewhere
/// under 4.5 MB). Spent newest-first: see the consumer's note.
///
/// Defined here because v4 defines it here; the spend itself is the Lantern
/// walk's (`services/message_context.rs`, section K).
pub const LANTERN_IMAGE_BASE64_BUDGET: usize = 2 * 1024 * 1024;

/// Outcome of a transport shrink (v4 `LlmImageShrinkResult`). `was_shrunk` is
/// false when nothing was done — and then `buffer` is the INPUT bytes,
/// `mime_type` the input mime, and `final_size == original_size`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmImageShrinkResult {
    pub buffer: Vec<u8>,
    pub mime_type: String,
    pub was_shrunk: bool,
    pub original_size: usize,
    pub final_size: usize,
    /// `undefined` on every unchanged path (v4 omits width/height there).
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// JS `` `${x}` `` for a possibly-absent dimension: v4 builds
/// `` `${metadata.width}x${metadata.height}` `` with no guard, so a metadata
/// bag that carries no width logs the literal `undefinedxundefined`. Reproduced
/// rather than tidied — the log bag is a comparand in the tier-1 family, and
/// tidying it would be a v5 invention.
fn js_dim(v: Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "undefined".to_string(),
    }
}

/// v4 `shrinkImageForLlmTransport({ buffer, mimeType, provider, filename })` —
/// reduce an image to what a model actually needs to read it.
///
/// Caps the long edge at [`LLM_TRANSPORT_MAX_EDGE`] (never enlarging), then
/// re-encodes as WebP, stepping down [`LLM_TRANSPORT_QUALITY_LADDER`] until the
/// base64 payload fits the smaller of [`LLM_TRANSPORT_TARGET_BASE64`] and the
/// provider's own per-image limit.
///
/// Never throws and never refuses: a format the codec cannot resize, an
/// unreadable buffer or an encode failure all return the input unchanged,
/// because a turn that sends the original bytes is strictly better than a turn
/// that sends none. Callers get `was_shrunk: false` and carry on.
///
/// `provider` is used ONLY to read the provider's per-image ceiling; `None`
/// applies this module's budget alone. `filename` is for logging only.
pub fn shrink_image_for_llm_transport(
    transcoder: &dyn ImageTranscoder,
    buffer: &[u8],
    mime_type: &str,
    provider: Option<&str>,
    filename: Option<&str>,
) -> LlmImageShrinkResult {
    let original_size = buffer.len();
    let unchanged = || LlmImageShrinkResult {
        buffer: buffer.to_vec(),
        mime_type: mime_type.to_string(),
        was_shrunk: false,
        original_size,
        final_size: original_size,
        width: None,
        height: None,
    };

    // (1) A format the codec cannot resize (and anything that is not an image
    // at all) passes through in silence — v4 logs nothing on this arm.
    if !mime_type.starts_with("image/") || !can_resize_image(mime_type) {
        return unchanged();
    }

    // (2) The provider's limit is a ceiling we may lower but must not exceed.
    //
    // MEASURED 2026-09-17: no provider — in v5's eleven manifests OR v4's own
    // plugin registry, which agree value for value — declares a
    // `maxBase64Size` under 500 KiB (OPENAI/GROK/GOOGLE 20 MiB, ANTHROPIC/Z_AI
    // 5 MiB, the rest absent → the 4 MiB default). So this `min` resolves to
    // `LLM_TRANSPORT_TARGET_BASE64` for every provider that exists today and
    // the lower-ceiling arm is unreachable through the real registry on EITHER
    // side. v4's own unit test reaches it by mocking `getAttachmentSupport` to
    // `{maxBase64Size: 8 * 1024}`; this function takes no injectable registry
    // and one is not being added for a test. `ceiling_is_the_lower_of_the_two`
    // below pins the arithmetic and the measurement, so a future manifest under
    // 500 KiB engages the `min` rather than surprising it.
    let ceiling = match provider {
        Some(p) => LLM_TRANSPORT_TARGET_BASE64.min(get_provider_max_base64_size(p)),
        None => LLM_TRANSPORT_TARGET_BASE64,
    };

    // (3) The probe. v4's `await sharp(buffer).metadata()`.
    let metadata = transcoder.metadata(buffer);
    let longest_edge = metadata
        .width
        .unwrap_or(0)
        .max(metadata.height.unwrap_or(0));

    // (4) Already small in both dimensions and already under the ceiling:
    // sending the stored bytes costs less than a pointless re-encode. Note the
    // `longest_edge > 0` conjunct — an unmeasurable image is NOT small, it is
    // unknown, and v4 sends it down the ladder.
    if longest_edge > 0
        && longest_edge <= LLM_TRANSPORT_MAX_EDGE
        && calculate_base64_size(buffer.len()) <= ceiling
    {
        return unchanged();
    }

    // (5) The ladder. `best` is the LAST rung TRIED, not the smallest: v4
    // assigns before testing and breaks on the first fit, so an image that
    // never fits is sent at rung 45.
    let mut best: Option<Vec<u8>> = None;
    for quality in LLM_TRANSPORT_QUALITY_LADDER {
        match transcoder.shrink_to_webp(buffer, LLM_TRANSPORT_MAX_EDGE, *quality) {
            Ok(encoded) => {
                let fits = calculate_base64_size(encoded.len()) <= ceiling;
                best = Some(encoded);
                if fits {
                    break;
                }
            }
            // v4's `catch` wraps the WHOLE ladder, so a throw on rung 2
            // discards rung 1's encode along with everything else: the stored
            // bytes go out and one warn is logged. A partial ladder never
            // leaks its `best`.
            Err(error) => {
                tracing::warn!(
                    module = "files:llm-image-budget",
                    filename = filename.unwrap_or_default(),
                    provider = provider.unwrap_or_default(),
                    mime_type = mime_type,
                    error = %error,
                    "Could not shrink image for LLM transport; sending stored bytes"
                );
                return unchanged();
            }
        }
    }

    // (6) v4's `if (!best) return unchanged`. Unreachable while
    // `LLM_TRANSPORT_QUALITY_LADDER` is non-empty — the loop assigns on its
    // first iteration — and carried rather than invented around, because an
    // empty ladder is a one-character edit away and this is what v4 does then.
    let Some(best) = best else {
        return unchanged();
    };

    // (7) A re-encode that grew the payload is a re-encode worth discarding — a
    // small PNG of flat colour can beat WebP at these qualities. NOTE the
    // SECOND conjunct: an over-1024-edge input is taken even when the encode
    // grew, because the dimension cap is worth paying bytes for on its own.
    if best.len() >= original_size && longest_edge <= LLM_TRANSPORT_MAX_EDGE {
        return unchanged();
    }

    // (8) v4's `await sharp(best).metadata()` + the success line.
    let final_meta = transcoder.metadata(&best);
    let final_size = best.len();
    tracing::debug!(
        module = "files:llm-image-budget",
        filename = filename.unwrap_or_default(),
        provider = provider.unwrap_or_default(),
        original_size = original_size,
        final_size = final_size,
        original_dimensions = format!("{}x{}", js_dim(metadata.width), js_dim(metadata.height)),
        final_dimensions = format!("{}x{}", js_dim(final_meta.width), js_dim(final_meta.height)),
        ceiling = ceiling,
        "Image shrunk for LLM transport"
    );

    LlmImageShrinkResult {
        buffer: best,
        mime_type: "image/webp".to_string(),
        was_shrunk: true,
        original_size,
        final_size,
        width: final_meta.width,
        height: final_meta.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::image_processing::{ImageMetadata, OutputFormat};
    use std::sync::Mutex;

    /// The unit fake: answers `metadata` from a script and `shrink_to_webp`
    /// with a buffer of a scripted length. Deliberately NOT the harness's
    /// `ScriptedTranscoder` — these tests pin arms the corpus cannot reach
    /// (`provider: None`'s ceiling, the empty-filename rendering) and want to
    /// be readable without the corpus in hand.
    struct Fake {
        width: Option<i64>,
        height: Option<i64>,
        /// One entry per ladder rung, in order: `Ok(len)` or `Err(message)`.
        steps: Vec<Result<usize, String>>,
        final_width: Option<i64>,
        final_height: Option<i64>,
        calls: Mutex<Vec<i64>>,
    }

    impl Fake {
        fn new(width: i64, height: i64, steps: Vec<Result<usize, String>>) -> Self {
            Fake {
                width: Some(width),
                height: Some(height),
                steps,
                final_width: Some(683),
                final_height: Some(1024),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl ImageTranscoder for Fake {
        fn metadata(&self, buffer: &[u8]) -> ImageMetadata {
            // The original buffer is the one filled with `b'O'`; anything else
            // is an encode this fake produced.
            if buffer.first() == Some(&b'O') {
                ImageMetadata {
                    width: self.width,
                    height: self.height,
                    has_alpha: false,
                }
            } else {
                ImageMetadata {
                    width: self.final_width,
                    height: self.final_height,
                    has_alpha: false,
                }
            }
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
        fn shrink_to_webp(
            &self,
            _buffer: &[u8],
            max_edge: i64,
            quality: i64,
        ) -> Result<Vec<u8>, String> {
            self.calls.lock().unwrap().push(quality);
            assert_eq!(max_edge, LLM_TRANSPORT_MAX_EDGE, "the seam's max edge");
            let n = self.calls.lock().unwrap().len();
            match self.steps.get(n - 1) {
                Some(Ok(len)) => Ok(vec![b'E'; *len]),
                Some(Err(m)) => Err(m.clone()),
                None => panic!(
                    "the ladder asked for rung {n}; the script has {} ",
                    self.steps.len()
                ),
            }
        }
    }

    fn original(len: usize) -> Vec<u8> {
        vec![b'O'; len]
    }

    #[test]
    fn non_image_and_unresizable_pass_through_untouched() {
        let fake = Fake::new(4096, 4096, vec![]);
        let buf = original(64);
        for mime in ["text/plain", "image/svg+xml", "application/pdf"] {
            let r = shrink_image_for_llm_transport(&fake, &buf, mime, Some("NANOGPT"), None);
            assert!(!r.was_shrunk, "{mime}");
            assert_eq!(r.buffer, buf, "{mime} keeps its bytes");
            assert_eq!(r.mime_type, mime, "{mime} keeps its mime");
            assert_eq!(r.final_size, r.original_size);
            assert_eq!((r.width, r.height), (None, None));
        }
        // The codec was never consulted.
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn small_and_under_the_ceiling_early_returns() {
        let fake = Fake::new(800, 600, vec![]);
        let buf = original(1024);
        let r = shrink_image_for_llm_transport(&fake, &buf, "image/png", Some("NANOGPT"), None);
        assert!(!r.was_shrunk);
        assert_eq!(r.buffer, buf);
        assert!(fake.calls.lock().unwrap().is_empty(), "no encode attempted");
    }

    #[test]
    fn the_ladder_stops_at_the_first_rung_that_fits() {
        // 400,000 bytes of base64 is 533,334 > the 500 KiB ceiling; 300,000
        // bytes is 400,000 <= it.
        let fake = Fake::new(2048, 1536, vec![Ok(400_000), Ok(300_000)]);
        let buf = original(1_000_000);
        let r = shrink_image_for_llm_transport(&fake, &buf, "image/webp", Some("NANOGPT"), None);
        assert!(r.was_shrunk);
        assert_eq!(r.mime_type, "image/webp");
        assert_eq!(r.final_size, 300_000);
        assert_eq!(*fake.calls.lock().unwrap(), vec![78, 65]);
        assert_eq!((r.width, r.height), (Some(683), Some(1024)));
    }

    #[test]
    fn an_image_that_never_fits_is_still_sent_at_the_bottom_rung() {
        let fake = Fake::new(
            4000,
            3000,
            vec![Ok(900_000), Ok(800_000), Ok(700_000), Ok(600_000)],
        );
        let buf = original(2_000_000);
        let r = shrink_image_for_llm_transport(&fake, &buf, "image/webp", Some("NANOGPT"), None);
        assert!(r.was_shrunk, "never refuses");
        assert_eq!(r.final_size, 600_000, "the LAST rung tried");
        assert_eq!(*fake.calls.lock().unwrap(), vec![78, 65, 55, 45]);
    }

    #[test]
    fn a_grown_encode_is_discarded_only_when_the_edge_was_already_small() {
        // Edge <= 1024 and every rung grows → unchanged.
        let fake = Fake::new(
            1000,
            900,
            vec![Ok(500_000), Ok(500_000), Ok(500_000), Ok(500_000)],
        );
        let buf = original(400_000); // base64 533,334 > the ceiling, so the ladder runs
                                     // …and the discard is SILENT: v4 logs on neither unchanged arm that is
                                     // not the catch, so a line here would be an invention. (Mutation M5b
                                     // adds exactly that line; without this leg only the tier-1 family
                                     // caught it.)
        let (r, lines) = crate::test_support::captured_with(|| {
            shrink_image_for_llm_transport(&fake, &buf, "image/webp", Some("NANOGPT"), None)
        });
        assert!(lines.is_empty(), "the grow-discard is silent: {lines:?}");
        assert!(!r.was_shrunk, "the grow-discard fires");
        assert_eq!(r.buffer, buf);
        assert_eq!(
            r.mime_type, "image/webp",
            "the INPUT mime, not the encode's"
        );

        // The SECOND conjunct: an oversize edge is taken even when it grew.
        let fake2 = Fake::new(2048, 1536, vec![Ok(5_000)]);
        let buf2 = original(1_000);
        let r2 = shrink_image_for_llm_transport(&fake2, &buf2, "image/webp", Some("NANOGPT"), None);
        assert!(r2.was_shrunk, "an over-1024 edge is worth paying bytes for");
        assert_eq!(r2.final_size, 5_000);
    }

    #[test]
    fn a_throw_mid_ladder_discards_the_partial_best() {
        let fake = Fake::new(2048, 1536, vec![Ok(600_000), Err("scripted boom".into())]);
        let buf = original(1_000_000);
        let r = shrink_image_for_llm_transport(&fake, &buf, "image/webp", Some("NANOGPT"), None);
        assert!(!r.was_shrunk);
        assert_eq!(r.buffer, buf, "rung 1's encode must NOT leak");
        assert_eq!(*fake.calls.lock().unwrap(), vec![78, 65]);
    }

    #[test]
    fn an_unmeasurable_image_takes_the_ladder_not_the_early_return() {
        // The mechanism difference from v4, made explicit: no dimensions means
        // `longest_edge == 0`, which FAILS the early return's first conjunct.
        struct NoDims;
        impl ImageTranscoder for NoDims {
            fn metadata(&self, _b: &[u8]) -> ImageMetadata {
                ImageMetadata::default()
            }
            fn resize_step(&self, b: &[u8], _t: i64, _f: OutputFormat, _q: i64) -> Vec<u8> {
                b.to_vec()
            }
            // The default `shrink_to_webp` Err — the not-configured answer.
        }
        let buf = original(8);
        let r = shrink_image_for_llm_transport(&NoDims, &buf, "image/png", Some("NANOGPT"), None);
        assert!(!r.was_shrunk, "the default Err lands in the warn arm");
        assert_eq!(r.buffer, buf);
    }

    #[test]
    fn a_measurable_image_with_no_dimensions_logs_undefinedxundefined() {
        // v4's template literal has no guard, so a metadata bag that resolves
        // WITHOUT a width renders the literal `undefined`. Reachable: the
        // encode succeeds, so the debug line fires.
        let mut fake = Fake::new(1, 1, vec![Ok(10)]);
        fake.width = None;
        fake.height = None;
        fake.final_width = None;
        fake.final_height = None;
        let buf = original(400_000);
        let (r, lines) = crate::test_support::captured_with(|| {
            shrink_image_for_llm_transport(
                &fake,
                &buf,
                "image/webp",
                Some("NANOGPT"),
                Some("a.webp"),
            )
        });
        assert!(r.was_shrunk);
        let line = lines
            .iter()
            .find(|l| l.contains("Image shrunk for LLM transport"))
            .unwrap_or_else(|| panic!("no debug line in {lines:?}"));
        assert!(
            line.contains("original_dimensions=undefinedxundefined"),
            "{line}"
        );
        assert!(
            line.contains("final_dimensions=undefinedxundefined"),
            "{line}"
        );
    }

    #[test]
    fn the_success_line_fires_with_v4s_bag_and_stays_silent_otherwise() {
        let fake = Fake::new(2048, 1536, vec![Ok(300_000)]);
        let buf = original(1_000_000);
        let (_r, lines) = crate::test_support::captured_with(|| {
            shrink_image_for_llm_transport(
                &fake,
                &buf,
                "image/webp",
                Some("NANOGPT"),
                Some("portrait.webp"),
            )
        });
        let line = lines
            .iter()
            .find(|l| l.contains("Image shrunk for LLM transport"))
            .unwrap_or_else(|| panic!("no debug line in {lines:?}"));
        assert!(line.starts_with("DEBUG "), "{line}");
        for want in [
            "module=files:llm-image-budget",
            "filename=portrait.webp",
            "provider=NANOGPT",
            "original_size=1000000",
            "final_size=300000",
            "original_dimensions=2048x1536",
            "final_dimensions=683x1024",
            // The ceiling is `min(500 KiB, NANOGPT's)`; NANOGPT declares none,
            // so the 4 MiB default loses to the transport target.
            "ceiling=512000",
        ] {
            assert!(line.contains(want), "missing {want} in {line}");
        }
        // The silence leg: no WARN on a successful shrink.
        assert!(
            !lines.iter().any(|l| l.starts_with("WARN ")),
            "a successful shrink must not warn: {lines:?}"
        );

        // …and the early return logs NOTHING at all (v4 logs on neither
        // unchanged arm that is not the catch).
        let quiet = Fake::new(800, 600, vec![]);
        let (_r2, lines2) = crate::test_support::captured_with(|| {
            shrink_image_for_llm_transport(&quiet, &original(64), "image/png", None, Some("s.png"))
        });
        assert!(lines2.is_empty(), "the early return is silent: {lines2:?}");
    }

    #[test]
    fn the_failure_line_fires_with_v4s_bag_and_stays_silent_otherwise() {
        let fake = Fake::new(2048, 1536, vec![Err("scripted decode failure".into())]);
        let buf = original(1_000);
        let (r, lines) = crate::test_support::captured_with(|| {
            shrink_image_for_llm_transport(&fake, &buf, "image/png", Some("GROK"), Some("junk.png"))
        });
        assert!(!r.was_shrunk);
        let line = lines
            .iter()
            .find(|l| l.contains("Could not shrink image for LLM transport; sending stored bytes"))
            .unwrap_or_else(|| panic!("no warn line in {lines:?}"));
        assert!(line.starts_with("WARN "), "{line}");
        for want in [
            "module=files:llm-image-budget",
            "filename=junk.png",
            "provider=GROK",
            "mime_type=image/png",
            "error=scripted decode failure",
        ] {
            assert!(line.contains(want), "missing {want} in {line}");
        }
        // The silence leg: no DEBUG on a failed shrink.
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Image shrunk for LLM transport")),
            "a failed shrink must not claim success: {lines:?}"
        );
    }

    /// RECORDED NARROWING: an absent `filename`/`provider` renders as an EMPTY
    /// field where v4's JSON-serialized bag simply has no key (`JSON.stringify`
    /// drops `undefined`). `tracing` has no conditional field, and branching
    /// the whole macro four ways to reproduce an absence would be worse than
    /// recording it. Unreachable from production: BOTH loaders always have a
    /// filename (`files.originalFilename`; `originalFileName ?? fileName`) and
    /// both gate the whole call on a `provider` being present.
    #[test]
    fn an_absent_filename_renders_empty_rather_than_absent() {
        let fake = Fake::new(2048, 1536, vec![Ok(300_000)]);
        let (_r, lines) = crate::test_support::captured_with(|| {
            shrink_image_for_llm_transport(&fake, &original(1_000_000), "image/webp", None, None)
        });
        let line = lines
            .iter()
            .find(|l| l.contains("Image shrunk for LLM transport"))
            .unwrap();
        assert!(line.contains("filename= "), "{line}");
        assert!(line.contains("provider= "), "{line}");
    }

    /// Tier-2 item 11: the ceiling comparison rides on
    /// `calculate_base64_size`, so pin that it IS `Math.ceil(len * 4 / 3)` —
    /// v4's spelling — over a wide range and at bug 151's own three sizes.
    #[test]
    fn base64_size_equals_v4s_ceil_division() {
        for len in 0usize..=10_000 {
            let want = ((len as f64) * 4.0 / 3.0).ceil() as i64;
            assert_eq!(calculate_base64_size(len), want, "len {len}");
        }
        // The two reported avatars and their sum (bug 151's measurement table).
        for len in [1_757_654usize, 1_796_172, 3_553_826] {
            let want = ((len as f64) * 4.0 / 3.0).ceil() as i64;
            assert_eq!(calculate_base64_size(len), want, "len {len}");
        }
        // And the table's own claim: each avatar is under the 4 MB per-image
        // cap that left it unresized, while their SUM is over it.
        assert!(calculate_base64_size(1_757_654) < 4 * 1024 * 1024);
        assert!(calculate_base64_size(1_796_172) < 4 * 1024 * 1024);
        assert!(calculate_base64_size(3_553_826) > 4 * 1024 * 1024);
    }

    /// The `min` arm's arithmetic AND the measurement that makes it dormant.
    /// If a manifest ever declares a ceiling under 500 KiB this test is the
    /// first thing that notices.
    #[test]
    fn ceiling_is_the_lower_of_the_two() {
        for p in [
            "OPENAI",
            "GROK",
            "GOOGLE",
            "ANTHROPIC",
            "Z_AI",
            "NANOGPT",
            "OPENROUTER",
            "DEEPSEEK",
            "OLLAMA",
            "OPENAI_COMPATIBLE",
            "BOGUS",
        ] {
            let declared = get_provider_max_base64_size(p);
            assert!(
                declared >= LLM_TRANSPORT_TARGET_BASE64,
                "{p} declares {declared}, under the transport target — the `min` \
                 arm is now LIVE and the tier-1 family should cover it"
            );
            assert_eq!(
                LLM_TRANSPORT_TARGET_BASE64.min(declared),
                LLM_TRANSPORT_TARGET_BASE64,
                "{p}"
            );
        }
        // The arithmetic itself, independent of any manifest.
        assert_eq!(LLM_TRANSPORT_TARGET_BASE64.min(8 * 1024), 8 * 1024);
    }

    #[test]
    fn the_constants_are_v4s() {
        assert_eq!(LLM_TRANSPORT_MAX_EDGE, 1024);
        assert_eq!(LLM_TRANSPORT_TARGET_BASE64, 512_000);
        assert_eq!(LLM_TRANSPORT_QUALITY_LADDER, &[78, 65, 55, 45]);
        assert_eq!(LANTERN_IMAGE_BASE64_BUDGET, 2_097_152);
    }
}
