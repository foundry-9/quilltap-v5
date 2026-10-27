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

use crate::characters::physical_description::{
    pick_physical_description, PhysicalDescriptionProfile, PhysicalDescriptionVariants,
};
use serde_json::Value;

use crate::cheap_llm::CheapLlmSelection;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::model::completion::CompletionProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::gatekeeper::{classify_content, ModerationProvider};
use crate::services::dangerous_content::resolver::ResolvedConciergePolicy;
use crate::services::image_scene_tasks::{
    resolve_appearance, sanitize_appearance, AppearanceSanitizeMode, AppearanceText,
    CharacterAppearanceInput, ChatMessage, EquippedWardrobeItem, PhysicalDescriptionInput,
};
use crate::wardrobe::{describe_outfit, OutfitSlotValues};

/// A physical description (the tiers the resolver + default builder read; v4
/// `PhysicalDescription`). `id`/`name` label the selection; the six variants
/// feed the `full-length` pick (`characters::physical_description`) — since
/// v4 `9ecebf986` that pick falls through `fullDescription` and then
/// `headAndShouldersPrompt` BEFORE the character's name, so both ride here.
#[derive(Clone, Debug, Default)]
pub struct PhysicalDescription {
    pub id: String,
    pub name: String,
    pub usage_context: Option<String>,
    pub short_prompt: Option<String>,
    pub medium_prompt: Option<String>,
    pub long_prompt: Option<String>,
    pub complete_prompt: Option<String>,
    pub full_description: Option<String>,
    pub head_and_shoulders_prompt: Option<String>,
}

impl PhysicalDescription {
    /// The six variants as the shared pick reads them.
    pub fn variants(&self) -> PhysicalDescriptionVariants<'_> {
        PhysicalDescriptionVariants {
            head_and_shoulders_prompt: self.head_and_shoulders_prompt.as_deref(),
            short_prompt: self.short_prompt.as_deref(),
            medium_prompt: self.medium_prompt.as_deref(),
            long_prompt: self.long_prompt.as_deref(),
            complete_prompt: self.complete_prompt.as_deref(),
            full_description: self.full_description.as_deref(),
        }
    }
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
    /// What the character carries in the chat (the Inventory's equipped
    /// objects' `imageDescription`s, e.g. "carrying a longsword with a jewelled
    /// pommel") — set by [`resolve_character_appearances`] whatever path
    /// resolved the rest, and rendered beside the outfit. `None` is v4's ABSENT
    /// key (`4fbdad186`): v4 writes it only when the phrase is non-null, and a
    /// sanitized text clears it.
    pub carrying_description: Option<String>,
    pub was_sanitized: bool,
    /// v4 `needsConcealment?` [97b25fc53]: set only by a `Conceal`-mode
    /// sanitize pass whose reply flagged the character undressed. v4 only ever
    /// writes `true` or leaves the key absent, so `false` here is "absent".
    /// Never persisted; only the story-background description builder reads it.
    pub needs_concealment: bool,
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

/// v4 `wardrobeItemsToSlotValues` (the local copy in the default builder;
/// built over v4's `buildOutfitSlotValues`, so new slots ride the registry).
fn wardrobe_items_to_slot_values(items: &[WardrobeItemInput]) -> OutfitSlotValues {
    crate::wardrobe::build_outfit_slot_values(|slot| {
        items
            .iter()
            .filter(|i| i.slot == slot)
            .map(|i| match i.image_prompt.as_deref().map(str::trim) {
                Some(p) if !p.is_empty() => p.to_string(),
                _ => i.title.clone(),
            })
            .collect()
    })
}

/// v4's `physDesc` selection (`9ecebf986`): `pickPhysicalDescription(primary,
/// 'full-length') || char.characterName` — complete ‖ long ‖ medium ‖ short ‖
/// fullDescription ‖ headAndShouldersPrompt, trimmed, whitespace-only skipped,
/// THEN the name (the pre-span chain stopped at `short`).
fn select_phys_desc(primary: Option<&PhysicalDescription>, character_name: &str) -> String {
    let variants = primary.map(PhysicalDescription::variants);
    let picked =
        pick_physical_description(variants.as_ref(), PhysicalDescriptionProfile::FullLength);
    if picked.is_empty() {
        character_name.to_string()
    } else {
        picked.to_string()
    }
}

/// v4 `equippedWardrobeItemsForAppearance(repos, chatId, characterId,
/// projectMountPointIds)` (`appearance-resolution.ts:100`, `9ecebf986`) — the
/// character's equipped slots in `chat_id`, resolved over the wearable pool
/// and flattened per slot in `WARDROBE_SLOT_TYPES` order. `None` when the
/// character has nothing equipped (no stored slots, or they resolve to no
/// leaf). The slots read is v4's fallback `safeQuery`, so a failed read is no
/// outfit.
pub fn equipped_wardrobe_items_for_appearance(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat_id: &str,
    character_id: &str,
    project_mount_point_ids: &[String],
) -> Option<Vec<WardrobeItemInput>> {
    let state = crate::db::chats_outfits::ChatOutfitsRepository::new(main)
        .get_equipped_outfit_for_character(chat_id, character_id)?;
    let slots = crate::wardrobe::Slots::from_value(Some(&state));
    // Group stores follow each character's own memberships, so they can't come
    // from the shared project list — the pool resolves them itself.
    let pool = crate::wearable_pool::load_wearable_pool(
        main,
        mount,
        character_id,
        Some(project_mount_point_ids),
        crate::wearable_pool::LoadWearablePoolOptions::default(),
    );
    let resolved =
        crate::services::resolve_equipped::resolve_equipped_outfit_for_character(&pool, &slots)
            .leaf_items_by_slot;
    let mut flat: Vec<WardrobeItemInput> = Vec::new();
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        for item in resolved.slot(slot) {
            flat.push(WardrobeItemInput {
                slot: slot.to_string(),
                title: item
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                image_prompt: item
                    .get("imagePrompt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
        }
    }
    (!flat.is_empty()).then_some(flat)
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
                carrying_description: None,
                was_sanitized: false,
                needs_concealment: false,
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
                carrying_description: None,
                was_sanitized: false,
                needs_concealment: false,
            }
        })
        .collect()
}

/// v4 `resolveCharacterAppearances` (`4fbdad186`): the appearances resolved
/// WITHOUT the Inventory ([`resolve_appearances_without_inventory`]), then —
/// in a chat, with anything resolved — what each character carries added by
/// [`with_carrying`]. The one place the image pipelines learn of the
/// Inventory.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_character_appearances<C: CompletionProvider>(
    db: &Db,
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
    let mut result = resolve_appearances_without_inventory(
        executor,
        completion,
        characters,
        recent_messages,
        image_prompt,
        selection,
        user_id,
        chat_id,
        scene_state,
    )
    .await;
    let Some(chat_id) = chat_id else {
        return result;
    };
    if result.appearances.is_empty() {
        return result;
    }
    result.appearances = with_carrying(db, std::mem::take(&mut result.appearances), chat_id).await;
    result
}

/// Add what each character carries in the chat (the Inventory) to its
/// resolved appearance (v4 `withCarrying`). Fails soft to the appearances as
/// they were.
///
/// The chat is read through v4's fallback `findById` (a failed read logs the
/// repository's line and answers "no chat" — as were, silently), and the
/// phrase loader never fails, so v4's `catch` (`Could not add carried objects
/// to appearances`) is unreachable through v4's real code. v5's home for it is
/// the two-connection read failing outright — a degraded mount index.
async fn with_carrying(
    db: &Db,
    appearances: Vec<ResolvedCharacterAppearance>,
    chat_id: &str,
) -> Vec<ResolvedCharacterAppearance> {
    let character_ids: Vec<String> = appearances.iter().map(|a| a.character_id.clone()).collect();
    let cid = chat_id.to_string();
    let phrases = crate::services::image_job_common::with_both_conns(db, move |main, mount| {
        let Some(chat) = crate::db::chats_read::find_by_id_or_none(main, &cid) else {
            return Ok(None);
        };
        Ok(Some(
            character_ids
                .iter()
                .map(|character_id| {
                    crate::inventory::image_description::load_carrying_phrase(
                        main,
                        mount,
                        &chat,
                        character_id,
                    )
                })
                .collect::<Vec<_>>(),
        ))
    })
    .await;
    match phrases {
        Ok(Some(phrases)) => appearances
            .into_iter()
            .zip(phrases)
            .map(|(mut appearance, carrying)| {
                if carrying.is_some() {
                    appearance.carrying_description = carrying;
                }
                appearance
            })
            .collect(),
        Ok(None) => appearances,
        Err(error) => {
            tracing::warn!(
                target: "quilltap::appearance_resolution",
                context = "image-gen.appearance-resolution",
                chat_id = chat_id,
                error = %crate::db::fallback::error_text(&error),
                "[AppearanceResolution] Could not add carried objects to appearances"
            );
            appearances
        }
    }
}

/// The appearances resolved without the Inventory (v4
/// `resolveAppearancesWithoutInventory` — the pre-`4fbdad186` body, renamed).
/// `scene_state` takes precedence (fast path, no LLM); then the trivial-case
/// skip; then the cheap-LLM resolution.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_appearances_without_inventory<C: CompletionProvider>(
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
                    carrying_description: None,
                    was_sanitized: false,
                    needs_concealment: false,
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

/// v4 `sanitizeAppearancesIfNeeded` — the gate IN ORDER. Preserving order is
/// load-bearing: each early return short-circuits the more expensive steps
/// below it.
///
/// Cleaning exists for one case only: a Moderated chat whose image is bound for
/// a moderated provider. Everywhere else the text goes through as written.
///
/// 1. The image is bound for an uncensored profile → return unchanged, whatever
///    the chat's state. That provider accepts it, so accuracy beats a draped
///    sheet, and no classifier call is spent finding that out.
/// 2. Unless the policy pre-screens (Moderated, on duty, pre-screen opted in) →
///    return unchanged. An Unmoderated chat bound for a moderated provider is
///    sent as written: a refusal is answered by the post-hoc failover, if an
///    understudy exists, not by rewriting the scene. Off duty and Locked keep
///    the Concierge's hands off as before.
/// 3. Classify the concatenated appearance text; safe → unchanged.
/// 4. Otherwise → sanitize via the cheap LLM.
///
/// `image_target_uncensored` is "the profile that will actually paint this is
/// uncensored" ([`is_uncensored_image_profile`] on the effective profile), never
/// "an uncensored profile is configured somewhere" (bug 133). Whether one
/// exists in settings and whether *this* image will be generated by it are
/// different questions:
///
/// - The image-generation tool classifies each prompt and reroutes on the spot
///   when failover is allowed, so it passes the profile it ended up on.
/// - Story backgrounds route up front only for an Unmoderated chat; otherwise
///   they paint on the chat's own image profile, uncensored or not.
///
/// (v4 `406dbdd5a` re-ruled bug 133: the port's `cc65d6bfc` shape took
/// `is_dangerous_chat` + `routes_dangerous_to_uncensored` and kept a fifth
/// "dangerous but routed uncensored" step; both are gone.)
///
/// [97b25fc53] `mode` — `Redress` swaps explicit states for neutral clothing;
/// `Conceal` keeps the state and sets `needs_concealment`. Only a caller whose
/// crafter carries the concealment guidance (story backgrounds) may pass
/// `Conceal` — anywhere else the flag would reach a moderated provider with
/// nothing downstream to act on it (v4's JSDoc).
///
/// [`is_uncensored_image_profile`]: crate::services::dangerous_content::understudy::is_uncensored_image_profile
#[allow(clippy::too_many_arguments)]
pub async fn sanitize_appearances_if_needed<M, C>(
    db: &Db,
    executor: &CheapLlmTaskExecutor,
    moderation: &M,
    completion: &C,
    appearances: Vec<ResolvedCharacterAppearance>,
    concierge_policy: &ResolvedConciergePolicy,
    image_target_uncensored: bool,
    selection: &CheapLlmSelection,
    user_id: &str,
    chat_id: Option<&str>,
    mode: AppearanceSanitizeMode,
) -> Vec<ResolvedCharacterAppearance>
where
    M: ModerationProvider,
    C: CompletionProvider,
{
    // 1. Bound for an uncensored painter → nothing to clean, in any chat state.
    if image_target_uncensored {
        tracing::debug!(
            target: "quilltap::appearance_resolution",
            context = "image-gen.appearance-resolution",
            chat_id = chat_id,
            concierge_state = concierge_policy.state.as_str(),
            "[AppearanceResolution] Image bound for an uncensored profile; appearances left as written"
        );
        return appearances;
    }

    // 2. Only a Moderated chat with the pre-screen on is cleaned. Unmoderated
    // goes out as written to whatever painter it has; off duty or Locked → the
    // Concierge keeps his hands off.
    if !concierge_policy.pre_screen.enabled {
        tracing::debug!(
            target: "quilltap::appearance_resolution",
            context = "image-gen.appearance-resolution",
            chat_id = chat_id,
            concierge_source = concierge_policy.source.as_str(),
            "[AppearanceResolution] Appearance sanitization not applicable under this Concierge policy"
        );
        return appearances;
    }

    // 3. Classify the concatenated appearance text — what each character
    // carries included, since it reaches the same moderated provider. v4's
    // `[phys, cloth, carry].filter(Boolean).join(' ')` (`4fbdad186`): an EMPTY
    // physical or clothing description drops out with its space, Inventory or
    // not (the old `${phys} ${cloth}` left a trailing space).
    let combined_text = appearances
        .iter()
        .map(|a| {
            [
                Some(a.physical_description.as_str()),
                Some(a.clothing_description.as_str()),
                a.carrying_description.as_deref(),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" | ");

    let classification = classify_content(
        db,
        moderation,
        completion,
        &combined_text,
        selection,
        user_id,
        concierge_policy,
        chat_id,
    )
    .await;

    // v4 wraps the classify in a `try` whose `catch` logs `Appearance
    // classification failed, passing through` — UNREACHABLE in v4 as well as
    // here (measured at `97b25fc53`, P4.D239): `classifyContent` is
    // `trackActivity('danger', runClassification)`, `runClassification` is a
    // total try/catch returning its safe fallback, and the activity
    // bookkeeping around it swallows its own `process.send` errors. v5's
    // `classify_content` returns a value, so there is no arm to port.

    // Not dangerous → pass through.
    if !classification.is_dangerous {
        return appearances;
    }

    // v4's rule-3 INFO (absent from the port until P4.D239). `categories` is
    // v4's `categories.map(c => c.category)` — an array, so it rides the file
    // layer's `…Json` convention (P4.91).
    let category_names: Vec<&str> = classification
        .categories
        .iter()
        .map(|c| c.category.as_str())
        .collect();
    let categories_json = serde_json::to_string(&category_names).unwrap_or_default();
    tracing::info!(
        target: "quilltap::appearance_resolution",
        context = "image-gen.appearance-resolution",
        chat_id = chat_id,
        score = classification.score,
        categoriesJson = %categories_json,
        "[AppearanceResolution] Appearance text classified as dangerous"
    );

    // 4. Dangerous and bound for a moderated provider → sanitize. v4's INFO
    // carries `mode` LAST (`97b25fc53`).
    tracing::info!(
        target: "quilltap::appearance_resolution",
        context = "image-gen.appearance-resolution",
        chat_id = chat_id,
        character_count = appearances.len(),
        mode = mode.as_str(),
        "[AppearanceResolution] Sanitizing dangerous appearance descriptions"
    );
    let to_sanitize: Vec<AppearanceText> = appearances
        .iter()
        .map(|a| AppearanceText {
            character_id: a.character_id.clone(),
            appearance_text: sanitizable_appearance_text(a),
        })
        .collect();

    let sanitize_result = sanitize_appearance(
        executor,
        completion,
        &to_sanitize,
        selection,
        user_id,
        chat_id,
        mode,
    )
    .await;

    let Some(sanitized_items) = sanitize_result.result.filter(|_| sanitize_result.success) else {
        tracing::warn!(
            target: "quilltap::appearance_resolution",
            context = "image-gen.appearance-resolution",
            chat_id = chat_id,
            error = sanitize_result.error.as_deref(),
            "[AppearanceResolution] Sanitization failed, passing through original"
        );
        return appearances;
    };

    // Merge sanitized text back in — v4's THREE-way merge (`97b25fc53`):
    //   - no match, or the text is unchanged and the character is not undressed
    //     → the appearance unchanged;
    //   - the text changed → the sanitized text replaces physical + clothing
    //     combined, `was_sanitized`;
    //   - undressed (conceal mode only) → `needs_concealment`, INDEPENDENTLY:
    //     an unchanged echo that is undressed KEEPS its clothing and stays
    //     `was_sanitized: false`.
    appearances
        .into_iter()
        .map(|appearance| {
            let Some(sanitized) = sanitized_items
                .iter()
                .find(|s| s.character_id == appearance.character_id)
            else {
                return appearance;
            };
            let text_changed =
                sanitized.appearance_text != sanitizable_appearance_text(&appearance);
            let undressed = sanitized.undressed == Some(true);
            if !text_changed && !undressed {
                return appearance;
            }
            let mut merged = appearance;
            if text_changed {
                // The sanitized text is physical + clothing + carrying combined;
                // the carrying phrase is inside it now, so it is not appended
                // again.
                merged.physical_description = sanitized.appearance_text.clone();
                merged.clothing_description = String::new();
                merged.carrying_description = None;
                merged.was_sanitized = true;
            }
            if undressed {
                merged.needs_concealment = true;
            }
            merged
        })
        .collect()
}

/// One character's appearance as the sanitizer sees it: physical, clothing and
/// (when anything carried is described) the carrying phrase, as sentences (v4
/// `sanitizableAppearanceText`, module-private there). An empty clothing gives
/// `phys.`, and a carrying phrase then `phys.. carrying` — v4's bytes.
pub fn sanitizable_appearance_text(a: &ResolvedCharacterAppearance) -> String {
    let base = js_trim(&format!(
        "{}. {}",
        a.physical_description, a.clothing_description
    ))
    .to_string();
    match a.carrying_description.as_deref().map(js_trim) {
        Some(carrying) if !carrying.is_empty() => {
            js_trim(&format!("{base}. {carrying}")).to_string()
        }
        _ => base,
    }
}

/// The Lantern's description of one resolved character: the (gender-prefixed)
/// physical description, what they wear, and what they carry — each a
/// sentence-fragment the caller joins with `. ` (v4 `sceneDescriptionParts`).
/// Pure.
///
/// The carrying phrase is capitalised as JS `charAt(0).toUpperCase()` does it:
/// ONE UTF-16 unit. A BMP first character upper-cases with Unicode's full
/// mapping (`ß` → `SS`); an astral first character is a lone high surrogate to
/// JS, which `toUpperCase` leaves alone, so the phrase is unchanged.
pub fn scene_description_parts(
    resolved: &ResolvedCharacterAppearance,
    gender_prefix: &str,
) -> Vec<String> {
    let mut parts = vec![format!("{gender_prefix}{}", resolved.physical_description)];
    if !resolved.clothing_description.is_empty() {
        parts.push(format!("Wearing: {}", resolved.clothing_description));
    }
    if let Some(carrying) = resolved
        .carrying_description
        .as_deref()
        .map(js_trim)
        .filter(|c| !c.is_empty())
    {
        parts.push(js_capitalize_first_unit(carrying));
    }
    parts
}

/// JS `s.charAt(0).toUpperCase() + s.slice(1)` — ONE UTF-16 unit upper-cased
/// (see [`scene_description_parts`]). Shared with the avatar portrait's
/// carrying sentence.
pub fn js_capitalize_first_unit(s: &str) -> String {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if first.len_utf16() > 1 {
        // An astral character: `charAt(0)` is a lone high surrogate, which
        // `toUpperCase` returns unchanged.
        return s.to_string();
    }
    let mut out: String = first.to_uppercase().collect();
    out.push_str(chars.as_str());
    out
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

    /// R-9 — v4's `Could not add carried objects to appearances` catch is
    /// UNREACHABLE through v4's real code (the chat read is a fallback read,
    /// the phrase loader never throws). v5's home is the two-connection read
    /// failing outright — here a `Db` with no mount index: the WARN with v4's
    /// `context` / `chat_id` / `error`, and the appearances as they were.
    #[tokio::test]
    async fn with_carrying_without_a_mount_index_warns_and_keeps_the_appearances() {
        crate::test_support::global_capture::install();
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_main(
            dir.path().join("main.db"),
            "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
        )
        .unwrap();
        let appearances = build_default_appearances(&[char_with_items(1)]);
        let (out, lines) = crate::test_support::global_capture::capture_async(with_carrying(
            &db,
            appearances.clone(),
            "chat-r9",
        ))
        .await;
        assert_eq!(out, appearances);
        let warn: Vec<&String> = lines
            .iter()
            .filter(|l| {
                l.contains("[AppearanceResolution] Could not add carried objects to appearances")
            })
            .collect();
        assert_eq!(warn.len(), 1, "{lines:?}");
        assert!(
            warn[0].starts_with("WARN quilltap::appearance_resolution"),
            "{}",
            warn[0]
        );
        assert!(
            warn[0].contains("context=image-gen.appearance-resolution chat_id=chat-r9 error="),
            "{}",
            warn[0]
        );
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
