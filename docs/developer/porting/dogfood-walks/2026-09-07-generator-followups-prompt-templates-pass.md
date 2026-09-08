# Dogfood walk — the generator follow-ups + prompt-templates round (P4.82 ∥ P4.83 ∥ P4.84 ∥ P4.85 ∥ P4.86)

**Date:** 2026-09-07
**Instance:** a COPY of Friday at `~/qt-dogfood-friday` (never the live iCloud tree).
**Server:** `RUST_BACKTRACE=1 ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, log in the scratchpad.
**Build:** `main` @ `0c3d6415` (the round's unification). core 0.0.846, harness 0.0.735, web 0.0.131, host 0.0.114, SPA 0.5.681.
**Round record:** `status-log.md` → "Round record — the generator follow-ups + prompt-templates round unification".

## §0 The drift ledger

The §2 freshness probe **PASSED** at walk start: checkout on `main`, tree clean,
`2f4254b42..main` and `1a2b2164c..bugfix` both EMPTY. v4 HEAD **IS** the oracle
baseline; the ledger's §3 is **EMPTY**. **No step in this walk may blame drift** —
there is none, and no surface here carries a pending drift row.

## §0.5 Pre-walk measurement (ledger §5.5) — what the real data actually holds

Run BEFORE the plan was written, because v4 runs daily on the live instance and
heals data out from under banked proofs. Three of this round's four 💸 items were
re-aimed by what came back.

1. **`instance_settings['headshoulders_backfill_enqueued_v1'] = 'true'`** — v4 has
   already run its own scan here. The cross-app leg (A1) is therefore live and
   free; the positive leg (A2/A3) needs the flag cleared deliberately.
2. **Backfill eligibility is ~ZERO on real data.** 48 `physical-prompts.json`
   vault files: **42** already carry a non-blank `headAndShoulders`, 6 are blank,
   and of those 6 exactly one has a seed — and that one's mount point belongs to
   **no character**. The three blank mount points that DO map to characters
   (Mochi, Devin, and the archived Tuman) have *all five tiers null*, so the scan
   skips them for want of a seed. **A scan today enqueues 0.** A3 therefore
   manufactures its one eligible character by clearing a real one's
   head-and-shoulders through the UI.
3. **v4 left a real DEAD `CHARACTER_HEADSHOULDERS_BACKFILL` job on this instance**
   — `baef7a26-fbd6-4555-9287-db42b0fe18c9`, character **Charlie**
   (`57ecc095-…`), 3/3 attempts, `lastError: "No response from model"`,
   2026-06-13. v4 could not finish it. It is A3/A4's material.
4. **`prompt_templates` holds 27 built-ins in THREE v4 seeding vintages** —
   2025-12-16 (the hyphenated `GPT-4O` / `GPT-5` / `MISTRAL_LARGE` names),
   2026-03-26 (the unhyphenated `GPT4O` / `GPT5` / `MISTRAL` names), and
   2026-08-19 (the `MODERN` trio). **All 21 of v5's vendored catalogue names are
   already present**, so v5's lazy seed must insert NOTHING. And the content
   DIVERGES: v4's `GEMINI Companion` row is **1986** chars where v5's vendored
   catalogue holds **2906** — so the "never an update" rule is measurable by
   consequence, not by assertion. `MODERN General` is **byte-identical** on both
   sides (3547 chars) — a free cross-implementation agreement.
5. Characters: **45**, all with vaults, 10 archived. Image profiles: **15**, every
   one with an API key (so D3 builds a key-less throwaway rather than spending).

## §1 What is newly LIVE this round

| Lane | Newly live surface |
|---|---|
| P4.82 | `CHARACTER_HEADSHOULDERS_BACKFILL` — the last named-refusal job type. Handler + dedupe-on-enqueue + the one-time boot scan, wired in `quilltap-host`. |
| P4.83 | The whole prompt-templates vertical: the vendored 21-entry catalogue + lazy seeding v5 NEVER had, five verbs, v4's REST edges, the Import-from-Template modal on BOTH character hosts. |
| P4.84 | The wizard review pane's three missing renders + the `fullDescription` CommonMark preview; the two continue-mode toasts; `runTemplateSave` on the shared apply helper; the "Summon From Lore" fixed title. |
| P4.85 | `[Chats v1] Impersonation stopped`; the `generators_detail` typed refusal; the two route-level `[Characters v1] … starting` lines; `?action=generate`'s code-point length gate. |
| P4.86 | AI-import steps 9 + 10 on a REAL JSON-Schema engine (`jsonschema` 0.55) — `VALIDATION_UNAVAILABLE` DELETED; the `sourceFileIds` / `regenerateSteps` truthy-non-array arms. |

## §2 What NOT to expect to work (recorded refusals — do not report as bugs)

- **`find_built_in` has no production caller yet** — the `.qtap` exporter is its
  named entry. Nothing in the UI reaches it.
- **The filesystem `prompts/` fallback** (`sample-prompts-loader.ts`) is a
  NO-COUNTERPART; it is `@deprecated` in v4 and its directory does not exist.
- **The registry-EMPTY arm** — v5 always holds the vendored catalogue, so v5
  seeds where a v4 install with the plugin missing would not. A recorded
  divergence in v5's favour.
- **A malformed JSON body on the prompt-templates edges** folds to `{}` → Zod
  **400** where v4's `req.json()` throws **500**. A pre-existing class
  (`subprompts_routes.rs` precedent), recorded, not this round's.
- **The `isPaused` continue guard** is measured, pinned and deliberately NOT
  ported.
- **ajv-formats' two lenient shapes** — a `date-time` with a space separator and a
  `urn:uuid:`-prefixed `uuid` — are accepted by v4 and refused by the crate. v4's
  exporter emits neither, so no real import can hit them.
- **The `qtap:` hrefs in the wizard preview** stay on a plain anchor (ratified).

## §3 The walk

### Part A — P4.82: the head-and-shoulders backfill

| # | Owner | Test | Gesture | Expected / how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | **The cross-app leg** — v4's flag is honoured | Boot v5 on the copy untouched | **PASS.** Zero `Head-and-shoulders backfill` lines in the boot log AND zero in `combined.log`; the flag row still `true`, untouched. The negative is not vacuous — A2 fires the identical grep positive on the very next boot. | **PASS** |
| A2 | CLAUDE | **The scan's positive leg** | Delete the flag row; restart | **PASS — and it corrected my own §0.5 measurement.** `Head-and-shoulders backfill scanning total=45` then `… enqueue complete scanned=45 enqueued=2 skipped=43`. I predicted **0**; the truth is **2**, because the seed test also reads `fullDescription`, which lives in `physical-description.md`, NOT in the `physical-prompts.json` I had measured. The flag was re-written to `true`. | **PASS** |
| A3a | CLAUDE | **Two REAL jobs, unprompted** — A2's own enqueues ran | (none — the pump picked them up) | **PASS.** **Devin** (`e63cc514`): `[HeadShouldersBackfill] Populated head-and-shoulders prompt … length=191`, job COMPLETED, the vault file now carries a 191-char `headAndShoulders` that was `null` an hour ago. **Tuman** (`3f278ac7`, archived): the model was called, then the archive write guard refused with `Character 3f278ac7-… is archived: this character is archived; rehydrate it to continue` — **byte-identical to v4's `characters.repository.ts:22-25`**, and v4 would enqueue it too (`findAll()` is an unfiltered `find({})`; the handler has no archived check), so this is faithful, not a v5 over-reach. Both wrote a `CHARACTER_WIZARD` row to `llm_logs` on the cheap LLM (NANOGPT `deepseek-v4-flash-latest`, 2,096 / 2,875 ms) — the handler's promise that its completion IS the wizard's completion. | **PASS** |
| A3b | CLAUDE | **The whole-merged-object rule**, on v4's own unfinished work | Cleared Charlie's `headAndShouldersPrompt` on the Appearance tab (the real UI write path); reset v4's dead job `baef7a26-…` to PENDING | **PASS — the walk's best proof.** v4 gave up on this exact job on 2026-06-13 after 3/3 attempts (`No response from model`). v5's newly-landed handler finished it: `Populated head-and-shoulders prompt … character_id=57ecc095-… length=212`, COMPLETED in 3.5 s. The tiers survived **md5-identical** — `short` `90f96a1ea3`, `medium` `2870b70644`, `long` `d68d5cb3c0`, all three the values recorded before the run. A partial write would have nulled every one. | **PASS** |
| A4 | CLAUDE | **The already-filled skip** | Reset `baef7a26-…` a second time, now that Charlie IS filled | **PASS.** Dispatched → completed in **4 ms** (against A3b's 3,500 ms — the timing is itself the discriminator); **zero** `[HeadShouldersBackfill]` sentences; `llm_logs` unchanged at **7,181** across the whole boot. The idempotence gate returns before the model. | **PASS** |

### Part B — P4.83: prompt templates against v4's seeded rows

| # | Owner | Test | Gesture | Expected / how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | The catalogue on a shared instance | Charlie → **System Prompts** → Import Template | **PASS.** All **27** of v4's built-ins render under "Sample Prompts" — all three vintages side by side (`GPT-4O`/`GPT-5`/`MISTRAL_LARGE` *and* `GPT4O`/`GPT5`/`MISTRAL`), no duplicates, and a separate "My Templates" group. The count in the DB **stayed 27**; zero `Sample prompt template seeded from plugin` lines. Before this round the modal read v4's own "No templates available". | **PASS** |
| B2 | CLAUDE | **The never-update rule, by consequence** | Same list; re-read the DB | **PASS.** `GEMINI Companion` still **1986** chars at `updatedAt 2026-03-26T03:08:59.530Z`, where v5's vendored entry is **2906**; `OLLAMA Romantic` still 2731 against a vendored 2463. v5 listed 27 rows whose text it disagrees with and changed none of them. | **PASS** |
| B2b | CLAUDE | **Proving that negative is not vacuous** — the seeder's insert path | `DELETE FROM prompt_templates WHERE name='MISTRAL Companion'` (v4's Mar-2026 row); restart; re-list | **PASS — the pair is the proof.** Exactly **one** log line: `Sample prompt template seeded from plugin … name=MISTRAL Companion prompt_id=default-system-prompts/MISTRAL_COMPANION model_hint=MISTRAL category=COMPANION` (the registry id the lane preserved so the sentence is byte-identical to v4's), and the re-inserted row carries **v5's vendored 2,372 chars**, not v4's 1,320. So the same code path that wrote 2,372 chars into a fresh row left its 1,986-char sibling untouched one call earlier. | **PASS** |
| B2c | CLAUDE | The built-in delete guard (free) | `DELETE /api/v1/prompt-templates/{a built-in id}` | **PASS.** `403 {"error":"Cannot delete built-in templates"}` — byte-identical to v4's `forbidden('Cannot delete built-in templates')` (`app/api/v1/prompt-templates/[id]/route.ts:129`). | **PASS** |
| B3 | CLAUDE | **A three-way byte agreement** | Import `MODERN General` on the edit host | **PASS.** v4's DB row (3,547), v5's vendored catalogue entry (3,547, md5 `6ca2b46521e4`) and the body persisted into Charlie's vault as `MODERN General.md` are **all three identical** — the vault file is that body under a 46-byte frontmatter header. The edit host's semantic is v4's: the click opens a pre-filled **Create Prompt** dialog. | **PASS** |
| B4 | CLAUDE | The NEW-character host's *different* semantic | New Character → Import Template → `CLAUDE Companion` | **PASS.** Same catalogue (Sample Prompts + My Templates), but the click writes **straight into the System Prompt field** rather than opening a dialog — v4's second open semantic, reproduced. | **PASS** |
| B5 | CLAUDE | The wire's measured asymmetry | `GET /api/v1/prompt-templates`, then a `POST` | **PASS.** The READ row is `{id,name,content,description,isBuiltIn,category,modelHint,tags,createdAt,updatedAt}` — **`userId` omitted** (NULL on a built-in). The CREATE answers **201** with `userId` present and `description: null`, `category: null`, `modelHint: null` **explicitly sent**. Exactly the lane's measured SQLite-hydration asymmetry. | **PASS** |

### Part C — P4.84: the SPA generator smalls on a real generation

| # | Owner | Test | Gesture | Expected / how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | **The review pane's three renders** | Ran the AI Wizard on a new character with **all twelve fields** selected (~18 real cheap-LLM calls), then expanded Personality / Scenarios / Physical Description | **PASS — all three, with v4's exact markup.** (1) Six **"Written as: …"** example lines, one per hinted field (identity, description, manifesto, personality, scenario, systemPrompt), rendered above the generated text. (2) The tier panel: `<strong class="text-foreground">Short (138 chars):</strong>` + `<p class="qt-text-secondary">` for Short/Medium/Long, then `Full Description:` rendered through the NEW CommonMark pipeline as real HTML (`<h2>Overview</h2><p>…`) inside `.prose` — where v5 used to show raw text. (3) Scenarios as `<strong>The Last Lamp at the Weeping Lock</strong>` + `<p class="qt-text-secondary mt-0.5 whitespace-pre-wrap">`, three of them — the class list v4's `:151-163` specifies, character for character. | **PASS** |
| C2 | CLAUDE | The "Summon From Lore" fixed title | Characters → Summon From Lore | **PASS.** The dialog header reads the fixed **"Summon From Lore"** (the lane's refutation holds: both wrappers carry it, so the fix sits at the dialog with no `[title]` input). | **PASS** |
| C3 | CLAUDE | The continue-mode toasts | Flipped every seat in a real 3-seat chat to user-controlled, then clicked the composer's Continue; then tried to engineer a stale roster | **BLOCKED by design — and the block is the lane's own recorded finding, now confirmed empirically.** The guard's predicate is v4's WIDE `hasActiveCharacters` (any active CHARACTER, whoever drives it), so making every seat user-controlled does not trip it. And **both continue buttons are disabled by the same predicate the toasts guard** — measured: once the roster no longer qualified, `.qt-composer-gutter-continue` reported `disabled: true`. So the two sentences are reachable only through the P4.D90 refresh race (another tab removing a seat between render and click), exactly as the ported code's own comment says. Driven by the unit specs; not producible from a single browser without faking a roster. | **BLOCKED(reachable only via a stale roster — as the port records)** |
| C4 | CLAUDE | `runTemplateSave` on the shared helper | Charlie's detail page → **Charlie → {{char}} (3)** | **PASS, with a counting discriminator.** The toast read v4's `Replaced character name with {{char}}`, and the buttons re-rendered from a genuine refetch: `Charlie → {{char}} (3)` vanished (0 left) while `{{char}} → Charlie` went **16 → 19** — precisely the three occurrences converted, across main fields and system prompts. That is the shared `applyCharacterFieldUpdates` fan-out plus v4's always-refetch, on real data. | **PASS** |

### Part D — P4.85: the Rust smalls

| # | Owner | Test | Gesture | Expected / how verified | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | `[Chats v1] Impersonation stopped` | Flipped Vergil's provider select to **User (you type)** in a real 3-seat chat, then clicked **Stop speaking as Vergil** | **PASS.** `[Chats v1] Impersonation stopped chat_id=a3dba0f4-… participant_id=8b23445c-… character_name=Vergil`, in the console **and** as a JSON line in `combined.log`. v4's site (`app/api/v1/chats/[id]/actions/participants.ts:126`) logs the identical sentence with the identical three-field bag `{chatId, participantId, characterName}`. | **PASS** |
| D2 | CLAUDE | The route-level starting lines | Real AI-import and AI-Wizard runs | **PASS (partial — optimizer/external-prompt at D2b).** `[System Tools v1] AI Import stream starting` with `{userId, profileId, sourceFileCount, hasSourceText, includeMemories, includeChats}` and `[Characters v1] AI Wizard starting (streaming)` with `{userId, characterName, fieldsToGenerate, sourceType}` — both with v4's field bags. ⭐ A free confirmation rode along: the E2 run that passed `sourceFileIds: "not-an-array"` logged **`sourceFileCount: 12`** — the JS `.length` of that 12-character string, which is precisely what v4 would log for a truthy non-array. The faithful arm proved itself in a log field nobody aimed at. | **PASS** |
| D2b | CLAUDE | The other two starting lines | A real external-prompt run and a real optimizer run on Charlie | **PASS.** `[Characters v1] External prompt generation starting user_id=… character_id=… connection_profile_id=… max_tokens=4000`, then `External prompt generated successfully … duration_ms=197060 tokens_used=4168 output_length=9308` and an **`EXTERNAL_PROMPT`** row in `llm_logs` — the exact `type` string P4.85 item 6 measured on v4 after un-mocking `llm-logging.service`. And `[Characters v1] Character optimizer starting (streaming)` with all eight of v4's fields (`max_memories=30 search_query=(none) use_semantic_search=true since_date=null before_date=null output_mode=apply`), followed by **`CHARACTER_OPTIMIZER`** rows. Both `type` strings match the lane's measurements. | **PASS** |
| D3 | CLAUDE | **`?action=generate`'s code-point gate** | `count:20`; empty prompt; bogus id + bad count; then 4000 vs 4001 astral code points against a purpose-built key-less profile | **PASS, zero spend.** `count:20` → 400 `Validation error` (the §3 review's fix — before it, `20` fell through to the tool's own schema); empty prompt → the same; a bogus id **with** a bad count → **404 `Image profile not found`**, so the 404 still beats the 400. The discriminator: **4000 astral code points = 8000 UTF-16 units** passed the gate and died downstream at `No API key configured`, while **4001** answered `Validation error`. The boundary sits at 4000 **code points**; the retired `utf16_len` would have refused the 4000 case. | **PASS** |

### Part E — P4.86: the AI-import validation + repair loop

| # | Owner | Test | Gesture | Expected / how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | **Step 9 on a real import** | "Summon From Lore" through the UI (Thessaly Bramwell-Okonjo, completed), then a captured stream (Ozias Vane) | **PASS.** The real `jsonschema` 0.55 engine validates a real LLM-assembled export against the vendored 89,769-byte schema in production: `{"type":"step_start","step":"validation"}` → `{"type":"step_complete","step":"validation","snippet":"Validation passed"}`. **`VALIDATION_UNAVAILABLE` appears nowhere** — in the stream, the log, or the review pane. The UI run completed to the Review pane with a named character. | **PASS** |
| E1b | CLAUDE | **Step 10 (repair) — honestly UNEXERCISED** | Three attempts to force an invalid export via `existingResult`: `pronouns.subject: 42`, `title: 1234`, `physical_descriptions: "not an object"` | **BLOCKED, not a defect.** The schema's `ExportedCharacter` is `additionalProperties: true` and does not declare `pronouns`, so a junk pronoun is legitimately valid; and the assembler **normalizes before validation** — a non-string `title` is dropped to `null`, a non-object `physical_descriptions` becomes the default object. So no externally-injectable shape reaches step 9 invalid. The repair loop stays covered where it can be: `Repair successful` ×9 in `ai_import_tier3`'s corpus against v4's REAL `validateQtapExport`. Recorded rather than claimed. | **BLOCKED(assembler normalizes; no injectable invalid shape)** |
| E2 | CLAUDE | The truthy-non-array arms + the body-parse arms | Five curls at `POST /api/v1/system/tools?action=ai-import-stream` | **PASS.** `regenerateSteps: 1` → a `done` frame carrying V8's own **`request.regenerateSteps?.includes is not a function`** in both `error` and `errors._fatal` — v4's TypeError, reproduced. `sourceFileIds: "not-an-array"` **proceeds** (which is the fix: v5 measurably used to 400 where v4 runs on). No `profileId` → 400 `Missing required field: profileId`. A non-JSON body → 500 **`Unexpected token 'o', "not json at all" is not valid JSON`** — V8's exact wording through `v8_json_parse_message`. A `null` body → 500 `Cannot read properties of null (reading 'profileId')`. | **PASS** |
| E3 | CLAUDE | The live SSE edge | `curl -sS -N -D` on the same route | **PASS.** `content-type: text/event-stream`, `cache-control: no-cache`, `connection: keep-alive` — all three of v4's headers. 18 frames over 30,185 bytes, **every one** `data: {json}` terminated by a blank line, checked mechanically. | **PASS** |

### Part F — deferred to the human (cost / judgment)

| # | Owner | Test | Why deferred | Status |
|---|---|---|---|---|
| F1 | HUMAN | The Brahma Console budget on a genuinely deep query | Long agent loop, real spend | DEFERRED-TO-HUMAN |
| F2 | HUMAN | Memory deduplication + conversation-summary regeneration first run | Batch job over 800 MB of real memories | DEFERRED-TO-HUMAN |
| F3 | HUMAN | Finding #101 — NanoGPT prompt caching writes but never reads | A cost question about the gateway's own side of the wire | DEFERRED-TO-HUMAN |

## §4 Findings

**ZERO v5 defects.** 20 rows: **16 PASS**, 2 BLOCKED-by-design, 3 deferred to the
human. Five server boots on the real 800 MB instance, ~2 hours, **zero panics and
zero `ERROR`-level lines** in any boot log. Every failed expectation this pass
produced was my own instrument or premise, never the app's.

### F1 — My §0.5 measurement was wrong, and the app corrected it

I predicted the backfill scan would enqueue **0** on real data, having counted
blank `headAndShoulders` values in the 48 `physical-prompts.json` vault files. The
scan enqueued **2**. The seed test is
`Boolean(mediumPrompt || shortPrompt || longPrompt || completePrompt ||
fullDescription)` and **`fullDescription` does not live in that JSON file** — it
comes from `physical-description.md` through the overlay. Two characters have the
one seed I could not see. Recorded because the same blind spot would bite the next
walk that sizes this feature from the JSON alone.

### F2 — Two arms are unreachable from a browser, by the port's own design

Neither is a defect; both are recorded here so a later pass does not re-chase them.

- **The AI-import repair loop (step 10).** No externally injectable shape reaches
  step 9 invalid: `ExportedCharacter` is `additionalProperties: true`, and the
  assembler normalizes before validation (a non-string `title` → `null`, a
  non-object `physical_descriptions` → the default object). Three deliberate
  corruptions through `existingResult` all validated clean. Covered where it can
  be: `Repair successful` ×9 in `ai_import_tier3` against v4's real
  `validateQtapExport`.
- **The two continue-mode toasts.** Measured, not assumed: the buttons that would
  fire them are disabled by the *same* predicate the toasts guard
  (`.qt-composer-gutter-continue` reported `disabled: true` the moment the roster
  stopped qualifying). They are defensive guards for the P4.D90 stale-roster race,
  exactly as the ported code's comment says.

### F3 — Observations that looked like divergences and are not

- **Selecting `User (you type)` clears the seat's connection profile and sets
  `controlledBy: 'user'`.** Checked against v4 before recording anything: v4's
  `ParticipantCard.tsx:207-208` calls `onConnectionProfileChange(id, null, 'user')`
  on that exact value. Faithful. (Impersonation proper still never writes
  `controlledBy` — `impersonatingParticipantIds` stayed `[]` throughout.)
- **`Cannot delete built-in templates`** on a built-in DELETE is v4's own
  `forbidden(...)` sentence, byte for byte.

### Test material left on the copy (disposable; the next rsync clears it)

Devin's and Charlie's `headAndShoulders` prompts are now populated (that is the
feature working); `MISTRAL Companion` carries v5's vendored 2,372-char text rather
than v4's 1,320 (the B2b probe); Charlie's fields carry `{{char}}` where his name
was (C4); the three seats of `a3dba0f4` are user-controlled; one imported
`MODERN General.md` sits in Charlie's vault. The throwaway prompt template and
image profile were **deleted** (27 templates and 15 image profiles, both back to
their starting counts).
