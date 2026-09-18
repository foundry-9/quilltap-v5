//! The one list of DALL·E image styles.
//!
//! v4 spells `z.enum(['vivid', 'natural'])` out at each of its three validating
//! sites (`lib/tools/image-generation-tool.ts:52`,
//! `app/api/v1/images/route.ts`'s `generateImageSchema`, and
//! `app/api/v1/image-profiles/[id]/route.ts:27`) — there is no v4 module to
//! port. v5 had copied all three; P4.96 needed a FOURTH for the profile-id
//! generate route and consolidated instead, on the
//! [`crate::image_gen::quality`] precedent: the value is never interpreted
//! here, only membership-checked and forwarded, so one list serves every gate.
//!
//! The provider-options *label* list (`model::openai_image_options`) is a UI
//! surface with its own `helpText` per row and stays separate — it renders
//! choices, it does not validate them.

/// v4's `['vivid', 'natural']`, in v4's order.
pub const IMAGE_STYLE_VALUES: [&str; 2] = ["vivid", "natural"];

/// v4 `z.enum(['vivid','natural']).safeParse(value).success` — membership,
/// nothing more.
pub fn is_image_style(value: &str) -> bool {
    IMAGE_STYLE_VALUES.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_v4s_order_and_membership() {
        assert_eq!(IMAGE_STYLE_VALUES, ["vivid", "natural"]);
        for s in IMAGE_STYLE_VALUES {
            assert!(is_image_style(s), "{s}");
        }
        for s in ["", "Vivid", "sketch", "NATURAL", "vivid natural"] {
            assert!(!is_image_style(s), "{s}");
        }
    }
}
