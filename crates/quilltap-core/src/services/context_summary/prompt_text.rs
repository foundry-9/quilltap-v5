//! GENERATED — the verbatim prompt-body text of v4
//! `lib/memory/cheap-llm-tasks/chat-tasks.ts` (`FOLD_SUMMARY_PROMPT`,
//! `CHAT_TITLE_FROM_SUMMARY_PROMPT`, `HELP_CHAT_TITLE_FROM_SUMMARY_PROMPT`).
//! Extracted mechanically (no byte transcribed by hand); the tier-3
//! differential proves the bytes reach the provider unchanged. Regenerate by
//! re-running the session extraction against the v4 checkout if the prompts change.

pub(crate) const FOLD_SUMMARY_PROMPT: &str = r#"You are updating an existing summary of an ongoing roleplay conversation.

The summary tracks four sections:

- Active threads: what is currently in motion. Carry forward anything still unresolved. Drop threads that have closed. Add new ones from the new turns.
- Resolved decisions: things now locked. Carry forward all prior entries — these don't unresolve. Add anything new the characters have committed to.
- Emotional state: where the room is right now, not the journey. Replace the prior Emotional state entirely with the current state.
- Open questions: unanswered things in the air. Drop any the new turns answered. Carry forward the rest. Add new ones.

Rewrite the four sections in plain prose. Be concise. Don't transcribe — synthesize. Use character names, not roles. Output only the four sections under their labels; no preamble, no closing remarks."#;

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
