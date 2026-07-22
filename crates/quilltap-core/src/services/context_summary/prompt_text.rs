//! GENERATED — the verbatim prompt-body text of v4
//! `lib/memory/cheap-llm-tasks/chat-tasks.ts` (`FOLD_SUMMARY_PROMPT`,
//! `CHAT_TITLE_FROM_SUMMARY_PROMPT`, `HELP_CHAT_TITLE_FROM_SUMMARY_PROMPT`, and
//! — added by P4.6ao for the TITLE_UPDATE handler —
//! `CHAT_TITLE_CONSIDERATION_PROMPT` + `HELP_CHAT_TITLE_CONSIDERATION_PROMPT`).
//! Extracted mechanically (no byte transcribed by hand); the tier-3
//! differential proves the bytes reach the provider unchanged. Regenerate by
//! re-running the session extraction against the v4 checkout if the prompts change.

pub(crate) const FOLD_SUMMARY_PROMPT: &str = r#"You are updating an existing summary of an ongoing roleplay conversation.

The summary tracks five sections:

- Active threads: what is currently in motion. Carry forward anything still unresolved. Drop threads that have closed. Add new ones from the new turns.
- Resolved decisions: things now locked. Carry forward all prior entries — these don't unresolve. Add anything new the characters have committed to.
- Emotional state: where the room is right now, not the journey. Replace the prior Emotional state entirely with the current state.
- Open questions: unanswered things in the air. Drop any the new turns answered. Carry forward the rest. Add new ones.
- Timeline: dated one-liners of specific things that HAPPENED — visits, outings, arrivals, purchases, incidents. Format each line as "- YYYY-MM-DD (narrative: 'in-story time', only if the story runs on its own timeline): what happened, naming place and participants". Dates come from the timestamps on the new turns. APPEND-ONLY: carry forward every prior Timeline line unchanged and add new lines at the bottom. Cap at ~30 lines — when over, merge the OLDEST lines into coarser one-liners (never drop the dates). Standing facts, moods, and decisions do not belong here — only events.

Rewrite the five sections in plain prose (the Timeline as its dated list). Be concise. Don't transcribe — synthesize. Use character names, not roles. Output only the five sections under their labels; no preamble, no closing remarks."#;

pub(crate) const CHAT_TITLE_FROM_SUMMARY_PROMPT: &str = r#"Generate a literary title for this conversation based on the summary provided, like titling a short story.
The title should:
- Be 3-8 words maximum and under 60 characters
- Focus on the unique narrative flair, quirky elements, or evocative mood
- Be poetic and distinctive — unless the conversation is really about technical details, in which case mention the kind of technical work being discussed
- Avoid moralistic, ethical, or sterile phrasing — no mentions of consent, boundaries, or lessons
- Unless the conversation is really about one specific character, avoid titling it by character name

Respond with only the title, no quotes or additional text."#;

pub(crate) const HELP_CHAT_TITLE_FROM_SUMMARY_PROMPT: &str = r#"Generate a short, practical title for this help/support conversation based on the summary provided.
The title should:
- Be 3-10 words maximum and under 60 characters
- Clearly describe what question was asked or what topic was discussed
- Be specific enough that someone scanning a list can find it later
- Use plain, descriptive language — no literary flair, no metaphors, no poetic phrasing

Respond with only the title, no quotes or additional text."#;

/// v4 `CHAT_TITLE_CONSIDERATION_PROMPT` — the literary title-evaluator system
/// prompt `considerTitleUpdate` sends.
pub(crate) const CHAT_TITLE_CONSIDERATION_PROMPT: &str = r#"You are a chat title evaluator. You will be given:
1. The current chat title
2. A previous summary or title (if available)
3. Recent messages from the chat

Determine if the chat needs a new title. Consider:
- If the current title is generic (like "Chat with [Name]" or "New Chat"), it SHOULD be replaced with a literary title based on what the conversation is actually about
- If the title is already descriptive, only suggest a change if the main topic has shifted significantly
- A good title is like titling a short story: it captures the unique narrative flair, quirky elements, or evocative mood in a few words
- Titles should be poetic and distinctive, not moralistic or sterile — avoid mentions of consent, boundaries, or lessons. Exception: if the conversation is really about technical details, the title should mention the kind of technical work being discussed
- Unless the conversation is really about one specific character, avoid titling it by character name

Respond with a JSON object:
{
  "needsNewTitle": true/false,
  "reason": "brief explanation",
  "suggestedTitle": "new title if needsNewTitle is true, otherwise null"
}

Keep suggested titles 3-8 words, under 60 characters, poetic and distinctive (or technically descriptive if the conversation is about technical work)."#;

/// v4 `HELP_CHAT_TITLE_CONSIDERATION_PROMPT` — the practical (non-literary)
/// evaluator `considerHelpChatTitleUpdate` sends for help-like chats.
pub(crate) const HELP_CHAT_TITLE_CONSIDERATION_PROMPT: &str = r#"You are a help chat title evaluator. You will be given:
1. The current chat title
2. A previous summary or title (if available)
3. Recent messages from the chat

Determine if the chat needs a new title. Consider:
- If the current title is generic (like "Help: [Name]" or "New Chat"), it SHOULD be replaced with a descriptive title about what the user actually asked
- If the title is already descriptive of the question/topic, only suggest a change if the main topic has shifted significantly
- A good help chat title clearly describes the question or topic so someone can find it in a list later
- Titles should be plain and practical — no literary flair, metaphors, or poetic phrasing

Respond with a JSON object:
{
  "needsNewTitle": true/false,
  "reason": "brief explanation",
  "suggestedTitle": "new title if needsNewTitle is true, otherwise null"
}

Keep suggested titles 3-10 words, under 60 characters, plain and descriptive."#;
