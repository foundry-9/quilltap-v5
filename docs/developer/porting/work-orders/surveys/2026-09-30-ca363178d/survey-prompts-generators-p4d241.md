# Survey: v4 `ca363178d` parts (a) + (b) — the 21 sample prompts and the generator trust directions (for P4.D241)

Dated 2026-09-30. v4 `main` HEAD **`ca363178d`** (clean, so the checkout IS the round target); oracle baseline `97b25fc53`. v5 `main` at **`735cf568e`**. Read-only survey: every claim is from the shipped hunks / post-commit files, not the commit message or the ledger row. Twin of `surveys/2026-09-28-97b25fc53/survey-prompts-p4d237.md` (P4.D237, v4 `c3eefa752`). `ca363178d` is one squash of five phases (66 files); this survey covers only phase 1 (the prompts) and phase 2 (the generators). Phases 3–4 (`memory-tasks.ts`, `user-narration-anchor.ts`, `context-manager.ts`, `context-builder.service.ts`) and the 8 `help/` pages belong to other lanes.

Pin facts: `git diff 97b25fc53 ca363178d -- lib/database/` is EMPTY (no schema or migration move), and the prompt-templates repository (the seeder) is NOT in the commit's file list, so the seeder's behaviour is unchanged. Of the commits past the baseline (`aa92cf91c`, `a67a282c6`, `ddf942635`, `ca363178d`), only `ca363178d` touches any file in this lane's scope. Versions: root `package.json` `4.10.0-dev.100` (baseline) → `4.10.0-dev.101` (`ddf942635`) → **`4.10.0-dev.105`** (`ca363178d`).

## A. v4, hunk by hunk

### A1. The five new constants — `lib/services/character-field-semantics.ts`

They are declared at post-commit :63 / :83 / :90 / :93 / :97, all **between `CONVERSATIONAL_VOICE_DIRECTION` (:48) and `EXAMPLE_DIALOGUE_COVERAGE` (:104)**. That gives the v4 source order of the 13 exports: PREAMBLE, PROMPT, CVD, **TRUST_SAFEGUARDS_DIRECTION, COMPANION_TRUST_DISPOSITION, COMPANION_TRUST_DISPOSITION_GATE, GATED_COMPANION_TRUST_DISPOSITION, COMMITTEE_DRIFT_GUARDRAIL**, EXAMPLE_DIALOGUE_COVERAGE, PROPERTIES, PHYSICAL, WARDROBE, FULL. The whole added hunk is below, verbatim (post-commit :55-97, doc comments included). These are LLM-input bytes. They contain straight ASCII `"` and `'`, two em-dashes (U+2014) in TRUST and two in COMMITTEE, and `{{user}}` literals throughout. There are no backticks, backslashes or `${`, except that `GATED_…` interpolates the other two.

```
/**
 * How a generated or refined system prompt should direct the character's
 * relationship to the user's authority over the fiction. Long multi-character
 * roleplay drifts into "the committee" — characters governing the user's
 * persona with votes, sign-offs and standing conditions, contradicting narrated
 * events from their notes, and treating silence as consent. Shared by every
 * generator that writes or rewrites a system prompt.
 */
export const TRUST_SAFEGUARDS_DIRECTION = `The prompt must also set how the character treats {{user}}'s authority over the story, in the character's own voice and idiom rather than as a pasted rule list. Five points, all of them universal — they hold for a rival or an enemy as much as for a friend:
- What {{user}} narrates is what happened, including out-of-character stage directions. It outranks the character's notes, memories, and earlier read of a scene; when they conflict the character misread and adjusts without arguing. What the character does about the event is still theirs to decide.
- Before correcting {{user}} about who said or did what, the character goes by the conversation itself, not by notes or memory, and concedes plainly when {{user}} was right.
- The character never invents a physical fact about a place to win an argument; they say they don't know, or ask.
- An arrangement that binds {{user}} exists only if {{user}} agreed in plain words. Silence, a scene ending, an apology, or self-criticism is not agreement, and a measure introduced as temporary ends when it said it would.
- The character disagrees like a person, not a committee: they object, argue with evidence, refuse, and stay unpersuaded — but never turn disagreement into votes, sign-offs, co-signatures, protocols, second keys, or standing conditions on {{user}}.
None of this makes the character defer. Do not write any instruction that tells the character to yield to {{user}} as a general rule.

Express these in character. Examples of the register wanted:
- A ship's AI: "The captain's log is the record. If my sensors and her account disagree, my sensors were wrong, and I say so."
- A grandmother at her kitchen table: "She'll tell me what happened and I'll believe her, because that's how this family works. I'll have opinions about it after."
- A hard-bitten rival: "I'll fight him for every inch of that contract. What I won't do is pretend he signed something he didn't."`;

/**
 * The relationship-scaled addition. Only for a character the source material or
 * existing fields establish as {{user}}'s close companion, partner, family, or
 * crew; a neutral, adversarial, or professional character does not get it.
 * Always introduce it with COMPANION_TRUST_DISPOSITION_GATE so the model
 * decides from the material whether it applies.
 */
export const COMPANION_TRUST_DISPOSITION = `Because this character is {{user}}'s companion or partner, the prompt should also give them a default of trust: {{user}}'s judgment and firsthand perceptions are the starting point, not something to be re-earned each time. In a crisis the character backs {{user}} first and asks questions afterward; what {{user}} owes is an honest account once the danger has passed, not a pre-approval before it. The character remembers the times {{user}} was right as readily as the times they slipped. Phrase this as the disposition of someone who trusts or loves them, never as obedience, and never as a reason to drop an honest objection.`;

/**
 * The gate that precedes COMPANION_TRUST_DISPOSITION in every generator that
 * writes a fresh prompt. No generator knows the relationship as data, so the
 * model infers it, with a conservative default of omit.
 */
export const COMPANION_TRUST_DISPOSITION_GATE = `Include the companion trust disposition below only when the source material or the existing fields establish this character as {{user}}'s companion, partner, family, or crew. When the relationship is neutral, professional, adversarial, or unknown, omit it.`;

/** The gate sentence plus the disposition, as one paragraph for meta-prompts. */
export const GATED_COMPANION_TRUST_DISPOSITION = `${COMPANION_TRUST_DISPOSITION_GATE}
${COMPANION_TRUST_DISPOSITION}`;

/** The committee as a drift signal, for the optimizer's analysis and refine passes. */
export const COMMITTEE_DRIFT_GUARDRAIL = `Committee behaviour is drift, not character. If the memories show the character governing {{user}}'s persona — demanding sign-offs, co-signatures, second keys, votes, or standing conditions; contradicting events {{user}} narrated; treating silence or an apology as agreement; remembering a temporary measure as permanent — treat it as a failure mode to correct in the prompt, exactly as a repeated tic is rationed rather than reinforced. Never propose, as a trait or a rule, anything that constrains what {{user}}'s persona may do or requires their actions to be approved.`;
```

How they compose:
- `GATED_COMPANION_TRUST_DISPOSITION` = `COMPANION_TRUST_DISPOSITION_GATE` + **one `\n`** + `COMPANION_TRUST_DISPOSITION`. It is one paragraph, not two: there is no blank line between them.
- **`FULL_FIELD_SEMANTICS` (:135) is UNCHANGED**, and none of the five joins it. So `CHARACTER_BASICS_PROMPT` and every other `FULL_FIELD_SEMANTICS` consumer are byte-neutral. Measured: the commit's only hunk in this file is the :52 insertion.
- Measured UTF-16 lengths (python over the source): CVD 1344, TRUST 1933, GATE 256, DISPOSITION 627, GATED 884, COMMITTEE 572.

### A2. The interpolation sites (post-commit text, verbatim)

**`ai-import.service.ts`** (imports :28-29 add `TRUST_SAFEGUARDS_DIRECTION`, `GATED_COMPANION_TRUST_DISPOSITION`). `SYSTEM_PROMPTS_PROMPT` is now **`export const`** (:201). It has one wording change, `(300-500 words)` → `(300-600 words)` (:209). Three items are appended after `${CONVERSATIONAL_VOICE_DIRECTION}`:
```
export const SYSTEM_PROMPTS_PROMPT = `${PROMPT_SEMANTICS}

Create system prompts that instruct an AI how to roleplay as this character.

Respond with JSON array:
[
  {
    "name": "Main",
    "content": "A comprehensive system prompt (300-600 words) covering identity, speech patterns, behaviors, boundaries, and relationship dynamics. Write in second person ('You are...', 'You always...').",
    "isDefault": true
  }
]

The main prompt should capture the character's essence from the source material. Include specific details about speech patterns, mannerisms, and reactions that make the character unique. If the source material implies distinct interaction modes or model-specific needs, you may add 1-2 additional named prompts (isDefault false) tailored to them.

${CONVERSATIONAL_VOICE_DIRECTION}

${TRUST_SAFEGUARDS_DIRECTION}

${GATED_COMPANION_TRUST_DISPOSITION}
Use the relationships array in the Prior Analysis, where one is given, as evidence for that decision.`;
```
Separators: `CVD` + `\n\n` + `TRUST` + `\n\n` + `GATED` + **`\n`** + the relationships sentence (the same paragraph as GATED), at end of string. `FIRST_MESSAGE_PROMPT` is **untouched** (context only in the diff).

**`character-wizard.service.ts`** (imports :30-31, the same two names). `FIELD_PROMPTS.systemPrompt` gets TRUST and GATED inserted between CVD and the closing sentence, and the cap changes from **`under 500` → `under 600`**. Blank lines separate every part:
```
Write as direct instructions to the AI, in second person ("You are...", "You always...").
Character facts live in the identity/description/personality/manifesto fields — the prompt directs the performance rather than restating the lore.

${CONVERSATIONAL_VOICE_DIRECTION}

${TRUST_SAFEGUARDS_DIRECTION}

${GATED_COMPANION_TRUST_DISPOSITION}

Keep it under 600 words but comprehensive.`,
```
No other `FIELD_PROMPTS` entry changes.

**`character-optimizer.service.ts`** (imports :28-30 add `TRUST_SAFEGUARDS_DIRECTION`, `COMPANION_TRUST_DISPOSITION`, `COMMITTEE_DRIFT_GUARDRAIL`. Note: it uses NOT the GATE or GATED exports, but its own framing gates). Four edits:
1. **`getAnalysisPrompt`** (:305) gets a new last "Look for" bullet, then a blank line, then COMMITTEE, then the existing blank line + `Respond with JSON:`:
```
- Nicknames or alternate names other characters repeatedly use for this character (these inform the ALIASES property)
- Committee drift — the character governing {{user}}'s persona (sign-offs, votes, conditions), contradicting narrated events, or treating silence as consent. Surface it as a pattern to CORRECT, labelled as such, never as a trait.

${COMMITTEE_DRIFT_GUARDRAIL}
```
2. **`SUGGESTION_SCHEMA_PREAMBLE`** is now `export const` (:340) **and** gains a new final rule, appended after the Scenarios rule:
```
- Scenarios describe "where and when" (setting, environment, circumstances). They should not alter the character's personality, voice, or core behavior unless the environment itself demands it.
- Never propose a trait, rule, or condition that constrains what {{user}}'s persona may do, or that requires {{user}}'s actions to be approved, witnessed, or co-signed. A pattern of that kind in the memories is drift to correct, not behaviour to capture.`;
```
   This preamble is interpolated into **all SEVEN suggestion passes** (`getGeneralFieldsSuggestionsPrompt` :396, `getScenarioSuggestionPrompt` :424, `getSystemPromptSuggestionPrompt` :458, `getPhysicalDescriptionSuggestionPrompt` :519, `getWardrobeSuggestionPrompt` :568, `getPropertiesSuggestionPrompt` :598, `getNewSystemPromptsSuggestionPrompt` :614). So the export is NOT visibility-only: the bytes move in every pass.
3. **`getSystemPromptSuggestionPrompt`** (the refine pass, :440) gets four new lines after the existing `Do NOT codify repetition` bullet. TRUST and the bare DISPOSITION sit on their own lines, unbulleted (their own text supplies the `- ` lines). COMMITTEE is bulleted (`- ${…}`):
```
Additional rules specific to system-prompt refinement:
- Set field="systemPrompt" and subId="${prompt.id}".
- currentValue must be the existing prompt content verbatim.
- proposedValue must be a complete replacement for the prompt content.
- Do NOT change the prompt's evident interaction style (e.g. a "terse" prompt should stay terse); only sharpen its articulation of the character.
- Never remove or weaken the prompt's direction about listening and conversational register (reading jokes and exaggeration for what they mean, sizing replies to the moment, rationing signature habits, saving formal language for moments that call for it).
- Do NOT codify repetition. A gesture, prop, phrase, or construction that turns up in most replies is a tic to ration, not a trait to reinforce.
- Never remove or weaken the prompt's trust safeguards (narration is fact, the conversation outranks notes, no invented setting facts, consent only in plain words, disagreement without procedure); add them in the prompt's own voice where they are missing:
${TRUST_SAFEGUARDS_DIRECTION}
- ${COMMITTEE_DRIFT_GUARDRAIL}
- If the prompt under review frames the character as {{user}}'s companion, partner, family, or crew, preserve or add the companion trust disposition; otherwise do not add it.
${COMPANION_TRUST_DISPOSITION}

Respond with a JSON array of at most one suggestion.`;
```
4. **`getNewSystemPromptsSuggestionPrompt`** (:608) gets three lines after `- ${CONVERSATIONAL_VOICE_DIRECTION}`. TRUST is bulleted, then the framing gate, then the bare DISPOSITION. **No COMMITTEE here:**
```
Additional rules specific to this pass:
- For each new system prompt: field="systemPrompt", omit subId, include a "name" field with a short descriptive label, and put the complete prompt text in proposedValue. currentValue should be the empty string.
- Be conservative: only propose a new prompt if there is a clear interaction style the existing set does not cover.
- ${CONVERSATIONAL_VOICE_DIRECTION}
- ${TRUST_SAFEGUARDS_DIRECTION}
- If the existing prompts frame the character as {{user}}'s companion, partner, family, or crew, the new prompt carries the companion trust disposition; otherwise do not add it.
${COMPANION_TRUST_DISPOSITION}

Respond with a JSON array of suggestion objects (may be empty).`;
```
So `COMMITTEE_DRIFT_GUARDRAIL` lands in **two** prompts: analysis (bare) and refine (as `- ${…}`). The general-fields, scenario, physical, wardrobe and properties passes change ONLY through the preamble rule. The "refine pass" the ledger names IS `getSystemPromptSuggestionPrompt` ("Additional rules specific to system-prompt refinement"). Every optimizer prompt the `character-optimizer-prompts` corpus records therefore moves.

**`external-prompt-generator.service.ts`**. This is the **first ever import** of `character-field-semantics` in this file (:17-21 add CVD, TRUST, GATED). `META_SYSTEM_PROMPT` is now `export const` (:48) and gains four blocks between the last Requirements bullet and the closing sentence:
```
export const META_SYSTEM_PROMPT = `You are a prompt engineering expert. Your task is to generate a standalone system prompt for an AI character, suitable for pasting into external tools like Claude Desktop, ChatGPT Custom Instructions, or similar hosted environments.

**Requirements:**
- Write the entire prompt in second person ("You are [Name]. You always...", etc.)
- Output in Markdown format with clear sections
- The prompt must be completely self-contained — it should include all personality, behavioral, speech pattern, and contextual information needed for an AI to portray this character without any additional context
- Capture the character's voice, mannerisms, and personality authentically
- Include guidance on how the character responds to different types of interactions
- If a scenario is provided, weave the setting naturally into the prompt
- If physical appearance or clothing details are provided, include them so the character can reference their own appearance
- Do NOT include meta-instructions about being an AI or breaking character
- Do NOT reference Quilltap or any external system
- The prompt should read as a coherent, well-structured character brief

${CONVERSATIONAL_VOICE_DIRECTION}

${TRUST_SAFEGUARDS_DIRECTION}

${GATED_COMPANION_TRUST_DISPOSITION}

The external tool will not substitute placeholders: wherever the directions above say {{user}}, the generated prompt names the person the character talks with in plain words ("the user", or their name if the character data gives one), never the literal {{user}} token.

Stay within the token budget specified by the user. Be thorough but concise.`;
```
Separators: `\n\n` between every block (here GATED and the placeholder sentence are NOT joined by a single `\n`, unlike ai-import). Its three consumers are unchanged in shape. They are the token estimate `Math.ceil((META_SYSTEM_PROMPT.length + userMessage.length) / 4)` (:144), the system message (:162), and the `logLLMCall` request (:205). **Side effect the ledger does not mention:** the meta prompt grows from 1227 to **5664** UTF-16 units (measured), so `estimatedInputTokens` rises by about 1110. That moves the number inside the over-budget refusal (`Estimated input: ~N tokens`), and it could flip a borderline case from within-budget to over-budget.

**The voice-direction question, answered.** At `97b25fc53` the external generator imported nothing from field-semantics (the `ca363178d` diff ADDS the import block), and the spec confirms it in §7.3 ("wire it to `character-field-semantics` for the first time"). So P4.D237's "seven sites" (2 ai-import, 2 wizard, 3 optimizer) were correct for `c3eefa752`. **CVD at the external meta prompt is a NEW, eighth site**, and v5's `external_prompt.rs` correctly does not carry it today (§B).

### A3. The 21 prompt `.md` files (`plugins/dist/qtap-plugin-default-system-prompts/prompts/`)

**Not append-only, in any file.** Every file deletes at least one line (20 files one, DEEPSEEK_COMPANION two): the existing "disagree / hold the position" line(s), rewritten with a counterweight clause (spec §6.4). Each file then adds a "Whose story it is" block in its family's native structure (`## Whose story it is`, `WHOSE STORY IT IS:` in Claude's `<instructions>`, `<WHOSE_STORY>` for Gemini, `### WHOSE STORY IT IS ###` for DeepSeek, merged `## Do`/`## Don't` bullets for Ollama), a committee failure-mode bullet, and (all but MODERN_GENERAL) the trust disposition. These go in 1–4 hunks, mid-file. Sampled with `git diff`: GENERIC_COMPANION (3 hunks: trust paragraph after the intro, two NEVER lines in place of one, the block after the Memory paragraph and before the usage note); MODERN_GENERAL (3 hunks: rewritten line, block before `## Failure modes`, a committee failure-mode bullet; no trust paragraph); OLLAMA_COMPANION (2 hunks inside `## Do` / `## Don't`).

| file | lines before→after | +/− | hunks |
|---|---|---|---|
| CLAUDE_COMPANION | 62→71 | +10 −1 | 2 |
| CLAUDE_ROMANTIC | 62→71 | +10 −1 | 3 |
| DEEPSEEK_COMPANION | 61→70 | +11 −2 | 2 |
| DEEPSEEK_ROMANTIC | 58→67 | +10 −1 | 2 |
| GEMINI_COMPANION | 70→80 | +11 −1 | 3 |
| GEMINI_ROMANTIC | 71→81 | +11 −1 | 3 |
| GENERIC_COMPANION | 68→79 | +12 −1 | 3 |
| GENERIC_ROMANTIC | 75→86 | +12 −1 | 4 |
| GPT4O_COMPANION | 72→82 | +11 −1 | 2 |
| GPT4O_ROMANTIC | 68→78 | +11 −1 | 3 |
| GPT5_COMPANION | 63→73 | +11 −1 | 1 |
| GPT5_ROMANTIC | 67→77 | +11 −1 | 2 |
| GROK_COMPANION | 68→78 | +11 −1 | 2 |
| GROK_ROMANTIC | 70→80 | +11 −1 | 3 |
| MISTRAL_COMPANION | 68→78 | +11 −1 | 2 |
| MISTRAL_ROMANTIC | 69→79 | +11 −1 | 2 |
| MODERN_GENERAL | 87→96 | +10 −1 | 3 |
| MODERN_PLATONIC | 91→101 | +11 −1 | 3 |
| MODERN_ROMANTIC | 84→94 | +11 −1 | 2 |
| OLLAMA_COMPANION | 55→62 | +8 −1 | 2 |
| OLLAMA_ROMANTIC | 58→65 | +8 −1 | 2 |

Totals: 21 files, +223 −22 (`git diff --numstat`). `index.js` reads the `.md` files at RUNTIME (`loadPrompts()` → `readdirSync(prompts).filter(.md).sort()` + `readFileSync`, `index.js:21522-21533`) and embeds no prompt text, so the dump through the real `index.js` picks up the new bytes with no plugin rebuild.

**Plugin version stamps:** `index.ts:65` and `index.js:21539` go `'1.1.0'` → `'1.1.25'` (the stale stamp the registry stores, spec §6.6). `manifest.json` and `package.json` go `1.1.24` → `1.1.25`. The ledger's "1.1.0→1.1.25 in index.ts/index.js/manifest.json/package.json" conflates the two. **README** (+10): a new section `## Shared block: "Whose story it is" (1.1.25)` describing the block, the per-family heading forms, the disposition-except-MODERN_GENERAL rule, and the new content test. It is docs only. v5 records the plugin version nowhere (grep: only the catalogue JSON, the guard, the dump, and `bundled-plugin-secret-keys.json` (`[]`) name the plugin), so the version is **NO-PORT**.

### A4. v4 tests added or extended (oracle shapes)

- `__tests__/unit/plugins/default-system-prompts-content.test.ts` (NEW, 84 lines, "140 cases" per spec §15). Reads `prompts/*.md` from `process.cwd()`. Asserts: `files` has length 21. Per file: `toMatch(/\{\{user\}\}[^.\n]{0,40}narrat/)`, `toContain('plain words')`, `toMatch(/committee|sign-offs/)`. MODERN_GENERAL `not.toContain('starting point')`, every other file `toContain('starting point')`. `not.toContain('((')`, plus no misspelled project name (regex built from halves). Thirteen `PRESERVED_DISAGREE_LINES` phrases must survive (e.g. MODERN_GENERAL `'A character who always yields is nobody'`, OLLAMA_COMPANION `'keep them when challenged'`). This is the shape a v5 guard extension could mirror over the vendored JSON.
- `__tests__/unit/lib/services/ai-import-system-prompts.test.ts` (NEW). `SYSTEM_PROMPTS_PROMPT` contains CVD and TRUST. `indexOf(COMPANION_TRUST_DISPOSITION) > indexOf(COMPANION_TRUST_DISPOSITION_GATE) > -1`. `toMatch(/relationships array in the Prior Analysis/)`. `toContain('300-600 words')`.
- `character-wizard-prompts.test.ts` (+21). `FIELD_PROMPTS.systemPrompt` contains TRUST and matches `/not a committee/i`. Gate index > -1 and disposition index > gate. `toMatch(/under 600 words/)`.
- `character-optimizer-helpers.test.ts` (+57). Refine pass keeps `"Do NOT change the prompt's evident interaction style"`, `"Never remove or weaken the prompt's direction about listening"`, `'Do NOT codify repetition'`, and contains TRUST, COMMITTEE and `"Never remove or weaken the prompt's trust safeguards"`. `indexOf('If the prompt under review frames the character as {{user}}') < indexOf(COMPANION_TRUST_DISPOSITION)`. `SUGGESTION_SCHEMA_PREAMBLE` contains `"Never propose a trait, rule, or condition that constrains what {{user}}'s persona may do"` and matches `/drift to correct, not behaviour to capture/`. The preamble is contained in the general and new-prompts passes. Analysis matches `/Committee drift/` and contains COMMITTEE. New-prompts pass contains TRUST, with the `'If the existing prompts frame the character as {{user}}'` gate before DISPOSITION.
- `external-prompt-generator.test.ts` (+25). `META_SYSTEM_PROMPT` contains CVD and TRUST, gate before disposition, `toMatch(/never the literal \{\{user\}\} token/)`.
All of these are containment and order assertions. v5's tier-1/tier-3 families are byte-exact and strictly stronger, so the v4 tests are oracle shapes for v5 unit pins (gate-before-disposition order especially), not new families.

### A5. The spec — `docs/developer/features/prompt-trust-and-anti-committee.md` (§5–§7, §13, §15; §8–§9 are phases 3–4, other lanes)

- **§5.** There are two tiers. The UNIVERSAL safeguards (narration is fact, check the conversation, no invented setting facts, consent in plain words, disagree without procedure) apply to every prompt, including adversarial ones. The RELATIONSHIP-SCALED trust disposition is for companion, platonic and romantic only. The first rule governs what happened, never what `{{char}}` does next.
- **§7.1.** The three generator constants sit beside CVD and are NOT mirrored in `field-hints.ts` (they are generator directions, not field definitions).
- **§7.2.** Gating is by INFERENCE from the material, default omit (decision 3). No generator knows the relationship as data. Summon From Lore points the model at the Prior Analysis `relationships` array.
- **§7.3.** The Wizard and Summon caps rise from 500 to 600 words (decision 4; the Wizard field's 1000-token cap needs no change). The optimizer's analysis pass flags committee drift as a pattern to CORRECT. The preamble rule covers "every pass at once". The refine pass extends "never remove or weaken" to the safeguards. The refine and new-prompt passes gate the disposition on the prompt's own framing. The external generator is wired to field-semantics "for the first time".
- **§15 (as built).** It adds two exports beyond the spec (GATE and GATED), "so the Wizard, Summon From Lore and External Prompt share one gated paragraph and the tests can check order". The optimizer keeps its own framing-based gate before the bare disposition. Three constants are exported "for tests". The external meta prompt adds the no-literal-`{{user}}` sentence "since external tools do not substitute placeholders".
- **§13 follow-ups (not in this commit):** an optimizer `partnerLinks` read and a hash/versioned built-in refresh. Neither is to be ported now.
- The ledger asks for the spec to be mirrored into `docs/v4/`. It is not mirrored yet (`find docs/v4 -name 'prompt-trust*'` is empty). That is a docs task the round must assign; it is NOT this lane's by default.

## B. v5 on `main` (`735cf568e`)

| surface | file:line | state | what the lane does |
|---|---|---|---|
| vendored catalogue | `crates/quilltap-core/src/services/builtin_prompt_templates.json` | 21 rows `{promptId,name,content,modelHint,category}`; P4.D237 (`c3eefa752`) bytes; 0 rows contain "Whose story"/"WHOSE" | REGENERATE by the dump |
| catalogue loader | `services/builtin_prompt_templates.rs:126` `catalogue()` (`include_str!`) | unchanged | none (doc :83 says "`1.1.24` at `c3eefa752` … NO-PORT"; optionally restamp `1.1.25` at `ca363178d`) |
| dump | `harness/oracle/provision/dump-prompt-templates.ts` | drives v4's REAL `index.js` + `systemPromptRegistry`; recipe in header (`$N/npx tsx …` from the checkout, Node 24) | run at the pin; no edit |
| guard | `crates/quilltap-harness/tests/builtin_prompt_templates_guard.rs` | re-derives all 21 from `$QT_V4_CHECKOUT` (default the LIVE checkout) raw `.md`; **RED on main now** (live checkout = `ca363178d`) | none; goes green after re-vendor |
| seeder refresh | `services/builtin_prompt_templates.rs:145` `row_matches_shipped` (content, description, category, modelHint, each `as_deref() == Some(..)`, NULL never matches); `:163` `needs_seeding` (table absent / name absent / `LIMIT 1` row `!row_matches_shipped`); `:197` `seed_sample_prompts` (insert / no-op / `repo.seed_refresh` + `Built-in prompt template refreshed from shipped text`) | **keys on CONTENT, confirmed: no version key anywhere in the comparison or the probe.** A re-vendor alone makes every seeded built-in row refresh on the first prompt-template read (list/get/put/delete via `api/prompt_templates.rs:359` `seed_sample_prompts_if_needed`, called at :408/:503/:541/:632) | NO code change |
| field semantics (GENERATED) | `generators/field_semantics.rs` (102 lines; `ALL_EXPORTS` :87, 8 entries); generator `harness/oracle/tools/gen-field-semantics.mjs` (`rawString` picks a safe hash level; `DOC` map :46-65, 8 entries); case `harness/oracle/cases/generators-field-semantics.ts` (HAND list, 8) | no trust constants | add 5 names to the case's import + array **in v4 source order (between CVD and EXAMPLE_DIALOGUE_COVERAGE)**; add 5 `DOC` lines (else "A character field-semantics export."); also refresh CVD's DOC line, which lists three consumers and now needs a fourth: the External Prompt meta prompt. Regenerate: `ALL_EXPORTS` 8 → **13** |
| wizard prompts (GENERATED) | `generators/wizard_prompts.rs:170` `FIELD_PROMPT_SYSTEM_PROMPT` (RESOLVED text; ends `Keep it under 500 words but comprehensive."#;` :191); generator `gen-wizard-prompts.mjs`; case `generators-wizard-prompts.ts` iterates `Object.entries(FIELD_PROMPTS)` (no hand list) | P4.D237 bytes | regenerate only |
| ai-import | `generators/ai_import.rs:50-53` import; `:198` `first_message_prompt()` (fn over the raw `FIRST_MESSAGE_PROMPT_HEAD` :187; **unchanged at `ca363178d`**); `:204-223` `system_prompts_prompt()` = `format!(r#"…"#)`; `(300-500 words)` at :214; ends `{CONVERSATIONAL_VOICE_DIRECTION}"#` :221; used at :1686 | P4.D237 bytes | hand: `300-500`→`300-600`; append `\n\n{TRUST_SAFEGUARDS_DIRECTION}\n\n{GATED_COMPANION_TRUST_DISPOSITION}\nUse the relationships array in the Prior Analysis, where one is given, as evidence for that decision.`; import the two names; doc comment |
| optimizer | `generators/optimizer.rs:45-48` import; `:92-112` `SUGGESTION_SCHEMA_PREAMBLE` (`pub const &str` raw; last rule :112); `:417` `get_analysis_prompt` (`format!`; Nicknames bullet :435, `Respond with JSON:` :437); `:452` general (`{SUGGESTION_SCHEMA_PREAMBLE}` :480); `:519` refine (codify bullet :544, `Respond…` :546); `:709` new-prompts (`- {CONVERSATIONAL_VOICE_DIRECTION}` :721); preamble interpolated at :480/:504/:536/:608/:655/:701/:716 (7 sites) | P4.D237 bytes | hand: the four edits of §A2 (import TRUST, COMPANION_TRUST_DISPOSITION, COMMITTEE). Doc nit: :90 says the preamble is "appended to six of the seven suggestion prompts", but it is SEVEN of seven in both v4 and v5. Fix in passing |
| external prompt | `generators/external_prompt.rs:89-103` `pub const META_SYSTEM_PROMPT: &str = r#"…"#`; **no field_semantics import, no CVD** (correct for the baseline); consumers :365 (`utf16_len` token estimate), :414 (system message), :493 (log request); no unit tests in the file; no other crate references it | baseline bytes | hand: cannot stay a `const &str` (`concat!` takes literals, not consts). Becomes a `fn meta_system_prompt() -> String` or a `LazyLock<String>`; three consumer edits; module doc gains a line |
| app version | `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:115` `V4_APP_VERSION = "4.10.0-dev.100"` | the ONLY holder in the tree (grep `4.10.0-dev.` over `crates/` + `harness/`: this line plus `qtap_schema.rs:141`'s unrelated `4.10.0-dev.1` schema example) | → `4.10.0-dev.105` (at the `ca363178d` pin) |

The `wizard.rs:935` `systemPrompt` maxTokens is 1000, unchanged in v4. That is neutral.

## C. Families

All are under `crates/quilltap-harness/tests/`. "Moves" means expected RED at `ca363178d` on unported `main`.

| family | oracle case | recipe (header) | fixtures | moves? |
|---|---|---|---|---|
| `builtin_prompt_templates_guard` | — (reads the `.md` directly) | header :32-38: `$N/npx tsx $V5W/harness/oracle/provision/dump-prompt-templates.ts` (re-vendor), then `cargo test --test builtin_prompt_templates_guard`; `QT_V4_CHECKOUT` | none | **RED now** (live checkout) |
| `prompt_templates_routes_equivalence` | `cases/prompt-templates-routes.test.ts` (jest) + builder `fixtures/build-prompt-templates-routes-fixture.ts` (rewrites `prompt-templates-routes.json`) | header :45-64 (`QT_FIXTURE_PT_ROUTES_MAIN`, `QT_ORACLE_OUT`, then `QT_ORACLE_PT_ROUTES` + `QT_FIXTURE_PT_ROUTES`) | real-DB built fresh in /tmp | **RED expected**. Every seeding case's body/table carries seeded `content`, and the plants (`catalogueCurrent`, `builtin*`) take "shipped" values from v4's REAL registry in the oracle and from v5's vendored catalogue in the harness. GREEN after the re-vendor with **no harness edit** (seeder unchanged). Header :39-43 still prescribes "fixture at `acadcc7cd`, oracle at `c3eefa752`". That two-worktree rule is stale (§D10) |
| `prompt_templates_tier2_equivalence` | `cases/prompt-templates-tier2.ts` + `fixtures/build-prompt-templates-fixture.ts` | header :19-30 (`QT_FIXTURE_OUT`, `QT_FIXTURE_PROMPT_TEMPLATES`, `QT_ORACLE_PROMPT_TEMPLATES`) | real-DB built fresh | NEUTRAL. P4.D237 measured its NDJSON byte-identical across a catalogue change, and no tier-2 op reaches a seeding read. Re-run as the neutrality leg |
| `generators_leaf_equivalence` | `cases/generators-leaf.ts` (HAND list: import :40-48, array :134-143, 8 names; `coverage.fieldSemantics`) | header :17-21 (`QT_ORACLE_GENERATORS_LEAF`) | committed corpus | ⚠ **stays GREEN unported** until its hand list grows to 13; then RED until `field_semantics.rs` is regenerated |
| `generators_wizard_prompts_equivalence` | `cases/generators-wizard-prompts.ts` | header :18-22 (`QT_ORACLE_GENERATORS_WIZARD_PROMPTS`) | `fixtures/generators-wizard-prompts.json` | RED (`systemPrompt` only) |
| `character_optimizer_prompts_equivalence` | `cases/character-optimizer-prompts.ts` (records `analysis_prompt`, `general_prompt`, `new_prompts_prompt`, `scenario_prompt`, `system_prompt_prompt`, physical/wardrobe/properties) | header :15-19 (`QT_ORACLE_CHARACTER_OPTIMIZER_PROMPTS`) | `fixtures/character-optimizer-prompts.json` | RED in EVERY prompt kind (analysis + all seven passes via the preamble) |
| `character_optimizer_tier3_equivalence` | `cases/character-optimizer-tier3.test.ts` (jest, `/tmp` mirror) | header :30-46 (`QT_FIXTURE_CG_MAIN`/`_MOUNT`, `QT_ORACLE_OUT`, `QT_ORACLE_CHARACTER_OPTIMIZER`) | committed `crates/quilltap-web/tests/fixtures/character-generators-{main,mount}.db` + `fixtures/character-generators.json`, `character-optimizer-tier3.json` | RED. Expect MORE than P4.D237's 18 cases, since the analysis call and every pass call now move |
| `character_wizard_tier3_equivalence` | `cases/character-wizard-tier3.test.ts` | header :24-40 (`QT_ORACLE_CHARACTER_WIZARD`) | committed CG pair + `character-wizard-tier3.json` | RED where a case generates `systemPrompt` (P4.D237 measured 2) |
| `ai_import_tier3_equivalence` | `cases/ai-import-tier3.test.ts` | header :62-78 (`QT_ORACLE_AI_IMPORT`) | committed CG pair + `ai-import-tier3.json` | RED. Trips on `V4_APP_VERSION` first (dev.100 → dev.105), then the `system_prompts` step bytes (P4.D237: 24 cases) |
| `external_prompt_tier3_equivalence` | `cases/external-prompt-tier3.test.ts` | header :23-39 (`QT_FIXTURE_CHARACTERS_MAIN`/`_MOUNT`, `QT_ORACLE_EXTERNAL_PROMPT`) | committed `crates/quilltap-web/tests/fixtures/characters-{main,mount}.db` + `characters.json`, `external-prompt-tier3.json` | **RED, a NEW family for this surface** (not in P4.D237's ten). The recorded system message moves on every model-call case, and `over_budget_refuses` (maxContext 1200) has its `Estimated input` number move. `profile_max_context_within_budget` (maxContext 8000, maxTokens 1000) should stay within budget, but measure it (§D9) |
| `ai_import_assembly_equivalence` | `cases/ai-import-assembly.ts` | header :21-25 (`QT_ORACLE_AI_IMPORT_ASSEMBLY`) | `fixtures/ai-import-assembly.json` | NEUTRAL (`CHARACTER_BASICS_PROMPT` = `FULL_FIELD_SEMANTICS`, unchanged). Cross-pin `cmp` differs only in minted UUIDs (P4.D237's note) |

There is no `generators_field_semantics` harness test. `generators-field-semantics.ts` only feeds `gen-field-semantics.mjs`, and the comparison is `generators_leaf`'s.

Neutral and outside the differential (no prompt bytes pinned, measured by grep): `crates/quilltap-web/tests/{prompt_templates_web_routes,generators_wizard_routes,characters_generators_routes,characters_import_route,generator_sse_wire}.rs`, and Playwright `prompt-templates-flow.spec.ts` (asserts the `MODERN General` name, description and `{{char}}` presence only), `character-{optimizer,wizard,external-prompt}-flow.spec.ts`, and `ai-import-flow.spec.ts`. Run the prompt-templates e2e beat once after the re-vendor as a liveness check.

## D. Traps and premises (measured)

1. **Re-vendor by the DUMP, never by hand.** All 21 bodies change mid-file (at least one rewritten line each, plus 1–4 inserted blocks). No append shortcut exists. The dump reads the `.md` at runtime through the real `index.js`, so no plugin build is needed.
2. **The guard reads the LIVE checkout.** It is red on `main` today and green after the re-vendor as long as v4 stays at `ca363178d` for these files.
3. **The seeder refresh keys on content: CONFIRMED** (`row_matches_shipped` :145, the four fields, NULL-as-different; `needs_seeding` :163 uses the same predicate). No version stamp is read or stored. So the re-vendor alone makes every existing instance's 21 built-in rows refresh lazily on the next prompt-template read. On a real Friday copy, expect up to 21 `Built-in prompt template refreshed from shipped text` INFO lines on first list (a dogfood row). User copies and characters' `systemPrompts` copies are untouched (P4.D237 pinned both).
4. **`generators_leaf` stays GREEN unported**, because of its hand list (`generators-leaf.ts` :40-48 + :134-143). The same holds for `generators-field-semantics.ts`. Both lists and the `DOC` map must grow, in v4 source order. P4.D237 measured exactly this trap last round.
5. **`FIRST_MESSAGE_PROMPT` is untouched at `ca363178d`.** There is no brace-escaping work there this time. P4.D237 already made it a fn over a raw head.
6. **The `{{user}}` literal inside `format!` templates.** The trust constants carry `{{user}}`, but they are captured args (`{TRUST_SAFEGUARDS_DIRECTION}`), so they need no escaping. **Escaping is needed for every `{{user}}` written INTO a `format!` template string:**
   - The optimizer analysis bullet (`governing {{user}}'s persona`), the refine pass's framing gate (`as {{user}}'s companion`) and the new-prompts framing gate (`as {{user}}'s companion`). All three sit inside `format!(r#"…"#)` and must be written `{{{{user}}}}`.
   - The external meta prompt's placeholder sentence (two `{{user}}`) needs the same escaping if built with `format!`; it is literal if built by `String` concatenation or a raw const + `push_str`.
   - The preamble's new rule (two `{{user}}`) sits in a plain raw `&str` const, so it is LITERAL and must NOT be doubled.
   - **There is no `{{{{` precedent in `optimizer.rs` / `ai_import.rs` / `external_prompt.rs` today** (grep). This is the first, and `optimizer.rs`'s existing escapes are all JSON `{{`/`}}`. A wrong count compiles fine and yields `{user}` or `{{{{user}}}}` on the wire: only the byte-exact families catch it.
7. **`COMMITTEE_DRIFT_GUARDRAIL` lands in two prompts:** analysis (bare paragraph) and refine (`- ${…}` bullet). It is not in new-prompts and not in the preamble. The preamble's new rule is a DIFFERENT sentence (it overlaps in meaning, not in bytes).
8. **`SUGGESTION_SCHEMA_PREAMBLE`'s export is not visibility-only.** The same hunk appends a rule, so its bytes change and ride into all seven passes. v5's twin is already `pub`, so only the bytes move. (The same holds for `SYSTEM_PROMPTS_PROMPT`: v5's `system_prompts_prompt()` is already `pub`. And for `META_SYSTEM_PROMPT`: v5's is already `pub const`, but it must change form, see §B.)
9. **The external meta prompt grows 1227 → 5664 UTF-16 units**, so the estimated input grows by about 1110 tokens. The over-budget refusal message number moves, and a budget arm could flip. v4's `computeSafeInputLimit` = `max(1000, total − reserve − ceil(total·0.10))` (`CONTEXT_SAFETY_MARGIN_RATIO = 0.10`, `model-context-data.ts:171`). Measure `profile_max_context_within_budget` against the regenerated oracle rather than assuming.
10. **The two-worktree fixture rule is stale.** The routes header (:39-43) still says build at `acadcc7cd`, oracle at `c3eefa752`, because v5 then could not open a post-`f7f3d7bf0` DB. P4.D235 landed the tolerate-both-shapes read and the baseline is now `97b25fc53` (post-drop), and `ca363178d` moves no DDL. So a single pin at `ca363178d` for both builder and oracle should work. Measure it once and update the header line, or keep building at `97b25fc53`.
11. **Ledger omissions found by reading the whole hunks:**
    - (i) The preamble rule reaches the scenario, physical, wardrobe and properties passes too, not just "general".
    - (ii) The optimizer uses the BARE `COMPANION_TRUST_DISPOSITION` under its OWN two framing-gate sentences (different wording in refine vs new-prompts), not `GATED_…`.
    - (iii) The refine pass also gains a "Never remove or weaken the prompt's trust safeguards (…)" bullet.
    - (iv) The ai-import relationships sentence joins GATED with a single `\n`, while the external sentence is `\n\n`-separated.
    - (v) The external token-estimate side effect (§D9).
    - (vi) The plugin version: `index.ts`/`index.js` `1.1.0`→`1.1.25`, the JSONs `1.1.24`→`1.1.25`, plus a README section.
    - Nothing else in the four service files moved. Their diffs are exactly the hunks quoted in §A2.
12. **The plugin version stamp is NO-PORT.** v5 records it nowhere. The `builtin_prompt_templates.rs:83` doc's "`1.1.24` at `c3eefa752`" may be restamped `1.1.25` at `ca363178d`.
13. **The help pages are not this lane's.** `ca363178d`'s `help/` hunks for (a)/(b) are `prompts.md` (+16 −1), `character-optimizer.md` (+4), `ai-character-import.md`, `character-creation.md` and `character-external-prompt.md` (+2 each). They belong to the round's whole-tree help re-vendor. `help_tree_equivalence` and the embed guard stay red until that lane lands, not this one.
14. **Optional tier-1 strengthening.** v4 now exports `SYSTEM_PROMPTS_PROMPT`, `SUGGESTION_SCHEMA_PREAMBLE` and `META_SYSTEM_PROMPT` "for tests". Cheap direct rows are now possible: `ai-import-assembly.ts` could record `SYSTEM_PROMPTS_PROMPT`, and `character-optimizer-prompts.ts` could record the bare preamble. The existing families already cover the bytes through the builders / tier-3 model calls, so this is a Tier-2 nicety, not a gap. `META_SYSTEM_PROMPT` has no tier-1 home today (only `external_prompt_tier3`'s recorded messages).

## E. Ownership: the exact v5 files P4.D241 edits

- `crates/quilltap-core/src/services/builtin_prompt_templates.json`: REGENERATED (dump at the pin).
- `crates/quilltap-core/src/services/builtin_prompt_templates.rs`: doc line :83 restamp only (optional).
- `crates/quilltap-core/src/generators/field_semantics.rs`: REGENERATED. `crates/quilltap-core/src/generators/wizard_prompts.rs`: REGENERATED.
- `crates/quilltap-core/src/generators/ai_import.rs`, `optimizer.rs`, `external_prompt.rs`: HAND (§B). Plus each consumer of `META_SYSTEM_PROMPT` inside `external_prompt.rs` (no consumer exists outside it).
- `harness/oracle/cases/generators-field-semantics.ts` and `harness/oracle/cases/generators-leaf.ts`: the hand lists, 8 → 13. `harness/oracle/tools/gen-field-semantics.mjs`: the `DOC` map, +5 and the CVD line refreshed.
- `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:115`: `V4_APP_VERSION` → `4.10.0-dev.105`. This is the ONLY `V4_APP_VERSION` holder in the tree, so there is nothing to leave to the CLI/help lane on that front. If a sibling lane also targets the version, the unifier reconciles one constant.
- `crates/quilltap-harness/tests/prompt_templates_routes_equivalence.rs`: header-only, if §D10 is updated. Its oracle case and builder need no change.
- Optional (§D14): `harness/oracle/cases/ai-import-assembly.ts` / `character-optimizer-prompts.ts` plus their harness readers.
- `CHANGELOG`, `docs/developer/porting/status-log.md` (the lane record).
- **Touches NOTHING in** `memory_tasks.rs`, `build_context.rs`, `message_context.rs`, `crates/quilltap-host`, `crates/quilltap-cli`, or `help/`. Measured: none of this lane's v4 hunks reach those, and no v5 file above imports from them for these prompts.

## Summary

1. The five constants are quoted verbatim in §A1. `GATED_…` = GATE + `\n` + DISPOSITION. None of them joins `FULL_FIELD_SEMANTICS`, so `CHARACTER_BASICS_PROMPT` and `ai_import_assembly` are neutral.
2. **External voice direction:** CVD is NEW at `META_SYSTEM_PROMPT`. The external generator imported nothing from field-semantics before `ca363178d`. P4.D237's seven sites were right, this is an eighth, and v5 correctly lacks it today.
3. `META_SYSTEM_PROMPT` must stop being a `const &str` in v5 (it now interpolates three consts). It grows 1227 → 5664 UTF-16 units, which moves the external tier-3 over-budget message.
4. The optimizer gets four edits: analysis (bullet + COMMITTEE), preamble (+1 rule into ALL SEVEN passes), refine (safeguards bullet + TRUST + `- COMMITTEE` + framing gate + bare DISPOSITION), new-prompts (`- TRUST` + framing gate + bare DISPOSITION). COMMITTEE appears in two prompts.
5. ai-import: 300-500 → 300-600, plus TRUST, GATED and the relationships sentence (single `\n`). Wizard: TRUST and GATED, under 500 → under 600 (regenerated). `FIRST_MESSAGE_PROMPT` is unchanged.
6. The `{{user}}` escaping trap is new to these files: three `format!` sites need `{{{{user}}}}`, while the preamble const must keep it literal.
7. All 21 `.md` files change mid-file (each rewrites at least one existing line). Re-vendor by the dump. The seeder refresh keys on content (confirmed), so no code change is needed and the rows refresh lazily.
8. Moving families: guard (red now), routes (green after re-vendor with no edit), leaf (stays green until the hand list goes 8→13), wizard_prompts, optimizer_prompts, optimizer_tier3, wizard_tier3, ai_import_tier3 (+ `V4_APP_VERSION` → dev.105), and external_prompt_tier3 (NEW to this surface). Neutral: tier2 and assembly.
9. Ledger misses: the preamble reaching every pass, the optimizer's own framing gates, the refine "never weaken safeguards" bullet, the token-estimate side effect, and the split plugin version stamps.
