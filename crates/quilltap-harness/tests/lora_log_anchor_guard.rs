//! P4.70 — the `[Image LoRA]` call-site anchors, pinned where they live.
//!
//! v4 spreads the caller's `{ context, chatId, jobId, profileId }` into every
//! `[Image LoRA]` line (`lib/image-gen/params-builder.ts:150`
//! `{ provider, model, ...logContext }`), so an operator reading
//! `combined.log` can tell WHICH generation dropped a malformed adapter. The
//! five sentences and the spread reaching them are pinned by the capture tests
//! in `quilltap_core::image_gen::lora_support`; what those cannot see is
//! whether each CALL SITE still supplies its anchor — a site that reverts to
//! `Default::default()` keeps every capture test green while logging an empty
//! `context`.
//!
//! v4's anchors, by call site (at the `4a38222e6` pin — P4.D283, v4
//! `9ecebf986`: ONE context per site; the `.concierge-route` /
//! `.concierge-reroute` suffixes are GONE, and the shared image attempt
//! (`image-attempt.ts`) spreads `profileId` + `rerouted` into every build's
//! context instead):
//!
//!   * `image-generation-handler.ts` — `tools.generate_image` (the attempt)
//!   * `image-generation-handler.ts` — `tools.generate_image.style-options`
//!     (`resolveProfileLoras`, BEFORE the params build — the one P4.70
//!     restored; without it a stored-list problem raised during the crafter's
//!     pre-pass is indistinguishable from one raised at generation time)
//!   * `character-avatar.ts` — `background-jobs.character-avatar`
//!   * `story-background.ts` — `background-jobs.story-background`
//!   * `item-image-generation.ts` — `wardrobe.item-image-generation`
//!   * `app/api/v1/images/route.ts` — `api.v1.images.generate`
//!   * `app/api/v1/wardrobe/preview-avatar/route.ts` — `api.v1.wardrobe.preview-avatar`
//!
//! A differential cannot see a log-only fix, so this is a source census in the
//! `db_error_key_guard` / `archived_wearer_read_guard` idiom.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test lora_log_anchor_guard

use std::path::PathBuf;

fn source(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn the_style_options_resolution_carries_v4s_anchor_and_the_chat_id() {
    let src = source("tools/generate_image.rs");
    // The pre-crafter resolution (v4 image-generation-handler.ts:868-871).
    let at = src
        .find("let (_, _, lora_trigger_phrase) = resolve_profile_loras(")
        .expect("the style-options resolve_profile_loras call has moved or gone");
    let call = &src[at..at + 700];
    assert!(
        call.contains(r#"context: "tools.generate_image.style-options","#),
        "the style-options resolution must carry v4's own anchor:\n{call}"
    );
    assert!(
        call.contains("chat_id: ctx.chat_id.clone(),"),
        "v4 spreads `chatId` at this site too:\n{call}"
    );
}

#[test]
fn every_v4_image_params_anchor_still_has_a_v5_call_site() {
    // The seven literals v5 has sites for, searched across the files that own
    // them (`params_builder.rs`'s doc comment: ONE literal per site since v4
    // `9ecebf986`).
    for anchor in [
        "tools.generate_image",
        "tools.generate_image.style-options",
        "background-jobs.character-avatar",
        "background-jobs.story-background",
        "wardrobe.item-image-generation",
        "api.v1.wardrobe.preview-avatar",
        // P4.76 landed `POST /api/v1/images?action=generate`, so v4's images
        // anchor has a v5 call site. The tripwire that asserted it ABSENT is
        // gone, exactly as its own doc comment instructed.
        "api.v1.images.generate",
    ] {
        assert!(
            haystack().contains(&format!("\"{anchor}\"")),
            "v4 anchor {anchor:?} has no v5 call site"
        );
    }
}

/// v4 `9ecebf986` deleted the per-site reroute contexts
/// (`tools.generate_image.concierge-reroute`,
/// `background-jobs.character-avatar.concierge-route`,
/// `background-jobs.story-background.concierge-reroute`): a rerouted build now
/// logs under its site's ONE context with `rerouted: true`. A literal coming
/// back is a site that stopped going through the shared attempt.
#[test]
fn no_retired_concierge_suffix_anchor_survives() {
    let hay = haystack();
    for retired in [
        "\"tools.generate_image.concierge-reroute\"",
        "\"background-jobs.character-avatar.concierge-route\"",
        "\"background-jobs.story-background.concierge-reroute\"",
    ] {
        assert!(
            !hay.contains(retired),
            "the retired reroute anchor {retired} is back in a v5 call site"
        );
    }
}

/// The shared attempt is where `profileId` + `rerouted` ride into the builder's
/// log context (v4 `image-attempt.ts:116`: `logContext: { ...options.params?.
/// logContext, profileId: profile.id, rerouted }`) — and the `[Image LoRA]` +
/// params-builder lines render the key.
#[test]
fn the_shared_attempt_spreads_profile_id_and_rerouted_into_the_builder_context() {
    let attempt = source("image_gen/image_attempt.rs");
    assert!(
        attempt.contains("profile_id: Some(profile.id.clone()),")
            && attempt.contains("rerouted: Some(rerouted),"),
        "the attempt must spread `profileId` + `rerouted` into the params builder's log context"
    );
    let lora = source("image_gen/lora_support.rs");
    assert!(
        lora.matches("rerouted = ?log_context.rerouted,").count() >= 5,
        "every `[Image LoRA]` line renders the `rerouted` key"
    );
    assert!(
        source("image_gen/params_builder.rs").contains("rerouted = ?log_context.rerouted,"),
        "the params builder's own line renders the `rerouted` key"
    );
}

/// Every v5 file that names an image-params call-site anchor.
fn haystack() -> String {
    [
        "tools/generate_image.rs",
        "services/image_job_common.rs",
        "services/character_avatar_job.rs",
        "services/story_background_job.rs",
        "services/wardrobe_item_image_generation.rs",
        "image_gen/image_attempt.rs",
        "api/wardrobe.rs",
        "api/image_profiles.rs",
        "api/images.rs",
    ]
    .iter()
    .map(|f| source(f))
    .collect::<Vec<_>>()
    .join("\n")
}
