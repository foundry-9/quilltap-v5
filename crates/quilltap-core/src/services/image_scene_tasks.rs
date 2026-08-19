//! Image / scene cheap-LLM tasks (v4 `lib/memory/cheap-llm-tasks/image-scene-tasks.ts`)
//! — the three in-scope functions the `generate_image` handler drives:
//!
//!   - [`craft_image_prompt`] — weave physical descriptions into a coherent scene
//!     prompt (the placeholder detail block + style-trigger + aesthetic sections,
//!     the quote-strip + UTF-16 truncation parser).
//!   - [`resolve_appearance`] — the per-character appearance-selection JSON parser
//!     (which physical description + what each character is wearing).
//!   - [`sanitize_appearance`] — explicit → safe appearance rewrite.
//!
//! W4.9c adds the two story-background scene tasks:
//!   - [`derive_scene_context`] — imagine a scene from the chat history (no
//!     max-tokens override; the quote/code-fence strip parser).
//!   - [`craft_story_background_prompt`] — the atmospheric landscape prompt
//!     (provider-specific length guidance, the shared `buildAestheticSection`,
//!     the `4000` max-tokens override).
//!
//! `updateSceneState` (scene-state tracking) / `describeAttachment` belong to
//! their own follow-ups, out of scope here.
//!
//! All three run over the ported [`CheapLlmTaskExecutor`] with the byte-exact
//! system prompts in the generated [`prompt_text`] submodule. The provider seam
//! is [`CompletionProvider`] — the tier-3 model boundary.

pub mod prompt_text;

use crate::cheap_llm::CheapLlmSelection;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskResult};
use crate::wardrobe::{describe_outfit, OutfitSlotValues};

// ===========================================================================
// craftImagePrompt
// ===========================================================================

/// One placeholder's expansion data (v4 `ImagePromptExpansionContext.placeholders[]`).
#[derive(Clone, Debug, Default)]
pub struct ExpansionPlaceholder {
    pub placeholder: String,
    pub name: String,
    /// `"male"` / `"female"` / `None`.
    pub gender: Option<String>,
    pub usage_context: Option<String>,
    pub tiers: ExpansionTiers,
    /// Present clothing/outfit hints (v4 `p.clothing`).
    pub clothing: Vec<ExpansionClothing>,
}

/// The four description tiers (v4 `p.tiers`).
#[derive(Clone, Debug, Default)]
pub struct ExpansionTiers {
    pub short: Option<String>,
    pub medium: Option<String>,
    pub long: Option<String>,
    pub complete: Option<String>,
}

/// One clothing/outfit hint on a placeholder (v4 `p.clothing[]`).
#[derive(Clone, Debug, Default)]
pub struct ExpansionClothing {
    pub name: String,
    pub usage_context: Option<String>,
    pub description: Option<String>,
}

/// v4 `ImagePromptExpansionContext` — the input to [`craft_image_prompt`].
#[derive(Clone, Debug, Default)]
pub struct ImagePromptExpansionContext {
    pub original_prompt: String,
    pub placeholders: Vec<ExpansionPlaceholder>,
    pub target_length: i64,
    /// The provider label (`OPENAI`/`GROK`/`GOOGLE_IMAGEN`) — only interpolated
    /// into the user message.
    pub provider: String,
    pub style_trigger_phrase: Option<String>,
    pub style_name: Option<String>,
    /// The default aesthetics + per-character depiction guidelines (the Ariel
    /// Clause). All error-swallowed at the call site → typically `None`.
    pub scene_aesthetic: Option<String>,
    pub character_aesthetic: Option<String>,
    pub depiction_guidelines: Vec<DepictionGuideline>,
}

/// One per-character depiction guideline (v4 `depictionGuidelines[]`).
#[derive(Clone, Debug)]
pub struct DepictionGuideline {
    pub character_name: String,
    pub content: String,
}

/// v4 `buildAestheticSection`: the labelled aesthetic + Ariel-Clause block
/// appended to a crafting call's user message. `""` when nothing is set. Shared
/// by [`craft_image_prompt`] and [`craft_story_background_prompt`].
fn build_aesthetic_section(
    scene_aesthetic: Option<&str>,
    character_aesthetic: Option<&str>,
    depiction_guidelines: &[DepictionGuideline],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(scene) = scene_aesthetic.filter(|s| !s.is_empty()) {
        parts.push(format!(
            "Overall image aesthetic (apply this style to the whole image):\n{scene}"
        ));
    }
    if let Some(character) = character_aesthetic.filter(|s| !s.is_empty()) {
        parts.push(format!(
            "Character depiction aesthetic (how people and clothing should look):\n{character}"
        ));
    }
    if !depiction_guidelines.is_empty() {
        let lines = depiction_guidelines
            .iter()
            .map(|g| format!("- {}: {}", g.character_name, g.content))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!(
            "Per-character depiction guidelines (MANDATORY — follow exactly for the named character, overriding the general aesthetic on conflict):\n{lines}"
        ));
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("\n{}\n", parts.join("\n\n"))
}

/// v4 `craftImagePrompt`: build the placeholder detail block + the user message,
/// run the cheap-LLM task, and parse (quote-strip + UTF-16 truncation).
#[allow(clippy::too_many_arguments)]
pub async fn craft_image_prompt<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    ctx: &ImagePromptExpansionContext,
    selection: &CheapLlmSelection,
    _user_id: &str,
    _chat_id: Option<&str>,
) -> CheapLlmTaskResult<String> {
    // Format placeholder data for the LLM.
    let placeholder_details = ctx
        .placeholders
        .iter()
        .map(format_placeholder)
        .collect::<Vec<_>>()
        .join("\n\n");

    // The style trigger section if provided.
    let style_trigger_section = match ctx.style_trigger_phrase.as_deref() {
        Some(phrase) => {
            let style_name = match ctx.style_name.as_deref() {
                Some(name) => format!(" (for \"{name}\" style)"),
                None => String::new(),
            };
            format!("\nStyle trigger phrase (MUST include exactly as shown): \"{phrase}\"{style_name}\n")
        }
        None => String::new(),
    };

    let aesthetic_section = build_aesthetic_section(
        ctx.scene_aesthetic.as_deref(),
        ctx.character_aesthetic.as_deref(),
        &ctx.depiction_guidelines,
    );

    let user_content = format!(
        "Original prompt: {original}\n\nAvailable descriptions:\n{details}\n{style}{aesthetic}\nTarget length: {target} characters (for {provider})\n\nCreate the final image prompt (maximize detail while staying under the limit):",
        original = ctx.original_prompt,
        details = placeholder_details,
        style = style_trigger_section,
        aesthetic = aesthetic_section,
        target = ctx.target_length,
        provider = ctx.provider,
    );

    let messages = vec![
        CompletionMessage::system(prompt_text::IMAGE_PROMPT_CRAFTING_PROMPT),
        CompletionMessage::user(user_content),
    ];

    let target_length = ctx.target_length;
    executor
        .execute(
            completion,
            selection,
            messages,
            move |content: &str| parse_craft_result(content, target_length),
            None,
            // v4 passes 4000 as `maxTokens`.
            Some(4000.0),
            // No character-id cache key on this path.
            None,
            Some("craft-image-prompt"),
        )
        .await
}

/// v4's `craftImagePrompt` placeholder formatter — one detail block per person.
fn format_placeholder(p: &ExpansionPlaceholder) -> String {
    let gender_hint = match p.gender.as_deref() {
        Some("male") => " [man]",
        Some("female") => " [woman]",
        _ => "",
    };
    let mut parts: Vec<String> = vec![format!("{} ({}{}):", p.placeholder, p.name, gender_hint)];

    if let Some(gender) = p.gender.as_deref() {
        let term = if gender == "male" { "man" } else { "woman" };
        parts.push(format!(
            "  Gender: {term} — always refer to this person as a {term} in the prompt"
        ));
    }

    if let Some(usage) = p.usage_context.as_deref() {
        parts.push(format!("  Usage context: {usage}"));
    }

    if let Some(v) = p.tiers.complete.as_deref() {
        parts.push(format!("  Complete: \"{v}\""));
    }
    if let Some(v) = p.tiers.long.as_deref() {
        parts.push(format!("  Long: \"{v}\""));
    }
    if let Some(v) = p.tiers.medium.as_deref() {
        parts.push(format!("  Medium: \"{v}\""));
    }
    if let Some(v) = p.tiers.short.as_deref() {
        parts.push(format!("  Short: \"{v}\""));
    }

    if !p.clothing.is_empty() {
        parts.push("  Clothing/Outfits:".to_string());
        for outfit in &p.clothing {
            let context_hint = match outfit.usage_context.as_deref() {
                Some(u) => format!(" (when: {u})"),
                None => String::new(),
            };
            let desc = match outfit.description.as_deref() {
                Some(d) => format!(": {d}"),
                None => String::new(),
            };
            parts.push(format!("    - \"{}\"{context_hint}{desc}", outfit.name));
        }
    }

    if parts.len() == 1 {
        parts.push("  (No descriptions available - use name only)".to_string());
    }

    parts.join("\n")
}

/// v4's craft parser: trim, strip a single wrapping quote at each end, and
/// UTF-16-truncate to the target length (append `...` on overflow).
fn parse_craft_result(content: &str, target_length: i64) -> String {
    // `prompt.replace(/^["']|["']$/g, '')` — strip one leading + one trailing quote.
    let mut prompt = strip_one_wrapping_quote_pair(js_trim(content));

    // `if (prompt.length > targetLength) prompt = prompt.substring(0, targetLength - 3) + '...'`
    // — `.length`/`.substring` are UTF-16.
    let target = target_length.max(0) as usize;
    if utf16_len(&prompt) > target {
        let cut = target.saturating_sub(3);
        prompt = format!("{}...", utf16_truncate(&prompt, cut));
    }

    prompt
}

// ===========================================================================
// resolveAppearance
// ===========================================================================

/// One character's appearance-resolution input (v4 `CharacterAppearanceInput`).
#[derive(Clone, Debug, Default)]
pub struct CharacterAppearanceInput {
    pub character_id: String,
    pub character_name: String,
    pub physical_descriptions: Vec<PhysicalDescriptionInput>,
    /// Equipped wardrobe items (per-slot leaf items).
    pub equipped_wardrobe_items: Vec<EquippedWardrobeItem>,
}

/// One physical-description tier bundle the resolver sees (v4
/// `CharacterAppearanceInput.physicalDescriptions[]`).
#[derive(Clone, Debug, Default)]
pub struct PhysicalDescriptionInput {
    pub id: String,
    pub name: String,
    pub usage_context: Option<String>,
    pub short_prompt: Option<String>,
    pub medium_prompt: Option<String>,
}

/// One equipped wardrobe item (v4's per-slot leaf, image-prompt preferred over
/// title).
#[derive(Clone, Debug, Default)]
pub struct EquippedWardrobeItem {
    pub slot: String,
    pub title: String,
    pub description: Option<String>,
    /// Plain-text image cue; preferred over `title` in image prompts.
    pub image_prompt: Option<String>,
}

/// One resolved appearance item (v4 `AppearanceResolutionItem`).
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceResolutionItem {
    pub character_id: String,
    pub selected_description_id: Option<String>,
    pub clothing_description: String,
    /// `"narrative" | "stored" | "default"`.
    pub clothing_source: String,
}

/// One chat message the resolver sees (v4 `ChatMessage`, `role: 'user' |
/// 'assistant' | 'system'`).
#[derive(Clone, Debug)]
pub struct ChatMessage {
    /// `"user"` / `"assistant"` / (anything else → rendered as `System`).
    pub role: String,
    pub content: String,
}

/// v4 `wardrobeItemsToSlotValues` / the resolver's inline `valuesFor`: per-slot
/// arrays for `describeOutfit`, image-prompt preferred over title (built over
/// v4's `buildOutfitSlotValues`, so new slots ride the registry).
fn wardrobe_items_to_slot_values(items: &[EquippedWardrobeItem]) -> OutfitSlotValues {
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

/// v4 `resolveAppearance`: build the character + message sections, run the
/// cheap-LLM task, and parse the JSON array.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_appearance<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    characters: &[CharacterAppearanceInput],
    recent_messages: &[ChatMessage],
    image_prompt: &str,
    selection: &CheapLlmSelection,
    _user_id: &str,
    _chat_id: Option<&str>,
) -> CheapLlmTaskResult<Vec<AppearanceResolutionItem>> {
    let character_section = characters
        .iter()
        .map(|char| {
            let desc_parts: Vec<String> = char
                .physical_descriptions
                .iter()
                .map(|d| {
                    let context = match d.usage_context.as_deref() {
                        Some(u) => format!(" (context: {u})"),
                        None => String::new(),
                    };
                    let preview = d
                        .medium_prompt
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .or(d.short_prompt.as_deref().filter(|s| !s.is_empty()))
                        .unwrap_or("(no description text)");
                    format!("    - ID: {}, Name: \"{}\"{context}: {preview}", d.id, d.name)
                })
                .collect();

            let wardrobe_section = if char.equipped_wardrobe_items.is_empty() {
                String::new()
            } else {
                let outfit = describe_outfit(&wardrobe_items_to_slot_values(
                    &char.equipped_wardrobe_items,
                ));
                format!(
                    "\n  Equipped Wardrobe (stored items the character currently has on — summarize as a brief visual sentence):\n{outfit}"
                )
            };

            let descs = if desc_parts.is_empty() {
                "    (none)".to_string()
            } else {
                desc_parts.join("\n")
            };

            format!(
                "  Character: {name} (ID: {id})\n  Physical Descriptions:\n{descs}{wardrobe}",
                name = char.character_name,
                id = char.character_id,
                wardrobe = wardrobe_section,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Format recent messages: last 20, 500-char UTF-16 truncation per message.
    let start = recent_messages.len().saturating_sub(20);
    let message_text = recent_messages[start..]
        .iter()
        .map(|m| {
            let speaker = match m.role.as_str() {
                "user" => "User",
                "assistant" => "Character",
                _ => "System",
            };
            let content = if utf16_len(&m.content) > 500 {
                format!("{}...", utf16_truncate(&m.content, 500))
            } else {
                m.content.clone()
            };
            format!("{speaker}: {content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let message_text = if message_text.is_empty() {
        "(no messages yet)".to_string()
    } else {
        message_text
    };

    let user_content = format!(
        "Image prompt: {image_prompt}\n\nCharacters:\n{character_section}\n\nRecent Conversation:\n{message_text}\n\nDetermine what each character currently looks like and is wearing:"
    );

    let messages = vec![
        CompletionMessage::system(prompt_text::APPEARANCE_RESOLUTION_PROMPT),
        CompletionMessage::user(user_content),
    ];

    executor
        .execute(
            completion,
            selection,
            messages,
            parse_appearance_result,
            None,
            None,
            None,
            Some("resolve-character-appearances"),
        )
        .await
}

/// v4's resolver JSON parser: strip fences, parse the array, coerce each item.
fn parse_appearance_result(content: &str) -> Vec<AppearanceResolutionItem> {
    let clean = strip_code_fences(content);
    let parsed: serde_json::Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = parsed.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .map(|item| {
            let character_id = js_string_or_empty(item.get("characterId"));
            let selected_description_id = match item.get("selectedDescriptionId") {
                Some(v) if !is_js_falsy(v) => Some(js_string(v)),
                _ => None,
            };
            let clothing_description = js_string_or_empty(item.get("clothingDescription"));
            let source_raw = js_string_or_empty(item.get("clothingSource"));
            let clothing_source =
                if ["narrative", "stored", "default"].contains(&source_raw.as_str()) {
                    source_raw
                } else {
                    "default".to_string()
                };
            AppearanceResolutionItem {
                character_id,
                selected_description_id,
                clothing_description,
                clothing_source,
            }
        })
        .collect()
}

// ===========================================================================
// sanitizeAppearance
// ===========================================================================

/// One appearance to sanitize (v4's `{ characterId, appearanceText }`).
#[derive(Clone, Debug, PartialEq)]
pub struct AppearanceText {
    pub character_id: String,
    pub appearance_text: String,
}

/// v4 `sanitizeAppearance`: send the JSON array, parse the sanitized reply
/// (returning the originals on any parse failure).
pub async fn sanitize_appearance<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    appearances: &[AppearanceText],
    selection: &CheapLlmSelection,
    _user_id: &str,
    _chat_id: Option<&str>,
) -> CheapLlmTaskResult<Vec<AppearanceText>> {
    // v4 `JSON.stringify(appearances)` — the compact serialization, in field
    // order `characterId, appearanceText` (a serde struct fixes key order).
    #[derive(serde::Serialize)]
    struct Wire<'a> {
        #[serde(rename = "characterId")]
        character_id: &'a str,
        #[serde(rename = "appearanceText")]
        appearance_text: &'a str,
    }
    let wire: Vec<Wire> = appearances
        .iter()
        .map(|a| Wire {
            character_id: &a.character_id,
            appearance_text: &a.appearance_text,
        })
        .collect();
    let user_content = serde_json::to_string(&wire).unwrap_or_else(|_| "[]".to_string());

    let messages = vec![
        CompletionMessage::system(prompt_text::APPEARANCE_SANITIZATION_PROMPT),
        CompletionMessage::user(user_content),
    ];

    let originals = appearances.to_vec();
    executor
        .execute(
            completion,
            selection,
            messages,
            move |content: &str| parse_sanitize_result(content, &originals),
            None,
            None,
            None,
            Some("sanitize-appearance"),
        )
        .await
}

/// v4's sanitize parser: strip fences, parse the array, coerce; on any failure
/// (non-array or JSON error) return the originals.
fn parse_sanitize_result(content: &str, originals: &[AppearanceText]) -> Vec<AppearanceText> {
    let clean = strip_code_fences(content);
    let parsed: serde_json::Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(_) => return originals.to_vec(),
    };
    let Some(arr) = parsed.as_array() else {
        return originals.to_vec();
    };
    arr.iter()
        .map(|item| AppearanceText {
            character_id: js_string_or_empty(item.get("characterId")),
            appearance_text: js_string_or_empty(item.get("appearanceText")),
        })
        .collect()
}

// ===========================================================================
// deriveSceneContext (W4.9c)
// ===========================================================================

/// v4 `DeriveSceneContextInput` — the input to [`derive_scene_context`].
#[derive(Clone, Debug, Default)]
pub struct DeriveSceneContextInput {
    pub chat_title: String,
    /// v4 `chat.contextSummary` (`string | null | undefined`); an empty string is
    /// JS-falsy so it is treated as absent by the `if (input.contextSummary)` gate.
    pub context_summary: Option<String>,
    pub recent_messages: Vec<ChatMessage>,
    pub character_names: Vec<String>,
}

/// v4 `deriveSceneContext`: build the context block + conversation window and
/// derive an imaginative scene description. NO max-tokens override (unlike
/// `craftStoryBackgroundPrompt`).
pub async fn derive_scene_context<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    input: &DeriveSceneContextInput,
    selection: &CheapLlmSelection,
    _user_id: &str,
    _chat_id: Option<&str>,
) -> CheapLlmTaskResult<String> {
    // Each message: `${speaker}: ${content}`, content truncated at 500 UTF-16.
    let message_text = input
        .recent_messages
        .iter()
        .map(|m| {
            let speaker = speaker_label(&m.role);
            let content = if utf16_len(&m.content) > 500 {
                format!("{}...", utf16_truncate(&m.content, 500))
            } else {
                m.content.clone()
            };
            format!("{speaker}: {content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut context_info = format!("Chat Title: \"{}\"", input.chat_title);
    if let Some(summary) = input.context_summary.as_deref().filter(|s| !s.is_empty()) {
        context_info.push_str(&format!("\n\nExisting Summary:\n{summary}"));
    }
    if !input.character_names.is_empty() {
        context_info.push_str(&format!(
            "\n\nCharacters present: {}",
            input.character_names.join(", ")
        ));
    }

    let user_content = format!(
        "{context_info}\n\nRecent Conversation:\n{message_text}\n\nBased on this conversation, describe the scene these characters might be in:"
    );

    let messages = vec![
        CompletionMessage::system(prompt_text::SCENE_CONTEXT_DERIVATION_PROMPT),
        CompletionMessage::user(user_content),
    ];

    executor
        .execute(
            completion,
            selection,
            messages,
            parse_scene_result,
            None,
            // No max-tokens override on this path.
            None,
            None,
            Some("derive-scene-context"),
        )
        .await
}

// ===========================================================================
// craftStoryBackgroundPrompt (W4.9c)
// ===========================================================================

/// One character section entry (v4 `StoryBackgroundPromptContext.characters[]`).
#[derive(Clone, Debug, Default)]
pub struct StoryBackgroundCharacter {
    pub name: String,
    pub description: String,
}

/// v4 `StoryBackgroundPromptContext` — the input to [`craft_story_background_prompt`].
#[derive(Clone, Debug, Default)]
pub struct StoryBackgroundPromptContext {
    pub scene_context: String,
    pub characters: Vec<StoryBackgroundCharacter>,
    /// The image provider label (`GROK` gets a tighter length cap).
    pub provider: String,
    pub scene_aesthetic: Option<String>,
    pub character_aesthetic: Option<String>,
    pub depiction_guidelines: Vec<DepictionGuideline>,
    /// [decd8ef9] True when the crafted prompt is bound for a Concierge
    /// uncensored image provider — a dangerous-marked chat with one configured,
    /// or a post-hoc moderation reroute onto one. Swaps the crafter's intimacy
    /// guidance from cinematic concealment to a candid depiction. Defaults to
    /// false: a prompt headed for a moderated provider still gets the
    /// concealment treatment. (v4 `StoryBackgroundPromptContext.
    /// uncensoredImageTarget?: boolean`, whose default is enforced at the call
    /// site as `context.uncensoredImageTarget === true` — `bool` here, since a
    /// missing key and an explicit `false` are the same prompt.)
    pub uncensored_image_target: bool,
}

/// v4 `buildStoryBackgroundPrompt`: assemble the story-background crafter's
/// system prompt from the shared head/tail plus ONE of the two intimacy blocks
/// and ONE of the two worked examples.
///
/// The concealment guidance exists to clear moderation the uncensored target
/// does not perform, so an uncensored target gets candid depiction instead;
/// both variants keep every BACKGROUND framing rule and both still refuse to
/// re-dress a character the narrative undressed.
///
/// The five-element order + the `"\n\n"` join reproduce the pre-split
/// `STORY_BACKGROUND_PROMPT` constant byte-for-byte on the concealed path
/// (pinned below at 5114 UTF-16 units — v4's own refactor claims the same
/// identity, and this port verified it against the previously generated
/// constant's exact bytes).
pub fn build_story_background_prompt(uncensored_image_target: bool) -> String {
    let intimacy = if uncensored_image_target {
        prompt_text::STORY_BACKGROUND_CANDID_INTIMACY
    } else {
        prompt_text::STORY_BACKGROUND_CONCEALED_INTIMACY
    };
    let example = if uncensored_image_target {
        prompt_text::STORY_BACKGROUND_CANDID_EXAMPLE
    } else {
        prompt_text::STORY_BACKGROUND_CONCEALED_EXAMPLE
    };
    [
        prompt_text::STORY_BACKGROUND_PROMPT_HEAD,
        intimacy,
        prompt_text::STORY_BACKGROUND_PROMPT_TAIL,
        example,
        prompt_text::STORY_BACKGROUND_PROMPT_CLOSING,
    ]
    .join("\n\n")
}

/// v4 `craftStoryBackgroundPrompt`: build the character section + the aesthetic
/// block + the provider length guidance, run the task (with the `4000`
/// max-tokens override), and parse (quote + code-fence strip).
pub async fn craft_story_background_prompt<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    ctx: &StoryBackgroundPromptContext,
    selection: &CheapLlmSelection,
    _user_id: &str,
    _chat_id: Option<&str>,
) -> CheapLlmTaskResult<String> {
    let character_section = if ctx.characters.is_empty() {
        "\nNo specific characters to include - create an atmospheric scene matching the context."
            .to_string()
    } else {
        let lines = ctx
            .characters
            .iter()
            .map(|c| format!("- {}: {}", c.name, c.description))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\nCharacters to include as figures in the scene:\n{lines}")
    };

    // Provider-specific length guidance.
    let length_guidance = if ctx.provider == "GROK" {
        "Keep the prompt under 1000 characters for Grok image generation."
    } else {
        "Keep the prompt under 1200 characters."
    };

    let aesthetic_section = build_aesthetic_section(
        ctx.scene_aesthetic.as_deref(),
        ctx.character_aesthetic.as_deref(),
        &ctx.depiction_guidelines,
    );

    let user_content = format!(
        "Scene context: {scene}\n{characters}\n{aesthetic}\nProvider: {provider}\n{length}\n\nCreate the atmospheric background prompt:",
        scene = ctx.scene_context,
        characters = character_section,
        aesthetic = aesthetic_section,
        provider = ctx.provider,
        length = length_guidance,
    );

    let messages = vec![
        CompletionMessage::system(build_story_background_prompt(ctx.uncensored_image_target)),
        CompletionMessage::user(user_content),
    ];

    executor
        .execute(
            completion,
            selection,
            messages,
            parse_scene_result,
            None,
            // v4 passes 4000 as `maxTokens`.
            Some(4000.0),
            None,
            Some("craft-story-background-prompt"),
        )
        .await
}

/// v4's speaker label for a chat message: `user → User`, `assistant → Character`,
/// anything else → `System`.
fn speaker_label(role: &str) -> &'static str {
    match role {
        "user" => "User",
        "assistant" => "Character",
        _ => "System",
    }
}

/// The shared `deriveSceneContext` / `craftStoryBackgroundPrompt` parser: trim,
/// strip one wrapping quote at each end, then strip a leading/trailing code
/// fence. (Distinct from `craft_image_prompt`, which truncates instead of
/// stripping fences, and from `strip_code_fences`, which trims first.)
fn parse_scene_result(content: &str) -> String {
    let trimmed = js_trim(content).to_string();
    let dequoted = strip_one_wrapping_quote_pair(&trimmed);
    strip_inline_code_fences(&dequoted)
}

/// v4 `.replace(/^["']|["']$/g, '')` — strip at most one leading and one trailing
/// quote (`"` or `'`). A global regex with two alternatives matches the start
/// once and the end once. Also used by [`parse_craft_result`].
fn strip_one_wrapping_quote_pair(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let has_lead = matches!(chars.first(), Some('"' | '\''));
    let has_trail = chars.len() > 1 && matches!(chars.last(), Some('"' | '\''));
    if !has_lead && !has_trail {
        return s.to_string();
    }
    let start = if has_lead { 1 } else { 0 };
    let end = if has_trail {
        chars.len() - 1
    } else {
        chars.len()
    };
    if start < end {
        chars[start..end].iter().collect()
    } else {
        String::new()
    }
}

/// Strip a leading and trailing Markdown code fence (v4's two anchored
/// `.replace` calls on the triple-backtick fence — see the source for the exact
/// regexes): a leading fence with an optional lowercase-ascii language tag + a
/// following JS-whitespace run, then a trailing (JS-whitespace run +) fence. Both
/// anchored, so each fires at most once.
fn strip_inline_code_fences(s: &str) -> String {
    use crate::jsstr::is_js_ws;
    // Leading `^```[a-z]*\s*`.
    let mut work: &str = s;
    if let Some(rest) = work.strip_prefix("```") {
        let after_lang = rest.trim_start_matches(|c: char| c.is_ascii_lowercase());
        let after_ws = after_lang.trim_start_matches(is_js_ws);
        work = after_ws;
    }
    // Trailing `\s*```$`.
    if let Some(rest) = work.strip_suffix("```") {
        work = rest.trim_end_matches(is_js_ws);
    }
    work.to_string()
}

// ===========================================================================
// JS `String(...)` coercion helpers (matching the parsers' `String(item.x || '')`)
// ===========================================================================

/// v4 `String(v)` on a JSON value (the coercions the parsers rely on: a string
/// stays itself, a number renders bare, a bool → `true`/`false`, `null`/absent
/// via the `|| ''` fallback so callers use [`js_string_or_empty`]).
fn js_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// `String(item.x || '')` — the `|| ''` short-circuits on a JS-falsy value
/// (absent / `null` / `false` / `0` / `""`), yielding `''`.
fn js_string_or_empty(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(val) if !is_js_falsy(val) => js_string(val),
        _ => String::new(),
    }
}

/// JS falsiness for the JSON values the parsers meet: `null`, `false`, `0`, `""`.
/// (Objects/arrays are truthy.)
fn is_js_falsy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(b) => !b,
        serde_json::Value::Number(n) => n.as_f64() == Some(0.0),
        serde_json::Value::String(s) => s.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [decd8ef9] The concealed (default) assembly is byte-identical to the
    /// pre-split `STORY_BACKGROUND_PROMPT` constant. The length pin is the one
    /// this file has always carried (5114 UTF-16 code units); the port ALSO
    /// diffed the assembly against the previously generated constant's exact
    /// bytes when the split landed.
    #[test]
    fn concealed_assembly_matches_the_pre_split_constant_length() {
        let concealed = build_story_background_prompt(false);
        assert_eq!(concealed.encode_utf16().count(), 5114);
        assert!(concealed.starts_with(
            "You are a skilled visual artist and prompt engineer specializing in atmospheric landscape scenes for story backgrounds."
        ));
        assert!(concealed.ends_with(
            "Respond with ONLY the final prompt - no explanations, no markdown formatting, no quotes."
        ));
        // The assembly is exactly the five pieces joined by a blank line.
        assert_eq!(
            concealed,
            [
                prompt_text::STORY_BACKGROUND_PROMPT_HEAD,
                prompt_text::STORY_BACKGROUND_CONCEALED_INTIMACY,
                prompt_text::STORY_BACKGROUND_PROMPT_TAIL,
                prompt_text::STORY_BACKGROUND_CONCEALED_EXAMPLE,
                prompt_text::STORY_BACKGROUND_PROMPT_CLOSING,
            ]
            .join("\n\n")
        );
    }

    /// v4 `story-background-intimacy.test.ts` case 1 + 2: the default and an
    /// explicit `false` both keep cinematic concealment. The assertion
    /// substrings are v4's, verbatim.
    #[test]
    fn defaults_to_cinematic_concealment() {
        for target in [
            build_story_background_prompt(false),
            // The context default (v4: an absent `uncensoredImageTarget` key).
            build_story_background_prompt(
                StoryBackgroundPromptContext::default().uncensored_image_target,
            ),
        ] {
            assert!(target.contains("do NOT render explicit nudity"));
            assert!(target.contains("cinematic concealment"));
            assert!(target.contains("Drapery:"));
            assert!(target.contains("BAD (too explicit, will be rejected)"));
            assert!(!target.contains("The target image provider accepts adult content"));
        }
    }

    /// v4 `story-background-intimacy.test.ts` case 3: an uncensored image
    /// target swaps in candid depiction and NONE of the concealment machinery
    /// survives.
    #[test]
    fn swaps_in_candid_depiction_for_an_uncensored_image_target() {
        let system = build_story_background_prompt(true);
        assert!(system.contains("The target image provider accepts adult content"));
        assert!(system.contains("BAD (needlessly coy"));
        assert!(!system.contains("cinematic concealment"));
        assert!(!system.contains("Drapery:"));
        assert!(!system.contains("Foreground occlusion:"));
        assert!(!system.contains("tasteful concealment"));
    }

    /// v4 `story-background-intimacy.test.ts` cases 4 + 5: neither variant
    /// re-dresses the scene, and both keep the shared background-framing rules.
    #[test]
    fn both_variants_refuse_re_dressing_and_keep_the_framing_rules() {
        for uncensored in [false, true] {
            let system = build_story_background_prompt(uncensored);
            assert!(system.contains("\"wearing pajamas\""));
            assert!(system.contains("This is for a BACKGROUND image, not a portrait"));
            assert!(system.contains("Characters should be toward the left and right of the frame"));
            assert!(system.contains("AESTHETIC & DEPICTION GUIDELINES"));
            assert!(system.contains("MANDATORY, binding constraints"));
            assert!(system.contains("Respond with ONLY the final prompt"));
        }
    }

    #[test]
    fn craft_parse_strips_quotes_and_truncates() {
        assert_eq!(parse_craft_result("  \"hello\"  ", 100), "hello");
        assert_eq!(parse_craft_result("'x'", 100), "x");
        // Truncation: target 5 → keep 2 chars + "...".
        assert_eq!(parse_craft_result("abcdefgh", 5), "ab...");
        // Only one quote at the end.
        assert_eq!(parse_craft_result("ab'", 100), "ab");
    }

    #[test]
    fn appearance_parse_defaults_and_coerces() {
        let items = parse_appearance_result(
            r#"[{"characterId":"c1","selectedDescriptionId":null,"clothingDescription":"a red dress","clothingSource":"narrative"},{"characterId":"c2","clothingSource":"bogus"}]"#,
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].character_id, "c1");
        assert_eq!(items[0].selected_description_id, None);
        assert_eq!(items[0].clothing_source, "narrative");
        assert_eq!(items[1].clothing_description, "");
        assert_eq!(items[1].clothing_source, "default");
    }

    #[test]
    fn sanitize_parse_falls_back_to_originals() {
        let originals = vec![AppearanceText {
            character_id: "c1".into(),
            appearance_text: "orig".into(),
        }];
        // Non-array → originals.
        assert_eq!(parse_sanitize_result("{}", &originals), originals);
        // Garbage → originals.
        assert_eq!(parse_sanitize_result("not json", &originals), originals);
        // Valid array is parsed.
        let out = parse_sanitize_result(
            r#"[{"characterId":"c1","appearanceText":"safe"}]"#,
            &originals,
        );
        assert_eq!(out[0].appearance_text, "safe");
    }

    #[test]
    fn wardrobe_items_prefer_image_prompt() {
        let items = vec![
            EquippedWardrobeItem {
                slot: "top".into(),
                title: "Blouse".into(),
                image_prompt: Some("silk blouse".into()),
                ..Default::default()
            },
            EquippedWardrobeItem {
                slot: "top".into(),
                title: "Jacket".into(),
                image_prompt: Some("  ".into()),
                ..Default::default()
            },
        ];
        let sv = wardrobe_items_to_slot_values(&items);
        assert_eq!(
            sv.top,
            vec!["silk blouse".to_string(), "Jacket".to_string()]
        );
    }
}
