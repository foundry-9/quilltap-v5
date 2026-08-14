//! The shared avatar prompt builder (v4 `lib/wardrobe/avatar-prompt.ts`,
//! REWORKED by `6b6e39ad` — the bare-top branch + collarbone-crop intro).
//!
//! Builds the head-and-shoulders portrait prompt used by the avatar-generation
//! job. Composes the ported wardrobe leaves
//! (`resolve_equipped_outfit_leaf_values` / `decorate_outfit_items_title_only` /
//! `describe_outfit_with_omit`) + the pronoun gender noun. Sync — the caller runs
//! it inside a DB read/write closure holding both connections.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::DbError;
use crate::jsstr::{js_trim_end, utf16_len, utf16_truncate};
use crate::pronoun_gender::gender_noun_from_pronouns;
use crate::tools::wardrobe_shared::resolve_equipped_outfit_leaf_values;
use crate::wardrobe::{
    decorate_outfit_items_title_only, describe_outfit_with_omit, OutfitSlotName, OutfitSlotValues,
    Slots,
};

/// Cap for the avatar aesthetic preamble (v4 `AVATAR_AESTHETIC_MAX_CHARS`).
const AVATAR_AESTHETIC_MAX_CHARS: usize = 600;

/// v4 `BuildPromptOptions`.
#[derive(Clone, Debug, Default)]
pub struct AvatarPromptOptions {
    /// Equipped slots to describe; `None` → no outfit is appended (physical
    /// descriptions alone).
    pub equipped_slots: Option<Slots>,
    /// Project document stores in scope (so project-store items resolve). The
    /// character's group stores are resolved from their own memberships inside
    /// the builder; Quilltap General and the character's vault are always in
    /// scope. Empty when there is no project context.
    pub project_mount_point_ids: Vec<String>,
    /// Resolved Aurora character aesthetic — prepended as a capped art-direction
    /// preamble (avatars have no LLM rewrite step). The Ariel Clause does NOT
    /// apply to avatars.
    pub character_aesthetic: Option<String>,
}

/// v4 `BuildPromptResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct AvatarPromptResult {
    pub prompt: String,
    pub has_appearance: bool,
    pub leaf_counts: LeafCounts,
}

/// Per-slot leaf counts after composite expansion (for logging/debug).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LeafCounts {
    pub top: usize,
    pub bottom: usize,
    pub footwear: usize,
    pub accessories: usize,
}

/// v4 `buildCharacterAvatarPrompt`. `character` is the vault-overlaid character
/// JSON (needs `id`, `name`, `pronouns`, `physicalDescription`).
pub fn build_character_avatar_prompt(
    main: &Connection,
    mount: &Connection,
    docs: &DocMountDocumentsRepository,
    character: &Value,
    options: &AvatarPromptOptions,
) -> Result<AvatarPromptResult, DbError> {
    let character_id = character
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let character_name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let mut leaf_counts = LeafCounts::default();

    // Physical description — fall back through the canonical fields the avatar
    // handler has always favored (`headAndShouldersPrompt || mediumPrompt ||
    // shortPrompt || longPrompt || completePrompt || fullDescription`).
    let physical_text = character
        .get("physicalDescription")
        .filter(|d| !d.is_null())
        .map(|desc| {
            const TIERS: [&str; 6] = [
                "headAndShouldersPrompt",
                "mediumPrompt",
                "shortPrompt",
                "longPrompt",
                "completePrompt",
                "fullDescription",
            ];
            let chosen = TIERS
                .iter()
                .find_map(|k| {
                    desc.get(*k)
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("");
            crate::jsstr::js_trim(chosen).to_string()
        })
        .unwrap_or_default();

    let mut outfit_text = String::new();
    // Whether the character's upper body is bare (no item bubbles up into the
    // top slot). Drives a tighter crop below so a bare chest is never in frame.
    let mut top_is_bare = false;
    if let Some(slots) = options.equipped_slots.as_ref() {
        // Pass the FULL equipped slots in so the resolver routes coverage by each
        // leaf's own `types`; render-time `omit` drops bottom/footwear so the
        // generator doesn't paste shoes/pants onto a cropped torso.
        let resolved = resolve_equipped_outfit_leaf_values(
            main,
            docs,
            character_id,
            slots,
            &crate::wardrobe_tiers::shared_wardrobe_tiers_for_character(
                main,
                mount,
                character_id,
                &options.project_mount_point_ids,
            ),
        )?;

        top_is_bare = resolved.top.is_empty();
        let accessories = decorate_outfit_items_title_only(&resolved.accessories);

        if top_is_bare {
            // Bare-topped character. Deliberately do NOT emit "topless"/"naked"
            // wardrobe language: it trips SFW image-provider moderation. The
            // tighter collarbone crop in the intro conveys the exposure; here we
            // only list accessories at or above the collar. Also avoid
            // describeOutfit's "completely naked and unadorned" fallback (which
            // would fire, reintroducing nudity language, when accessories are
            // empty too) — hence the explicit empty-accessories `''` branch.
            outfit_text = if accessories.is_empty() {
                String::new()
            } else {
                let slots = OutfitSlotValues {
                    top: Vec::new(),
                    bottom: Vec::new(),
                    footwear: Vec::new(),
                    accessories,
                };
                js_trim_end(&describe_outfit_with_omit(
                    &slots,
                    &[
                        OutfitSlotName::Top,
                        OutfitSlotName::Bottom,
                        OutfitSlotName::Footwear,
                    ],
                ))
                .to_string()
            };
        } else {
            let slots = OutfitSlotValues {
                top: decorate_outfit_items_title_only(&resolved.top),
                bottom: decorate_outfit_items_title_only(&resolved.bottom),
                footwear: decorate_outfit_items_title_only(&resolved.footwear),
                accessories,
            };
            outfit_text = js_trim_end(&describe_outfit_with_omit(
                &slots,
                &[OutfitSlotName::Bottom, OutfitSlotName::Footwear],
            ))
            .to_string();
        }

        leaf_counts.top = resolved.top.len();
        leaf_counts.bottom = resolved.bottom.len();
        leaf_counts.footwear = resolved.footwear.len();
        leaf_counts.accessories = resolved.accessories.len();
    }

    let has_appearance = !physical_text.is_empty() || !outfit_text.is_empty();
    let mut prompt = String::new();
    if has_appearance {
        // Anchor the figure's apparent sex from the character's pronouns.
        // `they`/neopronouns/unset → "person" (no forced binary presentation).
        let subject_noun =
            gender_noun_from_pronouns(pronoun_subject(character)).unwrap_or("person");
        // Bare-topped → crop higher (collarbone) so the chest is out of frame.
        let intro = if top_is_bare {
            format!(
                "Solo portrait of a single {subject_noun}: {character_name}. Show exactly one figure. Close-up headshot cropped at the collarbone — only the face, neck, and bare shoulders are visible; the chest and torso are outside the frame."
            )
        } else {
            format!(
                "Solo portrait of a single {subject_noun}: {character_name}. Show exactly one figure, head-and-shoulders crop, three-quarter view."
            )
        };
        let outro = "Character portrait, detailed, high quality, natural lighting. Only one person in the image.";
        // Strip trailing terminal punctuation off the physical description so we
        // don't end up with "background.." once we re-append a period.
        let phys_block = if physical_text.is_empty() {
            String::new()
        } else {
            format!("{}.", strip_trailing_terminal_punct(&physical_text))
        };
        // Outfit is a markdown list; a blank line before the first list item is
        // required, so it's separated from neighbors by `\n\n` on each side.
        let outfit_block = if outfit_text.is_empty() {
            " ".to_string()
        } else {
            format!("\n\n{outfit_text}\n\n")
        };
        prompt = if phys_block.is_empty() {
            format!("{intro}{outfit_block}{outro}")
        } else {
            format!("{intro} {phys_block}{outfit_block}{outro}")
        };

        // Prepend the Aurora character aesthetic as a capped art-direction
        // preamble (no LLM compresses this path).
        if let Some(aesthetic) = options
            .character_aesthetic
            .as_deref()
            .map(crate::jsstr::js_trim)
            .filter(|s| !s.is_empty())
        {
            let capped = if utf16_len(aesthetic) > AVATAR_AESTHETIC_MAX_CHARS {
                utf16_truncate(aesthetic, AVATAR_AESTHETIC_MAX_CHARS)
            } else {
                aesthetic.to_string()
            };
            prompt = format!("Art direction (apply this overall style): {capped}\n\n{prompt}");
        }
    }

    Ok(AvatarPromptResult {
        prompt,
        has_appearance,
        leaf_counts,
    })
}

/// The character's subject pronoun (v4 `character.pronouns.subject`), or `None`.
fn pronoun_subject(character: &Value) -> Option<&str> {
    character
        .get("pronouns")
        .and_then(|p| p.get("subject"))
        .and_then(Value::as_str)
}

/// v4 `text.replace(/[.!?]+$/, '')` — strip a trailing run of `.` / `!` / `?`.
fn strip_trailing_terminal_punct(text: &str) -> &str {
    text.trim_end_matches(['.', '!', '?'])
}
