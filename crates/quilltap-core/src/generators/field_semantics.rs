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
    ("PROPERTIES_SEMANTICS", PROPERTIES_SEMANTICS),
    (
        "PHYSICAL_DESCRIPTION_SEMANTICS",
        PHYSICAL_DESCRIPTION_SEMANTICS,
    ),
    ("WARDROBE_SEMANTICS", WARDROBE_SEMANTICS),
    ("FULL_FIELD_SEMANTICS", FULL_FIELD_SEMANTICS),
];
