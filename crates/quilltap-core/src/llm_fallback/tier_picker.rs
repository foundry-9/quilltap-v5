//! Fallback engine — the tier picker (v4 `lib/llm/fallback/tier-picker.ts`).
//!
//! The last resort in a fallback chain: when a profile and its named understudy
//! have both failed, draft ONE more candidate from the rest of the user's
//! company. One, not a list — the chain is capped at three attempts on purpose,
//! so this gets a single chance and then the call fails for real.
//!
//! Only reached when the failed profile has `allowTierFallback` on, because an
//! auto-picked replacement spends money at a provider the user did not choose
//! for this call.
//!
//! The ranking mirrors `pickAutoConfigureCandidates` (v4
//! `lib/services/auto-configure.service.ts` — unported, and recorded as such at
//! P4.D85) rather than inventing a second notion of "similar tier".

use crate::files::image_transport::provider_can_transport_images;
use crate::model_classes::get_model_class;
use crate::services::api_key_service::{provider_accepts_api_key, provider_requires_api_key};

use super::types::{FallbackContext, FallbackProfile};

/// Quality rank of a profile's model class. `-1` means the user never set one,
/// which is a distinct state from "set to the lowest tier" — see
/// [`tier_matches`].
fn quality_of(profile: &FallbackProfile) -> i64 {
    match profile.model_class.as_deref() {
        None => -1,
        Some(name) => get_model_class(name).map(|mc| mc.quality).unwrap_or(-1),
    }
}

/// Whether `candidate` is of the same or better tier than the profile that just
/// failed.
///
/// An unset `modelClass` is quality-*unknown*, not quality-zero. Unknown against
/// unknown is a match — neither profile has been classified, so the comparison
/// has nothing to say and blocking on it would make tier fallback useless for
/// the many users who never fill the field in. Unknown against a known tier is a
/// non-match in both directions: promoting an unclassified profile over a Deep
/// one could quietly downgrade the call, and demanding a classification the
/// failed profile itself lacks is arbitrary.
pub fn tier_matches(candidate: &FallbackProfile, failed: &FallbackProfile) -> bool {
    let candidate_quality = quality_of(candidate);
    let failed_quality = quality_of(failed);

    if candidate_quality == -1 && failed_quality == -1 {
        return true;
    }
    if candidate_quality == -1 || failed_quality == -1 {
        return false;
    }
    candidate_quality >= failed_quality
}

/// Whether a profile could actually authenticate if we sent a call through it.
///
/// Deliberately a *static* check, not a decrypt: the picker runs on a failure
/// path, sometimes inside the job runner, and a round trip to the key table per
/// candidate would add latency to a call that is already late. A provider that
/// takes no key at all (Ollama and friends) always passes.
fn has_usable_credentials(profile: &FallbackProfile) -> bool {
    if !provider_accepts_api_key(&profile.provider) {
        return true;
    }
    if profile.api_key_id.is_some() {
        return true;
    }
    !provider_requires_api_key(&profile.provider)
}

/// Why a candidate was passed over — v4's `skipped` debug entries, kept so the
/// reason reaches the log at the same granularity.
struct Skipped {
    profile_id: String,
    name: String,
    reason: &'static str,
}

/// Pick at most one replacement for a failed profile.
///
/// * `failed` — the profile whose call just failed
/// * `all_profiles` — every connection profile belonging to the user
/// * `context` — what the call needs from a stand-in
///
/// Returns the single best candidate, or `None` when nobody qualifies.
pub fn pick_tier_candidate<'a>(
    failed: &FallbackProfile,
    all_profiles: &'a [FallbackProfile],
    context: &FallbackContext,
) -> Option<&'a FallbackProfile> {
    let mut skipped: Vec<Skipped> = Vec::new();

    let eligible: Vec<&FallbackProfile> = all_profiles
        .iter()
        .filter(|candidate| {
            let mut note = |reason: &'static str| {
                skipped.push(Skipped {
                    profile_id: candidate.id.clone(),
                    name: candidate.name.clone(),
                    reason,
                });
                false
            };

            if candidate.id == failed.id {
                return note("is the failed profile");
            }
            if context.already_tried.contains(&candidate.id) {
                return note("already tried on this call");
            }

            // A Courier request is rendered as Markdown for a human to carry to
            // an external LLM by hand. Whatever that is, it is not automatic
            // failover.
            if candidate.transport == "courier" {
                return note("courier transport");
            }

            if !has_usable_credentials(candidate) {
                return note("no usable API key");
            }

            // Danger-safe. The reroute exists precisely because the content
            // needs a provider the user has cleared for it; drafting a
            // mainstream model here would hand the content back to the
            // moderation that just refused it.
            if context.dangerous && !candidate.is_dangerous_compatible {
                return note("not cleared for dangerous content");
            }

            if context.needs_vision {
                if !candidate.supports_image_upload {
                    return note("does not accept image uploads");
                }
                // Both halves matter: a describer whose plugin drops the bytes
                // would answer from the prompt alone and invent a picture.
                if !provider_can_transport_images(&candidate.provider) {
                    return note("provider cannot transport images");
                }
            }

            // Tools are the profile's own master override. Native
            // function-calling support is NOT required — a model without it is
            // served by the pseudo-tool formats, which is what
            // `pseudoToolMode: 'auto'` resolves to.
            if context.needs_tools && !candidate.allow_tool_use {
                return note("tool use disabled on the profile");
            }

            if !tier_matches(candidate, failed) {
                return note("model class below the failed profile");
            }

            true
        })
        .collect();

    if !skipped.is_empty() {
        tracing::debug!(
            target: "quilltap::fallback",
            failed_profile_id = %failed.id,
            purpose = %context.purpose,
            skipped = ?skipped
                .iter()
                .map(|s| (s.profile_id.as_str(), s.name.as_str(), s.reason))
                .collect::<Vec<_>>(),
            "Tier picker skipped candidates"
        );
    }

    if eligible.is_empty() {
        tracing::debug!(
            target: "quilltap::fallback",
            failed_profile_id = %failed.id,
            failed_provider = %failed.provider,
            purpose = %context.purpose,
            considered_count = all_profiles.len(),
            "No tier candidate qualified"
        );
        return None;
    }

    // `ProviderEnum` is an open string — a plugin-supplied id, not a closed enum
    // — so nothing guarantees the stored casing.
    let failed_provider = failed.provider.to_uppercase();

    let mut ranked = eligible.clone();
    // v4 sorts with `[...eligible].sort(cmp)`. `Array.prototype.sort` is stable
    // in every engine Quilltap runs on, and so is `sort_by` — the third
    // comparator key (`sortIndex`) can still tie, and both sides then keep the
    // input order.
    ranked.sort_by(|a, b| {
        // 1. A different provider first: the failure we are routing around is
        //    usually the provider's, not the model's, and a sibling profile on
        //    the same dead endpoint will fail identically.
        let a_different = i64::from(a.provider.to_uppercase() != failed_provider);
        let b_different = i64::from(b.provider.to_uppercase() != failed_provider);
        if a_different != b_different {
            return b_different.cmp(&a_different);
        }
        // 2. Then the best model available.
        let quality_delta = quality_of(b) - quality_of(a);
        if quality_delta != 0 {
            return quality_delta.cmp(&0);
        }
        // 3. Then the user's own ordering, so the choice is at least
        //    predictable. v4 subtracts two `?? 0` floats and returns the
        //    difference; `total_cmp` is the same ordering for every value the
        //    column can hold (a REAL, never NaN — the schema is
        //    `z.number().default(0)`).
        a.sort_index.total_cmp(&b.sort_index)
    });

    let pick = ranked[0];
    tracing::info!(
        target: "quilltap::fallback",
        failed_profile_id = %failed.id,
        failed_provider = %failed.provider,
        failed_model = %failed.model_name,
        picked_profile_id = %pick.id,
        picked_provider = %pick.provider,
        picked_model = %pick.model_name,
        different_provider = pick.provider.to_uppercase() != failed_provider,
        purpose = %context.purpose,
        eligible_count = eligible.len(),
        "Tier picker drafted a replacement"
    );

    Some(pick)
}
