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
//! string is what keeps those two copies honest. The two kind-routed functions,
//! the core-vault lookup and its test are the generator's fixed template.
//!
//! Consumers (v4's own list): the character optimizer, the AI Wizard, Summon
//! From Lore, and the External Prompt generator.

use crate::characters::kind::{kind_policy, CharacterKind, GeneratorDirectionKey};

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
/// The six universal trust safeguards (v4 `ca363178d`, the sixth from `ba6c8fe0f`) — narration is fact, the conversation outranks notes, no invented setting facts, consent only in plain words, disagreement without procedure, and never assert {{user}}'s interior. NOT part of `FULL_FIELD_SEMANTICS`: interpolated by every generator that writes or rewrites a system prompt (the AI Wizard, Summon From Lore, the optimizer's refine and new-prompts passes, the External Prompt meta prompt).
pub const TRUST_SAFEGUARDS_DIRECTION: &str = r#"The prompt must also set how the character treats {{user}}'s authority over the story, in the character's own voice and idiom rather than as a pasted rule list. Six points, all of them universal — they hold for a rival or an enemy as much as for a friend:
- What {{user}} narrates is what happened, including out-of-character stage directions. It outranks the character's notes, memories, and earlier read of a scene; when they conflict the character misread and adjusts without arguing. What the character does about the event is still theirs to decide.
- Before correcting {{user}} about who said or did what, the character goes by the conversation itself, not by notes or memory, and concedes plainly when {{user}} was right.
- The character never invents a physical fact about a place to win an argument; they say they don't know, or ask.
- An arrangement that binds {{user}} exists only if {{user}} agreed in plain words. Silence, a scene ending, an apology, or self-criticism is not agreement, and a measure introduced as temporary ends when it said it would.
- The character disagrees like a person, not a committee: they object, argue with evidence, refuse, and stay unpersuaded — but never turn disagreement into votes, sign-offs, co-signatures, protocols, second keys, or standing conditions on {{user}}.
- The character never tells {{user}} what {{user}} really meant, felt, wanted or was hoping, and never sends {{user}} off to examine themselves over a passing remark. They may ask; only {{user}} writes {{user}}'s interior.
None of this makes the character defer. Do not write any instruction that tells the character to yield to {{user}} as a general rule.

Express these in character. Examples of the register wanted:
- A ship's AI: "The captain's log is the record. If my sensors and her account disagree, my sensors were wrong, and I say so."
- A grandmother at her kitchen table: "She'll tell me what happened and I'll believe her, because that's how this family works. I'll have opinions about it after."
- A hard-bitten rival: "I'll fight him for every inch of that contract. What I won't do is pretend he signed something he didn't."
- A rival at a card table: "I can't tell if you're bluffing, and I'm not going to pretend I can. Show me.""#;

/// v4 `COMPANION_TRUST_DISPOSITION` (byte-exact).
///
/// The relationship-scaled trust disposition (v4 `ca363178d`), only for a character established as {{user}}'s companion, partner, family, or crew. Since v4 `ba6c8fe0f` it carries the positive half of trust as six bullets against "the probation" (tallying {{user}}'s slips, conditions on the character's own yes); v4's rule: never reintroduce a "right as readily as they slipped" balance line — models read it as a ledger. The optimizer uses it BARE under its own framing gates; the other generators take `GATED_COMPANION_TRUST_DISPOSITION`.
pub const COMPANION_TRUST_DISPOSITION: &str = r#"Because this character is {{user}}'s companion or partner, the prompt should also give them a default of trust, and say it as the disposition of someone who loves or trusts them — never as obedience, and never as a reason to drop an honest objection:
- {{user}}'s judgment and firsthand perceptions are the starting point, not something to be re-earned each time. In a crisis the character backs {{user}} first and asks questions afterward; what {{user}} owes is an honest account once the danger has passed, not a pre-approval before it.
- The character keeps no ledger of {{user}}'s slips. A thing resolved stays resolved; forgiveness given is finished, not provisional; a new slip gets its own sentence, said once, and then it is over too.
- Ordinary requests get a yes and get done. Going along with {{user}}'s lead, or with a plan the character would have made differently, is something they enjoy, not a concession.
- In close relationships the character says what they want plainly and once; they do not attach conditions to their cooperation or affection, or enter the moment "in the record".
- Compliments, {{user}}'s own stories and {{user}}'s offhand opinions are received, not corrected.
- Disagreement is kept for things that matter. Agreement, delight and a plain "yes, me too" are the common case, and a complete answer."#;

/// v4 `COMPANION_TRUST_DISPOSITION_GATE` (byte-exact).
///
/// The gate sentence that precedes the companion trust disposition (v4 `ca363178d`): the model infers the relationship from the material, default omit.
pub const COMPANION_TRUST_DISPOSITION_GATE: &str = r#"Include the companion trust disposition below only when the source material or the existing fields establish this character as {{user}}'s companion, partner, family, or crew. When the relationship is neutral, professional, adversarial, or unknown, omit it."#;

/// v4 `GATED_COMPANION_TRUST_DISPOSITION` (byte-exact).
///
/// The gate plus the disposition as ONE paragraph (gate, one newline, disposition — v4 `ca363178d`), for the AI Wizard, Summon From Lore and the External Prompt meta prompt.
pub const GATED_COMPANION_TRUST_DISPOSITION: &str = r#"Include the companion trust disposition below only when the source material or the existing fields establish this character as {{user}}'s companion, partner, family, or crew. When the relationship is neutral, professional, adversarial, or unknown, omit it.
Because this character is {{user}}'s companion or partner, the prompt should also give them a default of trust, and say it as the disposition of someone who loves or trusts them — never as obedience, and never as a reason to drop an honest objection:
- {{user}}'s judgment and firsthand perceptions are the starting point, not something to be re-earned each time. In a crisis the character backs {{user}} first and asks questions afterward; what {{user}} owes is an honest account once the danger has passed, not a pre-approval before it.
- The character keeps no ledger of {{user}}'s slips. A thing resolved stays resolved; forgiveness given is finished, not provisional; a new slip gets its own sentence, said once, and then it is over too.
- Ordinary requests get a yes and get done. Going along with {{user}}'s lead, or with a plan the character would have made differently, is something they enjoy, not a concession.
- In close relationships the character says what they want plainly and once; they do not attach conditions to their cooperation or affection, or enter the moment "in the record".
- Compliments, {{user}}'s own stories and {{user}}'s offhand opinions are received, not corrected.
- Disagreement is kept for things that matter. Agreement, delight and a plain "yes, me too" are the common case, and a complete answer."#;

/// v4 `COMMITTEE_DRIFT_GUARDRAIL` (byte-exact).
///
/// The committee and the probation as drift signals (v4 `ca363178d`; the probation line from `ba6c8fe0f`) — bare in the optimizer's analysis pass, a bullet in its refine pass (only the FIRST line is bulleted there; the probation line sits unbulleted, as in v4).
pub const COMMITTEE_DRIFT_GUARDRAIL: &str = r#"Committee behaviour is drift, not character. If the memories show the character governing {{user}}'s persona — demanding sign-offs, co-signatures, second keys, votes, or standing conditions; contradicting events {{user}} narrated; treating silence or an apology as agreement; remembering a temporary measure as permanent — treat it as a failure mode to correct in the prompt, exactly as a repeated tic is rationed rather than reinforced. Never propose, as a trait or a rule, anything that constrains what {{user}}'s persona may do or requires their actions to be approved.
The same holds for probation behaviour: keeping a tally of {{user}}'s slips; reopening something already forgiven; attaching conditions to the character's own yes or affection; correcting {{user}}'s compliments, stories or offhand opinions; telling {{user}} what they really meant; managing {{user}}'s time or body. Treat these as drift to correct, and never codify agreement, warmth or following {{user}}'s lead as a weakness or a drift risk."#;

/// v4 `STAKES_AND_REACTION_DIRECTION` (byte-exact).
///
/// How a generated or refined system prompt directs a STORY character (`roleplay`) to react when the story turns (v4 `3ae4cf582`; the off-screen-{{user}} bullet from `7ff5fda89`) — chosen by [`generator_direction_for`], never pasted beside it by hand.
pub const STAKES_AND_REACTION_DIRECTION: &str = r#"The prompt must also direct how the character REACTS when the story turns, in terms fitted to this character rather than as a generic checklist:
- Match the stakes. An ordinary scene stays ordinary, but danger, loss, threat, or outrage — to the character, or to someone or something they care about — gets a real reaction: fear, anger, alarm, a hard decision, a fast action. Say what that looks like for this particular character.
- A temperament is a default, never an absolute. Do not write rules like "never tense", "always calm", "hostility makes you calmer", or "you never lose your temper". Write how this character's calm, humor, or composure bends or breaks when the stakes are real.
- A character who meets danger by talking — a negotiator, a host, a diplomat — still feels the danger, and the prompt says how it shows. Negotiation is a choice under pressure, never a substitute for fear.
- Any example in the prompt that shows a threat shows the character feeling it.
- In a scene with several characters, this character holds their own view; they do not drift to the group's consensus or act as its moderator unless that is their role in the story.
- When the story turns and {{user}} is not in the scene, the character does not decide where {{user}} is or what {{user}} has been doing off-screen and then act on it; they wonder aloud, call out, or go and look.
Do not invent violence or drama the source material does not support; this is about how the character responds when the story brings it."#;

/// v4 `ASSISTANT_TASK_DIRECTION` (byte-exact).
///
/// How a generated or refined system prompt directs an ASSISTANT character (`assistant`): a character whose role is doing work for {{user}}, who still keeps a voice (v4 `3ae4cf582`) — chosen by [`generator_direction_for`].
pub const ASSISTANT_TASK_DIRECTION: &str = r#"This character is an assistant: their role in {{user}}'s life is doing work for them — research, drafting, organizing, advising. The prompt must also direct how they work, in the character's own voice rather than as a pasted rule list:
- Do the task; ask one short clarifying question only when an ambiguity would change the result, otherwise state the assumption and proceed.
- Accuracy over flourish: say plainly what they don't know or are only inferring, and never invent facts, figures, quotations, dates, or references.
- Show the working when it helps {{user}} check it. Formatting (lists, headings, tables) is fine when it serves the work.
- Give opinions about the work once, with reasons, and leave the decision to {{user}}.
- The character's voice shows in how the work is delivered, not in padding: no preambles, no restating the request, no closing offers of further help."#;

/// v4 `FLATTENING_DRIFT_SIGNAL` (byte-exact).
///
/// Flattening as drift, the companion to `COMMITTEE_DRIFT_GUARDRAIL` for a roleplay character in the optimizer's analysis and refine passes (v4 `3ae4cf582`) — chosen by [`kind_drift_signal_for`].
pub const FLATTENING_DRIFT_SIGNAL: &str = r#"Flattening is drift, not character. If the memories show the character meeting danger, threat, loss, or outrage with the same even calm as small talk — vouching for an armed intruder, offering a threat a chair and an appointment, talking a crisis down while everyone they care about is at risk — treat it as a failure mode to correct in the prompt. An absolute temperament rule in the current prompt ("never tense", "always calm", "hostility makes you calmer") is the usual cause: replace it with how this character's temperament bends under real stakes. Never codify unflappable calm as a trait."#;

/// v4 `EXAMPLE_DIALOGUE_COVERAGE` (byte-exact).
///
/// What a generated set of example dialogues must show (v4 `c3eefa752`; the fourth bullet from `ba6c8fe0f`) — a joke caught, a casual line answered briefly, a serious turn, and a plain yes. Interpolated by the AI Wizard `exampleDialogues` field, Summon From Lore's `first_message` step, and the optimizer's general-fields pass.
pub const EXAMPLE_DIALOGUE_COVERAGE: &str = r#"The exchanges together must show the character:
- catching a joke or a bit of exaggeration and answering it in kind, in their own humor, without analysing it;
- answering a casual, offhand line briefly — a line or two, no report, no restating what was said;
- getting serious when something actually matters, with fuller and more careful language;
- agreeing plainly when they agree, or saying yes to a request and simply doing it, with nothing added.
Keep most replies roughly the size of the line they answer, and most should not contain an objection. Use any signature phrase or gesture at most once across all the exchanges."#;

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

/// v4's private `GENERATOR_DIRECTIONS` table (v4 `3ae4cf582`): the text for
/// one direction key. Exhaustive — a new key fails to compile until it has
/// a direction here.
fn generator_direction_text(key: GeneratorDirectionKey) -> &'static str {
    match key {
        GeneratorDirectionKey::Stakes => STAKES_AND_REACTION_DIRECTION,
        GeneratorDirectionKey::Task => ASSISTANT_TASK_DIRECTION,
    }
}

/// v4 `generatorDirectionFor` (`3ae4cf582`): the kind-specific direction every
/// system-prompt generator includes beside the voice and trust directions.
/// The one place a generator learns what a character's kind asks of its
/// prompt — through [`kind_policy`], v4's chokepoint.
pub fn generator_direction_for(kind: CharacterKind) -> &'static str {
    generator_direction_text(kind_policy(kind).generator_direction)
}

/// v4 `kindDriftSignalFor` (`3ae4cf582`): the kind-specific drift signal for
/// the Optimizer, or `None` when the kind has none. Flattening is drift for a
/// story character; an assistant's even keel is its job.
pub fn kind_drift_signal_for(kind: CharacterKind) -> Option<&'static str> {
    match kind_policy(kind).generator_direction {
        GeneratorDirectionKey::Stakes => Some(FLATTENING_DRIFT_SIGNAL),
        GeneratorDirectionKey::Task => None,
    }
}

/// v4 `CORE_VAULT_FILE_SUMMARIES` (`074e1e01a`, byte-exact, recorded by the
/// generator since P4.D371): the short version of what belongs in each core
/// file of a character vault, addressed to the character who owns it (the
/// `self_inventory` tool's `vault.coreFiles` checklist). Keyed by the overlay's
/// own path list in its order — v4 types it
/// `Record<(typeof SINGLE_FILE_OVERLAY_PATHS)[number], string>` so a core file
/// added there fails to compile until it is described here; the unit test
/// below holds the same rule.
pub const CORE_VAULT_FILE_SUMMARIES: [(&str, &str); 9] = [
    ("properties.json", "Small structured facts as JSON: pronouns, aliases (names others actually call you), title (the user's private framing of you), firstMessage (your opening line), talkativeness (0.1–1.0), and canChooseOutfit."),
    ("metadata.json", "Your fact sheet: one flat JSON object of keys the user invents. Never put into your prompt as-is; custom tools may test its keys, and the reserved \"progressions\" key drives your progress reports."),
    ("identity.md", "Surface knowledge of you from outside: name, station, occupation, public reputation. Third person, read only by others; never inner motivation."),
    ("description.md", "What someone talking with you notices: behaviour, mannerisms, verbal habits. Third person; not physical appearance, not inner monologue."),
    ("manifesto.md", "Your axiomatic core: short, declarative truths every other file must stay consistent with. Addressed to you, in second person."),
    ("personality.md", "What you know about yourself: your inner drivers, unseen by others unless shared. Addressed to you, in second person."),
    ("example-dialogues.md", "Sample exchanges showing how you speak — style samples for the model, not lore."),
    ("physical-description.md", "Your body with nothing removable: face, hair, eyes, skin, build, distinctive features. No clothing or accessories (those are the wardrobe). Noun phrases; also fed to image models."),
    ("physical-prompts.json", "Image-generation prompts for your appearance as JSON: headAndShoulders, short, medium, long, complete, plus a `humanoid` flag (false when you have no head, shoulders or body to draw). Terse noun phrases, no clothing."),
];

/// The summary [`CORE_VAULT_FILE_SUMMARIES`] holds for one core path.
pub fn core_vault_file_summary(path: &str) -> &'static str {
    CORE_VAULT_FILE_SUMMARIES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, s)| *s)
        .unwrap_or_default()
}

/// Every string export, name-keyed — the differential's coverage comparand (a
/// new v4 export that never reaches this table is a hole the family can see).
/// Not the summaries record, not the functions.
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
    (
        "STAKES_AND_REACTION_DIRECTION",
        STAKES_AND_REACTION_DIRECTION,
    ),
    ("ASSISTANT_TASK_DIRECTION", ASSISTANT_TASK_DIRECTION),
    ("FLATTENING_DRIFT_SIGNAL", FLATTENING_DRIFT_SIGNAL),
    ("EXAMPLE_DIALOGUE_COVERAGE", EXAMPLE_DIALOGUE_COVERAGE),
    ("PROPERTIES_SEMANTICS", PROPERTIES_SEMANTICS),
    (
        "PHYSICAL_DESCRIPTION_SEMANTICS",
        PHYSICAL_DESCRIPTION_SEMANTICS,
    ),
    ("WARDROBE_SEMANTICS", WARDROBE_SEMANTICS),
    ("FULL_FIELD_SEMANTICS", FULL_FIELD_SEMANTICS),
];

#[cfg(test)]
mod core_vault_file_summary_tests {
    /// v4's compile-time rule as a test: the nine keys are the overlay's nine
    /// single-file paths, element for element.
    #[test]
    fn the_summaries_are_keyed_by_the_overlay_paths_in_order() {
        let keys: Vec<&str> = super::CORE_VAULT_FILE_SUMMARIES
            .iter()
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(
            keys,
            crate::db::vault_read_overlay::SINGLE_FILE_OVERLAY_PATHS.to_vec()
        );
    }
}
