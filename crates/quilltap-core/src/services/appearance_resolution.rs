//! Context-aware character appearance resolution for image generation (v4
//! `lib/image-gen/appearance-resolution.ts`). Determines what each depicted
//! character currently looks like and is wearing by analysing chat context, and
//! runs the Concierge appearance-sanitization gate.
//!
//! Two entry points:
//!   - [`resolve_character_appearances`] — the sceneState fast path, the trivial
//!     skip, the cheap-LLM resolution (with the dangerous-chat uncensored
//!     upgrade applied by the caller), and the default fallbacks.
//!   - [`sanitize_appearances_if_needed`] — the FIVE-step sanitize gate IN ORDER.
//!
//! Composes the ported [`image_scene_tasks`] (`resolve_appearance` /
//! `sanitize_appearance`) + the W4.2 gatekeeper [`classify_content`] over the
//! injected model seams.

use crate::cheap_llm::CheapLlmSelection;
use crate::db::chat_settings::DangerousContentSettings;
use crate::model::completion::CompletionProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::gatekeeper::{classify_content, ModerationProvider};
use crate::services::image_scene_tasks::{
    resolve_appearance, sanitize_appearance, AppearanceText, CharacterAppearanceInput, ChatMessage,
    EquippedWardrobeItem, PhysicalDescriptionInput,
};
use crate::wardrobe::{describe_outfit, OutfitSlotValues};

/// A physical description (the tiers the resolver + default builder read; v4
/// `PhysicalDescription`). `id`/`name` label the selection; the four prompt
/// tiers feed the default fallback.
#[derive(Clone, Debug, Default)]
pub struct PhysicalDescription {
    pub id: String,
    pub name: String,
    pub usage_context: Option<String>,
    pub short_prompt: Option<String>,
    pub medium_prompt: Option<String>,
    pub long_prompt: Option<String>,
    pub complete_prompt: Option<String>,
}

/// One equipped wardrobe item feeding appearance resolution (v4's per-slot leaf).
#[derive(Clone, Debug, Default)]
pub struct WardrobeItemInput {
    pub slot: String,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
}

/// Input for the appearance-resolution pipeline (v4 `AppearanceResolutionInput`).
#[derive(Clone, Debug, Default)]
pub struct AppearanceResolutionInput {
    pub character_id: String,
    pub character_name: String,
    pub physical_description: Option<PhysicalDescription>,
    pub equipped_wardrobe_items: Vec<WardrobeItemInput>,
}

/// A fully resolved character appearance ready for image generation (v4
/// `ResolvedCharacterAppearance`).
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedCharacterAppearance {
    pub character_id: String,
    pub character_name: String,
    pub physical_description: String,
    pub physical_description_name: String,
    pub clothing_description: String,
    /// `"narrative" | "stored" | "default"`.
    pub clothing_source: String,
    pub was_sanitized: bool,
}

/// The resolution result (v4 `AppearanceResolutionResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceResolutionResult {
    pub appearances: Vec<ResolvedCharacterAppearance>,
    /// Whether the LLM successfully resolved (false = used defaults).
    pub llm_resolved: bool,
}

/// One scene-state character (v4's `sceneState.characters[]`).
#[derive(Clone, Debug)]
pub struct SceneStateCharacter {
    pub character_id: String,
    pub character_name: String,
    pub appearance: Option<String>,
    pub clothing: Option<String>,
}

/// v4 `canSkipResolution`: skip the LLM when there are no messages AND every
/// character has ≤1 equipped item.
fn can_skip_resolution(
    characters: &[AppearanceResolutionInput],
    recent_messages: &[ChatMessage],
) -> bool {
    if !recent_messages.is_empty() {
        return false;
    }
    characters
        .iter()
        .all(|c| c.equipped_wardrobe_items.len() <= 1)
}

/// v4 `wardrobeItemsToSlotValues` (the local copy in the default builder).
fn wardrobe_items_to_slot_values(items: &[WardrobeItemInput]) -> OutfitSlotValues {
    let values_for = |slot: &str| -> Vec<String> {
        items
            .iter()
            .filter(|i| i.slot == slot)
            .map(|i| match i.image_prompt.as_deref().map(str::trim) {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => i.title.clone(),
            })
            .collect()
    };
    OutfitSlotValues {
        top: values_for("top"),
        bottom: values_for("bottom"),
        footwear: values_for("footwear"),
        accessories: values_for("accessories"),
    }
}

/// v4's `physDesc` selection: `complete || long || medium || short || name`.
fn select_phys_desc(primary: Option<&PhysicalDescription>, character_name: &str) -> String {
    let non_empty = |v: &Option<String>| v.as_deref().filter(|s| !s.is_empty()).map(str::to_string);
    primary
        .and_then(|p| {
            non_empty(&p.complete_prompt)
                .or_else(|| non_empty(&p.long_prompt))
                .or_else(|| non_empty(&p.medium_prompt))
                .or_else(|| non_empty(&p.short_prompt))
        })
        .unwrap_or_else(|| character_name.to_string())
}

/// v4 `buildDefaultAppearances`.
fn build_default_appearances(
    characters: &[AppearanceResolutionInput],
) -> Vec<ResolvedCharacterAppearance> {
    characters
        .iter()
        .map(|char| {
            let primary = char.physical_description.as_ref();
            let phys_desc = select_phys_desc(primary, &char.character_name);
            let has_wardrobe = !char.equipped_wardrobe_items.is_empty();
            ResolvedCharacterAppearance {
                character_id: char.character_id.clone(),
                character_name: char.character_name.clone(),
                physical_description: phys_desc,
                physical_description_name: primary
                    .map(|p| p.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "default".to_string()),
                clothing_description: if has_wardrobe {
                    describe_outfit(&wardrobe_items_to_slot_values(
                        &char.equipped_wardrobe_items,
                    ))
                } else {
                    String::new()
                },
                clothing_source: if has_wardrobe {
                    "stored".to_string()
                } else {
                    "default".to_string()
                },
                was_sanitized: false,
            }
        })
        .collect()
}

/// v4 `mapResolutionResults`: fold the LLM items back into full appearances.
fn map_resolution_results(
    characters: &[AppearanceResolutionInput],
    items: &[crate::services::image_scene_tasks::AppearanceResolutionItem],
) -> Vec<ResolvedCharacterAppearance> {
    characters
        .iter()
        .map(|char| {
            let resolved = items.iter().find(|i| i.character_id == char.character_id);
            let selected = char.physical_description.as_ref();
            let phys_desc = select_phys_desc(selected, &char.character_name);
            ResolvedCharacterAppearance {
                character_id: char.character_id.clone(),
                character_name: char.character_name.clone(),
                physical_description: phys_desc,
                physical_description_name: selected
                    .map(|p| p.name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "default".to_string()),
                clothing_description: resolved
                    .map(|r| r.clothing_description.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_default(),
                clothing_source: resolved
                    .map(|r| r.clothing_source.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "default".to_string()),
                was_sanitized: false,
            }
        })
        .collect()
}

/// v4 `resolveCharacterAppearances`. `scene_state` takes precedence (fast path,
/// no LLM); then the trivial-case skip; then the cheap-LLM resolution.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_character_appearances<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    characters: &[AppearanceResolutionInput],
    recent_messages: &[ChatMessage],
    image_prompt: &str,
    selection: &CheapLlmSelection,
    user_id: &str,
    chat_id: Option<&str>,
    scene_state: Option<&[SceneStateCharacter]>,
) -> AppearanceResolutionResult {
    if characters.is_empty() {
        return AppearanceResolutionResult {
            appearances: Vec::new(),
            llm_resolved: true,
        };
    }

    // Fast path: use scene state appearances if provided (avoids the LLM call).
    if let Some(scene) = scene_state.filter(|s| !s.is_empty()) {
        let appearances = characters
            .iter()
            .map(|char| {
                let scene_char = scene.iter().find(|sc| sc.character_id == char.character_id);
                let primary = char.physical_description.as_ref();
                let phys_desc = scene_char
                    .and_then(|sc| sc.appearance.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| select_phys_desc(primary, &char.character_name));
                ResolvedCharacterAppearance {
                    character_id: char.character_id.clone(),
                    character_name: char.character_name.clone(),
                    physical_description: phys_desc,
                    physical_description_name: primary
                        .map(|p| p.name.clone())
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| "scene-state".to_string()),
                    // If scene state has clothing info, use it. Explicit null/empty
                    // (undressed) does NOT fall back. Character absent → empty.
                    clothing_description: scene_char
                        .map(|sc| sc.clothing.clone().unwrap_or_default())
                        .unwrap_or_default(),
                    clothing_source: if scene_char.is_some() {
                        "narrative".to_string()
                    } else {
                        "default".to_string()
                    },
                    was_sanitized: false,
                }
            })
            .collect();
        return AppearanceResolutionResult {
            appearances,
            llm_resolved: true,
        };
    }

    // Trivial-case skip.
    if can_skip_resolution(characters, recent_messages) {
        return AppearanceResolutionResult {
            appearances: build_default_appearances(characters),
            llm_resolved: true,
        };
    }

    // Build the LLM input.
    let llm_input: Vec<CharacterAppearanceInput> = characters
        .iter()
        .map(|char| CharacterAppearanceInput {
            character_id: char.character_id.clone(),
            character_name: char.character_name.clone(),
            physical_descriptions: char
                .physical_description
                .as_ref()
                .map(|pd| {
                    vec![PhysicalDescriptionInput {
                        id: pd.id.clone(),
                        name: pd.name.clone(),
                        usage_context: pd.usage_context.clone(),
                        short_prompt: pd.short_prompt.clone(),
                        medium_prompt: pd.medium_prompt.clone(),
                    }]
                })
                .unwrap_or_default(),
            equipped_wardrobe_items: char
                .equipped_wardrobe_items
                .iter()
                .map(|w| EquippedWardrobeItem {
                    slot: w.slot.clone(),
                    title: w.title.clone(),
                    description: w.description.clone(),
                    image_prompt: w.image_prompt.clone(),
                })
                .collect(),
        })
        .collect();

    let result = resolve_appearance(
        executor,
        completion,
        &llm_input,
        recent_messages,
        image_prompt,
        selection,
        user_id,
        chat_id,
    )
    .await;

    match &result.result {
        Some(items) if result.success && !items.is_empty() => AppearanceResolutionResult {
            appearances: map_resolution_results(characters, items),
            llm_resolved: true,
        },
        _ => AppearanceResolutionResult {
            appearances: build_default_appearances(characters),
            llm_resolved: false,
        },
    }
}

/// v4 `sanitizeAppearancesIfNeeded` — the FIVE-step gate IN ORDER. Preserving
/// order is load-bearing: each early return short-circuits the more expensive
/// steps below it.
#[allow(clippy::too_many_arguments)]
pub async fn sanitize_appearances_if_needed<M, C>(
    executor: &CheapLlmTaskExecutor,
    moderation: &M,
    completion: &C,
    appearances: Vec<ResolvedCharacterAppearance>,
    danger_settings: &DangerousContentSettings,
    is_dangerous_chat: bool,
    has_uncensored_image_provider: bool,
    selection: &CheapLlmSelection,
    user_id: &str,
    chat_id: Option<&str>,
) -> Vec<ResolvedCharacterAppearance>
where
    M: ModerationProvider,
    C: CompletionProvider,
{
    // 1. The Concierge off → pass through.
    if danger_settings.mode == "OFF" {
        return appearances;
    }

    // 2. Dangerous chat with uncensored provider → accurate appearances are fine.
    if is_dangerous_chat && has_uncensored_image_provider {
        return appearances;
    }

    // 3. Classify the concatenated appearance text.
    let combined_text = appearances
        .iter()
        .map(|a| format!("{} {}", a.physical_description, a.clothing_description))
        .collect::<Vec<_>>()
        .join(" | ");

    let classification = classify_content(
        moderation,
        completion,
        &combined_text,
        selection,
        user_id,
        danger_settings,
        chat_id,
    )
    .await;

    // Not dangerous → pass through.
    if !classification.is_dangerous {
        return appearances;
    }

    // 4. Dangerous but uncensored provider available → will be routed there.
    if has_uncensored_image_provider {
        return appearances;
    }

    // 5. Dangerous with NO uncensored provider → sanitize.
    let to_sanitize: Vec<AppearanceText> = appearances
        .iter()
        .map(|a| AppearanceText {
            character_id: a.character_id.clone(),
            appearance_text: crate::jsstr::js_trim(&format!(
                "{}. {}",
                a.physical_description, a.clothing_description
            ))
            .to_string(),
        })
        .collect();

    let sanitize_result = sanitize_appearance(
        executor,
        completion,
        &to_sanitize,
        selection,
        user_id,
        chat_id,
    )
    .await;

    let Some(sanitized_items) = sanitize_result.result.filter(|_| sanitize_result.success) else {
        // Sanitization failed → pass through originals.
        return appearances;
    };

    // Merge sanitized text back in (v4's per-character diff check).
    appearances
        .into_iter()
        .map(|appearance| {
            let combined = crate::jsstr::js_trim(&format!(
                "{}. {}",
                appearance.physical_description, appearance.clothing_description
            ))
            .to_string();
            let sanitized = sanitized_items
                .iter()
                .find(|s| s.character_id == appearance.character_id);
            match sanitized {
                Some(s) if s.appearance_text != combined => ResolvedCharacterAppearance {
                    physical_description: s.appearance_text.clone(),
                    clothing_description: String::new(),
                    was_sanitized: true,
                    ..appearance
                },
                _ => appearance,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char_with_items(n: usize) -> AppearanceResolutionInput {
        AppearanceResolutionInput {
            character_id: "c1".into(),
            character_name: "Aurora".into(),
            physical_description: Some(PhysicalDescription {
                name: "primary".into(),
                complete_prompt: Some("tall with silver hair".into()),
                ..Default::default()
            }),
            equipped_wardrobe_items: (0..n)
                .map(|i| WardrobeItemInput {
                    slot: "top".into(),
                    title: format!("item{i}"),
                    ..Default::default()
                })
                .collect(),
        }
    }

    #[test]
    fn skip_when_trivial() {
        assert!(can_skip_resolution(&[char_with_items(1)], &[]));
        assert!(!can_skip_resolution(&[char_with_items(2)], &[]));
        assert!(!can_skip_resolution(
            &[char_with_items(0)],
            &[ChatMessage {
                role: "user".into(),
                content: "hi".into()
            }]
        ));
    }

    #[test]
    fn default_appearances_pick_best_tier_and_outfit() {
        let out = build_default_appearances(&[char_with_items(2)]);
        assert_eq!(out[0].physical_description, "tall with silver hair");
        assert_eq!(out[0].clothing_source, "stored");
        assert!(!out[0].clothing_description.is_empty());
    }
}
