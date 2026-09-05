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
//! v4's anchors, by call site:
//!
//!   * `image-generation-handler.ts:339` — `tools.generate_image`
//!   * `image-generation-handler.ts:434` — `tools.generate_image.concierge-reroute`
//!   * `image-generation-handler.ts:869` — `tools.generate_image.style-options`
//!     (`resolveProfileLoras`, BEFORE the params build — the one this lane
//!     restored; without it a stored-list problem raised during the crafter's
//!     pre-pass is indistinguishable from one raised at generation time)
//!   * `character-avatar.ts:239` / `:320` — `background-jobs.character-avatar`(`.concierge-reroute`)
//!   * `story-background.ts:648` / `:787` — `background-jobs.story-background`(`.concierge-reroute`)
//!   * `app/api/v1/images/route.ts:282` — `api.v1.images.generate`
//!   * `app/api/v1/wardrobe/preview-avatar/route.ts:111` — `api.v1.wardrobe.preview-avatar`
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
    // The eight literals v5 has sites for, searched across the files that own
    // them. (`params_builder.rs`'s doc comment says "nine literals across the
    // five consolidated sites"; the ninth is measured below.)
    for anchor in [
        "tools.generate_image",
        "tools.generate_image.concierge-reroute",
        "tools.generate_image.style-options",
        "background-jobs.character-avatar",
        "background-jobs.character-avatar.concierge-reroute",
        "background-jobs.story-background",
        "background-jobs.story-background.concierge-reroute",
        "api.v1.wardrobe.preview-avatar",
        // P4.76 landed `POST /api/v1/images?action=generate`, so v4's ninth
        // anchor finally has a v5 call site. The tripwire that asserted it
        // ABSENT is gone, exactly as its own doc comment instructed.
        "api.v1.images.generate",
    ] {
        assert!(
            haystack().contains(&format!("\"{anchor}\"")),
            "v4 anchor {anchor:?} has no v5 call site"
        );
    }
}

/// Every v5 file that names an image-params call-site anchor.
fn haystack() -> String {
    [
        "tools/generate_image.rs",
        "services/image_job_common.rs",
        "services/character_avatar_job.rs",
        "services/story_background_job.rs",
        "api/wardrobe.rs",
        "api/image_profiles.rs",
        "api/images.rs",
    ]
    .iter()
    .map(|f| source(f))
    .collect::<Vec<_>>()
    .join("\n")
}
