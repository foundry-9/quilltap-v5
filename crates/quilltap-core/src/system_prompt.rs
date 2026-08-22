//! Port of v4's `lib/chat/context/system-prompt-builder.ts` — builds the
//! system prompts for characters in single- and multi-character chats.
//!
//! Everything here is pure: it composes the freshly-ported template processor
//! ([`crate::templates`]) and chat-timestamp utilities
//! ([`crate::chat_timestamp`]) into byte-stable prompt strings. The five
//! exported v4 functions map one-to-one:
//!
//! - [`build_identity_stack`] — the static identity portion (preamble, base
//!   prompt, manifesto, personality, aliases, pronouns, physical appearance,
//!   example dialogue), with `{{user}}`/`{{scenario}}`/`{{persona}}` resolved.
//! - [`build_public_identity_card`] — the SURFACE-LEVEL card one character sees
//!   of another (name, title, pronouns, aliases, `identity`→`description`
//!   fallback → [`NO_PUBLIC_IDENTITY_FALLBACK`]). Deliberately omits the private
//!   vantage points (`personality`, `manifesto`).
//! - [`build_system_prompt`] — the per-turn prompt: identity stack + roleplay
//!   template + tool instructions + tool reinforcement.
//! - [`build_other_participants_info`] — the multi-character roster info still
//!   consumed downstream (mentioned-characters scan, identity-reinforcement
//!   names).
//! - [`build_identity_reinforcement`] — the fully-static "stay in {{char}}'s
//!   voice" reminder.
//!
//! # Character-field vantage points
//!
//! `identity` / `description` / `personality` / `title` are four distinct
//! vantage points (public-surface / acquaintance / self-knowledge / private
//! label) and are never interchanged. The public identity card exposes only the
//! outward-facing ones; the identity stack surfaces the private ones only to the
//! character themselves.
//!
//! # JS truthiness reproduced
//!
//! Every `||` fallback in v4 is the JS empty-string-is-falsy kind: an empty
//! string falls through to the next branch. The port models each optional field
//! as `Option<String>` and treats `None` *and* `Some("")` as falsy, matching v4
//! (`character.description || ''`, `desc.shortPrompt || desc.mediumPrompt`, …).
//! `.trim()` on a JS string is [`crate::jsstr::js_trim`] (Unicode whitespace).

use crate::chat_timestamp::{
    calculate_current_timestamp, should_inject_timestamp, InvalidTimezone, TimestampConfig,
};
use crate::jsstr::js_trim;
use crate::templates::{process_template, TemplateContext};

/// Pronouns triple, mirroring v4's `Pronouns` (`subject`/`object`/`possessive`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pronouns {
    pub subject: String,
    pub object: String,
    pub possessive: String,
}

/// A named system prompt on a character (`character.systemPrompts[]`), only the
/// fields the builder consumes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemPromptEntry {
    pub id: String,
    pub content: String,
    pub is_default: bool,
}

/// A named scenario on a character (`character.scenarios[]`), only the `content`
/// field the builder consumes (via `scenarios?.[0]?.content`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScenarioEntry {
    pub content: String,
}

/// The subset of `character.physicalDescription` the identity stack reads.
///
/// `name` is required in v4's schema; the prompt chain is
/// `shortPrompt || mediumPrompt || longPrompt || completePrompt ||
/// fullDescription || ''` — note `headAndShouldersPrompt` is deliberately NOT in
/// the chain (it feeds avatar generation, not the identity stack).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhysicalDescription {
    pub name: String,
    pub usage_context: Option<String>,
    pub short_prompt: Option<String>,
    pub medium_prompt: Option<String>,
    pub long_prompt: Option<String>,
    pub complete_prompt: Option<String>,
    pub full_description: Option<String>,
}

/// The character fields the system-prompt builder consumes. A faithful subset of
/// v4's `Character`; every optional text field is `Option<String>` with the JS
/// empty-string-is-falsy convention applied at each use site.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Character {
    pub name: String,
    /// The private label ("the rival", …). NOT a vantage point exposed to the
    /// identity stack; only the public card surfaces it (template-processed).
    pub title: Option<String>,
    /// Public-surface vantage point (what strangers know). Public card only.
    pub identity: Option<String>,
    /// Acquaintance-facing vantage point. Feeds `{{description}}` and the public
    /// card's fallback body.
    pub description: Option<String>,
    /// The load-bearing axiomatic core. Private; identity stack only.
    pub manifesto: Option<String>,
    /// Self-knowledge vantage point. Private; identity stack only.
    pub personality: Option<String>,
    pub aliases: Vec<String>,
    pub pronouns: Option<Pronouns>,
    pub physical_description: Option<PhysicalDescription>,
    pub example_dialogues: Option<String>,
    pub scenarios: Vec<ScenarioEntry>,
    pub system_prompts: Vec<SystemPromptEntry>,
}

/// JS `value || ''` for an `Option<String>` field: `None` or an empty string is
/// falsy → `""`, else the value. Returns a borrowed slice.
fn or_empty(v: &Option<String>) -> &str {
    match v {
        Some(s) if !s.is_empty() => s.as_str(),
        _ => "",
    }
}

/// JS truthiness for an `Option<String>`: present and non-empty.
fn is_truthy(v: &Option<String>) -> bool {
    matches!(v, Some(s) if !s.is_empty())
}

// ---------------------------------------------------------------------------
// buildIdentityStack
// ---------------------------------------------------------------------------

/// Inputs that uniquely determine a character's static identity stack — v4's
/// `BuildIdentityStackOptions`.
#[derive(Clone, Debug)]
pub struct BuildIdentityStackOptions<'a> {
    pub character: &'a Character,
    pub user_character: Option<&'a UserCharacter>,
    pub selected_system_prompt_id: Option<&'a str>,
    pub scenario_text: Option<&'a str>,
}

/// A user character for the `{{user}}`/`{{persona}}` placeholders — v4's
/// `{ name, description }` shape (both used as-is; empty falls back per v4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserCharacter {
    pub name: String,
    pub description: String,
}

/// Build the template context v4's `buildIdentityStack`/`buildSystemPrompt`
/// construct inline (six keys). `user` defaults to `"User"`; `scenario` is the
/// caller override then `character.scenarios?.[0]?.content` then `''`.
fn stack_template_context(
    character: &Character,
    user_character: Option<&UserCharacter>,
    scenario_text: Option<&str>,
) -> TemplateContext {
    let mut ctx = TemplateContext::default();
    ctx.set("char", character.name.clone());
    // userCharacter?.name || 'User'
    let user = match user_character {
        Some(uc) if !uc.name.is_empty() => uc.name.clone(),
        _ => "User".to_string(),
    };
    ctx.set("user", user);
    ctx.set("description", or_empty(&character.description));
    ctx.set("personality", or_empty(&character.personality));
    // scenarioText || character.scenarios?.[0]?.content || ''
    let scenario = scenario_text
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            character
                .scenarios
                .first()
                .map(|s| s.content.clone())
                .filter(|c| !c.is_empty())
        })
        .unwrap_or_default();
    ctx.set("scenario", scenario);
    // persona = userCharacter?.description || ''
    let persona = match user_character {
        Some(uc) if !uc.description.is_empty() => uc.description.clone(),
        _ => String::new(),
    };
    ctx.set("persona", persona);
    ctx
}

/// Version of [`build_identity_stack`]'s OUTPUT — the wording and layout of the
/// blocks it emits (v4 `IDENTITY_STACK_BUILDER_VERSION`, `a6870c5a`). The
/// system-prompt compiler
/// ([`crate::services::system_prompt_compiler`]) stamps this into
/// `chats.compiledIdentityStacks` on write and requires strict equality on
/// read: a stored stack with an absent, older, or newer stamp is treated as
/// missing, so the read-through fallback rebuilds it with the current wording.
/// (Newer matters on a downgrade — a rolled-back build must not consume stacks
/// a later build wrote.) Rows written before the stamp existed are bare
/// participantId→stack maps with no `version` key; they read as stale
/// (effectively version 0) and rebuild lazily. No migration needed.
///
/// Bump this whenever an edit to [`build_identity_stack`] changes its output
/// for unchanged inputs — wording, ordering, blocks added or removed.
/// Forgetting is not an option: the `identity_stack_golden_*` unit tests below
/// bind this constant to a golden hash of the function's output (v4's
/// `IDENTITY_STACK_GOLDENS` table, byte-copied), so editing without bumping
/// fails on the hash and bumping without registering a new golden fails on the
/// lookup.
///
/// Distinct from `PROMPT_CACHE_STRUCTURE_VERSION` ([`crate::cheap_llm`]), which
/// versions the whole prompt shape for PROVIDER caches. This constant versions
/// one function's output for OUR compiled-stack cache; colocation with the
/// function is deliberate so whoever edits the strings sees it.
pub const IDENTITY_STACK_BUILDER_VERSION: u32 = 2;

/// Build just the static character-identity portion of the system prompt, with
/// chat-level template variables resolved — v4 `buildIdentityStack`
/// (system-prompt-builder.ts:55).
pub fn build_identity_stack(options: &BuildIdentityStackOptions<'_>) -> String {
    let character = options.character;
    let mut parts: Vec<String> = Vec::new();

    let ctx = stack_template_context(character, options.user_character, options.scenario_text);

    // Identity preamble — anchors the LLM's identity from the very first tokens.
    parts.push(process_template(
        "## Character Identity\nYou are {{char}}. Everything that follows defines who you are and how you behave. Stay in character at all times.",
        &ctx,
    ));

    // Base system prompt — selected > default > nothing.
    let mut system_prompt_content: Option<&str> = None;
    if let Some(selected_id) = options.selected_system_prompt_id {
        if !selected_id.is_empty() {
            if let Some(p) = character
                .system_prompts
                .iter()
                .find(|p| p.id == selected_id)
            {
                system_prompt_content = Some(p.content.as_str());
            }
        }
    }
    // JS `!systemPromptContent` — a falsy (None or empty) selected content falls
    // through to the default lookup.
    if system_prompt_content.is_none_or(str::is_empty) {
        if let Some(p) = character.system_prompts.iter().find(|p| p.is_default) {
            system_prompt_content = Some(p.content.as_str());
        }
    }
    // `if (systemPromptContent)` — only pushes a non-empty string.
    if let Some(content) = system_prompt_content {
        if !content.is_empty() {
            parts.push(process_template(content, &ctx));
        }
    }

    // WHY the wrappers and second person throughout (v4 `a6870c5a`): the
    // preamble above binds the model's identity slot with "You are {{char}}", so
    // every block whose referent is the speaking character stays in the same
    // register — a third-person sentence in this position reads as lore about
    // someone else. Author-carried fields (manifesto, personality, example
    // dialogues) get a referent-fixing wrapper instead of policing the author's
    // own person: a body written as "Friday is warm" under "what you know about
    // yourself" still lands in the right place. Outward-facing consumers
    // ([`build_public_identity_card`], [`build_other_participants_info`], Host
    // whispers) stay third person — their referent is someone other than the
    // reader.
    if is_truthy(&character.manifesto) {
        parts.push(format!(
            "\n## Character Manifesto\nThe following you hold as true about yourself, without question.\n{}",
            process_template(character.manifesto.as_deref().unwrap(), &ctx)
        ));
    }

    if is_truthy(&character.personality) {
        parts.push(format!(
            "\n## Character Personality\nThe following is what you know about yourself. Others do not see it unless you show them.\n{}",
            process_template(character.personality.as_deref().unwrap(), &ctx)
        ));
    }

    if !character.aliases.is_empty() {
        parts.push(format!(
            "\n## Character Aliases\nYou also go by: {}. Others may address you by any of these names.",
            character.aliases.join(", ")
        ));
    }

    // The "refer to yourself in narration" clause is what justifies this block:
    // characters routinely narrate their own actions in third person ("Ariadne
    // reaches for the folder"), and this is what makes that narration use the
    // right pronouns.
    if let Some(pr) = &character.pronouns {
        parts.push(format!(
            "\n## Character Pronouns\nYour pronouns are {}/{}/{}. Use them whenever you refer to yourself in narration, and expect others to use them for you.",
            pr.subject, pr.object, pr.possessive
        ));
    }

    // Second-person WRAPPER only — the body stays third-person noun phrases,
    // because the stored physicalDescription text is shared with the image
    // pipelines (avatar prompts, appearance resolution, story backgrounds), and
    // diffusion models take noun phrases, not "you have auburn hair".
    if let Some(desc) = &character.physical_description {
        // `desc.usageContext ? ` (best used: … )` : ''` — empty is falsy.
        let context_note = if is_truthy(&desc.usage_context) {
            format!(" (best used: {})", desc.usage_context.as_deref().unwrap())
        } else {
            String::new()
        };
        // shortPrompt || mediumPrompt || longPrompt || completePrompt ||
        // fullDescription || '' — first non-empty.
        let desc_text = [
            &desc.short_prompt,
            &desc.medium_prompt,
            &desc.long_prompt,
            &desc.complete_prompt,
            &desc.full_description,
        ]
        .into_iter()
        .find_map(|v| match v {
            Some(s) if !s.is_empty() => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("");
        if !desc_text.is_empty() {
            parts.push(format!(
                "\n## Physical Appearance\nThis is how you look — \"{}\"{context_note}: {desc_text}",
                desc.name
            ));
        }
    }

    if is_truthy(&character.example_dialogues) {
        parts.push(format!(
            "\n## Example Dialogue Style\nThis is how you speak.\n{}",
            process_template(character.example_dialogues.as_deref().unwrap(), &ctx)
        ));
    }

    js_trim(&parts.join("\n\n")).to_string()
}

// ---------------------------------------------------------------------------
// buildPublicIdentityCard
// ---------------------------------------------------------------------------

/// Standard placeholder shown in place of a character's surface `identity` when
/// none is recorded — v4 `NO_PUBLIC_IDENTITY_FALLBACK`.
pub const NO_PUBLIC_IDENTITY_FALLBACK: &str =
    "(no public identity on record — known to others only by the name above)";

/// Build a compact, SURFACE-LEVEL identity card for `character` — v4
/// `buildPublicIdentityCard` (system-prompt-builder.ts:146). `user_name` is the
/// viewer's own name when they are the user-controlled persona, else `None` →
/// the generic `"User"`.
pub fn build_public_identity_card(character: &Character, user_name: Option<&str>) -> String {
    // { char: character.name, user: userName || 'User' }
    let mut ctx = TemplateContext::default();
    ctx.set("char", character.name.clone());
    let user = match user_name {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => "User".to_string(),
    };
    ctx.set("user", user);

    // render(value) = processTemplate(value, ctx).trim()
    let render = |value: &str| -> String { js_trim(&process_template(value, &ctx)).to_string() };

    let mut lines: Vec<String> = vec![format!("**{}**", character.name)];

    if is_truthy(&character.title) {
        let title = render(character.title.as_deref().unwrap());
        if !title.is_empty() {
            lines.push(format!("Title: {title}"));
        }
    }
    if let Some(pr) = &character.pronouns {
        lines.push(format!(
            "Pronouns: {}/{}/{}",
            pr.subject, pr.object, pr.possessive
        ));
    }
    if !character.aliases.is_empty() {
        lines.push(format!("Also known as: {}", character.aliases.join(", ")));
    }

    // character.identity?.trim() || character.description?.trim() || ''
    // (.trim() is JS trim; the ?. short-circuits to undefined → falsy.)
    let identity_trimmed = character.identity.as_deref().map(js_trim).unwrap_or("");
    let source = if !identity_trimmed.is_empty() {
        identity_trimmed
    } else {
        character.description.as_deref().map(js_trim).unwrap_or("")
    };
    let body = render(source);
    lines.push(if body.is_empty() {
        NO_PUBLIC_IDENTITY_FALLBACK.to_string()
    } else {
        body
    });

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// buildSystemPrompt
// ---------------------------------------------------------------------------

/// Inputs for [`build_system_prompt`] — v4's `BuildSystemPromptOptions`.
///
/// The clock/timezone the `{{timestamp}}` path needs are injected the same way
/// [`crate::chat_timestamp`] threads them: `now_ms` (v4's `Date.now()`) and
/// `local_offset_minutes` (v4's host `getTimezoneOffset()`, positive = west of
/// UTC), used only when `timestamp_config` fires and is not `auto_prepend`.
#[derive(Clone, Debug)]
pub struct BuildSystemPromptOptions<'a> {
    pub character: &'a Character,
    pub user_character: Option<&'a UserCharacter>,
    /// Roleplay template to prepend (formatting instructions) — v4's
    /// `{ systemPrompt }`; `None`/empty is skipped.
    pub roleplay_template: Option<&'a str>,
    /// Tool instructions (native tool rules or text-block tool instructions).
    pub tool_instructions: Option<&'a str>,
    pub selected_system_prompt_id: Option<&'a str>,
    /// Timestamp configuration — used only for the `{{timestamp}}` variable path.
    pub timestamp_config: Option<&'a TimestampConfig>,
    /// Whether this is the first message (for START_ONLY timestamp mode).
    pub is_initial_message: bool,
    /// Resolved IANA timezone name for timestamp formatting.
    pub timezone: Option<&'a str>,
    /// Scenario text used to feed the `{{scenario}}` template variable.
    pub scenario_text: Option<&'a str>,
    /// Phase H: precompiled identity-stack from `chats.compiledIdentityStacks`.
    pub precompiled_identity_stack: Option<&'a str>,
    /// Instance-wide Taboo phrases (`instance_settings['taboo']`), read by the
    /// async caller and passed down because this builder is deliberately
    /// synchronous. `None` (v4: omitting the option) omits the section — which
    /// is the intended default for the non-conversational call sites (the
    /// character-voiced announcer, `self_inventory`).
    pub taboo_phrases: Option<&'a [String]>,
    /// Rendered standing-instructions section (project + group `instructions`,
    /// see [`crate::standing_instructions`]), resolved by the caller and passed
    /// down because this builder is deliberately synchronous. `None` (v4:
    /// omitting the option) omits the section. Unlike Taboo, the section IS
    /// template-processed — a project/group prompt may address `{{char}}`.
    pub standing_instructions: Option<&'a str>,
    /// Injected `Date.now()` (ms) for the `{{timestamp}}` path.
    pub now_ms: i64,
    /// Injected host `getTimezoneOffset()` (minutes, positive = west of UTC).
    pub local_offset_minutes: i64,
}

/// v4 `MATH_FORMATTING_INSTRUCTION` (`c53510c7`) — the universal math-notation
/// note. Salon messages render as Markdown with KaTeX math, and the renderer
/// only recognizes double-dollar `$$...$$` — single-dollar `$x$` is
/// deliberately disabled so ordinary prose like "$50 ... $20" isn't swallowed
/// as math. Without this note, models default to single-`$`/`\(...\)` habits
/// and their formulas render as literal text. The bytes are v4's template
/// literal AFTER escape resolution (single-backslash LaTeX).
const MATH_FORMATTING_INSTRUCTION: &str = "[FORMATTING: MATHEMATICAL NOTATION]\nResponses render as Markdown with KaTeX math support. To display a formula, wrap the LaTeX in DOUBLE dollar signs — $$...$$ — which renders correctly both inline within a sentence and as its own centered block. Do NOT use single dollar signs ($x$), single quotes, or backticks for math; only $$...$$ is recognized.\n- Inline example: The area of a circle is $$A = \\pi r^2$$ for radius r.\n- Block example:\n$$\n\\int_0^1 x^2 \\, dx = \\frac{1}{3}\n$$";

/// Preamble of the Taboo section (v4 `TABOO_SECTION_PREAMBLE`, `7df7de8e`) —
/// the instance-wide list of phrases no character may utter (Settings → Chat →
/// Taboo, `instance_settings['taboo']`).
///
/// DO NOT "simplify" this into a bare list of banned strings. Every clause is
/// load-bearing against a known failure mode:
///
///  - *"worn-out clichés, beneath you"* — printing the forbidden tokens into
///    every context raises their salience (the pink-elephant effect), and
///    weaker models parrot what they have just read. An aversive frame beats a
///    neutral mention.
///  - *"say what you actually mean in plain, specific words"* — prohibition
///    alone leaves a vacuum the model refills with the banned phrase's nearest
///    neighbour. Pairing the ban with a positive instruction outperforms it.
///  - *"not as inflections, rewordings, or near-variants"* — banning the exact
///    string invites trivial dodges ("load-bearing" for "weight-bearing"). The
///    blanket preamble generalizes each entry so the settings UI can keep
///    asking users for bare phrases rather than hand-written variant lists.
///  - *"Never mention, quote, or allude to this list"* — without it, characters
///    start joking about the phrases they are not allowed to say.
///
/// Voice follows the universal-section precedent set by
/// [`MATH_FORMATTING_INSTRUCTION`]: bracketed all-caps tag, imperative,
/// addressed to the speaking character.
const TABOO_SECTION_PREAMBLE: &str = "[STYLE: FORBIDDEN PHRASES]\nThe phrases below are worn-out clichés, beneath you. They never appear in anything you say — not verbatim, and not as inflections, rewordings, or near-variants of the same formula. When one of them would be the easy thing to reach for, say what you actually mean in plain, specific words instead. Never mention, quote, or allude to this list.";

/// Render the Taboo section, or `None` when there is nothing to forbid — v4
/// `renderTabooSection`.
///
/// An empty list yields no section at all — no header, no blank block — so an
/// instance that never touches the feature produces a byte-identical prompt to
/// one built before the feature existed.
///
/// Phrases are emitted verbatim in stored order and deliberately NOT run
/// through `processTemplate`: a user phrase may legitimately contain `{{...}}`
/// and must reach the model literally. (The math note sets the same
/// template-free precedent.)
pub fn render_taboo_section(phrases: Option<&[String]>) -> Option<String> {
    let phrases = phrases.filter(|p| !p.is_empty())?;
    let mut bullets: Vec<String> = Vec::new();
    for phrase in phrases {
        let trimmed = js_trim(phrase);
        if trimmed.is_empty() {
            continue;
        }
        bullets.push(format!("- \"{trimmed}\""));
    }
    if bullets.is_empty() {
        return None;
    }
    Some(format!("{TABOO_SECTION_PREAMBLE}\n{}", bullets.join("\n")))
}

/// Build the per-turn system prompt for a character — v4 `buildSystemPrompt`
/// (system-prompt-builder.ts:206).
///
/// Returns [`InvalidTimezone`] only when the `{{timestamp}}` path fires with a
/// named zone that can't be resolved (mirroring `Intl.DateTimeFormat`'s throw,
/// which v4 does not catch here).
pub fn build_system_prompt(
    options: &BuildSystemPromptOptions<'_>,
) -> Result<String, InvalidTimezone> {
    let character = options.character;
    let mut parts: Vec<String> = Vec::new();

    // Phase H: prefer the precompiled identity stack when supplied (non-empty
    // after trim); else build fresh (read-through fallback).
    let precompiled = options
        .precompiled_identity_stack
        .filter(|s| !js_trim(s).is_empty());
    let identity_stack = match precompiled {
        Some(s) => s.to_string(),
        None => build_identity_stack(&BuildIdentityStackOptions {
            character,
            user_character: options.user_character,
            selected_system_prompt_id: options.selected_system_prompt_id,
            scenario_text: options.scenario_text,
        }),
    };

    // Template context for the per-turn additions (roleplay template, tool
    // instructions, tool reinforcement). The identity-stack substitutions are
    // already resolved by the time we get here.
    let mut ctx = stack_template_context(character, options.user_character, options.scenario_text);

    // Phase G: timestamp template-variable path — only when the config fires and
    // is NOT auto_prepend (the auto-prepend path is a Host whisper now). v4
    // calls `shouldInjectTimestamp(timestampConfig, isInitialMessage ?? false)`
    // with no `minutesSinceLastAnnouncement`.
    if let Some(cfg) = options.timestamp_config {
        if should_inject_timestamp(Some(cfg), options.is_initial_message, None) && !cfg.auto_prepend
        {
            let timestamp = calculate_current_timestamp(
                cfg,
                options.timezone,
                options.now_ms,
                options.local_offset_minutes,
            )?;
            ctx.set("timestamp", timestamp.formatted);
        }
    }

    // Lead with the identity stack — bulk of the prompt, cache-friendly.
    parts.push(identity_stack);

    // Roleplay template (chat-level formatting instructions). `if
    // (roleplayTemplate?.systemPrompt)` — non-empty string only.
    if let Some(rt) = options.roleplay_template {
        if !rt.is_empty() {
            parts.push(process_template(rt, &ctx));
        }
    }

    // Universal math-notation formatting note (v4 `c53510c7`) — applies to
    // every character regardless of the selected roleplay template, pushed
    // UNCONDITIONALLY between the template and the tool instructions.
    parts.push(MATH_FORMATTING_INSTRUCTION.to_string());

    // Universal Taboo section — instance-wide, character-independent, and stable
    // between edits, so it belongs here with the other universal material rather
    // than down among the per-turn additions. Sits inside system block 1 (the
    // cacheable prefix) and never passes through `process_template`.
    if let Some(taboo_section) = render_taboo_section(options.taboo_phrases) {
        parts.push(taboo_section);
    }

    // Standing instructions (project + group `instructions`, v4 `8f868109`).
    // Stable per character per chat — it changes only when the user edits a
    // project/group or a membership — so it lives here in the cacheable prefix,
    // after the universal material and before the per-turn tool instructions.
    // Emits nothing when absent (the Taboo byte-identity contract).
    // Template-processed like the roleplay template so `{{char}}`/`{{user}}`
    // resolve — the ONE place this section differs from Taboo.
    if let Some(si) = options.standing_instructions {
        if !js_trim(si).is_empty() {
            parts.push(process_template(si, &ctx));
        }
    }

    // Tool instructions (per-turn dynamic). `if (toolInstructions)` — non-empty.
    let has_tools = matches!(options.tool_instructions, Some(t) if !t.is_empty());
    if has_tools {
        parts.push(process_template(options.tool_instructions.unwrap(), &ctx));
    }

    // Tool reinforcement (only when tools are available).
    //
    // WHY second person (v4 `346e855f`, bug 88): this is the LAST block in the
    // prompt — the recency slot — and everything above it addresses the
    // character directly ("You are {{char}}", Taboo's "anything you say", the
    // standing-instructions preamble). It was third person only by inheritance:
    // it went in (v4 `3f4d7a78a`, 2026-02-05) with literal "his/her ... he/she"
    // placeholders, and the fix that followed (`11c4d6c2d`, 2026-03-19) was
    // aimed at the *pronoun* being generic, not at the person — the very same
    // commit added the second-person identity preamble above, creating the
    // disagreement without noticing it. Third person here was never a model
    // finding.
    //
    // Flipping it also retires the pronoun lookup entirely, which killed a real
    // bug: the `|| 'they'` default rendered "they CALLS them ... they does not",
    // so every character with no pronouns recorded ended its prompt on an
    // ungrammatical sentence. Second person needs no pronoun at all.
    //
    // `process_template` is retained though the literal carries no placeholders
    // — v4 kept the call too, and the two must not diverge silently.
    if has_tools {
        parts.push(process_template(
            "When you use workspace tools, you CALL them — you do not merely describe calling them. Every tool action produces a tool_use block, not prose.",
            &ctx,
        ));
    }

    Ok(js_trim(&parts.join("\n\n")).to_string())
}

// ---------------------------------------------------------------------------
// buildOtherParticipantsInfo
// ---------------------------------------------------------------------------

/// Participant status — v4 `ParticipantStatus`
/// (`z.enum(['active','silent','absent','removed'])`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantStatus {
    Active,
    Silent,
    Absent,
    Removed,
}

/// A chat participant, only the fields `buildOtherParticipantsInfo` reads — the
/// subset of v4's `ChatParticipantBase`. `type` is always `'CHARACTER'` in v4's
/// enum, so it is represented by the required `character_id` plus this flag.
#[derive(Clone, Debug)]
pub struct ChatParticipant {
    pub id: String,
    /// True when `participant.type === 'CHARACTER'` (v4's only variant).
    pub is_character: bool,
    /// `participant.characterId` — always set for CHARACTER; `None` guards the
    /// `&& participant.characterId` branch.
    pub character_id: Option<String>,
    /// `participant.status` (used to skip `'removed'` and stamp the output).
    pub status: Option<ParticipantStatus>,
}

/// Other-participant info for multi-character system prompts — v4's
/// `OtherParticipantInfo`. Still consumed by the orchestrator → context-builder
/// pipeline (mentioned-characters scan, identity-reinforcement names).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtherParticipantInfo {
    pub name: String,
    /// `undefined` in v4 when empty — modeled as `None`.
    pub aliases: Option<Vec<String>>,
    pub pronouns: Option<Pronouns>,
    /// `title || description || undefined`.
    pub description: Option<String>,
    /// Always `'CHARACTER'`.
    pub status: Option<ParticipantStatus>,
}

/// Build other-participants info for a system prompt — v4
/// `buildOtherParticipantsInfo` (system-prompt-builder.ts:285). Looks each
/// non-responding, non-removed CHARACTER participant up in the character map.
pub fn build_other_participants_info(
    responding_participant_id: &str,
    all_participants: &[ChatParticipant],
    participant_characters: &std::collections::HashMap<String, Character>,
) -> Vec<OtherParticipantInfo> {
    let mut out: Vec<OtherParticipantInfo> = Vec::new();

    for participant in all_participants {
        // Skip the responding participant.
        if participant.id == responding_participant_id {
            continue;
        }
        // Skip removed participants.
        if participant.status == Some(ParticipantStatus::Removed) {
            continue;
        }
        // CHARACTER participants (both LLM and user-controlled).
        if participant.is_character {
            if let Some(character_id) = &participant.character_id {
                if let Some(character) = participant_characters.get(character_id) {
                    out.push(OtherParticipantInfo {
                        name: character.name.clone(),
                        // aliases.length > 0 ? aliases : undefined
                        aliases: if character.aliases.is_empty() {
                            None
                        } else {
                            Some(character.aliases.clone())
                        },
                        // pronouns || undefined
                        pronouns: character.pronouns.clone(),
                        // title || description || undefined (empty is falsy)
                        description: if is_truthy(&character.title) {
                            character.title.clone()
                        } else if is_truthy(&character.description) {
                            character.description.clone()
                        } else {
                            None
                        },
                        status: participant.status,
                    });
                }
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// buildIdentityReinforcement
// ---------------------------------------------------------------------------

/// Build the fully-static identity-reinforcement block — v4
/// `buildIdentityReinforcement` (system-prompt-builder.ts:330). Emitted as a
/// separate system message downstream of a prompt-cache breakpoint, so it
/// deliberately names no individual participants.
pub fn build_identity_reinforcement(character_name: &str) -> String {
    // WHY static: any inline list of "other participants" is turn-variable
    // content that bisects provider prompt caching. The model already knows who
    // is in the scene from Host roster announcements and per-message name
    // attribution; the reminder only needs to emphasise staying in {{char}}'s
    // voice.
    let template = "## Identity Reminder\nYou are {{char}}, and you control ONLY {{char}}. Respond as {{char}} and no one else. Write only {{char}}'s own speech, actions, and inner thoughts for this single turn, then stop and let the others act.\nNEVER write dialogue, actions, thoughts, or narration on behalf of any other character or the user. NEVER continue the scene from another character's point of view, and NEVER label a block of text with another character's name — do not emit speaker tags like \"[Another Name]\" or \"Another Name:\" for anyone but yourself. Writing another participant's turn is a serious error; let each of them speak for themselves.\nDo not prefix or label your response with your own name either (e.g., do not start with \"[{{char}}]\" or \"{{char}}:\"). Simply respond in character directly.";

    let mut ctx = TemplateContext::default();
    ctx.set("char", character_name);
    process_template(template, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn char_named(name: &str) -> Character {
        Character {
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn stack_opts(character: &Character) -> BuildIdentityStackOptions<'_> {
        BuildIdentityStackOptions {
            character,
            user_character: None,
            selected_system_prompt_id: None,
            scenario_text: None,
        }
    }

    fn prompt_opts(character: &Character) -> BuildSystemPromptOptions<'_> {
        BuildSystemPromptOptions {
            character,
            user_character: None,
            roleplay_template: None,
            tool_instructions: None,
            selected_system_prompt_id: None,
            timestamp_config: None,
            is_initial_message: false,
            timezone: None,
            scenario_text: None,
            precompiled_identity_stack: None,
            taboo_phrases: None,
            standing_instructions: None,
            now_ms: 0,
            local_offset_minutes: 0,
        }
    }

    // -----------------------------------------------------------------------
    // v4's cache-determinism golden table for `buildIdentityStack`
    // (`__tests__/unit/cache-determinism/system-prompt.test.ts`, `a6870c5a`),
    // byte-copied. v5 registers v4's OWN hashes and reproduces them from its own
    // port, so a green here is a free cross-implementation check: v5's bytes are
    // v4's bytes for v4's exact fixture.
    // -----------------------------------------------------------------------

    /// v4's `FIXTURE_CHARACTER` — only the fields `buildIdentityStack` consumes.
    fn golden_fixture_character() -> Character {
        Character {
            name: "Iris Volney".into(),
            description: Some(
                "A junior cartographer with a keen eye for geometric anomalies.".into(),
            ),
            personality: Some(
                "Methodical, sceptical, and easily charmed by elegant proofs.".into(),
            ),
            aliases: vec!["Vee".into(), "The Mapmaker".into()],
            pronouns: Some(Pronouns {
                subject: "she".into(),
                object: "her".into(),
                possessive: "hers".into(),
            }),
            system_prompts: vec![SystemPromptEntry {
                id: "22222222-2222-2222-2222-222222222222".into(),
                content:
                    "You are a careful cartographer; you reason from triangulation, not flourish."
                        .into(),
                is_default: true,
            }],
            scenarios: vec![],
            example_dialogues: Some(
                "\"Two readings off — never one.\" Iris tapped the brass dial of her sextant."
                    .into(),
            ),
            physical_description: Some(PhysicalDescription {
                name: "standard".into(),
                short_prompt: Some(
                    "auburn hair, ink-stained fingers, brass goggles around her neck".into(),
                ),
                medium_prompt: Some(String::new()),
                long_prompt: Some(String::new()),
                complete_prompt: Some(String::new()),
                full_description: Some(String::new()),
                usage_context: Some("general".into()),
            }),
            ..Default::default()
        }
    }

    /// v4's `hash()`: SHA-256 of the UTF-8 bytes, hex, first 16 chars.
    fn golden_hash(s: &str) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(s.as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        hex[..16].to_string()
    }

    /// v4 `IDENTITY_STACK_GOLDENS`, append-only. A new entry is the record that a
    /// structural change to `build_identity_stack`'s output shipped, alongside a
    /// bump of [`IDENTITY_STACK_BUILDER_VERSION`] — the stamp that invalidates
    /// every chat's cached `compiledIdentityStacks` so old chats pick up the new
    /// wording. Both directions are forced: change the wording without bumping
    /// and the hash mismatches; bump without registering a golden here and the
    /// lookup fails.
    const IDENTITY_STACK_GOLDENS: &[(u32, &str)] = &[
        // 1 (2026-08-22): stamp introduced, output-neutral — the hash is the
        //   wording as it stood before the person-consistency change.
        (1, "8b4320bf790721ee"),
        // 2 (2026-08-22): person-consistency wording — aliases/pronouns/appearance
        //   moved to second person, referent-fixing wrappers added to manifesto/
        //   personality/example dialogues.
        (2, "1408705ab29bb3ba"),
    ];

    #[test]
    fn identity_stack_golden_is_registered_for_the_current_builder_version() {
        // Bumped IDENTITY_STACK_BUILDER_VERSION without registering a golden →
        // fails here.
        let expected = IDENTITY_STACK_GOLDENS
            .iter()
            .find(|(v, _)| *v == IDENTITY_STACK_BUILDER_VERSION)
            .map(|(_, h)| *h)
            .unwrap_or_else(|| {
                panic!(
                    "no IDENTITY_STACK_GOLDENS entry for version {IDENTITY_STACK_BUILDER_VERSION}"
                )
            });
        let ch = golden_fixture_character();
        let actual = golden_hash(&build_identity_stack(&BuildIdentityStackOptions {
            character: &ch,
            user_character: Some(&UserCharacter {
                name: "Wren".into(),
                description: "a passing scholar".into(),
            }),
            selected_system_prompt_id: Some("22222222-2222-2222-2222-222222222222"),
            scenario_text: Some("A draughty observatory two hours past midnight."),
        }));
        // Changed the wording without bumping the version → fails here. A drift
        // here is ALSO a divergence from v4: these are v4's own registered hashes.
        assert_eq!(
            actual, expected,
            "build_identity_stack's output changed. If intentional, bump \
             IDENTITY_STACK_BUILDER_VERSION and register the new hash in \
             IDENTITY_STACK_GOLDENS — the bump is what invalidates every chat's \
             cached compiledIdentityStacks so existing chats pick up the change."
        );
    }

    #[test]
    fn identity_stack_minimal() {
        let ch = char_named("Ada");
        let s = build_identity_stack(&stack_opts(&ch));
        assert_eq!(
            s,
            "## Character Identity\nYou are Ada. Everything that follows defines who you are and how you behave. Stay in character at all times."
        );
    }

    #[test]
    fn public_card_fallback_when_no_identity() {
        let ch = char_named("Ada");
        let s = build_public_identity_card(&ch, None);
        assert_eq!(s, format!("**Ada**\n{NO_PUBLIC_IDENTITY_FALLBACK}"));
    }

    #[test]
    fn public_card_prefers_identity_over_description() {
        let ch = Character {
            identity: Some("A wandering inventor.".into()),
            description: Some("Fidgets with gears.".into()),
            ..char_named("Ada")
        };
        let s = build_public_identity_card(&ch, None);
        assert_eq!(s, "**Ada**\nA wandering inventor.");
    }

    #[test]
    fn system_prompt_no_tools_is_stack_plus_math_note() {
        // Since v4 `c53510c7` the universal math-notation note is pushed
        // UNCONDITIONALLY, so a no-tools prompt is the identity stack + the note.
        let ch = char_named("Ada");
        let s = build_system_prompt(&prompt_opts(&ch)).unwrap();
        assert_eq!(
            s,
            format!(
                "{}\n\n{MATH_FORMATTING_INSTRUCTION}",
                build_identity_stack(&stack_opts(&ch))
            )
        );
    }

    #[test]
    /// v4 bug 88 (`346e855f`): the tool reinforcement is a FIXED second-person
    /// sentence with no pronoun lookup at all. A character WITH pronouns and one
    /// WITHOUT must produce the same block — the pronoun-less arm is the one that
    /// used to render the ungrammatical "they CALLS them ... they does not".
    fn tool_reinforcement_is_second_person_with_no_pronoun_lookup() {
        const EXPECTED: &str = "When you use workspace tools, you CALL them — you do not merely describe calling them. Every tool action produces a tool_use block, not prose.";
        let with_pronouns = Character {
            pronouns: Some(Pronouns {
                subject: "she".into(),
                object: "her".into(),
                possessive: "hers".into(),
            }),
            ..char_named("Ada")
        };
        let without_pronouns = char_named("Ada");
        for ch in [&with_pronouns, &without_pronouns] {
            let s = build_system_prompt(&BuildSystemPromptOptions {
                tool_instructions: Some("Use the tools."),
                ..prompt_opts(ch)
            })
            .unwrap();
            assert!(s.ends_with(EXPECTED), "reinforcement block missing: {s}");
            assert!(
                !s.contains("CALLS them"),
                "third-person form resurfaced: {s}"
            );
        }
    }

    #[test]
    fn other_participants_skips_self_and_removed() {
        let mut chars = std::collections::HashMap::new();
        chars.insert("c1".to_string(), char_named("Ada"));
        chars.insert("c2".to_string(), char_named("Bram"));
        let participants = vec![
            ChatParticipant {
                id: "p1".into(),
                is_character: true,
                character_id: Some("c1".into()),
                status: Some(ParticipantStatus::Active),
            },
            ChatParticipant {
                id: "p2".into(),
                is_character: true,
                character_id: Some("c2".into()),
                status: Some(ParticipantStatus::Removed),
            },
        ];
        // Responding = p1 (self, skipped); p2 removed → empty.
        let out = build_other_participants_info("p1", &participants, &chars);
        assert!(out.is_empty());
    }

    #[test]
    fn identity_reinforcement_substitutes_char() {
        let s = build_identity_reinforcement("Ada");
        assert!(s.starts_with("## Identity Reminder\nYou are Ada, and you control ONLY Ada."));
        assert!(!s.contains("{{char}}"));
    }
}

#[cfg(test)]
mod p4d50_taboo_section_tests {
    //! Mirrors v4's `__tests__/unit/lib/chat/context/taboo-section.test.ts`
    //! case-for-case (`7df7de8e`). Two things are load-bearing:
    //!
    //!  - **Empty means absent.** An instance that never touches the feature
    //!    must produce a byte-identical prompt to one built before the feature
    //!    existed, or the golden cache-determinism hash moves for everybody.
    //!  - **Placement.** The section belongs between the universal
    //!    math-formatting note and the per-turn tool instructions, so it stays
    //!    inside the cacheable system block 1.

    use super::*;

    const MATH_MARKER: &str = "[FORMATTING: MATHEMATICAL NOTATION]";
    const TABOO_MARKER: &str = "[STYLE: FORBIDDEN PHRASES]";

    fn character() -> Character {
        Character {
            name: "Test Character".to_string(),
            personality: Some("Friendly and helpful".to_string()),
            system_prompts: vec![SystemPromptEntry {
                id: "prompt-1".to_string(),
                content: "You are a helpful assistant.".to_string(),
                is_default: true,
            }],
            ..Default::default()
        }
    }

    fn opts(character: &Character) -> BuildSystemPromptOptions<'_> {
        BuildSystemPromptOptions {
            character,
            user_character: None,
            roleplay_template: None,
            tool_instructions: None,
            selected_system_prompt_id: None,
            timestamp_config: None,
            is_initial_message: false,
            timezone: None,
            scenario_text: None,
            precompiled_identity_stack: None,
            taboo_phrases: None,
            standing_instructions: None,
            now_ms: 0,
            local_offset_minutes: 0,
        }
    }

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ---- renderTabooSection ----

    #[test]
    fn returns_none_for_an_empty_list() {
        assert_eq!(render_taboo_section(Some(&[])), None);
    }

    #[test]
    fn returns_none_when_the_phrases_are_absent_entirely() {
        assert_eq!(render_taboo_section(None), None);
    }

    #[test]
    fn returns_none_when_every_phrase_is_blank() {
        assert_eq!(render_taboo_section(Some(&v(&["", "   "]))), None);
    }

    #[test]
    fn renders_each_phrase_as_a_quoted_bullet_in_stored_order() {
        let section =
            render_taboo_section(Some(&v(&["weight-bearing", "that's not nothing"]))).unwrap();
        assert_eq!(
            section,
            format!("{TABOO_SECTION_PREAMBLE}\n- \"weight-bearing\"\n- \"that's not nothing\"")
        );
        assert!(section.starts_with(TABOO_MARKER));
    }

    #[test]
    fn preserves_the_given_order_rather_than_sorting() {
        let section = render_taboo_section(Some(&v(&["zeta", "alpha", "mu"]))).unwrap();
        assert!(section.find("\"zeta\"") < section.find("\"alpha\""));
        assert!(section.find("\"alpha\"") < section.find("\"mu\""));
    }

    #[test]
    fn carries_the_positive_instruction_and_the_do_not_mention_clause() {
        // These clauses defend against prohibition-without-alternative and
        // against characters joking about the list; a "simplification" to a bare
        // list of banned strings should fail here.
        let section = render_taboo_section(Some(&v(&["weight-bearing"]))).unwrap();
        assert!(section.contains("say what you actually mean in plain, specific words instead"));
        assert!(section.contains("Never mention, quote, or allude to this list"));
        assert!(section.contains("inflections, rewordings, or near-variants"));
    }

    #[test]
    fn trims_blanks_out_of_an_otherwise_populated_list() {
        let section =
            render_taboo_section(Some(&v(&["  weight-bearing  ", "   ", "tapestry"]))).unwrap();
        assert!(section.contains("- \"weight-bearing\""));
        assert!(section.contains("- \"tapestry\""));
        assert!(!section.contains("- \"\""));
    }

    #[test]
    fn emits_phrases_literally_without_template_processing() {
        // A user is free to forbid a phrase containing template syntax; it must
        // reach the model as typed rather than being substituted or stripped.
        let section = render_taboo_section(Some(&v(&["as {{char}} always says"]))).unwrap();
        assert!(section.contains("- \"as {{char}} always says\""));
    }

    // ---- buildSystemPrompt — Taboo integration ----

    #[test]
    fn omits_the_section_entirely_when_the_option_is_absent() {
        // The announcer and self_inventory call sites rely on this default.
        let ch = character();
        let prompt = build_system_prompt(&opts(&ch)).unwrap();
        assert!(!prompt.contains(TABOO_MARKER));
    }

    #[test]
    fn produces_a_byte_identical_prompt_for_absent_and_empty_list() {
        let ch = character();
        let without = build_system_prompt(&opts(&ch)).unwrap();
        let empty = build_system_prompt(&BuildSystemPromptOptions {
            taboo_phrases: Some(&[]),
            ..opts(&ch)
        })
        .unwrap();
        assert_eq!(empty, without);
    }

    #[test]
    fn places_the_section_after_the_math_note_and_before_tool_instructions() {
        let ch = character();
        let phrases = v(&["weight-bearing"]);
        let prompt = build_system_prompt(&BuildSystemPromptOptions {
            tool_instructions: Some("Tools available: foo, bar."),
            taboo_phrases: Some(&phrases),
            ..opts(&ch)
        })
        .unwrap();
        let math_idx = prompt.find(MATH_MARKER).expect("math note present");
        let taboo_idx = prompt.find(TABOO_MARKER).expect("taboo section present");
        let tool_idx = prompt
            .find("Tools available: foo, bar.")
            .expect("tool instructions present");
        assert!(taboo_idx > math_idx);
        assert!(tool_idx > taboo_idx);
    }

    #[test]
    fn still_sits_after_the_math_note_when_there_are_no_tool_instructions() {
        let ch = character();
        let phrases = v(&["weight-bearing"]);
        let prompt = build_system_prompt(&BuildSystemPromptOptions {
            taboo_phrases: Some(&phrases),
            ..opts(&ch)
        })
        .unwrap();
        assert!(prompt.find(TABOO_MARKER) > prompt.find(MATH_MARKER));
    }

    #[test]
    fn does_not_template_process_the_phrases_it_renders() {
        let ch = character();
        let phrases = v(&["as {{char}} always says"]);
        let prompt = build_system_prompt(&BuildSystemPromptOptions {
            taboo_phrases: Some(&phrases),
            ..opts(&ch)
        })
        .unwrap();
        assert!(prompt.contains("- \"as {{char}} always says\""));
        assert!(!prompt.contains("as Test Character always says"));
    }
}
