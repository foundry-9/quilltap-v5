//! The one list of image quality tiers (v4 `d8d2890ee`,
//! `lib/image-gen/quality.ts`).
//!
//! Four places validate or offer a quality tier — the `generate_image` tool
//! schema, `POST /api/v1/images?action=generate`,
//! `POST /api/v1/image-profiles/[id]?action=generate`, and the profile editor
//! — and before v4's module existed the first three each spelled the union out
//! for themselves. Two of them were still carrying DALL·E's `standard | hd`
//! after the GPT Image tiers arrived, so a profile could store `max` and the
//! HTTP routes would reject it at the door.
//!
//! The host never interprets the value: it stores it, forwards it, and lets the
//! provider validate the subset its selected model actually supports (OpenAI's
//! table is [`crate::model::openai_image_models`]). So this list is
//! deliberately the *sum* of what the providers accept.
//!
//! v4 pins the list to `ImageGenParams['quality']` with a pair of compile-time
//! assertions. v5's `ImageGenParams.quality` is an untyped `Option<String>`
//! carrying whatever was stored, so there is no type to assert against;
//! `tool_definition_quality_enum_matches_the_shared_list` in
//! `tools::definitions` plays the same role against the surface that actually
//! constrains a model — the `generate_image` definition's own enum.

/// Every tier any provider accepts, in v4's order.
///
/// - `auto` / `low` / `medium` / `high` — the GPT Image spelling.
/// - `xhigh` / `max` — the premium tiers GPT Image 2.5 added.
/// - `standard` / `hd` — the DALL·E spelling.
pub const IMAGE_QUALITY_VALUES: [&str; 8] = [
    "auto", "low", "medium", "high", "xhigh", "max", "standard", "hd",
];

/// v4 `imageQualitySchema.safeParse(value).success` — membership, nothing more.
pub fn is_image_quality(value: &str) -> bool {
    IMAGE_QUALITY_VALUES.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_v4s_order_and_membership() {
        assert_eq!(
            IMAGE_QUALITY_VALUES,
            ["auto", "low", "medium", "high", "xhigh", "max", "standard", "hd"]
        );
        for q in IMAGE_QUALITY_VALUES {
            assert!(is_image_quality(q), "{q}");
        }
        assert!(!is_image_quality("ludicrous"));
        assert!(!is_image_quality(""));
        assert!(!is_image_quality("HD"));
    }

    /// Every tier the OpenAI capability table can name must be in the shared
    /// list — v4's `Assert<SharedQuality, ImageQuality>` direction, measured
    /// against the one table that enumerates them.
    #[test]
    fn every_openai_family_tier_is_in_the_shared_list() {
        for m in crate::model::openai_image_models::OPENAI_IMAGE_MODELS {
            for q in m.qualities {
                assert!(is_image_quality(q), "{} offers {q}", m.id);
            }
        }
    }
}
