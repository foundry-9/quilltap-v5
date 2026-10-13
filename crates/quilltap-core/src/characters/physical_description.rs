//! Which of a character's physical-description variants a reader uses — v4
//! `lib/characters/physical-description.ts` (`9ecebf986`, P4.D283).
//!
//! A character's appearance is stored as one record with several lengths of
//! the same truth — a head-and-shoulders prompt, short / medium / long /
//! complete prompts, and the free-prose full description. Each reader wants a
//! different one first and falls back through the rest in its own order. Those
//! orders used to be spelled inline at every reader (ten chains in v5, each a
//! JS `||` chain), and drifted — two of the Lantern's chains were SHORTER than
//! the `scene` order, so a character with only a `completePrompt` was
//! enumerated by NAME. This is the one place the orders live.
//!
//! Two semantics every caller inherits from v4's `desc[key]?.trim()`:
//!   - the WINNER IS TRIMMED (JS `String.prototype.trim`, `jsstr::js_trim`);
//!   - a WHITESPACE-ONLY variant is SKIPPED (the pre-span `||` chains took it
//!     as truthy, then some trimmed it to nothing).
//!
//! A non-string variant would throw at v4's `.trim()`; v4's Zod shapes make
//! every variant a string, so this port reads a non-string as absent (the
//! unreachable arm, recorded).
//!
//! Pure and client-safe.

use serde_json::Value;

use crate::jsstr::js_trim;

/// The variant fields a reader may choose between (v4
/// `PhysicalDescriptionVariants`'s keys, in v4's spelling).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VariantKey {
    HeadAndShouldersPrompt,
    ShortPrompt,
    MediumPrompt,
    LongPrompt,
    CompletePrompt,
    FullDescription,
}

impl VariantKey {
    /// The JSON key v4 reads (`physicalDescription.<key>`).
    pub fn as_str(self) -> &'static str {
        match self {
            VariantKey::HeadAndShouldersPrompt => "headAndShouldersPrompt",
            VariantKey::ShortPrompt => "shortPrompt",
            VariantKey::MediumPrompt => "mediumPrompt",
            VariantKey::LongPrompt => "longPrompt",
            VariantKey::CompletePrompt => "completePrompt",
            VariantKey::FullDescription => "fullDescription",
        }
    }
}

/// What the text is for, which decides the order variants are tried in (v4
/// `PhysicalDescriptionProfile`).
///
/// - `HeadAndShoulders` — a portrait crop (the avatar, a hairstyle's
///   picture). The dedicated prompt first: it never sends below-the-crop
///   anatomy that image-provider moderation rejects.
/// - `FullLength` — one figure, head to toe (a worn wardrobe picture, a
///   character's appearance in a resolved scene). The fullest prompt first.
/// - `Scene` — one of several figures in a scene prompt, or a one-line
///   preview. Compact first, so a cast does not swamp the prompt. Never the
///   head-and-shoulders prompt, which is a crop rather than a figure — this is
///   also the seed the head-and-shoulders backfill grounds a new one in.
/// - `SelfImage` — the character's own system prompt ("This is how you
///   look"). Shortest first: it rides along on every turn.
/// - `Fullest` — everything the operator wrote, for a reader that wants the
///   whole account (the external prompt generator).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicalDescriptionProfile {
    HeadAndShoulders,
    FullLength,
    Scene,
    SelfImage,
    Fullest,
}

impl PhysicalDescriptionProfile {
    /// v4's string tag.
    pub fn as_str(self) -> &'static str {
        match self {
            PhysicalDescriptionProfile::HeadAndShoulders => "head-and-shoulders",
            PhysicalDescriptionProfile::FullLength => "full-length",
            PhysicalDescriptionProfile::Scene => "scene",
            PhysicalDescriptionProfile::SelfImage => "self-image",
            PhysicalDescriptionProfile::Fullest => "fullest",
        }
    }

    /// v4's string tag → the profile (the harness drives the corpus by tag).
    pub fn parse(tag: &str) -> Option<Self> {
        Some(match tag {
            "head-and-shoulders" => PhysicalDescriptionProfile::HeadAndShoulders,
            "full-length" => PhysicalDescriptionProfile::FullLength,
            "scene" => PhysicalDescriptionProfile::Scene,
            "self-image" => PhysicalDescriptionProfile::SelfImage,
            "fullest" => PhysicalDescriptionProfile::Fullest,
            _ => return None,
        })
    }

    /// v4 `ORDER[profile]` — the variants tried, in order. VERBATIM from
    /// `physical-description.ts:51-57`.
    pub fn order(self) -> &'static [VariantKey] {
        use VariantKey::*;
        match self {
            PhysicalDescriptionProfile::HeadAndShoulders => &[
                HeadAndShouldersPrompt,
                MediumPrompt,
                ShortPrompt,
                LongPrompt,
                CompletePrompt,
                FullDescription,
            ],
            PhysicalDescriptionProfile::FullLength => &[
                CompletePrompt,
                LongPrompt,
                MediumPrompt,
                ShortPrompt,
                FullDescription,
                HeadAndShouldersPrompt,
            ],
            PhysicalDescriptionProfile::Scene => &[
                MediumPrompt,
                ShortPrompt,
                LongPrompt,
                CompletePrompt,
                FullDescription,
            ],
            PhysicalDescriptionProfile::SelfImage => &[
                ShortPrompt,
                MediumPrompt,
                LongPrompt,
                CompletePrompt,
                FullDescription,
            ],
            PhysicalDescriptionProfile::Fullest => &[
                FullDescription,
                CompletePrompt,
                LongPrompt,
                MediumPrompt,
                ShortPrompt,
            ],
        }
    }
}

/// A borrowed view of the six variants (v4 `PhysicalDescriptionVariants`) —
/// the typed `PhysicalDescription` structs around the crate build one of these
/// so every reader goes through the same pick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicalDescriptionVariants<'a> {
    pub head_and_shoulders_prompt: Option<&'a str>,
    pub short_prompt: Option<&'a str>,
    pub medium_prompt: Option<&'a str>,
    pub long_prompt: Option<&'a str>,
    pub complete_prompt: Option<&'a str>,
    pub full_description: Option<&'a str>,
}

impl<'a> PhysicalDescriptionVariants<'a> {
    /// The six keys off a `physicalDescription` JSON object. A non-object
    /// value (v4 `desc[key]` on a primitive → `undefined`) and a non-string
    /// variant both read as absent.
    pub fn from_value(desc: &'a Value) -> Self {
        let s = |k: VariantKey| desc.get(k.as_str()).and_then(Value::as_str);
        PhysicalDescriptionVariants {
            head_and_shoulders_prompt: s(VariantKey::HeadAndShouldersPrompt),
            short_prompt: s(VariantKey::ShortPrompt),
            medium_prompt: s(VariantKey::MediumPrompt),
            long_prompt: s(VariantKey::LongPrompt),
            complete_prompt: s(VariantKey::CompletePrompt),
            full_description: s(VariantKey::FullDescription),
        }
    }

    fn get(&self, key: VariantKey) -> Option<&'a str> {
        match key {
            VariantKey::HeadAndShouldersPrompt => self.head_and_shoulders_prompt,
            VariantKey::ShortPrompt => self.short_prompt,
            VariantKey::MediumPrompt => self.medium_prompt,
            VariantKey::LongPrompt => self.long_prompt,
            VariantKey::CompletePrompt => self.complete_prompt,
            VariantKey::FullDescription => self.full_description,
        }
    }
}

/// v4 `pickPhysicalDescription(desc, profile)` — the first non-blank variant
/// in the profile's order, TRIMMED; `""` when the character has none (or no
/// description at all). Callers supply their own fallback — usually the
/// character's name.
pub fn pick_physical_description<'a>(
    desc: Option<&PhysicalDescriptionVariants<'a>>,
    profile: PhysicalDescriptionProfile,
) -> &'a str {
    let Some(desc) = desc else {
        return "";
    };
    for key in profile.order() {
        // `desc[key]?.trim()` then `if (text)` — a whitespace-only variant
        // trims to '' and falls through.
        if let Some(text) = desc.get(*key).map(js_trim) {
            if !text.is_empty() {
                return text;
            }
        }
    }
    ""
}

/// `pick_physical_description` over a raw `physicalDescription` JSON value
/// (`null` / absent → `""`, as v4's `if (!desc) return ''`).
pub fn pick_physical_description_value(
    desc: Option<&Value>,
    profile: PhysicalDescriptionProfile,
) -> String {
    match desc.filter(|d| !d.is_null()) {
        Some(d) => {
            pick_physical_description(Some(&PhysicalDescriptionVariants::from_value(d)), profile)
                .to_string()
        }
        None => String::new(),
    }
}

/// v4 `hasPhysicalDescription(desc, profile)` — whether the character has any
/// variant `profile` would use.
pub fn has_physical_description(
    desc: Option<&PhysicalDescriptionVariants<'_>>,
    profile: PhysicalDescriptionProfile,
) -> bool {
    !pick_physical_description(desc, profile).is_empty()
}

/// `has_physical_description` over a raw `physicalDescription` JSON value.
pub fn has_physical_description_value(
    desc: Option<&Value>,
    profile: PhysicalDescriptionProfile,
) -> bool {
    !pick_physical_description_value(desc, profile).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn all() -> Value {
        json!({
            "headAndShouldersPrompt": "H&S",
            "shortPrompt": "SHORT",
            "mediumPrompt": "MEDIUM",
            "longPrompt": "LONG",
            "completePrompt": "COMPLETE",
            "fullDescription": "FULL",
        })
    }

    // v4 `physical-description.test.ts` — `%s leads with its own variant`.
    #[test]
    fn each_profile_leads_with_its_own_variant() {
        use PhysicalDescriptionProfile::*;
        for (profile, want) in [
            (HeadAndShoulders, "H&S"),
            (FullLength, "COMPLETE"),
            (Scene, "MEDIUM"),
            (SelfImage, "SHORT"),
            (Fullest, "FULL"),
        ] {
            assert_eq!(pick_physical_description_value(Some(&all()), profile), want);
        }
    }

    // v4 `falls through in order, skipping blanks and trimming`.
    #[test]
    fn falls_through_skipping_blanks_and_trimming() {
        let desc = json!({
            "headAndShouldersPrompt": "   ",
            "mediumPrompt": null,
            "shortPrompt": "  short text  ",
            "longPrompt": "LONG",
        });
        assert_eq!(
            pick_physical_description_value(
                Some(&desc),
                PhysicalDescriptionProfile::HeadAndShoulders
            ),
            "short text"
        );
        assert_eq!(
            pick_physical_description_value(Some(&desc), PhysicalDescriptionProfile::FullLength),
            "LONG"
        );
    }

    // v4 `never uses the head-and-shoulders crop for a scene figure`.
    #[test]
    fn scene_never_uses_the_crop() {
        let desc = json!({ "headAndShouldersPrompt": "H&S" });
        assert_eq!(
            pick_physical_description_value(Some(&desc), PhysicalDescriptionProfile::Scene),
            ""
        );
        assert!(!has_physical_description_value(
            Some(&desc),
            PhysicalDescriptionProfile::Scene
        ));
        assert_eq!(
            pick_physical_description_value(Some(&desc), PhysicalDescriptionProfile::FullLength),
            "H&S"
        );
    }

    // v4 `answers empty for no description at all`.
    #[test]
    fn empty_for_no_description() {
        assert_eq!(
            pick_physical_description_value(None, PhysicalDescriptionProfile::Scene),
            ""
        );
        assert_eq!(
            pick_physical_description_value(
                Some(&Value::Null),
                PhysicalDescriptionProfile::SelfImage
            ),
            ""
        );
        assert!(has_physical_description_value(
            Some(&json!({ "fullDescription": "prose" })),
            PhysicalDescriptionProfile::Scene
        ));
    }

    #[test]
    fn tags_round_trip() {
        for p in [
            PhysicalDescriptionProfile::HeadAndShoulders,
            PhysicalDescriptionProfile::FullLength,
            PhysicalDescriptionProfile::Scene,
            PhysicalDescriptionProfile::SelfImage,
            PhysicalDescriptionProfile::Fullest,
        ] {
            assert_eq!(PhysicalDescriptionProfile::parse(p.as_str()), Some(p));
        }
        assert_eq!(PhysicalDescriptionProfile::parse("portrait"), None);
    }
}
