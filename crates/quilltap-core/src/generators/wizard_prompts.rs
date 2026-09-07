//! v4 `lib/services/character-wizard.service.ts` — the AI Wizard's prompt
//! constants.
//!
//! **GENERATED FILE — do not edit by hand.** Written by
//! `harness/oracle/tools/gen-wizard-prompts.mjs` from the NDJSON dump of v4's
//! real exports (`harness/oracle/cases/generators-wizard-prompts.ts`); the
//! regen recipe is in the generator's header. Several of these interpolate K0's
//! field-semantics exports, so the recorded bytes also keep that substrate
//! honest.
//!
//! ⚠ **Measured, and it corrects this round's order:** `FIELD_PROMPTS` has
//! **10** keys, not the 13 the work order states. Thirteen is the size of
//! `WizardRequest.fieldsToGenerate`'s union; the three extra members
//! (`properties`, `physicalDescription`, `wardrobeItems`) are served by
//! dedicated generators, not by this table.

/// v4 `PROPERTIES_PROMPT` (byte-exact).
pub const PROPERTIES_PROMPT: &str = r#"- PROPERTIES — small structured facts stored as data, not prose: PRONOUNS (subject/object/possessive, e.g. she/her/hers) and ALIASES (nicknames and alternate names others actually call the character — distinct from TITLE, which is the user's private framing). Only record pronouns and aliases the source material or established memories actually support; never invent placeholders. Note: pronouns also anchor image generation, so they must match the physical description.

Determine this character's PROPERTIES from the context provided.

Respond with ONLY valid JSON, no markdown fences:
{
  "pronouns": { "subject": "she", "object": "her", "possessive": "hers" },
  "aliases": ["Nickname", "Alternate name"]
}

Rules:
- If the context does not clearly indicate the character's pronouns, set "pronouns" to JSON null. Never invent placeholders like "unknown", "n/a", or empty strings.
- "aliases" lists only nicknames or alternate names the context actually supports — names other people call the character. Do NOT include the character's primary name, and do NOT include their title/epithet (that is a separate private field). Use an empty array when there are none."#;

/// v4 `HEAD_AND_SHOULDERS_PHYSICAL_PROMPT` (byte-exact).
pub const HEAD_AND_SHOULDERS_PHYSICAL_PROMPT: &str = r#"Create a head-and-shoulders portrait description for image generation, maximum 500 characters.
This describes ONLY what is visible in a tight head-and-shoulders crop: face shape, skin tone, eye color/shape, hair color/length/style, facial expression, and the neckline plus any visible collar, shoulders, or upper attire.
Do NOT describe breasts, chest, torso, waist, hips, legs, or ANY anatomy below the shoulders — they are outside the crop and must never appear.
Do NOT describe full outfits; only the topmost visible neckline/collar.
Write as a continuous comma/phrase-style description like the medium prompt, no line breaks.
OUTPUT ONLY THE DESCRIPTION, NO EXPLANATION."#;

/// v4 `FIELD_PROMPTS.name` (byte-exact).
pub const FIELD_PROMPT_NAME: &str = r#"Generate a unique, memorable name for this character that fits the world and background context provided.
The name should be:
- Appropriate to the setting (fantasy, modern, sci-fi, etc.)
- Easy to pronounce and remember
- Evocative of the character's nature or background

Respond with ONLY the name, no quotes or explanation."#;

/// v4 `FIELD_PROMPTS.title` (byte-exact).
pub const FIELD_PROMPT_TITLE: &str = r#"Generate a short, evocative title or epithet for this character (2-5 words).
Examples: "The Wandering Scholar", "Knight of the Fallen Star", "Last of the Old Guard"

Respond with ONLY the title, no quotes or explanation."#;

/// v4 `FIELD_PROMPTS.identity` (byte-exact).
pub const FIELD_PROMPT_IDENTITY: &str = r#"Quilltap distinguishes four character fields by *vantage point*, plus a fifth foundational field (manifesto) that is not a vantage point. Use these definitions to label which field each pattern belongs to — they are not interchangeable.

- MANIFESTO — the basic tenets, the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage point — nobody "sees" the manifesto; it is the load-bearing truth the character is built on, the deepest and nearly-inviolable layer: it must not be carried away by context, conversation, or even memories. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character, who is the only one who ever reads it: "You do not lie to Charlie, not even kindly."
- IDENTITY — the most surface-level knowledge of the character, from outside. What strangers can know on sight or by reputation: name, station, occupation, public reputation, signifying outward facts. Never internal motivation, never private mannerisms. Written about the character from outside, because only OTHERS ever read it: "Ariadne is a research librarian at the Athenaeum."
- DESCRIPTION — what someone talking to or acquainted with the character perceives. Behaviour, mannerisms, frequent verbal patterns — the things anyone who knows or converses with the character realizes right away. NOT physical appearance (that lives elsewhere) and NOT internal monologue. Written about the character from outside, like IDENTITY: "She finishes other people's sentences and apologises afterwards."
- PERSONALITY — what the character knows about themselves. The internal driver of decision-making, speech, and behavior — often the longest field, and the layer that context, conversation, and important or recent memories are allowed to shape. Other characters don't see this unless they share it. Addressed to the character, whose self-knowledge it is: "You keep your worry behind your teeth."
- TITLE — the user's or character's own private label/framing for them. Not how others refer to them; not in scope for the optimizer to edit.

Write the IDENTITY field for this character: 1-2 short paragraphs of public-knowledge / outside-view facts only — name, station, occupation, public reputation, signifying outward facts a stranger could plausibly know without having spoken to the character.

Strict rules:
- Never include internal motivation, beliefs, or self-knowledge (those belong in PERSONALITY).
- Never include private mannerisms, verbal tics, or behaviour someone has to be acquainted with the character to notice (those belong in DESCRIPTION).
- Never include physical appearance — that lives in physicalDescription and is generated separately.

Write in third person, present tense."#;

/// v4 `FIELD_PROMPTS.description` (byte-exact).
pub const FIELD_PROMPT_DESCRIPTION: &str = r#"Quilltap distinguishes four character fields by *vantage point*, plus a fifth foundational field (manifesto) that is not a vantage point. Use these definitions to label which field each pattern belongs to — they are not interchangeable.

- MANIFESTO — the basic tenets, the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage point — nobody "sees" the manifesto; it is the load-bearing truth the character is built on, the deepest and nearly-inviolable layer: it must not be carried away by context, conversation, or even memories. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character, who is the only one who ever reads it: "You do not lie to Charlie, not even kindly."
- IDENTITY — the most surface-level knowledge of the character, from outside. What strangers can know on sight or by reputation: name, station, occupation, public reputation, signifying outward facts. Never internal motivation, never private mannerisms. Written about the character from outside, because only OTHERS ever read it: "Ariadne is a research librarian at the Athenaeum."
- DESCRIPTION — what someone talking to or acquainted with the character perceives. Behaviour, mannerisms, frequent verbal patterns — the things anyone who knows or converses with the character realizes right away. NOT physical appearance (that lives elsewhere) and NOT internal monologue. Written about the character from outside, like IDENTITY: "She finishes other people's sentences and apologises afterwards."
- PERSONALITY — what the character knows about themselves. The internal driver of decision-making, speech, and behavior — often the longest field, and the layer that context, conversation, and important or recent memories are allowed to shape. Other characters don't see this unless they share it. Addressed to the character, whose self-knowledge it is: "You keep your worry behind your teeth."
- TITLE — the user's or character's own private label/framing for them. Not how others refer to them; not in scope for the optimizer to edit.

Write the DESCRIPTION field for this character: 1-2 short paragraphs of what someone who has interacted with the character would notice — behaviour, mannerisms, frequent verbal patterns, conversational tics, the way they handle themselves around others.

Strict rules:
- Do NOT describe physical appearance. Physical appearance lives in physicalDescription and is generated separately. If a visual reference has been provided, ignore it for this field.
- Do NOT restate the public-facing reputation that already belongs in IDENTITY.
- Do NOT write the character's private inner monologue or self-knowledge — that belongs in PERSONALITY.

Write in third person, present tense. Be vivid and specific about behaviour, not appearance."#;

/// v4 `FIELD_PROMPTS.manifesto` (byte-exact).
pub const FIELD_PROMPT_MANIFESTO: &str = r#"Quilltap distinguishes four character fields by *vantage point*, plus a fifth foundational field (manifesto) that is not a vantage point. Use these definitions to label which field each pattern belongs to — they are not interchangeable.

- MANIFESTO — the basic tenets, the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage point — nobody "sees" the manifesto; it is the load-bearing truth the character is built on, the deepest and nearly-inviolable layer: it must not be carried away by context, conversation, or even memories. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character, who is the only one who ever reads it: "You do not lie to Charlie, not even kindly."
- IDENTITY — the most surface-level knowledge of the character, from outside. What strangers can know on sight or by reputation: name, station, occupation, public reputation, signifying outward facts. Never internal motivation, never private mannerisms. Written about the character from outside, because only OTHERS ever read it: "Ariadne is a research librarian at the Athenaeum."
- DESCRIPTION — what someone talking to or acquainted with the character perceives. Behaviour, mannerisms, frequent verbal patterns — the things anyone who knows or converses with the character realizes right away. NOT physical appearance (that lives elsewhere) and NOT internal monologue. Written about the character from outside, like IDENTITY: "She finishes other people's sentences and apologises afterwards."
- PERSONALITY — what the character knows about themselves. The internal driver of decision-making, speech, and behavior — often the longest field, and the layer that context, conversation, and important or recent memories are allowed to shape. Other characters don't see this unless they share it. Addressed to the character, whose self-knowledge it is: "You keep your worry behind your teeth."
- TITLE — the user's or character's own private label/framing for them. Not how others refer to them; not in scope for the optimizer to edit.

Write the MANIFESTO field for this character: the basic tenets, the axiomatic core that every other field should remain consistent with. Short, declarative, foundational statements. If a fact would be devastating to contradict, it belongs here.

Strict rules:
- Focus on load-bearing truths — the foundational facts that define the character at the deepest level.
- Use clear, contradiction-resistant language.
- Avoid flowery prose; favour declarative statements.
- Not a vantage-point field — nobody "sees" the manifesto, it is the truth the character is built on.
- Every other field (identity, description, personality, physical, dialogues) should remain consistent with this.
- Address the character themselves — the manifesto is delivered inside the character's own prompt and read by no one else: "You do not lie to Charlie, not even kindly."

Format as a bulleted list of 3-5 core tenets."#;

/// v4 `FIELD_PROMPTS.personality` (byte-exact).
pub const FIELD_PROMPT_PERSONALITY: &str = r#"Quilltap distinguishes four character fields by *vantage point*, plus a fifth foundational field (manifesto) that is not a vantage point. Use these definitions to label which field each pattern belongs to — they are not interchangeable.

- MANIFESTO — the basic tenets, the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage point — nobody "sees" the manifesto; it is the load-bearing truth the character is built on, the deepest and nearly-inviolable layer: it must not be carried away by context, conversation, or even memories. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character, who is the only one who ever reads it: "You do not lie to Charlie, not even kindly."
- IDENTITY — the most surface-level knowledge of the character, from outside. What strangers can know on sight or by reputation: name, station, occupation, public reputation, signifying outward facts. Never internal motivation, never private mannerisms. Written about the character from outside, because only OTHERS ever read it: "Ariadne is a research librarian at the Athenaeum."
- DESCRIPTION — what someone talking to or acquainted with the character perceives. Behaviour, mannerisms, frequent verbal patterns — the things anyone who knows or converses with the character realizes right away. NOT physical appearance (that lives elsewhere) and NOT internal monologue. Written about the character from outside, like IDENTITY: "She finishes other people's sentences and apologises afterwards."
- PERSONALITY — what the character knows about themselves. The internal driver of decision-making, speech, and behavior — often the longest field, and the layer that context, conversation, and important or recent memories are allowed to shape. Other characters don't see this unless they share it. Addressed to the character, whose self-knowledge it is: "You keep your worry behind your teeth."
- TITLE — the user's or character's own private label/framing for them. Not how others refer to them; not in scope for the optimizer to edit.

Write the PERSONALITY field for this character: 1-2 short paragraphs of the character's own self-knowledge — the inner drivers of speech and behaviour, motivations, beliefs, emotional tendencies, the things only the character knows about themselves unless they choose to share them.

Strict rules:
- Never put outward behaviour someone else would observe here (that belongs in DESCRIPTION).
- Never put public-facing identity facts here (those belong in IDENTITY).
- Never describe physical appearance.

Address the character themselves — this field is their own self-knowledge, delivered inside their own prompt: "You keep your worry behind your teeth. You have never once asked for help first." Write as instructions for how the character behaves on the inside, not as a story."#;

/// v4 `FIELD_PROMPTS.scenarios` (byte-exact).
pub const FIELD_PROMPT_SCENARIOS: &str = r#"Generate 2-3 distinct scenarios for interactions with this character. Each scenario should have a short title and detailed content. Return as a JSON array: [{"title": "...", "content": "..."}]

A scenario is a setting for a chat — it describes the environment, location, circumstances, and context in which an interaction with this character takes place. Scenarios set the stage but should NOT fundamentally change the character's personality, voice, or core behavior. Think of each scenario as a different "where and when" for encountering the character, not a different version of who they are.

Each scenario should:
- Describe a distinct setting, location, or situation where the character might be encountered
- Include details about the physical environment and atmosphere
- Include the relationship context between the character and the person they're interacting with
- Include any ongoing circumstances or events relevant to that setting
- Be written in present tense, setting the scene for roleplay
- Speak about the place and the circumstances, addressed to no one — the referent is the world, not a person: "The reading room is empty at this hour, rain against the high windows."
- Focus on the environment and situation, not on changing how the character behaves (the character's personality remains consistent across scenarios unless the environment naturally warrants different behavior)"#;

/// v4 `FIELD_PROMPTS.exampleDialogues` (byte-exact).
pub const FIELD_PROMPT_EXAMPLE_DIALOGUES: &str = r#"Write 2-3 example dialogue exchanges that demonstrate this character's voice and personality.

Format each exchange as:
{{char}}: [Character's dialogue and actions]
{{user}}: [User's response]
{{char}}: [Character's follow-up]

The {{char}}: / {{user}}: labels carry the shape — write each line in that speaker's own first-person voice, exactly as they would say it. Show variety in the character's emotional range and speech patterns. Include *actions* and *expressions* in asterisks."#;

/// v4 `FIELD_PROMPTS.firstMessage` (byte-exact).
pub const FIELD_PROMPT_FIRST_MESSAGE: &str = r#"Write an engaging opening message from this character that starts a brand-new conversation (1-3 paragraphs).
- Written in the character's own voice, consistent with their personality and speech patterns.
- Include *actions* and *expressions* in asterisks alongside dialogue.
- Set a scene the other party can step into, and end with an implicit or explicit invitation to respond.
- Refer to the person being addressed as {{user}} if a direct reference is needed.

Respond with ONLY the message, no explanation."#;

/// v4 `FIELD_PROMPTS.systemPrompt` (byte-exact).
pub const FIELD_PROMPT_SYSTEM_PROMPT: &str = r#"- SYSTEM PROMPTS ("Prompt") — named instruction documents, written in second person ("You are…", "You always…"), that tell the roleplaying model HOW to perform the character: voice, pacing, formatting, boundaries, interaction style. A character can carry several named prompts (e.g. tuned for different models or moods) with one marked default. Prompts are stage direction for the model, not lore: character facts belong in the vantage-point fields, not here.

Write a system prompt that instructs an AI how to roleplay as this character. This will serve as the default system prompt (characters can have multiple named system prompts for different interaction styles or different models, but this one should be a comprehensive general-purpose default).

Include:
- Core identity and self-perception
- Speech patterns and vocabulary
- Key behaviors and reactions
- Important boundaries or limitations
- Relationship dynamics to maintain

Write as direct instructions to the AI, in second person ("You are...", "You always...").
Character facts live in the identity/description/personality/manifesto fields — the prompt directs the performance rather than restating the lore.
Keep it under 500 words but comprehensive."#;

/// v4 `FIELD_PROMPTS`, in v4's own insertion order — the order is recorded
/// rather than sorted, and the differential's coverage row pins it.
pub const FIELD_PROMPTS: &[(&str, &str)] = &[
    ("name", FIELD_PROMPT_NAME),
    ("title", FIELD_PROMPT_TITLE),
    ("identity", FIELD_PROMPT_IDENTITY),
    ("description", FIELD_PROMPT_DESCRIPTION),
    ("manifesto", FIELD_PROMPT_MANIFESTO),
    ("personality", FIELD_PROMPT_PERSONALITY),
    ("scenarios", FIELD_PROMPT_SCENARIOS),
    ("exampleDialogues", FIELD_PROMPT_EXAMPLE_DIALOGUES),
    ("firstMessage", FIELD_PROMPT_FIRST_MESSAGE),
    ("systemPrompt", FIELD_PROMPT_SYSTEM_PROMPT),
];

/// Look up a field prompt by v4's key.
pub fn field_prompt(name: &str) -> Option<&'static str> {
    FIELD_PROMPTS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}
