//! The wardrobe item picture prompt — v4 `lib/wardrobe/item-image-prompt.ts`
//! (`7c8572869`, #82).
//!
//! Builds the image-generation prompt for one wardrobe item or outfit:
//!
//!   - **Worn** — a character-owned item is drawn on its owner: the same
//!     identity block the avatar portrait opens with
//!     ([`build_figure_identity_block`]), full length, the garment the point of
//!     the picture. A hair-slot item is framed head and shoulders instead.
//!   - **Catalogue** — a shared item (General, project, group), or one whose
//!     owner is gone or archived, is drawn alone: on a dress form or laid flat,
//!     no person.
//!
//! The cue per garment is `imagePrompt ?? title` (via
//! [`decorate_outfit_items_title_only`]), the rule every image pipeline
//! follows; the Markdown `description` never reaches a diffusion model. An
//! outfit's cue is its resolved leaves, phrased per slot through
//! `describeOutfit` — empty slots are omitted, so an ensemble without shoes is
//! never told it is barefoot.
//!
//! v4's builder awaits `resolveAesthetic({ kind: 'aurora',
//! projectOfficialMountPointId })` itself; here the caller resolves it (the
//! generation, over `services::aesthetics`) and passes the text in, so the
//! builder is pure.

use serde_json::Value;

use crate::image_gen::Orientation;
use crate::jsstr::{js_trim, js_trim_end, utf16_len, utf16_truncate};
use crate::services::avatar_prompt::{build_figure_identity_block, FigureFraming};
use crate::wardrobe::{
    build_outfit_slot_values, decorate_outfit_items_title_only, describe_outfit_with_omit,
    OutfitSlotName, WARDROBE_SLOT_TYPES,
};

/// Cap for the aesthetic preamble — matches the avatar portrait's (v4
/// `AESTHETIC_MAX_CHARS`, UTF-16 code units).
const AESTHETIC_MAX_CHARS: usize = 600;

/// v4 `WardrobeImageSubject`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WardrobeImageSubject {
    Worn,
    Catalogue,
}

impl WardrobeImageSubject {
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeImageSubject::Worn => "worn",
            WardrobeImageSubject::Catalogue => "catalogue",
        }
    }
}

/// v4 `WardrobeItemImagePrompt` — `orientation` is only ever `Portrait`
/// (worn) or `Square` (catalogue).
#[derive(Debug, Clone, PartialEq)]
pub struct WardrobeItemImagePrompt {
    pub prompt: String,
    pub orientation: Orientation,
    pub subject: WardrobeImageSubject,
}

fn item_types(item: &Value) -> Vec<&str> {
    item.get("types")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// v4 `primarySlot(item)` — the first canonical slot the item covers (where it
/// is listed in an outfit cue); `accessories` when none.
pub fn primary_slot(item: &Value) -> &'static str {
    let types = item_types(item);
    WARDROBE_SLOT_TYPES
        .iter()
        .find(|slot| types.contains(slot))
        .copied()
        .unwrap_or("accessories")
}

/// v4 `isHairOnly(items)` — true when every slot the items cover is hair (and
/// there is at least one item, each with at least one type).
pub fn is_hair_only(items: &[&Value]) -> bool {
    !items.is_empty()
        && items.iter().all(|i| {
            let types = item_types(i);
            !types.is_empty() && types.iter().all(|t| *t == "hair")
        })
}

fn slot_name(slot: &str) -> OutfitSlotName {
    match slot {
        "top" => OutfitSlotName::Top,
        "bottom" => OutfitSlotName::Bottom,
        "footwear" => OutfitSlotName::Footwear,
        "accessories" => OutfitSlotName::Accessories,
        _ => OutfitSlotName::Hair,
    }
}

/// v4 `buildWardrobeItemCue(item, components)` — one garment's `imagePrompt ??
/// title`, or an outfit's leaves listed per slot in canonical order (empty
/// slots omitted), right-trimmed.
pub fn build_wardrobe_item_cue(item: &Value, components: &[Value]) -> String {
    if components.is_empty() {
        return decorate_outfit_items_title_only(std::slice::from_ref(item))
            .into_iter()
            .next()
            .unwrap_or_default();
    }
    let by_slot = build_outfit_slot_values(|slot| {
        let in_slot: Vec<Value> = components
            .iter()
            .filter(|c| primary_slot(c) == slot)
            .cloned()
            .collect();
        decorate_outfit_items_title_only(&in_slot)
    });
    let omit: Vec<OutfitSlotName> = WARDROBE_SLOT_TYPES
        .iter()
        .filter(|slot| by_slot.slot(slot).is_empty())
        .map(|slot| slot_name(slot))
        .collect();
    js_trim_end(&describe_outfit_with_omit(&by_slot, &omit)).to_string()
}

/// v4 `buildWardrobeItemImagePrompt({ item, components, owner,
/// projectOfficialMountPointId })` — the prompt, orientation and subject for an
/// item's picture. `owner` is the wearer (a character JSON with `name`,
/// `physicalDescription`, `pronouns`, `archivedAt`); `None` or archived → a
/// catalogue shot. `aesthetic` is what v4's `resolveAesthetic({ kind: 'aurora',
/// … })` answered (untrimmed).
pub fn build_wardrobe_item_image_prompt(
    item: &Value,
    components: &[Value],
    owner: Option<&Value>,
    aesthetic: Option<&str>,
) -> WardrobeItemImagePrompt {
    let cue = build_wardrobe_item_cue(item, components);
    let is_outfit = !components.is_empty();
    // An outfit's cue is a Markdown list; a garment's is a phrase. Lists need
    // a blank line either side to stay lists.
    let cue_inline = if is_outfit {
        format!("the following ensemble:\n\n{cue}\n\n")
    } else {
        format!("{cue}. ")
    };
    let hair_only = if is_outfit {
        is_hair_only(&components.iter().collect::<Vec<_>>())
    } else {
        is_hair_only(&[item])
    };

    // v4 `owner && !owner.archivedAt` — a truthy `archivedAt` retires the
    // figure.
    let live_owner = owner.filter(|o| {
        o.get("archivedAt")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    });

    let (mut prompt, orientation, subject) = if let Some(owner) = live_owner {
        let figure = build_figure_identity_block(
            owner,
            if hair_only {
                FigureFraming::HeadAndShoulders
            } else {
                FigureFraming::FullLength
            },
        );
        let framing = if hair_only {
            "Head-and-shoulders portrait, facing the viewer, showing the hairstyle clearly"
        } else {
            "Full-length, standing, facing the viewer, head to toe in frame"
        };
        let name = owner.get("name").and_then(Value::as_str).unwrap_or("");
        let intro = format!(
            "Solo picture of a single {}: {name}. Show exactly one figure. {framing}.",
            figure.subject_noun
        );
        let phys_block = if figure.phys_block.is_empty() {
            String::new()
        } else {
            format!(" {}", figure.phys_block)
        };
        let wearing = if hair_only {
            format!("Wearing their hair as {cue_inline}")
        } else {
            format!("Wearing {cue_inline}")
        };
        let rest = if hair_only {
            "The hairstyle is the subject; neutral studio backdrop; even light."
        } else {
            "The rest of the attire plain and unremarkable so the garment is the subject; neutral studio backdrop; even light. Only one person in the image."
        };
        (
            format!("{intro}{phys_block} {wearing}{rest}"),
            Orientation::Portrait,
            WardrobeImageSubject::Worn,
        )
    } else {
        let display = if hair_only {
            "styled on a featureless mannequin head"
        } else if is_outfit {
            "arranged together on a dress form or laid flat"
        } else {
            "on a dress form or laid flat"
        };
        let prompt = if is_outfit {
            format!(
                "Product photograph of {cue_inline}Shown {display}, no person, neutral ground, even light. The clothing is the subject."
            )
        } else {
            format!(
                "Product photograph of {cue}, {display}, no person, neutral ground, even light. The garment is the subject."
            )
        };
        (prompt, Orientation::Square, WardrobeImageSubject::Catalogue)
    };

    if let Some(aesthetic) = aesthetic.map(js_trim).filter(|s| !s.is_empty()) {
        // v4 `aesthetic.slice(0, 600)` — UTF-16 code units, AFTER the trim.
        let capped = if utf16_len(aesthetic) > AESTHETIC_MAX_CHARS {
            utf16_truncate(aesthetic, AESTHETIC_MAX_CHARS)
        } else {
            aesthetic.to_string()
        };
        prompt = format!("Art direction (apply this overall style): {capped}\n\n{prompt}");
    }

    WardrobeItemImagePrompt {
        prompt: js_trim(&prompt).to_string(),
        orientation,
        subject,
    }
}
