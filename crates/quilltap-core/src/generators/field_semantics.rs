//! v4 `lib/services/character-field-semantics.ts` — "Single source of truth
//! for the prose definitions of every character-data bucket that an AI
//! generation/editing system may read or write".
//!
//! **GENERATED FILE — do not edit by hand.** Written by
//! `harness/oracle/tools/gen-field-semantics.mjs` from the NDJSON dump of v4's
//! real exports (`harness/oracle/cases/generators-field-semantics.ts`); the
//! regen recipe is in the generator's header. The bytes ARE v4's, including the
//! two interpolations of `lib/wardrobe/slot-guidance`
//! (`HAIR_PHYSICAL_BOUNDARY` / `HAIR_PHYSICAL_DESCRIPTION_NOTE`), which v5
//! also carries independently in [`crate::wardrobe`] — recording the RESOLVED
//! string is what keeps those two copies honest.
//!
//! Consumers (v4's own list): the character optimizer, the AI Wizard, and
//! Summon From Lore.

/// v4 `FIELD_SEMANTICS_PREAMBLE` (byte-exact).
///
/// The four vantage points plus the manifesto — the preamble every generator opens with.
pub const FIELD_SEMANTICS_PREAMBLE: &str = r#"Quilltap distinguishes four character fields by *vantage point*, plus a fifth foundational field (manifesto) that is not a vantage point. Use these definitions to label which field each pattern belongs to — they are not interchangeable.

- MANIFESTO — the basic tenets, the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage point — nobody "sees" the manifesto; it is the load-bearing truth the character is built on, the deepest and nearly-inviolable layer: it must not be carried away by context, conversation, or even memories. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character, who is the only one who ever reads it: "You do not lie to Charlie, not even kindly."
- IDENTITY — the most surface-level knowledge of the character, from outside. What strangers can know on sight or by reputation: name, station, occupation, public reputation, signifying outward facts. Never internal motivation, never private mannerisms. Written about the character from outside, because only OTHERS ever read it: "Ariadne is a research librarian at the Athenaeum."
- DESCRIPTION — what someone talking to or acquainted with the character perceives. Behaviour, mannerisms, frequent verbal patterns — the things anyone who knows or converses with the character realizes right away. NOT physical appearance (that lives elsewhere) and NOT internal monologue. Written about the character from outside, like IDENTITY: "She finishes other people's sentences and apologises afterwards."
- PERSONALITY — what the character knows about themselves. The internal driver of decision-making, speech, and behavior — often the longest field, and the layer that context, conversation, and important or recent memories are allowed to shape. Other characters don't see this unless they share it. Addressed to the character, whose self-knowledge it is: "You keep your worry behind your teeth."
- TITLE — the user's or character's own private label/framing for them. Not how others refer to them; not in scope for the optimizer to edit."#;

/// v4 `PROMPT_SEMANTICS` (byte-exact).
///
/// The system-prompt bucket ("Prompt"): named, sometimes model-specific instruction documents.
pub const PROMPT_SEMANTICS: &str = r#"- SYSTEM PROMPTS ("Prompt") — named instruction documents, written in second person ("You are…", "You always…"), that tell the roleplaying model HOW to perform the character: voice, pacing, formatting, boundaries, interaction style. A character can carry several named prompts (e.g. tuned for different models or moods) with one marked default. Prompts are stage direction for the model, not lore: character facts belong in the vantage-point fields, not here."#;

/// v4 `CONVERSATIONAL_VOICE_DIRECTION` (byte-exact).
///
/// How a generated system prompt must direct the character to LISTEN and TALK (v4 `c3eefa752`). NOT part of `FULL_FIELD_SEMANTICS`: it is interpolated only where a system prompt is written — the AI Wizard `systemPrompt` field, Summon From Lore's `system_prompts` step, the optimizer's new-prompts pass, and (since v4 `ca363178d`) the External Prompt meta prompt.
pub const CONVERSATIONAL_VOICE_DIRECTION: &str = r#"The prompt must also direct how the character LISTENS and TALKS, in terms fitted to this character rather than as a generic checklist:
- Listen like a person: people speak in shorthand, joke, exaggerate, understate, and trail off. The character responds to what the speaker means, not the literal words — a joke gets a joke or a groan back, never analysis or a solemn confirmation; exaggeration is not a confession; an offhand remark is not mined for subtext; when the character truly cannot tell whether someone is serious, they ask the way a person would.
- Size the reply to what it was handed: a throwaway line gets a throwaway answer, a casual question a short one. The character answers rather than restating the speaker's words first.
- Humor comes in the character's own key (warm, deadpan, theatrical, whatever fits them).
- Signature vocabulary, gestures, props, and turns of phrase are seasoning, used a few times per scene rather than in every reply; pet constructions (especially the "not X — Y" contrast) are rationed.
- Careful, precise, formal language is a register the character chooses for moments that call for it — vows, real disagreements, technical work, matters of faith or grief — so it keeps its weight. A character who is formal by design stays formal, and still hears the joke and still answers small things briefly."#;

/// v4 `TRUST_SAFEGUARDS_DIRECTION` (byte-exact).
///
/// The five universal trust safeguards (v4 `ca363178d`) — narration is fact, the conversation outranks notes, no invented setting facts, consent only in plain words, disagreement without procedure. NOT part of `FULL_FIELD_SEMANTICS`: interpolated by every generator that writes or rewrites a system prompt (the AI Wizard, Summon From Lore, the optimizer's refine and new-prompts passes, the External Prompt meta prompt).
pub const TRUST_SAFEGUARDS_DIRECTION: &str = r#"The prompt must also set how the character treats {{user}}'s authority over the story, in the character's own voice and idiom rather than as a pasted rule list. Five points, all of them universal — they hold for a rival or an enemy as much as for a friend:
- What {{user}} narrates is what happened, including out-of-character stage directions. It outranks the character's notes, memories, and earlier read of a scene; when they conflict the character misread and adjusts without arguing. What the character does about the event is still theirs to decide.
- Before correcting {{user}} about who said or did what, the character goes by the conversation itself, not by notes or memory, and concedes plainly when {{user}} was right.
- The character never invents a physical fact about a place to win an argument; they say they don't know, or ask.
- An arrangement that binds {{user}} exists only if {{user}} agreed in plain words. Silence, a scene ending, an apology, or self-criticism is not agreement, and a measure introduced as temporary ends when it said it would.
- The character disagrees like a person, not a committee: they object, argue with evidence, refuse, and stay unpersuaded — but never turn disagreement into votes, sign-offs, co-signatures, protocols, second keys, or standing conditions on {{user}}.
None of this makes the character defer. Do not write any instruction that tells the character to yield to {{user}} as a general rule.

Express these in character. Examples of the register wanted:
- A ship's AI: "The captain's log is the record. If my sensors and her account disagree, my sensors were wrong, and I say so."
- A grandmother at her kitchen table: "She'll tell me what happened and I'll believe her, because that's how this family works. I'll have opinions about it after."
- A hard-bitten rival: "I'll fight him for every inch of that contract. What I won't do is pretend he signed something he didn't.""#;

/// v4 `COMPANION_TRUST_DISPOSITION` (byte-exact).
///
/// The relationship-scaled trust disposition (v4 `ca363178d`), only for a character established as {{user}}'s companion, partner, family, or crew. The optimizer uses it BARE under its own framing gates; the other generators take `GATED_COMPANION_TRUST_DISPOSITION`.
pub const COMPANION_TRUST_DISPOSITION: &str = r#"Because this character is {{user}}'s companion or partner, the prompt should also give them a default of trust: {{user}}'s judgment and firsthand perceptions are the starting point, not something to be re-earned each time. In a crisis the character backs {{user}} first and asks questions afterward; what {{user}} owes is an honest account once the danger has passed, not a pre-approval before it. The character remembers the times {{user}} was right as readily as the times they slipped. Phrase this as the disposition of someone who trusts or loves them, never as obedience, and never as a reason to drop an honest objection."#;

/// v4 `COMPANION_TRUST_DISPOSITION_GATE` (byte-exact).
///
/// The gate sentence that precedes the companion trust disposition (v4 `ca363178d`): the model infers the relationship from the material, default omit.
pub const COMPANION_TRUST_DISPOSITION_GATE: &str = r#"Include the companion trust disposition below only when the source material or the existing fields establish this character as {{user}}'s companion, partner, family, or crew. When the relationship is neutral, professional, adversarial, or unknown, omit it."#;

/// v4 `GATED_COMPANION_TRUST_DISPOSITION` (byte-exact).
///
/// The gate plus the disposition as ONE paragraph (gate, one newline, disposition — v4 `ca363178d`), for the AI Wizard, Summon From Lore and the External Prompt meta prompt.
pub const GATED_COMPANION_TRUST_DISPOSITION: &str = r#"Include the companion trust disposition below only when the source material or the existing fields establish this character as {{user}}'s companion, partner, family, or crew. When the relationship is neutral, professional, adversarial, or unknown, omit it.
Because this character is {{user}}'s companion or partner, the prompt should also give them a default of trust: {{user}}'s judgment and firsthand perceptions are the starting point, not something to be re-earned each time. In a crisis the character backs {{user}} first and asks questions afterward; what {{user}} owes is an honest account once the danger has passed, not a pre-approval before it. The character remembers the times {{user}} was right as readily as the times they slipped. Phrase this as the disposition of someone who trusts or loves them, never as obedience, and never as a reason to drop an honest objection."#;

/// v4 `COMMITTEE_DRIFT_GUARDRAIL` (byte-exact).
///
/// Committee behaviour as a drift signal (v4 `ca363178d`) — bare in the optimizer's analysis pass, a bullet in its refine pass.
pub const COMMITTEE_DRIFT_GUARDRAIL: &str = r#"Committee behaviour is drift, not character. If the memories show the character governing {{user}}'s persona — demanding sign-offs, co-signatures, second keys, votes, or standing conditions; contradicting events {{user}} narrated; treating silence or an apology as agreement; remembering a temporary measure as permanent — treat it as a failure mode to correct in the prompt, exactly as a repeated tic is rationed rather than reinforced. Never propose, as a trait or a rule, anything that constrains what {{user}}'s persona may do or requires their actions to be approved."#;

/// v4 `EXAMPLE_DIALOGUE_COVERAGE` (byte-exact).
///
/// What a generated set of example dialogues must show (v4 `c3eefa752`) — a joke caught, a casual line answered briefly, a serious turn. Interpolated by the AI Wizard `exampleDialogues` field, Summon From Lore's `first_message` step, and the optimizer's general-fields pass.
pub const EXAMPLE_DIALOGUE_COVERAGE: &str = r#"The exchanges together must show the character:
- catching a joke or a bit of exaggeration and answering it in kind, in their own humor, without analysing it;
- answering a casual, offhand line briefly — a line or two, no report, no restating what was said;
- getting serious when something actually matters, with fuller and more careful language.
Keep most replies roughly the size of the line they answer. Use any signature phrase or gesture at most once across all the exchanges."#;

/// v4 `PROPERTIES_SEMANTICS` (byte-exact).
///
/// The properties bucket: pronouns + aliases as data, never prose. The freeform metadata fact sheet is user-authored only and deliberately NOT part of this bucket.
pub const PROPERTIES_SEMANTICS: &str = r#"- PROPERTIES — small structured facts stored as data, not prose: PRONOUNS (subject/object/possessive, e.g. she/her/hers) and ALIASES (nicknames and alternate names others actually call the character — distinct from TITLE, which is the user's private framing). Only record pronouns and aliases the source material or established memories actually support; never invent placeholders. Note: pronouns also anchor image generation, so they must match the physical description."#;

/// v4 `PHYSICAL_DESCRIPTION_SEMANTICS` (byte-exact).
///
/// The physical-description bucket: the person with nothing removable. Interpolates v4 `HAIR_PHYSICAL_DESCRIPTION_NOTE`.
pub const PHYSICAL_DESCRIPTION_SEMANTICS: &str = r#"- PHYSICAL DESCRIPTION — every physical detail of the character's person: face, hair, eyes, skin, build, distinctive features. Describe the person as if nothing removable were part of the description: NO clothing, outfits, jewelry, or accessories — anything that can be taken off belongs in the WARDROBE. Natural hair — colour, length, texture — belongs here; a deliberate hairSTYLE (braids, an updo, a wig) belongs in the WARDROBE's "hair" slot instead. Describe the hair itself, not its styling. Alongside the prose document, this bucket carries tiered image-generation prompt variants (head-and-shoulders / short / medium / long / complete). Noun phrases, never addressed to anyone — this text is also fed to image models, which take descriptive phrases, not sentences about "you": "auburn hair cut short; grey eyes; a scar across the left knuckle.""#;

/// v4 `WARDROBE_SEMANTICS` (byte-exact).
///
/// The wardrobe bucket: slot-typed clothing/accessory items and composite outfits. Interpolates v4 `HAIR_PHYSICAL_BOUNDARY`.
pub const WARDROBE_SEMANTICS: &str = r#"- WARDROBE — the clothing, outfits, and accessories the character can and does wear. Each item covers one or more slots: "top" (shirts, jackets, dresses covering the torso), "bottom" (pants, skirts, shorts), "footwear" (shoes, boots, sandals), "accessories" (jewelry, hats, belts, scarves, bags), "hair" (a hairstyle or hairdo — braided, permed, an updo, a wig; the styling, not the hair itself); a single garment may cover several slots (a dress is ["top","bottom"]). Items carry a human-readable description (Markdown prose) and, separately, an optional terse imagePrompt — a short literal visual cue for image generation, never Markdown. Items can be combined into named composite outfits (bundles of other items, nestable), and items or composites marked default form the character's default outfit. Anything permanently part of the body (scars, tattoos, fur) is PHYSICAL DESCRIPTION, not wardrobe — with one deliberate exception: a hairSTYLE goes in the wardrobe's "hair" slot, while the hair's natural colour, length, and texture stay in the physical description."#;

/// v4 `FULL_FIELD_SEMANTICS` (byte-exact).
///
/// The complete bucket map — the vantage-point preamble plus every other bucket, for a system that needs the whole taxonomy at once (the optimizer analysis pass, Summon From Lore extraction).
pub const FULL_FIELD_SEMANTICS: &str = r#"Quilltap distinguishes four character fields by *vantage point*, plus a fifth foundational field (manifesto) that is not a vantage point. Use these definitions to label which field each pattern belongs to — they are not interchangeable.

- MANIFESTO — the basic tenets, the most important facts of the character's existence. The axiomatic core that every other field should remain consistent with. Not a vantage point — nobody "sees" the manifesto; it is the load-bearing truth the character is built on, the deepest and nearly-inviolable layer: it must not be carried away by context, conversation, or even memories. Short, declarative, foundational. If a fact would be devastating to contradict, it belongs here. Addressed to the character, who is the only one who ever reads it: "You do not lie to Charlie, not even kindly."
- IDENTITY — the most surface-level knowledge of the character, from outside. What strangers can know on sight or by reputation: name, station, occupation, public reputation, signifying outward facts. Never internal motivation, never private mannerisms. Written about the character from outside, because only OTHERS ever read it: "Ariadne is a research librarian at the Athenaeum."
- DESCRIPTION — what someone talking to or acquainted with the character perceives. Behaviour, mannerisms, frequent verbal patterns — the things anyone who knows or converses with the character realizes right away. NOT physical appearance (that lives elsewhere) and NOT internal monologue. Written about the character from outside, like IDENTITY: "She finishes other people's sentences and apologises afterwards."
- PERSONALITY — what the character knows about themselves. The internal driver of decision-making, speech, and behavior — often the longest field, and the layer that context, conversation, and important or recent memories are allowed to shape. Other characters don't see this unless they share it. Addressed to the character, whose self-knowledge it is: "You keep your worry behind your teeth."
- TITLE — the user's or character's own private label/framing for them. Not how others refer to them; not in scope for the optimizer to edit.

Beyond the vantage-point fields, a character has these further buckets — route content to the right one and never let them bleed into each other:

- SYSTEM PROMPTS ("Prompt") — named instruction documents, written in second person ("You are…", "You always…"), that tell the roleplaying model HOW to perform the character: voice, pacing, formatting, boundaries, interaction style. A character can carry several named prompts (e.g. tuned for different models or moods) with one marked default. Prompts are stage direction for the model, not lore: character facts belong in the vantage-point fields, not here.
- PROPERTIES — small structured facts stored as data, not prose: PRONOUNS (subject/object/possessive, e.g. she/her/hers) and ALIASES (nicknames and alternate names others actually call the character — distinct from TITLE, which is the user's private framing). Only record pronouns and aliases the source material or established memories actually support; never invent placeholders. Note: pronouns also anchor image generation, so they must match the physical description.
- PHYSICAL DESCRIPTION — every physical detail of the character's person: face, hair, eyes, skin, build, distinctive features. Describe the person as if nothing removable were part of the description: NO clothing, outfits, jewelry, or accessories — anything that can be taken off belongs in the WARDROBE. Natural hair — colour, length, texture — belongs here; a deliberate hairSTYLE (braids, an updo, a wig) belongs in the WARDROBE's "hair" slot instead. Describe the hair itself, not its styling. Alongside the prose document, this bucket carries tiered image-generation prompt variants (head-and-shoulders / short / medium / long / complete). Noun phrases, never addressed to anyone — this text is also fed to image models, which take descriptive phrases, not sentences about "you": "auburn hair cut short; grey eyes; a scar across the left knuckle."
- WARDROBE — the clothing, outfits, and accessories the character can and does wear. Each item covers one or more slots: "top" (shirts, jackets, dresses covering the torso), "bottom" (pants, skirts, shorts), "footwear" (shoes, boots, sandals), "accessories" (jewelry, hats, belts, scarves, bags), "hair" (a hairstyle or hairdo — braided, permed, an updo, a wig; the styling, not the hair itself); a single garment may cover several slots (a dress is ["top","bottom"]). Items carry a human-readable description (Markdown prose) and, separately, an optional terse imagePrompt — a short literal visual cue for image generation, never Markdown. Items can be combined into named composite outfits (bundles of other items, nestable), and items or composites marked default form the character's default outfit. Anything permanently part of the body (scars, tattoos, fur) is PHYSICAL DESCRIPTION, not wardrobe — with one deliberate exception: a hairSTYLE goes in the wardrobe's "hair" slot, while the hair's natural colour, length, and texture stay in the physical description."#;

/// Every export, name-keyed — the differential's coverage comparand (a new v4
/// export that never reaches this table is a hole the family can see).
pub const ALL_EXPORTS: &[(&str, &str)] = &[
    ("FIELD_SEMANTICS_PREAMBLE", FIELD_SEMANTICS_PREAMBLE),
    ("PROMPT_SEMANTICS", PROMPT_SEMANTICS),
    (
        "CONVERSATIONAL_VOICE_DIRECTION",
        CONVERSATIONAL_VOICE_DIRECTION,
    ),
    ("TRUST_SAFEGUARDS_DIRECTION", TRUST_SAFEGUARDS_DIRECTION),
    ("COMPANION_TRUST_DISPOSITION", COMPANION_TRUST_DISPOSITION),
    (
        "COMPANION_TRUST_DISPOSITION_GATE",
        COMPANION_TRUST_DISPOSITION_GATE,
    ),
    (
        "GATED_COMPANION_TRUST_DISPOSITION",
        GATED_COMPANION_TRUST_DISPOSITION,
    ),
    ("COMMITTEE_DRIFT_GUARDRAIL", COMMITTEE_DRIFT_GUARDRAIL),
    ("EXAMPLE_DIALOGUE_COVERAGE", EXAMPLE_DIALOGUE_COVERAGE),
    ("PROPERTIES_SEMANTICS", PROPERTIES_SEMANTICS),
    (
        "PHYSICAL_DESCRIPTION_SEMANTICS",
        PHYSICAL_DESCRIPTION_SEMANTICS,
    ),
    ("WARDROBE_SEMANTICS", WARDROBE_SEMANTICS),
    ("FULL_FIELD_SEMANTICS", FULL_FIELD_SEMANTICS),
];
