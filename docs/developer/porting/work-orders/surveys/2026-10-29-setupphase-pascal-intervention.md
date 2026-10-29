# Survey — Pascal operator intervention: P4.D345 (SERVER) ∥ P4.D346 (SPA)

Read-only survey for the 2026-10-29 `/setupphase` (the `236c04e35` catch-up round).

## 0. Header

- **Date:** 2026-10-29. **Pin:** `236c04e35` (`4.12.0-dev.0`, code-identical to `7789ebed6` = v4 4.11.0; read from
  the detached worktree `/tmp/qt-v4-pin-setupphase-7789ebed6`). **v5 main:** `f6cdceedd` (core 0.0.1379, host
  0.0.217, web 0.0.251, SPA 0.5.907). **Oracle baseline:** `322a5488e`.
- **Commits in scope:**
  - `839575d89` 2026-10-22 — *Feature: operator intervention in Pascal custom tools* (80 files, +5,541 / −84; the
    TWENTIETH D23 move `chat_settings."customToolIntervention" INTEGER DEFAULT 1` is the KEYSTONE P4.D341's — this
    survey maps its READ sites).
  - `8ae8a127c` 2026-10-23 — *Feature: min/max ranges in the Pascal intervention dialog* (14 files, +259 / −12).
  - `b02d902f3` 2026-10-24 — *Fix: Pascal hold dialog can re-ask a hidden consult* (11 files, +119 / −21).
  - `332c650b9` 2026-10-22 — *Docs: spec for operator intervention* — **NO-PORT, docs only** (2 files +498; the
    spec moved to `docs/developer/features/complete/pascal-operator-intervention.md` by `839575d89`). The vendor
    lane P4.D360 ratifies it; nothing for P4.D345/D346.
- **Lanes informed:** P4.D345 (server), P4.D346 (SPA); contracts with P4.D341 (keystone), P4.D343/D344
  (Storyteller — shared `realtime` topic lists and `workbench` library entry), P4.D348 (the turn spine — the Stop
  veto hook), P4.D356 (the chat-settings PUT arms + the unknown-key 400), P4.D357 (CSS — `_intervention.css` rides
  `cce44e098`'s legibility pass), P4.D360 (help / docs vendor).

### The ten facts a planner must not miss

1. **The hold is awaited INSIDE the run service, between the deal and the effects** (`run-service.ts:200-220` at
   the pin) — so in v5 it lands in `pascal/run_service.rs` between `execute_custom_tool` (`:258-291`) and
   `apply_effects_for_run` (`:295-310`). P4.D345 can stay inside `pascal/**` + `api/**` + `tools/run_custom.rs`
   **only if P4.D348 hands it ONE contract: the turn's `StopSignal` reaching the tool executor** —
   `ToolExecutionContext` (`services/tool_execution.rs:257-316`) carries NO turn-control handle today; the
   executor's `run_run_custom` (`tools/executor.rs:730-790`) builds `RunCustomToolContext` without one. Named in §7
   as **`TurnStopReach`**.
2. **v4's `keepAlive` ping has NO v5 counterpart by architecture.** v4 holds the turn open on the SSE response and
   pings `: keep-alive` every 15 s (`turn-control.ts:51/72/82/120`, `turn-stream.ts:60`, `registry.ts:35
   KEEP_ALIVE_MS`). v5's Salon send is a JSON `ChatSend` dispatch the host AWAITS whole on a spawned thread
   (`quilltap-host/src/spine.rs:2542-2585`); the Event channel pings itself (`quilltap-web/src/events.rs:33
   KEEP_ALIVE_INTERVAL = 15 s`). A held run therefore holds the `POST …/messages` response open with NO bytes for
   as long as the operator takes. **Ruling owed (§10 R-1):** record `HOLD_KEEP_ALIVE_NO_STREAM` as a deliberate
   divergence (the registry's `keepAlive` option NOT ported; the SPA's `fetch` has no timeout —
   `core-transport.ts` sets none, measured) — or thread a `: keep-alive`-equivalent through the dispatch. The
   survey recommends the divergence.
3. **`PascalMetaIn` silently DROPS unknown keys** (`db/chats_messages.rs:283-345` — `#[derive(Deserialize)]`
   with no `deny_unknown_fields`). Without a new `intervention: Option<Value>` field, the operator's record
   (`pascalMeta.intervention`) would be written by the run service and silently lost at the row. That field is
   P4.D345's ONE edit in a KEYSTONE-owned file (the keystone adds `storytellerBid` plumbing in the same file) —
   P4.D345 is wave 1, so it branches from the keystone's tip; no textual overlap (different struct).
4. **The execution oracle emits the WHOLE run result** (`harness/oracle/cases/pascal-custom-tools-execution.test.ts:2168-2185`
   `out`), so at the pin EVERY `executeCustomTool` row carries `draw`, `rollSpan`, `valueSpan` (+ `llmPromptRendered`
   on consult rows) — `pascal_custom_tools_execution_equivalence` REDDENS whole until `CustomToolRunResult`
   (`custom_tools.rs:253-284`) grows the keys in v4's order. The route preview (`api/custom_tools.rs:1189-1254`)
   and the chat POST `result` body redden the same way; the Workbench library reddens on `intervention` AND on
   P4.D343's `effect` (one struct, two lanes — §7 contract **`LibraryEntryKeys`**).
5. **The definition grammar is a THREE-WAY twin with ONE corpus** (R-F): v4 Zod ⇄ `pascal/custom_tool_types.rs`
   ⇄ `apps/web/src/app/pascal/custom-tool-types.ts`, proven by `harness/oracle/cases/pascal-custom-tool-definition.ts`
   → `QT_ORACLE_PASCAL_DEFINITION` (Rust) and the byte-equal committed copy
   `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson` (716 lines; the spec header says
   710 rows = 10 title + 601 definition + 99 gate, regenerated ONCE at `322a5488e` by P4.D333/the unifier). The
   standing rule: **the corpus is regenerated ONCE per round, by one lane, copied byte-equal to the SPA.** Both
   `KNOWN_TOP_LEVEL_KEYS` twins are hand-written `[…; 17]` lists (`custom_tool_types.rs:5399-5417`,
   `custom-tool-types.ts:3206`) with `unknown-keys-*-known` tripwire rows — `intervention` makes them 18. P4.D345
   owns the ONE regen (its rows: the `intervention` block, `intervene` on parameters/effects); P4.D346 (wave 0)
   cannot be green against it until its SPA twin parses the same — **stack P4.D346's grammar part on P4.D345's
   Tier-1 commit, or let the unifier copy the NDJSON** (§7 / §9).
6. **The custom-tool JSON Schema is touched by exactly ONE span commit — `839575d89`** (66,469 B at the pin, md5
   `454fa04e7a11caa98becd877212f20e2`; v5 vendors 63,595 B at THREE sites guarded by
   `custom_tool_schema_embed_guard.rs:30` and `public_schemas_vendor_guard.rs:57`). P4.D345 re-vendors the Rust
   specimen (`pascal/specimens/qtap-custom-tool.schema.json`); the SPA copy `apps/web/public/schemas/` is
   P4.D346's under the same bytes (the guard compares them) — see §9. The two `CUSTOM_TOOL_SPEC*.json` specimens
   are UNCHANGED at the pin (md5 equal).
7. **`b02d902f3` deleted the only error text the ledger quotes for Re-ask** — at the pin
   `handleInterventionReask` refuses ONLY a definition without `llm` (`This tool consults no oracle`, 400); the old
   `This run's consult is not open to intervention` must NOT be ported. A hidden `llmPrompt` is still refused
   through `checkOverrides` (`the llmPrompt is not open to intervention`).
8. **The chat-settings PUT arm collides with P4.D356.** `3b8b6f42b` (P4.D356's) adds `ACCEPTED_KEYS` containing
   `'customToolIntervention'` (`settings/chat/route.ts:41-57` at the pin) and the unknown-key 400; the arm itself
   is `839575d89`'s (`:210-214`). v5's `api/settings.rs:952-956` is the `customToolTriggers` arm. Recommendation:
   **P4.D356 owns `api/settings.rs` whole this round** (the arm in v4's order: `customToolTriggers` →
   `customToolIntervention` → `agentModeSettings`, the ACCEPTED entry, the `settings_routes_equivalence` rows);
   P4.D345 does not touch it. Same for the Almanack line + `help_settings` key (§7).
9. **The realtime topic list is shared with Storyteller.** v4 appends `'storytellerRounds'` (`b15fe9404`) THEN
   `'customToolInterventions'` (`839575d89`) after `'memories'` in `REALTIME_TOPICS` (`lib/schemas/realtime.types.ts`)
   — v5's `realtime/types.rs:54-61` ends at `Inventory` (v4 appended `inventory` after `memories` earlier), and
   the SPA's `realtime.types.ts:48-66` / `realtime-topic-map.ts:151-159` likewise; `realtime-topic-map.spec.ts:136`
   asserts the LAST topic is `'inventory'`. Named contract **`RealtimeTopicAppend`** (§7): final order
   `…, inventory, storytellerRounds, customToolInterventions`; each lane adds ONLY its own entry; the "last topic"
   assertion is rewritten by each lane to `toContain`, the unifier restores the positional assert.
10. **Dogfood hazard the lanes must queue (§11):** with `customToolIntervention` defaulting ON and holds having NO
    timeout, an intervenable tool reached in a chat left unattended holds the turn (and the `ChatSend` POST) forever
    until Stop or the dialog; v4 says so in `help/chat-settings.md` ("A held roll waits for you indefinitely").
    The first dogfood row is the veto-by-Stop and the dialog's reappearance after a reload (the hold is server
    state, not stream state — `registry.ts:4-16`).

Ledger corrections carried forward (§1 cites each): the ledger row for `839575d89` says `pascalMeta.intervention
when something changed` — correct, and more precisely a `deal` whose `changedFields` is EMPTY is downgraded to
`as-rolled` and records nothing (`run-service.ts:345-410`); the row's "Routes … GET `?action=interventions`,
POST `?action=intervention-preview|intervention-reask|intervention-resolve`" is right; the row's statement that
the migration "logs the `Added …` INFO TWICE" is right (`add-custom-tool-intervention-field.ts:54-56` and
`:61-65`); the row says nothing of `chat_settings.repository.ts:73`'s seed `customToolIntervention: true` (a
read-site the keystone's seed row must carry — `provisioning/chat_settings_seed.json`). The `8ae8a127c` row says
"spans ignore pins" — correct (`rollRange` computes `rollSpan` from the resolved `min`/`max` only, `:1045-1046`).

## 1. Per commit, the shipped hunks

### 1.1 `839575d89` — operator intervention (production files; tests listed in §5)

**Format — `lib/pascal/custom-tool.types.ts`** (pin lines):
- `:248-254` `CustomToolParameterSchema.intervene: z.boolean().optional().describe('Operator intervention only:
  false hides this parameter from the hold dialog. Absent or true shows it. Ignored unless the tool\'s intervention
  block is enabled.')` — a model-visible `.describe()` (the description reaches `edit_custom_tool`'s schema
  action only through the published JSON Schema, whose text differs — see the schema hunk below).
- `:997-1003` `CustomToolEffectSchema.intervene` — same text with "this effect".
- `:1303-1315` `INTERVENTION_FIELD_KEYS = ['roll','value','llmPrompt','llmAnswer','outcome','message','state',
  'chipLabel','title','visibility','announcer'] as const` (closed; order is load-bearing for
  `interventionEligibility`'s `fields` order).
- `:1328-1343` `CustomToolInterventionSchema = z.strictObject({ enabled: z.boolean().describe('Required. true
  holds each model- or trigger-reached run for the operator; false keeps the block but switches it off.'), fields:
  z.strictObject({<each key>: z.boolean().optional()}).optional().describe('Per-field visibility in the hold dialog.
  A key set to false hides that field; absent keys are shown.') })`. An EMPTY block `{}` is refused (`enabled`
  required — v4 test "rejects an empty block as ambiguous"); an unknown `fields` key is a strict-object
  `Unrecognized key` issue; `null` for `enabled` is refused (boolean, not nullable).
- `:1415-1419` `QtapCustomToolSchema.intervention: CustomToolInterventionSchema.optional().describe('Operator
  intervention: hold each run a model or a trigger phrase reaches, so the operator can adjust, force or veto it
  before it is announced. Never shown to the model — but a character with read access to the store can read the
  file, flag and all.')` — placed AFTER `announceAsCharacter`, BEFORE `parameters` (key order of the parsed
  object; `KNOWN_TOP_LEVEL_KEYS` count 17 → 18).

**Engine — `lib/pascal/custom-tools.ts`** (pin lines):
- `:752` `export type RecordedDraw = { form: 'range'; unit: number } | { form: 'dice'; faces: number[] }`.
- `:768-779` `EffectPin { mode: 'auto'|'on'|'off'; value?; items?; log? }`; `:784-805` `RunPins { roll?, dice?,
  value?, llmAnswer?: {ok, output}, outcomeIndex?, message?, state?, chipLabel?, effects?: Record<number, EffectPin> }`.
- `CustomToolRunResult` gains, AFTER `effects?` in declaration order: `draw: RecordedDraw` (`:842`), then (from
  `8ae8a127c`) `rollSpan: RunSpan` (`:844`), `valueSpan: RunSpan` (`:846`), then `llmPromptRendered?: string`
  (`:852`), `tableOutcomeIndex?: number` (`:854`). The emitted object literal (`:2355-2368`) spreads in this
  order: `…effects, draw, rollSpan, valueSpan, …llmPromptRendered, …tableOutcomeIndex` — **this is the wire /
  oracle key order.**
- `:997-1002` `cryptoUniform(min,max)` → `cryptoUnit(): number` — the SAME 6-byte / 2^48 math, returning the unit
  fraction; the scaling `min + unit * (max - min)` moved into `rollRange` (`:1038`).
- `:1011-1047` `rollRange(definition, range, params, state, inventory, unitDraw?, pins?)` → `{raw, value, unit,
  rollSpan, valueSpan}`: `const unit = unitDraw ?? cryptoUnit()` is taken ALWAYS (even for `min === max` — **this
  changes the byte-consumption fact the execution family pins**: v4 at `322a5488e` drew NOTHING on a degenerate
  range; at the pin it draws 6 bytes and discards the fraction, `raw = pins?.roll ?? (min === max ? min : min +
  unit*(max-min))`); `value = raw*multiplier + offset`, rounded if `round`, then `if (pins?.value !== undefined)
  value = pins.value` AFTER rounding; the non-finite check stays.
- `:1634-1700` `resolveEffects(definition, subjects, pins?)`: per index, `pin.mode === 'off'` → `skipped(index,
  'switched off for this run')`; `mode !== 'on'` → evaluate `when` as before (`'condition did not hold'` /
  `condition could not be evaluated: …`); `mode === 'on'` skips the test; `literalItems = pin.items.map(trim)
  .filter(non-empty)` replaces a wardrobe / inventory gesture's `items` and is passed as `literal = true` to
  `resolveWardrobeGesture` / `resolveInventoryGesture` (no `renderTemplate` on each item); a log effect's line is
  `pin.log ?? renderTemplate(effect.log)` then trimmed (`'log line rendered empty'` still applies); a write with
  `pin.value !== undefined` returns `{index, target, value: pin.value}` BEFORE the expression path.
- `:2043-2053` `export function poseLlmConsult(definition, prompt, invoke)`: throws `CustomToolRunError(`${name}:
  this tool consults no model`)` without `llm`; else `resolveLlmConsult(name, definition.llm, prompt, invoke)` (the
  existing fail-soft resolver — its `Custom tool LLM consult failed` WARN at `:2007` fires as today).
- `:2099-2137` `drawRoll(…, replay?: {draw?, pins?})`: dice — `faces = pins?.dice ?? (draw.form==='dice' ? draw.faces
  : undefined)`; `faces ? diceFromFaces(name, parsed, faces) : rollNotation(parsed)`; returns `draw: {form:'dice',
  faces:[...results]}` and `rollSpan = valueSpan = diceSpan(parsed)`. Range — `unit = draw.form==='range' ?
  draw.unit : undefined` → `rollRange(…, unit, pins)`; `draw: {form:'range', unit: used}`.
- `:2140-2150` `diceFromFaces(toolName, notation, faces)`: `faces.length !== notation.count` → `CustomToolRunError(`${toolName}:
  ${faces.length} dice given for ${count}d${sides}`)`; each face `Math.min(sides, Math.max(1, Math.round(face)))`;
  `subtotal` sum; `total = subtotal + modifier`.
- `:2172-2368` `executeCustomTool(definition, supplied, overrides?)` gains `draw?` (`:2202`), `consult?` (`:2207`),
  `pins?` (`:2209`). Order: `resolveParams`; **`if (pins?.outcomeIndex !== undefined && !definition.outcomes[pins.
  outcomeIndex]) throw CustomToolRunError(`${name}: there is no outcome row ${pins.outcomeIndex}`)`** (`:2217`);
  `prepareRoll`; `drawRoll`; the consult closure (`:2260-2264`): `pins.llmAnswer` → `{ok, output, prompt}` (NO
  provider/model/reason), else `overrides.consult` VERBATIM (its old `prompt` kept — this is how `staleAnswer`
  is detected), else `resolveLlmConsult`; both run orders set `llmPromptRendered` to the prompt rendered THIS run
  and `outcomeIndex = pins?.outcomeIndex ?? tablePick`, `tableOutcomeIndex = tablePick` only when pinned
  (`:2272-2297`); `finalState = pins?.state ?? outcome.state` feeds `winning.state` (so effects test the pinned
  verdict) and `result.state`; `message = pins?.message ?? renderTemplate(outcome.message, …)` (literal — no
  placeholders re-rendered); `chipLabel = pins?.chipLabel ?? render(…)` only when the definition declares one;
  `resolveEffects(definition, effectSubjects, pins?.effects)`.

**Engine — `8ae8a127c`'s additions** (same file): `:760-766` `RunSpan { min, max, minExclusive?, maxExclusive? }`;
`rollRange` ends `rollSpan = min === max ? {min,max} : {min,max,maxExclusive:true}` and `valueSpan =
transformSpan(rollSpan, multiplier, offset, range.round === true)`; `:1056-1072` `transformSpan`: `low = min*m+o`,
`high = max*m+o`; `m === 0` → `{min:o, max:o}` (NO exclusive flags); `m < 0` → `{min:high, max:low, minExclusive:
true if span.maxExclusive}`; else `{min:low, max:high, maxExclusive: true if span.maxExclusive}`; if `round`:
`{min: Math.round(out.min), max: out.maxExclusive ? Math.ceil(out.max - 0.5) : Math.round(out.max)}` (flags
DROPPED — both ends reachable integers; note `minExclusive` on a swapped span is dropped by rounding too — v4's
`Math.round(out.min)` on an exclusive low end can name an unreachable value; port faithfully, record as
`ROUND_DROPS_MIN_EXCLUSIVE` only if the corpus shows it). `:1074-1076` `diceSpan(n) = {min: count+modifier, max:
count*sides+modifier}`. `JS Math.round` semantics (halves toward +∞) — v5 has `js_round` (`custom_tools.rs:415`).

**Hold machinery — NEW `lib/pascal/intervention/`** (all read at the pin; see the bytes in the tool output
captured for this survey):
- `registry.ts` (213 lines): `KEEP_ALIVE_MS = 15_000` (`:35`), `SETTLED_MEMORY = 200` (`:38`);
  `InterventionDecision = {kind:'deal', overrides, run: RecomputedRun, reasks} | {kind:'as-rolled'} | {kind:'veto',
  cause:'dialog'|'stop'}` (`:41-45`); `PendingIntervention { id, chatId, userId, createdAt, entrance:'llm'|'trigger',
  characterId, characterName, triggerPhrase?, definition, definitionTier, definitionMountId, snapshot, draw,
  baseline, consults: LlmConsultResult[] }` (`:47-66`); `processSingleton('pascal.interventionRegistry')`
  `{byId: Map, settled: string[]}` (`:78-81`); `hold(pending, {signal?, keepAlive?}) → {id: randomUUID(),
  decision: Promise}` (`:93-150`): registers, starts a `setInterval(ping, KEEP_ALIVE_MS)` that stops itself when
  the ping returns false but KEEPS WAITING; an already-aborted signal settles via `queueMicrotask(onAbort)` so the
  add publishes before the removal; logs INFO `Custom tool run held for the operator` and publishes
  `customToolInterventions` for the chat; `settleIntervention(id, decision) → 'resolved'|'unknown'|'settled'`
  (`:158-180`): first wins, `settled` ring trimmed to 200, WARN `Custom tool hold resolve refused` on a miss, INFO
  `Custom tool hold resolved`, publish again; `interventionStatus(id)` (`:183`), `getIntervention(id)` (`:189`),
  `listInterventions(chatId)` oldest-first by `createdAt` (`:194-199`), `addConsult(id, consult) → boolean` (`:202`),
  `resetInterventionRegistryForTests` (`:210`).
- `eligibility.ts` (75): `isIntervenable(def) = def.intervention?.enabled === true` (`:19`);
  `interventionEligibility(def, rollForm) → {fields, params, effects}` (`:44-75`): `applicable` map in
  `INTERVENTION_FIELD_KEYS` order — `value` only for `'range'`, `llmPrompt`/`llmAnswer` only with `llm`,
  `chipLabel` only when declared, the rest always; `fields = keys.filter(applicable && flags[key] !== false)`;
  `params` = declared names whose `intervene !== false` (declaration order); `effects` = indices whose
  `intervene !== false`.
- `overrides.ts` (131): `EffectPinSchema = strictObject{ mode: enum(auto|on|off), value?: number().finite() |
  string | boolean, items?: array(string().max(400)).max(16), log?: string().max(MAX_LLM_OUTPUT_CEILING + 4000) }`
  (`:34-39`); `InterventionOverridesSchema = strictObject{ params?: record(string, Literal), roll?: finite,
  dice?: array(int).min(1).max(MAX_DICE_COUNT=100), value?: finite, llmPrompt?: string().min(1).max(MAX_LLM_PROMPT_LENGTH),
  llmAnswer?: strictObject{ok: boolean, output: string().max(MAX_LLM_OUTPUT_CEILING)}, outcomeIndex?: int().min(0),
  message?: string().min(1).max(ceiling+4000), state?: OutcomeStateSchema, chipLabel?: min(1).max(MAX_CHIP_LABEL_LENGTH),
  title?: min(1).max(MAX_TITLE_LENGTH), visibility?: VisibilitySchema, announcer?: enum(pascal|character),
  effects?: record(string().regex(/^\d+$/, 'effect keys are indices'), EffectPinSchema) }` (`:41-57`);
  `InterventionDecisionSchema = discriminatedUnion('kind', [ {kind:'deal', overrides}, {kind:'as-rolled'},
  {kind:'veto'} ])` — all strictObjects (`:63-67`); `checkOverrides(def, {rollForm, diceCount?}, overrides) →
  string | null` (`:82-131`) with these EXACT refusal texts, in this check order: `parameter "${name}" is not open
  to intervention`; `the roll is not open to intervention` (roll OR dice given while `roll` hidden); `a dice roll
  is set by its faces, not a number` (`roll` on dice form); `this roll is not dice` (`dice` on range form);
  `this roll takes ${diceCount} dice, not ${n}`; then for the pairs `[value→value, llmPrompt, llmAnswer,
  outcomeIndex→outcome, message, state, chipLabel, title, visibility, announcer]`: `the ${field} is not open to
  intervention`; `there is no outcome row ${i}`; `effect ${i} is not open to intervention`.
- `recompute.ts` (269): `RunSnapshot { params: Record|null, private: boolean|undefined, metadata, progress, now,
  character: string|null, state, inventory }` (`:32-43`); `RunPresentation {title, visibility, announcer}`;
  `RecomputedRun { result, presentation, staleAnswer, promptNow?, clampedParams, warnings }` (`:65-76`);
  `pinsFromOverrides` (`:79-93`, effects re-keyed by `Number(key)`); `presentationFor(def, visibility,
  {title?, announcer?}) = { title: overrides.title ?? displayTitle(def), visibility, announcer: overrides.announcer
  ?? (def.announceAsCharacter === true ? 'character' : 'pascal') }` (`:96-106`); `recomputeRun(input)` (`:112-179`):
  `supplied = {...snapshot.params, ...overrides.params}`; `privacy = overrides.visibility !== undefined ?
  visibility === 'whisper' : snapshot.private`; calls `executeCustomTool` with the snapshot's metadata / progress /
  now / character / state / inventory + `draw`, `consult`, `pins`; `clampedParams` = override names whose resolved
  value `!==` the given one; `promptNow = def.llm ? (overrides.llmPrompt ?? result.llmPromptRendered) : undefined`;
  `staleAnswer = llm && overrides.llmAnswer === undefined && consult !== undefined && promptNow !== consult.prompt`;
  warnings, in order: `The table would have dealt row ${tableOutcomeIndex+1} here; the odds are shown to the
  model, so a forced row may look out of place.` (only when `revealOdds !== false`); `The message is pinned, but a
  different outcome row now wins; check that the words still fit it.`; `The pinned draw lies outside what the
  table could deal (${describeSpan(rollSpan)}).`; `The pinned value lies outside what the modifiers could reach
  (${describeSpan(valueSpan)}).`; `The consult's answer was given to a different prompt. Re-ask, or deal it as it
  stands.`. `summarizeEffects` (`:190-200`: `target = authored.target ?? (wardrobe ? 'wardrobe' : inventory ?
  'inventory' : 'log')`, `next` = value / wardrobe.items / inventory.items / log); `RunFacts` + `factsOf`
  (`:203-236`); `changedFields(dealt, final)` (`:246-269`) — keys `params.<name>`, `roll` (raw OR diceRolls differ),
  `value`, `llmAnswer` (ok+output JSON-compared), `outcome`, `message`, `state`, `chipLabel`, `title`,
  `visibility`, `announcer`, `effects.<index>` (fired or `next` differ), in THAT order.
- `veto-result.ts` (23): `VETO_RESULT_PLAIN = 'The tool did not run.'`; `VETO_RESULT_TRANSPARENT = 'The table is
  closed for the moment; this tool did not run. Carry on without it.'`; `vetoResultFor(transparent)`.
- `view.ts` (100, types only): `InterventionEffectView { index, kind: write|wardrobe|inventory|log, label,
  fired, skipped?, value?, items?, log? }`; `InterventionRunView { params, rollForm, notation?, raw, diceRolls?,
  diceBreakdown, rollSpan, value, valueSpan, outcomeIndex, tableOutcomeIndex?, state, message, chipLabel?, title,
  visibility, announcer, llm?: {ok, output, prompt}, effects }`; `InterventionPreview { run, staleAnswer,
  promptNow?, clampedParams, warnings }`; `InterventionHoldView { id, tool, title, characterName, entrance,
  triggerPhrase?, createdAt, eligible, hasCharacter, definition: { parameters, outcomes: [{when, state, message,
  brief?}], effects, revealOdds, hasLlm, diceSides? }, baseline: InterventionPreview, consults }`.
- `spans.ts` (36, `8ae8a127c`): `withinSpan(n, span)` honouring exclusive ends; `describeSpan`: `exactly ${min}`
  when `min === max` and neither exclusive; `more than ${min}, up to ${maxExclusive ? '(not including) '+max :
  max}` when `minExclusive`; `${min} up to (not including) ${max}` when `maxExclusive`; else `${min} to ${max}`;
  numbers via `Number.isInteger(n) ? String(n) : String(Number(n.toPrecision(4)))` — v5 has both twins in
  `pascal/js_value.rs`: `number_to_string` (`:207`) and `to_precision(x, p)` (`:260`, ECMA-262 20.1.3.5) — measured,
  nothing NEW.

**Run service — `lib/pascal/run-service.ts`** (pin): `RunCustomToolParams.mayHold: boolean` (`:103`);
`RunCustomToolOutcome` gains `| { ok: 'vetoed' }` (`:125`); the hold block `:200-220` (after execute, before
effects): `presentation = presentationFor(def, result.visibility)`; `if (await mayHoldRun(params, chat))` →
`holdForOperator(params, {characterName, result, snapshot: {params: params.parameters ?? null, private:
params.private, metadata, progress, now: nowMs, character: characterName, state: cascade?.merged ?? {}, inventory}})`;
`veto` → `return {ok:'vetoed'}`; `deal` → `result = held.run.result; presentation = held.run.presentation;
intervention = held.record`. Downstream: `toolTitle = presentation.title` (was `displayTitle`), `announcerCharacterId
= presentation.announcer === 'character' ? characterId : null`, `pascalMeta: {…, …(intervention ? {intervention}
: {})}` appended LAST (after `callerParticipantId`). `mayHoldRun` (`:306-337`): `!isIntervenable` → false SILENT;
then each skip logs DEBUG `Custom tool run not held` `{context, chatId, tool, reason}` with reasons `this entrance
never holds (the operator's own run)`, `running in the job child` (`QUILLTAP_JOB_CHILD === '1'`), `an autonomous
room` (`chat?.chatType === 'autonomous'`), `custom tools are off` (`settings?.customTools === false`), `the
operator switched intervention off` (`settings?.customToolIntervention === false`); an unreadable settings read →
DEBUG `Custom tool hold: chat settings unreadable, holding by default` `{context, chatId, error}` and HOLD. Note the
chat read: `chat = params.chat ?? findById(chatId)` is now stored (`:147-149`) so the autonomous check reuses it.
`holdForOperator` (`:345-410`): `hold({… entrance: invokedBy === 'trigger' ? 'trigger' : 'llm', consults: result.llm
? [result.llm] : []}, {signal: currentTurnStopSignal(), keepAlive: currentTurnKeepAlive()})`; veto → INFO `Custom
tool run set aside by the operator` `{context, chatId, tool, invokedBy, cause}`; deal → `changed = changedFields(
factsOf(before), factsOf(after))`, INFO `Custom tool run dealt by the operator` `{context, chatId, tool, changed,
reasks, staleAnswer}`, **`changed.length === 0` → `{kind:'as-rolled'}` (records nothing)**, else the record
`{ resolvedAt: new Date().toISOString(), decision: 'deal', changed, dealt: before, …(reasks > 0 ? {reasks}),
…(staleAnswer ? {staleAnswer: true}) }`.

**Entrances:** `lib/tools/handlers/run-custom-handler.ts` — `mayHold: true` (`:217`); `callerKnowsItIsAGame(ctx)`
(`:104-123`): `chats.findById` → the caller seat `controlledBy === 'user'` → true; no `characterId` → false;
`characters.findByIdRaw(characterId)?.systemTransparency === true`; any throw → DEBUG `run_custom veto: caller
transparency unreadable, treating as plain` `{context, chatId, error}` → false. The vetoed arm (`:227-238`): NO
Prospero bubble; INFO `run_custom set aside by the operator` `{context, chatId, tool, transparent}`; returns
`failure(vetoResultFor(transparent), toolName)` — the model's result is the ORDINARY failure shape (`success:false,
error: <veto text>`), so `formatRunCustomResults` renders `Custom tool error: The tool did not run.`.
`lib/pascal/trigger-runner.ts` — `mayHold: true` (`:198`); `outcome.ok === 'vetoed'` → DEBUG `Triggered custom tool
set aside by the operator` `{context, chatId, tool, phrase}` and `continue` (`:211-220`). The operator's `?action=run`
(`app/api/v1/chats/[id]/custom-tools/route.ts:465`) passes `mayHold: false` and keeps an unreachable arm `vetoed →
successResponse({messages: [], result: null})` (`:469-472`).

**Turn control — `lib/chat/turn-control.ts`:** `TurnControlEntry.keepAlive?: () => boolean` (`:51`);
`runWithTurnControl(chatId, fn, options: {keepAlive?})` (`:72-82`); `currentTurnKeepAlive()` (`:120-122`). The ONE
caller: `lib/services/chat-message/turn-stream.ts:60` `{ keepAlive: () => safeEnqueue(controller,
encodeKeepAlive(encoder)) }` (the pipeline refactor `a8214b253` moved it there from `orchestrator.service.ts`;
`encodeKeepAlive` = `': keep-alive\n\n'`, `streaming.service.ts:882-884`).

**Routes — `app/api/v1/chats/[id]/custom-tools/route.ts`:** GET `withActionDispatch({ interventions:
handleListInterventions }, handleList)` (`:517`); POST `{ run, 'intervention-preview', 'intervention-reask',
'intervention-resolve' }` (`:521-526`). NEW `interventions.ts` (353 lines, read whole): `HANDLER =
'api.v1.chats.custom-tools.interventions'`; `effectView` (`:70-93`: `kind` from the authored effect, `label` =
`target ?? 'write'` / `wardrobe ${action}` / `inventory ${action}` / `'log'`); `runView` (`:95-117`, key order
`params, rollForm, notation?, raw, diceRolls?, diceBreakdown, rollSpan, value, valueSpan, outcomeIndex,
tableOutcomeIndex?, state, message, chipLabel?, title, visibility, announcer, llm?, effects`); `previewOf`;
`holdView` (`:129-164` — `title = presentationFor(def, baseline.visibility).title`, `eligible =
interventionEligibility(def, baseline.rollForm)`, `hasCharacter = characterId !== null`, `definition.diceSides` from
`parseDiceNotation(baseline.notation).sides`, `baseline.promptNow = baseline.llmPromptRendered`, `consults` mapped
to `{ok, output, prompt}`); bodies read through `readBody` — a non-JSON body → 400 `The request body is not JSON`,
a Zod failure → 400 `issues.map(message).join('; ')`; `previewSchema = {id: string().min(1), overrides}`,
`resolveSchema = {id, decision}`. Handlers: **list** → `{holds}` + DEBUG `Custom tool holds listed` `{context,
chatId, holds: n}`; **preview** → 404 `notFound('Held run')` (v4's envelope `Held run not found`), 400 on a
refusal, 400 on `CustomToolRunError.message`, DEBUG `Custom tool hold previewed` `{context, chatId, holdId,
pinned: Object.keys(overrides), staleAnswer}` → `{preview}`; **reask** (post-`b02d902f3`) → 404; `!llm` → 400
`This tool consults no oracle`; `llmAnswer` STRIPPED from the overrides; refusal → 400; `before = recompute`;
`prompt = before.promptNow ?? definition.llm.prompt`; `consult = poseLlmConsult(def, prompt,
buildCustomToolLlmInvoker({userId, chatId}))` (💸 spends a model call); `addConsult` false → 409 `That run has
already been settled`; DEBUG `Custom tool hold re-asked` `{context, chatId, holdId, ok, reasks: consults.length-1}`;
`after = recompute` → `{preview, consults}`; a non-`CustomToolRunError` throw logs WARN `Custom tool hold re-ask
failed` `{context, chatId, error}` and rethrows (500); **resolve** → missing hold: WARN `Custom tool hold resolve
refused` `{context, chatId, holdId, status}` then 409 `That run has already been settled` if `status === 'settled'`
else 404 `Held run`; `as-rolled` / `veto` settle (`cause: 'dialog'`) → `{resolved: kind}` or 409; `deal` → refusal
400, recompute (400 on `CustomToolRunError`), `reasks = max(0, consults.length - (baseline.llm ? 1 : 0))`, settle →
`{resolved: 'deal'}` or 409. An unknown `?action=` on GET or POST → v4's `withActionDispatch` envelope (`lib/api/
middleware/actions.ts:209`; v5 `quilltap-web/src/query.rs` `dispatch_action` / `dispatch_required_action` already
answer the same envelope — `custom_tools_routes.rs:44-80`).

**Settings & records:** `lib/schemas/settings.types.ts:815` `customToolIntervention: z.boolean().default(true)`
(after `customToolTriggers`, before `compositionModeDefault`); `chat-settings.repository.ts:73` seed `true`;
`app/api/v1/settings/chat/route.ts:210-214` the arm (`Invalid customToolIntervention value (must be boolean)`),
`:80/:450/:489` the plumbing; `lib/tools/almanack/types.ts:279`, `phase3-ledgers.ts:635` (default `true`) and
`:743` (`?? true`), `render.ts:729` ``- **Hold Intervenable Rolls for the Operator**: ${yesNo(…)}`` right after
the trigger line; `help-settings-handler.ts:114` `customToolIntervention` between `customToolTriggers` and
`llmLoggingSettings`; `lib/startup/prettify.ts` label `'add-custom-tool-intervention-field-v1': 'Teaching Pascal to
wait for your nod'` (NO-PORT — the deferred migration runner); `lib/schemas/chat.types.ts:572-610`
`MessageEventSchema.pascalMeta.intervention?: { resolvedAt: string, decision: enum(deal|as-rolled), changed:
string[], dealt: { params, raw, diceRolls?, value, outcomeIndex, state, message, chipLabel?, toolTitle?,
visibility, announcer, llm?: {ok, output, prompt}, effects?: [{index, fired, target, next?}] }, reasks?: int,
staleAnswer? }` (last key of `pascalMeta`); `lib/pascal/workbench.ts:48-49` `CustomToolLibraryEntry.intervention:
boolean` = `isIntervenable(def)` (`:201`), declared after `triggerCount` and BEFORE `effect` (`b15fe9404`'s);
`lib/realtime/job-topics.ts` `customToolInterventions: []` (never published from the job child); `topic-map.ts`
`case 'customToolInterventions': id ? [queryKeys.customTools.interventions(id)] : [queryKeys.customTools.all]` and
`ALL_REALTIME_PREFIXES += queryKeys.customTools.all`; `lib/query/keys.ts` `customTools.interventions(chatId) =
['custom-tools', chatId, 'interventions']`; `lib/schemas/realtime.types.ts` `'customToolInterventions'` appended
LAST (after `'storytellerRounds'`).

**Published schemas:** `public/schemas/qtap-custom-tool.schema.json` +62 (the `intervention` object with
`additionalProperties: false`, `required: ["enabled"]`, the eleven boolean `fields`; `intervene` on parameters and
on effects — each description ends "Never shown to the model — but the file is an ordinary document, so a character
with character_read on the store can read the flag, as with revealOdds."); `qtap-export.schema.json` +15 the
`pascalMeta.intervention` object (`required: ["resolvedAt","decision","changed","dealt"]`).

**Docs / help (vendor lane bytes, §1.4):** `help/chat-settings.md` +1 (the **Hold intervenable rolls for me**
bullet), `help/custom-tools.md` +34 (the `## Taking a Hand in the Deal — \`intervention\`` section + the
"Held rolls have one too" paragraph), `help/pascals-workbench.md` +1 (the **Taking a hand** bullet), `docs/developer/
API.md` (the `customToolIntervention` sentence), `DDL.md` (the column + the `pascalMeta` comment's `intervention`
clause), `CHANGELOG.md`, `features/ROADMAP.md`, the spec moved to `features/complete/`.

### 1.2 `8ae8a127c` — spans

Engine + `spans.ts` + `view.ts` + `recompute.ts` hunks as above; `interventions.ts:103/:105` add `rollSpan` /
`valueSpan` to `runView`; the dialog's `SpanHint` (`:536-543`) under the roll (`The table draws ${describeSpan(rollSpan)}.`
/ `${notation ?? 'The dice'} can come to ${describeSpan(rollSpan)}, each die 1 to ${diceSides}.`) and under the
value (`After the modifiers, it comes to ${describeSpan(valueSpan)}.`), `data-testid="intervention-roll-span"` /
`"intervention-value-span"`; `help/custom-tools.md` +2 (**Knowing the bounds of the table**). The spans ALSO
reach the Workbench bench wire through `successResponse({...result})` (`app/api/v1/custom-tools/route.ts:287`) and
the chat POST `result` — not in the message.

### 1.3 `b02d902f3` — the re-ask fix

`interventions.ts:267-273` (the `!llm` → `This tool consults no oracle` 400; the eligibility check DELETED);
`CustomToolInterventionDialog.tsx:423-446` the compact **The oracle** section (`data-testid=
"intervention-hidden-consult"`, texts `Re-ask` / `Asking…`, `Your changes have altered what the oracle would be
asked; its answer was given to a different question.`, `The oracle’s answer still fits the question.` (U+2019),
`Asked ${n} times.`); `Section.pinned` / `onReset` made optional (`:509-534`); `help/custom-tools.md` one sentence
widened ("One courtesy survives the hiding of the oracle: …").

### 1.4 `332c650b9` — NO-PORT

Two files: NEW `docs/developer/features/pascal-operator-intervention.md` (492 lines, "Not implemented") + CHANGELOG.
`839575d89` moves it to `features/complete/` (+31). P4.D360 vendors `docs/v4/developer/features/complete/
pascal-operator-intervention.md` and ratifies the row.

## 2. The v5 surface map (v4 production file → v5 home, CURRENT shape)

| v4 (pin) | v5 home | v5 today |
|---|---|---|
| `lib/pascal/custom-tool.types.ts` (`intervene` ×2, `INTERVENTION_FIELD_KEYS`, `CustomToolInterventionSchema`, `intervention`) | `crates/quilltap-core/src/pascal/custom_tool_types.rs`: `CustomToolParameter` `:572-588` + known keys `:3093` `["type","default","description","min","max"]`; `CustomToolEffect` `:1319-1325` + known keys `:3773-3776` `["when","target","value","wardrobe","inventory","log"]`; `QtapCustomTool` `:1668-1730` (`announce_as_character` `:1711`, parse `:4090`); `KNOWN_TOP_LEVEL_KEYS: [&str; 17]` `:5399-5417` | NO `intervene` / `intervention` anywhere (grep clean). Strictness is hand-rolled `unrecognized_keys` (`:1929`); a file carrying `intervention` today is REFUSED at load (`Unrecognized key: "intervention"`) — the red to state. |
| same, the SPA twin | `apps/web/src/app/pascal/custom-tool-types.ts` (`announceAsCharacter` `:1073`, `optBool` `:2372-2403`, emit `:2481`, `KNOWN_TOP_LEVEL_KEYS` `:3206`) | same red; P4.D346's. |
| `lib/pascal/custom-tools.ts` (engine) | `crates/quilltap-core/src/pascal/custom_tools.rs`: `CustomToolRunResult` `:253-284` (ends at `effects`), `crypto_uniform(min,max,rng)` `:515-523` (scales inside), `roll_range(…, rng) → (f64,f64)` `:526`, `resolve_effects(def, subjects)` `:1801`, `resolve_wardrobe_gesture` `:1943`, `resolve_inventory_gesture` `:1989`, `resolve_llm_consult(spec, prompt, invoke)` `:2741` (private), `execute_custom_tool` `:2867` (12 positionals; dice `:2904-2920`, `$inventory` `:2924-2933`, range `:2934-2946`, consult orders `:2988-3040`, message `:3063`, chip `:3070`, effects `:3076-3091`, literal `:3098-3116`), `ResolvedEffect` `:1599`, `LlmConsultResult` `:198` | No draw record, no pins, no spans, no `llm_prompt_rendered` / `table_outcome_index`; `RollRange` degenerate `min == max` short-circuits WITHOUT drawing (the execution family pins `bytesConsumed` — at the pin v4 now ALWAYS draws 6 bytes, see §5). |
| `lib/pascal/intervention/*.ts` (7 files) | **NEW** `crates/quilltap-core/src/pascal/intervention/{mod,registry,eligibility,overrides,recompute,veto_result,view,spans}.rs` (mirror v4's file split; `mod.rs` re-exports) | NO-COUNTERPART today. The registry must be a process-global (v4 `processSingleton`) — the `services/turn_control.rs:95-97` `registry()` `Mutex` + `Arc<Entry>` idiom; the `decision` is a `tokio::sync::oneshot` (first `Sender::send` wins; a second resolve finds the entry gone → `'settled'`). |
| `lib/pascal/run-service.ts` | `crates/quilltap-core/src/pascal/run_service.rs`: `RunCustomToolParams` `:84-131`, `RunCustomToolOutcome` `:141-156` (`Ok` / `Refused`), `run_custom_tool` `:163-380` (execute `:258-291`, effects `:295-310`, `tool_title = display_title(…)` `:319`, `build_pascal_meta` `:323-331` / fn `:441`, announcer `:338-342`, post `:344-354`, `Custom tool dealt` DEBUG `:357-375`) | No `may_hold`, no `Vetoed` arm, title/announcer taken from the definition, pascalMeta assembled without `intervention`. |
| `lib/tools/handlers/run-custom-handler.ts` | `crates/quilltap-core/src/tools/run_custom.rs`: `RunCustomToolContext` `:662-697`, `failure` `:715`, `report_failure` `:756`, the service call `:962-989`, the outcome match `:991-1007` | No veto arm, no transparency read; the precedent for the asker-flag read is `services/carina_query.rs:1076 resolve_asker_flag(db, chat_id, asker_pid, "systemTransparency")` (private — either make it `pub(crate)` [P4.D348's file? no — `carina_query.rs` is nobody's this round; a one-line visibility change is P4.D345's with a note] or re-implement v4's own two reads in `run_custom.rs`; v4's handler reads `chats.findById` + `characters.findByIdRaw`, NOT Carina's helper, so re-implementing is the faithful choice). |
| `lib/pascal/trigger-runner.ts` | `crates/quilltap-core/src/pascal/trigger_runner.rs`: the call `:261-287`, the outcome match `:296-320` | No `Vetoed` arm. **Shared with P4.D343** (`b15fe9404` adds `effectCeiling` + the DEBUG `Triggered custom tool skipped — above the turn's effect ceiling`) — different hunks (`RunTriggeredCustomToolsParams` field + a pre-run `continue` vs a post-run arm); textual proximity only. |
| `lib/chat/turn-control.ts` (`keepAlive`, `currentTurnKeepAlive`) | `crates/quilltap-core/src/services/turn_control.rs` (`TurnControlHandle` `:101`, `StopSignal` `:105-122` with `wait()`, `stop_signal()` `:139`, `is_stopped()` `:144`, `register_active_turn` `:226`) | `StopSignal::wait()` IS the veto hook (v4 `signal.addEventListener('abort')`). No keep-alive concept (§0 fact 2). **P4.D348's file** — P4.D345 needs NOTHING changed in it. |
| `lib/services/chat-message/turn-stream.ts:60` (the keepAlive binding) | the host's send bridge `quilltap-host/src/spine.rs:2542-2585` + `quilltap-web/src/events.rs` | NO-COUNTERPART (ruling R-1). |
| `app/api/v1/chats/[id]/custom-tools/route.ts` + `interventions.ts` | `crates/quilltap-core/src/api/custom_tools.rs` (`chat_custom_tools_list` `:371`, `parse_run_body` `:582`, `chat_custom_tool_run` `:635`, `run_result_to_value` `:1189-1254`), `api/types.rs` (`ChatCustomToolsList` `:2500`, `ChatCustomToolRun` `:2508`; `Response::CustomToolsList/CustomToolRun` `:4815-4817`), `api/engine.rs:5145-5172` (dispatch), `quilltap-web/src/custom_tools_routes.rs:44-80` (GET `dispatch_action(&pairs, &[], …)`, POST `AVAILABLE = &["run"]`) | NEW `api/custom_tool_interventions.rs` (the four handlers + views); NEW `Request` variants + `Response` variants (§6); the web route's `AVAILABLE` lists grow (`interventions` on GET; the three on POST). The unknown-action envelope is already v4's. |
| `app/api/v1/settings/chat/route.ts:210-214` | `crates/quilltap-core/src/api/settings.rs:952-956` (the `customToolTriggers` arm, the pattern) | **P4.D356's file** this round (§7). |
| `lib/database/repositories/chat-settings.repository.ts:73` seed; `settings.types.ts:815` | keystone: `db/chat_settings.rs` six sites (`:913`, `:1062`, `:1164`, `:1203`, `:1311-1313`, `:1524`, `:1650`), `services/provisioning/chat_settings_seed.json`, `fresh_schema.json` (cid 18 `customToolTriggers` → `customToolIntervention` lands between it and `compositionModeDefault`), the boot ensure beside `db/chat_settings_4_11_repair.rs` (`TRIGGERS_DECL = "INTEGER DEFAULT 0"` pattern; `INTEGER DEFAULT 1` here), `restore_vintage_state.rs:162-168` boot-align | **P4.D341's.** P4.D345 reads the struct field `custom_tool_intervention: bool` the keystone adds. |
| `lib/tools/almanack/{types,phase3-ledgers,render}.ts` | `almanack/types.rs:530`, `phase3_ledgers.rs:630` + `:765`, `render.rs:1126-1127` | The three one-liners. Shared with P4.D351 ("Almanack lines") and possibly P4.D343 — §7 assigns them to ONE lane. |
| `lib/tools/handlers/help-settings-handler.ts:114` | `crates/quilltap-core/src/tools/help.rs:215-216` | one `obj.insert("customToolIntervention", …)` after `customToolTriggers`. |
| `lib/pascal/workbench.ts:48-49/:201` | `crates/quilltap-core/src/pascal/workbench.rs` `CustomToolLibraryEntry` `:50-75` (`trigger_count` `:69`) + `apps/web/src/app/core/core-contract.ts:7637-7657` | no `intervention`; **shared with P4.D343/D344's `effect`** — contract `LibraryEntryKeys` (§7). |
| `lib/schemas/realtime.types.ts`, `realtime/job-topics.ts`, `realtime/topic-map.ts`, `lib/query/keys.ts` | `crates/quilltap-core/src/realtime/types.rs:54-61` (+ the test list `:116-123`), `realtime/job_topics.rs:166-169`, `realtime/bus.rs:188 publish_realtime(topic, id)`, `realtime/publish_sites.rs` (the census of publish points); SPA `apps/web/src/app/core/realtime.types.ts:48-66`, `realtime-topic-map.ts:151-159`, `realtime-topic-map.spec.ts:136/:236-238`, `apps/web/src/app/chat/custom-tools.api.ts:18-33` (`customToolsKeys.byChat/presets`) | contract `RealtimeTopicAppend` (§7). The SPA key `customToolsKeys.interventions(chatId) = ['customTools', chatId, 'interventions']` is NEW (v5 keys are per-feature; v4's prefix is `'custom-tools'` — v5 already uses `'customTools'`, keep v5's). |
| `lib/schemas/chat.types.ts` (`pascalMeta.intervention`) | `crates/quilltap-core/src/db/chats_messages.rs` `PascalMetaIn` `:285-345`; the SPA `core-contract.ts:3190-3210` | **UNKNOWN KEYS DROPPED** (§0 fact 3) → `PascalMetaIn.intervention: Option<serde_json::Value>` (opaque — v4's zod shape is `z.object` non-strict; keep it a `Value` and let the carriers pass it through: the `.qtap` export / backup / restore write `pascalMeta` as a stored-TEXT blob — `qtap_export/schema-key-order.json:195` lists `pascalMeta` as one key; the lane MEASURES that `backup/collect.rs` / `restore` never re-parse through `PascalMetaIn` — grep showed no `PascalMetaIn` under `services/backup`/`qtap_import`). |
| `public/schemas/qtap-custom-tool.schema.json` | `crates/quilltap-core/src/pascal/specimens/qtap-custom-tool.schema.json` (`custom_tool_schema.rs` + `custom_tool_schema_embed_guard.rs:30` `SCHEMA_BYTES = 63_595`), `apps/web/public/schemas/qtap-custom-tool.schema.json` (`public_schemas_vendor_guard.rs:57` `bytes: 63_595`), `docs/v4/` mirror | 63,595 → **66,469 B** (md5 `454fa04e…`). §9. |
| `public/schemas/qtap-export.schema.json` | `crates/quilltap-core/src/generators/qtap-export.schema.json` + `qtap_schema.rs` guard | touched by SEVEN span commits — ONE re-vendor, the vendor lane's or the keystone's (§9); P4.D345 does NOT own it. |
| `components/chat/CustomToolInterventionDialog.tsx` (777) | **NEW** `apps/web/src/app/chat/custom-tool-intervention-dialog.ts` (+ `.spec.ts`), mounted beside the popup host in `apps/web/src/app/chat/chat-composer.ts:410` (`<qt-custom-tools-popup …>`) — or at the Salon level in `screens/salon/salon-conversation.ts` where `run-tool-modal` is hosted (v4 mounts it in `ChatModals.tsx`, chat-scoped) | NO-COUNTERPART; §6. |
| `components/custom-tools/InterventionToggle.tsx` (80), `BuilderForm.tsx` (+96), `OutcomesSection.tsx` (+36), `SideEffectsSection.tsx` (+9), `WorkbenchLibrary.tsx` (+7) | `apps/web/src/app/screens/custom-tools/builder-form.ts` (`Reveal the odds` `:221`, `announceAsCharacter` `:244-248`), `workbench-editor.ts`, `workbench-library.ts:231` (`tool.title`), `:303-305` (the `✦ n phrases` badge) | §6. |
| `lib/pascal/tool-draft.ts` (+57) | `apps/web/src/app/pascal/tool-draft.ts` (`announceAsCharacter` `:405/:445/:485/:972/:1222`), `tool-draft.spec.ts`, `tool-draft-grammar.oracle.spec.ts` + `apps/web/src/testing/fixtures/tool-draft-grammar.oracle.ndjson` (244 rows) | NO Rust twin (confirmed: no `tool_draft` in `crates/`); P4.D346 owns the draft twin + ITS corpus regen (the `tool-draft-grammar` corpus is SPA-only; the P4.D334 precedent). |
| `components/settings/chat-settings/{types,AutomationSettings,hooks/useChatSettings}.ts(x)`, `tabs/ChatTabContent.tsx` | `apps/web/src/app/screens/settings/chat/chat-settings.types.ts:138-170` (`AUTOMATION_OPTIONS`, `AutomationKey`, `DEFAULT_CUSTOM_TOOL_TRIGGERS`), `automation-settings.ts:85-113` (`rows` / `onChange`), `core-contract.ts:3953` (`customToolTriggers?: boolean` on the settings wire type), `scalar-toggle-cards.spec.ts` | the third row. |
| `app/salon/[id]/components/system-message-labels.ts` (`getInterventionSummary`), `AnnouncementChip.tsx`, `app/salon/[id]/types.ts` | `apps/web/src/app/chat/system-message-labels.ts` (exports `:77-376`), `message-row.ts:157` (`<span class="qt-chat-system-bar-kind">{{ staffKind() }}</span>`, `staffKind` `:653`), `core-contract.ts:3190-3210` (`pascalMeta` type: `invokedBy` already `'llm'|'user'|'trigger'`) | the badge + its tooltip; §6. |
| `app/styles/qt-components/_intervention.css` (129) + `_index.css` | `apps/web/src/styles/qt-components/` (13 files; NO `_intervention.css`, NO `_storyteller.css`) | NEW file; `_index.css` import line shared with P4.D344/D357 (§7). The pin's bytes already include `cce44e098`'s legibility edits (note ink `--qt-alert-warning-fg`, the struck-through hidden hand) — author from the PIN, not from `839575d89`'s hunk. |
| `help/*.md` ×3, `docs/developer/*` | `help/custom-tools.md` (78,894 B on main vs 84,956 at the pin), `help/chat-settings.md` (14,262 vs 14,757), `help/pascals-workbench.md` (22,467 vs 23,897); `docs/v4/developer/{API,DDL}.md`, `docs/v4/developer/features/complete/` | P4.D360's byte copies (the size deltas include other span commits' hunks). |

## 3. Schema / D23 shapes

The column is the KEYSTONE's (P4.D341); recorded here for its read sites and the ping-pong argument.

- **Migration** `migrations/scripts/add-custom-tool-intervention-field.ts`: id `add-custom-tool-intervention-field-v1`,
  `introducedInVersion '4.11.0'`, `dependsOn: ['add-custom-tool-triggers-field-v1']`, `shouldRun = isSQLiteBackend()
  && sqliteTableExists('chat_settings') && !sqliteColumnExists('chat_settings','customToolIntervention')`,
  `addColumnIfMissing('chat_settings', 'customToolIntervention', 'INTEGER DEFAULT 1')`; INFO `Added
  customToolIntervention column to chat_settings table` `{context:'migration.add-custom-tool-intervention-field'}`
  TWICE on a real add (`:54-56` inside the `if`, `:61-65` the summary with `columnsAdded`, `durationMs`); ERROR
  `Failed to add customToolIntervention column` `{context, error}`; registered after `addCustomToolTriggersFieldMigration`,
  before `addComposerSlashCommandsFieldMigration` (`migrations/scripts/index.ts:921`). No data rewrite. Prettify label
  `Teaching Pascal to wait for your nod`.
- **generateDDL** renders from `ChatSettingsSchema` declaration order: `"customToolIntervention" INTEGER DEFAULT 1`
  between `"customToolTriggers" INTEGER DEFAULT 0` and `"compositionModeDefault" INTEGER DEFAULT 0` (DDL.md at the
  pin `:1143-1145` agrees). Fresh shape == migration clause byte-for-byte (`INTEGER DEFAULT 1`); only the POSITION
  differs — the `chat_settings_4_11_repair.rs` "ONE shape per column" rule holds.
- **Stamping:** NO stamp (R-B of P4.D304) — `shouldRun`'s column gate makes a re-ADD a no-op on either app; nothing
  to ping-pong. The ensure must be added to `restore_vintage_state.rs`'s boot-align (memory note).
- **Default ON** means a v4-provisioned row reads `1`; a v5 reader with the column absent (a pre-round v5
  instance before its first boot) must default `true` (the `chat_settings.rs:2392` pattern asserts the ABSENT-column
  default per column — the keystone adds the `true` twin).

## 4. Log lines (v4 text + keys + level → v5 home)

All v4 lines are `logger.<level>(message, {context, …})`; v5 captures them under `target: "quilltap::pascal"`
with the same `context` field, pinned per op by `global_capture` (memory note: `Db::write` closures are invisible
to `captured_with`).

| level | message | keys | v4 site | v5 home |
|---|---|---|---|---|
| INFO | `Custom tool run held for the operator` | `context:'pascal.intervention.registry', chatId, holdId, tool, entrance, characterId` | `registry.ts:139-146` | NEW `pascal/intervention/registry.rs` |
| WARN | `Custom tool hold resolve refused` | `context:'pascal.intervention.registry', holdId, outcome` | `registry.ts:162` | NEW registry |
| INFO | `Custom tool hold resolved` | `context, chatId, holdId, tool, decision, cause` (`cause` = veto's cause else `'dialog'`) | `registry.ts:169-176` | NEW registry |
| DEBUG | `Custom tool run not held` | `context:'pascal.run-service', chatId, tool, reason` (five reasons, §1.1) | `run-service.ts:311` | `run_service.rs` NEW |
| DEBUG | `Custom tool hold: chat settings unreadable, holding by default` | `context, chatId, error` | `run-service.ts:326` | `run_service.rs` NEW (reachable only through the connection failure arm — v4's `findByUserId` is a fallback read; the precedent `:231-247`) |
| INFO | `Custom tool run set aside by the operator` | `context, chatId, tool, invokedBy, cause` | `run-service.ts:375` | `run_service.rs` NEW |
| INFO | `Custom tool run dealt by the operator` | `context, chatId, tool, changed (array → `changedJson` per §R.5), reasks, staleAnswer` | `run-service.ts:389` | `run_service.rs` NEW |
| INFO | `run_custom set aside by the operator` | `context:'tools.run-custom', chatId, tool, transparent` | `run-custom-handler.ts:231` | `tools/run_custom.rs` NEW (`HANDLER_CONTEXT`) |
| DEBUG | `run_custom veto: caller transparency unreadable, treating as plain` | `context, chatId, error` | `run-custom-handler.ts:116` | `tools/run_custom.rs` NEW |
| DEBUG | `Triggered custom tool set aside by the operator` | `context, chatId, tool, phrase` | `trigger-runner.ts:213` | `trigger_runner.rs` NEW |
| DEBUG | `Custom tool holds listed` | `context:'api.v1.chats.custom-tools.interventions', chatId, holds` | `interventions.ts:224` | NEW `api/custom_tool_interventions.rs` |
| DEBUG | `Custom tool hold previewed` | `context, chatId, holdId, pinned (array), staleAnswer` | `interventions.ts:243` | NEW api |
| DEBUG | `Custom tool hold re-asked` | `context, chatId, holdId, ok, reasks` | `interventions.ts:288` | NEW api |
| WARN | `Custom tool hold re-ask failed` | `context, chatId, error` | `interventions.ts:302` | NEW api |
| WARN | `Custom tool hold resolve refused` | `context:'api.v1…interventions', chatId, holdId, status` | `interventions.ts:320` | NEW api (a SECOND line with the same text under a different context — both fire on a late resolve: the api's first, then NOT the registry's, since the api never calls `settleIntervention` on a missing hold) |
| WARN | `Custom tool LLM consult failed` | `context:'pascal.custom-tools', tool, reason` | `custom-tools.ts:2007` (unchanged; now also reached by Re-ask) | `custom_tools.rs` `resolve_llm_consult` (existing) |
| WARN | `[Settings v1] Rejected a PUT carrying unknown settings keys` | `userId, unknown` | `settings/chat/route.ts` (`3b8b6f42b`) | **P4.D356** |
| INFO ×2 / ERROR | the migration's three lines | — | migration script | NO-PORT (runner deferred; the keystone's ensure logs v5's ONE `quilltap::boot` INFO) |

Lines UNCHANGED but whose *reach* changes: `Custom tool dealt` DEBUG (`run_service.rs:357-375`) now fires AFTER a
hold with `announcedBy` from the presentation; `Custom tool wardrobe item reference rendered empty; dropped` /
`… inventory reference …` (`custom_tools.rs` gesture resolvers) fire for literal items too.

## 5. Harness / oracle impact

**Import-dead at the pin (this area): NONE.** Every Pascal case imports exports that still exist (`executeCustomTool`,
`QtapCustomToolSchema`, `buildCustomToolLibrary`, `runTriggeredCustomTools`, `executeRunCustomTool`, the route
modules). `turn-control.ts` (the oracle case) imports `runWithTurnControl` etc. — all present; its new optional
`options` argument is additive.

**REDDEN (behaviour / shape moved):**
- `pascal_custom_tools_execution_equivalence.rs` (696) — the oracle (`pascal-custom-tools-execution.test.ts:2160-2185`)
  emits `out` = the whole `CustomToolRunResult`: every `executeCustomTool` row gains `draw` (`{form:'range', unit}` —
  the unit is the 6-byte fraction, so it is REPRODUCIBLE from the scripted pool), `rollSpan`, `valueSpan`,
  `llmPromptRendered` on consult rows; AND `bytesConsumed` CHANGES on every degenerate `min === max` range (v4 now
  draws 6 bytes it discards — `rollRange:1037`). The family's header (`:5-11`) explicitly pins "a degenerate range
  short-circuits WITHOUT drawing" — that sentence is now FALSE at the pin; rewrite the doc and the `consumed()`
  assertion. v5's `roll_range` must draw the unit unconditionally. Regen the oracle at the pin (PIN REQUIRED).
- `pascal_custom_tools_route_equivalence.rs` (1060) — POST `?action=run`'s `{status, body}` with `body.result` = the
  whole result → `draw`/spans appear. GET unaffected.
- `pascal_workbench_route_equivalence.rs` (606) — `?action=preview` body spreads the result → `draw` / `rollSpan` /
  `valueSpan` (+ `llmPromptRendered` on consult rows). The corpus `workbench-route-cases.json` is read by both
  sides; no corpus change needed.
- `pascal_workbench_equivalence.rs` (248) — library entries gain `intervention: false` AND (P4.D343) `effect`.
  Contract `LibraryEntryKeys`.
- `pascal_custom_tool_definition_equivalence.rs` (379) + the SPA `custom-tool-types.corpus.spec.ts` — the corpus
  regenerated at the pin carries the unknown-keys tripwire rows; NEW rows to add in the generator
  (`harness/oracle/cases/pascal-custom-tool-definition.ts`, fenced): `intervention-enabled-bare`, `intervention-with-
  fields`, `intervention-empty-block` (refused), `intervention-unknown-field-key` (refused, the strict sentence),
  `intervention-enabled-null` (refused), `intervention-flags-inert` (accepted on a non-intervenable tool),
  `parameter-intervene-false` / `-string` (refused), `effect-intervene-false-on-each-kind` (write / wardrobe /
  inventory / log), `unknown-keys-intervention-known` (the 18th tripwire). The SPA spec header's row count /
  provenance paragraph is updated by whoever copies (R-F).
- `pascal_run_custom_handler_equivalence.rs` (1201), `pascal_trigger_runner_equivalence.rs` (424) — drive v4's REAL
  run service; the fixture tools are NOT intervenable, so `mayHoldRun` returns at `isIntervenable` (no settings
  read, no new lines) → should stay GREEN; verify by regen. NEW rows: a hold/veto cannot be driven through jest
  without a resolver (v4's own tests mock `hold`); the faithful tier-3 shape is a **scripted resolver** on both
  sides — the oracle case registers the hold, awaits `listInterventions`, posts the decision through the REAL
  `handleInterventionResolve`, and dumps the rows; v5 does the same through `chat_custom_tool_interventions_resolve`
  on a second task. Rows: `held-deal-changed-roll`, `held-as-rolled`, `held-veto-plain-caller`, `held-veto-
  transparent-caller`, `held-veto-user-seat`, `held-stop-vetoes` (the Stop arm — v5 side only unless the oracle
  aborts the ALS signal: it can, `runWithTurnControl` + `stopActiveTurns`), `trigger-held-veto-continues`.
- `settings_routes_equivalence.rs` (792) — P4.D356's rows (`customToolIntervention` arm + the unknown-key 400).
- `chat_settings_column_sites_guard.rs` (201), `chat_settings_tier2_equivalence.rs` (416), `chat_settings_4_11_
  columns_ensure_equivalence.rs` (182), `restore_vintage_state.rs` — the keystone's (the column).
- `almanack_render_equivalence.rs` (146) — the new `- **Hold Intervenable Rolls for the Operator**` line in the
  fixture's rendered report (re-record at the pin; whichever lane owns the Almanack trio).
- `turn_control_equivalence.rs` (475) — the registry rows replay `turn-control.test.ts`; `839575d89` adds NO test
  there (its +1 to `run-service.test.ts` and +11 to `trigger-runner.test.ts` are `mayHold` plumbing); `keepAlive` is
  not modelled → unchanged.
- `p4_6ay_workbench_wire_contract.rs` (174) — pins the four Workbench verbs name-for-name; NEW verbs are the CHAT
  route's, so add a sibling pin (`pascal_intervention_wire_contract.rs`, §6) rather than widen this one.
- `publish_sites.rs` census (`realtime/publish_sites.rs`) — TWO new publish points (`hold` add, `settle` removal)
  must be pinned by a capturing-subscriber test per the P4.D124 rule; the `realtime/types.rs:116-123` topic-list
  test grows.

**GARBLE risk:** none positional. The execution oracle's `overrides` field is emitted as given; a v5 reader that
re-keys `pins` must map `effects` indices from JSON string keys.

**NEW families (P4.D345):** `pascal_intervention_pure_equivalence.rs` (tier 1 — `interventionEligibility`,
`checkOverrides` sentences, `pinsFromOverrides`, `changedFields`, `factsOf`, `summarizeEffects`, `describeSpan` /
`withinSpan`, `transformSpan` through the engine, `diceFromFaces` clamps, the replay-draw invariant "the same unit
through a moved range"; oracle `harness/oracle/cases/pascal-intervention.ts` importing `@/lib/pascal/intervention/
{eligibility,overrides,recompute,spans}` + `executeCustomTool` with a scripted `draw`/`consult`/`pins` — v4's
`intervention-recompute.test.ts`'s 22 cases are the template), `pascal_intervention_route_equivalence.rs` (tier 2 —
the four actions over the `pascal-run-custom-{main,mount}.db` fixture with a definition PLANTED intervenable in the
Trigger Shelf store; v4's `custom-tools-interventions.test.ts`'s 26 cases are the template; the Re-ask row is
PROFILE-BEARING over the recorded canned completion, as `run-oracle-consult-resolved` is). The `intervention-leaks`
suite (v4, 10 cases) becomes a v5 unit test over `build_run_custom_description` / `collect_tool_vocabulary` / the
storyteller roster (the latter P4.D343's — the leak row for it lands when both exist).

**Committed fixtures:** `crates/quilltap-web/tests/fixtures/pascal-run-custom-{main,mount}.db` and the
`workbench-{main,mount}.db` pair carry NO `customToolIntervention` column; v5's tolerant SELECT reads the absent
column as the default (`chat_settings.rs:2392-2399` pattern — the keystone adds the `true` twin), and v4's jest
oracles open the copies through `initializeDatabase` (which runs v4's migrations on the copy — the P4.D304
precedent `migrate-pascal-run-custom-columns.ts` exists for when it does not). The route family needs ONE
intervenable definition planted in the Trigger Shelf store: an `extend-pascal-run-custom-intervention.ts` (the
`extend-pascal-run-custom-triggers.ts` precedent) that mints no ids a sibling family orders by.

## 6. SPA

**v4 component → v5 Angular home:**
- `CustomToolInterventionDialog.tsx` (777) → NEW `apps/web/src/app/chat/custom-tool-intervention-dialog.ts`
  (standalone, signals; `injectQuery` on `customToolsKeys.interventions(chatId)` with `RealtimeService.onTopic
  ('customToolInterventions')` invalidation + the 5 s offline poll via the existing refetch-interval idiom;
  the `HeldRun` child keyed by hold id so a fresh hold starts with empty pins; `PREVIEW_DEBOUNCE_MS = 300`;
  sections in v4's order: Parameters (reusing `custom-tool-params-form.ts` — v4 reuses `CustomToolParamsForm` with
  `layout="stacked"` and `coerceParamValues`), The roll (dice: one `<input type=number>` per face `aria-label="Die
  ${i+1}"`, `min=1`, `max=diceSides`; range: `NumberField` `Raw draw`), The value (`Value after modifiers`), The
  consult (`Prompt` textarea, `The oracle answered` checkbox, `Answer` textarea, `Re-ask`/`Asking…`, note `This
  answer was given to a different question.`), The outcome (the `<ol>` of rows `aria-pressed`, `summarizeWhen` —
  v4 reuses `COMPARATOR_LABELS` from `components/custom-tools/comparator-labels`; v5's twin is in
  `screens/custom-tools/comparator-labels.ts` — present), The oracle (compact), The message, The verdict (`State`
  select), Consequences (`Effect ${i+1} mode|value|items|log` controls), Presentation (`Heard by` `The whole table`
  / `A whisper`, `Announced by` `Pascal` / `${characterName ?? 'The character'}` only when `hasCharacter`, `Title`,
  `Chip label`), the warnings `<ul class="qt-intervention-note">`, the preview `As it will be announced — N
  change(s) pinned` with `data-testid="intervention-preview"` (`chipLabel ?? title`, ` · whispered`, ` · in
  ${name}'s voice`); footer `Set the table aside` → confirm `Sweep the cards away? Nothing is announced and nothing
  changes.` / `Keep the table` / `Set it aside`; `As rolled`; `Deal it` (disabled while a preview is pending);
  title `Pascal holds: ${dealt.title}`; the intro `${reached}. The cards are dealt but not yet shown — adjust what
  you like, then deal it. No one at the table will know you touched the deck.` with `reached` = `overheard: “${phrase}”`
  / `called by ${name}` / `called by a character`, and ` ${n} more waiting.`; error toasts `Pascal could not reckon
  that change`, `The oracle would not answer again`, `Pascal could not settle that roll`; the modal is NOT dismissable
  (no close / Escape / click-outside — the shared full-screen frame's `closeOnEscape=false` equivalent); the
  `Section` chrome `dealt: ${dealt} · reset`; `data-testid="custom-tool-intervention"`.
  Mount: v4 mounts it in `ChatModals.tsx` (per chat, always rendered, renders `null` with no hold) — v5's
  equivalent host is `screens/salon/salon-conversation.ts` (where `run-tool-modal` lives) or `chat-composer.ts:410`
  beside `<qt-custom-tools-popup>`; prefer the Salon conversation (the query needs only `chatId`).
- `InterventionToggle.tsx` → NEW `screens/custom-tools/intervention-toggle.ts` (`qt-intervene-toggle` /
  `--off`, `aria-pressed`, `aria-label`, `title="${label} (shown|hidden)"`, the `hand-raised` icon — PRESENT in
  v5's registry (`apps/web/src/app/ui/icon.ts`, already used by `chat/chat-composer.ts`; nothing NEW)).
- `BuilderForm.tsx` (+96) → `screens/custom-tools/builder-form.ts`: the master tick **Let me take a hand in each
  roll** (title attr `On: each run a character or a trigger phrase reaches waits for you before Pascal announces
  it. The model is never told. A character who can read this store can read the flag.`), the hint paragraph (`With
  this on, each run … can read the flag.`), the `fieldToggle` beside Title (`Show the title in the hold dialog`),
  Chip label, Results (`Show the visibility…`), The character announces it (`Show the announcer…`), each parameter
  (`Show ${name || 'this parameter'} in the hold dialog`), The roll (`Show the roll…`), `Final value after the
  multiplier, offset and rounding` (`Show the final value…`, range form only), The question (`Show the oracle’s
  question…` U+2019), Answer cap (`Show the oracle’s answer…`).
- `OutcomesSection.tsx` → the outcome-table card: `Show the winning outcome in the hold dialog`, the `Message` /
  `State` pair (`Show the rendered message…`, `Show the outcome state…`) shown only while intervening.
- `SideEffectsSection.tsx` → per-effect `Show this effect in the hold dialog` (`effect.intervene`).
- `WorkbenchLibrary.tsx` → `workbench-library.ts:231`: the `hand-raised` mark `qt-intervention-mark` with
  `title="Holds each roll for you"` beside the title when `tool.intervention`.
- `tool-draft.ts` → `apps/web/src/app/pascal/tool-draft.ts`: `DraftParameter.intervene?`, `DraftEffect.intervene?`,
  `ToolDraft.intervention: {enabled, authored, fields}`, `newDraft` seed, `effectToDraft` / `effectFromDraft` flag
  spread on all four kinds, `parameterToDraft` / `parameterFromDraft`, `draftFromDefinition`, `definitionFromDraft`
  (emits `intervention` AFTER `announceAsCharacter`, BEFORE `parameters`), `interventionFromDraft` (emits the block
  when enabled OR authored OR any hidden field OR any member flag; only `false` flags are written). v4's
  `intervention-format.test.ts` rows "round-trips every flag", "keeps a switched-off block", "writes no block for a
  new tool" become `tool-draft-grammar` corpus rows (P4.D346 regenerates THAT corpus from v4's real `tool-draft.ts`
  at the pin — its own, SPA-only).
- `AutomationSettings` / `types` / `useChatSettings` / `ChatTabContent` → `chat-settings.types.ts:138-170`
  (`AUTOMATION_OPTIONS` third entry `{ key: 'customToolIntervention', label: 'Hold intervenable rolls for me',
  description: 'A custom tool marked for intervention pauses for your say-so before Pascal announces it. A held
  roll waits for you indefinitely; switch this off and every roll goes ahead as dealt, which is how a chat may be
  left to run unattended. Requires Custom Tools.' }`, `AutomationKey` widened, `DEFAULT_CUSTOM_TOOL_INTERVENTION =
  true`), `automation-settings.ts:85-113` (the row `disabled: !customToolsOn`, save error text `Failed to update
  roll intervention setting`), `core-contract.ts:3953` (`customToolIntervention?: boolean`), `scalar-toggle-cards.spec.ts`.
- `system-message-labels.ts` `getInterventionSummary(message)` → `apps/web/src/app/chat/system-message-labels.ts`
  (NEW export; pure — an oracle case can import v4's `app/salon/[id]/components/system-message-labels.ts` directly
  as the existing `system-message-labels.oracle.spec.ts` family does — check its oracle's import; `announcement-
  sender-name.oracle.ndjson` is that family's corpus). Tooltip bytes: `Adjusted before it was dealt: ${parts.join(';
  ')}` with parts `${name} ${show(dealt)} → ${show(final)}`, `effect ${i+1} changed`, `dice [a, b] → [c, d]` /
  `roll ${a} → ${b}`, `value ${a} → ${b}`, `outcome row ${a+1} → ${b+1}`, `verdict ${a} → ${b}`, `message
  rewritten`, `the oracle's answer replaced`, `label "${a}" → "${b}"`, `title "${a}" → "${b}"`, `${v} → ${other}`,
  `announced by the character, not Pascal` / `announced by Pascal, not the character`; `show(n)` = 4-sig-digit
  floats. Returns `null` unless `systemSender === 'pascal'` and `changed.length > 0`.
- `AnnouncementChip.tsx` → `message-row.ts:157` region: `<span class="qt-intervention-badge" [title]="adjusted">`
  with the `hand-raised` icon `w-3 h-3` + `sr-only` text, between the kind label and the whisper tag.
- `app/salon/[id]/types.ts` → `core-contract.ts:3190-3210` `pascalMeta.intervention?` (the exact `dealt` type).
- CSS: NEW `apps/web/src/styles/qt-components/_intervention.css` from the PIN (classes `.qt-intervene-toggle`,
  `--off`, `.qt-intervention`, `-badge`, `-effect`, `-mark`, `-note`, `-queue`, `-reset`, `-row`, `-row--winning`,
  `-section`, `-section--pinned`) + the `_index.css` import.

**Component-harness specs touched:** `builder-form.spec.ts`, `workbench-editor.spec.ts`, `workbench-library.spec.ts`,
`tool-draft.spec.ts`, `tool-draft-grammar.oracle.spec.ts`, `custom-tool-types.corpus.spec.ts` (+ the topical
`custom-tool-types.*.spec.ts`), `scalar-toggle-cards.spec.ts`, `system-message-labels.spec.ts`, `message-row` specs,
`realtime-topic-map.spec.ts`, NEW `custom-tool-intervention-dialog.spec.ts` (v4's 19 dialog cases + the 11
`workbench-intervention` cases are the templates; the memory note `spa-component-harness-gotchas` — stream stub +
TanStack for `injectQuery`).

**Playwright:** NEW `apps/web/e2e/salon-intervention-flow.spec.ts` — (a) a seeded intervenable tool reached by a
trigger phrase opens the hold dialog, `As rolled` posts the dealt bubble; (b) `Deal it` with a pinned outcome row
posts the badge (hover text begins `Adjusted before it was dealt:`); (c) Stop while held posts nothing and the
dialog closes; (d) a reload with a hold pending reopens the dialog (server state); gated until P4.D345 lands (the
`salon-inform-flow` LIVE-flip precedent). `workbench-triggers-effects-flow.spec.ts` gains (g) the **Hold intervenable
rolls for me** row (ticked by default, dead with Custom Tools off, unticking persists) and `workbench-flow.spec.ts`
gains the hands (tick the master → a hand beside the title; click it → `aria-pressed=false`, saved file carries
`"fields": {"title": false}`). 💸 none of these spend a model call if the seeded tool has no `llm` block; a Re-ask
beat does.

**Wire contract — `InterventionWire` (P4.D345 builds, P4.D346 consumes), frozen names:**
- `Request::ChatCustomToolInterventionsList { chat_id }` → `Response::CustomToolInterventions(Value)` = v4's
  `{holds: InterventionHoldView[]}` (key order per `holdView`).
- `Request::ChatCustomToolInterventionPreview { chat_id, id, overrides: Value }` → `Response::CustomToolInterventionPreview
  (Value)` = `{preview}`; errors: 404 `Held run not found` (v4 `notFound('Held run')`), 400 with the
  `checkOverrides` sentence / Zod issues joined `; ` / `CustomToolRunError.message`.
- `Request::ChatCustomToolInterventionReask { chat_id, id, overrides }` → `{preview, consults}`; 400 `This tool
  consults no oracle`; 409 `That run has already been settled`.
- `Request::ChatCustomToolInterventionResolve { chat_id, id, decision: Value }` → `{resolved: 'deal'|'as-rolled'|
  'veto'}`; 404 / 409 / 400 as §1.1.
- HTTP: `GET /api/v1/chats/{id}/custom-tools?action=interventions`, `POST …?action=intervention-preview|
  intervention-reask|intervention-resolve` (`custom_tools_routes.rs` AVAILABLE lists); Tauri: the same `Request`s.
- SPA types in `custom-tools.api.ts` (or a new `custom-tool-interventions.api.ts`): `InterventionHoldView`,
  `InterventionPreview`, `InterventionRunView`, `InterventionEffectView`, `InterventionOverrides`, `EffectPin`,
  `RunSpan` — byte-named as v4's `view.ts` / `overrides.ts`; the dispatch verbs `'chatCustomToolInterventionsList'`,
  `'chatCustomToolInterventionPreview'`, `'chatCustomToolInterventionReask'`, `'chatCustomToolInterventionResolve'`
  in `core-contract.ts`.
- Event: `RealtimeTopic::CustomToolInterventions` (`"customToolInterventions"`), scoped by chat id; the SPA maps it
  to `[customToolsKeys.interventions(id)]` / unscoped `[customToolsKeys.all]` (NEW `all: ['customTools']`) and
  `ALL_REALTIME_PREFIXES += customToolsKeys.all`.
- `pascalMeta.intervention` on `chat_messages` rows (the `MessageEvent` type) — the badge's input.
- The Workbench library entry key `intervention: boolean` (after `triggerCount`, before P4.D343's `effect`).
- The chat-settings wire key `customToolIntervention?: boolean` (GET/PUT) — P4.D341 (row) + P4.D356 (PUT arm).
A `pascal_intervention_wire_contract.rs` guard (the `p4_6ay_workbench_wire_contract` shape) pins the verbs so
P4.D346 can author blind.

## 7. The lane split, by FILE OWNERSHIP

**Confirmed with amendments.** P4.D345 wave 1 (from the keystone tip — it needs `chat_settings.custom_tool_intervention`
and `PascalMetaIn` compiled); P4.D346 wave 0 EXCEPT its grammar-twin part (see the stacking note).

### P4.D345 — files it creates / edits
- NEW `crates/quilltap-core/src/pascal/intervention/{mod,registry,eligibility,overrides,recompute,veto_result,view,spans}.rs`;
  `pascal/mod.rs` (+1 `pub mod intervention;`).
- `pascal/custom_tool_types.rs` (the `intervene` fields + parse-key lists `:3093`, `:3773`; `CustomToolIntervention`
  struct + parser; `QtapCustomTool.intervention`; `KNOWN_TOP_LEVEL_KEYS` 18; the issue-path forms).
- `pascal/custom_tools.rs` (`RecordedDraw`, `RunSpan`, `EffectPin`, `RunPins`, the result's five keys,
  `crypto_unit`, `roll_range` always-draw + pins + spans, `transform_span`, `dice_span`, `dice_from_faces`,
  `resolve_effects(…, pins)`, the two gesture resolvers' `literal`, `pose_llm_consult`, `execute_custom_tool`'s
  three new inputs — **R-Q:** three MORE positionals or a `ReplayInputs<'_>` struct as the 13th; recommend the
  struct, and name it as a contract because P4.D343's `isComputedTool` removal / effects ceiling may touch
  `execute_custom_tool`'s CALLERS in `tools/` — verify P4.D343's survey; if it edits `custom_tools.rs` too, P4.D343
  must branch after P4.D345 or vice-versa).
- `pascal/run_service.rs` (`may_hold`, `Vetoed`, the hold block, presentation, the record), `pascal/trigger_runner.rs`
  (the `Vetoed` arm ONLY — P4.D343's ceiling hunk is above the call; disjoint lines), `pascal/workbench.rs`
  (`intervention` field — contract `LibraryEntryKeys`).
- `tools/run_custom.rs` (`may_hold`-less: the entrance passes `may_hold: true`; the veto arm; `caller_knows_it_is_a_game`
  with v4's two reads).
- NEW `api/custom_tool_interventions.rs`; `api/types.rs` (+4 `Request`, +4 `Response` variants, the `RealtimeTopic`
  is `realtime/types.rs`); `api/engine.rs` (+4 dispatch arms, the `ready_db_and_consult()` pattern `:5150`);
  `quilltap-web/src/custom_tools_routes.rs` (AVAILABLE lists + 3 POST arms + 1 GET arm); `realtime/types.rs` (+1
  variant — contract `RealtimeTopicAppend`), `realtime/job_topics.rs` (+1 `=> &[]`), `realtime/publish_sites.rs`
  (+2 pins).
- `db/chats_messages.rs` (`PascalMetaIn.intervention: Option<Value>` — ONE field; the keystone's `storytellerBid`
  edits are on `ChatMessageIn`/the row, not this struct).
- `tools/help.rs:216` (+1 insert) — **only if** P4.D356 does not claim `help.rs`; otherwise P4.D356's.
- `almanack/{types,phase3_ledgers,render}.rs` (the trio) — **assign to P4.D356** (it already owns the settings
  surface and its `settings_routes` / `almanack_render` re-records) unless P4.D351's "Almanack lines" claims the
  files; either way NOT P4.D345 (one owner for `render.rs` this round).
- `pascal/specimens/qtap-custom-tool.schema.json` (re-vendored WHOLE at the pin, 66,469 B) +
  `custom_tool_schema_embed_guard.rs:30` `SCHEMA_BYTES = 66_469` + `public_schemas_vendor_guard.rs:57` — see §9
  (the SPA copy is P4.D346's under the same bytes; the guard compares the two, so the lanes must land the SAME
  file — P4.D345 commits both copies, P4.D346 touches neither).
- Harness: `harness/oracle/cases/pascal-custom-tool-definition.ts` (+ rows, the ONE regen), NEW
  `harness/oracle/cases/pascal-intervention.ts` + `pascal-intervention-route.test.ts`, NEW
  `extend-pascal-run-custom-intervention.ts`; `crates/quilltap-harness/tests/pascal_custom_tools_execution_
  equivalence.rs` (doc + consumed assertion), NEW `pascal_intervention_pure_equivalence.rs`, `pascal_intervention_
  route_equivalence.rs`, `pascal_intervention_wire_contract.rs`; re-records of the execution / route / workbench /
  workbench-route oracles at the pin.
- `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson` (the byte-equal copy — see stacking).

### P4.D346 — files
- NEW `apps/web/src/app/chat/custom-tool-intervention-dialog.ts` + `.spec.ts`; `chat/custom-tools.api.ts` (keys +
  types) or NEW `chat/custom-tool-interventions.api.ts`; `core/core-contract.ts` (the 4 verbs, `pascalMeta.intervention`,
  `CustomToolLibraryEntry.intervention`, `customToolIntervention?`, `CustomToolRunResult` + `BenchRoll`'s new keys —
  contract `LibraryEntryKeys` shares this FILE with P4.D344's storyteller types: `core-contract.ts` is edited by
  nearly every SPA lane; the unifier merges by region, as every round).
- `core/realtime.types.ts`, `core/realtime-topic-map.ts`, `core/realtime-topic-map.spec.ts` (contract
  `RealtimeTopicAppend` with P4.D344).
- `screens/salon/salon-conversation.ts` (the mount — one element; shared with P4.D344's board mount and P4.D359's
  console: region merge).
- `chat/system-message-labels.ts` (+`getInterventionSummary`), `chat/message-row.ts` (the badge), their specs.
- `pascal/tool-draft.ts` + `tool-draft.spec.ts` + `tool-draft-grammar.oracle.spec.ts` + the `tool-draft-grammar`
  NDJSON (its own regen from v4's `lib/pascal/tool-draft.ts` at the pin).
- `pascal/custom-tool-types.ts` (the SPA grammar twin: `intervene` ×2, `CustomToolIntervention`, `intervention`,
  `KNOWN_TOP_LEVEL_KEYS` 18) + `custom-tool-types.corpus.spec.ts` header — **STACK this part on P4.D345's Tier-1
  commit** (the corpus copy lands with P4.D345; the SPA spec replays it). If the planner keeps P4.D346 strictly
  wave 0, P4.D346 authors the twin against v4's Zod and the corpus rows by NAME and the unifier runs
  `custom-tool-types.corpus.spec.ts` on the union — the P4.D333 R-F precedent.
- `screens/custom-tools/{builder-form,workbench-editor,workbench-library}.ts` + specs; NEW
  `screens/custom-tools/intervention-toggle.ts`; `screens/settings/chat/{chat-settings.types,automation-settings}.ts`
  + `scalar-toggle-cards.spec.ts`.
- NEW `apps/web/src/styles/qt-components/_intervention.css` (+ the `_index.css` import — contract `CssIndexImports`
  with P4.D344 `_storyteller.css` and P4.D357: each lane appends ONE `@import` line; the unifier orders them as v4's
  `_index.css` at the pin).
- NEW `apps/web/e2e/salon-intervention-flow.spec.ts` (gated); `workbench-flow.spec.ts`,
  `workbench-triggers-effects-flow.spec.ts` (+1 test each).
- (The `hand-raised` icon already exists in `ui/icon.ts` — no icon work.)

### Named shared contracts
- **`TurnStopReach`** (P4.D348 builds, P4.D345 consumes): `services/tool_execution.rs` `ToolExecutionContext.turn_control:
  Option<TurnControlHandle>` (default `None`), filled by `orchestrator.rs:786 turn_tool_context` from
  `deps.turn_control` (`:643`), and by the finalizer's reply-trigger site + `trigger_phrase_sites.rs` params
  (`ReplyTriggerParams` / `UserMessageTriggerParams` gain `turn_control: Option<&TurnControlHandle>`) — v4 reads the
  ALS, so BOTH trigger sites and the tool loop see the same signal. P4.D345 then threads
  `RunCustomToolContext.turn_control: Option<TurnControlHandle>` (`tools/run_custom.rs:662`, built at
  `tools/executor.rs:731`) → `RunCustomToolParams.stop: Option<StopSignal>` (`run_service.rs:84`) and awaits
  `tokio::select! { d = decision_rx => …, _ = stop.wait() => Vetoed(cause: 'stop') }`. If P4.D348 cannot land the
  field in time, P4.D345 ships with `stop: None` everywhere (Stop does NOT veto — a loud `tracing::warn!` once per
  hold, recorded as a HANDOFF) — never a silent stub.
- **`LibraryEntryKeys`** (P4.D343 + P4.D345 on `pascal/workbench.rs`; P4.D344 + P4.D346 on `core-contract.ts`):
  v4 key order `…, llm, triggerCount, intervention, effect, parameterCount, …` (`workbench.ts:40-56` at the pin —
  `triggerCount` `:47`, `intervention` `:49`, `effect` `:54`, `parameterCount` `:55`). Each lane inserts ONLY its key; the `pascal_workbench_equivalence` oracle
  is regenerated ONCE at the pin by P4.D345 (it carries both keys; P4.D343's lane sees `effect` red until union —
  record as a union-expected red, or P4.D343 branches from P4.D345's tip).
- **`RealtimeTopicAppend`** (P4.D343/D344 then P4.D345/D346): final order `…, inventory, storytellerRounds,
  customToolInterventions` in `realtime/types.rs` (+ its `:116-123` list) and `realtime.types.ts`; the spec's
  "last topic" assertion goes positional only at union.
- **`SettingsArmOwner`** (P4.D356 builds, P4.D345/D346 consume): the `customToolIntervention` PUT arm + ACCEPTED
  key + `settings_routes` rows + the Almanack trio + `help.rs:216` + `almanack_render` re-record are P4.D356's;
  P4.D346's Automation row posts `{customToolIntervention: value}` and reads `settings.customToolIntervention`.
- **`KeystoneColumn`** (P4.D341 builds): `ChatSettingsRow.custom_tool_intervention: bool` (default `true` when the
  column is absent), the seed, the ensure (no stamp), the vintage mirror, `fresh_schema.json`.
- **`CssIndexImports`** (P4.D344 / D346 / D357): one `@import` line each in `_index.css`.
- **`ExportSchemaVendor`** (§9): ONE lane re-vendors `generators/qtap-export.schema.json` at the pin.

### Stacking
P4.D345 branches from `KEYSTONE` (needs the column + `PascalMetaIn` compile; `db/chats_messages.rs` is edited by
both — different structs). P4.D346's grammar part (`custom-tool-types.ts` + the corpus copy) stacks on P4.D345's
Tier-1 commit; the rest of P4.D346 may start from main. P4.D345's Stop veto depends on P4.D348's `TurnStopReach`
— P4.D348 is wave 0; P4.D345 (wave 1) can branch from a point that includes P4.D348's Tier-1 IF the planner
sequences it, else the HANDOFF above.

## 8. Tiering

**P4.D345 — Tier 1 (must land):** the grammar twin (Rust) + the ONE corpus regen + the schema re-vendor; the engine
(draw record, pins, spans, replay, `pose_llm_consult`, `dice_from_faces`, the always-draw change with the
execution family rewritten and GREEN at the pin); the hold registry (first-wins, settled memory 200, publish on
add/remove, NO timeout); eligibility / overrides / recompute / facts / changedFields with the pure family; the run
service hold block (`may_hold`, every `not held` reason, the veto / as-rolled / deal arms, the record, the
presentation-driven title + announcer); the two entrances' arms + the veto texts by transparency; the four API
actions with v4's status codes and sentences (incl. `b02d902f3`'s shape); the realtime topic + publish pins; the
`PascalMetaIn` field; the workbench `intervention` key; the route / handler / trigger / execution / workbench
re-records; Tier R rows for the new verbs.
**Tier 2 (should land):** the Stop veto via `TurnStopReach` (Tier 1 if P4.D348's field is in the branch base);
the `intervention-leaks` unit rows; the tier-3 scripted-resolver rows over the real fixture; the dogfood walk
doc rows.
**Tier 3 (loud typed refusals):** NONE acceptable as stubs. If the Re-ask consult runner is absent on an
assembly (read-only / canned), the action answers v4's fail-soft consult (`ok:false`, the author's `errorMessage`)
— that IS v4's behaviour through `buildCustomToolLlmInvoker`, not a refusal. The keep-alive is a RECORDED
divergence (R-1), not a refusal.

**P4.D346 — Tier 1:** the hold dialog whole (all sections, the three exits, the confirm, the queue count, the
reopen-on-reload via the query), the Automation row, the Workbench master tick + the field hands + the per-member
hands + the library mark, the draft twin + its corpus, the SPA grammar twin (stacked), the badge + tooltip, the
realtime map, the component specs, the two Workbench e2e beats LIVE, the Salon e2e beat GATED until P4.D345 lands.
**Tier 2:** the Salon beat LIVE on the union; the `hidden-consult` compact section beat; the spans hints beat.
**Tier 3:** none.

## 9. Fixtures

- **The definition corpus** (`harness/oracle/cases/pascal-custom-tool-definition.ts` → NDJSON; SPA copy
  `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`, 716 lines today): P4.D345 regenerates
  ONCE at the pin (new fenced rows named in §5), copies byte-equal to the SPA, records `wc -l` / `md5` in its
  lane record; P4.D346's spec header paragraph is updated by P4.D346 on the stacked commit. Sibling lanes
  (P4.D343's effects? P4.D350's inventory?) that add grammar rows this round must hand their rows to P4.D345's
  generator BEFORE the regen (the R-11 fence) — the planner names P4.D345 the round's corpus owner.
- **`qtap-custom-tool.schema.json`**: ONE commit touches it in the span (`839575d89`), so P4.D345 re-vendors all
  three v5 copies (`pascal/specimens/`, `apps/web/public/schemas/`, the `docs/v4/` mirror is P4.D360's — the embed
  guard compares against the mirror too (`custom_tool_schema.rs:18-19`): coordinate the mirror bytes with P4.D360
  or let the guard's mirror leg go red until union) and bumps the two guard constants to `66_469`.
- **`qtap-export.schema.json`** (115,274 B at the pin; seven span commits): ONE re-vendor into
  `generators/qtap-export.schema.json` + its guard — assign to the KEYSTONE (whose `storytellerBid` / keystone /
  `turnMode` carriers are the bulk) or P4.D360; P4.D345 does not touch it; its `pascalMeta.intervention` bytes ride
  that copy.
- **`tool-draft-grammar.oracle.ndjson`** (244 rows): P4.D346 regenerates from v4's real `tool-draft.ts` at the pin
  (SPA-only; the P4.D334 recipe).
- **`pascal-run-custom-{main,mount}.db`**: NO rebuild; an `extend-pascal-run-custom-intervention.ts` plants one
  intervenable definition (range form with `$param` bounds, one `llm` block, one write effect + one log effect,
  `fields: {chipLabel: false}` and one `intervene: false` parameter — enough to drive every refusal sentence) in the
  Trigger Shelf store; dependent oracles (route, handler, trigger-runner, the new route family) regenerate at the
  pin by P4.D345. The `workbench-{main,mount}.db` pair: NO change; the library oracle re-records.
- **`chat-settings-tier2.json` / `settings` fixtures**: the keystone's (`customToolIntervention` column) and
  P4.D356's (PUT rows).
- **`almanack-render`** oracle: re-recorded by the Almanack trio's owner (P4.D356 per §7).

## 10. Rulings owed to the human

- **R-1 — `HOLD_KEEP_ALIVE_NO_STREAM`:** v4's `keepAlive` ping (`KEEP_ALIVE_MS = 15_000`, `turn-control.ts:51`,
  `turn-stream.ts:60`) has no v5 counterpart: the Salon send is an awaited JSON dispatch (`spine.rs:2542-2585`),
  the Event channel pings itself (`events.rs:33`). Record as a deliberate divergence (the SPA's send `fetch` has no
  timeout; Tauri `invoke` has none) — or require P4.D348 to add a dispatch-level keep-alive. Recommendation: the
  divergence, with the dogfood row that measures a 10-minute hold over HTTP (Docker) and Tauri.
- **R-2 — `execute_custom_tool`'s signature:** three more positionals (R-Q's "LAST positional" rule, now 15) vs a
  `ReplayInputs { draw, consult, pins }` struct. Recommendation: the struct (one `Option<&ReplayInputs>` as the 13th).
- **R-3 — the always-draw change:** v4 at the pin draws 6 bytes on a degenerate range it never did before; v5 must
  follow (the execution family pins `bytesConsumed`). No divergence — but the family's header sentence is
  rewritten; the human should know the byte-stream fact flipped.
- **R-4 — the Stop veto's home:** `TurnStopReach` adds `turn_control` to `ToolExecutionContext` (P4.D348's file).
  Approve the field, or approve the HANDOFF (Stop does not veto a held run in this round, loud WARN).
- **R-5 — the Almanack trio + `help.rs` + the settings arm owner:** P4.D356 (recommended) vs P4.D345.
- **R-6 — `resolve_asker_flag` reuse vs v4's own two reads** in the veto transparency check: v4's handler does its
  own `chats.findById` + `characters.findByIdRaw`; porting those two reads (fallback semantics, DEBUG on a
  connection failure) is the faithful choice — recommended; no visibility change in `carina_query.rs`.
- **R-7 — `pascalMeta.intervention` as an opaque `Value`** in `PascalMetaIn` (v4's Zod shape is a non-strict
  `z.object` — a typed twin would REFUSE rows a newer v4 writes). Recommended opaque.
- **R-8 — the realtime invalidation's reach:** v4 ALSO invalidates `queryKeys.customTools.all` on reconnect
  (`ALL_REALTIME_PREFIXES`); v5's popup reads its roster fresh per open — adding the prefix is harmless; port it.
- **NO-PORT candidates (vendor lane ratifies):** `332c650b9` (spec); the migration's INFO/ERROR lines + prettify
  label (the deferred runner); `packages/theme-storybook` CSS mirror (v5 has none — the ledger's standing
  NO-PORT); the `.claude/commands/update-documentation.md` +1; `README.md` version stamps.

## 11. Dogfood rows the lanes should queue

1. Workbench: open a seeded tool, tick **Let me take a hand in each roll**, hide the chip label by its hand, save;
   the file carries `"intervention": {"enabled": true, "fields": {"chipLabel": false}}` and the library shows the
   hand. Untick the master; the file keeps the block with `"enabled": false`.
2. Salon (trigger phrase, no `llm`): say the phrase → the dialog opens `Pascal holds: <title>` with `overheard:
   “…”`; `As rolled` posts the bubble unchanged; no badge.
3. Salon (model `run_custom`) 💸: a character calls the intervenable tool → the dialog; pin the outcome row → the
   warning `The table would have dealt row …` (revealOdds on); `Deal it` → the bubble carries the hand badge whose
   hover begins `Adjusted before it was dealt: outcome row 2 → 1`; the model's tool result is the FINAL run only.
4. Salon veto, plain character 💸: `Set the table aside` → `Set it aside`; nothing posted; the model's next turn
   shows it received `The tool did not run.` (LLM Inspector); a `systemTransparency` character receives `The table is
   closed for the moment; …`.
5. Stop while held: press Stop → the dialog closes, nothing posted, the turn ends (`settled: true` without the 15 s
   wait) — ONLY if R-4 landed; else record the HANDOFF's WARN.
6. Reload with a hold pending: the dialog reappears (server state); a second tab shows the same hold; resolving in
   one makes the other's resolve answer 409 `That run has already been settled` (toast `Pascal could not settle
   that roll`, then the dialog moves on).
7. Two holds queued (two trigger phrases in one message): ` 1 more waiting.`, oldest first.
8. Re-ask 💸: a consult-bearing tool; pin a parameter the prompt quotes → `This answer was given to a different
   question.`; `Re-ask` spends ONE call (an `llm_logs` `CUSTOM_TOOL_CONSULT` row), the stale note clears,
   `reasks: 1` in `pascalMeta.intervention` after `Deal it`.
9. Hidden consult 💸: `fields: {llmPrompt: false, llmAnswer: false}` → the compact **The oracle** section; a pinned
   `llmPrompt` POSTed by hand → 400 `the llmPrompt is not open to intervention`.
10. Spans: a `$param`-bounded range → `The table draws 0 up to (not including) 10.`; nudge the bound → the hint
    moves; pin a roll outside → the warning; dice `3d6+2 can come to 5 to 20, each die 1 to 6`.
11. Settings: untick **Hold intervenable rolls for me** → the same trigger deals as rolled with no dialog; the
    Almanack line reads `No`; `help_settings` returns `customToolIntervention: false`; re-tick.
12. Unattended hazard (R-1 measurement): hold a run for 10 minutes over HTTP (Docker) and in Tauri; the POST must
    survive and the bubble must post on `Deal it`.
13. Autonomous room with an intervenable tool: deals as rolled (`an autonomous room` in the DEBUG log); the job
    child path likewise.
14. `.qtap` export / backup → restore of a chat carrying `pascalMeta.intervention`: the record survives byte-equal
    (the carriers pass the blob); the import of a definition file carrying `intervention` loads (no `Unrecognized
    key`).
