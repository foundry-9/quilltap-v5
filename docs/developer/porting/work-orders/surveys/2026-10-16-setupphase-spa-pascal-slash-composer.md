# Survey — the SPA side of the `25b8fab6a` round: Pascal's trigger phrases + wardrobe effects (Workbench + chat settings), the composer's `/` slash commands, the bug-251 trim, and the four smalls

Read-only planning survey, **2026-10-16**. v4 = `~/source/quilltap-server`
read ONLY through `git show 25b8fab6a:<path>` (**the PIN** = v4 main HEAD
`25b8fab6aa58636c6885ba63684c7f8fbfca9c3b`, `4.11.0-dev.10`, clean) and
`git show <sha> -- <paths>` / `git diff 83334b9a5 25b8fab6a -- <paths>` (the
span). **The oracle baseline v5 has ported is `83334b9a5`**; v5 = `main` at
**`56554d98c`** (`/Volumes/External 1/source_overflow/quilltap-v5`;
`~/source/quilltap-v5` symlinks to it). **Every v4 line number below is AT THE
PIN**; every v5 line number is at `56554d98c`. Nothing here is taken from commit
prose (ledger §5.3): every claim is from a shipped hunk. The ledger's §3 rows
`362426c8f` (`:316`), `f0db8cbb8` (`:317`), `4877fc2da` (`:319`), `b2793400f`
(`:322`), `25b8fab6a` (`:323`), `a42c282b8` (`:300`), `8489d08b9` (`:304`),
`01f508d4e` (`:299`) were read first and are built on, not restated; where a
hunk disagrees with a row this survey says so (§9).

**Scope.** The whole SPA side of the eight commits, to be cut into TWO lanes
with ZERO shared files: **LANE P** (Pascal SPA — the Workbench's trigger-phrase
field + the Side-effects card's wardrobe Kind + the Proving Bench's "would
attempt" + the library badge + the run dialog's wardrobe sentences + the
`pascal/*` TS twins and their corpora + the Automation card's
`customToolTriggers` toggle) and **LANE C** (the composer — the `/` slash
commands end to end: the pure tier, the ProseMirror typeahead, the runner, the
run-tool / custom-tool dialogs' `initialTool`, the RNG request extraction, the
composer settings card; plus the bug-251 code-block exit trim and the three
non-Pascal smalls). **Out of scope** (other surveys): every server-side hunk —
`lib/pascal/{run-service,trigger-runner,trigger-phrases,wardrobe-effects,
side-effects,custom-tools,tool-vocabulary}.ts`, the two migrations + D23
re-dump, the chat-settings PUT arms, the orchestrator / finalizer sites,
`help/` (→ `p4.9i2`), `docs/v4/` (→ the docs lane), `public/schemas/*` (noted
in §3 as a hand-off decision), the Rust `custom_tool_types.rs` keystone and
the `pascal_*_equivalence` families. Where the SPA READS a wire shape those
lanes emit, §7 names the shape and the gating flag.

---

## 0. The span for this slice, in one paragraph

Eight commits past the `83334b9a5` pin touch the SPA. **`362426c8f`** (trigger
phrases): a NEW `TriggerPhrasesField.tsx` (110) mounted by `BuilderForm.tsx`
at the end of the identity section, a `✦ N phrase(s)` badge on
`WorkbenchLibrary.tsx`, `AutomationSettings.tsx` rewritten as a row table over
`AUTOMATION_OPTIONS` with a second row (`customToolTriggers`, dead while
`customTools` is off), `types.ts` / `useChatSettings.ts` / `ChatTabContent.tsx`
plumbing, and the pure twins `custom-tool.types.ts` (+51: `triggers` +
`validateTriggers`) and `tool-draft.ts` (+59: `ToolDraft.triggers`,
`validateDraftTriggers`, the self-echo warning) — the SPA imports
`foldTriggerText` / `triggersContainedIn` from the NEW `lib/pascal/
trigger-phrases.ts`. **`f0db8cbb8`** (wardrobe effects): `custom-tool.types.ts`
+233 (the write ∣ wardrobe effect union, `isWriteEffect` / `isWardrobeEffect`,
`wardrobeGestureProblems`, `validateWardrobeGesture`, two caps),
`tool-draft.ts` +159 (`DraftEffect.kind` + the always-present `wardrobe` half,
`newDraftEffect`, `wardrobeFromDraft`, `validateDraftWardrobeGesture`, the
rename / references arms), `SideEffectsSection.tsx` +215 (the Kind select and
`WardrobeFields`), `ProvingBench.tsx` +24 (`describeBenchGesture`, "would
attempt"), `CustomToolRunDialog.tsx` +31 (three `references` keys and three
sentences). **`4877fc2da`** (slash commands): `lib/slash-commands/` (408 lines
pure + a 172-line test), `SlashCommandPlugin.tsx` (288) on the shared typeahead
shell, `slash-commands/context.ts` (67), `useSlashCommandRunner.ts` (79),
`rng-request.ts` (73) shared by `RngDropdown` and the quick rolls, `initialTool`
on `RunToolModal` and `CustomToolRunDialog`, the dialog moved into `ChatModals`,
`useModalState`'s two request records, `ComposerSlashCommandsSettings.tsx`
(30) + the "Slash Commands" card, `queryKeys.tools.byChat`, the ESLint
restriction widened, four jsdom tests. **`b2793400f`**: Lexical 0.52 (NO-PORT)
+ bug 251's `$codeBlockToEscape` (NO-PORT half) and the exit TRIM narrowing
(PORT). **`25b8fab6a`**: bug 250's `GuardedTypeaheadMenuPlugin` (NO-PORT).
**`a42c282b8`**: one class token on `memory-card.tsx`. **`8489d08b9`**: four
About blurbs. **`01f508d4e`**: one `console.debug` removed from
`useWardrobeItemActions.ts`.

---

## 1. v4 AT THE PIN — every hunk in the slice, file by file, per commit

### 1.1 `362426c8f` — trigger phrases

#### 1.1.1 `components/custom-tools/TriggerPhrasesField.tsx` (NEW, 110 lines)

- Props `:19-24`: `triggers: string[]`, `issues: DraftIssue[]`,
  `onChange: (next: string[]) => void`, `disabled?: boolean` (default `false`).
- State `:27`: `pending` (the input's text).
- Issue plumbing `:29-33`: `triggerErrors` = issues with `severity === 'error'`
  and `where.section === 'triggers'`; `errorAt(index)` = the first error whose
  `where.index === index`, else null; `listError` = the first error with
  `where.index === undefined`.
- Derived `:35-37`: `candidate = pending.trim()`; `atCap = triggers.length >=
  MAX_TRIGGERS` (8); `canAdd = !disabled && !atCap && candidate.length >=
  MIN_TRIGGER_PHRASE_LENGTH` (3).
- `add()` `:39-43`: if `!canAdd` return; `onChange([...triggers, candidate])`
  (the TRIMMED text — so a padded phrase can never be entered from the field);
  clears `pending`.
- Markup: `<label htmlFor="wb-trigger-phrase" class="qt-label">Trigger
  phrases</label>` `:47-49`; when `triggers.length > 0` a `<ul class="flex
  flex-wrap gap-2 mb-2" aria-label="Trigger phrases">` `:51` whose `<li
  key={`${index}-${phrase}`}>` carries `qt-badge-secondary inline-flex
  items-center gap-1` + `qt-text-destructive` when that index has an error,
  `title={error ?? undefined}` `:55-59`; inside, `<span>{phrase}</span>` and a
  `<button type="button" class="qt-button qt-button-ghost qt-button-sm px-1"
  aria-label={`Remove trigger phrase ${phrase}`}>×</button>` `:61-69` that
  calls `onChange(triggers.filter((_, i) => i !== index))`, disabled by
  `disabled`.
- The input row `:75-95`: `<input id="wb-trigger-phrase" type="text"
  maxLength={MAX_TRIGGER_PHRASE_LENGTH}` (80) with `onKeyDown` Enter →
  `preventDefault()` + `add()` `:82-87`; `placeholder={atCap ? `At most
  ${MAX_TRIGGERS} phrases` : 'abracadabra'}` `:88`; `disabled={disabled ||
  atCap}` `:89`; class `qt-input w-full`; then `<button type="button"
  class="qt-button qt-button-secondary qt-button-sm" onClick={add}
  disabled={!canAdd}>Add</button>` `:92-94`.
- Hint `:96-100` (`qt-hint`): **"Said by anyone in the chat, these words run
  the tool with its defaults — only when the chat setting is on. Exact words,
  any case. 3–80 characters each, up to 8."** (the three numbers are the
  constants, interpolated; note the `{' '}` before `{MAX_TRIGGERS}`).
- Error lines `:101-107`: `[listError, ...triggers.map((_, i) => errorAt(i))]`
  filtered non-null, each a `<p key={message} class="text-xs
  qt-text-destructive mt-1">` — so a per-index error renders BOTH as the chip's
  title and as a line below.

#### 1.1.2 `components/custom-tools/BuilderForm.tsx` (+8)

Import `:49`; mounted `:344-349` INSIDE the identity `<section>` ("The
contrivance itself"), AFTER the `Results` (default visibility) `<select>`'s
closing `</div>` and BEFORE `</section>`, as
`<TriggerPhrasesField triggers={draft.triggers} issues={issues}
onChange={(triggers) => update({ triggers })} disabled={disabled} />`.

#### 1.1.3 `components/custom-tools/WorkbenchLibrary.tsx` (+8)

`:241-248`, between the `oracle` badge and the `N param(s)` badge:
`{tool.triggerCount > 0 && (<span className="qt-badge qt-badge-outline"
title="Listens for trigger phrases — said in a chat with the setting on, they
run this tool">✦ {tool.triggerCount} phrase{tool.triggerCount === 1 ? '' :
's'}</span>)}`. Reads the NEW library wire key `triggerCount` (`lib/pascal/
workbench.ts:42-43` `CustomToolLibraryEntry.triggerCount: number`, computed
`:194` `entry.definition.triggers?.length ?? 0`).

#### 1.1.4 `components/settings/chat-settings/AutomationSettings.tsx` (rewritten; 82 lines at the pin)

- Props `:13-18`: `settings: ChatSettings`, `saving: boolean`,
  `onAutoDetectRngChange`, **`onCustomToolTriggersChange: (value: boolean) =>
  Promise<void>`**.
- `AutomationRow` `:21-25` `{ value: boolean; onChange; disabled?: boolean }`.
- `:38` `customToolsOn = settings.customTools ?? DEFAULT_CUSTOM_TOOLS` (true).
- `rows: Record<AutomationKey, AutomationRow>` `:40-50`: `autoDetectRng` →
  `settings.autoDetectRng ?? DEFAULT_AUTO_DETECT_RNG` (true);
  `customToolTriggers` → `settings.customToolTriggers ??
  DEFAULT_CUSTOM_TOOL_TRIGGERS` (false), **`disabled: !customToolsOn`**.
- Render `:53-80`: `<SettingsCard title="Automation" subtitle="Configure
  automatic behavior in chats">`; per option `disabled = saving ||
  row.disabled === true` `:57`; the `<label>` class is `flex items-start gap-3
  p-4 border qt-border-default rounded transition-colors` + (`row.disabled` ?
  **`opacity-50 cursor-not-allowed`** : `qt-hover-accent cursor-pointer`)
  `:61-63`; the checkbox `checked={row.value}` `onChange={(e) => { void
  row.onChange(e.target.checked) }}` `disabled={disabled}` `:65-71`; label /
  description `:72-75`. (Pre-commit: `checked={option.key === 'autoDetectRng'
  ? autoDetectRng : false}` and a hard-coded `qt-hover-accent cursor-pointer`
  class — the shape v5 carries, §2.6.)

#### 1.1.5 `components/settings/chat-settings/types.ts` (+15 here; +2 more in 1.3.13)

- `ChatSettings.customToolTriggers?: boolean` `:100-101` after `customTools`
  (`:98-99`), doc **"Whether a custom tool's trigger phrases, said by anyone in
  a chat, run it automatically with its defaults"**.
- `AUTOMATION_OPTIONS` `:500-511` gains `{ key: 'customToolTriggers', label:
  'Auto-Run Custom Tools on Trigger Phrases', description: "When anyone in a
  chat says a phrase one of Pascal's custom tools declares, run that tool
  automatically with its defaults. Requires Custom Tools." }` (the first row's
  bytes unchanged: `Auto-Detect RNG Calls` / `Automatically detect dice rolls
  (e.g., 2d6), coin flips, and "spin the bottle" in your messages and execute
  them`).
- `export type AutomationKey = (typeof AUTOMATION_OPTIONS)[number]['key']`
  `:513-514`.
- `export const DEFAULT_CUSTOM_TOOL_TRIGGERS = false` `:526-529`, doc
  **"Default trigger-phrase setting — off: overhearing is a surprise the first
  time"**.

#### 1.1.6 `components/settings/chat-settings/hooks/useChatSettings.ts` (+18)

`UseChatSettingsReturn.handleCustomToolTriggersChange` `:64`; the handler
`:368-382`: `patchChatSettings({ customToolTriggers: value }, 'Failed to update
trigger phrase setting', 'Failed to update trigger phrase setting')`; returned
`:773`.

#### 1.1.7 `components/settings/tabs/ChatTabContent.tsx` (+2)

Destructures the handler `:55`; passes `onCustomToolTriggersChange=
{handleCustomToolTriggersChange}` to `<AutomationSettings>` `:183`. The
Automation card itself (`:178-185`) keeps `title="Automation"
description="Configure automatic detection features" sectionId="automation"`.

#### 1.1.8 `lib/pascal/custom-tool.types.ts` (+51) — the SPA twin's half

- `:85-93`: `MAX_TRIGGERS = 8`; `MIN_TRIGGER_PHRASE_LENGTH = 3` ("keeps `"a"`
  and `"ok"` out. A length, not a word count"); `MAX_TRIGGER_PHRASE_LENGTH =
  80`.
- `QtapCustomToolSchema.triggers` `:855-864`: `z.array(z.string().min(3).max(80))
  .min(1).max(8).optional()`, placed AFTER `chipLabel` and BEFORE
  `description`; description "Phrases that run this tool automatically, with
  every parameter at its default, when a message from anyone in the chat
  contains one — exact substrings, case-insensitive, no wildcards. Only
  honoured when the chat setting "Auto-Run Custom Tools on Trigger Phrases" is
  on (it is off by default)."
- `superRefine` gains `validateTriggers(tool, ctx)` `:910` (fourth, after
  `validateEffects`).
- `validateTriggers` `:1001-1024`: per phrase, `phrase !== phrase.trim()` →
  `trigger ${i+1} has leading or trailing whitespace` at `['triggers', i]`;
  fold `toLocaleLowerCase`, a repeat → `trigger ${i+1} repeats trigger
  ${earlier+1} (phrases match regardless of case)` at `['triggers', i]`
  (first occurrence wins the map).
- `KNOWN_TOP_LEVEL_KEYS` gains `'triggers'` (v4's own test `:745-747`).

#### 1.1.9 `lib/pascal/tool-draft.ts` (+59)

- Imports `:27-29` (the three caps) and `:64` `foldTriggerText,
  triggersContainedIn` from `./trigger-phrases`.
- `ToolDraft.triggers: string[]` `:219-220` ("Empty = no `triggers` key
  emitted"); `newDraft()` `:278` `triggers: []`; `draftFromDefinition` `:613`
  `triggers: [...(definition.triggers ?? [])]`; `definitionFromDraft` `:812`
  `if (draft.triggers.length > 0) doc.triggers = [...draft.triggers]` —
  emitted right after `chipLabel`, before `description` (v4's test `:931-942`
  pins `['$schema','name','chipLabel','triggers','description']`).
- `DraftIssue.where` gains `| { section: 'triggers'; index?: number }` `:879`.
- `validateDraft` calls `validateDraftTriggers(draft, issues)` `:1354` right
  after `validateChipLabelPlaceholders`.
- `validateDraftTriggers` `:1507-1552`: returns when empty; over the cap →
  `err({section:'triggers'}, 'at most 8 trigger phrases')`; per phrase (where
  = `{section:'triggers', index}`): `< 3` → `a trigger phrase needs at least 3
  characters`; `> 80` → `a trigger phrase is at most 80 characters`; padded →
  `a trigger phrase must not begin or end with a space`; a fold repeat →
  `"${phrase}" is listed twice (phrases match regardless of case)` (the
  SECOND occurrence is flagged); then the SELF-ECHO warning: for every
  outcome message (`{section:'message', id}`, "this message"), the chip label
  (`{section:'identity', field:'chipLabel'}`, "the chip label") and, when
  `llmEnabled`, the consult prompt (`{section:'llm', field:'prompt'}`, "the
  consult prompt") — each phrase `triggersContainedIn` finds →
  `warn(where, `${what} contains the trigger "${phrase}" — a character who
  repeats Pascal's line will set this tool off again`)`.

#### 1.1.10 `lib/pascal/trigger-phrases.ts` (NEW, 92 lines — the SPA needs a twin)

Pure; imports `stripCarinaQueryLines` from `@/lib/chat/carina-parser` (NEW
there, +72 — `parseCarinaQuery`'s loop body extracted into
`parseQueryLine(line)`; `stripCarinaQueryLines(text)` splits `/\r?\n/`,
drops every line that parses as a query, joins with `'\n'`). Exports:
`MAX_TRIGGER_RUNS_PER_MESSAGE = 4` `:27`; `TriggerHit { tool; phrase; index }`
`:29-36`; `foldTriggerText(text) = text.toLocaleLowerCase()` `:39-41`;
`triggersContainedIn(text, triggers)` `:47-51` (`[]` on empty text or no
triggers; phrases of length > 0 whose fold the folded text `includes`, AS
WRITTEN); `scanTriggerPhrases(content, tools)` `:59-92` (strip Carina lines,
fold, ONE hit per tool at its earliest phrase, stable sort by index, slice at
4 → `{hits, dropped}`). The SPA consumes only `foldTriggerText` and
`triggersContainedIn` (`tool-draft.ts:64`).

#### 1.1.11 Tests that pin the SPA side

- `__tests__/unit/lib/pascal/tool-draft.test.ts` +67 (`:917-982`, "trigger
  phrases in the draft"): round-trip as written and in order; emit order
  `['$schema','name','chipLabel','triggers','description']` and no key while
  empty; `['ok', ' abracadabra', 'Hocus', 'HOCUS']` → errors at indexes
  `[0, 1, 3]` (all `severity: 'error'`); nine phrases → an error; an outcome
  message `'You cry ABRACADABRA and the door opens.'` with trigger
  `abracadabra` → exactly ONE `message`-section WARNING containing
  `"abracadabra"`; chip label `Abracadabra!` + prompt `Say abracadabra back.`
  (llm on) → a chipLabel warning and an llm warning, each containing
  `trigger`.
- `__tests__/unit/lib/pascal/custom-tool-definition.test.ts` +44: the
  JSON-Schema-mirror table gains six `triggers` rows (`:639-644`); "is
  deliberately weaker than Zod on trigger case-folding and whitespace"
  (`:701-710`); a `triggers` describe (`:724-748`): stores `'Open Sesame'` as
  written; `['abracadabra', 'open sesame ']` → `trigger 2 has leading or
  trailing whitespace`; `['Abracadabra', 'ABRACADABRA']` → `trigger 2 repeats
  trigger 1`; `KNOWN_TOP_LEVEL_KEYS` contains `'triggers'`.
- `__tests__/unit/lib/pascal/trigger-phrases.test.ts` (80, NEW): the
  scanner's nine cases + `triggersContainedIn` (`:72-80`: `'The word
  ABRACADABRA rings out.'` with `['Abracadabra', 'open sesame']` →
  `['Abracadabra']`; no match → `[]`; `undefined` triggers → `[]`).

### 1.2 `f0db8cbb8` — wardrobe effects

#### 1.2.1 `lib/pascal/custom-tool.types.ts` (+233) — the SPA twin's half

- Imports `:37-38`: `scanPlaceholders as scanPlaceholdersInText` from
  `./placeholders`; `WardrobeItemTypeEnum, type WardrobeItemType` from
  `@/lib/schemas/wardrobe.types`.
- Caps `:99-103`: `MAX_WARDROBE_EFFECT_ITEMS = 8`; `MAX_WARDROBE_ITEM_REF_LENGTH
  = 200`.
- `WardrobeEffectActionSchema = z.enum(['wear', 'take_off', 'clear_slot'])`
  `:637`; `WardrobeEffectModeSchema = z.enum(['wear', 'replace',
  'add_to_slot'])` `:641`.
- `CustomToolWardrobeGestureSchema` `:655-674` — `z.strictObject({ action,
  items: z.array(z.string().min(1).max(200)).min(1).max(8).optional(), mode:
  optional, slot: WardrobeItemTypeEnum.optional() })` with the four
  descriptions (action: `"wear" puts the listed items on the rolling
  character; "take_off" takes them off; "clear_slot" empties one slot.`;
  items: `The garments or outfits, each by title (case-insensitive) or id, in
  order. A template: {{params.x}}, {{metadata.key}} and the other placeholder
  families are substituted before lookup. Required for wear and take_off.`;
  mode: `wear only. Same meaning as the wardrobe_wear tool: honour the item's
  replace flag (default), force a swap, or layer into one slot.`; slot:
  `Required for clear_slot and for mode add_to_slot; optional on take_off to
  restrict the removal to one slot.`).
- `CustomToolEffectSchema` `:690-739`: `target` and `value` become
  `.optional()` (target's description gains `A write needs both target and
  value.`), a new `wardrobe: CustomToolWardrobeGestureSchema.optional()`
  (`A wardrobe gesture on the rolling character instead of a write: wear
  items, take them off, or clear a slot.`), and a `superRefine`: both →
  `an effect is either a write (target + value) or a wardrobe gesture, not
  both`; neither → `an effect needs target + value, or wardrobe`; a write
  missing one half → `a write needs both target and value` at `[target|
  value]` (whichever is undefined).
- Types + guards `:743-757`: `CustomToolWriteEffect`,
  `CustomToolWardrobeEffect`, `isWriteEffect(effect)` (`wardrobe === undefined
  && target !== undefined && value !== undefined`), `isWardrobeEffect(effect)`
  (`wardrobe !== undefined && target === undefined && value === undefined`).
- `wardrobeGestureProblems(gesture)` `:764-792` → `Array<{ field: 'items' |
  'mode' | 'slot'; message }>`: `wear`: no items → `wear needs at least one
  item`; `(mode ?? 'wear') === 'add_to_slot'` without slot → `mode add_to_slot
  needs a slot`; another mode WITH a slot → `slot is only used with mode
  add_to_slot`. `take_off`: no items → `take_off needs at least one item`;
  a mode → `mode is only used with wear`. `clear_slot`: no slot →
  `clear_slot needs a slot`; `items !== undefined` → `clear_slot takes no
  items`; a mode → `mode is only used with wear`.
- `export type { WardrobeItemType }` `:795`.
- `QtapCustomToolSchema.effects` description `:1028-1030` → `Side effects
  applied after the run: writes into tiered persistent state or the rolling
  character's metadata, or wardrobe gestures that change what the rolling
  character is wearing.`
- `validateEffects` `:1279-1290` reorders: `when` subjects FIRST, then
  `wardrobe` → `validateWardrobeGesture(declared, Boolean(tool.llm),
  effect.wardrobe, ctx, [...base, 'wardrobe'])` and `return`; then a bare
  `return` when either half is undefined (the refine already spoke).
- `validateWardrobeGesture` `:1342-1371`: every `wardrobeGestureProblems`
  entry as an issue at `[...path, field]`; then each item's placeholders
  (`scanPlaceholdersInText`): `params` not declared → `item references
  undeclared parameter "${name}"` at `[...path, 'items', m]`; `llm` without an
  llm block → `item references {{llm}}, but the tool declares no \`llm\`
  block`. Unknown families are NOT rejected.

#### 1.2.2 `lib/pascal/tool-draft.ts` (+159)

- Imports `:46-49` `wardrobeGestureProblems`, `WardrobeEffectAction`,
  `WardrobeEffectMode`, `WardrobeItemType`.
- `DraftEffect` `:210-223`: NEW `kind: 'write' | 'wardrobe'` ("Both halves are
  always present on the draft so switching kind in the form loses nothing
  until save; only the active half is emitted") and `wardrobe:
  DraftWardrobeGesture`.
- `DraftWardrobeGesture` `:226-231` `{ action; items: string[] ("Blank rows are
  dropped on save"); mode: WardrobeEffectMode; slot: WardrobeItemType | '' }`.
- `emptyDraftWardrobeGesture()` `:234-236` → `{ action: 'wear', items: [''],
  mode: 'wear', slot: '' }`; `newDraftEffect(kind = 'write')` `:239-250` →
  `{ id: nextDraftId('effect'), when: {kind:'always'}, kind, target: '',
  valueKind: 'expression', value: '', wardrobe: emptyDraftWardrobeGesture() }`.
- `effectToDraft` `:534-569`: a `wardrobe` effect → `kind: 'wardrobe'`, blank
  write half, `wardrobe: { action, items: items?.length ? [...items] : [''],
  mode: mode ?? 'wear', slot: slot ?? '' }`; a write → `kind: 'write'`,
  `target: effect.target ?? ''`, `value: effect.value === undefined ? '' :
  String(effect.value)`, `wardrobe: emptyDraftWardrobeGesture()`.
- `wardrobeFromDraft(gesture)` `:577-592`: items trimmed, blanks dropped;
  `wear` → `{ action:'wear', items, ...(mode !== 'wear' ? {mode} : {}),
  ...(mode === 'add_to_slot' && slot ? {slot} : {}) }`; `take_off` → `{
  action:'take_off', items, ...(slot ? {slot} : {}) }`; `clear_slot` → `{
  action:'clear_slot', ...(slot ? {slot} : {}) }`. ("Fields the action does
  not use are left out rather than emitted and rejected.")
- `effectFromDraft` `:600-605`: `kind === 'wardrobe'` → `{ ...(when ? {when}
  : {}), wardrobe: wardrobeFromDraft(effect.wardrobe) }`.
- `validateDraftEffects` `:1304-1312`: the verbatim-`when`-tests-llm error
  moves BEFORE the kind branch; `kind === 'wardrobe'` →
  `validateDraftWardrobeGesture(effect, draft, where, issues)` + `continue`.
- `validateDraftWardrobeGesture` `:1353-1387`: runs `wardrobeGestureProblems`
  over what the row would EMIT (`wardrobeFromDraft`, with an empty `items`
  array normalised to `undefined`), each as `err(where, problem.message)`;
  then every item's placeholders: undeclared `params` →
  `{{${key}}} names no declared parameter`; `llm` off → `{{llm}} needs the LLM
  consult enabled`; `unknown` → `warn(where, `{{${key}}} is not a placeholder
  this build knows — that item will not be found`)`.
- `renameParameterEverywhere` `:1739-1745`: every effect's
  `wardrobe.items` get the `{{params.from}}` → `{{params.to}}` replace.
- `findParameterReferences` `:1790-1795`: a wardrobe effect whose item matches
  `paramPlaceholderPattern(name)` → site `effect ${index + 1}: a wardrobe item
  names it`.

#### 1.2.3 `components/custom-tools/SideEffectsSection.tsx` (+215; 428 lines at the pin)

- Header `:8-19` rewritten (each row is a WRITE or a WARDROBE row; "Switching
  kind keeps the other half on the draft until save, so nothing typed is
  lost").
- Imports `:21-39`: `MAX_WARDROBE_EFFECT_ITEMS`, `MAX_WARDROBE_ITEM_REF_LENGTH`,
  `WardrobeEffectAction`, `WardrobeEffectMode`; `newDraftEffect`,
  `DraftWardrobeGesture`; `WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES,
  WardrobeItemType` from `@/lib/schemas/wardrobe.types`.
- `addEffect` `:65-67` → `update({ effects: [...draft.effects,
  newDraftEffect()] })` (the inline literal is gone).
- The empty-state sentence `:97-101`: "None declared. An effect lets a roll
  record its consequences — tick a counter in the story's state, note a fact
  on the rolling character's sheet, **put a garment on them or take one off** —
  applied server-side as part of the deal, beyond any model's power to fudge."
- The Kind select `:113-125`, FIRST in the row's top line (before When):
  `<label class="flex items-center gap-2">Kind <select value={effect.kind}
  onChange={… { kind }} disabled class="qt-select qt-select-sm"
  aria-label="Effect kind"><option value="write">Write a value</option><option
  value="wardrobe">Change the outfit</option></select></label>`.
- `:168-176`: `effect.kind === 'wardrobe'` → `<WardrobeFields
  gesture={effect.wardrobe} hasErrors={errors.length > 0} disabled
  onChange={(wardrobe) => updateEffect(effect.id, { wardrobe })} />`, else the
  existing Target + Value blocks (wrapped in a fragment `:177-261`).
- The hint `:277-283` gains: "An outfit change dresses or undresses the
  rolling character and nobody else; an item it cannot find is skipped and the
  rest still go on."
- `WARDROBE_ACTIONS` `:288-292`: `wear` / `take off` / `clear a slot` (labels
  lowercase). `WARDROBE_MODES` `:294-298`: `{ wear: 'as the item says',
  title: "Honour each item's own replace flag" }`, `{ replace: 'replace',
  title: 'Swap out whatever is in the slots the item covers' }`,
  `{ add_to_slot: 'layer into one slot', title: 'Add the item to one named
  slot' }`.
- `WardrobeFields` `:308-426` (props `gesture`, `hasErrors`, `disabled`,
  `onChange`): `wantsItems = action !== 'clear_slot'`; `wantsSlot =
  clear_slot || take_off || (wear && mode === 'add_to_slot')`; `slotOptional =
  take_off`; `items = gesture.items.length > 0 ? gesture.items : ['']`;
  `setItem(i, v)`; `removeItem(i)` (never below one blank row). Markup: Action
  `<select aria-label="Wardrobe action">` over `WARDROBE_ACTIONS`; when `wear`,
  Mode `<select aria-label="Wear mode">` over `WARDROBE_MODES` (each `<option
  title=…>`); when `wantsSlot`, Slot `<select aria-label="Wardrobe slot">`
  whose first option is `""` labelled **`every slot it covers`** when
  `slotOptional` else **`choose a slot`**, then `WARDROBE_SLOT_TYPES` with
  `WARDROBE_SLOT_META[slot].label`; when `wantsItems`, one row per item: `<input
  type="text" maxLength={200} placeholder={index === 0 ? 'Cloak of Night' :
  '{{params.garment}}'} class="qt-input flex-1 min-w-48 (+qt-input-error when
  hasErrors)" aria-label={`Item ${index + 1}`}>` + a trash button
  (`title="Remove this item"`, disabled when `items.length <= 1`); then
  `<button class="qt-button qt-button-ghost qt-button-sm">+ Add item</button>`
  (disabled at 8, `title="At most 8 items"`) and the hint `a title or id —
  <code>{{params.x}}</code> lets a parameter choose`.

#### 1.2.4 `components/custom-tools/ProvingBench.tsx` (+24; 718 at the pin)

- Import `:24` `ResolvedWardrobeGesture` from `@/lib/pascal/custom-tools`.
- `MiniPascalBubble` `:683-687`: a third branch between the write and the
  skipped arms — `'wardrobe' in effect` → `<p key={effect.index}>→ wardrobe:
  {describeBenchGesture(effect.wardrobe)} <em>(would attempt)</em></p>`.
- `describeBenchGesture` `:700-716`: `items = gesture.items.map((i) =>
  `“${i}”`).join(', ')` (curly double quotes U+201C/U+201D); `wear` → `wear
  ${items}${mode}` where `mode = gesture.mode && gesture.mode !== 'wear' ? `
  (${gesture.mode.replace(/_/g, ' ')}${gesture.slot ? ` ${gesture.slot}` :
  ''})` : ''` (so `add_to_slot` + `hair` → ` (add to slot hair)`); `take_off`
  → `take off ${items}${slot ? ` from ${slot}` : ''}`; `clear_slot` → `clear
  ${slot ?? 'a slot'}`. Doc: "Would attempt", never "would apply": the bench
  has no character, so it cannot know the garment exists.
- The bench's `ResolvedEffect` (wire, `lib/pascal/custom-tools.ts:1144-1156`)
  is now a THREE-arm union: `{ index; target; value } | { index; wardrobe:
  ResolvedWardrobeGesture } | { index; skipped }`, with
  `ResolvedWardrobeGesture { action; items: string[] (rendered, trimmed,
  empties dropped); mode?; slot? }` and `isApplicableEffect` = write OR
  wardrobe (`:1173-1177`).

#### 1.2.5 `components/chat/CustomToolRunDialog.tsx` (+31 from `f0db8cbb8`; the `4877fc2da` +31 is 1.3.11)

- `CustomToolReferences` `:92-97`: `wears?: string[]` ("Item references this
  tool's wardrobe effects may put on, verbatim. Absent on older servers."),
  `takesOff?: string[]`, `clearsSlots?: string[]`.
- `ToolReferencePanel` `:926-932`: `wardrobeLines` = `[{ key:'wears', lead:'It
  may dress the running character in', values }, { key:'takesOff', lead:'It
  may undress them of', values }, { key:'clearsSlots', lead:'It may clear
  their', values }].filter(values.length > 0)`; the null-panel predicate
  `:936` gains `&& wardrobeLines.length === 0`; the rendering `:1001-1014`,
  AFTER the writes paragraph: one `<p class="text-xs qt-text-secondary mt-4
  leading-relaxed">` per line — `{lead} ` + the values as `<code
  class="font-mono qt-text">` joined by `, ` + (for `clearsSlots` only: ` slots`
  when > 1 else ` slot`) + **`. Only when you run it as a character — a run
  made as nobody dresses nobody.`**

#### 1.2.6 Tests that pin the SPA side

- `tool-draft.test.ts` +79 (`:918-995`, "wardrobe effects in the draft"):
  round-trip the six shapes beside a write; `kind` flips keep both halves
  (`['Hat', '  ']` emits `['Hat']`; `target` survives; flipping back emits the
  write); the per-action rules in the loader's words (`['']` → `['wear needs
  at least one item']`; `add_to_slot` sans slot → `['mode add_to_slot needs a
  slot']`; `clear_slot` sans slot → `['clear_slot needs a slot']`; `clear_slot`
  + slot + items `['ignored']` → `[]` — the form DROPS unused fields before
  judging); `['{{params.ghost}}', '{{mystery}}']` → one error
  `{{params.ghost}} names no declared parameter` + one warning;
  `findParameterReferences` contains `effect 1: a wardrobe item names it`
  and `renameParameterEverywhere` rewrites the item.
- `custom-tool-definition.test.ts` +102 (`:276-354`, "effects — wardrobe
  gestures"): accepts each action; a `{{params.material}} gloves` item;
  tolerates `{{mystery}}`; both/neither/half-write rejections (`/not both/`,
  `/needs target \+ value, or wardrobe/`, `/needs both target and value/`);
  the per-action rules at PRECISE paths (`effects.0.wardrobe.slot` ×2,
  `.items`, `.mode`, `clear_slot needs a slot`, `.items.*takes no items`);
  undeclared param at `effects.0.wardrobe.items.1`, `{{llm}}` without a
  block, nine items / a 201-char ref rejected, gestures + writes count
  together against 16; ten more JSON-Schema-mirror rows (`:725-734`) and
  "deliberately weaker than Zod on wardrobe per-action rules" (`:791-801`).
- `__tests__/unit/lib/pascal/tool-vocabulary.test.ts` +29 (server-side
  `wears` / `takesOff` / `clearsSlots` — the wire the dialog reads).

### 1.3 `4877fc2da` — slash commands

#### 1.3.1 `lib/slash-commands/types.ts` (97) — VERBATIM API

```ts
export type SlashCommandKind = 'action' | 'builtin-tool' | 'custom-tool';
export type ComposerAction =
  | 'attach' | 'library' | 'image' | 'announce' | 'mail' | 'inform'
  | 'tool'      // The Run Tool dialog, no pre-selection.
  | 'pascal'    // The custom-tool dialog, no pre-selection.
  | 'roll'      // The Run Tool dialog, pre-selected on `rng`.
  | 'd6' | 'd20' | 'coin' | 'bottle';
export type SlashCommandPayload =
  | { kind: 'action'; action: ComposerAction }
  | { kind: 'builtin-tool'; toolId: string }
  | { kind: 'custom-tool'; name: string; asCharacterId: string | null };
export interface SlashCommand {
  key: string;        // 'action:attach', 'builtin:wardrobe_wear', 'custom:<name>::<asCharacterId>'
  kind: SlashCommandKind;
  command: string;    // the word after the slash, matched case-insensitively
  label: string; description: string; keywords: string[]; source: string;
  needsDialog: boolean;
  payload: SlashCommandPayload;
}
export interface SlashBuiltInToolSource {
  id: string; name: string; description: string; category?: string; pluginName?: string;
  available?: boolean; unavailableReason?: string; userInvocable?: boolean;
  parameters?: { properties?: Record<string, unknown> } | Record<string, unknown>;
}
export interface SlashCustomToolSource {
  name: string; title: string; description: string; parameters: Record<string, unknown>;
  characterLabel?: string; asCharacterId?: string | null;
}
export interface SlashCommandSources {
  actions: ReadonlySet<ComposerAction>;
  builtInTools: readonly SlashBuiltInToolSource[];
  customTools: readonly SlashCustomToolSource[];
}
```

The header `:4-9` is the portability contract: every file imports only from
`lib/char-insert/` (by relative path) and its siblings — "quilltap-v5 copies
this directory the way it copies `lib/char-insert/`".

#### 1.3.2 `lib/slash-commands/trigger.ts` (33) — VERBATIM

```ts
export const SLASH_TRIGGER: TriggerConfig = {
  opener: '/',
  queryPattern: /[A-Za-z0-9_-]/,
  queryStartPattern: /[A-Za-z]/,   // `/1`, `/-` are never commands
  minQueryLength: 0,               // a bare `/` opens the full menu
  maxQueryLength: 64,              // longer than any tool id
  closer: null,
  lowercaseQuery: true,            // command words are lowercase by construction
};
export function findSlashTrigger(textBefore: string): TriggerMatch | null {
  return findTrigger(textBefore, SLASH_TRIGGER);
}
```

`findTrigger` (`lib/char-insert/trigger.ts:65-96`, byte-identical in v5
`apps/web/src/app/editor/char-insert/trigger.ts:65`) supplies the opener rule:
run start, whitespace, or one of `( [ { " ' “ ‘ — –` before the `/`
(`OPENER_CHARS` `:31-41`), so `and/or`, `10/15`, `http://`, `a/b/c`, `word/`
stay prose; a space ends the query (not in `queryPattern`); `/un ` is null.

#### 1.3.3 `lib/slash-commands/ranking.ts` (76) — VERBATIM rules

```ts
const MatchTier = { Exact: 0, CommandPrefix: 1, WordPrefix: 2 } as const;
const KIND_ORDER: Record<SlashCommandKind, number> = { action: 0, 'custom-tool': 1, 'builtin-tool': 2 };
const WORD_SPLIT = /[\s\-_'’.·]+/u;
function fold(text: string): string { return text.toLocaleLowerCase(); }
function words(text: string): string[] { return fold(text).split(WORD_SPLIT).filter(Boolean); }
function matchTier(command, needle): MatchTier | null {
  const word = fold(command.command);
  if (word === needle) return Exact;
  if (word.startsWith(needle)) return CommandPrefix;
  const terms = [command.command, command.label, ...command.keywords];
  if (terms.some((term) => words(term).some((w) => w.startsWith(needle)))) return WordPrefix;
  return null;
}
function byKindThenLabel(a, b) {
  return KIND_ORDER[a.kind] - KIND_ORDER[b.kind]
    || a.label.localeCompare(b.label, undefined, { sensitivity: 'base' })
    || a.key.localeCompare(b.key);
}
export function rankSlashCommands(commands: readonly SlashCommand[], query: string, limit = 12): SlashCommand[] {
  const needle = fold(query);
  if (needle.length === 0) return [...commands].sort(byKindThenLabel);   // limit does NOT apply
  const scored = commands.flatMap(c => { const t = matchTier(c, needle); return t === null ? [] : [{ command: c, tier: t }] });
  scored.sort((a, b) => a.tier - b.tier || byKindThenLabel(a.command, b.command));
  return scored.slice(0, limit).map((e) => e.command);
}
```

Mid-word substrings are deliberately NOT matched ("the mention typeahead's
precedent"). Note `localeCompare` and `toLocaleLowerCase` are the runtime's
ICU — the mention oracle's precedent (§4.2).

#### 1.3.4 `lib/slash-commands/registry.ts` (191) — VERBATIM

`COMPOSER_ACTION_SPECS: Readonly<Record<ComposerAction, { label; description;
keywords; needsDialog }>>` `:28-107`:

| action | label | description | keywords | needsDialog |
|---|---|---|---|---|
| attach | Attach file | Attach a file from your computer | upload, paperclip | false |
| library | Attach from library | Attach a file already in your library | file, document | false |
| image | Generate image | Commission a picture for the chat | picture, camera, lantern | false |
| announce | Insert announcement | Post an announcement into the chat | megaphone | false |
| mail | Post a letter | Compose mail for a character | letter, post, office | false |
| inform | Inform the cast | A word out of character, delivered before a seat next speaks | ooc, tell | false |
| tool | Run Tool | Open Prospero's Run Tool dialog | prospero | **true** |
| pascal | Pascal's Table | Open the custom-tool dialog | custom, tools, wand | **true** |
| roll | Roll dice | Open the random-number tool to choose sides and count | dice, rng, random | **true** |
| d6 | Roll a d6 | Roll one six-sided die | dice, die, roll | false |
| d20 | Roll a d20 | Roll one twenty-sided die | dice, die, roll | false |
| coin | Flip a coin | Heads or tails | flip, toss, heads, tails | false |
| bottle | Spin the bottle | Let the bottle choose someone in the chat | spin | false |

- `actionCommand(action)` `:109-122`: key `action:${action}`, `command:
  action`, `keywords: [...spec.keywords]`, `source: 'Composer'`, payload
  `{kind:'action', action}`.
- `declaresProperties(parameters)` `:125-129`: object with a non-empty
  `properties` object.
- `builtInCommand(tool)` `:131-144`: key `builtin:${tool.id}`, `command:
  tool.id`, `label: tool.name`, `description: tool.description`, `keywords:
  [category, pluginName].filter(truthy)`, `source: qualifier ? `Prospero ·
  ${qualifier}` : 'Prospero'` where `qualifier = tool.pluginName ||
  tool.category`, `needsDialog: declaresProperties(tool.parameters)`.
- `customCommand(tool)` `:146-159`: `asCharacterId = tool.asCharacterId ??
  null`; key `custom:${tool.name}::${asCharacterId ?? ''}`, `command:
  tool.name`, `label: tool.title`, `keywords: characterLabel ?
  [characterLabel] : []`, `source: characterLabel ? `Pascal · as
  ${characterLabel}` : 'Pascal'`, `needsDialog: Object.keys(tool.parameters
  ?? {}).length > 0`, payload `{kind:'custom-tool', name, asCharacterId}`.
- `buildSlashCommands(sources)` `:166-191`: actions in `COMPOSER_ACTION_SPECS`
  key order, only those in `sources.actions`; built-ins SKIPPED when
  `userInvocable === false` or `available === false` or no `id` or a repeated
  id; customs skipped when no `name` or a repeated KEY (so the same name under
  two characters lists twice).

#### 1.3.5 `lib/slash-commands/index.ts` (11)

`export * from './types'; export { SLASH_TRIGGER, findSlashTrigger } from
'./trigger'; export { rankSlashCommands } from './ranking'; export {
COMPOSER_ACTION_SPECS, buildSlashCommands } from './registry';`

#### 1.3.6 `lib/slash-commands/__tests__/slash-commands.test.ts` (172) — the vectors a tier-1 corpus must carry

- `findSlashTrigger` opens on: `'/'`→(0,'' ), `'/un'`→(0,'un'), `' /un'`→(1),
  `'(/un'`→(1), `'hello /Wear'`→(6,'wear'), `'line\n/d20'`→(5,'d20'),
  `'/wardrobe_wear'`→(0); each `{ start, end: text.length, query, closed:
  false }`. Stays shut on: `and/or`, `10/15`, `http://`, `http://x`, `see
  /usr/bin`, `a/b/c`, `/1`, `/-`, `/un `, `word/`, `/` + 65 `a`s.
- `rankSlashCommands` over `wear` (builtin `wardrobe_wear` / "Wear Wardrobe
  Items"), `unlock` (custom / "Force the Lock"), `unlockBuiltIn` (builtin
  `unlock` / "Unlock"), `attach`, `announce` (actions), `undo` (builtin
  `redo_thing` / "Redo"): bare → `[attach, announce, unlock, undo,
  unlockBuiltIn, wear]`; `'un'` → `[unlock, unlockBuiltIn]`; exact `'unlock'`
  over `[undo, unlockBuiltIn]` → `unlockBuiltIn` first; `'wear'` → `[wear]`;
  `'lock'` → `[unlock]`; keyword `'off'` → the `office`-tagged mail; `'nlo'`
  → `[]`; `'att'` over `[library, attach]` → `[attach, library]` (tier beats
  kind); 20 `tool_N` with `'tool'`, limit 5 → 5; bare, limit 5 → 20.
- `buildSlashCommands`: wired-only actions (`['attach','d20']`, first
  `toMatchObject({kind:'action', source:'Composer', payload})`); all 13 when
  all wired; the five-built-in table → `['self_inventory','wardrobe_wear',
  'mcp_x']` with `needsDialog false / source 'Prospero · utility' / key
  'builtin:self_inventory'`, `needsDialog true / 'Prospero · wardrobe'`,
  `'Prospero · mcp'`; the custom trio → keys `['custom:unlock::',
  'custom:wear_regalia::', 'custom:wear_regalia::char-v']`, `needsDialog
  true / 'Pascal' / asCharacterId null`, `false`, `'Pascal · as Vivienne'`;
  `rng` as both → two commands, ranked `['custom-tool','builtin-tool']`.

#### 1.3.7 `components/chat/lexical/slash-commands/context.ts` (67)

```ts
export interface SlashCustomToolRef { name: string; asCharacterId: string | null }
export interface SlashCommandContext {
  chatId: string
  actions: Partial<Record<ComposerAction, () => void>>   // `tool`, `pascal`, `roll` derived from the openers
  openRunTool: (toolId?: string) => void
  openCustomTool: (initial?: SlashCustomToolRef) => void
  runBuiltInNow: (toolId: string) => Promise<void>
  runCustomNow: (tool: SlashCustomToolRef) => Promise<void>
  customToolsAvailable: boolean       // `/pascal` is listed only then
}
const QUICK_ROLLS = { d6: { type: 6, rolls: 1 }, d20: { type: 20, rolls: 1 },
  coin: { type: 'flip_coin', rolls: 1 }, bottle: { type: 'spin_the_bottle', rolls: 1 } }
export function buildQuickRollActions(chatId, onPendingResult): Partial<Record<ComposerAction, () => void>>
  // each: requestRngPendingResult(chatId, args).then(onPendingResult).catch(err => showErrorToast(err.message || 'The dice would not roll'))
```

#### 1.3.8 `components/chat/lexical/plugins/SlashCommandPlugin.tsx` (288)

- Constants: `MENU_LIMIT = 12` `:59`; `SLASH_COMMAND_TAG =
  'slash-command-commit'` `:66` (deliberately NOT a history-merge tag, "so an
  undo would take the whole word away instead of bringing it back");
  `LISTBOX_ID = 'slash-command-listbox'` `:68`; `EMPTY_LABEL = 'No such
  command'` `:69`; `LOADING_LABEL = 'Consulting the tool room…'` `:70`;
  `DIALOG_ACTIONS = new Set(['tool','roll','pascal'])` `:73`; `ACTION_ICONS`
  `:75-84` (attach→paperclip, library→file-plus, image→camera,
  announce→megaphone, mail→mail, inform→info, tool→wrench, pascal→wand); the
  glyph `:86-92`: builtin → `wrench`, custom → `wand`, action →
  `ACTION_ICONS[action] ?? 'dice'`.
- `dispatchSlashCommand(command, context)` `:103-123`: `action` → `tool` →
  `openRunTool()`; `roll` → `openRunTool('rng')`; `pascal` →
  `openCustomTool()`; else `context.actions[action]?.()`. `builtin-tool` →
  `needsDialog ? openRunTool(toolId) : void runBuiltInNow(toolId)`.
  `custom-tool` → `{name, asCharacterId}` → `needsDialog ? openCustomTool(tool)
  : void runCustomNow(tool)`.
- `HoldCommitKeys` `:130-144`: while mounted, Enter and Tab are swallowed at
  `COMMAND_PRIORITY_CRITICAL` (prevents `/self_inv` + Enter sending the draft
  before the tools arrive). Mounted `:269` only when `loading &&
  itemProps.options.length === 0`.
- `SlashCommandPlugin` `:150-155`: `enabled = chatSettings?.composerSlashCommands
  ?? true`; returns null when off (the plugin is UNMOUNTED, so Enter reaches
  the editor — v4's test `:176-184`).
- `SlashCommandMenu` `:157-286`: `query` state (null = closed); `rootElement`
  via `registerRootListener`; `contextRef` (latest handlers through a ref);
  `builtInQuery` `:173-179` `queryKey: queryKeys.tools.byChat(chatId)`,
  `GET /api/v1/tools?chatId=…&includeSchemas=true`, `enabled: menuOpen`,
  `staleTime: 60_000`; `customQuery` `:184-190` `queryKeys.customTools.byChat
  (chatId)` (the SAME key as the dialog), `enabled: menuOpen &&
  customToolsAvailable`, `staleTime: 0`; `commands` `:192-204`: the wired
  action keys minus `DIALOG_ACTIONS`, plus `tool`, `roll`, and `pascal` iff
  `customToolsAvailable`; built-ins from the query; customs only when
  available. `options` `:206-220`: `rankSlashCommands(commands, query, 12)`
  mapped to `TypeaheadOption` with `label: `/${command.command} —
  ${command.label}`` and `detail: command.source`. `triggerFn` `:222-233`:
  null while composing; `findSlashTrigger(text)`; null when
  `$isGluedToPreviousRun`; `toMenuTextMatch`. `onSelectOption` `:235-254`:
  `editor.update(() => nodeToReplace?.remove(), { tag: SLASH_COMMAND_TAG,
  onUpdate: () => dispatchSlashCommand(command, contextRef.current) })` —
  "Dispatch after the commit, so a dialog opening never races the editor's
  own selection restore"; then `closeMenu()`. `loading` `:257-259` = a pending
  query with NO data (cached tools show at once). Shell `:260-264`
  `useTypeaheadShell({ listboxId, emptyLabel: loading ? LOADING_LABEL :
  EMPTY_LABEL, activeDescendantTarget: rootElement })`. Mounted at
  `COMMAND_PRIORITY_CRITICAL` `:283`.
- Keyboard: Enter / Tab / click commit (the shell); Escape closes and leaves
  the text; Space is NOT a commit key — it falls out of `queryPattern`, so the
  trigger dissolves and the menu closes with the text left as prose
  (`:15-17`).

#### 1.3.9 `components/chat/useSlashCommandRunner.ts` (79)

`useSlashCommandRunner({ chatId, defaultCharacterId, onRan? })` →
`{ runBuiltInNow, runCustomNow }`. `runBuiltInNow(toolId)` `:31-56`: `POST
/api/v1/chats/${chatId}?action=run-tool` with `{ toolName: toolId, arguments:
{}, characterId: defaultCharacterId || undefined, private: false }`; `!res.ok
|| !data.success` → `showErrorToast(data.error || data.message || `${toolId}
could not be run`)`; success → `showSuccessToast(`Prospero has run
${toolId}.`)` + `onRan?.()`; a throw → the error toast with `err.message`.
`runCustomNow(tool)` `:58-76`: `apiFetch(`/api/v1/chats/${chatId}/custom-tools
?action=run`, { body: { tool: tool.name, parameters: null, asCharacterId:
tool.asCharacterId } })` — `private` OMITTED ("the definition's own default
visibility decides"); success → `showSuccessToast(`Pascal has settled
${tool.name}.`)` (the NAME, where the dialog toasts the TITLE), invalidates
`queryKeys.chats.detail(chatId)` and `queryKeys.customTools.byChat(chatId)`,
`onRan?.()`; failure → `showErrorToast(apiErrorMessage(err, `${tool.name}
could not be run`))`. "A private run is never reachable from here; the dialog
is the place for that."

#### 1.3.10 `components/chat/rng-request.ts` (73) + `RngDropdown.tsx` (−42 / +14)

`rng-request.ts`: `RngRollType = number | 'flip_coin' | 'spin_the_bottle'`;
`RngRequestArgs { type; rolls }`; `RngPendingResult` (moved from
`RngDropdown.tsx:31-40`, same eight keys); `postRng(chatId, args, preview)`
`:29-40` (`POST …?action=rng` `{ type, rolls, preview }`; `!ok` → `Error(
data.message || data.error || 'RNG failed')`); `requestRngPendingResult`
`:46-64` (`!data.preview || !data.result` → `Error('RNG did not return a
preview')`; else the chip `{ tool:'rng', displayName:'Random Number
Generator', icon:'🎲', summary, formattedResult: result.formattedText,
requestPrompt, arguments, success: true }`); `requestRngDirect` `:67-73`
(legacy). `RngDropdown.tsx` `:13-20` imports + re-exports `RngPendingResult`;
`executeRng` `:82-96` → `onPendingResult ? onPendingResult(await
requestRngPendingResult(…)) : onSuccess?.(await requestRngDirect(…))`.

#### 1.3.11 `components/chat/CustomToolRunDialog.tsx` (+31 from `4877fc2da`)

- Prop `:153-158` `initialTool?: { name: string; asCharacterId: string | null;
  nonce: number } | null` (default null) — "Applied once per nonce, after the
  fresh roster arrives; it lands on the remembered values for that tool like
  any other selection."
- `:272-292`: `appliedNonceRef`; `rosterSettled = rosterQuery.isSuccess &&
  !rosterQuery.isFetching` ("A pre-selection waits for the roster THIS open
  fetched, not a cached one, so a tool added to a store mid-chat is found. One
  nonce, one application."); effect: bail unless open + initialTool +
  settled; bail if the nonce was applied; `named = tools.filter(t => t.name
  === initialTool.name)`; `match = named.find(t => (t.asCharacterId ?? null)
  === initialTool.asCharacterId) ?? (initialTool.asCharacterId === null ?
  named[0] : undefined) ?? null`; no match → `setSelectedKey(null)` +
  `showErrorToast(`No tool named "${initialTool.name}" is on Pascal's table in
  this chat`)`; else `handleSelect(match)`.

#### 1.3.12 `components/chat/RunToolModal.tsx` (+42 / −18)

- Prop `:27-32` `initialTool?: { toolId: string; nonce: number } | null` —
  "Re-applied on every open — the nonce changes with each request, so picking
  the same command twice in a row re-selects."
- `handleSelectTool` hoisted ABOVE the fetch effect `:76-91` (unchanged body:
  `available === false` → return; select; seed defaults; `formValid false`).
- `:95-96` `requestedToolId`, `requestNonce`; the fetch effect `:113-133`:
  after `setTools(invocable)`, when `requestedToolId`: not found →
  `showErrorToast(`No tool named "${requestedToolId}" is available in this
  chat`)`; `available === false` → `showErrorToast(requested.unavailableReason
  || `"${requested.name}" is not available in this chat`)`; else
  `handleSelectTool(requested)`. Deps gain `requestedToolId, requestNonce,
  handleSelectTool`.

#### 1.3.13 `CustomToolsButton.tsx` (44) / `ComposerGutterTools.tsx` (+5 / −6) / `ChatModals.tsx` (+31) / `useModalState.ts` (+37) / `ChatComposer.tsx` (+74) / `SalonView.tsx` (+20) / `LexicalComposerWrapper.tsx` (+9)

- `CustomToolsButton` is now "Nothing but a button": props `onClick`,
  `disabled`, `variant` (`chatId` / `onRan` / `onClose` GONE); `aria-expanded`
  GONE; title / aria-label `Custom tools`, `wand` glyph unchanged.
- `ComposerGutterTools` `:31-32` `onCustomToolsClick?: () => void` replaces
  `onCustomToolRan`; `:149-155` renders the button only when
  `customToolsAvailable && onCustomToolsClick`.
- `useModalState` `:27-36`: `RunToolRequest { toolId: string | null; nonce }`,
  `CustomToolRequest { tool: { name; asCharacterId } | null; nonce }`;
  state `:51-53`; `openRunTool(toolId?)` `:93-97` bumps the nonce and opens;
  `openCustomTool(initial?)` `:102-106` likewise ("Without one the dialog
  lands wherever it remembers being"); `closeCustomTool` `:107`.
- `ChatModals` `:67-73` new props; `:491` `<RunToolModal … initialTool=
  {runToolRequest.toolId ? {toolId, nonce} : null}>`; `:494-505` the
  `<CustomToolRunDialog isOpen={customToolDialogOpen} onClose chatId
  onRan={fetchChat} initialTool={customToolRequest.tool ? {...tool, nonce} :
  null} />` — "Rendered unconditionally so it keeps its memory … One instance
  serves the gutter wand and the composer's `/` commands alike."
- `ChatComposer` `:110-119` five new props (`onCustomToolsClick`,
  `onOpenRunTool`, `onOpenCustomTool`, `onSlashToolRan`,
  `slashDefaultCharacterId`); `:233-237` `useSlashCommandRunner({ chatId: id,
  defaultCharacterId: slashDefaultCharacterId, onRan: onSlashToolRan })`;
  `:238-274` the memoised `SlashCommandContext` (null unless BOTH openers are
  given): `actions` = `attach` (clicks the hidden file input then
  `onAttachFileClick?.()`), `library`, `image`, `announce`, `mail`, `inform`
  (the gutter's own handlers) + `buildQuickRollActions(id, onPendingToolResult)`
  when `onPendingToolResult`; `openRunTool`, `openCustomTool`, the two runners,
  `customToolsAvailable: !!customToolsAvailable`; `:507` the gutter gets
  `onCustomToolsClick`; `:587` `<LexicalComposerWrapper … slashCommands=
  {slashCommands}>`.
- `SalonView` `:253-259` `slashDefaultCharacterId` = the first participant with
  `type === 'CHARACTER' && isActive && !removedAt` → `character.id`, else null
  ("acts as Run Tool's default would"); `:1738-1742` passes the five props;
  `:1947-1951` the three modal props; `:2077` the sidebar's `onRunToolClick={()
  => modals.openRunTool()}`. `customToolsAvailable` (`:386-388`, pre-existing)
  = `tools.length > 0 || errors.length > 0`.
- `LexicalComposerWrapper` `:84-88` `slashCommands?: SlashCommandContext |
  null` ("Omit (or null) and there is no menu — the plugin also stays
  unmounted while the `composerSlashCommands` setting is off"); `:174` mounted
  AFTER `MentionTypeaheadPlugin`, BEFORE `SmartTypographyPlugin`.

#### 1.3.14 `components/settings/chat-settings/ComposerSlashCommandsSettings.tsx` (NEW, 30) + `types.ts` (+2) + `useChatSettings.ts` (+19) + `ChatTabContent.tsx` (+10)

- `ComposerSlashCommandsSettings`: `enabled = settings.composerSlashCommands ??
  true` `:17`; `<SettingsToggleRow checked disabled={saving} onChange
  heading="Slash commands">` with the copy **"Type `/` at the start of a word
  to run a tool or open a composer action. The gutter buttons and the sidebar
  are never affected."** (`<code>/</code>`).
- `types.ts` `:110-111` `composerSlashCommands?: boolean` after
  `composerUnicode`, doc "Whether `/` opens the slash-command menu in the Salon
  composer (the gutter buttons and sidebar are not gated by this)".
- `useChatSettings.ts` `:69` + `:455-466` `handleComposerSlashCommandsChange`
  → `patchChatSettings({ composerSlashCommands: value }, 'Failed to update
  composer slash-commands setting' ×2)`; returned `:796`.
- `ChatTabContent.tsx` `:10` import; `:46` destructure; a NEW card `:112-118`
  BETWEEN the Composer card (`:89-110`) and Auto-Scroll (`:120-126`):
  `<CollapsibleCard title="Slash Commands" description="Run tools and composer
  actions by typing / in the Salon composer" sectionId="composer-slash-commands"
  forceOpen={activeSection === 'composer-slash-commands'}>`. Note: v4 did NOT
  put the toggle INSIDE the Composer card (unlike emoji / unicode / voice).

#### 1.3.15 `lib/query/keys.ts` (+6) and `eslint.config.mjs` (+4 / −1)

`queryKeys.tools.byChat = (chatId) => ['tools', 'chat', chatId]` `:174-179`
("Read by the composer's `/` command menu while it is open"). The
`no-restricted-imports` override `:94-97` now covers `files: ['lib/
char-insert/**/*.ts', 'lib/slash-commands/**/*.ts']` ("it may import
`lib/char-insert/` only by relative path").

#### 1.3.16 The four jsdom tests (what they pin)

- `SlashCommandPlugin.test.tsx` (265, against a REAL Lexical editor):
  BUILT_INS `self_inventory` (utility, empty properties), `wardrobe_wear`
  (wardrobe, `operations` property), `generate_image` (unavailable); ROSTER
  `unlock` (a `bonus` param, `asCharacterId 'char-1'`), `wear_regalia` (no
  params). Context: actions `attach`, `library`, `inform`, `d20`;
  `customToolsAvailable: true`. The menu: a bare `/` lists `/attach`,
  `/unlock`, `/wardrobe_wear`, `/pascal`, NOT `/generate_image`; `then /un` →
  exactly one option containing `/unlock`; `/wear` → first `/wear_regalia`,
  also `/wardrobe_wear`; `and/or`, `10/15`, `see http://x` → no options; with
  `composerSlashCommands: false` + `/att` → no options, Enter reaches the
  editor, `attach` not called. Picking: `/atta` + Enter → `defaultPrevented`,
  text `''`, `attach` ×1; `hello /infor` + Enter → `'hello '`, `inform` ×1;
  ONE undo restores `/atta`; Escape → no options, text `/atta`, nothing
  called; `/wardrobe_w` → `openRunTool('wardrobe_wear')`, no immediate run;
  `/self_inventory` → `runBuiltInNow('self_inventory')`, no dialog; `/unlock`
  → `openCustomTool({name:'unlock', asCharacterId:'char-1'})`; `/wear_regalia`
  → `runCustomNow({…'char-1'})`; `/roll` → `openRunTool('rng')`; `/d20` →
  `actions.d20` ×1.
- `CustomToolRunDialog.test.tsx` +27 (`:147-172`): `initialTool: {name:'wheel',
  asCharacterId:'char-1', nonce:1}` → `Run Spin the Wheel` button, no `Force
  the Lock`; roster `[UNLOCK, {...UNLOCK, characterLabel:'Vivienne',
  asCharacterId:'char-v'}]` + `{name:'unlock', asCharacterId:'char-v'}` → the
  run body `{ tool:'unlock', asCharacterId:'char-v' }`; `nonesuch` → the
  picker (`Force the Lock` visible) + `showErrorToast` containing `nonesuch`.
- `RunToolModal.test.tsx` +48 (`:613-659`): `initialTool {toolId:
  'wardrobe_wear', nonce:1}` → `Run Tool: Wear Wardrobe Items` + the form;
  Back returns to the list; `nonesuch` → the list + the exact toast `No tool
  named "nonesuch" is available in this chat`; a NEW nonce for the same tool
  re-selects after Back.
- `ComposerGutterTools.test.tsx` (+6 / −4): the wand is a plain button wired
  to `onCustomToolsClick` (`fireEvent.click` → called once).

### 1.4 `b2793400f` — Lexical 0.52 + bug 251 (`KeyboardPlugin.tsx`, 152 at the pin)

#### 1.4.1 The bug-251 hunk — the caret half (NO-PORT, §8.2)

`$codeBlockToEscape(selection)` `:38-56`: a collapsed range selection; an
ELEMENT-point anchor on the `CodeNode` at `offset === getChildrenSize()`, OR a
text-point anchor whose parent is the CodeNode, that is its LAST child, at
`offset === getTextContentSize()`; then `lines = getTextContent().split('\n')`,
`< 2` lines or a non-blank last line → null. The element-point arm is the fix
("straight after Shift+Enter Lexical leaves an ELEMENT point on the CodeNode
itself … Missing the second shape is why the escape never fired and chat mode
sent the message instead").

#### 1.4.2 The bug-251 hunk — the exit TRIM (PORT) — OLD vs NEW exactly

OLD (`b2793400f^`, `KeyboardPlugin.tsx:92-99`): `const trimmedText =
text.replace(/\n\s*$/, '')`, then `codeBlock.clear()` and re-append ONE
`$createCodeHighlightNode(trimmedText)` when non-empty. The regex `/\n\s*$/`
is unanchored on the left: `\s*` matches NEWLINES too, so it deletes from the
LEFTMOST `\n` whose remainder is all whitespace to the end — every trailing
whitespace-only line, not one. `"a\n  \n"` → `"a"`. `"one\ntwo\n\n\n"` →
`"one\ntwo"`.

NEW (`:93-103`): `let last = codeBlock.getLastChild(); while ($isTextNode(last)
&& last.getTextContent().trim() === '') { previous = last.getPreviousSibling();
last.remove(); last = previous } if ($isLineBreakNode(last)) last.remove();`
— remove trailing whitespace-only TEXT nodes, then ONE line break; "Earlier
lines (and their highlight nodes) are left alone." In text terms: strip
trailing `[ \t]*` (one whitespace-only run — the last line's content) then ONE
`\n`. `"a\n  \n"` → `"a\n  "` (the blank-line `\n` and nothing else go; the
spaces of the PREVIOUS line stay). `"one\ntwo\n\n\n"` → `"one\ntwo\n\n"`.
`"a\n  "` (last line whitespace, caret at end) → `"a"`. v4's own test
`:124-134` only asserts `codeText: 'a'` for `'a'` + line break + `'  '` — the
NEW path yields `'a'` there too (the `'  '` text node goes, then the break);
the `"a\n  \n"` divergence needs a seed with a whitespace line BEFORE the blank
one.

#### 1.4.3 The Lexical-only remainder (NO-PORT, §8.1)

`CharTypeaheadPlugin.tsx:262` / `MentionTypeaheadPlugin.tsx:103,177` (the
`KeyboardEvent | null` → `KeyboardEvent` command-type narrowing, a
`LexicalNode | null` loop annotation), `ImagePastePlugin.tsx:37-38`
(`'clipboardData' in event`), `table-transformer.ts:126-133` (a bare
`regExpEnd` so `@lexical/markdown` 0.52 skips the transformer on the live
shortcut path), `jest.config.ts`, the nine `package.json` / lock bumps, the
new `KeyboardPlugin.test.tsx` (194) and `table-transformer.test.tsx` (108).

### 1.5 `25b8fab6a` — bug 250 (`GuardedTypeaheadMenuPlugin.tsx`, NEW 81) — NO-PORT (§8.3)

Wraps `LexicalTypeaheadMenuPlugin`: `openCount` bumped in the consumer's
`onOpen` (inside the plugin's `startTransition`), an effect that re-runs the
consumer's `triggerFn` over the anchor text node's text up to the caret
(`triggerStillHolds` `:37-50` — `editor.read('latest', …)`, a collapsed text
anchor on a simple text node) and, when nothing matches, logs
`console.debug('[Typeahead] Closing a menu whose trigger vanished before it
opened')`, calls `onClose`, and bumps `generation` (the plugin's React `key`,
so it REMOUNTS and drops the stale resolution). The three typeaheads swap
`LexicalTypeaheadMenuPlugin` for it (`CharTypeaheadPlugin.tsx:37,329`,
`MentionTypeaheadPlugin.tsx:30,395`, `SlashCommandPlugin.tsx:30,277`).
`MentionTypeaheadPlugin.test.tsx:153-167` pins it: two `act`-batched seeds
(`hello @ar` then `''`) → no `#qt-mention-typeahead-listbox`, no options.

### 1.6 `a42c282b8` — `components/memory/memory-card.tsx:105`

`className="text-primary hover:underline"` → `className="qt-link"` on the
`Superseded by a digest` button (the `onViewDigest` branch). Nothing else.

### 1.7 `8489d08b9` — `app/about/AboutView.tsx` four blurbs (verbatim)

- `:146` **Aurora – Characters**: `detailed profiles with pronouns, aliases,
  clothing records, personalities, multi-character turn management, and
  archiving into an encrypted bundle you can rehydrate later` → `detailed
  profiles with pronouns, aliases, personalities, subprompts and progressions,
  a four-tier wardrobe with folders, pictures and a wear ledger,
  multi-character turn management, and archiving into an encrypted bundle you
  can rehydrate later`.
- `:150` **The Salon – Chat**: `chat interface with tool palette, agent mode,
  server-side rendering, and embedded tool messages` → `chat interface with
  tool palette, agent mode, server-side rendering, embedded tool messages,
  Inform notes from the wings, the Scenario Builder, and indexed message
  search`.
- `:158` **The Commonplace Book – Memory**: `long-term memory with semantic
  recall, memory gate, proactive recall, and deduplication` → `long-term memory
  with semantic recall and proactive recall, active and archived shelves,
  consolidation into digests, and one per-character maintenance pass`.
- `:166` **The Concierge – Alternative Content Provision and Routing**:
  `content classification with detection, auto-routing to uncensored
  providers, quick-hide integration, and a three-state per-chat control
  (Moderated, Unmoderated, Locked) settable at creation as well as
  mid-conversation, with the Concierge switching a chat himself after repeated
  refusals` → `refusal failover to the uncensored desk, an opt-in pre-screen,
  quick-hide integration, a Try Uncensored shield on every reply, and a
  three-state per-chat control (Moderated, Unmoderated, Locked) settable at
  creation as well as mid-conversation, with the Concierge switching a chat
  himself after repeated refusals` (the tail from `and a three-state` is
  unchanged).

### 1.8 `01f508d4e` — `components/wardrobe/hooks/useWardrobeItemActions.ts` (−7)

The `console.debug('[useWardrobeItemActions] Duplicating wardrobe item', {
sourceId, targetScope, targetId, newTitle, componentItemIds })` between
`nextCopyTitle` and `draftToPayload` (pre-commit `:210-216`) is DELETED.
Nothing else in the file.

---

## 2. v5 TODAY (`56554d98c`) — per surface, file:line

### 2.1 The `pascal/*` TS twins and their corpora

- `apps/web/src/app/pascal/custom-tool-types.ts` (2,2xx lines; a hand-ported
  Zod slice — the SPA has no `zod`): caps `:104-163` (`MAX_EFFECTS` `:135`,
  `MAX_EFFECT_TARGET_LENGTH` `:138`) — NO `MAX_TRIGGERS` / wardrobe caps;
  `CustomToolEffect { when?; target: string; value }` `:512-516` (REQUIRED
  target/value, no `wardrobe`); `QtapCustomTool` `:670-695` (no `triggers`;
  `chipLabel` `:679`); the parser's `chipLabel` arm `:1602-1606` (the site
  `triggers` goes right after); `effects` parse `:1672-1692`; the superRefine
  order `:1748-1752` (`validateReferences`, `validateGates`,
  `validateEffects` — `validateTriggers` goes fourth); `validateEffects`
  `:1944-`; the known-key list `:2203-2213` (`chipLabel` … `effects`);
  `safeParse` `:1578`; `collectUnknownKeys` `:2222`;
  `formatDefinitionIssues` `:2171`.
  - **Corpus:** `custom-tool-types.corpus.spec.ts` replays
    `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`
    (362 rows: 10 `title` + 300 `definition` + 52 `gate`; provenance in
    `src/testing/fixtures/README.md:7-63`), byte-comparing verdict / parsed
    data (`JSON.stringify`) / `collectUnknownKeys` / the FULL
    `formatDefinitionIssues` sentence. **Recorder:**
    `harness/oracle/cases/pascal-custom-tool-definition.ts` (OWNED by the
    Pascal server family — "lane AY's tree — this is a consumer copy"); the
    recipe (`README.md:53-63`): `cd <pinned v4 worktree> && npx tsx
    <V5>/harness/oracle/cases/pascal-custom-tool-definition.ts >
    <V5>/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`
    (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`; expect 362 lines today).
    The same NDJSON drives the Rust `pascal_custom_tool_definition_equivalence`.
  - The sibling specs `custom-tool-types.{effects,gate,llm,state}.spec.ts` are
    hand-transcriptions of v4's `custom-tool-definition.test.ts` describes.
- `apps/web/src/app/pascal/tool-draft.ts` (1,8xx): `DraftEffect` `:201-213`
  (no `kind` / `wardrobe`); `ToolDraft` `:215-252` (no `triggers`);
  `KNOWN_KEY_ORDER` `:255-270` (no `'triggers'`); `newDraft` `:290`;
  `draftFromDefinition` `:599-650`; `definitionFromDraft` `:817-878`
  (`chipLabel` `:824`, then `description`); `DraftIssue.where` `:888-904` (no
  `triggers` arm); `validateDraft` `:1432-` (`validateChipLabelPlaceholders`
  `:1457` — `validateDraftTriggers` goes right after);
  `validateDraftEffects` `:1319-1375`; `renameParameterEverywhere`
  `:1700-1734`; `findParameterReferences` `:1736-`. **Proof:**
  `tool-draft.spec.ts` (1,0xx) — "A CASE-FOR-CASE port of v4's
  `tool-draft.test.ts`" (no oracle corpus; transcribed cases + the §6.2
  round-trip normaliser `:40-70`).
- `apps/web/src/app/pascal/progress-arms.oracle.spec.ts` replays
  `pascal-progress.oracle.ndjson` (71 rows) recorded by
  `apps/web/oracle/pascal-progress.recorder.ts` (the SPA-owned recorder
  pattern; `README.md:65-91`).
- `tool-gate.ts` / `tool-presets.ts` / `expressions.ts` / `placeholders.ts`:
  untouched by the span. `tool-presets.ts` is corpus-free ("v4's own 92-line
  suite, ported case-for-case").
- **No `trigger-phrases.ts` twin; no `tool-vocabulary.ts` twin** (server-only;
  the dialog reads its OUTPUT as `references`). `chat/carina-parser.ts` has
  `parseCarinaQuery` `:111` and `isCarinaInvocableName` `:80` but NO
  `stripCarinaQueryLines` / `parseQueryLine`; its corpus is
  `src/testing/fixtures/carina-parser.ndjson` via
  `apps/web/oracle/carina-parser.recorder.ts` + `chat/carina-parser.oracle.spec.ts`.

### 2.2 The Workbench screens (`apps/web/src/app/screens/custom-tools/`)

- `builder-form.ts` (883): the identity `<section>` `:112-236` — Title
  `:115-129`, Chip label `:132-150` (`chipLabelError` / `chipLabelWarnings`
  `:638-649`), Description `:~170-185`, the toggles + the `Results`
  `<select aria-label="Default visibility">` `:222-235`, `</section>` `:236`;
  the gate section `:239-244`; `update(partial)` `:730-732`. The trigger field
  mounts at `:235/236` (after the Results label's closing `</div>`).
- `side-effects-section.ts` (328): header `:11-22`; `EFFECTS_HINT` /
  `_TAIL` `:27-33`; the empty-state sentence `:77-83`; the row's first line
  (When + trash) `:87-120`; Target `:122-168` (with the three prefix buttons);
  Value `:170-211`; errors / warnings `:213-218`; the hint `:222-224`;
  `addEffect` `:285-297` (an inline literal with `id: `new-effect-${n}``);
  `effectErrors` / `effectWarnings` `:303-319`. **Spec**
  `side-effects-section.spec.ts` (184) — TestBed-mounted, `effect()` /
  `draftWith()` helpers `:17-35`.
- `proving-bench.ts` (929): the effects block `:158-171` (`isApplicable` →
  `→ {{ effectTarget }} = {{ effectValue }} <em>(would write)</em>`, else `·
  effect N skipped: …`), the helpers `:904-920`; imports `isApplicableEffect,
  BenchRoll, DryEffect` from `./workbench.api` `:33`. `workbench.api.ts`
  `DryEffect` `:66-74` is a TWO-arm union (write | skipped); `BenchRoll`
  `:82-88`. **Spec** `proving-bench.spec.ts` (705).
- `workbench-library.ts` (465): badges `:264-310` — `oracle` `:289-295`,
  `N param(s)` `:296-300` (the `✦ N phrase(s)` badge goes between). The entry
  type is `CustomToolLibraryEntry` (`core/core-contract.ts:7500-7515`: `llm`,
  `parameterCount`, `outcomeCount` … NO `triggerCount`). **Spec**
  `workbench-library.spec.ts` (333).
- `workbench-editor.ts` (759) + `workbench-editor.spec.ts` (623) host the
  form; `custom-tools-page.ts` routes.

### 2.3 The custom-tool run dialog + Run Tool modal + RNG + the composer gutter (`apps/web/src/app/chat/`)

- `custom-tools-popup.ts` (786) — v5 FOLDS v4's `CustomToolsButton` +
  `CustomToolRunDialog` into ONE component (`:33-40` header): the wand
  `:98-109` (`title="Custom tools" aria-label="Custom tools"
  aria-haspopup="dialog" [attr.aria-expanded]="isOpen()"`, `(click)=
  "openDialog()"`), rendered only when `available()` `:91` (= `tools.length >
  0 || errors.length > 0` `:496-498`, the v4 `customToolsAvailable`
  predicate, derived HERE from the roster query); the modal `:111-449`; inputs
  `chatId` / `disabled` / `variant` `:460-463`, output `ran` `:465`; `isOpen`
  `:473`; `selectedKey` / `formValues` / `privateByKey` `:479-481` ("All three
  survive a close on purpose"); `rosterQuery` `:484-487` (`customToolsKeys.
  byChat(chatId)` + `fetchCustomToolsRoster`); `selectedTool` `:505-507`
  (derived); `keyOf` `:531-533` (`${name}::${asCharacterId ?? ''}`);
  `openDialog()` `:561-566` (protected; sets `isOpen`, `refetch()`); `close()`
  `:568-571`; `select(tool)` `:585-595`; `run(tool)` `:649-671` (toasts
  `Pascal has settled ${tool.title}.`); the reference panel `:209-268` —
  predicate `referenceRows().length > 0 || writeTargets().length > 0 ||
  readsProgress().length > 0` `:209-213`, the writes paragraph `:258-267`
  (the wardrobe lines go after it), `writeTargets` `:749-756`,
  `readsProgress` `:766-768`, `missingPlaceholderNote` `:771-786`. **Spec**
  `custom-tools-popup.spec.ts` (827; v4's `CustomToolRunDialog.test.tsx`
  "ported case for case", UNLOCK / WHEEL fixtures `:19-62`, the wand found by
  `button[aria-label="Custom tools"]` `:145`, `Run Force the Lock` /
  `Choose another tool` buttons).
- `custom-tools.api.ts` (120): `customToolsKeys.byChat` `:20` (`['customTools',
  chatId]`) and `.presets` `:31`; `CustomToolReferences` `:47-76` (through
  `now?` — NO `wears` / `takesOff` / `clearsSlots`); `CustomToolRosterEntry`
  `:79-81`; `fetchCustomToolsRoster` `:89-95` (`chatCustomToolsList`);
  `RunCustomToolArgs` `:98-103` (`parameters?: Record<…>`, `private?`,
  `asCharacterId?`); `runCustomTool` `:106-120` (`chatCustomToolRun`).
- `core/core-contract.ts`: `CustomToolListing` `:771-805` (`characterLabel?`,
  `asCharacterId: string` — REQUIRED, where v4's is optional; `parameters:
  Record<string, CustomToolParameterSpec>`), `CustomToolsRosterData` `:825`,
  `CustomToolRunData` `:853`; `pascalMeta.invokedBy: 'llm' | 'user'` `:3138`
  (+ `callerParticipantId?` `:3139`; no `triggerPhrase`); `ChatSettingsDto`
  `:3833-3872` — declared keys + an INDEX SIGNATURE `[key: string]: unknown`
  `:3871`, so an undeclared boolean reads without a type edit (the
  `composerUnicode` precedent: `char-insert-settings.ts:55-56` reads
  `query.data()?.['composerUnicode'] as boolean | undefined`).
- `tools/run-tool-modal.ts` (395): inputs `chatId`, `participants` `:267-268`;
  outputs `executed`, `close` `:271-272`; `selected` `:275`; `toolsQuery`
  `:282-285` (`queryKey: ['tools', this.chatId(), 'schemas']`, `listTools(core,
  {chatId, includeSchemas: true})`); `tools` `:288-290` (filters
  `userInvocable !== false`); `selectTool` `:354-364` (v4 `handleSelectTool`);
  `back` `:367-371`; `execute` `:374-394`. **Mounted CONDITIONALLY** by the
  Salon (`screens/salon/salon-conversation.ts:766-772`: `@if (showRunTool() &&
  chat(); as c) { <qt-run-tool-modal [chatId] [participants]
  (executed)="onToolRun()" (close)="showRunTool.set(false)" /> }`), opened by
  the sidebar's `(openRunTool)="showRunTool.set(true)"` `:427` — so the
  component is RECREATED per open (v4 keeps it mounted and re-applies by
  nonce). `chat-admin.api.ts` `listTools` `:184-191` (`toolsList`) and
  `runTool` `:198-` (`chatRunTool`; throws on `success === false`).
- `rng-dropdown.ts` (327): `RngPendingResult` `:21-30` (the same eight keys);
  `roll(kind, rolls)` `:300-327` → `rollRng(core, chatId, kind, rolls, true)`
  (`chat-cast.api.ts:246`, the `chatRng` verb) and emits the chip `:308-317`
  (the `displayName: 'Random Number Generator'` / `icon: '🎲'` literals are
  INLINE here — the extraction target). v5 "always previews" (`:53-60`); no
  legacy `requestRngDirect` branch.
- `chat-composer.ts` (1,197): the gutter `:317-440` (RNG `:389-396`,
  `<qt-custom-tools-popup [chatId] [disabled] variant="gutter"
  (ran)="customToolRan.emit()" />` `:401-406`, Inform `:432-440`); the editor
  `:495-518` (`[composerEmoji]="charInsert.emoji()"`, `[composerUnicode]=
  "charInsert.unicode()"`, `[mentionSource]`, `[mentionPriorityCharacterIds]`);
  `charInsert = injectCharInsertSettings()` `:569`; `mentionSource` `:579-583`
  (`createQueryMentionSource(QueryClient, characterKeys.list(), fetch)`);
  inputs `:585-663`; outputs `:665-690` (`customToolRan` `:679`,
  `pendingToolResult` `:684`); `editorView = viewChild(RichEditor)` `:695`.
  There is NO `customToolsAvailable` input (the popup self-gates). The Salon
  binds it at `salon-conversation.ts:504-537` (`(pendingToolResult)=
  "onPendingToolResult($event)"` `:525`, `(customToolRan)="onCustomToolRan()"`
  `:537`).

### 2.4 The editor's typeahead shell (`apps/web/src/app/editor/`) — how a `/` typeahead mounts

- `rich-editor.ts` (`qt-rich-editor`): inputs `:65-123` (`submitOnEnter`
  `:69`, `submitOnModEnter` `:76`, `composerEmoji` `:111`, `composerUnicode`
  `:113`, `mentionSource` `:121`, `mentionPriorityCharacterIds` `:123`);
  `buildPlugins()` `:291-360`: `dialectInputRules`, the two
  `charTypeaheadPlugin({ profile, enabled: () => this.composerX() })`
  `:339-340`, `mentionTypeaheadPlugin({ source: () => this.mentionSource(),
  priorityCharacterIds })` `:344-347` ("after the two char hosts as v4 mounts
  it … and above the keymaps, so Enter / Tab / Space reach it before the
  composer's submit"), `smartTypographyPlugin` `:352`, `textReplacementPlugin`
  `:355`, `history()`, the keymaps (incl. `escapeCodeBlockCmd` `:305-309`).
  **A `/` plugin goes at `:348` — after the mention plugin, before smart
  typography — exactly v4's `LexicalComposerWrapper.tsx:171-174` order.** The
  editor is shared by the composer, the Document-Mode pane
  (`documents/document-pane.ts:236-243` passes `composerEmoji` /
  `composerUnicode`, NO `mentionSource`) and form fields; the `mentionSource`
  null-means-inert contract (`:115-121`) is the model: a `slashCommands` input
  defaulting to `null` keeps every other host inert with no change.
- `char-insert/char-typeahead-plugin.ts` (354) — the adapter shape every v5
  typeahead copies: `options { profile; enabled: () => boolean }` `:59-73`; a
  per-view `Runtime` in a `WeakMap` `:131-141` (`menu: TypeaheadMenu`, `rows`,
  `selected`, `openQuery`, `openTrigger`); `liveTrigger(state)` `:160-177`
  (`textBeforeCursor` + `findTrigger` + the math bail); `refresh(view)`
  `:196-242` (bails: disabled / composing / not editable / no trigger; renders
  `menu.render(rows, selectedOrMinusOne, caretBox)` from `view.coordsAtPos`);
  `commitRow` `:244-254`; `handleMenuKey` `:310-345` (ArrowDown / ArrowUp /
  Enter / Tab / Escape — an EMPTY menu owns only Escape); the `Plugin` `:347-`
  with `view()` (runtime create / `update: refresh` / `destroy`) and `props.
  handleKeyDown` + a `blur` DOM handler.
- `mentions/mention-typeahead-plugin.ts` (4xx) — the richer precedent: a
  query-backed `source` (`mention-source.ts` — a `QueryObserver` over the
  shared cache key with `setActive(active)` = v4's `enabled: query !== null`),
  `holdCommitKeys` `:252` (Enter / Tab held while loading with zero rows
  `:396-398` — v4's `HoldCommitKeys`), `setEmptyLabel` for loading / error /
  empty `:308-311`, a commit that dispatches with `closeHistory` and a trailing
  `closeHistory` transaction so the completion is its own undo step
  `:357-363`, constants `MENU_LIMIT` / `LISTBOX_ID` / `EMPTY_LABEL` /
  `LOADING_LABEL` / `ERROR_LABEL` `:76-81`, `toRow` `:479-487` (`glyph: '@'`).
- `char-insert/typeahead-menu.ts`: `TypeaheadRow { key; glyph?: string; label;
  detail? }` `:21-30` — the glyph is a TEXT string (v4's `SlashCommandPlugin`
  renders an `<Icon>`; §3.3 decides); `TypeaheadMenu` `:103-` (`render`,
  `setEmptyLabel`, `close`, `destroy`; `.qt-typeahead-menu [role="option"]
  .qt-typeahead-option-label`).
- `char-insert/trigger-context.ts`: `textBeforeCursor(state)` `:51-67`
  (null on a non-collapsed selection or in code context; the whole textblock's
  text up to the caret, leaves as U+FFFC, `hard_break` as `\n`). v4's
  `$isGluedToPreviousRun` has NO v5 twin by design (`:5-10`): reading the
  whole textblock answers it.
- `char-insert/trigger.ts`: `findTrigger` `:65` / `isTriggerOpenerContext`
  `:52` — v4's byte-for-byte (the `emoji-trigger-vectors.json` /
  `unicode-trigger-vectors.json` corpora in `__fixtures__/`).
- `char-insert/char-insert-settings.ts` (58): `injectCharInsertSettings()` →
  `{ emoji, unicode }` signals over `chatSettingsKeys.all` (`?? true`); the
  accessor a `slashCommands` signal joins.
- `char-insert/char-typeahead-harness.ts` + `mentions/mention-typeahead-harness.ts`
  (the REAL-editor spec harnesses: `seed`, `type`, `pressKey` through every
  plugin's `handleKeyDown`, `settle` = four 5 ms ticks, `optionLabels()`,
  `menuText()`, `undo()`); `mention-typeahead-plugin.spec.ts` is v4's
  `MentionTypeaheadPlugin.test.tsx` case for case.
- **ESLint:** `apps/web` has NO eslint config (measured: no `eslint.config.*`,
  no `no-restricted-imports` anywhere; `package.json:6` `"lint"` is
  `scripts/check-qt-classes.mjs`). The char-insert portability contract is
  enforced by convention + the corpus specs, not by lint — there is no
  restriction twin to widen (§3.3 proposes a one-case imports census spec).

### 2.5 `editor/editing-commands.ts` `exitCodeBlockOnBlankLine` `:215-249` + its spec

Conditions `:220-232` (collapsed caret, parent is `code_block`, caret at
`content.size`, ≥ 2 lines, last line `trim() === ''`); the trim `:235`
`text.replace(/\n\s*$/, '')` — v4's OLD regex; the delete `:241` as a range
from `end - (text.length - trimmed.length)` to `end`; a paragraph inserted
after `:242-244`. The docstring `:211-213` claims the regex "strips one newline
plus any trailing whitespace, not every trailing blank line" — FALSE for the
old regex (§9.4). Bound in `rich-editor.ts:305-309` ahead of the submit
command for the two composer modes only. **Spec:** `rich-editor.spec.ts:237-320`
(four cases through `setMarkdown('```\ncode\n\n```')` + `pressEnter`; none
seeds a whitespace line before the blank one, so the OLD and NEW trims are
indistinguishable there — the re-port adds the `"a\n  \n"` case).

### 2.6 Settings → Chat (`apps/web/src/app/screens/settings/chat/`)

- `chat-settings.types.ts`: `AUTOMATION_OPTIONS` `:138-149` typed `readonly {
  key: 'autoDetectRng'; label; description }[]` with ONE row;
  `DEFAULT_AUTO_DETECT_RNG = true` `:151`; no `AutomationKey`, no
  `DEFAULT_CUSTOM_TOOL_TRIGGERS`, no `DEFAULT_CUSTOM_TOOLS` (the Custom Tools
  card inlines `?? true` at `custom-tools-settings.ts:89-91`).
- `automation-settings.ts` (67): the PRE-commit shape verbatim — the loop with
  `[checked]="option.key === 'autoDetectRng' ? autoDetectRng() : false"` `:40`,
  the hard-coded `qt-hover-accent cursor-pointer` class `:35`, `onChange(key,
  value)` `:63-66` returning for any other key. The card reads
  `this.settings()?.['autoDetectRng']` `:59-61` and saves via
  `ChatSettingsCard.save({ autoDetectRng }, 'Failed to update auto-detect RNG
  setting')` — v5's cards are SELF-CONTAINED (`chat-settings.api.ts:60-100`:
  the shared query, `save(partial, failureText)` seeding the cache from the
  PUT echo), so v4's `useChatSettings` handler + `ChatTabContent` prop
  plumbing has NO v5 analogue: a new boolean is one `save({ key })` call
  inside the card.
- `composer-unicode-settings.ts` (62) — the toggle-row twin to copy for the
  slash card: `enabled = settings()?.['composerUnicode'] ?? true` `:55-57`,
  `save({ composerUnicode: value }, 'Failed to update symbol shortcut
  setting')` `:60`, the `qt-settings-toggle-row` markup `:31-48` (heading
  `qt-settings-section-heading`, copy `qt-text-small mt-1`).
- `chat-tab.ts`: the Composer card `:107-122` (`sectionId="composer-spellcheck"`;
  hosts spellcheck → emoji → unicode → In Their Own Words), Auto-Scroll
  `:124-131`, Automation `:187-194` (`<qt-automation-settings />` — no
  bindings), Custom Tools `:196-203`. **`chat-tab.spec.ts`** pins the card
  ORDER (`V4_CARD_ORDER` `:29-30` …) and the COUNT `expect(titles.length).
  toBe(20)` `:102`; the Composer card's four rows `:126-138`.
- Spec conventions: `chat-settings.spec-harness.ts` (`settingsRow(over)`,
  `cardStub(row, failUpdate)` with `stub.updates`, `mountCard`, `checkboxes`,
  `toggle`); `scalar-toggle-cards.spec.ts` (hosts the `AutomationSettings`
  describe) and `composer-char-insert-cards.spec.ts` (the emoji / unicode
  pattern: default-when-unset, persisted false, "PUTs the bare scalar on
  toggle" → `stub.updates toEqual([{ key: value }])`, the copy, dogfood #6's
  visible failure).

### 2.7 The smalls

- `memory/memory-card.ts:70` — the provenance line carries `<button
  type="button" class="text-primary hover:underline" (click)="viewDigest.emit
  (digestId)">Superseded by a digest</button>`; `memory-card.spec.ts:253-254`
  asserts `classList.contains('text-primary')` and `('hover:underline')`. The
  OTHER `text-primary hover:underline` buttons (`:77` Show more, and the edit
  buttons further down) are unchanged in v4 — do not over-port.
- `screens/about/about-page.ts` feature list: `:326` Aurora, `:330` Salon,
  `:338` Commonplace Book, `:346` Concierge — the PRE-commit bytes;
  `about.spec.ts:107-121` asserts the OLD Concierge sentence verbatim (`:116`)
  under the title "carries e9a9c538e's three widened feature sentences".
- `wardrobe/wardrobe-item-actions.ts:231-237` — the `console.debug` twin,
  verbatim; `wardrobe-item-actions.spec.ts:150-166` SPIES on `console.debug`
  and asserts the call ("logs v4's debug line").

### 2.8 The e2e suite (`apps/web/e2e/`)

116 files; `support/env.ts` — the shared server on `PORT = 4319`
(`BASE_URL`), `MOCK_LLM_PORT = 45301`, `MOCK_SERPER_PORT = 45303`,
`E2E_PASSPHRASE`, `FIXTURES_DIR = crates/quilltap-web/tests/fixtures`. Specs
that need their OWN instance pick a port constant (`PROJ_PORT = 4325`,
`CONCIERGE_PORT = 4331`, `WIZARD_PORT = 4333` + `MOCK_LLM_PORT = 45304`, …;
taken: 4320–4342 except none free below 4343 — measured `4323 / 4335 / 4337 /
4338 / 4339` also appear — so a new own-instance spec takes **4343+** and a
mock port **45341+**). Files ride the SHARED global-setup server and must sort
AFTER `aa-foundation.spec.ts` (workers: 1). The gating idiom: a NAMED constant
`const P4Dnnn_SERVER_LANDED = true/false` with `test.skip(!FLAG, 'awaits …')`
(`settings-chat-cards-flow.spec.ts:33-46,233-237`,
`workbench-progress-flow.spec.ts:19-35`, `salon-send-failure-restore-flow.
spec.ts:39-42`); NEVER a capability probe for a defined-but-refusing verb.
`support/seed-pascal-tools-fixture.ts` (run by `global-setup.ts`) seeds three
`Tools/*.tool.json` into Aria's vault for `Group Expedition` — `coin` (public,
NO parameters, `min === max`), `whispered` (private), `ansible` (gated) — the
roster `salon-custom-tools-flow.spec.ts` drives (`Flip A Coin` / `Run Flip A
Coin` / `Choose another tool`). `composer-mention-flow.spec.ts` is the
typeahead e2e precedent (`composerEditor(page)` = `.qt-chat-composer-input
.qt-rich-editor-content`; `menu(page)` by listbox id; derives expectations from
the instance; blanks the draft after each beat; never presses Enter on an
empty settled menu). `workbench-flow.spec.ts` authors a definition through the
live editor + a picked destination.

---

## 3. The two-lane FILE SPLIT (zero overlap) + tiers

### 3.1 LANE P — Pascal SPA (Workbench + run-dialog wardrobe lines + the Automation toggle)

| v5 path | change | tier |
|---|---|---|
| `apps/web/src/app/pascal/custom-tool-types.ts` | `MAX_TRIGGERS` / `MIN_` / `MAX_TRIGGER_PHRASE_LENGTH`, `MAX_WARDROBE_EFFECT_ITEMS`, `MAX_WARDROBE_ITEM_REF_LENGTH`; `WardrobeEffectAction` / `WardrobeEffectMode` / `CustomToolWardrobeGesture` / `WardrobeItemType` (+ a `WARDROBE_ITEM_TYPES` enum list — the SPA has no `wardrobe.types` schema module; mirror v4's `WardrobeItemTypeEnum` values, see §7.3); `CustomToolEffect` → `{ when?; target?; value?; wardrobe? }` + `CustomToolWriteEffect` / `CustomToolWardrobeEffect` / `isWriteEffect` / `isWardrobeEffect` / `wardrobeGestureProblems`; the effect parser's strictObject + refine (three sentences, `[target\|value]` path); `QtapCustomTool.triggers?`; the `triggers` parse arm after `chipLabel` (Zod's own `min(3)` / `max(80)` / array `min(1)` / `max(8)` sentences through the shim); `validateTriggers` fourth; `validateEffects` reordered + `validateWardrobeGesture`; known keys + `'triggers'` | **1** (corpus) |
| `apps/web/src/app/pascal/custom-tool-types.corpus.spec.ts` | consume the RE-RECORDED 362+N-row fixture (row count guard updated) | 1 |
| `apps/web/src/app/pascal/custom-tool-types.effects.spec.ts` (+ a new `custom-tool-types.triggers.spec.ts`) | transcribe v4's `custom-tool-definition.test.ts` +44 / +102 describes case for case | 1 (transcription) |
| `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson` | the consumer copy, re-recorded (see §4.1 — the GENERATOR is the server family's; P re-runs it) | — |
| `apps/web/src/testing/fixtures/README.md` | the provenance paragraph for the refresh | — |
| NEW `apps/web/src/app/pascal/trigger-phrases.ts` + `trigger-phrases.spec.ts` | the 92-line twin whole (`MAX_TRIGGER_RUNS_PER_MESSAGE`, `TriggerHit`, `foldTriggerText`, `triggersContainedIn`, `scanTriggerPhrases`) with v4's 80-line test transcribed | 1 |
| `apps/web/src/app/chat/carina-parser.ts` + `carina-parser.oracle.spec.ts` + `apps/web/oracle/carina-parser.recorder.ts` + `fixtures/carina-parser.ndjson` | `parseQueryLine` extracted, `stripCarinaQueryLines` added; the recorder grows `strip` rows; re-record | 1 (corpus) |
| `apps/web/src/app/pascal/tool-draft.ts` + `tool-draft.spec.ts` | `ToolDraft.triggers`, `DraftEffect.kind` + `wardrobe`, `DraftWardrobeGesture`, `emptyDraftWardrobeGesture`, `newDraftEffect`, `effectToDraft` / `wardrobeFromDraft` / `effectFromDraft`, `DraftIssue.where` `triggers` arm, `validateDraftTriggers`, `validateDraftEffects` reorder + `validateDraftWardrobeGesture`, the rename / references arms; `KNOWN_KEY_ORDER` + `'triggers'` after `'chipLabel'`; the spec transcribes 1.1.11 + 1.2.6 case for case | 1 (transcription) |
| NEW `apps/web/src/app/screens/custom-tools/trigger-phrases-field.ts` + `.spec.ts` | `qt-trigger-phrases-field` (1.1.1 verbatim: label, chip list, input + Add, hint, error lines) | 2 (component) |
| `apps/web/src/app/screens/custom-tools/builder-form.ts` + `builder-form.spec.ts` | mount the field at `:235/236` with `[triggers]="draft().triggers" [issues]="issues()" (triggersChange)="update({ triggers: $event })" [disabled]="disabled()"` | 2 |
| `apps/web/src/app/screens/custom-tools/side-effects-section.ts` + `.spec.ts` | the Kind select, `WardrobeFields` (as a second component in the same file or `wardrobe-fields.ts` — P's choice, same lane), `newDraftEffect()` in `addEffect`, the two copy changes | 2 |
| `apps/web/src/app/screens/custom-tools/workbench.api.ts` | `DryEffect` gains the `{ index; wardrobe: ResolvedWardrobeGesture }` arm; `ResolvedWardrobeGesture`; `isApplicableWriteEffect` / `isApplicableWardrobeEffect` / `isApplicableEffect` (either) | — (wire type) |
| `apps/web/src/app/screens/custom-tools/proving-bench.ts` + `.spec.ts` | the third branch + `describeBenchGesture` (curly quotes, `replace(/_/g, ' ')`) | 2 |
| `apps/web/src/app/screens/custom-tools/workbench-library.ts` + `.spec.ts` | the `✦ N phrase(s)` badge with its title, between `oracle` and `N param(s)` | 2 |
| `apps/web/src/app/core/core-contract.ts` | `CustomToolLibraryEntry.triggerCount: number` (after `llm`); `pascalMeta.invokedBy: 'llm' \| 'user' \| 'trigger'` + `triggerPhrase?: string`; document `customToolTriggers?: boolean` and `composerSlashCommands?: boolean` on `ChatSettingsDto` (optional — the index signature already admits them; declaring both HERE keeps C's accessor cast-free after unification) | — |
| `apps/web/src/app/chat/custom-tools.api.ts` | `CustomToolReferences.wears? / takesOff? / clearsSlots?` | — |
| NEW `apps/web/src/app/chat/custom-tool-wardrobe-lines.ts` + `.spec.ts` | the pure `wardrobeReferenceLines(refs)` + the `qt-custom-tool-wardrobe-lines` component (§3.3 meeting point 1) | 2 |
| `apps/web/src/app/screens/settings/chat/chat-settings.types.ts` | `AUTOMATION_OPTIONS` second row (the union key type widens to `'autoDetectRng' \| 'customToolTriggers'`), `AutomationKey`, `DEFAULT_CUSTOM_TOOLS = true`, `DEFAULT_CUSTOM_TOOL_TRIGGERS = false` | — |
| `apps/web/src/app/screens/settings/chat/automation-settings.ts` + `scalar-toggle-cards.spec.ts` | the row-table rewrite (1.1.4): `rows` keyed by `AutomationKey`, the `customTools ?? true` gate, `opacity-50 cursor-not-allowed`, `save({ customToolTriggers }, 'Failed to update trigger phrase setting')` | 2 |
| `apps/web/src/app/wardrobe/wardrobe-item-actions.ts` + `.spec.ts` | delete the `console.debug` + the spy assertion (01f508d4e) | — |
| `apps/web/public/schemas/qtap-custom-tool.schema.json` | byte-copy of v4's at the pin (+8 triggers, +102 wardrobe) — **IF the Pascal server lane does not already claim it; settle at order-writing** (the ledger lists it under the server family) | — |
| NEW `apps/web/e2e/workbench-triggers-effects-flow.spec.ts` | the P beats (§4.4) | 3 (e2e) |

**P never touches:** `chat/custom-tools-popup.ts`, `chat/chat-composer.ts`,
`chat/tools/*`, `chat/rng-dropdown.ts`, `editor/**`, `screens/salon/**`,
`screens/settings/chat/chat-tab.ts` (+spec), `composer-*-settings.ts`,
`memory/**`, `screens/about/**`.

### 3.2 LANE C — the composer (slash commands + bug 251 + the two non-Pascal smalls)

| v5 path | change | tier |
|---|---|---|
| NEW `apps/web/src/app/chat/slash-commands/{types,trigger,ranking,registry,index}.ts` | the pure tier, a character-for-character transcription of 1.3.1–1.3.5 (imports `findTrigger` / `TriggerConfig` / `TriggerMatch` from `../../editor/char-insert/{trigger,types}` by relative path — the `chat/mentions/mention-typeahead.ts` precedent) | **1** |
| NEW `apps/web/oracle/slash-commands.recorder.ts` + `apps/web/src/testing/fixtures/slash-commands.ndjson` + NEW `apps/web/src/app/chat/slash-commands/slash-commands.oracle.spec.ts` | the tier-1 oracle (§4.2) | 1 |
| NEW `apps/web/src/app/chat/slash-commands/slash-commands.spec.ts` | v4's 172-line jest file transcribed (belt and braces beside the corpus) + a one-case imports census (§3.3) | 1 |
| NEW `apps/web/src/app/editor/slash/slash-command-plugin.ts` + `slash-command-host.ts` + `slash-command-harness.ts` + `slash-command-plugin.spec.ts` | the ProseMirror adapter (§4.3): `slashCommandPlugin({ host: () => SlashCommandHost \| null })`, the host contract (v4's `SlashCommandContext` + the two query-backed sources), constants `MENU_LIMIT = 12`, `LISTBOX_ID = 'qt-slash-command-listbox'`, `EMPTY_LABEL = 'No such command'`, `LOADING_LABEL = 'Consulting the tool room…'` | 2 |
| `apps/web/src/app/editor/char-insert/typeahead-menu.ts` (+spec) | ONLY if an icon glyph is wanted (§3.3 point 4); otherwise untouched | 2 |
| `apps/web/src/app/editor/char-insert/char-insert-settings.ts` | `slashCommands: Signal<boolean>` = `query.data()?.['composerSlashCommands'] as boolean \| undefined ?? true` — **the local typed accessor** (the cast is the file's own existing idiom; no `core-contract.ts` edit) | — |
| `apps/web/src/app/editor/rich-editor.ts` + `rich-editor.spec.ts` | input `slashCommands = input<SlashCommandHost \| null>(null)`; mount `slashCommandPlugin({ host: () => this.slashCommands() })` at `:348`; the bug-251 trim case | 2 |
| `apps/web/src/app/editor/editing-commands.ts` | the trim re-port (§4.5) + the docstring made true | 1 (vector) |
| NEW `apps/web/src/app/chat/rng-request.ts` + `.spec.ts` | `RngRollType`, `RngRequestArgs`, `RngPendingResult` (moved), `requestRngPendingResult(core, chatId, args)` over `rollRng` (the `chatRng` verb) — no `requestRngDirect` (v5 never previews false; record the NO-PORT) | 2 |
| `apps/web/src/app/chat/rng-dropdown.ts` + `.spec.ts` | import + re-export `RngPendingResult`; `roll()` → `requestRngPendingResult` | 2 |
| NEW `apps/web/src/app/chat/slash-command-runner.ts` + `.spec.ts` | `createSlashCommandRunner({ core, queryClient, toasts, chatId, defaultCharacterId, onRan })` → `{ runBuiltInNow, runCustomNow }` (1.3.9 over `runTool` / `runCustomTool`) | 2 |
| `apps/web/src/app/chat/custom-tools-popup.ts` + `.spec.ts` | `initialTool = input<{ name; asCharacterId: string \| null; nonce } \| null>(null)` + a PUBLIC `open(initial?)` method (the composer's slash host calls it through `viewChild`); the nonce-once effect over `rosterQuery.isSuccess() && !rosterQuery.isFetching()`; the toast `No tool named "…" is on Pascal's table in this chat`; a public `available` read for the host's `customToolsAvailable` | 2 |
| `apps/web/src/app/chat/tools/run-tool-modal.ts` + `.spec.ts` | `initialTool = input<{ toolId: string; nonce: number } \| null>(null)`; applied in an `effect` once the query settles (the two toasts, `available === false` → `unavailableReason`) — the modal is recreated per open so the nonce is a fidelity carry, not a need | 2 |
| `apps/web/src/app/chat/chat-composer.ts` + `.spec.ts` | the slash host assembled from the gutter's own handlers (`openAnnouncement` / `openMail` / `openLibrary` / `openGenerate` / `openInform` emit, `attach` = `fileInput.click()`, the quick rolls → `pendingToolResult.emit`), `openRunTool` → a NEW output `openRunTool = output<string \| undefined>()`, `openCustomTool` → the popup's `open()`, the runner, `customToolsAvailable` from the popup; `[slashCommands]` bound only when `charInsert.slashCommands()`; a `slashDefaultCharacterId` input | 2 |
| `apps/web/src/app/screens/salon/salon-conversation.ts` | `runToolRequest = signal<{ toolId: string \| null; nonce } >`; the sidebar's and the composer's openers both bump it; `<qt-run-tool-modal [initialTool]>`; `[slashDefaultCharacterId]` = the first active, non-removed CHARACTER participant's `character.id`; `onSlashToolRan` → the existing `onToolRun` / `onCustomToolRan` refetch | 2 |
| NEW `apps/web/src/app/screens/settings/chat/composer-slash-commands-settings.ts` + `.spec.ts` | the toggle-row card (1.3.14 copy verbatim; `?? true`; `save({ composerSlashCommands }, 'Failed to update composer slash-commands setting')`) | 2 |
| `apps/web/src/app/screens/settings/chat/chat-tab.ts` + `chat-tab.spec.ts` | the "Slash Commands" card between Composer and Auto-Scroll (`sectionId="composer-slash-commands"`); the order table + `toBe(21)` | 2 |
| `apps/web/src/app/memory/memory-card.ts` + `.spec.ts` | `class="qt-link"` on the digest button; the spec asserts `qt-link` and the ABSENCE of `text-primary` on that button | — |
| `apps/web/src/app/screens/about/about-page.ts` + `about.spec.ts` | the four sentences; the spec's Concierge assertion updated + one per new sentence | — |
| NEW `apps/web/e2e/composer-slash-commands-flow.spec.ts` | the C beats (§4.4) | 3 (e2e) |

**C never touches:** `pascal/**`, `screens/custom-tools/**`,
`chat/custom-tools.api.ts`, `core/core-contract.ts`, `chat/carina-parser.ts`,
`screens/settings/chat/{chat-settings.types,automation-settings}.ts`,
`scalar-toggle-cards.spec.ts`, `wardrobe/**`, `public/schemas/**`.

### 3.3 The meeting points, named

1. **The custom-tool dialog's wardrobe sentences (P) inside C's
   `custom-tools-popup.ts`.** Viable from the +31 hunk: the hunk is three
   optional type keys (→ `custom-tools.api.ts`, P), a pure derivation
   (`wardrobeLines`), one predicate extension, and three `<p>`s rendered
   AFTER the writes paragraph. P delivers
   `chat/custom-tool-wardrobe-lines.ts`: `export function
   wardrobeReferenceLines(refs: CustomToolReferences | undefined): Array<{
   key: 'wears' | 'takesOff' | 'clearsSlots'; lead: string; values: string[];
   suffix: string }>` (suffix `' slots'` / `' slot'` for `clearsSlots`, `''`
   otherwise) and a standalone `@Component({ selector:
   'qt-custom-tool-wardrobe-lines', template: @for (line of lines(); …) {<p
   class="text-xs qt-text-secondary mt-4 leading-relaxed">{{ line.lead }}
   @for (value …) {<span>@if (i > 0) {, }<code class="font-mono qt-text">{{
   value }}</code></span>}{{ line.suffix }}. Only when you run it as a
   character — a run made as nobody dresses nobody.</p>} })` with
   `references = input<CustomToolReferences | undefined>()` and `lines =
   computed(() => wardrobeReferenceLines(this.references()))`. **Unifier
   wire (three lines in C's file):** add the component to the popup's
   `imports`, extend the `@if` at `:209-213` with `|| wardrobeLines().length >
   0` (a `computed` over the same pure fn), and place
   `<qt-custom-tool-wardrobe-lines [references]="tool.references" />` after the
   writes `@if` at `:267`. P proves the component in its own spec (lead
   sentences, the `slot` / `slots` suffix, the `code` joins, nothing rendered
   when every list is empty); C's popup spec is untouched until unification,
   where ONE case (a roster with `wears`) is added.
2. **The chat-settings keys.** P owns `chat-settings.types.ts` /
   `automation-settings.ts` / `core-contract.ts`; C reads
   `composerSlashCommands` through `char-insert-settings.ts`'s own accessor
   (the existing `as boolean | undefined` idiom over the DTO's index
   signature) and writes it from its own card via `save({ composerSlashCommands
   })` — no shared file, no cast to retire.
3. **The run-tool / custom-tool openers.** C owns both dialogs and the Salon
   wiring; the Workbench never opens them. P's Automation toggle and C's slash
   card both extend the chat settings screen but in DIFFERENT files
   (`automation-settings.ts` vs a new card + `chat-tab.ts`).
4. **The typeahead glyph.** v4 renders an `<Icon name>` per row
   (`SlashCommandPlugin.tsx:86-92`); v5's `TypeaheadRow.glyph` is a string
   rendered as text. C decides: (a) extend `TypeaheadRow` with `iconName?:
   IconName` and have `TypeaheadMenu.render` emit the icon's SVG (the menu
   renders plain DOM — `ui/icon.ts` would need a static-markup export), or
   (b) a text glyph per kind (`/` for actions, `⚙` for Prospero, `✦` for
   Pascal) — a recorded divergence. The icon names all exist in `ui/icon.ts`
   (`paperclip`, `file-plus`, `camera`, `megaphone`, `mail`, `info`, `wrench`,
   `wand`, `dice`). Either way it is C-only (`editor/char-insert/
   typeahead-menu.ts` is C's).
5. **The portability contract.** No ESLint to widen; C adds a vitest case to
   `slash-commands.spec.ts` that reads the five files' import lines
   (`import.meta.glob` is unavailable in the Angular builder — read them via
   the `?raw`-equivalent `text` loader by registering `.ts` is NOT possible;
   instead assert on a hand-maintained `IMPORT_CENSUS` string constant per
   file, or skip the census and rely on the oracle spec). Recommend: rely on
   the oracle spec + a header comment; note the absent lint in the order.
6. **The schema JSON** (`apps/web/public/schemas/qtap-custom-tool.schema.json`)
   — P OR the Pascal server lane; the ledger assigns it to the server family's
   "byte-copy (the standing rule)". Settle in the orders; the unifier's
   byte-compare against `25b8fab6a:public/schemas/qtap-custom-tool.schema.json`
   catches either way.

---

## 4. The differential plan per lane

### 4.1 LANE P — corpora re-records + transcriptions

- **`pascal-custom-tool-definition.oracle.ndjson`** — the generator
  (`harness/oracle/cases/pascal-custom-tool-definition.ts`) must GROW for
  `triggers` (the six mirror rows of 1.1.11 + the three `triggers` describe
  cases: as-written storage, the whitespace sentence, the case-fold repeat) and
  for wardrobe effects (the ten mirror rows of 1.2.6 + the per-action-rule
  paths + the undeclared-param / `{{llm}}` items + the 9-item / 201-char refs +
  the mixed cap). **The generator is the Pascal SERVER lane's tree** (the
  `pascal_custom_tool_definition_equivalence` family shares the file); P's
  order says: *co-ordinate — the server lane adds the rows, P re-records from
  the pin and consumes the copy; if the server lane has not landed its rows
  when P needs them, P adds them to the generator under a clearly-marked
  `// P4.<P> rows` block and the unifier merges by content (the §3 review
  catches a drift)*. Recipe: a pinned worktree at `25b8fab6a` (`git -C
  ~/source/quilltap-server worktree add --detach /tmp/qt-v4-pin-<order>-25b8fab6a
  25b8fab6a`; `ln -sfn ~/source/quilltap-server/node_modules …`), `TZ=UTC npx
  tsx <V5>/harness/oracle/cases/pascal-custom-tool-definition.ts >
  <V5>/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`,
  then the row-count guard in the corpus spec and `README.md`. Expect 362 + the
  added rows; a SHORTER file is the empty-file trap. The Zod version at the pin
  (check `package.json` — the README's last note is 4.6.5) must match the
  shim's built-in sentences (the `zod_version_guard` precedent, P4.D211).
- **`carina-parser.ndjson`** — grow `apps/web/oracle/carina-parser.recorder.ts`
  with `strip` rows (`stripCarinaQueryLines`): public and whisper lines, a
  query among spoken lines (v4's own `'Hold on.\n@Alice: what is a spell?\n
  Abracadabra!'`), CRLF, leading whitespace (not a query), an empty text; the
  oracle spec gains the `strip` kind. The recorder's header carries the pin
  recipe.
- **`trigger-phrases.spec.ts`** — v4's 80 lines transcribed (needs the strip);
  the ICU fold is the runtime's on both sides (`toLocaleLowerCase` — the
  mention precedent).
- **`tool-draft.spec.ts`** — 1.1.11's six cases and 1.2.6's five cases
  transcribed VERBATIM (same documents, same assertions, incl. the emit-order
  `['$schema','name','chipLabel','triggers','description']` and the `[0, 1, 3]`
  index list). The §6.2 normaliser needs no change (`triggers` is emitted only
  when non-empty).
- **`custom-tool-types.*.spec.ts`** — the two describe blocks transcribed; the
  paths in the regexes (`effects.0.wardrobe.slot`) read against the shim's
  joined sentence exactly as the corpus does.
- **Component specs** (TestBed, the `side-effects-section.spec.ts` shape):
  the Kind select's two options and that switching kind keeps the write half
  (set `kind` via the select, read the emitted draft); `WardrobeFields`
  visibility rules (Mode only on `wear`; Slot on `clear_slot` / `take_off` /
  `wear`+`add_to_slot`; the `every slot it covers` vs `choose a slot` first
  option; `aria-label="Item N"` rows; Add item disabled at 8 with the title;
  remove disabled at one row); the trigger field (Enter adds the TRIMMED
  candidate, `canAdd` false under 3 chars and at the cap, the placeholder
  flips to `At most 8 phrases`, the per-index error as chip title AND line,
  `Remove trigger phrase X`); the bench's `describeBenchGesture` four shapes
  (`wear “Hat”`, `wear “Ribbon” (add to slot hair)`, `take off “Sundress”
  from top`, `clear hair`, `clear a slot`); the library badge (`1 phrase` /
  `2 phrases`, the title, hidden at 0); the Automation card (second row,
  default unchecked, dead with `opacity-50 cursor-not-allowed` while
  `customTools: false`, `stub.updates toEqual([{ customToolTriggers: true }])`,
  dogfood-#6's failure text `Failed to update trigger phrase setting`).
- **Traps:** `react-closure-reads-in-signal-ports` — `addEffect` / the Kind
  switch read `effects()` before emitting; `{{…}}` in templates (the
  `EFFECT_VALUE_PLACEHOLDER` constant precedent at `side-effects-section.ts:24-25`
  — the `{{params.garment}}` placeholder and the `{{params.x}}` hint MUST be
  constants); `[class]` re-sorts tokens — use `[attr.class]` only where a byte
  string is pinned; a `curly-quote` in a template expression (`“${item}”`) is
  built in TS, not the template.

### 4.2 LANE C — the tier-1 oracle for `lib/slash-commands`

- **Recorder** `apps/web/oracle/slash-commands.recorder.ts` (outside `src/`;
  imports `@/lib/slash-commands`), the `mention-typeahead.recorder.ts` shape
  (`emit({ id, kind, … })` with duplicate-id refusal; the pin recipe in the
  header). Row kinds: `constants` (`SLASH_TRIGGER` with its regexes as
  `source` strings, `COMPOSER_ACTION_SPECS` whole), `trigger` (`text` → `out:
  TriggerMatch | null`), `rank` (`commands` (full `SlashCommand[]`), `query`,
  `limit` → `out: string[]` keys), `build` (`sources` with `actions` as an
  array → `out: SlashCommand[]` whole). Vectors: v4's test table (1.3.6) +
  every opener context (`(` `[` `{` `"` `'` `“` `‘` `—` `–`, a soft `\n`, an
  image leaf U+FFFC before the slash), the alphabet edges (`/a-b_c9`, `/Ä`
  rejected — `queryStartPattern` is ASCII), 64 vs 65, uppercase folding
  (`/WEAR`), the `WORD_SPLIT` set (`'`, `’`, `.`, `·`, `-`, `_`, whitespace),
  kind-order ties with equal labels differing only by key, `sensitivity:
  'base'` (`Attach` vs `attach` vs `Átach`), the limit at exactly 12 / 13,
  built-ins with `available: undefined` (listed), `userInvocable: undefined`
  (listed), an empty `id`, duplicate ids, `parameters` with `properties: {}`
  vs absent vs a non-object, customs with `asCharacterId: undefined` vs `null`
  vs a string, duplicate keys across characters.
- **Spec** `chat/slash-commands/slash-commands.oracle.spec.ts` replays every
  row (`toEqual` on parsed JSON; the `constants` row compares `RegExp.source`
  and `flags`). `localeCompare` / `toLocaleLowerCase` are Node's ICU on both
  sides — pin the Node version in the README paragraph (the mention
  precedent). Plus `slash-commands.spec.ts`: v4's jest file transcribed
  (keeps the lane honest before the recorder runs).
- **Expect ~150–200 lines**; record the count in `README.md`.

### 4.3 LANE C — the ProseMirror adapter + its harness (transcribing `SlashCommandPlugin.test.tsx`)

- `editor/slash/slash-command-host.ts`: `SlashCustomToolRef`,
  `SlashCommandHost { chatId; actions: Partial<Record<ComposerAction, () =>
  void>>; openRunTool(toolId?); openCustomTool(initial?); runBuiltInNow;
  runCustomNow; customToolsAvailable: () => boolean; builtInTools:
  SlashToolSource; customTools: SlashToolSource }` where `SlashToolSource` is
  the `MentionCharacterSource` shape (`snapshot(): { tools | undefined;
  isPending; isError }`, `setActive`, `subscribe`) — built over
  `QueryObserver`s on `['tools', chatId, 'schemas']` (REUSE the run-tool
  modal's key: same `listTools({chatId, includeSchemas: true})` body, so the
  modal is served from cache after a `/`) and `customToolsKeys.byChat(chatId)`
  (the popup's key; `staleTime: 0` ↔ `refetchOnMount`-equivalent: call
  `observer.refetch()` on open). `buildQuickRollActions(core, chatId,
  onPendingResult)` over `requestRngPendingResult`, toasting `The dice would
  not roll` on a bare throw.
- `editor/slash/slash-command-plugin.ts`: the char/mention adapter shape —
  `liveTrigger` = `textBeforeCursor` + `findSlashTrigger`; `refresh` bails
  when `host() === null` / composing / not editable; on a live trigger
  `setActive(true)` both sources (customs only when `customToolsAvailable()`),
  `buildSlashCommands({ actions: wired − {tool, roll, pascal} + {tool, roll}
  + (pascal iff available), builtInTools, customTools })`, `rankSlashCommands(
  commands, query, 12)`, rows `{ key: command.key, glyph, label:
  `/${command} — ${label}`, detail: source }`; `loading` = a pending source
  with no data; `holdCommitKeys = loading && rows.length === 0` (Enter / Tab
  swallowed — v4's `HoldCommitKeys`); `setEmptyLabel(loading ? LOADING_LABEL
  : EMPTY_LABEL)`; commit: `tr.delete(trigger.from, trigger.to)` dispatched
  with `closeHistory` BEFORE and a trailing `closeHistory` transaction (the
  mention commit precedent, so ONE undo restores `/atta`), then `dispatch(
  command, host)` synchronously after the dispatch returns (ProseMirror
  dispatch is synchronous; v4's `onUpdate` ordering is satisfied); Space is
  not handled (the trigger dissolves on the next refresh); Escape closes and
  leaves the text; the `blur` handler closes.
- `editor/slash/slash-command-harness.ts` (the `mention-typeahead-harness.ts`
  shape with scripted `builtInTools` / `customTools` answers and a stub host
  recording calls) and `slash-command-plugin.spec.ts` transcribing 1.3.6's
  fourteen cases (BUILT_INS / ROSTER fixtures verbatim, `then /un`, `/wear`
  first `/wear_regalia`, the three prose texts, the setting off → no menu and
  Enter handled by the base keymap, `/atta`+Enter → `''`, `hello /infor` →
  `'hello '`, one undo → `/atta`, Escape, `/wardrobe_w` → `openRunTool(
  'wardrobe_wear')`, `/self_inventory` → `runBuiltInNow`, `/unlock` →
  `openCustomTool({name:'unlock', asCharacterId:'char-1'})`, `/wear_regalia`
  → `runCustomNow`, `/roll` → `openRunTool('rng')`, `/d20` → `actions.d20`).
- `rich-editor.spec.ts`: one case that a `slashCommands` host mounts the
  listbox on `/` and `null` leaves `/` as prose; `chat-composer.spec.ts`: the
  host's `actions` map calls the composer's outputs (the `openAnnouncement`
  etc. emits), the quick roll emits `pendingToolResult`, `openRunTool` emits
  the tool id, `openCustomTool` calls the popup's `open`.
- `custom-tools-popup.spec.ts`: 1.3.16's three `initialTool` cases
  (`wheel` → `Run Spin the Wheel`; the `char-v` variant → run body
  `asCharacterId: 'char-v'`; `nonesuch` → the picker + the toast); note
  `tanstack-angular-data-lags-invalidate` — the once-per-nonce effect must
  read `rosterQuery.isFetching()` / `isSuccess()` signals AFTER settle (await
  the harness `settle()` twice).
- `run-tool-modal.spec.ts`: the three cases (open onto `Run Tool: Wear
  Wardrobe Items` + the form + Back; `nonesuch` → the exact toast; a new nonce
  re-selects).
- `rng-request.spec.ts`: the chip shape from a scripted `chatRng` answer;
  `RNG did not return a preview` on `preview: false` / missing `result`.
- `composer-slash-commands-settings.spec.ts`: the four scalar-toggle cases
  (default `true`, persisted false, `[{ composerSlashCommands: false }]`, the
  failure text) + the copy ("The gutter buttons and the sidebar are never
  affected.").

### 4.4 e2e beats + gating (both lanes, NEW files only)

- **C — `composer-slash-commands-flow.spec.ts`** (shared server, `Group
  Expedition`, Aria's seeded `coin` tool — NO parameters, so `/coin`-the-custom
  vs `/coin`-the-action collide: the custom ranks FIRST; use `/flip` or the
  tool's actual `name` from the roster dispatch, derived like the mention
  spec): (a) a bare `/` opens `#qt-slash-command-listbox` with `/attach`,
  `/tool`, `/roll`, `/pascal`; (b) `/d20` + Enter → a pending-result chip
  appears above the box and the draft is empty (the `chatRng` preview is
  real); (c) `/<coin-name>` + Enter → `Pascal has settled …` toast and the
  Pascal row lands (the seeded tool is deterministic); (d) `/wardrobe_w` +
  Enter → the Run Tool dialog opens ON `Run Tool: Wear Wardrobe Items`; (e)
  `and/or` stays prose; (f) Settings → Chat → "Slash Commands" off → reload →
  `/att` + nothing opens, draft blanked; toggle back on. (b)–(d) ride the
  server's existing verbs (`chatRng`, `chatCustomToolRun`, `toolsList`,
  `chatRunTool`) — LIVE in-lane; (f) needs the server's `composerSlashCommands`
  PUT arm → `const P4Dnnn_SERVER_LANDED = false` + `test.skip` until the
  settings server lane lands (an unknown key is DROPPED by a defined verb, so a
  probe would lie — the standing rule).
- **P — `workbench-triggers-effects-flow.spec.ts`** (shared server): (a) open
  the Workbench, a seeded tool, add a trigger phrase `abracadabra` via the
  field + Enter → the chip renders; type `ok` → Add disabled; save → the
  library badge `✦ 1 phrase` (needs the server's `triggerCount` AND the
  server accepting `triggers` on save — gated `P4Dnnn_SERVER_LANDED`); (b) an
  effect row → Kind `Change the outfit` → `wear` + an item → the bench's
  `would attempt` line (needs the preview verb's wardrobe arm — gated);
  (c) Settings → Chat → Automation: the second row dead while Custom Tools is
  off (LIVE: a client-side gate), toggle Custom Tools on → row live → tick →
  reload → persisted (gated on the PUT arm).

### 4.5 LANE C — the bug-251 trim re-port (tier 1 by vector)

Replace `:235` with the NEW algorithm expressed on the block's text: `const
withoutTail = text.replace(/[^\S\n]*$/, '')` (the last line's whitespace-only
content — `[^\S\n]` is whitespace excluding newline) then, if `withoutTail`
ends with `\n`, drop ONE `\n`; delete the range `[end - (text.length -
trimmed.length), end)` as today. Vectors (add to `rich-editor.spec.ts` or an
`editing-commands.spec.ts`): `"a\n  \n"` → `"a\n  "` (the divergence);
`"a\n  "` → `"a"`; `"one\ntwo\n\n\n"` → `"one\ntwo\n\n"`; `"code\n"` →
`"code"` (the existing case); the docstring at `:211-213` becomes TRUE and
cites bug 251. (The markdown serialiser trims a code block's trailing newline,
so assert through `inCodeBlock()` + the block's `textContent` via
`runCommand`, as `rich-editor.spec.ts:279-286` already does.)

---

## 5. Fixtures

- `apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`
  — RE-RECORD (P; generator shared with the server family, §4.1).
- `apps/web/src/testing/fixtures/carina-parser.ndjson` — RE-RECORD with the
  `strip` kind (P).
- NEW `apps/web/src/testing/fixtures/slash-commands.ndjson` (C).
- `apps/web/src/testing/fixtures/README.md` — THREE paragraphs touched by two
  lanes → **a conflict waiting to happen.** Rule: each lane appends its own
  `## ` section at the END of the file (P: the two refreshes; C: the new
  corpus) and touches no other line; the unifier takes both hunks.
- The e2e seeded roster (`seed-pascal-tools-fixture.ts`, from
  `crates/quilltap-web/tests/fixtures/pascal-run-custom-*.db`) is UNCHANGED by
  either lane; the slash beat derives the tool name at run time.
- `apps/web/public/schemas/qtap-custom-tool.schema.json` — a byte-copy from
  the pin (§3.3 point 6).
- No committed `*-main.db` pairs are this slice's (the D23 re-dump is the
  server keystone's).

---

## 6. Traps (from the memory notes + this survey's own measurements)

- **`react-closure-reads-in-signal-ports`:** v4's `SideEffectsSection`
  `updateEffect` and `WardrobeFields.set` spread the PREVIOUS gesture; in
  signals read `this.effects()` / the row once into a local before emitting.
  `TriggerPhrasesField.add()` reads `canAdd` then `onChange` — compute
  `candidate` from the input's current value, not from a signal that the
  `(input)` handler may not yet have set in the same tick.
- **`spa-component-harness-gotchas`:** the popup already needs
  `provideTanStackQuery` + `coreStreamStub()`; a slash harness over the
  composer needs the same (the composer injects `RealtimeService`?— no, but it
  injects `QueryClient` via `createQueryMentionSource`, so the composer spec
  already provides one). TanStack's default retry hides the error arm —
  `retry: false` in every harness client (the mention harness does). jsdom
  sanitises `type="number"` values (irrelevant here; the trigger input is
  text). `[class]` re-sorts tokens — the Automation row's conditional class
  string is NOT byte-pinned by any v4 test; use `[class.opacity-50]` etc.
- **`tanstack-angular-data-lags-invalidate`:** the once-per-nonce effect in
  the popup must not read `rosterQuery.data()` in the same macrotask as the
  refetch it triggered on open; gate on `isFetching() === false` and let the
  effect re-run.
- **Angular template `{{…}}`:** every placeholder-bearing string
  (`{{params.garment}}`, `{{params.x}}`, the `{{…}}` in the hint) is a TS
  constant (`side-effects-section.ts:24-25` precedent).
- **Apostrophes in template expressions** (`Pascal's Table`, `Prospero's Run
  Tool dialog`, `"Honour each item's own replace flag"`): keep them in TS
  constants (the popup's `workbenchTitle` precedent `:468`).
- **The curly quotes** in `describeBenchGesture` (`“…”` U+201C/U+201D) and the
  em dashes in the hint copy — byte-pin them in the spec.
- **`keystone-lane-swarm-gotchas`:** the pascal generator is shared; re-measure
  the corpus row count LAST, after the server lane's rows land.
- **Two typeaheads on `/`?** None today — `@`, `:`, `\` are the only openers;
  but `.` is a smart-typography / text-replacement trigger and `/` is NOT, so
  no priority interaction; still mount the slash plugin ABOVE smart typography
  (v4's order) so a future interaction keeps v4's precedence.
- **The run-tool modal is recreated per open** in v5 (`@if (showRunTool())`),
  so `initialTool` must be read in the constructor's effect after the query's
  first success — v4's "re-applied on every open" is automatic; keep the nonce
  anyway so a `/roll` while the modal is ALREADY open (the sidebar opened it)
  re-selects as v4 does.
- **`customToolsAvailable`** is derived from the roster query in v5's popup,
  not passed from the Salon; the slash host must read the SAME derivation (a
  public computed on the popup or the host's own observer over the same key)
  — `/pascal` and the customs list must agree with the wand's visibility.
- **`parameters: null` on the immediate custom run:** v4 sends `null`; v5's
  `runCustomTool` args type is `parameters?: Record<…>` — the Rust verb reads
  `Option<Map>` (`api/custom_tools.rs:560-581`, `.nullish()`), so OMIT the key
  (the ergonomic equivalent) and record it.
- **`lane-gate-hygiene`:** the Playwright beats that WRITE (a definition save,
  a settings toggle) must restore state; `settings-chat-cards-flow` already
  toggles things back — follow it.
- **Zone-less timing:** the trigger field's Enter handler must
  `preventDefault()` BEFORE the editor's keymap sees it — it is a plain
  `<input>`, not the editor, so no conflict; but the `wb-trigger-phrase` input
  sits inside the Workbench form whose Enter may submit — v4 prevents default
  for that reason.

---

## 7. Contract candidates (wire shapes the SPA reads from the server lanes)

1. **`triggers` on a definition** (`QtapCustomTool.triggers?: string[]`, 1–8
   phrases, 3–80 chars, stored as written) — read by the Workbench through the
   file read (`mountFileRead`) and the library's `triggerCount: number`
   (`customToolsLibrary` entry, after `llm`). SPA key name: `triggerCount`.
2. **The wardrobe effect union** on a definition: `effects[i] = { when?;
   target; value } | { when?; wardrobe: { action: 'wear'|'take_off'|
   'clear_slot'; items?: string[] (1–8, ≤200 chars each); mode?: 'wear'|
   'replace'|'add_to_slot'; slot?: WardrobeItemType } }`; the bench's
   `customToolPreview` reply `effects[i]` gains `{ index; wardrobe: { action;
   items: string[]; mode?; slot? } }`; the roster's `references` gains
   `wears?: string[]`, `takesOff?: string[]`, `clearsSlots?: string[]`
   (sorted; verbatim item references).
3. **`WardrobeItemType`** — the slot enum the SPA must mirror for the Slot
   `<select>` (`WARDROBE_SLOT_TYPES` + `WARDROBE_SLOT_META[slot].label` in v4
   `lib/schemas/wardrobe.types.ts`); v5 has a wardrobe SPA (`apps/web/src/app/
   wardrobe/**`, P4.D285/286) whose slot list + labels the Side-effects card
   should IMPORT rather than duplicate — P checks `wardrobe/` for the existing
   constant before minting one (a grep for `WARDROBE_SLOT` in `apps/web/src`
   at order time).
4. **`pascalMeta`** keys: `invokedBy: 'llm' | 'user' | 'trigger'`,
   `triggerPhrase?: string`, `effects[i].target` may be `wardrobe.wear` /
   `wardrobe.take_off` / `wardrobe.clear_slot` with a `WardrobeEffectRecord`
   `next` — the SPA's message row renders `effects` generically today
   (`core-contract.ts:3131-3137`); no renderer keys on `invokedBy` (measured:
   only the contract file mentions it), so the widening is type-only.
5. **The two chat-settings booleans:** `customToolTriggers` (default `false`,
   dead while `customTools === false` — a CLIENT rule; the server stores it
   regardless) and `composerSlashCommands` (default `true`); both through the
   `chatSettingsUpdate` dispatch (`PUT /api/v1/settings/chat` arm `Invalid
   <key> value (must be boolean)`), echoed by `chatSettings`.
6. **The dialog `initialTool` input names:** `initialTool: { name: string;
   asCharacterId: string | null; nonce: number } | null` on the custom-tool
   popup; `initialTool: { toolId: string; nonce: number } | null` on the
   run-tool modal — v4's names, kept.
7. **The accessor:** `injectCharInsertSettings().slashCommands` (C) reading
   `['composerSlashCommands'] ?? true`; after unification P's documented
   `ChatSettingsDto.composerSlashCommands?: boolean` makes the cast redundant
   (leave it — the sibling keys keep theirs).
8. **The built-in tools source:** `toolsList { chatId, includeSchemas: true }`
   → `AvailableTool[]` (`id`, `name`, `description`, `category?`,
   `pluginName?`, `available?`, `unavailableReason?`, `userInvocable?`,
   `parameters?`) — exactly `SlashBuiltInToolSource`; the custom roster
   `chatCustomToolsList` → `CustomToolListing[]` (`name`, `title`,
   `description`, `parameters`, `characterLabel?`, `asCharacterId`) — exactly
   `SlashCustomToolSource` (note v5's `asCharacterId` is REQUIRED on the wire
   where v4's registry tolerates `undefined`; the `?? null` in `customCommand`
   still applies).

---

## 8. NO-PORT evidence

### 8.1 Lexical 0.52 internals (`b2793400f`)

Nine `package.json` bumps + lockfile; `jest.config.ts`'s ESM exemption; three
command-callback TYPE narrowings (`KeyboardEvent | null` → `KeyboardEvent` —
Lexical 0.52's `registerCommand` typing); `ImagePastePlugin.tsx`'s
`'clipboardData' in event` (Lexical now dispatches `PASTE_COMMAND` with a
`ClipboardEvent | InputEvent`); `table-transformer.ts`'s bare `regExpEnd`
(`@lexical/markdown` 0.52 skips multiline transformers whose end regexp is
optional on the live shortcut path — v5 builds tables from markdown on import
through its own dialect parser, `editor/markdown-dialect.ts`, with no live
shortcut runner); `table-transformer.test.tsx` (108) pins that Lexical
behaviour. v5 is ProseMirror (D17): no file corresponds. **NO-PORT.**

### 8.2 Bug 251's caret half (`$codeBlockToEscape`'s element-point arm)

Lexical leaves an ELEMENT-point selection on the `CodeNode` after
`insertLineBreak`; ProseMirror's caret in a `code_block` is ALWAYS a text
position inside the block (`$from.parent.type === code_block`,
`$from.parentOffset`), which `exitCodeBlockOnBlankLine:223-227` already reads
— there is no second caret shape to recognise. v4's own test
(`KeyboardPlugin.test.tsx:104-120`, "Enter after a trailing line break leaves
the block without submitting") maps onto v5's `rich-editor.spec.ts:247-262`,
which already passes. **NO-PORT for the caret; PORT for the trim (§4.5).**

### 8.3 Bug 250 (`GuardedTypeaheadMenuPlugin`, `25b8fab6a`)

The defect is `LexicalTypeaheadMenuPlugin` opening its menu inside a React
`startTransition` whose commit lands AFTER a second editor update has cleared
the trigger, with `closeTypeahead` returning early on `resolution === null`
(the wrapper's header `:5-12`). v5's three typeaheads (`char-typeahead-plugin.ts`,
`mention-typeahead-plugin.ts`, and the slash plugin to come) run `refresh()`
SYNCHRONOUSLY in the plugin view's `update` on every transaction
(`char-typeahead-plugin.ts:352` `update: (view) => refresh(view)`) and close
the menu the moment `liveTrigger` returns null — there is no deferred commit
and no resolution object to orphan. v4's test (`MentionTypeaheadPlugin.test.
tsx:153-167`: two batched seeds, no listbox left behind) can be transcribed as
a REGRESSION case into `mention-typeahead-plugin.spec.ts` (seed `hello @ar`
then `''` without settling between; assert no `#qt-mention-typeahead-listbox`)
— passes today by construction; worth one case, not a port. The `console.debug
('[Typeahead] Closing a menu whose trigger vanished before it opened')` line
has no v5 analogue and is not a log-census obligation (SPA logs are not
captured). **NO-PORT, with the one regression case recommended (C).**

### 8.4 `useChatSettings.ts` / `ChatTabContent.tsx` handler plumbing

v5's cards are self-contained (`ChatSettingsCard.save`); the `handleX` +
prop-threading hunks (1.1.6, 1.1.7, 1.3.14's hook half) have no counterpart —
their BEHAVIOUR (the failure texts, the partial PUT bodies) lands inside the
two cards. **NO-PORT as files; PORT as behaviour.**

### 8.5 `CustomToolsButton.tsx` / `ComposerGutterTools.tsx` / `ChatModals.tsx` / `useModalState.ts` — the dialog's MOVE

v4 moved the custom-tool dialog out of the gutter button into `ChatModals` so
the wand and the `/` menu share ONE instance. v5's `CustomToolsPopup` is
already one instance that hosts both the button and the dialog and lives in
the composer, which is also where the slash host lives — the composer can call
`popup.open(initial)` through `viewChild` with no move. **The move is
NO-PORT; the shared-instance INVARIANT (memory survives a close, one dialog
for two launch points) is PORTED by construction.** `useModalState`'s
`RunToolRequest` becomes a Salon signal (the modal lives there), 
`CustomToolRequest` becomes the popup's own `open(initial)`.

### 8.6 `requestRngDirect` (legacy non-preview RNG)

v5 always previews (`rng-dropdown.ts:53-60`, "v4's legacy branch … has no v5
caller and is NOT ported") — the extraction keeps `requestRngPendingResult`
only. **NO-PORT (standing).**

### 8.7 `queryKeys.tools.byChat` + the ESLint widening

v5 has no central keys module (keys are per feature,
`custom-tools.api.ts:27-30`) and no ESLint (§2.4). The slash host reuses the
run-tool modal's `['tools', chatId, 'schemas']`. **NO-PORT as files.**

---

## 9. What the ledger rows got wrong (or left out)

1. **`4877fc2da` row — "the custom-tool dialog moved into `ChatModals`" is
   stated as a v5 obligation by implication; the hunks show the move exists
   only to share one instance between two launch points, which v5's
   `CustomToolsPopup` already is (§8.5). The port is `initialTool` + an
   `open()` method, not a relocation.**
2. **`4877fc2da` row — `queryKeys.tools.byChat` "a read of `?chatId=…&
   includeSchemas=true`":** v5's run-tool modal ALREADY issues exactly that
   read under `['tools', chatId, 'schemas']` (`run-tool-modal.ts:282-285`);
   the row lists the key as new work — it is a reuse.
3. **`b2793400f` row — the `"a\n  \n"` claim is right, but the row's
   algorithm gloss ("removes trailing whitespace-only text nodes then ONE line
   break") understates one case:** `"a\n  "` (whitespace last line with NO
   trailing newline — the blank-line condition `lines[last].trim() === ''`
   holds) → the text node `'  '` is removed, then `last` is the line break
   after `a`, removed too → `"a"`; both old and new agree there. The
   divergence needs a whitespace line BEFORE the blank one (§1.4.2, §4.5).
4. **`b2793400f` row — the v5 docstring claim is FALSE "for the old regex" —
   confirmed by measurement** (`/\n\s*$/`: `\s` matches `\n`, so the match
   starts at the leftmost newline of the trailing whitespace run). The row is
   right; this survey adds the vectors.
5. **`362426c8f` row — "a NEW trigger-phrases field" under Workbench SPA**
   omits that `tool-draft.ts` IMPORTS `foldTriggerText` /
   `triggersContainedIn` from `lib/pascal/trigger-phrases.ts`, which imports
   `stripCarinaQueryLines` — so the SPA needs a `trigger-phrases.ts` twin AND
   `chat/carina-parser.ts` grows (`stripCarinaQueryLines`, `parseQueryLine`)
   with its corpus (§3.1); the row files the Carina hunk under the Rust
   `carina_parser.rs` only.
6. **`362426c8f` row — `AutomationSettings.tsx` "rewritten as a row table"**
   — correct, and v5's `automation-settings.ts` is the PRE-commit shape (the
   `: false` arm the row calls "the extension point"); the row does not note
   that the `disabled` row gets a DIFFERENT class string (`opacity-50
   cursor-not-allowed` replaces `qt-hover-accent cursor-pointer`), which the
   spec should pin.
7. **`f0db8cbb8` row — "the SPA's Side effects card Kind select, the Proving
   Bench 'would attempt', the run dialog"** — the row omits `tool-draft.ts`'s
   +159 as SPA-twin work (listed under the server "Workbench draft check")
   and the bench's `DryEffect` union widening in `workbench.api.ts`; both are
   P's.
8. **`25b8fab6a` row — "Bug 250: v5's SPA typeaheads … — none"** — agreed, with
   the evidence in §8.3 (synchronous `refresh` in the plugin view's `update`);
   the row did not say WHY. One regression case is recommended.
9. **`8489d08b9` row** — the About cites are exact (`:326/:330/:338/:346`,
   `about.spec.ts:116`); nothing to add.
10. **`a42c282b8` row** — exact (`memory-card.ts:70`, spec `:253`); the spec
    ALSO asserts `hover:underline` at `:254` — both lines change.
11. **`01f508d4e` row** — the v5 twin is `wardrobe/wardrobe-item-actions.ts:
    231-237` (not under `components/`); its spec SPIES on the debug line
    (`:150-166`) and must drop the assertion with the call.
12. **The ledger does not record that `ChatSettingsDto` carries an index
    signature** (`core-contract.ts:3871`) — which is what makes the two-lane
    split of the settings keys possible without a shared type edit (§3.3
    point 2).
13. **Line numbers:** the `362426c8f` row's `types.ts` placement
    ("`customToolTriggers` IMMEDIATELY after `customTools`") is the SCHEMA
    (`lib/schemas/settings.types.ts`); in the SPA `types.ts` it is `:100-101`
    after `customTools` `:98-99` — consistent, just a different file.
