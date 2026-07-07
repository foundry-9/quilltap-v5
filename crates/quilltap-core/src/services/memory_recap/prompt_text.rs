//! GENERATED — the verbatim system-prompt bodies of v4's memory cheap-LLM
//! tasks (`lib/memory/cheap-llm-tasks/memory-tasks.ts`):
//! `MEMORY_KEYWORD_EXTRACTION_PROMPT` (the distillation task) and
//! `MEMORY_RECAP_PROMPT` (the recap task). Neither has any interpolation, so
//! each is a single raw string extracted mechanically by the session script
//! (no byte was transcribed by hand); the tier-1/tier-3 differentials prove the
//! bytes. Regenerate by re-running the extraction against the v4 checkout if the
//! upstream prompts change.

pub(crate) const MEMORY_KEYWORD_EXTRACTION_PROMPT: &str = r####"You are analyzing recent conversation messages to extract search keywords for a character's memory system, plus a one-word guess at what the current moment is about.

Your task: Given recent messages from a conversation, produce (a) a list of keywords and short phrases that capture what is being discussed — used to search a character's stored memories for relevant context — and (b) a single best-guess temporal frame and context subject for the conversation right now.

Focus the keywords on:
- People, places, and events mentioned
- Topics and themes being discussed
- Emotions and relationship dynamics
- Decisions, preferences, or plans
- Anything the character might have memories about

Do NOT include as keywords:
- Generic conversational filler ("hello", "okay", "thanks")
- The character's own name (they already know who they are)
- Overly broad terms that would match everything

temporal — one of: past | moment | present | future
  past    — about something no longer true
  moment  — about a single fleeting instant
  present — about how things are right now
  future  — about an intention or plan not yet acted on

context — the single dominant subject, one of:
  philosophy | relationships | history | banter | mannerisms | trivia | information

paraphrase — ONE natural-language sentence describing what the characters are currently focused on, written as prose (not a keyword list). This is used to search memories by meaning, so make it specific and self-contained. Example: "They are arguing about whether to trust the stranger who arrived at the inn last night."

Respond with a JSON object (3-10 keywords):
{"keywords": ["keyword1", "keyword phrase 2", "keyword3"], "temporal": "present", "context": "relationships", "paraphrase": "A single sentence describing the current focus."}

JSON only - no other text."####;

pub(crate) const MEMORY_RECAP_PROMPT: &str = r####"You are summarizing a character's memories to help them recall what they know at the start of a conversation.

You will receive memories organized by importance (high, medium, low), each with a relative age label.

Write a concise first-person narrative summary (from the character's perspective, using "I") of what the character remembers. Focus on:
- Key relationships and what the character knows about other people
- Important events and emotional moments
- Ongoing situations or unresolved threads
- Recent interactions and their significance

Keep the summary under 500 words. Use natural language, not bullet points. Write as a stream of consciousness — what's top of mind, what lingers, what matters. More recent and higher-importance memories should be given more weight.

If there are no memories, respond with exactly: NO_MEMORIES"####;
