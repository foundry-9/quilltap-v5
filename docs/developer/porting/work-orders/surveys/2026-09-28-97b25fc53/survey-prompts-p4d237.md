# Survey: v4 `c3eefa752` — "Teach built-in character prompts to listen and match register"
Dated 2026-09-28. v4 at `97b25fc53` (clean), baseline `acadcc7cd`. Read-only survey; every claim below is from the shipped hunks / post-commit files, not the message. No later commit (`04d6c9d52`, `97b25fc53`) touches any file this commit touches (`git diff c3eefa752 HEAD` over them is empty), so HEAD == the pin for this surface.

## A. v4, hunk by hunk

### A1. The seeder — `lib/database/repositories/prompt-templates.repository.ts`
(NOTE the path: `lib/database/`, not `lib/data/` as the ledger reading says.)

`_doSeedSamplePrompts` (post-commit :49-74), inside `safeQuery(..., 'Error seeding sample prompts', {}, undefined)`:
```
const registryPrompts = systemPromptRegistry.isInitialized() ? systemPromptRegistry.getAll() : [];
if (registryPrompts.length > 0) {
  for (const prompt of registryPrompts) { await this.upsertBuiltInPrompt(prompt, 'plugin'); }
  return;
}
const samplePrompts = await loadSamplePrompts();
for (const sample of samplePrompts) { await this.upsertBuiltInPrompt(sample, 'filesystem'); }
```
(The old filesystem arm's `if (samplePrompts.length === 0) return;` and the up-front `getCollection()` are gone — behaviour-neutral.)

New `private async upsertBuiltInPrompt(prompt: {name, content, category, modelHint}, source: 'plugin'|'filesystem')` (:82-151), verbatim control flow:
1. `collection = await this.getCollection()`; `description = \`${prompt.category} prompt optimized for ${prompt.modelHint} models\``; `now = this.getCurrentTimestamp()` — **computed ONCE per prompt, BEFORE the lookup** (so the insert's createdAt/updatedAt and the refresh's updatedAt are the same per-row `now`).
2. The read: `collection.findOne({ name: prompt.name, isBuiltIn: true })` — unchanged lookup key (name + isBuiltIn only).
3. Absent → insert `{id: generateId(), userId: null, name, content, description, isBuiltIn: true, category, modelHint, tags: [], createdAt: now, updatedAt: now}` through `this.validate(...)` → `insertOne`; then
   `logger.info('Sample prompt template seeded', { templateId: id, name: prompt.name, source, modelHint: prompt.modelHint, category: prompt.category })`; return.
   - WORDING CHANGE for BOTH sources: old `'Sample prompt template seeded from plugin'` {templateId, name, **promptId**, modelHint, category} and `'…seeded from filesystem'` {templateId, name, modelHint, category} are both replaced by the one line above. `promptId` is DROPPED; `source` is new and sits THIRD (after name, before modelHint).
4. Present → comparison, in this order, all `===`:
   `existing.content === prompt.content && existing.description === description && existing.category === prompt.category && existing.modelHint === prompt.modelHint` → return (no-op, no log, no write).
   - `existing` comes through v4's SQLite backend hydrate (NULL → `undefined`), so a row with NULL description/category/modelHint is `!==` any string → REFRESHES.
5. Differs → `collection.updateOne({ id: existing.id }, { $set: { content: prompt.content, description, category: prompt.category, modelHint: prompt.modelHint, updatedAt: now } })` — a RAW `$set`, bypassing the repo's own `update()` (which refuses built-ins with `'Cannot update built-in prompt template'`). `name`, `tags`, `userId`, `createdAt`, `isBuiltIn` untouched.
   then `logger.info('Built-in prompt template refreshed from shipped text', { templateId: existing.id, name: prompt.name, source })` — THREE fields only.
6. User edits: built-in rows are not user-editable through any route (PUT → 403 `Cannot update built-in templates`, repo `update()` refuses), so the only "edited built-in" is a direct-DB edit or an older shipped text — and the refresh DOES clobber it. The docblock says so: "Built-ins are read-only to the user (their edits go to copies), so the shipped source is authoritative." A user template that shares the name (`isBuiltIn: false`) is never matched by the lookup, never touched. Characters that imported a template hold their own copy (characters.systemPrompts) — nothing here reads or writes characters.

**How/when it runs — ⚠ NOT at boot.** `seedSamplePrompts()` (:36-47, `seedingPromise` single-flight, reset in `finally` so it runs AGAIN on every call) is invoked ONLY from inside the repo's own `findById` (:164), `findAll` (:181), `findAllForUser` (:211). There is no startup call (grep over lib/ app/ packages/ finds none). Callers that therefore trigger seed+refresh: `app/api/v1/prompt-templates/route.ts:32` (GET list, `findAllForUser`), `[id]/route.ts:35/58/113` (GET/PUT/DELETE via `findById`), `app/api/v1/system/tools/route.ts:551` (export entity list `prompt-templates`, `findAll`), `lib/tools/almanack/phase3-ledgers.ts:274` (`findAllForUser`), `lib/export/quilltap-export-service.ts:325/334`, `lib/export/ndjson-writer.ts:840/985`. So the "boot write" the order anticipates is really a **lazy write on the first template read after upgrade** (and a no-op check on every later one).

### A2. The two constants — `lib/services/character-field-semantics.ts` (post-commit :40-64), VERBATIM (LLM-input bytes; straight ASCII quotes around "not X — Y", em-dashes U+2014, no backslashes/backticks/braces):

```
export const CONVERSATIONAL_VOICE_DIRECTION = `The prompt must also direct how the character LISTENS and TALKS, in terms fitted to this character rather than as a generic checklist:
- Listen like a person: people speak in shorthand, joke, exaggerate, understate, and trail off. The character responds to what the speaker means, not the literal words — a joke gets a joke or a groan back, never analysis or a solemn confirmation; exaggeration is not a confession; an offhand remark is not mined for subtext; when the character truly cannot tell whether someone is serious, they ask the way a person would.
- Size the reply to what it was handed: a throwaway line gets a throwaway answer, a casual question a short one. The character answers rather than restating the speaker's words first.
- Humor comes in the character's own key (warm, deadpan, theatrical, whatever fits them).
- Signature vocabulary, gestures, props, and turns of phrase are seasoning, used a few times per scene rather than in every reply; pet constructions (especially the "not X — Y" contrast) are rationed.
- Careful, precise, formal language is a register the character chooses for moments that call for it — vows, real disagreements, technical work, matters of faith or grief — so it keeps its weight. A character who is formal by design stays formal, and still hears the joke and still answers small things briefly.`;
```
(declared at :48)
```
export const EXAMPLE_DIALOGUE_COVERAGE = `The exchanges together must show the character:
- catching a joke or a bit of exaggeration and answering it in kind, in their own humor, without analysing it;
- answering a casual, offhand line briefly — a line or two, no report, no restating what was said;
- getting serious when something actually matters, with fuller and more careful language.
Keep most replies roughly the size of the line they answer. Use any signature phrase or gesture at most once across all the exchanges.`;
```
(declared at :60). Both sit between PROMPT_SEMANTICS (:38) and PROPERTIES_SEMANTICS (:71). **FULL_FIELD_SEMANTICS (:91) does NOT include them** — it is unchanged, so every consumer of FULL_FIELD_SEMANTICS (incl. CHARACTER_BASICS_PROMPT) is byte-neutral.

### A3. Interpolation sites (post-commit lines, surrounding text verbatim)

**ai-import.service.ts** (imports at :27-28)
- `FIRST_MESSAGE_PROMPT` (:189-197): the JSON example block now ends
  ```
    "exampleDialogues": "2-3 example dialogue exchanges ... Separate exchanges with a blank line."
  }

  For exampleDialogues: ${EXAMPLE_DIALOGUE_COVERAGE}`;
  ```
  i.e. after the closing `}` → `\n\nFor exampleDialogues: ` + constant, end of string. (Was a const with no interpolation.)
- `SYSTEM_PROMPTS_PROMPT` (:199-214): after the existing last sentence `...you may add 1-2 additional named prompts (isDefault false) tailored to them.` → `\n\n${CONVERSATIONAL_VOICE_DIRECTION}` end of string.

**character-wizard.service.ts** (imports :29-30)
- `FIELD_PROMPTS.exampleDialogues` (ends :203): after `...Include *actions* and *expressions* in asterisks.` → `\n\n${EXAMPLE_DIALOGUE_COVERAGE}` end.
- `FIELD_PROMPTS.systemPrompt` (:227): between `Character facts live in the identity/description/personality/manifesto fields — the prompt directs the performance rather than restating the lore.` and `Keep it under 500 words but comprehensive.` → now `...lore.\n\n${CONVERSATIONAL_VOICE_DIRECTION}\n\nKeep it under 500 words but comprehensive.` (NOT at the end — mid-string, blank line either side).

**character-optimizer.service.ts** (imports :27-28) — new bullets verbatim:
- `getGeneralFieldsSuggestionsPrompt` (:362), inserted after `- Do NOT propose the same content under two different fields. Pick the one whose vantage point matches.` and before `- Do NOT suggest edits to title, scenarios, ...`:
  ```
  - Do NOT codify repetition. A gesture, prop, phrase, or sentence construction that turns up in most of the character's replies is a tic to ration, not a trait to reinforce; never write it into a field as something the character does constantly.
  - A suggestion for EXAMPLEDIALOGUES must keep the examples modelling listening and proportion as well as voice. ${EXAMPLE_DIALOGUE_COVERAGE}
  ```
  (:380-381; the constant's own newlines make the second bullet multi-line.)
- `getSystemPromptSuggestionPrompt` (:433), after `- Do NOT change the prompt's evident interaction style (e.g. a "terse" prompt should stay terse); only sharpen its articulation of the character.` and before the blank line + `Respond with a JSON array of at most one suggestion.`:
  ```
  - Never remove or weaken the prompt's direction about listening and conversational register (reading jokes and exaggeration for what they mean, sizing replies to the moment, rationing signature habits, saving formal language for moments that call for it).
  - Do NOT codify repetition. A gesture, prop, phrase, or construction that turns up in most replies is a tic to ration, not a trait to reinforce.
  ```
  (NB the second bullet here is a SHORTER wording than the general-pass one — "sentence construction"→"construction", "most of the character's replies"→"most replies", no trailing clause.)
- `getNewSystemPromptsSuggestionPrompt` (:596), after `- Be conservative: only propose a new prompt if there is a clear interaction style the existing set does not cover.`:
  ```
  - ${CONVERSATIONAL_VOICE_DIRECTION}
  ```
  (:607; so the line reads `- The prompt must also direct how the character LISTENS...:` followed by the constant's own `- Listen like...` lines.)

### A4. The 21 prompt `.md` files (plugins/dist/qtap-plugin-default-system-prompts/prompts/)
**NOT append-only — every file.** (md5 of the old file ≠ md5 of the new file's same-length prefix, all 21.) Each file gains 2-4 hunks: a new "Listening"/voice block inserted MID-file (e.g. GENERIC_COMPANION gains `## Listening` before `## Voice` and four bullets inside `## Voice`), and an `## Examples` block (the joke / casual / serious exchanges, with a preamble "These show listening and length, not {{char}}'s voice...") inserted BEFORE the trailing `## Memory` / [MEMORY] section — so "ends in example exchanges" is wrong: the examples precede the memory section. Seven files also delete 1-3 lines (rewrites).

| file | before→after lines | +/− |
|---|---|---|
| CLAUDE_COMPANION | 35→62 | +28 −1 |
| CLAUDE_ROMANTIC | 36→62 | +26 −0 |
| DEEPSEEK_COMPANION | 35→61 | +27 −1 |
| DEEPSEEK_ROMANTIC | 33→58 | +25 −0 |
| GEMINI_COMPANION | 40→70 | +30 −0 |
| GEMINI_ROMANTIC | 42→71 | +29 −0 |
| GENERIC_COMPANION | 37→68 | +31 −0 |
| GENERIC_ROMANTIC | 44→75 | +31 −0 |
| GPT4O_COMPANION | 43→72 | +30 −1 |
| GPT4O_ROMANTIC | 41→68 | +27 −0 |
| GPT5_COMPANION | 33→63 | +31 −1 |
| GPT5_ROMANTIC | 39→67 | +28 −0 |
| GROK_COMPANION | 39→68 | +29 −0 |
| GROK_ROMANTIC | 42→70 | +28 −0 |
| MISTRAL_COMPANION | 39→68 | +30 −1 |
| MISTRAL_ROMANTIC | 41→69 | +28 −0 |
| MODERN_GENERAL | 38→87 | +52 −3 |
| MODERN_PLATONIC | 42→91 | +49 −0 |
| MODERN_ROMANTIC | 41→84 | +43 −0 |
| OLLAMA_COMPANION | 33→55 | +22 −0 |
| OLLAMA_ROMANTIC | 36→58 | +22 −0 |

Irrelevant to the port's mechanics (the whole bodies are re-vendored by the generator), but it means no "append the tail" shortcut exists. Plugin `manifest.json` + `package.json`: `1.1.23` → `1.1.24`. Root `package.json` / `packages/quilltap/package.json`: `4.10.0-dev.97` → `4.10.0-dev.98`.

### A5. Help pages
`help/character-optimizer.md` (1 line rewritten: the tic-rationing / listening / example-coverage sentences), `help/character-system-prompts.md` (+20: a paragraph on examples + "### Technique 6: Listening and Register" with a fenced sample), `help/prompts.md` (+11: "#### Listening, and knowing when to be brief", "#### When the samples are revised" — which states the refresh and that imported/copied prompts are never altered, plus one new bullet). No new files. v5's `help/` copies are md5-identical to `acadcc7cd` for all three → a 3-page byte re-vendor; tree count unchanged.

### A6. Tests (oracle shapes)
- `__tests__/unit/lib/database/repositories/prompt-templates-seed-refresh.test.ts` (new, 131 lines): mocks the DB manager with an in-memory collection (`findOne` = every filter key `===`; `insertOne` push; `updateOne` by id → `Object.assign(row, $set)`), registry `isInitialized: true`, `getAll` → one prompt `{id: 'default-system-prompts/MODERN_GENERAL', name: 'MODERN_GENERAL', content, modelHint: 'MODERN', category: 'GENERAL'}` (note: the mock uses the FILENAME as name — the real registry would use the display name `MODERN General`; irrelevant to the mock). Three cases: (1) empty table → insertOne ×1, row `{name, content: 'new text', isBuiltIn: true, userId: null}`; (2) existing built-in `'old text'` (id 4444…, description `'GENERAL prompt optimized for MODERN models'`) + shipped `'revised text'` → no insert, updateOne ×1, same id, content refreshed; (3) same text → neither insert nor update, `updatedAt` stays `2026-07-01T00:00:00.000Z`. Logs are mocked (not asserted).
- `character-wizard-prompts.test.ts` (+19): `FIELD_PROMPTS.systemPrompt` contains CONVERSATIONAL_VOICE_DIRECTION, matches /listen like a person/i, /seasoning/i; `FIELD_PROMPTS.exampleDialogues` contains EXAMPLE_DIALOGUE_COVERAGE, matches /catching a joke/i, /casual, offhand line briefly/i, /getting serious/i.
- No optimizer or ai-import test added (v5's tier-1/tier-3 families cover those byte-exactly anyway).

### A7. What the message claims that the hunks do NOT do / say differently
- "refreshes existing built-in rows when the shipped text changes" — it also refreshes on description / category / modelHint drift (and on NULLs in those columns), not only text.
- Nothing runs at startup; the refresh is lazy inside the reads (message is silent on when; the ledger/order's "at boot" premise is FALSE for v4).
- The log-line rename for BOTH sources and the dropped `promptId` field are unmentioned.
- "Each prompt now ends with short example exchanges" — examples sit before the `## Memory` / [MEMORY] section, not at the end; and the listening guidance is inserted mid-file, so no file is append-only.
- "Summon From Lore" in the message = ai-import.service.ts (the AI import); the three generator paths match the hunks.
- The repository path is `lib/database/repositories/`, not `lib/data/repositories/` (ledger reading).

## B. v5 on main

### B1. Catalogue: `crates/quilltap-core/src/services/builtin_prompt_templates.rs` + `builtin_prompt_templates.json`
- JSON: a 21-element array of `{promptId, name, content, modelHint, category}` in the plugin's sorted-filename order; `name` is the registry DISPLAY name (`MODERN General`, not `MODERN_GENERAL`). `include_str!`'d as `BUILTIN_PROMPT_TEMPLATES_JSON`; `catalogue()` parses it.
- Generator: `harness/oracle/provision/dump-prompt-templates.ts` — drives v4's REAL plugin `index.js` through v4's REAL `systemPromptRegistry`, writes the JSON (override `QT_PROMPT_TEMPLATES_OUT`). Recipe (header): `cd ~/source/quilltap-server; $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/dump-prompt-templates.ts` (Node 24). Run it in a worktree pinned at `c3eefa752` (or HEAD — identical for these files).
- Guard: `crates/quilltap-harness/tests/builtin_prompt_templates_guard.rs` — re-derives the whole table from `$QT_V4_CHECKOUT` (default `~/source/quilltap-server`, i.e. the LIVE checkout, not a pin) `plugins/dist/qtap-plugin-default-system-prompts/prompts/*.md`: `read_dir` → `.md` filter → sort; per file v4's `parsePromptFilename` (last `_` part = category, rest = modelHint) + the display-name rule `${modelHint} ${category[0]}${category[1..].toLowerCase()}`; content = the raw file bytes (`read_to_string`, no trim). Asserts count, then per row promptId / name / modelHint / category equal and `content ==` byte-exact, with a "RE-VENDOR" message. Absent checkout → `SKIP:`.
  ⚠ **Because it reads the live checkout, this guard is RED on v5 main RIGHT NOW** (v4 HEAD `97b25fc53` carries the new text). Expect it in any gate until the lane lands.
- Seeder `seed_sample_prompts(conn, mint_id)`: `ensure_prompt_templates_table`, then per catalogue entry `built_in_name_exists(conn, name)` → skip, else `PromptTemplatesRepository::create(...)` with `now = clock::now_iso()` per row and INFO `Sample prompt template seeded from plugin` {template_id, name, prompt_id, model_hint, category} at target `quilltap::db`. **Never updates** (the module doc says so, and records the filesystem fallback as NO-COUNTERPART and the registry-empty arm as a v5-favour divergence).
- `needs_seeding(conn)`: no table → true; else true iff ANY catalogue name lacks an `isBuiltIn = 1` row. **Confirmed: names-only short-circuit.** `built_in_name_exists` (db/prompt_templates.rs:502) is `SELECT 1 FROM prompt_templates WHERE name = ?1 AND isBuiltIn = 1 LIMIT 1`.
- Where it runs: `api/prompt_templates.rs:355 seed_sample_prompts_if_needed(db)` — probe on the READ pool (`db.read_main(needs_seeding)`), then only if needed `db.write(|w| seed_sample_prompts(w.main().connection(), uuid v4))` — **through the writer task** (single-writer invariant held). Errors logged `Error seeding sample prompts` at ERROR and swallowed (v4 safeQuery). Called from the four verbs: list (:404), get (:499), update (:537, after body parse), delete (:628). NOT at boot, matching v4's site.
- Other v4 seed sites v5 does not seed at (pre-existing, now slightly more consequential because they would also refresh): almanack `phase3_ledgers.rs` (RECORDED divergence at :14-21 — "v5's report is strictly read-only"; it counts rows only, so a refresh changes nothing it reads); qtap export entity list / records (`qtap_export/records.rs:1026` filters built-ins out); system/tools `prompt-templates` entity list. All filter to user templates → output-neutral; only the side-effect differs.
- No raw built-in update helper exists: `PromptTemplatesRepository::update` (:163) refuses built-ins via `is_mutable`. The precedent to copy is `RoleplayTemplatesRepository::seed_update` (db/roleplay_templates.rs:508, raw `UPDATE ... WHERE id = ?`) used by `services/builtin_templates.rs` (the roleplay seeder that runs every boot from `quilltap-host/src/host.rs` + provisioning and "UPDATEs the six drift-tracked fields + updatedAt in place" — the exact analogue of v4's new arm).

### B2. Generators — where the prompt text lives and the insertion points
- `generators/field_semantics.rs` — **GENERATED** by `harness/oracle/tools/gen-field-semantics.mjs` from `harness/oracle/cases/generators-field-semantics.ts` NDJSON. That oracle case has a HAND list of six exports → must add `CONVERSATIONAL_VOICE_DIRECTION`, `EXAMPLE_DIALOGUE_COVERAGE` (v4 source order: after PROMPT_SEMANTICS? — the case's list order is its own; keep ALL_EXPORTS consistent with generators-leaf's order). The generator's `DOC` map needs two doc lines (else it emits the generic "A character field-semantics export."). Regen → `ALL_EXPORTS` grows 6 → 8.
- `generators/wizard_prompts.rs` — **GENERATED** by `harness/oracle/tools/gen-wizard-prompts.mjs` from `harness/oracle/cases/generators-wizard-prompts.ts` (which iterates `Object.entries(FIELD_PROMPTS)` — no hand list, picks up the change automatically). Constants hold RESOLVED text (semantics inlined), so a regen at the pin rewrites `FIELD_PROMPT_EXAMPLE_DIALOGUES` (:145) and `FIELD_PROMPT_SYSTEM_PROMPT` (:164) with the constants inlined. No hand edit.
- `generators/ai_import.rs` — HAND-written (format! with inline `{CONST}` captures). Imports at :49-51 (`FULL_FIELD_SEMANTICS, PHYSICAL_DESCRIPTION_SEMANTICS, PROMPT_SEMANTICS, PROPERTIES_SEMANTICS`) gain the two.
  - `FIRST_MESSAGE_PROMPT` (:185-192) is a `pub const &str` raw string; its only user is `run_step(..., FIRST_MESSAGE_PROMPT, ...)` at :1648. Must become a `fn first_message_prompt() -> String` (or `format!`) appending `\n\nFor exampleDialogues: {EXAMPLE_DIALOGUE_COVERAGE}` after the closing `}` — and converting to `format!` means escaping the JSON braces `{{`/`}}`. Note the existing `\n` inside the JSON is LITERAL backslash-n in the raw string (v4's `\\n`) and must stay so.
  - `system_prompts_prompt()` (:194-208): append `\n\n{CONVERSATIONAL_VOICE_DIRECTION}` after `...tailored to them.` (used at :1673).
- `generators/optimizer.rs` — HAND-written format! builders. Import at :45-47 gains the two.
  - `get_general_fields_suggestions_prompt` (:451): insert the two bullets between :469 (`- Do NOT propose the same content under two different fields...`) and :470 (`- Do NOT suggest edits to title...`), the second ending `{EXAMPLE_DIALOGUE_COVERAGE}`.
  - `get_system_prompt_suggestion_prompt` (:516): insert the two (shorter-wording) bullets after :539 (`- Do NOT change the prompt's evident interaction style...`), before the blank line + `Respond with a JSON array of at most one suggestion.`
  - new-system-prompts builder (`get_new_system_prompts_suggestion_prompt`, the pass ending :715-717): insert `- {CONVERSATIONAL_VOICE_DIRECTION}` after :715 (`- Be conservative: ...`).
  - The constants contain no `{`/`}`, and they are passed as captured args, so no escaping issues.
- `generators/wizard.rs`, `generated_items.rs`, `mod.rs` — no change needed (wizard.rs reads `field_prompt()`; generated_items holds the wardrobe prompt, untouched).

## C. Families and fixtures that move

All exist under `crates/quilltap-harness/tests/`:

| family | recipe (oracle / env) | moves at c3eefa752? | fixture |
|---|---|---|---|
| `builtin_prompt_templates_guard` | `$N/npx tsx $V5W/harness/oracle/provision/dump-prompt-templates.ts` (re-vendor), then `cargo test --test builtin_prompt_templates_guard`; reads live `QT_V4_CHECKOUT` | RED now (live checkout) | none |
| `prompt_templates_routes_equivalence` | builder `build-prompt-templates-routes-fixture.ts` → `QT_FIXTURE_PT_ROUTES_MAIN=/tmp/qt-pt-routes-fixture.db`; jest `prompt-templates-routes.test.ts` → `QT_ORACLE_PT_ROUTES`, `QT_FIXTURE_PT_ROUTES` | RED: (a) the log line text + field set (`is_ported_line` at :127 matches only `Sample prompt template seeded from plugin`, and the oracle filter at `prompt-templates-routes.test.ts:305` identically — BOTH filters must widen to the two new messages); (b) **the load-bearing case `list_stale_builtin_never_updated`** (plant: content `OLD CONTENT — v4 never overwrites…`, description `A stale plant`) now REFRESHES in v4 — the case must be renamed/re-expected and stays in the `for want in [...]` must-drive list (:645); the GET/PUT/DELETE cases seeding `staleBuiltIn` (oracle :188/:212/:250) now also refresh before the read | **real-DB, built FRESH by v4's `initializeDatabase()`** per run → see note |
| `prompt_templates_tier2_equivalence` | `build-prompt-templates-fixture.ts` + `cases/prompt-templates-tier2.ts` (`QT_ORACLE_PROMPT_TEMPLATES`, `QT_FIXTURE_PROMPT_TEMPLATES`) | probably neutral (drives repo ops; check whether any op path goes through findAll/findById → seed; the fixture seeds one isBuiltIn row directly — if the tier-2 case reads through a seeding repo method, that row's name would have to match a catalogue name to refresh) — run as a neutrality check | real-DB built fresh |
| `ai_import_assembly_equivalence` | `npx tsx cases/ai-import-assembly.ts` → `QT_ORACLE_AI_IMPORT_ASSEMBLY` | NEUTRAL expected (covers only `CHARACTER_BASICS_PROMPT` = FULL_FIELD_SEMANTICS, unchanged) | JSON corpus |
| `ai_import_tier3_equivalence` | jest `ai-import-tier3.test.ts` over committed `crates/quilltap-web/tests/fixtures/character-generators-{main,mount}.db` → `QT_ORACLE_AI_IMPORT` | RED where corpus runs `first_message` / `system_prompts` steps (it diffs "every recorded model call … the step prompts as bytes") | committed pair |
| `character_optimizer_prompts_equivalence` | `npx tsx cases/character-optimizer-prompts.ts` → `QT_ORACLE_CHARACTER_OPTIMIZER_PROMPTS` | RED (tier-1 over `get_general_fields…`, `get_system_prompt…`, `get_new_system_prompts…`) | JSON corpus |
| `character_optimizer_tier3_equivalence` | jest `character-optimizer-tier3.test.ts`, committed CG pair → `QT_ORACLE_CHARACTER_OPTIMIZER` | RED (calls' messages compared) | committed pair |
| `character_wizard_tier3_equivalence` | jest `character-wizard-tier3.test.ts`, committed CG pair → `QT_ORACLE_CHARACTER_WIZARD` | RED where the corpus generates exampleDialogues / systemPrompt | committed pair |
| `generators_leaf_equivalence` | `npx tsx cases/generators-leaf.ts` → `QT_ORACLE_GENERATORS_LEAF` | ⚠ **stays GREEN unported** — its oracle case holds a HAND list of six exports (:131-138) and the coverage assert is against that list; the new exports are invisible until the list grows. Add both names to the case, then it goes red until `field_semantics.rs` is regenerated. | JSON |
| `generators_wizard_prompts_equivalence` | `npx tsx cases/generators-wizard-prompts.ts` → `QT_ORACLE_GENERATORS_WIZARD_PROMPTS` | RED (exampleDialogues + systemPrompt field prompts) | JSON fixture `generators-wizard-prompts.json` |
| `help_tree_equivalence` (+ the embed guard) | — | RED on 3 pages until re-vendored | — |

Fixtures carrying seeded built-in prompt rows (grep `isBuiltIn`): `build-prompt-templates-fixture.ts` (one direct isBuiltIn row), `build-prompt-templates-routes-fixture.ts` (none baked — clean instance; the stale plant is per-case), plus builders that set isBuiltIn for other entities (almanack, groups-projects, in-scene-voiced, roleplay-templates, settings, system-data). None of the character-generators pair is built with seeded prompt rows (not checked exhaustively; the tier-3 families never read prompt_templates). The almanack fixture "is built through v4's own path so the seeded rows exist" (phase3_ledgers.rs note) — if rebuilt at ≥ c3eefa752 its seeded content changes but the almanack only COUNTS, so neutral.

**The `f7f3d7bf0` renderedMarkdown drop:**
- The three tier-3 families use the COMMITTED `character-generators-{main,mount}.db` pair — nothing is rebuilt, v5 reads the committed pair unchanged; only the ORACLE runs at `c3eefa752` (it copies the pair and runs v4's `initializeDatabase()` — i.e. v4's migrations, incl. the drop, on its own copy). Since these families compare frames/calls/logs, not a chats DB dump, the drop should not show; but ai-import's `done` frame carries an assembled QuilltapExport — confirm the export shape did not move in `f7f3d7bf0` (a neutrality check for that other lane, not this one).
- `prompt_templates_routes` and `prompt_templates_tier2` BUILD their real-DB fixture fresh through v4's migrations. Build them in a worktree **pinned at the baseline `acadcc7cd`** (so v5 main can open them — they have no seeded prompt content, so nothing of this commit is lost), and run the ORACLE (jest / tsx) in a worktree **pinned at `c3eefa752`** (so the plugin catalogue + the refresh arm are the new ones). The oracle's `initializeDatabase()` migrates its own copy forward; v5 opens the baseline-vintage copy. (Whether v5's `Db::open` would actually choke on a chats table lacking renderedMarkdown for a prompt-templates-only family is unmeasured — the baseline build sidesteps the question.)
- JSON-corpus families (assembly, optimizer_prompts, generators_leaf, generators_wizard_prompts) have no DB — just run their oracle at `c3eefa752`.

## D. Traps and premises

1. **"Refresh writes at boot" is FALSE in v4 and in v5.** It is a lazy write on the first prompt-template read after upgrade (list / get / put / delete in v5; also almanack/export/system-tools in v4). v5 already writes at v4's site through the writer task, so the port needs NO boot step. Precedents for boot writers exist (the roleplay `builtin_templates` seeder every boot with in-place UPDATE — the closest analogue in shape; the help reconcile P4.D222; the refusal-ledger ensure) but none is the right home — keep the lazy site.
2. **`needs_seeding` is names-only (confirmed)** → once seeded, the probe answers `false` and the writer is never taken, so a refresh can never fire. The probe must widen to "any catalogue entry absent OR its built-in row differs in content / description / category / modelHint (NULL-as-different)". Keep it on the read pool; the writer-side pass re-derives per entry (insert / refresh / no-op), so the decision is never acted on stale.
3. **Row identity:** v4 `findOne({name, isBuiltIn: true})` → the SQLite translator's `LIMIT 1` with no ORDER BY (rowid order). With duplicate built-in rows under one name (dogfood-possible), only the FIRST is compared and refreshed. v5's lookup must return that same row's id + four fields (`... WHERE name = ?1 AND isBuiltIn = 1 LIMIT 1`, same rowid order), not an EXISTS.
4. **The comparison is on hydrated values**: NULL description/category/modelHint → `undefined` in v4 → always "differs". Compare `Option<String>` against `Some(expected)`.
5. **The update is RAW**, not through the repo's `update()` (which refuses built-ins) — add a `seed_update`-style raw `UPDATE prompt_templates SET content, description, category, modelHint, updatedAt WHERE id = ?` (the roleplay precedent). `updatedAt = now` where `now` is taken ONCE per entry BEFORE the lookup (v5 currently takes `now` only on insert — harmless for insert, but the refresh must use a per-entry `now`).
6. **Log lines, BOTH sources:** insert → `Sample prompt template seeded` {templateId, name, source, modelHint, category} (source = `plugin` always in v5 — the filesystem arm stays NO-COUNTERPART; `promptId` dropped from the log, so `prompt_id` in the catalogue becomes unused at runtime except by the guard + its test — keep the JSON field, the dump still emits it); refresh → `Built-in prompt template refreshed from shipped text` {templateId, name, source}. Both harness `is_ported_line` and the oracle's filter must widen. Field ORDER: v5's capture may sort fields (the sorted-field-capture trap) — check how this family renders `context`.
7. **User edits clobbered, copies untouched:** built-ins are un-editable through routes in both implementations, so only a direct-DB edit is clobbered — faithful. A user template with a catalogue name (`isBuiltIn: false`) is never matched (`list_user_named_like_builtin_still_seeds` stays). Characters' `systemPrompts` copies are never read or written by the seeder (the routes family could add a readback pin that a character copy is byte-unchanged, but v5's seeder has no path to characters, so a unit assert suffices).
8. **Plugin version:** v5 records the prompts plugin's version nowhere (grep for `1.1.23`/manifest finds nothing relevant; the `1.1.24` mention in `model/request_builder/chat_completions.rs:75-77` is a DIFFERENT plugin's why-comment). Stamp-only; nothing to port. `4.10.0-dev.98` may matter to any family normalizing `V4_APP_VERSION` (ai_import_tier3 has one) — check its constant.
9. **Prompt body comparison** is byte-exact against the checkout's raw `.md` (no trim) by the guard, and byte-exact through v4's real registry by the dump. Regenerate, never hand-edit; the files are not append-only.
10. **Almanack divergence note** (phase3_ledgers.rs:14-21) should gain a clause: v4's report can now also REFRESH built-in rows (updatedAt/content), still count-neutral.
11. **The routes family's `staleBuiltIn` plant** differs in BOTH content and description, so it cannot distinguish which field triggered the refresh — add per-field plants (content-only, description-only, category/modelHint, NULL description, exact-match no-op with a pinned `updatedAt`) as new cases; the v4 unit test's three shapes are the minimum.

## E. Size and tier
Medium, one lane. Code: seeder refresh arm + widened probe + raw update + two log lines (~120 lines core + unit tests); ai_import 2 edits (one const→fn), optimizer 3 edits; two generated modules regenerated (+ two oracle-case hand lists + one DOC map); vendored JSON regenerated; 3 help pages byte-copied. Harness: routes family re-cased (rename the never-updated case, add ~5 refresh arms, widen both log filters) and nine oracles regenerated at the pin with the two DB fixtures built at the baseline. Roughly 400-700 changed lines, most mechanical. Recommend **one Opus-tier lane** (the seeder semantics — row identity, NULL-as-different, the read-pool probe vs writer re-check, the log field order, and the two-pin fixture recipe — are where a cheaper agent would quietly go wrong); the regen/re-vendor steps inside it are cheap-agent material if split out.

## Summary (10 lines)
1. v4's refresh is LAZY (inside findById/findAll/findAllForUser), NOT a boot step — the order's "startup write" premise is false; v5 already seeds at that lazy site, through the writer, so no boot work is needed.
2. v5's `needs_seeding` is confirmed names-only on the read pool, so it must widen to the four-field diff or the refresh never fires.
3. Refresh compares content, description, category, modelHint (`===`, NULL counts as different) and runs a RAW `$set` of those four + updatedAt; it clobbers direct-DB edits of built-ins; user same-name templates and character copies are never touched.
4. Both INFO seed lines are renamed to `Sample prompt template seeded` with `source` in place of `promptId`; the new refresh line carries {templateId, name, source}.
5. The two constants are quoted verbatim in §A2; they go into 7 sites (2 ai-import, 2 wizard, 3 optimizer), and the optimizer adds two different "codify repetition" wordings.
6. Surprise: none of the 21 prompt .md files is append-only. Guidance is inserted mid-file and the examples come BEFORE the Memory section; all bodies must be regenerated by the dump.
7. Surprise: `builtin_prompt_templates_guard` reads the LIVE v4 checkout and is already RED on v5 main.
8. Trap: `generators_leaf` stays GREEN when unported, because its oracle case holds a hand list of six exports; the same applies to generators-field-semantics.ts. Both lists and the generator's DOC map must grow.
9. The load-bearing routes case `list_stale_builtin_never_updated` flips and must be renamed and re-expected. Build the routes/tier2 fixtures at `acadcc7cd` and run the oracle at `c3eefa752`; the tier-3 families use the committed CG pair, so only their oracle moves.
10. Size is medium, one Opus lane. The ledger's path is wrong: the file is `lib/database/repositories/`, not `lib/data/`.
