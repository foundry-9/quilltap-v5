# Survey — Pascal wardrobe effects, the SERVER side of `f0db8cbb8`

Read-only planning survey, 2026-10-16. v4 = `~/source/quilltap-server` read
ONLY through `git show f0db8cbb8:<path>`, `git show 25b8fab6a:<path>` (the PIN,
v4 main HEAD, `4.11.0-dev.10`, clean) and `git diff f0db8cbb8^ f0db8cbb8 --
<paths>`; v5 = `main` at `56554d98c` (`/Volumes/External 1/source_overflow/
quilltap-v5`; `~/source/quilltap-v5` symlinks to it). **Every v4 line number
below is AT THE PIN `25b8fab6a`** — and `git diff --stat f0db8cbb8 25b8fab6a --
<every server path in scope>` is **EMPTY**, so for this slice the pin IS
`f0db8cbb8`: nothing in `lib/pascal/**`, `lib/tools/run-custom-tool.ts`,
`lib/tools/handlers/{run-custom,wardrobe-take-off}-handler.ts`,
`lib/chat/tool-executor.ts`, `lib/wardrobe/wear-ops.ts`, the two row/event
schemas, `public/schemas/*.json` or `docs/developer/CUSTOM_TOOL_SPEC.json` moved
in the six commits after it (`6069a7acd` bug 249, `4877fc2da` slash commands,
`b93c2efda`, `eb5d42338`, `b2793400f`, `25b8fab6a` touch none of them). Every
v5 line number is at `56554d98c`. Port facts come from SHIPPED HUNKS, never the
commit message (ledger §5.3); the ledger's §3 row for `f0db8cbb8`
(`drift-ledger.md:317`) was read first and is built on, not restated — where a
hunk disagrees with it, §9 says so.

Scope: the server side of `f0db8cbb8` — `lib/pascal/custom-tool.types.ts`
(+233), `custom-tools.ts` (+92), `wardrobe-effects.ts` (269, NEW),
`side-effects.ts` (+79), `tool-draft.ts` (+159), `tool-vocabulary.ts` (+33),
`run-service.ts` (+12), `lib/tools/handlers/run-custom-handler.ts` (+8),
`lib/tools/run-custom-tool.ts` (+31, MODEL-VISIBLE), `lib/chat/tool-executor.ts`
(+1), `chats-messages.ops.ts` + `chat.types.ts` (comment-only),
`wardrobe-take-off-handler.ts` (+16, the composite FIX), `lib/wardrobe/
wear-ops.ts` (+73), `public/schemas/qtap-custom-tool.schema.json` (+102),
`qtap-export.schema.json` (one description), `CUSTOM_TOOL_SPEC.json`, and the
eight test files. **Out of scope** (other surveys): `components/**` (the SPA
survey — named here only where a contract crosses to it), `help/` (the vendor
survey), and `362426c8f`'s run service itself (the trigger-phrases survey —
this lane only ASSUMES it, §8).

---

## 0. The span in one paragraph

`f0db8cbb8` makes a custom-tool effect EITHER a value write (`target` +
`value`, both now `.optional()` on the Zod object with a `superRefine` that
demands exactly one half) OR a wardrobe gesture `{ wardrobe: { action: 'wear'
| 'take_off' | 'clear_slot', items?, mode?, slot? } }` on the ROLLING character
(`custom-tool.types.ts:637-787`), with the per-action rules in ONE function
(`wardrobeGestureProblems`, `:763-787`) that load-time validation
(`validateWardrobeGesture`, `:1342-1370`) and the Workbench draft check
(`tool-draft.ts:1354-1387`) both read. The pure core renders each item
reference as a TEMPLATE with the outcome message's subjects
(`resolveWardrobeGesture`, `custom-tools.ts:1249-1282` — trimmed, empties
dropped with a DEBUG, an unresolved placeholder left as written), emitting a
third `ResolvedEffect` arm `{ index, wardrobe: ResolvedWardrobeGesture }`.
The NEW applier `lib/pascal/wardrobe-effects.ts` runs AFTER the value writes
commit (`side-effects.ts:145-170` splits the old function into
`applyWriteEffects` + a wardrobe pass, then merges the two applied lists back
into effect order by a stable index sort), loads ONE pool (`loadToolPool`) +
the current slots once, and takes each gesture one at a time through
`resolveWearable` / `wearItem` (source `'tool'`) / the NEW `takeOffWearable` /
`applyDisplacement({ mode: 'clear_slot' })`, skipping unresolvable, archived
(wear only) or slot-mismatched items per item with a DEBUG, dropping a thrown
commit per item with a WARN, recording one `AppliedEffect` per change that
actually changed the slots (`target` `wardrobe.wear` / `wardrobe.take_off` /
`wardrobe.clear_slot`, `previous` = the slots before, `next` = a
`WardrobeEffectRecord`), and calling `notifyWardrobeChanged` ONCE after the
loop iff anything landed — joining the turn's `pendingWardrobeAnnouncements`
set when the run came through the model's `run_custom` (threaded
`tool-executor.ts:625` → `run-custom-handler.ts:56,190` →
`run-service.ts:80,162` → `side-effects.ts:135,161`), enqueuing at once
otherwise. The roster's `Side effects:` line becomes `describeEffects`
(`run-custom-tool.ts:266-281`) and the preamble sentence gains `, or what they
are wearing` (`:91`) — model-visible bytes. The vocabulary gains three sorted
lists (`wears` / `takesOff` / `clearsSlots`). The same commit FIXES
`wardrobe_take_off` on a composite: worn composites are stored as their
leaves, so filtering the composite's own id out of its `types` slots removed
nothing — the NEW `takeOffWearable` (`wear-ops.ts:159-187`) takes a composite
off by its component graph's leaves and exports `loadCurrentSlots`
(`:194-201`); **v5 shares that bug** (`tools/wardrobe_take_off.rs:244-256`).
No DDL, no migration, no D23 move: `pascalMeta.effects[].next` is `z.unknown()`
in both the row and event schemas, and v5 reads `pascalMeta` opaque
(`db/chats_messages_read.rs:231`).

---

## 1. v4 at the pin — the hunks, file by file

### 1.1 `lib/pascal/custom-tool.types.ts` (+233)

**Imports** (`:37-38`): `scanPlaceholders as scanPlaceholdersInText` from
`./placeholders`; `WardrobeItemTypeEnum, type WardrobeItemType` from
`@/lib/schemas/wardrobe.types` (the five slots `top` / `bottom` / `footwear` /
`accessories` / `hair`, in that order — the published schema's enum at
`qtap-custom-tool.schema.json` `$defs.WardrobeGesture.properties.slot.enum`
spells them out).

**Constants** (`:100`, `:103`):
```ts
export const MAX_WARDROBE_EFFECT_ITEMS = 8;
export const MAX_WARDROBE_ITEM_REF_LENGTH = 200;
```

**Enums** (`:637-642`):
```ts
export const WardrobeEffectActionSchema = z.enum(['wear', 'take_off', 'clear_slot']);
export const WardrobeEffectModeSchema = z.enum(['wear', 'replace', 'add_to_slot']);
```

**`CustomToolWardrobeGestureSchema`** (`:655-676`) — `z.strictObject`:
- `action: WardrobeEffectActionSchema` — describe: `"wear" puts the listed items on the rolling character; "take_off" takes them off; "clear_slot" empties one slot.`
- `items: z.array(z.string().min(1).max(200)).min(1).max(8).optional()` — describe: `The garments or outfits, each by title (case-insensitive) or id, in order. A template: {{params.x}}, {{metadata.key}} and the other placeholder families are substituted before lookup. Required for wear and take_off.`
- `mode: WardrobeEffectModeSchema.optional()` — describe: `wear only. Same meaning as the wardrobe_wear tool: honour the item's replace flag (default), force a swap, or layer into one slot.`
- `slot: WardrobeItemTypeEnum.optional()` — describe: `Required for clear_slot and for mode add_to_slot; optional on take_off to restrict the removal to one slot.`

Note what the Zod shape DOES and DOES NOT refuse: `items: []` fails `.min(1)`
(a hard array issue at `effects.N.wardrobe.items`); `items` of 9 fails
`.max(8)`; a 201-char reference fails `.max(200)`; an unknown key under
`wardrobe` (`who: 'Bob'`) fails strict; an unknown `action` / `slot` fails the
enum. The per-action cross-field rules are NOT here (see `validateEffects`).

**`CustomToolEffectSchema`** (`:690-741`) — `z.strictObject({ when?, target?,
value?, wardrobe? }).superRefine(...)`:
- `target` is now `.optional()`; its describe gains the trailing sentence
  `A write needs both target and value.` (the rest byte-identical to before).
- `value` is now `.optional()` (describe unchanged).
- `wardrobe: CustomToolWardrobeGestureSchema.optional()` — describe: `A wardrobe gesture on the rolling character instead of a write: wear items, take them off, or clear a slot.`
- `superRefine` (`:721-741`), issues all `code: 'custom'`:
  | condition | message | path |
  |---|---|---|
  | `isWrite && isWardrobe` (`isWrite` = `target !== undefined \|\| value !== undefined`) | `an effect is either a write (target + value) or a wardrobe gesture, not both` | `[]` (the effect) |
  | `!isWrite && !isWardrobe` | `an effect needs target + value, or wardrobe` | `[]` |
  | `isWrite && (target === undefined \|\| value === undefined)` | `a write needs both target and value` | `['target']` if target is missing, else `['value']` |
  Formatted through `formatDefinitionIssues` these read `effects.0: an effect is
  either … not both`, `effects.0: an effect needs target + value, or wardrobe`,
  `effects.0.value: a write needs both target and value` (the test regexes in
  §1.13 pin the message text, not the prefix).
- The `effects` array's own describe on `QtapCustomToolSchema` (`:1028-1030`)
  gains `, or wardrobe gestures that change what the rolling character is wearing.` after `…rolling character's metadata`.

**Types + predicates** (`:743-760`):
```ts
export type CustomToolWriteEffect = CustomToolEffect & { target: string; value: number | boolean | string };
export type CustomToolWardrobeEffect = CustomToolEffect & { wardrobe: CustomToolWardrobeGesture };
export function isWriteEffect(effect): effect is CustomToolWriteEffect   // wardrobe === undefined && target !== undefined && value !== undefined
export function isWardrobeEffect(effect): effect is CustomToolWardrobeEffect // wardrobe !== undefined && target === undefined && value === undefined
```
Both are STRICT (a both-halves effect is neither) — only reachable post-parse
on a shape the refine already rejected, but the predicates are what every
downstream reader uses (`run-custom-tool.ts`, `custom-tools.ts`).

**`wardrobeGestureProblems(gesture)`** (`:763-787`) — input `Pick<…, 'action' |
'mode' | 'slot'> & { items?: readonly string[] }`, returns `Array<{ field:
'items' | 'mode' | 'slot'; message }>` in THIS order per action
(`hasItems = (items?.length ?? 0) > 0`):
| action | checks, in order | message |
|---|---|---|
| `wear` | `!hasItems` | `wear needs at least one item` (field `items`) |
| | `mode ?? 'wear'` is `add_to_slot` and `slot === undefined` | `mode add_to_slot needs a slot` (`slot`) |
| | else if mode is NOT `add_to_slot` and `slot !== undefined` | `slot is only used with mode add_to_slot` (`slot`) |
| `take_off` | `!hasItems` | `take_off needs at least one item` (`items`) |
| | `mode !== undefined` | `mode is only used with wear` (`mode`) |
| `clear_slot` | `slot === undefined` | `clear_slot needs a slot` (`slot`) |
| | `items !== undefined` | `clear_slot takes no items` (`items`) |
| | `mode !== undefined` | `mode is only used with wear` (`mode`) |
(`clear_slot` with `items: []` is `items !== undefined` → `takes no items` at
load; the draft check rewrites `[]` to `undefined` first — §1.5.)

**`validateEffects`** (`:1269-1333`) reorders: `when` subjects are now checked
FIRST (`validateWhenSubjects` at `[...base, 'when']`, moved up from the tail),
then `if (effect.wardrobe !== undefined) { validateWardrobeGesture(declared,
Boolean(tool.llm), effect.wardrobe, ctx, [...base, 'wardrobe']); return; }`,
then `if (effect.target === undefined || effect.value === undefined) return;`
(the refine already spoke), then the unchanged target/value checks. **Issue
ORDER within one effect changed for writes too** (when-issues now precede
target-issues) — a byte-level fact `formatDefinitionIssues` exposes (§4.1).

**`validateWardrobeGesture`** (`:1342-1370`): every `wardrobeGestureProblems`
entry becomes `ctx.addIssue({ code: 'custom', message, path: [...path,
field] })` (so `effects.0.wardrobe.slot: mode add_to_slot needs a slot`); then
for each `items[m]`, for each `scanPlaceholdersInText(item)` ref:
`ref.kind === 'params' && !declared[ref.name]` → `item references undeclared
parameter "<name>"` at `[...path, 'items', m]`; `ref.kind === 'llm' &&
!hasLlmBlock` → ``item references {{llm}}, but the tool declares no `llm` block``
at the same path. An `unknown` family is NOT rejected (the `renderTemplate`
doctrine: left as written, fails to resolve at the table).

### 1.2 `lib/pascal/custom-tools.ts` (+92)

**Imports** (`:49-54`): `isWardrobeEffect`, types `CustomToolWardrobeGesture`,
`WardrobeEffectAction`, `WardrobeEffectMode`, `WardrobeItemType`.

**`ResolvedEffect`** (`:1144-1148`) gains the middle arm:
```ts
export type ResolvedEffect =
  | { index: number; target: EffectTarget; value: ExprValue }
  | { index: number; wardrobe: ResolvedWardrobeGesture }
  | { index: number; skipped: string };
export interface ResolvedWardrobeGesture {           // :1150-1156
  action: WardrobeEffectAction;
  /** Rendered, trimmed item references in file order. Empty strings are dropped here, with a debug log. */
  items: string[];
  mode?: WardrobeEffectMode;
  slot?: WardrobeItemType;
}
```
Serialized key order (the Workbench preview spreads the run result, the
execution oracle emits it): `{ index, wardrobe: { action, items, mode?, slot? } }`
— `items` is ALWAYS present (`[]` on `clear_slot`); `mode` / `slot` ride only
when defined (`...(gesture.mode !== undefined ? { mode } : {})`).

**Predicates** (`:1159-1178`): `isApplicableWriteEffect` (`'target' in effect`),
`isApplicableWardrobeEffect` (`'wardrobe' in effect`), and **`isApplicableEffect`
CHANGES MEANING** — it was "would write"; it is now "either kind would apply"
(`isApplicableWriteEffect || isApplicableWardrobeEffect`) — the Proving Bench's
count. v5's `ResolvedEffect::is_applicable` (`custom_tools.rs:1213`) is the OLD
meaning (§2.2).

**`resolveEffects`** (`:1190-1247`): after the `when` gate, `if
(isWardrobeEffect(effect)) return resolveWardrobeGesture(definition.name,
index, effect.wardrobe, subjects, skipped);` then a regression guard `if
(effect.target === undefined || effect.value === undefined) return
skipped(index, 'effect is neither a complete write nor a wardrobe gesture');`
(unreachable past load validation; records the reason anyway).

**`resolveWardrobeGesture`** (`:1249-1282`): for each raw ref in `gesture.items
?? []`: `renderTemplate(raw, subjects).trim()`; empty → DEBUG
`Custom tool wardrobe item reference rendered empty; dropped` with
`{ context: 'pascal.custom-tools', tool, effectIndex, reference: raw }` and
`continue`; else push. Then `if (gesture.action !== 'clear_slot' &&
items.length === 0) return skipped(index, 'every item reference rendered
empty')` (the shared `skipped` helper ALSO logs its own DEBUG `Custom tool
effect skipped` — v5 `custom_tools.rs:1303-1311` already captures that line).
Returns `{ index, wardrobe: { action, items, ...mode, ...slot } }`. The
`subjects` are the FULL `EffectSubjects` (value / roll / dice / params /
metadata / progress / now / llm / state) — `{{now}}` renders in an item
reference exactly as in an effect expression.

### 1.3 `lib/pascal/wardrobe-effects.ts` — NEW, 269 lines (the applier)

Read whole at `f0db8cbb8` = the pin. Line numbers below are the file's own.

- `:33-51` imports: `logger`, `getErrorMessage`, `getRepositories`,
  `WARDROBE_SLOT_TYPES`, `applyDisplacement` (`outfit-displacement`),
  `notifyWardrobeChanged` (`outfit-change-effects`), `{ findInPool,
  loadCurrentSlots, resolveWearable, takeOffWearable,
  wardrobeItemNotFoundMessage, wearItem }` (`wear-ops`), `loadToolPool`
  (`tools/handlers/wardrobe-handler-shared`), types `ResolvedWardrobeGesture`
  (`./custom-tools`), `AppliedEffect` (`./side-effects`).
- `:53` `const CONTEXT = 'pascal.wardrobe-effects';`
- `:56` `export type WardrobeEffectTarget = 'wardrobe.wear' | 'wardrobe.take_off' | 'wardrobe.clear_slot';`
- `:59-68` **`WardrobeEffectRecord`** — `{ item?: { id, title }; slot?: WardrobeItemType; slotsAffected: WardrobeItemType[]; effect: 'layered' | 'replaced' | 'removed' | 'cleared'; slots: EquippedSlots }` — the `next` of a wardrobe `AppliedEffect`. Emitted key order per site (object literals): wear `{ item, slot?, slotsAffected, effect, slots }` (`:184-190`, `slot` only when `mode === 'add_to_slot' && gesture.slot`); take_off `{ item, slot?, slotsAffected, effect: 'removed', slots }` (`:207-213`, `slot` when `gesture.slot`); clear_slot `{ slot, slotsAffected: [slot], effect: 'cleared', slots }` (`:167`).
- `:71-74` `IndexedAppliedEffect { index; applied: AppliedEffect }`.
- `:76-85` **`ApplyWardrobeEffectsParams`** `{ userId; chatId; toolName; characterId: string | null; gestures: Array<{ index; wardrobe: ResolvedWardrobeGesture }>; pendingWardrobeAnnouncements?: Set<string> }`.
- `:92-244` **`applyWardrobeEffects(params): Promise<IndexedAppliedEffect[]>`** — never throws:
  1. `gestures.length === 0` → `[]` (no log).
  2. `!characterId` → DEBUG `Custom tool: no rolling character, wardrobe effects skipped` `{ context, chatId, tool, gestures: <count> }` → `[]`. (`loadToolPool` NOT called — pinned by the test.)
  3. `pool = await loadToolPool(repos, chatId, characterId); slots = await loadCurrentSlots(repos, chatId, characterId)` in ONE try; a throw → WARN `Custom tool wardrobe effects skipped — the wardrobe could not be read` `{ context, chatId, tool, characterId, error }` → `[]`.
  4. `skip(index, reason, detail)` → DEBUG `Custom tool wardrobe effect not applied` `{ context, chatId, tool, characterId, effectIndex, reason, ...detail }`.
  5. `land(index, target, previous, record)` → if `sameSlots(previous, record.slots)` → `skip(index, 'the outfit was already so', { item: record.item?.title, slot: record.slot })` and NOTHING recorded; else `applied.push({ index, applied: { target, previous, next: record } })`. **`previous` is always present on a wardrobe entry** (never `undefined`), and there is NO `tier` key.
  6. Loop over gestures IN ORDER; `slots` is threaded gesture-to-gesture (`slots = outcome.slots`) — "each sees the last":
     - `clear_slot`: `!slot` → `skip(index, 'clear_slot names no slot')`; else `applyDisplacement(repos, chatId, characterId, { mode: 'clear_slot', slot })` (NO source argument — the chokepoint stamps take-off itself); a throw → `warnFailed(…, { slot }, error)`; else `land(index, 'wardrobe.clear_slot', previous, { slot, slotsAffected: [slot], effect: 'cleared', slots })`. `items` are IGNORED on `clear_slot`.
     - `wear` (per `ref` in `gesture.items`): `mode = gesture.mode ?? 'wear'`; `resolveWearable(pool, { itemId: ref, itemTitle: ref }, mode, gesture.slot)` — **the same string is tried as id THEN title** (`findInPool`: `pool.get(ref)` first, then `pool.findByTitle(ref)` case-insensitive); refusal → `skip(index, resolution.message, { reference: ref })` with v4's three sentences verbatim (`Wardrobe item not found with ID "<ref>" with title "<ref>"` — BOTH clauses, since both fields carry the ref; `Item "<title>" is archived and cannot be worn`; `Item "<title>" (types: a, b) cannot be added to the "<slot>" slot`); else `wearItem(repos, chatId, pool, item, mode, gesture.slot, 'tool')`; a throw → `warnFailed(…, { item: item.title }, error)`; else `land(index, 'wardrobe.wear', previous, { item: { id, title }, ...(mode === 'add_to_slot' && gesture.slot ? { slot } : {}), slotsAffected: outcome.slotsAffected, effect: outcome.effect, slots })`.
     - `take_off` (per `ref`): `findInPool(pool, { itemId: ref, itemTitle: ref })` — archived items ARE found (archiving never undresses); `null` → `skip(index, wardrobeItemNotFoundMessage(ref, ref), { reference: ref })`; else `takeOffWearable(repos, chatId, pool, item, gesture.slot)`; throw → `warnFailed(…, { item: item.title }, error)`; else `land(index, 'wardrobe.take_off', previous, { item, ...(gesture.slot ? { slot } : {}), slotsAffected: outcome.slotsAffected, effect: 'removed', slots })`.
  7. `if (applied.length > 0)`: `await notifyWardrobeChanged(repos, { userId, chatId, characterId, ...(pending ? { pendingWardrobeAnnouncements } : {}) }, CONTEXT)` — ONE call, `sourceContext` = `'pascal.wardrobe-effects'` (so the avatar trigger's / scheduler's own lines carry `context: 'pascal.wardrobe-effects'`); then DEBUG `Custom tool wardrobe effects applied` `{ context, chatId, tool, characterId, applied: [targets…], announcement: 'joined the turn' | 'enqueued now' }`.
- `:246-261` `warnFailed(chatId, toolName, index, detail, error)` → WARN `Custom tool wardrobe effect failed; that change was dropped` `{ context, chatId, tool, effectIndex, ...detail, error: getErrorMessage(error) }`.
- `:263-269` `sameSlots(a, b)` — every `WARDROBE_SLOT_TYPES` slot's arrays equal by length and element, missing key = `[]`.

Behaviours to carry that the header states and the tests pin: **sequential
gestures, never batched** (the wear ledger credits one wear per committed
transition); **source `'tool'`** on every put-on (`applyDisplacement`'s fifth
argument — the test asserts `call[4] === 'tool'`); **one pool per run**;
**fail-soft per item, never per run**.

### 1.4 `lib/pascal/side-effects.ts` (+79)

- Header gains `## Wardrobe gestures go after the writes` (`:53-61`): a
  `wardrobe` effect is applied by `applyWardrobeEffects` AFTER every write has
  committed, so a `metadata.` write and a gesture in one tool never interleave
  their store reads; the two applied lists are merged back into effect order.
- Imports (`:76-77`): `isApplicableWardrobeEffect, isApplicableWriteEffect` replace `isApplicableEffect`; `applyWardrobeEffects` from `./wardrobe-effects`.
- `AppliedEffect` (`:92-106`) — doc only: `target` may be `wardrobe.wear` / `wardrobe.take_off` / `wardrobe.clear_slot`, then `previous` / `next` are the slots before and a `WardrobeEffectRecord`. The shape (`target`, `previous?`, `next`, `tier?`) is unchanged.
- `ApplyCustomToolEffectsParams` (`:108-136`) gains **`userId: string`** (`:109`, REQUIRED — `notifyWardrobeChanged` needs it) and `pendingWardrobeAnnouncements?: Set<string>` (`:135`).
- **`applyCustomToolEffects`** (`:145-170`):
  ```ts
  const writes = await applyWriteEffects(params, effects.filter(isApplicableWriteEffect));
  const gestures = effects.filter(isApplicableWardrobeEffect);
  const changes = gestures.length ? await applyWardrobeEffects({ userId, chatId, toolName, characterId, gestures, ...(pending ? { pendingWardrobeAnnouncements } : {}) }) : [];
  if (changes.length === 0) return writes.map((w) => w.applied);
  return [...writes, ...changes].sort((a, b) => a.index - b.index).map((w) => w.applied);
  ```
  `applyWardrobeEffects` is called ONLY when a gesture is applicable (the test
  `never calls the wardrobe applier for a run with no gestures`); the merge is
  a STABLE sort by effect index (entries of one effect keep their own order;
  V8's `Array.prototype.sort` is stable — a write at index 1 sits between two
  wear records at index 0 and a clear at index 2: `['wardrobe.wear',
  'wardrobe.wear', 'state.count', 'wardrobe.clear_slot', 'metadata.k']`).
- **`applyWriteEffects`** (`:176-365`) is the old body verbatim, except
  `pending` entries carry `index` (`:204`, `:220`, `:249`, `:284`) and the
  return is `applied.map((p) => ({ index: p.index, applied: p.entry }))`; the
  `Custom tool effects applied` DEBUG still lists `applied.map((p) =>
  p.entry.target)` (writes only — the wardrobe pass logs its own line).

### 1.5 `lib/pascal/tool-draft.ts` (+159) — the Workbench draft (browser-shared; v5's twin is the SPA's `apps/web/src/app/pascal/tool-draft.ts`, §2.9)

- `DraftEffect` (`:215-225`) gains `kind: 'write' | 'wardrobe'` and `wardrobe: DraftWardrobeGesture` — BOTH halves always present; only the active half is emitted. `target` / `valueKind` / `value` stay.
- `DraftWardrobeGesture` (`:227-232`) `{ action; items: string[]; mode: WardrobeEffectMode; slot: WardrobeItemType | '' }` — `mode` `'wear'` and `slot` `''` are the omitted forms.
- `emptyDraftWardrobeGesture()` (`:236-238`) → `{ action: 'wear', items: [''], mode: 'wear', slot: '' }`.
- `newDraftEffect(kind = 'write')` (`:241-251`) → `{ id: nextDraftId('effect'), when: { kind: 'always' }, kind, target: '', valueKind: 'expression', value: '', wardrobe: emptyDraftWardrobeGesture() }`.
- `effectToDraft` (`:523-572`): a `wardrobe` effect → `kind: 'wardrobe'`, `target: ''`, `valueKind: 'expression'`, `value: ''`, `wardrobe: { action, items: items?.length ? [...items] : [''], mode: mode ?? 'wear', slot: slot ?? '' }`; a write → `kind: 'write'`, `target: effect.target ?? ''`, `value: effect.value === undefined ? '' : String(effect.value)`, `wardrobe: emptyDraftWardrobeGesture()`.
- `wardrobeFromDraft` (`:574-590`): trims + drops blank items; `wear` → `{ action, items, ...(mode !== 'wear' ? { mode } : {}), ...(mode === 'add_to_slot' && slot ? { slot } : {}) }`; `take_off` → `{ action, items, ...(slot ? { slot } : {}) }`; `clear_slot` → `{ action, ...(slot ? { slot } : {}) }`. Fields an action does not use are LEFT OUT (the form hides them).
- `effectFromDraft` (`:592-610`): `kind === 'wardrobe'` → `{ ...(when ? { when } : {}), wardrobe: wardrobeFromDraft(…) }`.
- `validateDraftEffects` (`:1294-1345`): the `verbatim`-when LLM check MOVES to the top of the loop (before the kind branch); `kind === 'wardrobe'` → `validateDraftWardrobeGesture(…)` + `continue`.
- `validateDraftWardrobeGesture` (`:1354-1387`): `emitted = wardrobeFromDraft(effect.wardrobe)`; `wardrobeGestureProblems({ ...emitted, items: emitted.items?.length ? emitted.items : undefined })` → each `err(where, problem.message)` (the LOADER's words — `wear needs at least one item`, `mode add_to_slot needs a slot`, `clear_slot needs a slot`; a `clear_slot` row with typed items is CLEAN because the emitter dropped them); then per raw item (`effect.wardrobe.items`, untrimmed) per `scanPlaceholders` ref: `params` undeclared → `err` `` {{<key>}} names no declared parameter ``; `llm` without consult → `err` `{{llm}} needs the LLM consult enabled`; `unknown` → `warn` `` {{<key>}} is not a placeholder this build knows — that item will not be found ``.
- `renameParameterEverywhere` (`:1716-1757`) maps `effects[*].wardrobe.items` through the same `placeholder → {{params.<to>}}` replace; `findParameterReferences` (`:1759-1796`) adds `` `effect ${index + 1}: a wardrobe item names it` `` for a `wardrobe` row whose any item matches `paramPlaceholderPattern(name)`.

### 1.6 `lib/pascal/tool-vocabulary.ts` (+33)

- `ToolVocabulary` (`:86-96`) gains `wears: string[]` (item references a `wear` gesture may PUT ON, verbatim — a `{{params.garment}}` stays as written; sorted), `takesOff: string[]`, `clearsSlots: string[]` — appended AFTER `now` in the interface AND in the returned object (`:221-223`: `now, wears, takesOff, clearsSlots` — key order is wire-visible on the run dialog's vocabulary payload).
- `isEmptyVocabulary` (`:99-117`) ANDs the three new lengths.
- `collectToolVocabulary` (`:124-225`): per effect, the existing `value` / `when.metadata` / `when.progress` reads run FIRST; then `if (effect.wardrobe) { const { action, items = [], slot } = effect.wardrobe; for (const item of items) collectPlaceholders(item, declared, found); if (action === 'wear') items → found.wears; else if (action === 'take_off') items → found.takesOff; else if (slot) found.clearsSlots.add(slot); continue; }` then `if (effect.target === undefined) continue;` before `parseEffectTarget`. Item templates are scanned like any rendered string (`{{params.garment}}` lands in `params`, `{{metadata.favouriteHat}}` in `metadata`); a gesture is NOT a write (`stateWrites` / `metadataWrites` untouched). `sorted` is `localeCompare` (the existing helper) — `['{{params.garment}}', 'Cloak of Night']` (brace sorts before letters under en-US collation).

### 1.7 `lib/pascal/run-service.ts` (+12) — `362426c8f`'s file, extended

`RunCustomToolParams.pendingWardrobeAnnouncements?: Set<string>` (`:76-80`);
`applyCustomToolEffects` is now called with `userId` (`:154`) and the set
spread in when present (`:162-164`); the comment at `:150-151` reads "`characterId`
null → metadata, progress and wardrobe effects skip". Everything else in the
file is `362426c8f`'s (§8).

### 1.8 `lib/tools/handlers/run-custom-handler.ts` (+8) + `lib/chat/tool-executor.ts` (+1)

`RunCustomToolContext.pendingWardrobeAnnouncements?: Set<string>` (`:51-56`,
doc: "so a tool's `wardrobe` effect joins the turn's single Aurora
announcement alongside any `wardrobe_wear` call"); `executeRunCustomTool`
spreads it into `runCustomTool({...})` (`:190-192`). The executor's `run_custom`
arm passes `pendingWardrobeAnnouncements: context.pendingWardrobeAnnouncements`
(`tool-executor.ts:625`) — the SAME `Set` the seven wardrobe tools receive
(`ToolExecutionContext.pendingWardrobeAnnouncements`, `:176-186`). The two
OTHER entrances — the operator's popup route (`app/api/v1/chats/[id]/
custom-tools/route.ts:451` `runCustomTool({…})`) and the trigger runner
(`lib/pascal/trigger-runner.ts:165`) — pass NO set (measured by grep at the
pin), so their wardrobe effects enqueue the announcement at once.

### 1.9 `lib/tools/run-custom-tool.ts` (+31) — MODEL-VISIBLE

- Imports `isWardrobeEffect, isWriteEffect` (`:21-22`).
- `RUN_CUSTOM_PREAMBLE` line 6 (`:91`) — the ONE changed sentence, verbatim:
  > `Some tools record side effects when they run — adjusting the scene's persistent state, the rolling character's own records, or what they are wearing, server-side, as part of the roll.`
  (was `…persistent state or the rolling character's own records, server-side…` — v5 `run_custom.rs:106` carries the OLD sentence).
- `buildRunCustomDescription` (`:188-233`): the roster's effects line
  (`:227`) is `` `    Side effects: ${describeEffects(definition.effects)}` ``
  — still only under `revealOdds !== false` and only when
  `definition.effects.length > 0`.
- **`describeEffects(effects)`** (`:266-281`) — the exact rendering rules:
  1. `targets = [...new Set(effects.filter(isWriteEffect).map((e) => e.target))]` — write targets, deduplicated, first-occurrence order; if any → first clause `` `writes ${targets.join(', ')}` ``.
  2. For each effect in file order that `isWardrobeEffect`: `{ action, items = [], slot }`; clause = `wear` → `` `wears ${items.join(', ')}` ``; `take_off` → `` `takes off ${items.join(', ')}${slot ? ` (${slot} only)` : ''}` ``; `clear_slot` → `` `clears ${slot}` ``. Pushed only if not already present (`clauses.includes(clause)` — exact-string dedup).
  3. `clauses.join('; ')`.
  Items render VERBATIM, placeholders and all (`takes off {{params.garment}}`); `mode` never appears; `wears` with `add_to_slot` + slot still reads `wears X` (no slot suffix — only `take_off` gets ` (<slot> only)`). The pinned line: `    Side effects: writes state.encounter.count, metadata.ansibleTool; wears Cloak of Night, Boots; takes off {{params.garment}}; clears hair`.

### 1.10 `chats-messages.ops.ts` + `chat.types.ts` — comment-only

`ChatMessageRowSchema.pascalMeta.effects` (`chats-messages.ops.ts:159-170`) and
`MessageEventSchema.pascalMeta.effects` (`chat.types.ts:536-552`) keep
`z.array(z.object({ target: z.string(), previous: z.unknown().optional(), next:
z.unknown(), tier: … }))`; only the doc comments gain the wardrobe sentence
(`next` = `{ item?, slot?, slotsAffected, effect, slots }`). **No DDL, no
migration, no D23 move.** v5 reads `pascalMeta` as opaque JSON
(`db/chats_messages_read.rs:231` `put_opt_json`), so nothing in the row
reader moves either.

### 1.11 The composite take-off FIX — `wear-ops.ts` (+73) + `wardrobe-take-off-handler.ts` (+16)

**`wear-ops.ts`** (201 lines at the pin):
- Imports (`:27`, `:30`): `WARDROBE_SLOT_TYPES, cloneEquippedSlots, makeEmptyEquippedSlots` and `componentGraph` from `pool`.
- Header (`:16-20`): `takeOffWearable` is the by-reference form the tool and Pascal's wardrobe effects share; worn composites are stored dissolved, so filtering the composite's own id out of a slot would remove nothing.
- `:139-144` **`TakeOffOutcome { slots: EquippedSlots; slotsAffected: WardrobeItemType[] }`** — `slots` = after the last commit, or what was loaded when nothing was committed; `slotsAffected` = every slot a removal was committed against, in slot order.
- `:159-187` **`takeOffWearable(repos: DisplacementRepos, chatId, pool: WearablePool, item: WardrobeItemWithOrigin, slot?: WardrobeItemType): Promise<TakeOffOutcome>`** — `characterId = pool.characterId`:
  - `(item.componentItemIds ?? []).length === 0` (a single garment): `slotsAffected = slot ? [slot] : [...item.types]`; one `applyDisplacement(repos, chatId, characterId, { mode: 'remove_from_slot', slot: s, itemId: item.id })` per slot, `slots` = the last return, or `await loadCurrentSlots(…)` when the list was empty (an item with `types: []` and no slot named → zero commits, `slotsAffected: []`).
  - else (a composite): `current = await loadCurrentSlots(…)`; `graphIds = new Set(componentGraph(pool, [item.id]).keys())` — the composite ITSELF included ("in case it was stored whole") plus every transitive leaf; `candidateSlots = slot ? [slot] : [...WARDROBE_SLOT_TYPES]`; for each `s`: `worn = (current[s] ?? []).filter((id) => graphIds.has(id))` — read from the ONE `current` snapshot, not re-read per commit; one `remove_from_slot` commit per worn id in that slot's stored order; `if (worn.length > 0) slotsAffected.push(s)`. `slots` = the last commit's return, or `current` when nothing was on. **A composite with nothing of it on commits nothing** (`slotsAffected: []`) — the applier's `land` then sees `sameSlots` and records nothing.
- `:194-201` **`loadCurrentSlots(repos, chatId, characterId)`** → `repos.chats.getEquippedOutfitForCharacter(…)`, `cloneEquippedSlots(current)` or `makeEmptyEquippedSlots()` — both `bySlot(...)`, i.e. ALL FIVE keys in `WARDROBE_SLOT_TYPES` order (so a record's `previous` / `slots` always serialize as a full five-key object in canonical order — v5's `Slots` matches).
- `takeOffItem` (`:127-135`) is UNCHANGED and still exported (no caller left in production — `wardrobe-take-off-handler.ts` now imports `takeOffWearable`).

**`wardrobe-take-off-handler.ts`**: header (`:12-13`) "a composite comes off by its leaves, since worn composites are stored dissolved (`takeOffWearable`)"; the `remove` arm (`:104-106`) becomes `const { slotsAffected } = await takeOffWearable(repos, context.chatId, pool, item, op.slot);` — the `WardrobeItemType` import is gone. `slots_affected` in the op result is now what `takeOffWearable` COMMITTED AGAINST (for a composite: the slots where a leaf was actually on; for a single garment: the named slot or `item.types` — unchanged), and `effect_summary` (`describeWardrobeEffect('removed', slotsAffected, item.title)`) follows. The INFO `Wardrobe item taken off` keeps the same fields (`slotsAffected` now possibly `[]` for a composite with nothing on — still `appliedCount++`, still `success`).

### 1.12 Published schema + specimen

- `public/schemas/qtap-custom-tool.schema.json` (+102; 43,493 bytes at the pin vs the 39,471 v5 vendors — §2.10): `triggers.items` re-wrapped onto four lines (a `362426c8f` key re-flowed, byte-visible); `effects.description` gains `, and wardrobe gestures that dress or undress the rolling character`; `$defs.Effect` DROPS `required: ["target","value"]`, its description becomes `One side effect, recorded after the outcome table has dealt: EITHER a write (target + value) OR a wardrobe gesture (wardrobe), never both and never neither. Effects apply in order, each seeing the writes of the ones before it (value writes commit first, then wardrobe gestures); …`, `target.description` gains ` A write needs both target and value.`, a `wardrobe` property (`$ref: #/$defs/WardrobeGesture`, description `A wardrobe gesture on the rolling character (and nobody else) instead of a write: wear items, take them off, or clear a slot. A run made as nobody dresses nobody.`), and a `oneOf` (`[{required:[target,value], not:{required:[wardrobe]}}, {required:[wardrobe], not:{anyOf:[{required:[target]},{required:[value]}]}}]`); NEW `$defs.WardrobeGesture` (`required: ["action"]`, `additionalProperties: false`, the per-action rules stated in the description "checked by the loader, not by this schema", `items` 1..8 of 1..200 strings, `mode` enum, `slot` enum of the five). The test `is deliberately weaker than Zod on wardrobe per-action rules` pins that the MIRROR accepts `add_to_slot` without a slot, `clear_slot` with items, and `{{params.ghost}}` while Zod rejects all three.
- `public/schemas/qtap-export.schema.json`: ONE description string on `pascalMeta.effects` (`:960`) gains the wardrobe sentence (105,774 bytes at the pin vs 105,310 embedded in v5 — §2.10).
- `docs/developer/CUSTOM_TOOL_SPEC.json` (the range specimen the definition test loads): a new `gloves` string parameter (default `Lockpicking Gloves`) and `"effects": [{ "when": { "outcome": { "eq": "failure" } }, "wardrobe": { "action": "take_off", "items": ["{{params.gloves}}"] } }]` — v5 mirrors it under `docs/v4/developer/CUSTOM_TOOL_SPEC.json` (§2.10).

### 1.13 The eight test files — what each pins

| file | adds | pins |
|---|---|---|
| `__tests__/unit/lib/pascal/custom-tool-definition.test.ts` (+102) | `describe('effects — wardrobe gestures')` ×8 `it`s; 10 rows in the Ajv-mirror table; `is deliberately weaker than Zod on wardrobe per-action rules` | every action shape accepted (incl. `when` + `take_off` + `slot`); `{{params.material}} gloves` with `STR_PARAM`; `{{mystery}}` tolerated; the three refine messages (`/not both/`, `/needs target \+ value, or wardrobe/`, `/needs both target and value/`); the per-action rules AT PATHS (`effects\.0\.wardrobe\.slot.*mode add_to_slot needs a slot`, `…slot.*only used with mode add_to_slot`, `…items.*at least one item`, `effects\.0\.wardrobe\.mode`, `clear_slot needs a slot`, `…items.*takes no items`); `effects\.0\.wardrobe\.items\.1.*undeclared parameter "ghost"`; ``/declares no `llm` block/`` and accepted with `LLM_BLOCK`; 9 items / a 201-char ref refused; gestures + writes counted together against `MAX_EFFECTS`; the mirror agrees with Zod on 10 new docs and is weaker on 3 |
| `custom-tools-effects.test.ts` (+98) | the helper `applied()` now filters `isApplicableWriteEffect`; `describe('wardrobe gestures in executeCustomTool')` ×4; one roster `it` | item rendering (trim, `{{params.garment}}` → `Velvet Slippers`, `{{metadata.hat}}` → `Crown`, `mode: 'replace'` carried, `clear_slot` → `{ action, items: [], slot }`); the three predicates' truth tables; an unresolved `{{metadata.absent}}` left as written; `skipped: 'every item reference rendered empty'` vs the empties dropped; the `when` gate on gestures; the roster line regex `^\s*Side effects: writes state\.encounter\.count, metadata\.ansibleTool; wears Cloak of Night, Boots; takes off \{\{params\.garment\}\}; clears hair$` and `'or what they are wearing'` |
| `side-effects.test.ts` (+57) | `jest.mock('@/lib/pascal/wardrobe-effects')`; `userId: 'user-1'` on every call; `describe('wardrobe gestures beside the writes')` ×3 | order `['chat write', 'wardrobe']`; the applier's EXACT argument object (`userId, chatId, toolName, characterId, gestures, pendingWardrobeAnnouncements`); the merged order `['wardrobe.wear','wardrobe.wear','state.count','wardrobe.clear_slot','metadata.k']`; never called without gestures |
| `side-effects-progress.test.ts` (+2) | `userId: 'user-1'` in both call shapes | the new REQUIRED param only |
| `tool-draft.test.ts` (+79) | `describe('wardrobe effects in the draft')` ×5 | the six-effect round trip; both halves kept / only the active half emitted (`items: ['Hat', '  ']` → `['Hat']`; `slot: 'hair'` dropped under mode `wear`); the loader's words on four drafts; `{{params.ghost}}` error + `{{mystery}}` ONE warning; rename inside items + `'effect 1: a wardrobe item names it'` |
| `tool-vocabulary.test.ts` (+29) | the three keys on both empty-shape expectations; one `it` | `wears: ['{{params.garment}}', 'Cloak of Night']`, `takesOff: ['{{metadata.favouriteHat}}']`, `clearsSlots: ['hair']`, `params: ['garment']`, `metadata: ['favouriteHat']`, writes untouched, not empty |
| `wardrobe-effects.test.ts` (350, NEW) | 17 `it`s over the REAL `applyWardrobeEffects` + REAL `buildWearablePool` + REAL `computeDisplacedSlots`, with `applyDisplacement` mocked to apply the pure math to an in-memory `stored` outfit | §1.3's every branch: no character → nothing + the DEBUG; unreadable wardrobe → `[]` + a WARN; pool read ONCE per run; wear by title AND id with the full two-entry audit (`previous` / `next` byte shapes, `effect: 'layered'`), `call[4] === 'tool'`; `replace` via flag → `'replaced'`; `add_to_slot` → `slot: 'hair'` on the record; archived skip with `/archived/i` + `reference: 'Old Coat'` and the rest still worn; not-in-pool / slot-mismatch → `[]` and NO displacement, NO notify; a composite worn whole (`itemsById: pool.byId`) dissolving to leaves; take_off whole (`slotsAffected: ['top','bottom']`) / restricted (`slot: 'top'`) / a composite by its leaves (three `remove_from_slot` calls in slot order) / an archived item / not-on → `[]` and no notify; `clear_slot` through the chokepoint with the exact record; a throwing commit dropped with the WARN `{ item: 'Linen Shirt', error: 'disk on fire' }`; notify ONCE with `{ userId, chatId, characterId, pendingWardrobeAnnouncements }` and `'pascal.wardrobe-effects'`, and WITHOUT the set key when none was passed |
| `__tests__/unit/lib/tools/handlers/wardrobe-handlers.test.ts` (+27) | one `it` under the take-off describe | `Picnic Outfit` (`componentItemIds: ['shirt','skirt']`, stored dissolved + a scarf) `remove` → `success`, `slots_affected: ['top','bottom']`, exactly `[{remove_from_slot top shirt}, {remove_from_slot bottom skirt}]` — the scarf untouched |

### 1.14 Log-line census (verbatim; level — context — fields)

| line | level | context | fields | site |
|---|---|---|---|---|
| `Custom tool wardrobe item reference rendered empty; dropped` | debug | `pascal.custom-tools` | `tool, effectIndex, reference` | `custom-tools.ts:1260` |
| `Custom tool effect skipped` (existing) | debug | — | `tool, effectIndex, reason` (reason now also `every item reference rendered empty` / `effect is neither a complete write nor a wardrobe gesture`) | the `skipped` helper |
| `Custom tool: no rolling character, wardrobe effects skipped` | debug | `pascal.wardrobe-effects` | `chatId, tool, gestures` | `wardrobe-effects.ts:98` |
| `Custom tool wardrobe effects skipped — the wardrobe could not be read` | warn | `pascal.wardrobe-effects` | `chatId, tool, characterId, error` | `:115` |
| `Custom tool wardrobe effect not applied` | debug | `pascal.wardrobe-effects` | `chatId, tool, characterId, effectIndex, reason` + `reference` \| `item, slot` \| none | `:126` |
| `Custom tool wardrobe effect failed; that change was dropped` | warn | `pascal.wardrobe-effects` | `chatId, tool, effectIndex` + `slot` \| `item`, `error` | `:253` |
| `Custom tool wardrobe effects applied` | debug | `pascal.wardrobe-effects` | `chatId, tool, characterId, applied: string[], announcement: 'joined the turn' \| 'enqueued now'` | `:233` |
| `Custom tool effects applied` (existing) | debug | `pascal.side-effects` | `chatId, tool, applied` (writes only now) | `side-effects.ts:356` |
| (beneath) `Wardrobe outfit announcement scheduled` / `Failed to schedule wardrobe outfit announcement` | debug / warn | **`pascal.wardrobe-effects`** (the `sourceContext`) | `chatId, characterId[, error]` | `outfit-change-effects.ts:64,70` — only on the no-set path |
| (beneath) `[applyDisplacement] Outfit change committed` | — | the chokepoint's | `mode, itemId, slot, source` — `source=tool` on wears, `take-off` on removes/clears | the displacement module |

---

## 2. v5 on `main` (`56554d98c`) — where each lands, what exists, what is missing

### 2.1 `crates/quilltap-core/src/pascal/custom_tool_types.rs` (3,424 lines) — the definition format

- Constants `:81-127` (`MAX_EFFECTS` `:104`, `MAX_EFFECT_TARGET_LENGTH` `:107`); **no** `MAX_WARDROBE_EFFECT_ITEMS` / `MAX_WARDROBE_ITEM_REF_LENGTH`.
- `Issue` `:175-215` (`hard` aborting / `check` continuable / `at`), `aborted` `:213`, `prefix` `:219`, `union` `:250`, `unrecognized_keys` `:1203`, `issue_at` `:2995`, `format_definition_issues` `:3286-3312` (`path.join(".") + ": " + message`, `"; "`-joined) — the machinery a `superRefine`-shaped effect refine and a `wardrobe` sub-object slot into.
- `EffectWhen` `:797`, `EffectValue` `:808`, **`CustomToolEffect { when: Option<EffectWhen>, target: String, value: EffectValue }`** `:822-830` — both halves REQUIRED; the `Serialize` is derive (`when` skipped when `None`). No `wardrobe`.
- `parse_custom_tool_effect` `:2307-2370`: parses `when`, `target` (`parse_string` 1..200 — a MISSING target is an `invalid_type` hard issue today), the `value` union, `unrecognized_keys(obj, &["when","target","value"])`, then `(Some, Some) if !aborted` → the struct. The effects array `:2584-2612` (`prefix(i)`, the `at most 16 effects` check).
- `validate_effects` `:2914-2960`: `parse_effect_target` issue → `target <reason>` at `effects.i.target`; the string-value expression check with `value references undeclared parameter "<name>"` / `` value references {{llm}}, but the tool declares no `llm` block ``; then `validate_when_subjects` at `effects.i.when` — **in the OLD order (target, value, when)**; v4 now emits `when` issues FIRST (§1.1).
- `WardrobeItemType`'s v5 home is `wardrobe.rs:31` `WARDROBE_SLOT_TYPES: [&str; 5]` (no enum type; slots are `&str` everywhere in the wardrobe family).
- Family: `pascal_custom_tool_definition_equivalence.rs` (284 lines) ← `harness/oracle/cases/pascal-custom-tool-definition.ts` (812 lines; `QT_ORACLE_PASCAL_DEFINITION`; tier 1, `npx tsx`, `TZ=UTC`) — rows `title` / `definition` (`inputJson` BYTES, `success`, `reason` = `formatDefinitionIssues`, `data`, `unknownKeys`) / `gate`. The SAME corpus is the SPA twin's (`apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson`, `custom-tool-types.corpus.spec.ts:40`).

### 2.2 `pascal/custom_tools.rs` (2,414 lines) — the pure core

- `CustomToolRunResult.effects: Option<Vec<ResolvedEffect>>` `:250`.
- `EffectSubjects` `:1149`; **`ResolvedEffect { Applicable { index, target, value } | Skipped { index, reason } }`** `:1193-1210`, `is_applicable` `:1213` (the OLD "would write" meaning), `Serialize` `:1220-1240` (`index, target, value` / `index, skipped`) — no wardrobe arm.
- `resolve_effects` `:1281-1340`: the `skipped` closure logs DEBUG `Custom tool effect skipped` (`target: "quilltap::pascal"`, `tool, effect_index, reason`) `:1303-1311`; the `when` gate `:1319-1332`; `parse_effect_target` `:1334-1337`; the value arm follows. No wardrobe branch; no `rendered empty` line.
- `TemplateVars` `:1378-1395` (`value, roll, dice, params, metadata, progress, now: Option<i64>, llm, state`); `render_template` `:1402` (regex replace, `js_trim` of the key, `classify_placeholder` + `resolve_placeholder_value`, the four not-renderable DEBUG reasons) — the item-reference renderer already exists; `resolve_effects` builds the `TemplateVars` at `:1286-1300` with `now: Some(subjects.now)`.
- `placeholders.rs`: `PlaceholderRef` `:55`, `classify_placeholder` `:87`, `ScannedPlaceholder` `:141`, `scan_placeholders` `:151` — `validateWardrobeGesture`'s scanner exists.
- Family: `pascal_custom_tools_execution_equivalence.rs` (516 lines) ← `cases/pascal-custom-tools-execution.test.ts` (jest; `QT_ORACLE_PASCAL_EXECUTION`; the `effects-*` rows at `:1235-1340` emit the run result with `effects` spread last, `:213-220` of the Rust side inserts `effects` via `serde_json::to_value`).

### 2.3 `pascal/side_effects.rs` (1,625 lines) — the applier

- Header `:1-58` — SYNC on a borrowed `WriterSet` inside ONE `Db::write` closure; the pure `plan_applications` `:206` / impure `commit_plan` `:663` split (the differential seam `differential::plan_run` `:936-949`, `cfg(any(test, feature = "test-support"))`).
- `CONTEXT = "pascal.side-effects"` `:69`; `EffectTier` `:73`; **`AppliedEffect { target: String, previous: Option<Value>, next: Value, tier: Option<EffectTier> }`** `:97-123` with a hand `Serialize` (`target`, `previous` iff `Some`, `next`, `tier` iff `Some`) — the wardrobe record FITS this struct unchanged (`previous: Some(slots)`, `next: record`, `tier: None`).
- **`ApplyCustomToolEffectsParams<'a> { chat_id, tool_name, effects: &[ResolvedEffect], cascade: Option<&StateCascadeResult>, character_id: Option<&str>, metadata_snapshot: &Map, now_ms: i64 }`** `:126-146` — no `user_id`, no pending set.
- `apply_custom_tool_effects(writers: &mut WriterSet, params) -> Vec<AppliedEffect>` `:176-204`; `plan_applications` filters `ResolvedEffect::Applicable` `:218-224`; the `skip` DEBUG `Custom tool effect not applied` `:240-250`; `commit_plan`'s WARN `Custom tool effect write failed; those effects were dropped` `:691`; the `Custom tool effects applied` DEBUG `:803`.
- **`apply_effects_for_run(db, chat_id, result, cascade, character_id: Option<String>, metadata_snapshot, now_ms) -> Vec<AppliedEffect>`** `:825-880` — the SHARED wrapper both entrances call (v4's two six-line call sites); opens `db.write(move |writers| …)`, and on a dead writer logs WARN `Custom tool effects could not be applied; the writer was unavailable` `:868` (v5-only). **This is the seam for the after-commit `notify_wardrobe_changed`** (§3.3) — the async half cannot run inside the write closure (the tool table's two-halves rule, `wardrobe_tool_table.rs:52-66`).
- Family: `pascal_side_effects_equivalence.rs` (442 lines) ← `cases/pascal-side-effects.test.ts` (jest, mocked repos as RECORDERS; `QT_ORACLE_PASCAL_SIDE_EFFECTS`; corpus `harness/oracle/fixtures/pascal-side-effects.json`, 49 rows, `nowMs: 1788876000000`) — compares `applied` / `characterWrite` / `stateWrites` / `warns` / `snapshotMutated` through `differential::plan_run`.

### 2.4 `pascal/tool_vocabulary.rs` (307 lines)

`ToolVocabulary` `:54-97` (`value, roll, dice, llm, params, metadata, state,
stateWrites, metadataWrites, progress, progressWrites, now`) — derive
`Serialize` with renames; **no `wears` / `takesOff` / `clearsSlots`**;
`is_empty_vocabulary` `:99`; `collect_tool_vocabulary` `:152` — the effects
loop `:200-222` reads `effect.value` / `when` / `parse_effect_target(&effect.target)`
(unconditionally — it will not compile once `target` is optional); `sorted` =
`locale_compare` `:236`. Family: `pascal_tool_vocabulary_equivalence.rs` (169
lines) ← `cases/pascal-tool-vocabulary.ts` (264 lines, tier 1, `npx tsx`,
`QT_ORACLE_PASCAL_VOCABULARY`). The SPA's wire type is
`apps/web/src/app/chat/custom-tools.api.ts:37-73` (`ToolVocabulary`,
`progressWrites?` optional — the SPA survey owns adding the three keys).

### 2.5 `tools/run_custom.rs` (965 lines) — the model entrance + roster description

- `RUN_CUSTOM_PREAMBLE` `:100-110`; line `:106` carries the OLD sentence (`…persistent state or the rolling character's own records, server-side…`).
- `build_run_custom_description` `:359-430`; the effects line `:407-421` builds `targets` by first-occurrence dedup over `effect.target` and pushes `format!("    Side effects: writes {}", targets.join(", "))` — no `describe_effects`.
- `RunCustomToolContext` `:444-466` (`user_id, chat_id, character_id: Option, character_mount_point_id, character_ids, project_id, caller_participant_id, now_ms`) — **no pending set**; the executor builds it at `tools/executor.rs:661-673` (from `ctx.user_id` / `chat_id` / `character_id` / `character_ids` / `calling_participant_id`, `now_ms: now_unix_ms()`), while the SAME executor hands the wardrobe tools `Some(&ctx.pending_wardrobe_announcements)` at `:1668` (`services/tool_execution.rs:290` `pub pending_wardrobe_announcements: Arc<Mutex<HashSet<String>>>`, created at `:334`). The orchestrator drains it at `services/orchestrator.rs:3571`.
- `execute_run_custom_tool` `:529` / `_with_consult` `:551`; the effects call `:761-770` (`apply_effects_for_run(db, &ctx.chat_id, &result, cascade, ctx.character_id.clone(), &metadata, ctx.now_ms)`); `pascalMeta` assembly `:789-845` (`effects` inserted iff non-empty `:836-840`, serialized through `AppliedEffect::serialize`); `post_pascal_result` `:847`.
- The operator route: `api/custom_tools.rs:893-901` calls the same wrapper with `as_character_id.map(|_| perspective_character_id)` (no set — matches v4's route, §1.8).
- Families: `pascal_run_custom_equivalence.rs` (66 lines; tier 1; `cases/pascal-run-custom.ts` 274 lines, `QT_ORACLE_PASCAL_RUN_CUSTOM`; rows `{ id, definitions: [raw…], description }` compared byte-exact through `build_run_custom_description`) and `pascal_run_custom_handler_equivalence.rs` (770 lines; tier 3; jest `cases/pascal-run-custom-handler.test.ts` with `QT_ORACLE_OUT`, `QT_ORACLE_PASCAL_RUN_CUSTOM_HANDLER`; fixture pair `crates/quilltap-web/tests/fixtures/pascal-run-custom-{main,mount}.db` + `.meta.json` built by `harness/oracle/fixtures/build-pascal-run-custom-fixture.ts` with `pascal-run-custom.json` (`testPepperBase64`, `userId`, `seedTimestamp 2026-04-01`); the `effects-*` cases at `:542-566` dump `chat_messages` (`:699`) and compare the posted row incl. `pascalMeta`). Also `pascal_custom_tools_route_equivalence.rs` (925 lines, the operator route, one `effects` mention).

### 2.6 `wear_ops.rs` (730 lines) — the wear-ops front half

- `PutOnMode` `:32-63` (`parse`, `as_str`, `displacement_mode`); `WearableRef<'a> { item_id: Option<&str>, item_title: Option<&str> }` `:68` (JS truthiness: `""` is no reference); `WearableRefusal` `:75`; `WearableResolution<'p> { Ok(&Value) | Refused { reason, message } }` `:93`; `wardrobe_item_not_found_message` `:104-113`; `find_in_pool` `:117-127`; `resolve_wearable` `:142-178` (the three sentences byte-exact); `WearEffect` `:182` (`as_str` → `layered` / `replaced`); `WearOutcome { slots: Slots, effect, slots_affected: Vec<String> }` `:198`; **`wear_item(main: &Connection, chat_id, pool: &WearablePool, item: &Value, mode, slot: Option<&str>, source: EquipSource) -> Result<WearOutcome, DbError>`** `:209-249`; **`take_off_item(main, chat_id, character_id, item_id, slots: &[String]) -> Result<(), DbError>`** `:255-278` (one `apply_displacement(RemoveFromSlot)` per slot, `EquipSource::Ui` — the chokepoint stamps `TakeOff`). **No `take_off_wearable`, no `TakeOffOutcome`, no `load_current_slots`** — `load_slots(main, chat_id, character_id) -> Slots` exists but PRIVATE at `services/outfit_displacement.rs:42-46`.
- Tests `:280-730` include the REAL-displacement rig (`rig(initial)` `:508-531` over `fresh_schema.json`'s `chats` DDL + `WARDROBE_WEAR_STATS_DDL`, `stored()` `:533`, the `commits(lines)` reader of `[applyDisplacement] Outfit change committed … mode=… itemId=… slot=… source=…` `:543-554`, the composite `suit` pool `:470-504`) — the rig a `take_off_wearable` unit test and the applier's tests can reuse as-is.
- Family: `wear_ops_equivalence.rs` (132 lines; tier 1; `cases/wear-ops.ts` 185 lines, `node --import tsx`, `QT_ORACLE_WEAR_OPS`) — the PURE half only (`resolveWearable` / `findInPool` / `wardrobeItemNotFoundMessage` / `wearable.ts`); its header says the persisted half (`wearItem` / `takeOffItem`) is NOT in the corpus (proven by the Rust rig above).
- Neighbours: `wearable_pool.rs` `WearablePool { character_id, tiers, by_id, order, own_ids, layers_weakest_first, wearable_cache }` `:58-71`, `by_id()` `:104`, `get` `:138`, `find_by_title` `:162`, **`component_graph(pool, root_ids) -> Vec<Value>`** `:490-510` (BFS, discovery order, cycle-safe — v4's `componentGraph(pool, rootIds).keys()` set is this list's ids) and `component_graph_lookup` `:516`; `wardrobe.rs` `WARDROBE_SLOT_TYPES` `:31`, `Slots` `:553-600` (`fresh` `:563`, `slot(name)` `:569`, `slot_mut` `:581`, `from_value` `:597`; `Default` + `PartialEq` — `assert_eq!(stored(&conn), outcome.slots)` compiles); `db/chats_outfits.rs:133-143` `get_equipped_outfit_for_character -> Option<Value>`; `db/wardrobe_wear_stats.rs:232-250` `EquipSource` (`Tool` `:238` → `"tool"`); `services/outfit_displacement.rs:88-102` `apply_displacement(main, chat_id, character_id, &ComputeDisplacedOptions, source) -> Result<Slots, DbError>` (the mode's `is_take_off()` forces `EquipSource::TakeOff`); `wearable.rs:25` `wear_refusal(&Value) -> Option<String>`.

### 2.7 `tools/wardrobe_take_off.rs` (≈370 lines) — SHARES THE COMPOSITE BUG

`execute(main, mount, user_id, chat_id, character_id, args) -> (WardrobeTakeOffToolOutput, Vec<String>)` `:131-153` (sync, inside the write; the changed-character list is the second half's input); `run` `:155`; `load_tool_pool(main, mount, chat_id, character_id)` `:167`; the `remove` arm `:221-278`: `find_in_pool` `:221-228`, `slots_affected` = the named slot or **`item.types`** `:244-255`, then `take_off_item(main, chat_id, character_id, &id, &slots_affected)` `:256` — **exactly v4's pre-fix `(pre)` shape**: a worn composite (stored as its leaves) is filtered out of its `types` slots by ITS OWN id and nothing comes off, yet the op reports `success` with `slots_affected = item.types`. The INFO `Wardrobe item taken off` `:258-268` (`slotsAffectedJson`), the per-op WARN `Wardrobe take off operation failed` `:287-299` (`errorMessage` — the recorded `message`-field rename).

The tool table: `tools/wardrobe_tool_table.rs` — `load_tool_pool` `:20-35`,
`WardrobeToolContext { user_id, chat_id, character_id, pending_wardrobe_announcements: Option<&Arc<Mutex<HashSet<String>>>> }`
`:97-105`, `WardrobeToolStage { output, changed }` `:122`,
`run_wardrobe_tool_handler` (the in-write half), **`finish_wardrobe_tool(db, ctx,
stage)`** `:236-300` — `notify_wardrobe_changed(db, &OutfitChangeContext{…}, source)`
per changed character `:254-264` with `source` = the handler's context string
(`"wardrobe-take-off-handler"` …). Families: `wardrobe_tools_equivalence.rs`
(1,040 lines; tier 2/3; `cases/wardrobe-tools.ts` + `fixtures/wardrobe-tools.json`
+ `build-wardrobe-tools-fixture.ts`; `QT_ORACLE_WT`; the Rust side drives
`run_wardrobe_tool_handler` `:720` then `finish_wardrobe_tool`) — **its corpus
has FIVE `wardrobe_take_off` ops (`fixtures/wardrobe-tools.json:366,377,389,495,517` —
`takeoff_blouse`, `takeoff_clear_footwear`, …) and NONE names the fixture's
composite `Casual Outfit` (`:57`, `componentItemIds` = items 1 + 2, `isDefault`),
which only `wardrobe_update` ops touch (`:309,777,788`)** — so the family is
GREEN on the shared bug today and will stay green after the fix unless a row
is added (§4.4). `wardrobe_tools_avatar_trigger_equivalence.rs` (749 lines) and
`outfit_change_effects_tier3_equivalence.rs` (666 lines; `cases/outfit-change-effects-tier3.ts`
+ `fixtures/outfit-change-effects-tier3.json`, `QT_ORACLE_OUTFIT_EFFECTS`, a
scripted matrix of `notify` / `record` / `schedule` / `flush` steps with a
shared `Set`) cover the announcement half.

### 2.8 `services/outfit_change_effects.rs` — `notifyWardrobeChanged`'s twin

`OutfitChangeContext<'a> { user_id, chat_id, character_id, pending_wardrobe_announcements: Option<&'a Arc<Mutex<HashSet<String>>>> }`
`:34-43`; `refresh_avatar_for_outfit(db, user_id, chat_id, character_id,
source_context: &'static str)` `:52`; `schedule_wardrobe_announcement` `:83`
(the `scheduled` DEBUG / `Failed to schedule` WARN with `context =
source_context`); `record_pending_wardrobe_announcement` `:114-139` (inserts
into the set and returns, else schedules now); **`notify_wardrobe_changed(db:
&Db, ctx: &OutfitChangeContext<'_>, source_context: &'static str)`** `:142-158`
— async, takes `&Db` (NOT a writer): it must run AFTER `apply_effects_for_run`'s
`db.write` returns. `flush_pending_wardrobe_announcements` `:174` (sorted
drain). The three production callers: `wardrobe_tool_table.rs:255`,
`api/chat_outfits.rs:838`, and the harness.

### 2.9 The SPA twins (NAMED only — the SPA survey owns them)

`apps/web/src/app/pascal/custom-tool-types.ts` (2,225 lines; zero `wardrobe`
mentions), `tool-draft.ts` (1,772; zero), `tool-draft.spec.ts` (1,379),
`custom-tool-types.effects.spec.ts` (240; the chipLabel + side-effects arms
"pinned against v4's REAL Zod", refreshed at `25f534c0b`),
`custom-tool-types.corpus.spec.ts` (reads `apps/web/src/testing/fixtures/
pascal-custom-tool-definition.oracle.ndjson` — the SAME NDJSON
`pascal_custom_tool_definition_equivalence` reads; one regen feeds both),
`placeholders.ts`, `tool-gate.ts`, `tool-presets.ts`; the vocabulary wire type
`apps/web/src/app/chat/custom-tools.api.ts:37-73`; the Workbench / Proving
Bench / run dialog under `apps/web/src/app/screens/custom-tools/`
(`proving-bench.spec.ts`, `workbench-editor.spec.ts` both reference the
published schema).

### 2.10 Vendored artifacts + carriers

- `apps/web/public/schemas/qtap-custom-tool.schema.json` = **39,471 bytes = v4 at `83334b9a5`** (so NEITHER `362426c8f`'s `triggers` block NOR this commit's `wardrobe` block is vendored); v4 at the pin = 43,493. Guard: `crates/quilltap-harness/tests/public_schemas_vendor_guard.rs:49-52` (`bytes: 39_471`, the pinned size "at v4 `25f534c0b`") — a RE-VENDOR obligation shared with the trigger lane (one copy, one pin bump — §6.9).
- `crates/quilltap-core/src/generators/qtap-export.schema.json` = 105,310 bytes = v4 at `83334b9a5`; the pin = 105,774 (+464 for the one `pascalMeta.effects` description); guard `qtap_schema_embed_guard.rs:72` `VENDORED_BYTES = 105_310`. **Note the ELEVENTH D23 commit `4877fc2da` may also touch this file** — measure `git diff 83334b9a5 25b8fab6a --stat -- public/schemas/qtap-export.schema.json` at order time; the re-vendor is whole-file either way.
- `docs/v4/developer/CUSTOM_TOOL_SPEC.json` + `CUSTOM_TOOL_SPEC_DICE.json` + `DDL.md` exist in the mirror (the `help/` + `docs/v4/` re-vendor lane carries the specimen's `gloves` parameter + `effects`; the definition oracle imports the specimen at `custom-tool-definition.test.ts:26`, so a regen reads it from the PIN's checkout automatically).
- `.qtap` carriers: there is NO custom-tool entity in `services/qtap_export/` (grep `custom.tool|.tool.json` → nothing) — definitions travel as `Tools/*.tool.json` DOCUMENTS inside a store, and `pascalMeta` rides inside the message row's opaque JSON. **Nothing to port in the carriers**; the vendored export schema's description is the only byte.
- `crates/quilltap-core/src/pascal/mod.rs` (15 lines): `custom_tool_types, custom_tools, dice, expressions, js_value, llm_consult, metadata_match, placeholders, roster, side_effects, tool_gate, tool_vocabulary, workbench` — **no `wardrobe_effects`; no run service** (`362426c8f`'s — §8). `pascal/workbench.rs` (556 lines) is the LIBRARY + DESTINATIONS (`build_custom_tool_library` `:349`, `list_custom_tool_destinations` `:425`), NOT the draft check (§9).

---

## 3. Proposed tier split

### 3.1 Tier 1 — exact, pure (extend existing families)

| unit | v5 site | family |
|---|---|---|
| the effect's two-halves parse + `superRefine` + `CustomToolWardrobeGestureSchema` + `wardrobeGestureProblems` + `validateWardrobeGesture` + the `when`-first reorder | `custom_tool_types.rs` `parse_custom_tool_effect` `:2307`, `validate_effects` `:2914`, new `parse_custom_tool_wardrobe_gesture`, `wardrobe_gesture_problems`, `validate_wardrobe_gesture`; `is_write_effect` / `is_wardrobe_effect` | `pascal_custom_tool_definition_equivalence` (the `reason` bytes through `format_definition_issues` + `data` bytes through the derive `Serialize`) |
| `resolveWardrobeGesture` + the third `ResolvedEffect` arm + the three predicates | `custom_tools.rs` `ResolvedEffect::Wardrobe { index, wardrobe: ResolvedWardrobeGesture }`, `resolve_effects` | `pascal_custom_tools_execution_equivalence` (the run result's `effects` JSON) |
| `describeEffects` + the preamble sentence | `run_custom.rs:106`, `:407-421` → `describe_effects` | `pascal_run_custom_equivalence` (byte-exact) |
| the vocabulary's three lists | `tool_vocabulary.rs` | `pascal_tool_vocabulary_equivalence` |
| the write/wardrobe merge order (stable index sort) | `side_effects.rs` | `pascal_side_effects_equivalence` (the `applied` targets order — with a wardrobe pass SCRIPTED, §4.3) |

### 3.2 Tier 2 — structural DB diff (the wear-ops writes)

| unit | v5 site | family |
|---|---|---|
| `takeOffWearable` (single garment / composite-by-leaves / restricted / nothing-on) + `loadCurrentSlots` | `wear_ops.rs` `take_off_wearable`, `load_current_slots` (make `load_slots` the public twin or re-export) | a NEW tier-2 case over the `outfit_displacement_tier2` fixture shape (§4.4), or rows in `wardrobe_tools_equivalence` through the take-off handler |
| `wardrobe_take_off` on a composite | `tools/wardrobe_take_off.rs:244-256` | `wardrobe_tools_equivalence` + a NEW op row (`takeoff_casual_outfit`) |
| the applier's writes: `chats.equippedOutfit` after each gesture, `wardrobe_wear_stats` credits (`source=tool`), the `pascalMeta.effects` record bytes | the NEW `pascal/wardrobe_effects.rs` (sync, in-write) | a NEW tier-2/3 family `pascal_wardrobe_effects_equivalence` (§4.2) |

### 3.3 Tier 3 — the announcement + the turn seam

`notifyWardrobeChanged` ONCE after the loop, `sourceContext
'pascal.wardrobe-effects'`, joining the turn's set vs enqueuing now; the
`run_custom` arm threading the set. v5 shape: `apply_custom_tool_effects`
returns the applied list PLUS `wardrobe_changed: bool` (or the character id);
`apply_effects_for_run` gains `user_id: &str` and `pending: Option<&Arc<Mutex<
HashSet<String>>>>` and, after `db.write` returns, calls
`notify_wardrobe_changed(db, &OutfitChangeContext { …, pending }, "pascal.wardrobe-effects")`
then logs `Custom tool wardrobe effects applied` (its `announcement` value is
decided by whether a set was passed — same as v4). Covered by extending
`outfit_change_effects_tier3_equivalence`'s matrix shape (a `pascal` step) or
by the handler family's `chat_messages` dump + a `background_jobs` dump (§4.5).

---

## 4. The differential plan

### 4.1 `pascal_custom_tool_definition_equivalence` — grow the corpus (tier 1, no new family)

Oracle `cases/pascal-custom-tool-definition.ts` emits `definition` rows from
raw JSON BYTES. Add rows (names suggested) — each both sides parse with their
own parser and compare `success` / `reason` / `data` / `unknownKeys`:
- `effects-wardrobe-each-action` — the six-effect accept list from the jest test (wear plain; `add_to_slot` + `hair`; `replace`; `when` + `take_off`; `take_off` + `slot: top`; `clear_slot` + `hair`) → `data` bytes pin the derive `Serialize` key order (`when?, wardrobe: { action, items?, mode?, slot? }` — v4's Zod output is the INPUT order of a strict object, i.e. the author's key order is NOT preserved; Zod emits schema order).
- `effects-wardrobe-param-template` (`{{params.material}} gloves` + `STR_PARAM`), `effects-wardrobe-unknown-family` (`{{mystery}}` accepted).
- `effects-both-halves`, `effects-empty-object`, `effects-write-no-value`, `effects-write-no-target` → the three refine sentences AND the path prefix (`effects.0: …` vs `effects.0.value: …` / `effects.0.target: …`).
- The six per-action rows (`wear`+`add_to_slot`−slot; `wear`+slot−`add_to_slot`; `wear` no items; `take_off`+`mode`; `clear_slot` no slot; `clear_slot`+items) → `effects.0.wardrobe.<field>: <message>` ordering within one effect (v4 pushes `items` issues before `slot` before `mode` per the switch order — and ALSO the Zod `items.min(1)` hard issue for `items: []` precedes the refine's).
- `effects-wardrobe-undeclared-param-at-index-1`, `effects-wardrobe-llm-no-block`, `effects-wardrobe-llm-with-block`.
- `effects-wardrobe-nine-items`, `effects-wardrobe-201-char-ref`, `effects-wardrobe-empty-items` (`.min(1)`), `effects-wardrobe-unknown-key`, `effects-wardrobe-unknown-action`, `effects-wardrobe-unknown-slot` (the enum `invalid_value` sentence bytes — v5's `union`/enum helpers already render Zod 4's wording for other enums; confirm on `slot`).
- `effects-cap-mixed` (17 alternating writes/gestures → `at most 16 effects`).
- **`effects-write-when-order`** — a WRITE with BOTH a bad `when` subject and a bad target, to pin the reorder (when-issues first) — this row would be RED on v5 today even with no wardrobe key (§1.1).
Because the SPA twin reads the SAME NDJSON, the SPA lane inherits these rows for free; coordinate the regen (one `TZ=UTC npx tsx …` run → copy to `apps/web/src/testing/fixtures/`).

### 4.2 NEW tier-2/3 family — `pascal_wardrobe_effects_equivalence` (the applier)

Shape: `wardrobe-tools.ts` / `outfit-change-effects-tier3.ts` (tsx, real DB,
no jest, no mocks) — drive v4's REAL `applyWardrobeEffects` over a copy of a
fixture whose main DB holds a chat + a character with a vault wardrobe (own
leaves `shirt`/`trousers`/`boots`, archived `coat`, `ribbon` (hair), a
replace-flagged two-slot `cloak`, the composite `regalia` = shirt+trousers+boots
— v4's own test wardrobe) and a General archetype tier; the oracle case calls
`applyWardrobeEffects({ userId, chatId, toolName, characterId, gestures,
pendingWardrobeAnnouncements? })` per step and emits: the return value
(`IndexedAppliedEffect[]` bytes), the chat's `equippedOutfit[characterId]`
after the step, the `wardrobe_wear_stats` rows (the `source=tool` credits —
`wardrobe_wear_stats_tier2`'s projection), the `background_jobs` table (or the
set's contents when a set was passed — `outfit-change-effects-tier3`'s
projection), and the module's OWN logger lines (`logger.transports` splice —
memory `v4-oracle-job-host-and-log-capture`; v5 side `global_capture`, memory
`capture-rig-writer-thread`, because `Db::write` closures are invisible to
`captured_with`). Steps (one per v4 `it`, plus the composites): no character;
wear by title + by id; `replace` by flag; `add_to_slot` + `hair`; archived
skip + rest worn; not-in-pool + slot-mismatch (no writes, no job); composite
worn whole; take_off whole / restricted / composite-by-leaves / archived /
not-on; `clear_slot`; two gestures in one run with a shared set (ONE set
entry, NO job); same without a set (ONE job, the `scheduled` DEBUG with
`context: 'pascal.wardrobe-effects'`); the "already so" no-op (`wear` an item
already on with `add_to_slot` → the DEBUG, no record — NOT in v4's tests;
worth a row). The v5 side calls `pascal::wardrobe_effects::apply_wardrobe_effects`
inside `db.write` then the after-commit notify, exactly as production will.
The one v4 branch a real-DB oracle cannot script is the THROWING commit
(`disk on fire`) — keep that in the Rust unit test over the `wear_ops.rs` rig
(a dropped `chats` row mid-run), and say so in the corpus `note` (the
`pascal-side-effects.json` precedent).

### 4.3 `pascal_side_effects_equivalence` — the merge order (tier 1)

The existing jest oracle mocks `getRepositories`; add `jest.mock('@/lib/pascal/wardrobe-effects')`
returning a SCRIPTED `IndexedAppliedEffect[]` per row (the corpus row carries
`wardrobeReturns`), so both sides exercise `applyCustomToolEffects`'s split +
stable merge with no database: rows `merge-interleaved` (the five-target
order), `merge-no-gestures` (the applier NOT called — record `wardrobeCalled:
false`), `merge-gestures-only-writes-empty`. On the v5 side `differential::plan_run`
grows a `wardrobe_plan` input the same way. Also add `userId` to every
existing row's params (v4's suites did).

### 4.4 `wardrobe_tools_equivalence` + a take-off tier-2 row (the fix)

Add ops to `fixtures/wardrobe-tools.json`: `wear_casual_outfit` (so the
composite is ON, dissolved) then `takeoff_casual_outfit` (`item_title: "Casual
Outfit"`, mode `remove`) → the op result's `slots_affected` (`["top","bottom"]`
— the leaves' slots, in slot order), `effect_summary`, the `equippedOutfit`
after, the wear-stats rows, and the INFO line's `slotsAffected`; plus
`takeoff_casual_outfit_restricted` (`slot: "top"`) and
`takeoff_casual_outfit_not_on` (nothing of it on → `slots_affected: []`,
`success: true`, no displacement). Each row is RED on unported v5 (the bug) and
green after. For the pure `takeOffWearable` leaf, extend `wear-ops.ts`'s corpus
header NOTE rather than its rows — the persisted half stays proven by the Rust
rig (`wear_ops.rs:508`), where `take_off_wearable_composite_by_leaves` reads
the `commits(lines)` sequence `[remove_from_slot top shirt, remove_from_slot
bottom trousers, remove_from_slot footwear boots]` (the jest test's exact
expectation) and the single-garment / restricted / `types: []` arms.

### 4.5 The announcement seam (tier 3) + the run_custom threading

Two options; the lane picks one and says why: (a) a `pascal` step kind in
`fixtures/outfit-change-effects-tier3.json` whose v4 side calls
`applyWardrobeEffects` with / without the shared set and whose projection is
the existing one (set contents + `background_jobs` + the module's two lines
with `context: 'pascal.wardrobe-effects'`); (b) rows in
`pascal_run_custom_handler_equivalence` (`effects-wardrobe-*` cases with a
definition carrying a gesture in the fixture vaults — needs the fixture pair
REBUILT, memory `rebuilt-fixture-pair-vintage`) which already dump
`chat_messages` (the `pascalMeta.effects` bytes end to end) and can add a
`background_jobs` dump. (b) proves the MODEL entrance's `pascalMeta` bytes;
(a) proves the set seam. Both are cheap; (b) is the one that catches a wrong
`previous` / `slots` serialisation in production.

### 4.6 `pascal_run_custom_equivalence` + `pascal_tool_vocabulary_equivalence` (tier 1)

Add rosters: the jest roster (writes + wear + take_off-with-template +
clear) → the pinned line; a `take_off` WITH `slot` (` (top only)`); duplicate
clauses (two identical `wears X` effects collapse; `wears X` and `wears X, Y`
do NOT); a gesture-only tool (no `writes` clause — the line starts `Side
effects: wears …`); `add_to_slot` + slot (NO suffix); `revealOdds: false` with
gestures (line gone); and a roster whose preamble bytes pin the new sentence.
Vocabulary rows: the jest `it`'s definition; a `clear_slot` without items; a
gesture whose item quotes `{{state.x}}` / `{{now}}` / `{{llm}}` (the `state` /
`now` / `llm` flags set from an ITEM); sort-order rows with mixed-case titles
and a brace-led template (`locale_compare`).

### 4.7 Lines to capture-pin (per op)

v5 side via `global_capture` around the whole `apply_effects_for_run`: the
seven lines of §1.14 with their field sets (the `applied` list, the
`announcement` word, `reason` sentences, `reference` / `item` / `slot`
details, `error` text), plus the `[applyDisplacement] Outfit change committed`
tails (`source=tool` / `take-off`) and — on the no-set path — the scheduler's
`context=pascal.wardrobe-effects`. The `rendered empty` DEBUG lives in the pure
core's family (capture around `execute_custom_tool`).

---

## 5. Fixtures

- **Reuse:** `pascal-side-effects.json` (grow; keep `nowMs`); the definition /
  vocabulary / run-custom corpora are inline in their `.ts` cases; the
  wardrobe-tools fixture (`build-wardrobe-tools-fixture.ts` — its `Casual
  Outfit` composite `a0000000-…-0004` of items `-0001` + `-0002`, `isDefault`,
  is exactly the plant §4.4 needs; the builder's `seedItem` carries
  `componentItemIds` and `archived`); `crates/quilltap-web/tests/fixtures/pascal-run-custom-{main,mount}.db`
  (rebuild through `build-pascal-run-custom-fixture.ts` if §4.5(b) is taken —
  add a `regalia`-style tool definition + wardrobe items to vault A; re-run
  `migrate-pascal-run-custom-columns.ts` afterwards per the vintage note).
- **New:** `build-pascal-wardrobe-effects-fixture.ts` + `pascal-wardrobe-effects.json`
  (the step matrix) for §4.2 — model it on `build-outfit-change-effects-fixture.ts`
  (one chat, one user, `instance_settings` present — memory
  `builder-fixtures-lack-instance-settings`; UUID ids, not `char-1` — memory
  `memory-raw-reads-drop-non-uuid-fixtures` is about memories but the vault
  overlay reads are equally strict about character ids); seed the wardrobe as
  VAULT documents (`Wardrobe/*.md` under the character's store — the
  keystone's recursive read) so `loadToolPool` / `load_tool_pool` see the same
  pool both sides; include one General archetype so `find_in_pool`'s shadowing
  is exercised.
- The vendored schema bytes (`qtap-custom-tool.schema.json` 43,493;
  `qtap-export.schema.json` 105,774 — re-measure at the order's pin) and the
  specimen under `docs/v4/`.

---

## 6. Traps and risks (with the memory notes that apply)

1. **The applier runs inside `Db::write`; the notify cannot.** v4 awaits
   `notifyWardrobeChanged` inside `applyWardrobeEffects`; v5's
   `apply_custom_tool_effects` is sync on a `WriterSet`. Follow the tool
   table's two halves (`wardrobe_tool_table.rs:52-66`): the in-write half
   returns `changed`, `apply_effects_for_run` notifies after the write
   returns. The `announcement` word in the `applied` DEBUG must still be
   computed from "was a set passed", not from "did the enqueue happen".
   Memory: `capture-rig-writer-thread` (lines logged inside the write closure
   are invisible to `captured_with`; use `global_capture`).
2. **`load_slots` is private** (`outfit_displacement.rs:42`); v4 EXPORTS
   `loadCurrentSlots` from `wear-ops` and the applier + `takeOffWearable` both
   call it. Add `wear_ops::load_current_slots` (pub) and point
   `outfit_displacement` at it or vice-versa — ONE reader (the §S review's
   "one reader" doctrine).
3. **`is_applicable` changes meaning in v4** (§1.2). Grep every v5 caller of
   `ResolvedEffect::is_applicable` (the Proving Bench count lives in the SPA;
   the server's `plan_applications` filters by pattern match, not the
   predicate) before renaming — the write-only filter must stay write-only.
4. **The `when`-first issue reorder** (§1.1) reddens EXISTING definition rows
   whose effects carry both a `when` issue and a target/value issue — expect
   it in the regen, do not "fix" v5 back; it is a v4 change at the pin.
   Memory: `unify-review-finding-shapes` (false "pinned" claims — pin the
   NEW order with a row that has both issues, §4.1).
5. **Serialisation of `ResolvedWardrobeGesture`**: `items` ALWAYS present
   (`[]` on clear_slot), `mode` / `slot` only when set; the Proving Bench
   spreads the run result, so key order is wire bytes. Memory:
   `serde-json-map-eq-ignores-order` — compare bytes via `to_string`, never
   `Value ==`.
6. **`WardrobeEffectRecord.slots` / `previous` are five-key objects in
   `WARDROBE_SLOT_TYPES` order** on both sides (`bySlot` vs `Slots`) — but
   `AppliedEffect.previous` is `Option<Value>`; a wardrobe entry must set
   `Some(slots)` even when the slots are all empty (v4's `previous: empty()`
   is pinned in the test) — the write path's "absent when the store held
   nothing" rule does NOT apply here.
7. **`find_in_pool` with the SAME string as id and title**: v5's
   `WearableRef { item_id: Some(ref), item_title: Some(ref) }` reproduces
   v4's `{ itemId: ref, itemTitle: ref }`; the not-found sentence then carries
   BOTH clauses (`with ID "x" with title "x"`) — pinned by
   `wardrobeItemNotFoundMessage(ref, ref)`; an empty rendered ref never
   reaches here (dropped at resolve).
8. **`take_off_wearable` on a composite reads ONE `current` snapshot** and
   loops `candidateSlots` in `WARDROBE_SLOT_TYPES` order, ids in the stored
   slot order; the returned `slots` is the LAST commit's return (or `current`)
   — do not re-read after each commit (a later gesture reads the returned
   value). `component_graph` includes the root; v5's `component_graph` does
   too (`:490`), in BFS order — only the SET matters here.
9. **The schema re-vendor is a UNION obligation** with the `362426c8f` lane
   (the `triggers` block + this `wardrobe` block are one file; the guard pins
   ONE byte size) — decide at order time which lane carries
   `public/schemas/qtap-custom-tool.schema.json` (43,493) and the guard bump;
   the other lane's gate must not also move it. Same for
   `qtap-export.schema.json` if `4877fc2da` touches it. Memory:
   `parallel-lane-shared-resources`, `keystone-lane-swarm-gotchas`.
10. **The definition NDJSON feeds the SPA twin too** (§2.1) — regen once, copy
    once; memory `shared-fixture-rebuilds-and-jest-setup-mocks`,
    `sweep-driver-spaced-path` (`--v5w ~/source/quilltap-v5`),
    `sweep-driver-recipe-header-rules` (new recipe headers; `--families` is
    comma-separated), `lane-gate-hygiene`.
11. **jest oracles for the side-effects merge** (§4.3): `jest.doMock` of
    `wardrobe-effects` must be un-mocked per case if any row wants the real
    applier — memory `jest-domock-survives-resetmodules`; the logger mock
    needs `child` (the case's own header says so). Memory
    `oracle-async-tails-and-jest-mocks` for `jest.setup` globals.
12. **The composite fix changes the take-off handler's `slots_affected` for
    composites only** — single garments are byte-identical before and after;
    the wardrobe-tools family's existing five take-off rows stay green on both
    sides (they name leaves). A lane that "fixes" `slots_affected` to the
    composite's `types` would pass the old rows and fail the new one — write
    the new row first (red-first).
13. **`EquipSource`**: the applier passes `Tool` on wears; take-offs/clears
    pass nothing in v4 (the chokepoint stamps `take-off`) — v5's
    `take_off_item` passes `Ui`, which the chokepoint overrides
    (`outfit_displacement.rs:98-102`); keep that shape, do not invent a
    `TakeOff` argument.
14. **Tier R / vacuous rows** (memory `tier-r-vacuous-equal-rows`): a
    gesture whose item fails to resolve on BOTH sides for a planting mistake
    (wrong title case in the fixture, an unvaulted item) passes as "skipped"
    identically — every applier row must assert at least one `applied` entry
    or a specific `reason`.
15. **Live-data**: none — no schema move, JSON-column only; a v5 build may
    open a v4-written instance carrying `wardrobe.*` effects today (opaque
    read) and a v4 build reads v5's (same bytes). The ledger's standing
    hazards (§1) are untouched by this slice.

---

## 7. Contract candidates (what crosses lanes)

### 7.1 The wardrobe effect on a tool definition (wire = the definition JSON; owner: this lane's `custom_tool_types.rs`; consumers: the SPA twin, the Workbench, the schema file)

```jsonc
{ "when"?: EffectWhen,
  "wardrobe": { "action": "wear" | "take_off" | "clear_slot",
                "items"?: string[1..8] /* each 1..200, a TEMPLATE */,
                "mode"?: "wear" | "replace" | "add_to_slot",
                "slot"?: "top" | "bottom" | "footwear" | "accessories" | "hair" } }
```
Rules (load + draft, ONE statement): `wear` needs `items`; `add_to_slot` ⇔
`slot` on `wear`; `take_off` needs `items`, may name `slot`, no `mode`;
`clear_slot` needs `slot`, no `items`, no `mode`. An effect is a write XOR a
gesture. Issue messages: the nine strings of §1.1 verbatim.

### 7.2 `pascalMeta.effects[]` entry for a wardrobe change (wire = the message row / SSE event; owner: this lane; consumer: the SPA's transcript / Almanack readers, the `.qtap` export schema description)

```jsonc
{ "target": "wardrobe.wear" | "wardrobe.take_off" | "wardrobe.clear_slot",
  "previous": EquippedSlots /* ALWAYS present, five keys in slot order */,
  "next": { "item"?: { "id": string, "title": string },
            "slot"?: WardrobeItemType,
            "slotsAffected": WardrobeItemType[],
            "effect": "layered" | "replaced" | "removed" | "cleared",
            "slots": EquippedSlots } }
```
No `tier`. One entry per change that CHANGED the slots; a no-op is absent.
Order: by effect index, stable, interleaved with the writes.

### 7.3 `ResolvedEffect` third arm (wire = the Proving Bench preview body; owner: `custom_tools.rs`; consumer: the SPA's Proving Bench "would attempt")

`{ "index": n, "wardrobe": { "action", "items": string[] /* always */, "mode"?, "slot"? } }`;
skip reason `every item reference rendered empty`.

### 7.4 The vocabulary (wire = the run dialog's vocabulary payload; owner: `tool_vocabulary.rs`; consumer: `apps/web/src/app/chat/custom-tools.api.ts`)

Three NEW keys appended after `now`: `wears: string[]`, `takesOff: string[]`,
`clearsSlots: string[]` — sorted, references verbatim; `isEmptyVocabulary`
false when any is non-empty.

### 7.5 `take_off_wearable` (Rust; owner: `wear_ops.rs`; consumers: `tools/wardrobe_take_off.rs`, `pascal/wardrobe_effects.rs`)

```rust
pub struct TakeOffOutcome { pub slots: Slots, pub slots_affected: Vec<String> }
pub fn take_off_wearable(main: &Connection, chat_id: &str, pool: &WearablePool, item: &Value, slot: Option<&str>) -> Result<TakeOffOutcome, DbError>;
pub fn load_current_slots(main: &Connection, chat_id: &str, character_id: &str) -> Slots;
```
(`character_id` from `pool.character_id`, as v4.) `take_off_item` stays (no
production caller after the handler moves — keep for the rig's test or
retire with a census line).

### 7.6 The applier (Rust; owner: NEW `pascal/wardrobe_effects.rs`)

```rust
pub const CONTEXT: &str = "pascal.wardrobe-effects";
pub struct ResolvedWardrobeGesture { pub action: WardrobeEffectAction, pub items: Vec<String>, pub mode: Option<PutOnMode>, pub slot: Option<String> }   // in custom_tools.rs
pub struct IndexedAppliedEffect { pub index: usize, pub applied: AppliedEffect }
pub struct ApplyWardrobeEffectsParams<'a> { pub chat_id: &'a str, pub tool_name: &'a str, pub character_id: Option<&'a str>, pub gestures: &'a [(usize, &'a ResolvedWardrobeGesture)] }
/// In-write half: the gestures through wear-ops; never fails. `changed` = something landed (→ the caller notifies after commit).
pub fn apply_wardrobe_effects(main: &Connection, mount: &Connection, params) -> (Vec<IndexedAppliedEffect>, bool);
```
and in `side_effects.rs`: `apply_custom_tool_effects` → `(Vec<AppliedEffect>, bool /* wardrobe changed */)`;
`apply_effects_for_run(db, user_id, chat_id, result, cascade, character_id, metadata_snapshot, now_ms, pending: Option<&Arc<Mutex<HashSet<String>>>>) -> Vec<AppliedEffect>`
(the notify + the `applied` DEBUG after the write). `RunCustomToolContext`
gains `pending_wardrobe_announcements: Option<Arc<Mutex<HashSet<String>>>>`
(the executor clones the `Arc` at `executor.rs:661`); the operator route and
the trigger runner pass `None`.

### 7.7 Model-visible bytes (owner: `run_custom.rs`)

The preamble sentence of §1.9 and `describe_effects` exactly as §1.9 — pinned
by `pascal_run_custom_equivalence`; `tools/definitions/data.rs` (the empty-
roster `run_custom` definition) is UNTOUCHED by this commit (the roster
description is built per call).

---

## 8. The seam with `362426c8f`'s run service — what this lane ASSUMES landed

`f0db8cbb8` is STACKED on `362426c8f`: its `run-service.ts` hunk (+12) edits a
file `362426c8f` CREATED (`lib/pascal/run-service.ts`, 219 lines at the pin —
`runCustomTool(params: RunCustomToolParams): Promise<RunCustomToolOutcome>`,
the ONE sequence cascade → `executeCustomTool` → `applyCustomToolEffects` →
`postPascalResult` for all three entrances, with `invokedBy: 'llm' | 'user' |
'trigger'`, `triggerPhrase`, `chat?`, `whisperTargetIds`, `nowMs`). v5 at
`56554d98c` has NO run-service twin: the two entrances each call
`apply_effects_for_run` (`run_custom.rs:761`, `api/custom_tools.rs:893`) and
assemble `pascalMeta` themselves (`run_custom.rs:789-845`). This lane's
threading therefore lands on WHICHEVER shape exists when it starts:

- If the trigger lane has landed a `pascal/run_service.rs` (its
  `RunCustomToolParams` twin), this lane adds ONE field
  (`pending_wardrobe_announcements`) + `user_id` already present there, and
  the handler's context field; the operator route and trigger runner pass
  `None` (v4 passes nothing at `route.ts:451`, `trigger-runner.ts:165`).
- If not, this lane adds the field to `RunCustomToolContext` and the
  `user_id` + `pending` arguments to `apply_effects_for_run` (§7.6) — and the
  trigger lane's later run service must carry them (a HANDOFF line either
  way).
- Either way the ONLY `362426c8f` facts this lane depends on: `invokedBy`
  `'trigger'` exists as a third entrance that passes NO set (so a triggered
  wardrobe effect enqueues at once — the `announcement: 'enqueued now'`
  arm), and `pascalMeta` key order (`…llm, effects, invokedBy, triggerPhrase?,
  callerParticipantId?`) is `362426c8f`'s — this commit does not move it.
- The published-schema file and the `custom-tool-definition.test.ts` Ajv
  mirror table carry BOTH commits' rows (§6.9); the definition corpus regen
  at a pin ≥ `f0db8cbb8` emits `triggers` rows the trigger lane owns — a
  joint regen, one NDJSON.

---

## 9. What the ledger row (`drift-ledger.md:317`) got wrong or left out — measured from the hunks

1. **"`pascal/workbench.rs` (the draft check)"** — wrong home. v5's
   `pascal/workbench.rs` is the Workbench LIBRARY + destinations (§2.10); the
   draft check (`tool-draft.ts`'s `validateDraft` / `effectToDraft` /
   `effectFromDraft`) lives ONLY in the SPA (`apps/web/src/app/pascal/
   tool-draft.ts`, 1,772 lines) — it is the SPA survey's unit, not this
   lane's.
2. **"44 files, +2,158 / −124"** — the server slice is 17 of them; the
   `--stat` also shows `package.json` / `package-lock.json` /
   `packages/quilltap/package.json` version bumps (`4.11.0-dev.4`) and
   `.claude/commands/update-documentation.md` — none with a port surface.
3. The row says `isWriteEffect` / `isWardrobeEffect` and the applier, but
   NOT that **`isApplicableEffect` changed meaning** (§1.2) nor that
   **`validateEffects` reordered its issues** (`when` first — §1.1) — both are
   oracle-visible and the second reddens EXISTING rows.
4. The row says "`pascalMeta.effects` entries … with a `WardrobeEffectRecord`
   `next`" — add: **`previous` is ALWAYS present** on a wardrobe entry (the
   slots before) and there is **no `tier`**; an entry is recorded only when
   the slots CHANGED (`the outfit was already so` is a silent skip).
5. The row's "no `notifyWardrobeChanged` … ONE" is right, but the `sourceContext`
   is `'pascal.wardrobe-effects'` — the scheduler's / avatar trigger's lines
   carry THAT context on the no-set path, a capture-pin detail the row omits.
6. The row does not say that **`loadToolPool`'s pool is read ONCE and the
   slots threaded gesture-to-gesture** (sequential, never batched — the wear
   ledger's one-credit-per-transition rule), which is the applier's
   load-bearing doctrine and the reason it is a separate file.
7. "`run-service.ts` / `run-custom-handler.ts` / `tool-executor.ts` thread
   the turn's `pendingWardrobeAnnouncements`" — correct, and the row should
   add that the OTHER two entrances (the operator route, the trigger runner)
   do NOT (measured at the pin), so their effects enqueue at once.
8. The row's "v5 SHARES the composite bug: it filters the item id out of
   `item.types`' slots, as v4 did" — confirmed at `tools/wardrobe_take_off.rs:244-256`;
   add that the `wardrobe_tools_equivalence` corpus has NO composite take-off
   row, so the shared bug is INVISIBLE to the harness today (§2.7).
9. The vendored `qtap-custom-tool.schema.json` is 39,471 bytes = v4 at the
   `83334b9a5` baseline — the row's "`apps/web/public/schemas/
   qtap-custom-tool.schema.json` (byte copy)" should say the copy is TWO
   commits stale (`362426c8f` + this one) and the guard pins the old size.
10. `tool-draft.ts`'s `renameParameterEverywhere` / `findParameterReferences`
    hunks and the vocabulary's `collectPlaceholders` over item templates
    (`params` / `metadata` / `state` / `now` / `llm` flags set from an ITEM)
    are not in the row; both are corpus rows (§4.6).
