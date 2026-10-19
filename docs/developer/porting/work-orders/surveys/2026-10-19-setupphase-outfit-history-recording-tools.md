# Survey — outfit-history RECORDING on the chat spine, the two tools, the rollback POST

Read-only planning survey, 2026-10-19. v4 = `~/source/quilltap-server`, read
ONLY through `git show e8e17b2d9:<path>` (the round's PIN, v4 `main`,
2026-10-18), `git show <sha> -- <path>` and `git diff 25b8fab6a e8e17b2d9 --
<paths>` (`25b8fab6a` = the current oracle baseline). The v4 working tree sits
at `a6292f169` (past the pin, the Inventory programme) and was never read. v5 =
`main` at `2e89d291a`. **Every v4 line number below is AT THE PIN**; every v5
line number is at `2e89d291a`. Port facts come from SHIPPED HUNKS, never commit
messages.

**For this slice the pin IS `4ae20977f`'s shape:** `git diff --stat 4ae20977f
e8e17b2d9 -- lib/wardrobe lib/tools lib/chat/tool-executor.ts lib/pascal
lib/services/{brahma-console,chat-message,scenario-builder} app/api/v1/chats
app/api/v1/tools components/chat/ToolMessage.tsx` touches ONLY
`lib/wardrobe/gallery-images.ts` + its test (`1cfc9b504`, Aurora's wardrobe
pictures — not this slice). `7953da3a0` (carriers) and `f46cf40a2`
(`describe-entry.ts` + its test only) touch nothing here either.

**Scope (this survey):** `05005c562` whole minus the child-proxy line (NO-PORT);
`ebb480edd` minus `describe-entry.ts` (the keystone survey's); `4ae20977f`'s
RECORDING hunks only (`operator-provenance.ts`, the `toolName` provenance at the
Pascal site, the two `outfit.ts` provenance hunks, the history handler's
`&& row.source !== 'operator'`); `8d8618819`'s SERVER POST half
(`?action=wardrobe-rollback`, its three dispatch-map lines). **Out of scope:**
the table / DDL / migrations / repository (`83387d0e5`, `4ae20977f`'s schema +
`settle`'s `actorToolName` column write — the KEYSTONE survey), `describe-entry`
(`resolveHistorySlots` / `summarizeHistorySlots` / `changedByLabel` — keystone),
`GET ?action=wardrobe-history` (the history-API / SPA survey), the History tab
and every other `components/**` hunk except `ToolMessage.tsx`, the carriers
(`7953da3a0`), `help/` and `docs/` (the vendor survey).

---

## 0. The span in one paragraph

`05005c562` makes every outfit change carry WHO made it. v4's
`OutfitChangeContext` gains a REQUIRED `provenance: { source, actorCharacterId?,
toolName? }` and an optional `historyIntent` (rollback only), and now `extends
WardrobeTurnContainers` — a NEW pure module `lib/wardrobe/turn-containers.ts`
whose factory `newWardrobeTurnContainers()` creates the per-turn announcement
`Set` TOGETHER with a NEW per-turn history `Map<characterId, intent>` (last
intent per character wins), and whose `turnContainersOf(x)` forwards whichever
of the two a context carries. `recordPendingWardrobeAnnouncement` gains a
leading `repos` argument: inside a turn the character joins the Set and its
intent REPLACES any earlier one in the Map (a Set arriving without its Map
settles the history AT ONCE); outside a turn it schedules Aurora's announcement
FIRST, then settles the history (neither waits on the other's success; the
settle is guarded by a NEW WARN `Failed to settle wardrobe history`).
`flushPendingWardrobeAnnouncements(context, repos?)` drains BOTH containers —
per character it settles (default intent `append` / `character` / self) THEN
schedules, context `orchestrator-turn-end`. Every call site now names its
provenance: the operator's `?action=equip` → `operator` (by `4ae20977f`,
resolved from the chat's "Speaking As" seat by NEW `operatorWardrobeProvenance`,
read AFTER the write); the `wardrobe_*` tool table → `character` with the caller
as actor (so `wardrobe_create`'s `equip_now` gift records the GIVER as actor of
the RECIPIENT's entry); Pascal's `applyWardrobeEffects` → `pascal` with the
rolling character AND (by `4ae20977f`) the tool's name; `applyOutfitSelections`
settles `append` directly (no announcement) with its own `chat-start` /
`participant-added` / `merge` source after each successful commit, and a failed
commit now `continue`s past it. `createToolContext` and both Brahma surfaces
mint their containers through the factory; `run-service` / `run-custom-handler`
/ `side-effects` / `wardrobe-effects` / `tool-executor` thread both through
`turnContainersOf`. `ebb480edd` adds `lib/wardrobe/history/rollback.ts` — the
ONE rollback (target `index ?? current − 1`; four refusals; `filterStaleItems`;
ONE `commitEquippedOutfit` with `wornBundles: []`; `notifyWardrobeChanged` with a
`{ kind: 'cursor', index }` intent) — and two model-visible tools,
`wardrobe_rollback` (`index` int ≥ 0, optional) and `wardrobe_history` (`limit`
int 1–100, default 20, the last `limit` entries plus the current one prepended
with `gap_before_window`), gated by `canDressThemselves` at EVERY site that
lists `wardrobe_take_off` (the wardrobe table seven → nine; the executor's
built-in list; `GET /api/v1/tools`; the plugin tool builder; the simple-JSON +
legacy text-block prompts and parsers; `pseudo-tool.service.ts`;
`streaming.service.ts` `buildTools`; Brahma ×2 and the Scenario Builder set both
`false`; `ToolMessage.tsx`; the snapshot +30). `8d8618819`'s server POST
`?action=wardrobe-rollback { characterId, index }` is a thin operator caller of
the same `rollbackWardrobe` (ledger source `ui`), answering `{ equippedSlots,
previousIndex, restoredIndex, skipped }` or a 400 with the refusal's sentence.

---

## 1. v4 at the pin — the hunks, file by file

### 1.1 `lib/wardrobe/turn-containers.ts` — NEW, 32 lines, WHOLE

```ts
// 1-9: module doc — "The per-turn wardrobe containers: the announcement set and
// the outfit-history intent map an orchestrator threads through one model
// turn's tools, drained together by `flushPendingWardrobeAnnouncements`. Pure,
// with no imports that reach the database, so call sites can thread them under
// test mocks of `outfit-change-effects`."
import type { WardrobeHistoryIntent } from '@/lib/schemas/chat-wardrobe-history.types';   // :11

export interface WardrobeTurnContainers {                                                  // :14
  /** Per-turn announcement queue the orchestrator threads through the tools. */
  pendingWardrobeAnnouncements?: Set<string>;                                              // :16
  /** Per-turn history intents, created beside the Set. Last intent per character wins. */
  pendingWardrobeHistory?: Map<string, WardrobeHistoryIntent>;                             // :18
}

export function newWardrobeTurnContainers(): Required<WardrobeTurnContainers> {           // :22
  return { pendingWardrobeAnnouncements: new Set(), pendingWardrobeHistory: new Map() };
}

export function turnContainersOf(source: WardrobeTurnContainers | undefined): WardrobeTurnContainers {  // :27
  return {
    ...(source?.pendingWardrobeAnnouncements ? { pendingWardrobeAnnouncements: source.pendingWardrobeAnnouncements } : {}),
    ...(source?.pendingWardrobeHistory ? { pendingWardrobeHistory: source.pendingWardrobeHistory } : {}),
  };
}
```

Semantics the port must keep: the two containers are INDEPENDENTLY optional (a
Set without a Map is a legal, handled state — §1.3); `turnContainersOf` forwards
by REFERENCE (the same Set/Map objects), never copies; an absent container
stays absent (no `undefined`-valued key). The module split exists for jest
mocks — NO-PORT as a file boundary, but the factory and the forwarder are the
contract (§3.1).

### 1.2 The types `OutfitChangeContext` reads (`lib/schemas/chat-wardrobe-history.types.ts`, keystone-owned, quoted for the contract)

`WardrobeHistorySourceSchema` (:24-32) = `z.enum(['chat-start',
'participant-added', 'merge', 'operator', 'character', 'pascal', 'seeded'])` —
in that order. At the pin (`4ae20977f`):

```ts
export interface WardrobeHistoryProvenance {                       // :71
  source: Exclude<WardrobeHistorySource, 'seeded'>;
  /** See ChatWardrobeHistoryRowSchema's actorCharacterId. */
  actorCharacterId?: string;
  /** `pascal` only: the custom tool's name. */
  toolName?: string;                                               // :76 (4ae20977f)
}
export type WardrobeHistoryIntent =                                // :80
  | { kind: 'append'; provenance: WardrobeHistoryProvenance }
  | { kind: 'cursor'; index: number };
```

The repository's `settle(chatId, characterId, intent)` (keystone) reads the LIVE
slots at SETTLE time (not at the change), compares `hashEquippedSlots` with the
current entry, appends at `max + 1` / answers `unchanged` / moves the cursor,
writes `actorToolName = source === 'pascal' ? toolName ?? null : null`, publishes
`chats` realtime on append / move, and NEVER throws (its own WARN `Wardrobe
history settle failed; the outfit change stands unrecorded`, `module:
'wardrobe-history'` child logger). This slice CALLS it; the keystone owns it.

### 1.3 `lib/wardrobe/outfit-change-effects.ts` (196 lines at the pin)

**`OutfitChangeContext` at its final shape (:44-53):**

```ts
/** Who changed whose outfit, and the turn's containers if there are any. */
export interface OutfitChangeContext extends WardrobeTurnContainers {
  userId: string;
  chatId: string;
  characterId: string;
  /** Who is changing the outfit. Required: no caller may forget it. */
  provenance: WardrobeHistoryProvenance;
  /** Rollback only: move the cursor instead of appending. Everyone else leaves it unset. */
  historyIntent?: WardrobeHistoryIntent;
}
```

`:55-59` re-exports `newWardrobeTurnContainers`, `turnContainersOf`, `type
WardrobeTurnContainers`. `:62` `type HistoryRepos = Pick<RepositoryContainer,
'wardrobeHistory'>`.

**`settleHistory` (private, :65-83):**

```ts
try { await repos.wardrobeHistory.settle(chatId, characterId, intent); }
catch (error) {
  logger.warn('Failed to settle wardrobe history', {
    context: sourceContext, chatId, characterId, intent,
    error: error instanceof Error ? error.message : String(error),
  });
}
```

WARN fields in order `context, chatId, characterId, intent, error`; `intent` is
the whole intent OBJECT (`{ kind, provenance: { source, actorCharacterId?,
toolName? } }` or `{ kind: 'cursor', index }`). In v4 this WARN fires only when
`repos.wardrobeHistory` itself throws (a missing repo — `settle` swallows its
own failures); v5's twin fires it when the async settle returns `Err` (writer
gone) — §6 T4.

**`refreshAvatarForOutfit` (:86-97)** and **`scheduleWardrobeAnnouncement`
(:100-122)** are UNCHANGED (DEBUG `Wardrobe outfit announcement scheduled` /
WARN `Failed to schedule wardrobe outfit announcement`, fields `context, chatId,
characterId[, error]`).

**`recordPendingWardrobeAnnouncement(repos, context, sourceContext)` (:131-150)
— the SIGNATURE GAINED A LEADING `repos`:**

```ts
const intent: WardrobeHistoryIntent = context.historyIntent ?? { kind: 'append', provenance: context.provenance };
if (context.pendingWardrobeAnnouncements) {
  context.pendingWardrobeAnnouncements.add(context.characterId);
  if (context.pendingWardrobeHistory) {
    context.pendingWardrobeHistory.set(context.characterId, intent);   // last wins
  } else {
    // A turn set without its history map (a caller that predates the
    // factory): record now rather than lose the entry.
    await settleHistory(repos, context.chatId, context.characterId, intent, sourceContext);
  }
  return;
}
await scheduleWardrobeAnnouncement(sourceContext, context);
await settleHistory(repos, context.chatId, context.characterId, intent, sourceContext);
```

Exact branch table:

| Set | Map | Announcement | History |
|---|---|---|---|
| present | present | Set.add (deferred) | Map.set — REPLACES an earlier intent for that character (deferred) |
| present | absent | Set.add (deferred) | settled NOW |
| absent | (any) | scheduled NOW | settled NOW, AFTER the schedule (the Map is ignored without a Set) |

**`notifyWardrobeChanged(repos, context, sourceContext)` (:156-163):** avatar
refresh, then `recordPendingWardrobeAnnouncement(repos, context,
sourceContext)`. Call ONCE per gesture / per tool call, only when something
landed.

**`flushPendingWardrobeAnnouncements(context, repos?)` (:173-196):**

```ts
context: Pick<OutfitChangeContext, 'userId' | 'chatId' | 'pendingWardrobeAnnouncements' | 'pendingWardrobeHistory'>,
repos?: HistoryRepos,
…
const pending = context.pendingWardrobeAnnouncements;
if (!pending || pending.size === 0) return;            // an empty SET returns even if the Map holds intents
const characterIds = Array.from(pending);               // insertion order
const intents = new Map(context.pendingWardrobeHistory ?? []);   // snapshot
pending.clear();
context.pendingWardrobeHistory?.clear();                // BOTH cleared before the first await
const historyRepos = repos ?? (await import('@/lib/repositories/factory')).getRepositories();
for (const characterId of characterIds) {
  const intent = intents.get(characterId)
    ?? { kind: 'append', provenance: { source: 'character', actorCharacterId: characterId } };
  await settleHistory(historyRepos, context.chatId, characterId, intent, 'orchestrator-turn-end');  // SETTLE FIRST
  await scheduleWardrobeAnnouncement('orchestrator-turn-end', { userId, chatId, characterId });     // THEN ANNOUNCE
}
```

**Two orders, deliberately different (v4's own tests pin both):** outside a
turn, ANNOUNCE then SETTLE (`recordPending…` `:148-149`); at the turn-end drain,
SETTLE then ANNOUNCE (`:189-190`; test `settles before announcing`). The
per-character pair is sequential; characters are drained in Set insertion order.

The only `flushPending…` caller at the pin is
`lib/services/chat-message/orchestrator.service.ts:1721`
(`await flushPendingWardrobeAnnouncements(toolContext)` — no `repos`, so the
process repositories). **Brahma mints containers but NEVER flushes them** (both
Brahma surfaces have every wardrobe tool off — §1.13 — so nothing lands there).

### 1.4 The provenance per call site (THE table)

| # | v4 site (pin) | path into the effects | `source` | `actorCharacterId` | `toolName` | intent | v5 landing site |
|---|---|---|---|---|---|---|---|
| 1 | `app/api/v1/chats/[id]/actions/outfit.ts:274-284` `handleEquipSlot` (`?action=equip`, all 7 modes) | `notifyWardrobeChanged(…, '[Chats v1] outfit-equip')`, no containers | `operator` | `operatorWardrobeProvenance(chat)` — the Speaking-As seat's `characterId`, absent if none; the chat is read AFTER the write (`:274`); chat absent → bare `{ source: 'operator' }` | — | append | `crates/quilltap-core/src/api/chat_outfits.rs:838-848` (`chat_equip`, after the write at `:690-836`) |
| 2 | `outfit.ts:379-393` `handleWardrobeRollback` (`?action=wardrobe-rollback`) | `rollbackWardrobe(…, '[Chats v1] wardrobe-rollback')` → `notifyWardrobeChanged`, no containers | `operator` | `operatorWardrobeProvenance(chat)` (chat read BEFORE, `:379`) | — | **cursor** `{ kind:'cursor', index: targetIndex }` | NEW in `api/chat_outfits.rs` (§2.2) |
| 3 | `lib/chat/tool-executor.ts:731-738` → `WARDROBE_TOOL_TABLE` → `wardrobe_wear` / `wardrobe_take_off` via `finalizeWardrobeMutation` (`wardrobe-handler-shared.ts:186-216`, `notifyWardrobeChanged(repos, context, sourceContext)` at `:204`) | turn containers via `turnContainersOf(context)` | `character` | the CALLER's `characterId`, only when truthy (`...(characterId ? { actorCharacterId: characterId } : {})`) | — | append | `tools/executor.rs:1640-1690` `run_wardrobe_tool` (builds the ctx) → `tools/wardrobe_tool_table.rs:240-265` `finish_wardrobe_tool` |
| 4 | `wardrobe-create-handler.ts:262-271` (`equip_now`, incl. a gift) | same context; `characterId: targetCharacterId` | `character` | **the CALLER** (`context.provenance` passed through) — the RECIPIENT's entry names the giver | — | append | same as #3 (v5's stage `changed = target_character_id`, `wardrobe_tool_table.rs:188-197`) |
| 5 | `wardrobe-archive-handler.ts:102-113` (archiving an equipped own item) | same context | `character` | the caller | — | append (typically settles `unchanged` — archiving never undresses, bug 191; §6 T9) | same as #3 (`wardrobe_archive::execute`'s `changed`) |
| 6 | `wardrobe-rollback-handler.ts:51-55` → `rollbackWardrobe(…, 'wardrobe-rollback-handler')` | the tool context (containers ride it) | `character` | the caller | — | **cursor** | NEW `tools/wardrobe_rollback.rs` through the table (§2.3) |
| 7 | `lib/pascal/wardrobe-effects.ts:219-229` `applyWardrobeEffects` (once after the loop, only if `applied.length > 0`) | `turnContainersOf(params)` | `pascal` | the ROLLING `characterId` | `toolName` (the custom tool's `name`) (`4ae20977f`) | append | `pascal/wardrobe_effects.rs:433-463` `finish_wardrobe_effects` |
| 8 | `lib/wardrobe/apply-outfit-selections.ts:509` + `:544` (new chat) | **direct `repos.wardrobeHistory.settle`** — no notify, no announcement | `chat-start` (`context?.source ?? 'chat-start'`) | — (none) | — | append | `services/outfit_selections.rs:716-735` `commit_selection` (reached from `chat_create.rs:1633`) |
| 9 | same, `participants.ts:211` (`source: 'participant-added'`) | direct settle | `participant-added` | — | — | append | **`services/chat_participants.rs:1517-1545`** `apply_outfit_for_added_participant` (v5 INLINES the commit — it does NOT go through `commit_selection`) |
| 10 | same, `lib/chat/apply-chat-merge.ts:246` (`source: 'merge'`) | direct settle | `merge` | — | — | append | `outfit_selections.rs:863` `apply_outfit_selection_sync` → `commit_selection` (from `chat_merge.rs:498`) |
| 11 | `flushPendingWardrobeAnnouncements` default (a Set member without a Map intent) | — | `character` | the character ITSELF | — | append | `services/outfit_change_effects.rs:174-203` |
| — | Pascal outside a turn: `actions/run-tool.ts` (operator Run Tool), the trigger runner, the Workbench route | `applyWardrobeEffects` with NO containers | `pascal` (as #7) | rolling character | tool name | append, settled at once | `services/chat_run_tool.rs:183`, `pascal/trigger_runner.rs:272`, `api/custom_tools.rs:836` (all pass `None` — unchanged, only the field type moves) |

Sources NEVER written by a runtime caller: `seeded` (the migration / settle's
first-touch seed — keystone). There is no `operator` source on any tool path and
no `character` source on any route path.

### 1.5 `lib/wardrobe/history/operator-provenance.ts` — NEW (`4ae20977f`), 43 lines

```ts
export interface OperatorProvenanceChat {
  id: string;
  participants?: ChatParticipantBase[] | null;
  activeTypingParticipantId?: string | null;
  impersonatingParticipantIds?: readonly string[] | null;
}
/** `{ source: 'operator' }`, naming the user's live character when there is one. */
export function operatorWardrobeProvenance(chat: OperatorProvenanceChat): WardrobeHistoryProvenance {
  const seat = findActiveUserParticipant(
    chat.participants ?? [], chat.activeTypingParticipantId, chat.impersonatingParticipantIds);
  const actorCharacterId = seat?.characterId ?? undefined;
  logger.debug('Resolved operator for an outfit change', {
    context: 'wardrobe-history', chatId: chat.id, actorCharacterId: actorCharacterId ?? null });
  return actorCharacterId ? { source: 'operator', actorCharacterId } : { source: 'operator' };
}
```

`findActiveUserParticipant` = `lib/chat/turn-manager/utils` (Speaking-As seat,
impersonation included, fallback the first user-owned seat). An empty-string
`characterId` is FALSY → bare `{ source: 'operator' }`, but the DEBUG logs `''`
(`'' ?? null` is `''`) — §6 T6.

### 1.6 `applyOutfitSelections`' per-arrival settle (`lib/wardrobe/apply-outfit-selections.ts`)

`:163` the context's `source?: Extract<EquipSource, 'chat-start' |
'participant-added' | 'merge'>`; `:509` `const source = context?.source ??
'chat-start'`; the serial commit loop `:510-545`:

- `outcome.status === 'rejected'` → ERROR `[applyOutfitSelections] Failed to
  resolve outfit; character left as-is`, `continue` (no settle);
- `!outcome.value` → `continue` (no settle);
- `commitEquippedOutfit(...)` throws → ERROR `[applyOutfitSelections] Failed to
  persist equipped outfit` and — NEW at `:538` — `continue` (no settle; v4 test
  `records nothing when the commit fails`);
- success → `:544` `await repos.wardrobeHistory.settle(chatId, characterId, {
  kind: 'append', provenance: { source } })` — NO `actorCharacterId`, NOT
  wrapped (settle never throws), NOT through `notifyWardrobeChanged` (no
  announcement, no avatar refresh — the arrival paths keep their own
  `refreshAvatarForOutfit` elsewhere).

Comment at `:540-543`: "entry 0 for a first arrival (an empty outfit is an
outfit), or max + 1 for a seat that returns. `settle` reads the slots just
written and never throws." The settle is PER CHARACTER, immediately after THAT
character's commit (not batched after the loop).

### 1.7 `lib/wardrobe/history/rollback.ts` — NEW (`ebb480edd`), 183 lines, WHOLE

Types (`:29-60`):

```ts
export interface RollbackInput {
  chatId: string;
  pool: WearablePool;              // the caller's: loadToolPool, or loadWearablePool(..., { operator: true })
  index?: number;                  // absolute entry index; default = current - 1
  context: Omit<OutfitChangeContext, 'historyIntent'>;   // characterId = whose outfit rolls back
  source: Extract<EquipSource, 'ui' | 'tool'>;
}
export interface RollbackSkippedItem { itemId: string; title: string | null; reason: 'archived' | 'missing'; }
export type RollbackOutcome =
  | { ok: true; previousIndex: number; restoredIndex: number; slots: EquippedSlots; skipped: RollbackSkippedItem[] }
  | { ok: false; reason: 'no_history' | 'at_first' | 'out_of_range' | 'already_current'; message: string };
type RollbackRepos = Pick<RepositoryContainer, 'wardrobeHistory' | 'wardrobeWear' | 'chats' | 'characters'>;
const CONTEXT = 'wardrobe.history.rollback';
```

`characterName` (`:66-72`): `(await repos.characters.findByIdRaw(characterId))?.name
|| 'This character'`; a throw → `'This character'`. (Falsy name `''` →
fallback.)

`filterStaleItems(pool, target, live)` (`:78-98`, pure, exported):

```ts
const skipped = new Map<string, RollbackSkippedItem>();
const slots = bySlot((slot) =>
  (target[slot] ?? []).filter((itemId) => {
    const item = pool.get(itemId);
    if (!item) { if (!skipped.has(itemId)) skipped.set(itemId, { itemId, title: null, reason: 'missing' }); return false; }
    if (!item.archivedAt) return true;
    if ((live[slot] ?? []).includes(itemId)) return true;      // archived but worn in THIS slot → stays
    if (!skipped.has(itemId)) skipped.set(itemId, { itemId, title: item.title, reason: 'archived' });
    return false;
  }));
return { slots, skipped: Array.from(skipped.values()) };
```

`bySlot` walks `WARDROBE_SLOT_TYPES` (top, bottom, footwear, accessories,
hair); `skipped` is DE-DUPLICATED by id in first-seen (slot-major) order; a
multi-slot archived item worn in ONE of its slots stays in that slot and is
dropped (and reported once) from the others (v4 test `filterStaleItems drops an
archived item worn in a different slot`). `pool.get` is by id over every tier,
archived INCLUDED.

`rollbackWardrobe(repos, input, sourceContext = CONTEXT)` (`:100-183`):

1. `entries = await repos.wardrobeHistory.findEntries(chatId, characterId)`
   (ascending by `entryIndex`). Empty → refusal **`no_history`**:
   `` `No outfit history yet for ${name} in this chat.` ``
2. `current = entries.find(isCurrent) ?? entries[last]`; `max =
   entries[last].entryIndex`; `targetIndex = input.index ?? current.entryIndex - 1`.
3. `input.index === undefined && current.entryIndex === 0` → **`at_first`**:
   `` `${name} is already in the first outfit of this chat (0); there is nothing earlier to return to.` ``
4. `targetIndex < 0 || targetIndex > max` → **`out_of_range`**:
   `` `There is no outfit ${targetIndex}; this chat's outfits are numbered 0 to ${max}.` ``
   (no name read).
5. `targetIndex === current.entryIndex` → **`already_current`**:
   `` `${name} is already wearing outfit ${targetIndex}.` ``
6. `target = entries.find(e => e.entryIndex === targetIndex)!` (assumes
   contiguous indexes — a gap would crash on `.slots` of `undefined` → the
   caller's catch); `live = (await repos.chats.getEquippedOutfitForCharacter(chatId,
   characterId)) ?? makeEmptyEquippedSlots()`; `filterStaleItems(pool,
   target.slots, live)`.
7. Per skipped item, INFO **`Rollback left a stale item off`** `{ context:
   sourceContext, chatId, characterId, targetIndex, itemId, reason }`.
8. `await repos.wardrobeWear.commitEquippedOutfit({ chatId, characterId,
   nextSlots: slots, wornBundles: [], source })` — ONE commit, the `set_all`
   shape; the chokepoint's own lines (`Committed equipped outfit` etc.) follow.
9. `await notifyWardrobeChanged(repos, { ...context, historyIntent: { kind:
   'cursor', index: targetIndex } }, sourceContext)` — the avatar refresh, then
   the announcement + the CURSOR intent (deferred in a turn, at once outside).
10. DEBUG **`Rolled a wardrobe back`** `{ context: sourceContext, chatId,
    characterId, previousIndex: current.entryIndex, restoredIndex: targetIndex,
    skipped: skipped.length, provenance: context.provenance.source }` — AFTER the
    notify.
11. `return { ok: true, previousIndex: current.entryIndex, restoredIndex:
    targetIndex, slots, skipped }` — `slots` is what was WRITTEN (stale items
    removed), not the entry's.

The name read happens lazily per refusal (`no_history`, `at_first`,
`already_current` — not `out_of_range`). Rolling FORWARD is allowed (any
recorded index). A cursor move whose restored slots differ from the target
entry's (stale items removed) leaves the cursor on an entry whose hash ≠ the
live hash — the next append then compares against THAT entry (§6 T7).

### 1.8 The two tool definitions (`ebb480edd`)

**`lib/tools/wardrobe-rollback-tool.ts` (69 lines):**

```ts
export const wardrobeRollbackToolInputSchema = z.object({
  index: z.number().int().nonnegative()
    .describe('The outfit to return to, by its number in wardrobe_history. Omit to step back ' +
      'one outfit from the current one. Any recorded number is allowed, earlier or later than ' +
      'the current one.')
    .optional(),
})
export interface WardrobeRollbackSkippedItem { item_id: string; title: string | null; reason: 'archived' | 'missing'; }
export interface WardrobeRollbackToolOutput {
  success: boolean; previous_index?: number; restored_index?: number;
  current_state?: Record<WardrobeItemType, string[]>; coverage_summary?: string;
  skipped?: WardrobeRollbackSkippedItem[]; error?: string;
}
export const wardrobeRollbackToolDefinition = { type: 'function', function: {
  name: 'wardrobe_rollback',
  description:
    'Return to an outfit you wore earlier in this chat. Every outfit change is numbered ' +
    'from 0 (what you arrived in); wardrobe_history lists them. With no index this steps ' +
    'back one. Rolling back is itself a change of clothes: it is announced, and the next ' +
    'thing you put on becomes a new numbered outfit at the end of the list.',
  parameters: zodToOpenAISchema(wardrobeRollbackToolInputSchema) } };
export function validateWardrobeRollbackInput(input: unknown) {
  const parsed = wardrobeRollbackToolInputSchema.safeParse(input ?? {}); return parsed.success ? parsed.data : null }
```

Snapshot (`tool-definitions-snapshot.test.ts.snap`, jest SORTS keys — NOT the
wire order): `index: { description, maximum: 9007199254740991, minimum: 0, type:
'integer' }`, `additionalProperties: false`, no `required`.

**`lib/tools/wardrobe-history-tool.ts` (72 lines):**

```ts
export const WARDROBE_HISTORY_DEFAULT_LIMIT = 20
export const wardrobeHistoryToolInputSchema = z.object({
  limit: z.number().int().positive().max(100)
    .describe('How many of the most recent outfits to list. Default 20.').optional(),
})
export interface WardrobeHistoryToolEntry { index: number; at: string; source: WardrobeHistorySource;
  changed_by: string; is_current: boolean; summary: string; }
export interface WardrobeHistoryToolOutput { success: boolean; current_index: number | null; total: number;
  entries: WardrobeHistoryToolEntry[]; gap_before_window?: boolean; error?: string; }
export const wardrobeHistoryToolDefinition = { type: 'function', function: {
  name: 'wardrobe_history',
  description:
    'List the outfits you have worn in this chat, numbered from 0 (what you arrived in), ' +
    'with when each began, who made the change, and what it was. The one you are wearing ' +
    'now is marked. Use wardrobe_rollback with a number to return to one. A change made ' +
    'earlier in this same reply may not be listed yet.',
  parameters: zodToOpenAISchema(wardrobeHistoryToolInputSchema) } };
```

Snapshot: `limit: { description, exclusiveMinimum: 0, maximum: 100, type:
'integer' }` — **the FIRST `exclusiveMinimum` in v5's catalog** (`grep
exclusiveMinimum crates/quilltap-core/src/tools/definitions/data.rs` is empty),
and NO `maximum: 9007199254740991` (`.max(100)` replaces the safe-int bound).
The JSON bytes come from the regen, never from the sorted snapshot (§4.1).

### 1.9 The two handlers (`ebb480edd`; history `+1/−1` in `4ae20977f`)

**`lib/tools/handlers/wardrobe-rollback-handler.ts` (109 lines), `CONTEXT =
'wardrobe-rollback-handler'`:**

- invalid input → WARN **`Wardrobe rollback tool validation failed`** `{
  context, chatId, characterId, input }` (the RAW input) and `{ success: false,
  error: 'Invalid input: "index" must be a whole number of 0 or more, or omitted.' }`;
- `!context.characterId` → `{ success: false, error: 'wardrobe_rollback needs a
  character to dress.' }` (no log);
- `repos = getRepositories()`; try: `pool = await loadToolPool(repos, chatId,
  characterId)` (the CHARACTER's pool — project roster applies, NOT operator);
  `rollbackWardrobe(repos, { chatId, pool, index: parsed.index, context, source:
  'tool' }, CONTEXT)`;
- refusal → INFO **`Wardrobe rollback refused`** `{ context, chatId,
  characterId, reason }` and `{ success: false, error: outcome.message }`;
- success → `currentState = await loadCurrentWardrobeState(repos, chatId,
  characterId)` (re-read AFTER the notify); returns, in key order, `{ success:
  true, previous_index, restored_index, current_state, coverage_summary:
  buildWardrobeCoverageSummaryFromState(pool, currentState), skipped:
  outcome.skipped.map(i => ({ item_id, title, reason })) }`;
- catch → WARN **`Wardrobe rollback failed`** `{ context, chatId, characterId,
  error }` and `{ success: false, error: `Could not return to that outfit:
  ${message}` }`.

`formatWardrobeRollbackResults(output)` (`:88-109`), bytes:

```
!success → `Wardrobe Error: ${output.error || 'Unknown error'}`
lines = [`Returned to outfit ${restored_index} (from ${previous_index}).`]
per skipped: name = item.title ? `the ${item.title}` : `item ${item.item_id}`   // TRUTHY title
  archived → `${name} could not be put back on: it has been archived.`
  missing  → `${name} could not be put back on: it no longer exists.`
if (current_state) { '' ; 'Current outfit:' ; ...formatEquippedSlotLines(current_state) }
'' ; `Now wearing: ${coverage_summary ?? ''}`
join('\n')
```

`formatEquippedSlotLines` (`wardrobe-handler-shared.ts:120-130`): per slot in
`WARDROBE_SLOT_TYPES` order, `  ${slot}: ${ids.length === 0 ? '(empty)' :
ids.join(', ')}`, an EMPTY `hair` slot OMITTED (`isSlotReportedWhenEmpty`). The
rollback's last line is **`Now wearing:`** — NOT the wear / take-off formatter's
`Summary:` (§6 T11).

**`lib/tools/handlers/wardrobe-history-handler.ts` (136 lines), `CONTEXT =
'wardrobe-history-handler'`:**

- `windowHistory(rows, limit)` (pure, exported, `:35-45`): `window =
  rows.slice(Math.max(0, rows.length - limit))`; `current =
  rows.find(isCurrent)`; current not in window → `{ rows: [current, ...window],
  gapBeforeWindow: true }`, else `{ rows: window, gapBeforeWindow: false }`.
- `failure(error)` = `{ success: false, current_index: null, total: 0, entries:
  [], error }`.
- invalid → `failure('Invalid input: "limit" must be a whole number from 1 to
  100, or omitted.')` — **NO log line** (unlike rollback);
- `!context.characterId` → `failure('wardrobe_history needs a character.')`;
- `rows = findEntries(...)`; empty → `{ success: true, current_index: null,
  total: 0, entries: [] }` WITHOUT loading the pool;
- `pool = loadToolPool(...)`; `windowHistory(rows, parsed.limit ??
  WARDROBE_HISTORY_DEFAULT_LIMIT)`;
- actor names: for each SHOWN row, skip when `!actor || (actor ===
  context.characterId && row.source !== 'operator') || actorNames.has(actor)`
  (the `row.source !== 'operator'` clause is `4ae20977f`'s — an operator
  speaking AS the character is still NAMED); else
  `actorNames.set(actor, (await repos.characters.findByIdRaw(actor))?.name ??
  null)`, a throw → `null`;
- entries `{ index: entryIndex, at: createdAt, source, changed_by:
  changedByLabel(row, actor ? actorNames.get(actor) ?? null : null, 'you'),
  is_current, summary: summarizeHistorySlots(resolveHistorySlots(pool,
  row.slots)) }` (the describe-entry trio is the KEYSTONE's);
- DEBUG **`Listed wardrobe history`** `{ context, chatId, characterId, total:
  rows.length, shown: entries.length }`;
- returns `{ success: true, current_index: rows.find(isCurrent)?.entryIndex ??
  null, total: rows.length, entries, ...(gap ? { gap_before_window: true } : {}) }`;
- catch → WARN **`Wardrobe history read failed`** `{ context, chatId,
  characterId, error }` and `failure(`Could not read the outfit record:
  ${message}`)`.

`formatWardrobeHistoryResults(output)` (`:122-136`), bytes:

```
!success → `Wardrobe Error: ${output.error || 'Unknown error'}`
total === 0 → 'No outfits recorded yet in this chat. The first change of clothes starts the record.'
lines = [`Outfits in this chat: ${total} (numbered 0 to ${total - 1}). Wearing now: ${current_index}.`]
entries.forEach((entry, i) => {
  if (i === 1 && gap_before_window) lines.push('  …')          // U+2026
  marker = entry.is_current ? ' ← wearing now' : ''            // U+2190
  lines.push(`  ${entry.index}. ${entry.at} — ${entry.changed_by}: ${entry.summary}${marker}`)   // U+2014
})
join('\n')
```

`current_index` null with rows present (no current row — a corrupt timeline)
renders the literal `null` (§6 T12). `at` is the row's raw `createdAt` ISO
string, not localized.

### 1.10 `lib/tools/handlers/wardrobe-tool-table.ts` — seven → nine (`ebb480edd`)

Module doc now says "The nine `wardrobe_*` tools". Two entries appended AFTER
`wardrobe_take_off` (`:111-135`):

```ts
wardrobe_rollback: async (input, context) => {
  const r = await executeWardrobeRollbackTool(input, context);
  return { success: r.success,
    result: r.success ? { formattedText: formatWardrobeRollbackResults(r), previous_index: r.previous_index,
      restored_index: r.restored_index, current_state: r.current_state, coverage_summary: r.coverage_summary,
      skipped: r.skipped } : null,
    error: r.success ? undefined : r.error };
},
wardrobe_history: async (input, context) => {
  const r = await executeWardrobeHistoryTool(input, context);
  return { success: r.success,
    result: r.success ? { formattedText: formatWardrobeHistoryResults(r), ...r } : null,
    error: r.success ? undefined : r.error };
},
```

The history payload's key order is `formattedText, success, current_index,
total, entries[, gap_before_window]` (the spread — `success: true` INSIDE the
result, like `wardrobe_read`). `WardrobeToolContext` (`wardrobe-handler-shared.ts:17`)
= `Omit<OutfitChangeContext, 'historyIntent'>` — it now carries `provenance` and
both containers.

### 1.11 Every site that lists `wardrobe_take_off` (seven wardrobe tools → nine)

| # | v4 site (pin) | hunk | v5 twin (line at `2e89d291a`) |
|---|---|---|---|
| S1 | `lib/tools/handlers/wardrobe-tool-table.ts:111-135` | the two entries (§1.10) | `tools/wardrobe_tool_table.rs` — `WARDROBE_TOOL_NAMES: [&str; 7]` `:86-94` → 9; `source_context` `:144-156`; `run_wardrobe_tool_handler` `:160-231`; `finish_wardrobe_tool`'s `source` match `:247-254`; `WardrobeToolOutput` `:110-121`; `to_value` / `format_at` / `run` `:291-421`; tests `:428-469` |
| S2 | `lib/chat/tool-executor.ts:264-268` `BUILT_IN_TOOLS` Set | `'wardrobe_rollback', 'wardrobe_history'` after `'wardrobe_take_off'` | `tools/executor.rs` `BUILT_IN_TOOLS` `:112-118` and `PORTED_TOOLS` `:179-185`; test `wardrobe_tools_are_ported` `:2199-2207` |
| S3 | `lib/tools/index.ts:404-428` | barrel exports (definition, validator, handler, formatter, types for each) | `tools/mod.rs` (`pub mod wardrobe_rollback; pub mod wardrobe_history;` beside `:41`) |
| S4 | `lib/tools/plugin-tool-builder.ts:72-73` imports; `:253-257` `BuildToolsOptions.wardrobeRollback?` / `wardrobeHistory?`; `:381-382` the `buildToolsForProvider called` DEBUG `options` object (two keys after `wardrobeTakeOff`); `:535-540` the two `universalTools.push` AFTER take-off, BEFORE create | | `services/tool_build.rs` `ToolBuildOptions` `:229` (+2 fields after `wardrobe_take_off`), `push_key` `:353-355` (+2 after `wardrobeTakeOff`), the options builder `:613` (`input.can_dress_themselves` ×2), the test `:830-836`; v5 does NOT port the `buildToolsForProvider called` DEBUG (grep: no `tracing::debug` in `tool_build.rs`) |
| S5 | `app/api/v1/tools/route.ts:43-44` imports; `:99-100` `BUILT_IN_TOOL_SCHEMAS` (after `wardrobe_take_off`); `:290-303` `BUILT_IN_TOOLS` rows; `:722-724` the availability `case`s | rows `{ id: 'wardrobe_rollback', name: 'Return to an Earlier Outfit', description: 'Return to an outfit worn earlier in this chat, by its number', source: 'built-in', category: 'wardrobe' }`, `{ id: 'wardrobe_history', name: 'Outfit History', description: 'List the numbered outfits worn in this chat', source: 'built-in', category: 'wardrobe' }`; unavailable reason `Character does not have wardrobe self-dressing enabled` | `services/tools_inventory.rs` — `BuiltInTool` rows `:66-72` (+2 after `:72`), the schema-key map `:116-122` (`("wardrobe_rollback", "wardrobeRollback")`, `("wardrobe_history", "wardrobeHistory")` after `:122`), the availability arm `:349` (+2 names); `assert_eq!(BUILT_IN_TOOLS.len(), 43)` `:403` → 45 |
| S6 | `lib/tools/simple-json-prompt.ts:36-37` imports; `:59-60` options; `:221-226` `renderToolEntry` — **rollback BEFORE history**, after take-off | | `tools/simple_json_prompt.rs` options `:33` (+2), entries `:219-221` (+`entry_for("wardrobeRollback")`, then `entry_for("wardrobeHistory")`) |
| S7 | `lib/tools/simple-json-parser.ts:105-107` `TOOL_NAME_ALIASES` | `rollback_outfit: 'wardrobe_rollback'`, `undo_outfit: 'wardrobe_rollback'`, `outfit_history: 'wardrobe_history'` (aliases ONLY — the canonical names are not in this map) | `tools/simple_json_parser.rs:109` (+ two arms after `take_off`) |
| S8 | `lib/tools/legacy/text-block-prompt.ts:47-50` options; `:191-204` the two blocks — **history BEFORE rollback** (the opposite of S6), after take-off, before create | bytes below | `tools/text_block_prompt.rs` options `:29` (+2), two NEW `const`s beside `WARDROBE_TAKE_OFF` `:125-130`, pushes after `:198-200` (history, then rollback) |
| S9 | `lib/tools/legacy/text-block-parser.ts:109-114` `TEXT_BLOCK_TOOL_NAME_MAP` | `'wardrobe_rollback' / 'rollback_outfit' / 'undo_outfit' → 'wardrobe_rollback'`; `'wardrobe_history' / 'outfit_history' → 'wardrobe_history'`; NO `PARAM_ALIAS_MAP` rows; numeric attribute strings become numbers in `convertTextBlockToToolCallRequest` (`:358-369` — `index="2"` → `2`, `limit="5"` → `5`) | `tools/text_block_parser.rs:156` (+2 arms); `has_param_alias_map` `:206-222` UNCHANGED |
| S10 | `lib/services/chat-message/pseudo-tool.service.ts:95-96` `TextBlockEnabledToolOptions`; `:130-131` `determineTextBlockToolOptions` (`canDressThemselves !== false`); `:160-161` and `:250-251` the text-block / simple-JSON forwarders | | `services/pseudo_tool.rs` struct `:49` (+2 fields after `wardrobe_take_off` — the `determine_text_block` NDJSON's key order), `:116` (+2 `can_dress`), `:140` and `:191` (+2 each) |
| S11 | `lib/services/chat-message/streaming.service.ts:173` doc comment; `:300-301` `buildTools`' options (`canDressThemselves !== false`) | | the native path's options builder is `services/tool_build.rs:613` (S4) |
| S12 | `lib/services/brahma-console/one-shot.service.ts:132-133`, `orchestrator.service.ts:252-253` — both `false` | | `services/brahma_console/mod.rs:278` and `services/brahma_console/orchestrator.rs:444` (+2 `false` each) |
| S13 | `lib/services/scenario-builder/scenario-builder.service.ts:206-207` — both `false` | | `services/scenario_builder/mod.rs:401` (+2 `false`) |
| S14 | `components/chat/ToolMessage.tsx:93` `WARDROBE_ACTION_TOOLS` gains `'wardrobe_rollback'` (NOT history); `:125-137` the rollback summary; `:337-346` `toolInfo` rows | bytes below | SPA `apps/web/src/app/chat/tool-message.ts` — `WARDROBE_ACTION_TOOLS` `:49-55`, `TOOL_INFO` `:73-79`, `buildWardrobeActionSummary` `:93-140` |
| S15 | `lib/services/chat-message/orchestrator.service.ts:1077` | comment only | — |
| S16 | `lib/tools/__tests__/tool-definitions-snapshot.test.ts` `ALL_TOOLS` (+2 after `wardrobeTakeOff`, before `webSearch`) + `.snap` +30 | | `harness/oracle/cases/tool-definitions.ts` (imports + `ALL_TOOLS` `:153`), `tool-definitions-canonical.ts` (`:84`, `:148`); `tools/definitions/data.rs` (61 → 63, regen), `tools/definitions/mod.rs:101` (`assert_eq!(TOOL_DEFINITIONS.len(), 61)` → 63) |

Text-block prompt bytes (`text-block-prompt.ts:191-204`):

```
if (options.wardrobeHistory) toolDocs.push(`
### Outfit History
List the outfits you have worn in this chat, numbered from 0 (what you arrived in). The one you wear now is marked.
[[OUTFIT_HISTORY /]]
Only the most recent few: [[OUTFIT_HISTORY limit="5" /]]`)

if (options.wardrobeRollback) toolDocs.push(`
### Return to an Earlier Outfit
Step back one outfit: [[ROLLBACK_OUTFIT /]]
Return to a numbered outfit from the history: [[ROLLBACK_OUTFIT index="2" /]]`)
```

(each string starts with `\n`, as the neighbouring entries do.)

`ToolMessage.tsx` bytes (`:125-137`, `:337-346`):

```ts
if (toolData.toolName === 'wardrobe_rollback') {
  const restored = result.restored_index as number | undefined
  if (typeof restored !== 'number') return null
  lines.push(`Returned to outfit ${restored}.`)
  const skipped = (result.skipped as Array<{ title?: string | null; item_id?: string; reason?: string }> | undefined) ?? []
  for (const item of skipped) {
    const name = item.title ?? item.item_id ?? 'An item'                     // NULLISH, not truthy
    lines.push(`${name} could not be put back on (${item.reason === 'archived' ? 'archived' : 'no longer exists'}).`)
  }
  const coverageSummary = result.coverage_summary as string | undefined
  if (coverageSummary) lines.push(coverageSummary)
  return { label: 'Wardrobe', lines }
}
…
wardrobe_rollback: { displayName: 'Earlier Outfit', icon: '👗', bgColor: 'qt-bg-muted border qt-border-default' },
wardrobe_history:  { displayName: 'Outfit History', icon: '👗', bgColor: 'qt-bg-muted border qt-border-default' },
```

(The summary only runs for a SUCCESSFUL tool row — `buildWardrobeActionSummary`'s
existing guard. History renders no summary lines.)

The Google request builder needs nothing (v4's commit message; v5's
`model/request_builder/google.rs` strips by FIELD NAME — `UNSUPPORTED_SCHEMA_FIELDS`
`:24-32`; `exclusiveMinimum` is not in it, so it rides the wire, as v4's does).

### 1.12 The threading hunks (`05005c562`) — exact v4 lines

| v4 file:lines (pin) | hunk |
|---|---|
| `lib/chat/tool-executor.ts:6` | `import { turnContainersOf, type WardrobeTurnContainers } …` |
| `tool-executor.ts:189-193` | `ToolExecutionContext.pendingWardrobeHistory?: WardrobeTurnContainers['pendingWardrobeHistory']` beside `pendingWardrobeAnnouncements?` (`:188`) |
| `tool-executor.ts:633` | the `run_custom` arm: `...turnContainersOf(context)` (was `pendingWardrobeAnnouncements: context.pendingWardrobeAnnouncements`) |
| `tool-executor.ts:725-738` | the wardrobe arm: `provenance: { source: 'character', ...(characterId ? { actorCharacterId: characterId } : {}) }` + `...turnContainersOf(context)`; comment "and every change is recorded as the calling character's" |
| `lib/services/chat-message/tool-execution.service.ts:321` | `createToolContext`: `...newWardrobeTurnContainers()` (was `pendingWardrobeAnnouncements: new Set<string>()`) |
| `lib/services/brahma-console/one-shot.service.ts:153` | `...newWardrobeTurnContainers()` |
| `lib/services/brahma-console/orchestrator.service.ts:492` | `...newWardrobeTurnContainers()` |
| `lib/tools/handlers/run-custom-handler.ts:18`, `:58-59` | `RunCustomToolContext.pendingWardrobeHistory?` beside the Set; doc "The effects applier supplies the `'pascal'` provenance." |
| `run-custom-handler.ts:193` | `...turnContainersOf(context)` into `runCustomTool` |
| `lib/pascal/run-service.ts:29`, `:87-88` | `RunCustomToolParams.pendingWardrobeHistory?` beside the Set (doc reworded) |
| `run-service.ts:173` | `...turnContainersOf(params)` into `applyCustomToolEffects` |
| `lib/pascal/side-effects.ts:70`, `:136-137` | `ApplyCustomToolEffectsParams.pendingWardrobeHistory?` |
| `side-effects.ts:163` | `...turnContainersOf(params)` into `applyWardrobeEffects` |
| `lib/pascal/wardrobe-effects.ts:41`, `:77` | `ApplyWardrobeEffectsParams extends WardrobeTurnContainers` (the inline Set field deleted) |
| `wardrobe-effects.ts:219-229` | `provenance: { source: 'pascal', actorCharacterId: characterId, toolName }` + `...turnContainersOf(params)` |
| `wardrobe-effects.ts:237` | UNCHANGED: `announcement: params.pendingWardrobeAnnouncements ? 'joined the turn' : 'enqueued now'` — still keyed on the SET alone |
| `lib/tools/handlers/wardrobe-archive-handler.ts:24`, `:109-110` | `provenance: context.provenance, ...turnContainersOf(context)` |
| `lib/tools/handlers/wardrobe-create-handler.ts:33`, `:268-269` | same, for `targetCharacterId` |
| `lib/tools/handlers/wardrobe-handler-shared.ts:5`, `:11-17`, `:186-188` | `WardrobeToolContext = Omit<OutfitChangeContext, 'historyIntent'>`; `finalizeWardrobeMutation(repos, context: WardrobeToolContext, …)` |
| `app/api/v1/chats/[id]/actions/outfit.ts:274-284` | the equip provenance (§1.4 #1) |
| `lib/wardrobe/apply-outfit-selections.ts:538`, `:540-544` | `continue` + the settle (§1.6) |

NOT in this span (verified): `lib/pascal/trigger-runner.ts`,
`app/api/v1/chats/[id]/actions/run-tool.ts`, `app/api/v1/custom-tools/route.ts`
pass NO containers — their Pascal changes settle immediately with `pascal`
provenance through the applier. v4 `scenario-builder.service.ts:231-236`'s
`ToolExecutionContext` carries NO announcement set (v5's
`services/scenario_builder/mod.rs:214` mints one — a pre-existing v5 divergence,
§8 Q5).

### 1.13 Brahma

Both surfaces mint `...newWardrobeTurnContainers()` on their operator-surface
`ToolExecutionContext` (`one-shot.service.ts:145-154`, `orchestrator.service.ts:487-493`)
and set every wardrobe tool `false` (§1.11 S12). Neither surface flushes. With
every wardrobe tool off, nothing ever lands in either container; the port is the
factory call + the two `false`s.

### 1.14 `POST /api/v1/chats/[id]?action=wardrobe-rollback` (`8d8618819` + `4ae20977f`)

Dispatch: `handlers/post.ts:37` import, `:106` `'wardrobe-rollback': () =>
handleWardrobeRollback(req, chatId, ctx)` — the key sits **between `equip` and
`toggle-avatar-generation`** in the thunk map (the `availableActions` order of
the unknown/bare refusal). `route.ts:38` and `actions/index.ts:31` doc / export
lines. **The route-level `handlePost` (`post.ts:72-76`) reads the chat FIRST →
404 `Chat`**, before the body is parsed.

Handler (`actions/outfit.ts:296-299`, `:366-416`):

```ts
const rollbackBodySchema = z.object({
  characterId: z.string().min(1, 'characterId is required'),
  index: z.number().int().nonnegative(),             // REQUIRED here (optional on the tool)
});
…
try {
  const { characterId, index } = rollbackBodySchema.parse(await req.json());
  const chat = await repos.chats.findById(chatId);
  if (!chat) return notFound('Chat');
  const pool = await loadWearablePool(repos, characterId, undefined, { chatId, operator: true });
  const outcome = await rollbackWardrobe(repos, {
      chatId, pool, index,
      context: { userId: ctx.user.id, chatId, characterId, provenance: operatorWardrobeProvenance(chat) },
      source: 'ui' },
    '[Chats v1] wardrobe-rollback');
  if (!outcome.ok) {
    logger.info('[Chats v1] Wardrobe rollback refused', { chatId, characterId, index, reason: outcome.reason, context: 'wardrobe' });
    return badRequest(outcome.message);
  }
  logger.info('[Chats v1] Wardrobe rolled back', {
    chatId, characterId, previousIndex: outcome.previousIndex, restoredIndex: outcome.restoredIndex,
    skipped: outcome.skipped.length, context: 'wardrobe' });
  return NextResponse.json({ equippedSlots: outcome.slots, previousIndex: outcome.previousIndex,
    restoredIndex: outcome.restoredIndex, skipped: outcome.skipped });
} catch (error) {
  if (error instanceof z.ZodError) return badRequest(error.issues.map((e) => e.message).join(', '));
  logger.error('[Chats v1] Error rolling back wardrobe', { chatId }, error instanceof Error ? error : undefined);
  return serverError('Failed to roll back the outfit');
}
```

Wire facts:

- **200** body key order `equippedSlots, previousIndex, restoredIndex, skipped`;
  `skipped` items are the CAMELCASE `{ itemId, title, reason }` (the tool's are
  snake_case `{ item_id, title, reason }`).
- **400** Zod — the issue messages joined `', '`: an empty `characterId` →
  `characterId is required`; a missing / non-string `characterId`, a missing /
  non-number / non-integer / negative `index` → Zod 4.6.7's own messages (to be
  MEASURED through the oracle, never hand-written — §6 T15).
- **400** refusal — the refusal's sentence as `{ error }`. **`at_first` is
  UNREACHABLE here** (index is required — the doc comment names only
  `no_history`, `out_of_range`, `already_current`).
- **404** `Chat` (the route gate; the handler's own re-read is a race arm).
- **500** `Failed to roll back the outfit` on a non-Zod throw — INCLUDING a
  malformed JSON body (`req.json()` throws a `SyntaxError`, not a `ZodError`),
  with ERROR `[Chats v1] Error rolling back wardrobe` `{ chatId }`.
- Log order on success: `[Chats v1]`-less chokepoint lines, the avatar
  trigger's, DEBUG `Resolved operator for an outfit change` (BEFORE the pool
  load — `operatorWardrobeProvenance(chat)` is evaluated while building the
  input), INFO `Rollback left a stale item off`×n (context `[Chats v1]
  wardrobe-rollback`), the commit's lines, the avatar refresh's, DEBUG
  `Wardrobe outfit announcement scheduled`, the settle's cursor DEBUG (keystone),
  DEBUG `Rolled a wardrobe back`, INFO `[Chats v1] Wardrobe rolled back`.

The equip arm's `4ae20977f` change (`outfit.ts:274-284`): `const chat = await
repos.chats.findById(chatId)` AFTER the write — a THROW there lands in the
handler's catch → 500 `Failed to equip wardrobe slot` although the outfit was
already written (§6 T5); absent → bare `{ source: 'operator' }`.

### 1.15 Log-line census (verbatim; level — message — context — fields, in order)

| level | message | context | fields | site |
|---|---|---|---|---|
| WARN | `Failed to settle wardrobe history` | `sourceContext` / `orchestrator-turn-end` | `context, chatId, characterId, intent, error` | `outfit-change-effects.ts:75` |
| DEBUG | `Wardrobe outfit announcement scheduled` | unchanged | `context, chatId, characterId` | `:109` |
| WARN | `Failed to schedule wardrobe outfit announcement` | unchanged | `context, chatId, characterId, error` | `:115` |
| DEBUG | `Resolved operator for an outfit change` | `wardrobe-history` | `context, chatId, actorCharacterId` (null when absent) | `operator-provenance.ts:37` |
| INFO | `Rollback left a stale item off` | `sourceContext` | `context, chatId, characterId, targetIndex, itemId, reason` | `rollback.ts:148` |
| DEBUG | `Rolled a wardrobe back` | `sourceContext` | `context, chatId, characterId, previousIndex, restoredIndex, skipped, provenance` | `rollback.ts:172` |
| WARN | `Wardrobe rollback tool validation failed` | `wardrobe-rollback-handler` | `context, chatId, characterId, input` | handler `:36` |
| INFO | `Wardrobe rollback refused` | `wardrobe-rollback-handler` | `context, chatId, characterId, reason` | handler `:57` |
| WARN | `Wardrobe rollback failed` | `wardrobe-rollback-handler` | `context, chatId, characterId, error` | handler `:77` |
| DEBUG | `Listed wardrobe history` | `wardrobe-history-handler` | `context, chatId, characterId, total, shown` | handler `:94` |
| WARN | `Wardrobe history read failed` | `wardrobe-history-handler` | `context, chatId, characterId, error` | handler `:111` |
| INFO | `[Chats v1] Wardrobe rollback refused` | `wardrobe` (LAST) | `chatId, characterId, index, reason, context` | `outfit.ts:395` |
| INFO | `[Chats v1] Wardrobe rolled back` | `wardrobe` (LAST) | `chatId, characterId, previousIndex, restoredIndex, skipped, context` | `outfit.ts:399` |
| ERROR | `[Chats v1] Error rolling back wardrobe` | — | `chatId` + the error | `outfit.ts:413` |
| DEBUG | `Custom tool wardrobe effects applied` | `pascal.wardrobe-effects` | UNCHANGED (`announcement` still keyed on the Set) | `wardrobe-effects.ts:230` |

The repository's own lines (`Recorded wardrobe history entry`, `Outfit unchanged
since the current entry; nothing recorded`, `Moved wardrobe history cursor`,
`Wardrobe history cursor target missing; cursor left where it was`, `Wardrobe
history settle failed; …`) are the KEYSTONE's; this slice's families must
capture them only as far as they prove the intent arrived (§4).

---

## 2. v5 on `main` (`2e89d291a`) — where each lands, and the ownership confinement

### 2.1 `crates/quilltap-core/src/services/outfit_change_effects.rs` (203 lines) — THIS LANE OWNS THE FILE

- `OutfitChangeContext<'a>` `:36-44` — `#[derive(Clone, Copy, Debug)]`, borrowed
  `user_id` / `chat_id` / `character_id` / `pending_wardrobe_announcements:
  Option<&'a Arc<Mutex<HashSet<String>>>>`. Gains `provenance`,
  `history_intent`, and the history Map (§3.1).
- `refresh_avatar_for_outfit` `:53-73`, `schedule_wardrobe_announcement`
  `:83-109` — unchanged.
- `record_pending_wardrobe_announcement` `:114-137` — gains the history half
  (the branch table, §1.3). v5 does not need a `repos` parameter (it has `db`
  already).
- `notify_wardrobe_changed` `:142-156` — unchanged body; the ctx carries more.
- `flush_pending_wardrobe_announcements(db, user_id, chat_id, pending)`
  `:174-203` — gains the Map parameter (or takes the containers), drains BOTH
  under their locks before any await, SETTLES before each schedule. v5 SORTS the
  drained ids (`:192`, the recorded W4.1d2 decision); with the settle added the
  order changes only the order of per-character settles/realtime publishes and
  the DEBUG lines — still not observable in any job / row (§6 T3).
- NEW: a private `settle_history(db, chat_id, character_id, intent,
  source_context)` → the keystone's async `settle`; an `Err` → WARN `Failed to
  settle wardrobe history` with `intent` rendered as v4's JSON object.

Callers to update (all borrowed-ctx constructions): `pascal/wardrobe_effects.rs:444`,
`tools/wardrobe_tool_table.rs:257`, `api/chat_outfits.rs:840`; the flush at
`services/orchestrator.rs:3644-3650`; harness
`outfit_change_effects_tier3_equivalence.rs:329,339,368,377,537-545`,
`pascal_wardrobe_effects_equivalence.rs:676-680`.

### 2.2 `crates/quilltap-core/src/api/chat_outfits.rs` (1,112 lines) — equip provenance vs the NEW rollback action

| Hunk | v4 | v5 lines | owner |
|---|---|---|---|
| equip: read the chat AFTER the write, `operator_wardrobe_provenance(&chat)` (or bare), pass `provenance` in the ctx | `05005c562` + `4ae20977f` (`outfit.ts:274-284`) | `chat_equip` `:690-853` — ONLY the tail `:836-850` (between the `Err(e)` arm's `return internal(...)` and `notify_wardrobe_changed`) | this lane |
| a chat-read failure in that tail → ERROR `[Chats v1] Error equipping wardrobe slot` + 500 `Failed to equip wardrobe slot` (the outfit stays written) | `outfit.ts:287-292` catch | same tail | this lane |
| NEW `pub async fn chat_wardrobe_rollback(db, user_id, chat_id, body) -> Response` + its Zod-faithful `parse_rollback_body` | `8d8618819` (`outfit.ts:296-299`, `:366-416`) | NEW functions APPENDED after `put_on_or_fit` (`:855-924`), before the regenerate-avatar section (`:926`) | this lane |
| GET `?action=wardrobe-history` (`chat_wardrobe_history` read) | `8d8618819` GET half | NEW functions — **NOT this lane**; the history-API lane appends its own block. Agree: GET block after the rollback block, or in a sibling file | history-API lane |

Reusable inside the file: `received()` `:283`, `bad_request` `:128`,
`not_found` `:132`, `internal` `:124`, the pool load idiom of `put_on_or_fit`
`:855-900` (`with_both_conns` + `load_wearable_pool(…, LoadWearablePoolOptions {
operator: true, chat_id: Some(&cid), .. })`).

Transport plumbing (this lane): `api/types.rs:2672` beside `ChatEquip` → NEW
`Request::ChatWardrobeRollback { chat_id, #[serde(flatten)] body }` (the P4.6w
raw-flatten idiom, wire type `chatWardrobeRollback`); `api/engine.rs:5414-5419`
beside `ChatEquip` → its arm; `crates/quilltap-web/src/wardrobe_routes.rs`
`CHAT_POST_ACTIONS` `:634-686` — insert `"wardrobe-rollback"` AFTER `"equip"`
(`:663`) and BEFORE `"toggle-avatar-generation"`; `chat_action_post` `:691-720`
— a REST arm beside `"equip"` (`:713-719`) with `parse_body(&body, "Failed to
roll back the outfit")`. The `COLLAPSE_CENSUS` row for
`crates/quilltap-web/src/wardrobe_routes.rs`
(`crates/quilltap-harness/tests/web_edge_body_parse_guard.rs:187-196`) — its
expected collapsing-site count moves by one if the new arm parses through the
same collapsing helper; adjudicate it in the row's text. The SPA contract
(`apps/web/src/app/core/core-contract.ts` beside `ChatEquipRequest` `:8224-8240`)
is the SPA lane's (§3.3).

### 2.3 `tools/` — the two tools, the table, the executor

- NEW `tools/wardrobe_rollback.rs` (validate → stage inside the write → format)
  and `tools/wardrobe_history.rs` (validate → read inside the write → format);
  `tools/mod.rs` `pub mod` lines (`:41` neighbourhood).
- `tools/wardrobe_tool_table.rs` (531 lines): `WARDROBE_TOOL_NAMES` `:86-94`
  (7 → 9; the doc "The seven" → "nine" at `:1`, `:85`); `WardrobeToolContext`
  `:100-106` gains `provenance` + the history Map; `WardrobeToolOutput` `:110-121`
  gains `Rollback(..)` / `History(..)`; **`WardrobeToolStage` `:125-131` gains
  the per-change INTENT** — `changed: Vec<String>` becomes `changed:
  Vec<(String, Option<WardrobeHistoryIntent>)>` or a sibling
  `history_intent: Option<WardrobeHistoryIntent>` (rollback's cursor; everyone
  else `None` → `append` with the ctx provenance); `source_context` `:144-156`
  (+2: `wardrobe-rollback-handler`, `wardrobe-history-handler`);
  `run_wardrobe_tool_handler` `:160-231` (+2 arms); `finish_wardrobe_tool`
  `:236-289` (the provenance + containers into each `OutfitChangeContext`
  `:255-264`; the rollback's DEBUG `Rolled a wardrobe back` must be emitted HERE,
  after the notify — §6 T8); `to_value` / `format_at` / `run` `:291-421` (+2
  arms: the rollback projection `{formattedText, previous_index, restored_index,
  current_state, coverage_summary, skipped}`; history = `spread(formatted, o)`).
  Tests `:428-506` (`covers_exactly_the_seven…` → nine; the bug-193 set test
  grows to assert the Map too).
- `tools/executor.rs` (2,241 lines): `BUILT_IN_TOOLS` `:112-118`,
  `PORTED_TOOLS` `:179-185`, the test `:2199-2207` (+2 each); `run_run_custom`
  `:667-690` (`:683` — thread the Map beside the Set into
  `RunCustomToolContext`); `run_wardrobe_tool` `:1640-1690` (the ctx at
  `:1675-1680` gains `provenance: { source: Character, actor: ctx.character_id
  filtered non-empty }` — derive it from `ctx.character_id` BEFORE
  `wardrobe_snapshot`'s `unwrap_or_default()` `:1866-1878` flattens `None` to
  `""`, §6 T10 — plus the Map).
- `tools/wardrobe_shared.rs` (178 lines): `load_current_wardrobe_state`
  `:105-113` is reusable; there is NO `format_equipped_slot_lines` twin — v5
  inlines the per-slot lines in `wardrobe_wear.rs:330-345` and
  `wardrobe_take_off.rs:363-390`; the rollback formatter needs them (extract a
  shared helper here, or inline a third copy with v4's `hair` rule).
- `tools/definitions/data.rs` (GENERATED; 61 → 63 after `wardrobeTakeOff`
  `:305-308`), `tools/definitions/mod.rs:101`.
- `tools/text_block_parser.rs:156`, `tools/simple_json_parser.rs:109`,
  `tools/text_block_prompt.rs:29,125-130,198-200`,
  `tools/simple_json_prompt.rs:33,219-221` (§1.11).

### 2.4 `services/tool_execution.rs` (1,103 lines) — the context + the factory

- `ToolExecutionContext` `:257-306`: the field `pending_wardrobe_announcements`
  `:279-293` (doc + type). Add the history Map sibling (or fold both into the
  containers struct — §3.1); keep `None`-means-absent (the operator Run Tool,
  `services/chat_run_tool.rs:183`, `:338` asserts it).
- `create_tool_context` `:310-342`: `:337` → the factory.

### 2.5 `services/orchestrator.rs` (5,916 lines)

`:3371-3376` mints the turn's ONE set (and OVERRIDES `create_tool_context`'s at
`:3412` and `:3478`); `:3637-3650` the turn-end drain. Both become the
containers: mint ONE `WardrobeTurnContainers` at `:3375`, hand clones at `:3412`
/ `:3478`, drain both at `:3644`. `turn_tool_context` `:776-788` unchanged.

### 2.6 Brahma / Scenario Builder

`services/brahma_console/mod.rs:275-281` (+2 `false`), `:295-297` (the factory);
`services/brahma_console/orchestrator.rs:441-447` (+2 `false`), `:717` (the
factory); `services/scenario_builder/mod.rs:398-404` (+2 `false`); `:214` — §8 Q5.

### 2.7 The arrivals — `services/outfit_selections.rs` (2,060 lines) + `services/chat_participants.rs`

- `commit_selection` `:716-735`: after `commit_equipped_outfit` succeeds, settle
  `append` with `{ source: <ctx.source as history source> }` on the SAME `main`
  writer connection (the function already runs on the writer: `main:
  &Connection`); on `Err` → `log_persist_failure` and NO settle. Reached from
  `apply_outfit_selections` `:664-708` (chat create, `chat_create.rs:1633`, and
  the llm path) and `apply_outfit_selection_sync` `:853-865` (merge,
  `chat_merge.rs:498`).
- `services/chat_participants.rs` `apply_outfit_for_added_participant`
  (`:1321`…): the commit is INLINED at `:1517-1545` inside its own `db.write`;
  the settle goes inside the same closure after `Ok(_)` (the `Ok(Ok(_)) => {}`
  arm at `:1535`), source `participant-added`.
- The `EquipSource` → history `source` mapping is the identity for the three
  arrival members (`db/wardrobe_wear_stats.rs:232-250`); the keystone's history
  source enum must not depend on `EquipSource` (it has `operator` / `character`
  / `pascal` / `seeded`, which `EquipSource` lacks).

### 2.8 The Pascal four — the OWNERSHIP CONFINEMENT TABLE

The sibling Pascal lane ports `5cb0e0190` (Workbench wardrobe picker) +
`a05c6f755` (`{{character}}` + `announceAsCharacter`). **Measured from the v4
hunks:** neither Pascal commit touches `lib/pascal/wardrobe-effects.ts`,
`lib/tools/handlers/run-custom-handler.ts`, `lib/chat/tool-executor.ts`, or
`lib/pascal/side-effects.ts` (`git show --stat 5cb0e0190 a05c6f755 -- lib`
lists `bench-wardrobe.ts`, `tool-draft.ts`, `workbench.ts`, `wardrobe/pool.ts`,
`wardrobe/wardrobe-container.ts`, `wardrobe/wearable-ref.ts`,
`custom-tool.types.ts`, `custom-tools.ts`, `placeholders.ts`, `run-service.ts`,
`tool-vocabulary.ts`, `trigger-runner.ts`, `schemas/chat.types.ts`,
`services/pascal/writer.ts`). The only v4 file BOTH lanes edit is
`lib/pascal/run-service.ts`; in v5 the overlap is wider because Rust struct
literals must name every field.

| v5 file | THIS lane's hunks (v5 lines at `2e89d291a`) | the Pascal lane's hunks | collision / rule |
|---|---|---|---|
| `pascal/run_service.rs` (647) | `RunCustomToolParams` field `pending_wardrobe_announcements` `:109-114` (doc + type; add the history Map sibling right after it, as v4 `:87-88`); the `apply_effects_for_run(…, params.pending_wardrobe_announcements)` argument `:266-276` (pass both containers); the test literal's LAST field `:496` | v4 `a05c6f755` `run-service.ts:66-70` `characterName?` → a NEW field after `metadata` `:96`; `resolveCharacterName` (`:107` + the new fn after `:233`) → the top of `run_custom_tool` `:141-…` (before the cascade) and a new private fn; `:134` the `character` override into the executor; `:186-197` `announcerCharacterId` into `post_pascal_result` (`:300-…`) and the DEBUG `announcedBy` field (`run-service.ts:230`); the test literal's `metadata:` neighbourhood `:490` | **Disjoint by rule:** Pascal adds `character_name` IMMEDIATELY AFTER `metadata` (v4's position) in the struct and in EVERY literal; this lane edits ONLY the `pending_wardrobe_*` line(s) (the LAST field) in the struct and every literal. Never reorder fields. The two edit points are ≥ 13 lines apart in the struct and 6 apart in the test literal — cherry-pick-safe if neither lane reflows the block. |
| `pascal/wardrobe_effects.rs` (463) | `finish_wardrobe_effects` `:427-463` ONLY: the `pending` parameter (→ both containers), the `OutfitChangeContext` at `:444-449` (`provenance: { source: Pascal, actor: Some(character_id), tool_name: Some(tool_name) }`); the DEBUG's `announcement` word stays keyed on the SET alone (`:460`) | none in v4. If `{{character}}` ever reaches a wardrobe ref at apply time it is resolved upstream in `custom_tools.rs` (`OutcomeSubjects`, `a05c6f755` `custom-tools.ts`), not here. Confine any Pascal edit to the in-write half `apply_wardrobe_effects` `:145-415` | disjoint (`:1-426` Pascal, `:427-463` this lane) |
| `tools/run_custom.rs` (939) | `RunCustomToolContext` field `pending_wardrobe_announcements` `:500-505` (+ the history Map sibling); the `RunCustomToolParams` literal's LAST field `:761` (both containers) | the same literal `:743-762` gains `character_name: None` AFTER `metadata: &metadata,` `:751` (v4's handler passes NO `characterName`, so the service reads the raw row); nothing else in v4's `run-custom-handler.ts` / `run-custom-tool.ts` moves | disjoint lines in ONE literal (`:751` vs `:761`) |
| `services/tool_execution.rs` (1,103) | `ToolExecutionContext` `:279-293` (the field + its doc), `create_tool_context` `:307-342` (the factory at `:337`) | **none** — v4's Pascal commits never touch `tool-executor.ts`'s context or `tool-execution.service.ts` | this lane alone |
| `tools/executor.rs` (2,241) — the actual twin of `tool-executor.ts`'s dispatcher | `:112-118`, `:179-185`, `:683`, `:1640-1690`, `:2199-2207` | none in v4 | this lane alone |
| `pascal/side_effects.rs` (1,799) | `apply_effects_for_run` `:924-1006` ONLY — the `pending` parameter `:935` and its forward to `finish_wardrobe_effects` `:974-982` (both containers; the doc `:919-922`) | none in v4 (`side-effects.ts` not in either Pascal commit) | this lane alone |
| `pascal/trigger_runner.rs` | the `RunCustomToolParams` literal's LAST field `:272` (type only — stays `None`) | `a05c6f755` `trigger-runner.ts` passes `characterName: character.name` → a field after `metadata` `:270` | disjoint lines in one literal |
| `api/custom_tools.rs` | the literal's LAST field `:836` (stays `None`) | `a05c6f755` route + `resolve_metadata`'s return → `character_name` after `metadata` `:831`; `5cb0e0190` `?action=preview`'s `wardrobeResolution` (bench) — elsewhere in the file | disjoint lines |

**Recommendation that keeps the confinement trivially true:** do NOT rename the
existing `pending_wardrobe_announcements` field anywhere; ADD a sibling
`pending_wardrobe_history` field directly after it (exactly v4's move — v4 added
a sibling field on all four param structs and left the Set field alone). Every
literal then changes by ONE added line after its last field, never touching the
Pascal lane's `character_name` line after `metadata`.

---

## 3. Shared contracts

### 3.1 The Rust containers + context (owner: THIS lane; consumers: the keystone's `settle`, the Pascal lane's struct literals)

```rust
// services/outfit_change_effects.rs (or a pure sibling module — v4's turn-containers.ts)
pub type PendingWardrobeAnnouncements = Arc<Mutex<HashSet<String>>>;              // exists today
pub type PendingWardrobeHistory = Arc<Mutex<HashMap<String, WardrobeHistoryIntent>>>;

/// v4 `newWardrobeTurnContainers()` — BOTH, created together.
pub fn new_wardrobe_turn_containers() -> (PendingWardrobeAnnouncements, PendingWardrobeHistory);

#[derive(Clone, Copy, Debug)]
pub struct OutfitChangeContext<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
    /// v4 REQUIRED `provenance` — not an Option: no caller may forget it.
    pub provenance: &'a WardrobeHistoryProvenance,
    /// v4 `historyIntent?` — rollback only.
    pub history_intent: Option<&'a WardrobeHistoryIntent>,
    pub pending_wardrobe_announcements: Option<&'a PendingWardrobeAnnouncements>,
    pub pending_wardrobe_history: Option<&'a PendingWardrobeHistory>,
}

pub async fn flush_pending_wardrobe_announcements(
    db: &Db, user_id: &str, chat_id: &str,
    pending: Option<&PendingWardrobeAnnouncements>,
    history: Option<&PendingWardrobeHistory>,
);
```

The keystone OWNS (in its `db/chat_wardrobe_history.rs` or the types module)
and this lane CONSUMES:

```rust
pub enum WardrobeHistorySource { ChatStart, ParticipantAdded, Merge, Operator, Character, Pascal, Seeded }
   // serde: "chat-start" | "participant-added" | "merge" | "operator" | "character" | "pascal" | "seeded"
pub struct WardrobeHistoryProvenance {
    pub source: WardrobeHistorySource,           // never Seeded at a runtime call site
    pub actor_character_id: Option<String>,      // absent → key OMITTED (v4 `?:`), not null
    pub tool_name: Option<String>,               // pascal only
}
pub enum WardrobeHistoryIntent {
    Append { provenance: WardrobeHistoryProvenance },
    Cursor { index: i64 },
}
// settle — callable BOTH ways, never failing in its observable effect:
pub fn settle(main: &Connection, chat_id: &str, character_id: &str, intent: &WardrobeHistoryIntent) -> SettleOutcome; // inside a write (the arrivals, rollback-in-write if chosen)
pub async fn settle_db(db: &Db, chat_id: &str, character_id: &str, intent: &WardrobeHistoryIntent) -> Result<SettleOutcome, DbError>; // after-commit (the effects module); Err = writer gone → this lane's WARN
```

The intent's `Serialize` must produce v4's object bytes for the WARN `intent`
field: `{"kind":"append","provenance":{"source":"character","actorCharacterId":"…"}}`
(key order `kind, provenance`; inside `source, actorCharacterId, toolName`,
absent keys omitted) and `{"kind":"cursor","index":3}`. The realtime publish
(`publish_realtime(RealtimeTopic::Chats, Some(chat_id))`, `realtime/bus.rs:188`)
belongs INSIDE `settle` on `appended` / `cursor-moved` (keystone).

### 3.2 `operator_wardrobe_provenance` (owner: this lane)

`fn operator_wardrobe_provenance(chat: &Value) -> WardrobeHistoryProvenance`,
built exactly as `services/user_identity_resolver.rs:140-152` builds its
Speaking-As lookup (`participants_of` → `to_filter_participant` →
`chats_impersonation::read_impersonating(chat)` →
`participant_filters::find_active_user_participant` `:91`), with the chat's OWN
`activeTypingParticipantId` (v4 reads it off the chat row here, not off a
request). Empty `character_id` → bare `operator`; DEBUG `Resolved operator for
an outfit change` `{ context: "wardrobe-history", chatId, actorCharacterId }`
(`null` only when the seat has no `characterId`; `""` stays `""`).

### 3.3 The POST wire (owner: this lane's server half; consumer: the SPA lane's History tab "Wear again")

Dispatch request `{ type: 'chatWardrobeRollback', chatId, characterId, index }`
(flattened raw body, the `chatEquip` idiom). REST `POST
/api/v1/chats/{id}?action=wardrobe-rollback` body `{ characterId, index }`.
200 `{ equippedSlots: EquippedSlots, previousIndex: number, restoredIndex:
number, skipped: Array<{ itemId: string; title: string | null; reason:
'archived' | 'missing' }> }`; 400 `{ error }` (Zod-joined or the refusal
sentence); 404 `{ error: 'Chat not found' }` (whatever v5's `not_found("Chat")`
renders — `chat_outfits.rs:132`); 500 `{ error: 'Failed to roll back the outfit' }`.
The `chats` realtime topic fires from the cursor move (keystone), which is what
refetches the SPA's history query — the POST answer itself carries no history.

### 3.4 The tool wire (owner: this lane; consumer: the SPA `tool-message.ts`, the transcript readers)

Rollback tool-row `result` (success): `{ formattedText, previous_index,
restored_index, current_state, coverage_summary, skipped: [{ item_id, title,
reason }] }`; failure `result: null` + `error`. History success: `{
formattedText, success: true, current_index, total, entries: [{ index, at,
source, changed_by, is_current, summary }], gap_before_window? }`.

---

## 4. The differential plan

### 4.1 Tier 1 — the model-visible bytes (no new family needed except where noted)

- **`tool_definitions_equivalence`** (`crates/quilltap-harness/tests/tool_definitions_equivalence.rs`;
  oracle `harness/oracle/cases/tool-definitions.ts` + `tool-definitions-canonical.ts`):
  grow BOTH cases' import lists and `ALL_TOOLS` (after `wardrobeTakeOff`), REGEN
  `data.rs` through `harness/oracle/tools/gen-tool-catalog.mjs` (recipe in its
  header) — 61 → 63 — and bump `definitions/mod.rs:101`. Never hand-write the
  two JSON blobs (the jest snapshot SORTS keys; Zod 4's emitter order is what the
  wire carries).
- **`tools_inventory_equivalence`** (`chat-dialogs-tools` jest case, header
  `:17-19`): +2 rows, the schema-key map, the availability arm; a dress-off
  corpus row proving both report `Character does not have wardrobe self-dressing
  enabled`; `tools_inventory.rs:403` 43 → 45.
- **`pseudo_tool_prompts_equivalence`** (`harness/oracle/cases/pseudo-tool-prompts.ts`):
  add `wardrobeRollback: true, wardrobeHistory: true` to `all-on` and a new combo
  `wardrobe-history-rollback` (`{ wardrobeTakeOff: true, wardrobeRollback: true,
  wardrobeHistory: true, wardrobeCreate: true }` — pins the two OPPOSITE
  orders, S6 vs S8); the `determine_text_block` rows' `out` objects now carry the
  two keys after `wardrobeTakeOff` (struct field order in `pseudo_tool.rs`).
- **`pseudo_tool_parsers_equivalence`** (`pseudo-tool-parsers.ts`): text-block
  rows `[[OUTFIT_HISTORY /]]`, `[[OUTFIT_HISTORY limit="5" /]]`,
  `[[ROLLBACK_OUTFIT /]]`, `[[ROLLBACK_OUTFIT index="2" /]]`, `[[UNDO_OUTFIT
  index="0" /]]`, `[[wardrobe_rollback /]]`; simple-JSON rows naming
  `rollback_outfit`, `undo_outfit`, `outfit_history`, and the canonical names.
- **`tool_build_equivalence`** (fixture `harness/oracle/fixtures/tool-build.json`):
  a dress-on row — the two keys land after `wardrobeTakeOff`, before
  `wardrobeCreate`.
- NEW tier-1 rows (inside `wardrobe_tools_equivalence`'s pure section or a small
  NEW `wardrobe_history_tools_format_equivalence` over a tsx case importing the
  REAL handler modules): `filterStaleItems` (missing; archived-not-worn;
  archived-worn-same-slot; archived worn in ANOTHER slot of a multi-slot item;
  duplicate id across slots → one skipped entry), `windowHistory` (limit ≥
  rows; current inside; current before the window; no current),
  `formatWardrobeRollbackResults` (failure; titled / untitled / `''`-titled
  skipped; empty `hair`; no `current_state`), `formatWardrobeHistoryResults`
  (empty; gap; `current_index: null` with rows; the `…`/`←`/`—` code points).
  Closest recipe: `describe_outfit_equivalence` (tsx, pure, NDJSON).

### 4.2 Tier 2/3 — recording (MOVES existing families)

- **`outfit_change_effects_tier3_equivalence`** (oracle
  `harness/oracle/cases/outfit-change-effects-tier3.ts`, fixture builder
  `fixtures/build-outfit-change-effects-fixture.ts`, spec
  `fixtures/outfit-change-effects-tier3.json`): the family that proves §1.3.
  Extend the step vocabulary with `provenance`, `historyIntent`, and
  `withMap: boolean` (a Set WITHOUT a Map is a v4-legal branch); dump the
  `chat_wardrobe_history` table per step (mask `id` / `createdAt`; order by
  `chatId, characterId, entryIndex`) beside the jobs table and the Set; dump the
  Map's contents too (sorted). Rows: inside a turn the history DEFERS and the
  LAST intent wins (wear → rollback-cursor → wear, one character); two
  characters; a Set without a Map settles at once; outside a turn the schedule
  precedes the settle (line order) and a dropped `background_jobs` table still
  settles; the flush SETTLES BEFORE it schedules; the flush default intent for a
  Set member without a Map entry; an empty Set with a non-empty Map is a no-op
  that leaves the Map intact; a cursor intent at an index that does not exist
  (the keystone's `missing-index` WARN). Widen the captured-message allowlist
  from two to THREE (`Failed to settle wardrobe history`). The `equip` step's
  `handleEquipSlot` now logs DEBUG `Resolved operator for an outfit change` and
  settles an `operator` entry — add an impersonation row and a no-user-seat row.
- **`pascal_wardrobe_effects_equivalence`** (oracle `pascal-wardrobe-effects.ts`):
  the entry's `source: 'pascal'`, `actorCharacterId` = rolling character,
  `actorToolName` = `spec.toolName`; a nobody-rolled step changes nothing; dump
  the history table.
- **`wardrobe_tools_equivalence`** (oracle `wardrobe-tools.ts`, fixture
  `fixtures/build-wardrobe-tools-fixture.ts` + `wardrobe-tools.json`): grow to
  NINE tools; the ctx carries `provenance: { source: 'character',
  actorCharacterId }` + BOTH containers; add `wardrobe_history` ops (empty;
  window; gap; operator-as-self named; a Pascal row's label — keystone's
  `changedByLabel`) and `wardrobe_rollback` ops (default step back; explicit
  forward; the four refusals; a stale archived + a stale missing item; an
  archived item still worn in its slot; refusal leaves no write and no set
  member). Dump `chat_wardrobe_history` + the Map. `wardrobe_create` `equip_now`
  for a recipient pins the GIVER as actor.
- **`wardrobe_tools_avatar_trigger_equivalence`** (`wardrobe-tools-avatar-trigger.ts`):
  re-aim the ctx only (provenance + the Map) — its comparand is the avatar job.
- **NEW `wardrobe_rollback_tier2_equivalence`**: v4's REAL `rollbackWardrobe`
  over seeded timelines (route-free), both `source: 'ui'` / `'tool'`, the pool
  passed both ways (`operator: true` vs the tool pool behind a project roster —
  a project-tier item the CHARACTER cannot reach is `missing` on the tool path
  and restored on the route path); dumps `chats.equippedOutfit`, the wear ledger
  (`wardrobe_wear_stats`, credited leaves with `wornBundles: []`), the history
  table, the jobs. Closest recipes: `wardrobe_wear_commit_tier2_equivalence`
  (route + commit ops, a lines allowlist) and `outfit_displacement_tier2_equivalence`.
- **The rollback POST** — rows in `crates/quilltap-web/tests/wardrobe_routes_equivalence.rs`
  (oracle `harness/oracle/cases/wardrobe-routes.test.ts`, which already imports
  `@/app/api/v1/chats/[id]/actions/outfit` `:318-325`): 200 with a skipped item;
  each Zod 400 (empty / missing characterId; missing / string / float / negative
  / 2^53 index — the messages MEASURED); the three refusals; an unknown chat →
  404 (through `handlePost`'s gate — drive `handlePost`, not the bare handler,
  for that row); a malformed JSON body → 500; the operator actor resolved from
  `activeTypingParticipantId` and from an impersonated seat. Plus the
  `query_param_semantics_equivalence` / dispatch-action census rows the new
  POST key needs (the `availableActions` order).
- **Arrivals:** `chat_create_capstone_equivalence` (oracle
  `chat-create-capstone.test.ts`) — `chat-start` entry 0 per seated character
  (dump the history table; mask ids/timestamps); `chat_cast_routes_equivalence`
  (oracle `chat-cast-routes.test.ts:356` add-participant) — `participant-added`
  (entry 0 for a first arrival; `max + 1` for a returning seat — a remove → re-add
  row); `chat_admin_routes_equivalence` (merge, oracle `chat-admin-routes.test.ts`)
  — `merge`; `outfit_llm_choose_tier3_equivalence` (oracle
  `chat-dialogs-llm-choose-tier3.test.ts:375-377`, `applyOutfitSelections`
  direct); `wardrobe_wear_commit_tier2_equivalence` (`apply` ops; a failed commit
  records NOTHING — the `:538` `continue`).
- **Orchestrator turn:** `orchestrator_tier3_equivalence` — a scenario that calls
  `wardrobe_wear` then `wardrobe_take_off` in one turn → ONE history entry
  (`character`) and one announcement at turn end; the current corpus has NO
  wardrobe tool call (`grep -c wardrobe_wear fixtures/orchestrator-tier3.json` =
  0), so this is a corpus ADD.
- **`tool_execution_tier2` / `tool_execution_process_tier3` / native + text tool
  loops:** they use the REAL `createToolContext` → both containers for free; no
  corpus move unless a wardrobe tool is driven.

### 4.3 IMPORT-DEAD / RE-AIM — every existing oracle case that BREAKS or DRIFTS at the pin

`tsx` does not type-check, so a now-REQUIRED `provenance` never throws at
import; the breaks are RUNTIME shape shifts. Every case below runs v4's REAL
repositories over a fixture built from generateDDL, which **does not contain
`chat_wardrobe_history`** (the repository is a plain class — no
`ensureCollection`; `initializeDatabase` runs no migrations — `lib/database/manager.ts:94`).
So at the pin every settle in these cases logs the repository's WARN `Wardrobe
history settle failed; the outfit change stands unrecorded` (error `no such
table: chat_wardrobe_history`, or a TypeError when `provenance` is undefined),
unless the fixture builder creates the table from `CHAT_WARDROBE_HISTORY_DDL`
(keystone — §5).

| oracle case | what moves at the pin | families it feeds | action |
|---|---|---|---|
| `cases/outfit-change-effects-tier3.ts:201-207` (`record` op) | **POSITIONAL BREAK:** calls `recordPendingWardrobeAnnouncement({ ctx }, step.context)` — the pin's signature is `(repos, context, sourceContext)`, so the ctx object becomes `repos`, the context STRING becomes `context` (`.pendingWardrobeAnnouncements` undefined → the immediate branch with `userId` undefined; `repos.wardrobeHistory` undefined → `Failed to settle wardrobe history` TypeError WARN) | `outfit_change_effects_tier3_equivalence` | **RE-AIM REQUIRED** — pass `repos` first; add `provenance` to every `notify` / `record` ctx (`:189-207`); widen the captured-message allowlist to three |
| same case, `notify` / `flush` / `equip` ops (`:189-199`, `:230-240`, `:246-256`) | no `provenance` → `{ kind: 'append', provenance: undefined }` → the repo's TypeError WARN; a set without a Map → settles at once (not the production shape); `equip` reads the chat + logs `Resolved operator …` | same | re-aim (provenance, `newWardrobeTurnContainers()`); the table into the fixture |
| `cases/pascal-wardrobe-effects.ts:163-170` | passes a Set WITHOUT a Map → every in-turn step now settles AT ONCE (the "predates the factory" branch), unlike production | `pascal_wardrobe_effects_equivalence` | re-aim: `...newWardrobeTurnContainers()` when `withSet`; table in fixture |
| `cases/wardrobe-tools.ts:176-184` | ctx `{ userId, chatId, characterId, pendingWardrobeAnnouncements }` — no `provenance`, no Map → immediate settle with an undefined provenance → repo TypeError WARN; this case captures EVERY v4 line (`:120-125`) | `wardrobe_tools_equivalence` | re-aim (provenance + both containers); table in fixture |
| `cases/wardrobe-tools-avatar-trigger.ts:112-117` | same ctx shape | `wardrobe_tools_avatar_trigger_equivalence` | re-aim |
| `cases/wardrobe-wear-commit-tier2.ts:141-175` (`route` + `apply` ops) | `handleEquipSlot` adds a chat read + DEBUG + an operator settle; `applyOutfitSelections` settles; its lines are an ALLOWLIST (`:67-84`) so the new lines are filtered, but a DB dump of all tables would gain the history table | `wardrobe_wear_commit_tier2_equivalence` | table in fixture; decide whether to dump it (recommended: yes) |
| `cases/wardrobe-routes.test.ts:318-330` (`equip`) | as above | `quilltap-web` `wardrobe_routes_equivalence` | table in fixture; grows the rollback rows (§4.2) |
| `cases/chat-dialogs-llm-choose-tier3.test.ts:375-377` | `applyOutfitSelections` direct → settle | `outfit_llm_choose_tier3_equivalence` | table in fixture (committed `crates/quilltap-web/tests/fixtures/chat-dialogs-main.db`) |
| `cases/chat-create-capstone.test.ts` | the POST route → `applyOutfitSelections` → `chat-start` settles | `chat_create_capstone_equivalence` | table in fixture; mask the history rows |
| `cases/chat-cast-routes.test.ts:356` (add-participant) | `participant-added` settles | `chat_cast_routes_equivalence` | table in fixture |
| `cases/chat-admin-routes.test.ts` (merge `:381`; run-tool) | `merge` settles; the run-tool action's Pascal effects settle `pascal` at once | `chat_admin_routes_equivalence` | table in fixture |
| `cases/native-tool-loop-tier3.test.ts:296`, `text-tool-loop-tier3.test.ts:308`, `tool-execution-process-tier3.test.ts:105` | REAL `createToolContext` → now both containers; no wardrobe tool driven | native/text loop, process families | none (benign) |
| `cases/brahma-console-tier3.test.ts:427`, `cases/search-tools-search.test.ts:323` | hand-made `pendingWardrobeAnnouncements: new Set<string>()` (no Map) — no wardrobe tool runs | `brahma_console_tier3_equivalence`, search family | none required; optionally re-aim to the factory (mirrors v4's fence test) |
| `cases/orchestrator-tier3.test.ts`, `regenerate-swipe-tier3`, `primary-stream-tier3`, `help-chat-orchestrator-tier3` | the flush runs with real repos but the Set is empty (no wardrobe calls in the corpora) | orchestrator families | none (until §4.2's corpus add) |
| `cases/pascal-side-effects.test.ts:50-56` | mocks `getRepositories` AND `applyWardrobeEffects`; asserts the args handed to the applier (`:212`) — `turnContainersOf` forwards only present containers, so a Set-only case forwards a Set-only arg, as before | `pascal_side_effects_equivalence` | none; optionally add a Map row |
| `cases/pascal-run-custom-handler.test.ts:316-346` | the handler's ctx carries no containers | `pascal_run_custom_handler_equivalence` | none |

No module the cases import was DELETED in the span (`git diff --stat 25b8fab6a
e8e17b2d9 -- lib` for this slice is additions + signature changes only); the one
signature that SHIFTED positionally is `recordPendingWardrobeAnnouncement`. A
REAIM stage-0 lane is not needed for imports; the re-aims above ride the
families' own lanes, but **the fixture-builder table creation is shared** by
every row of the table and must land FIRST (keystone or a stage-0 fixture lane —
§8 Q1).

### 4.4 Lines to capture-pin (per op)

`Failed to settle wardrobe history` (a writer-gone arm — v5-only reachability,
pin the v5 side alone with a stated reason); `Resolved operator for an outfit
change`; `Rollback left a stale item off`; `Rolled a wardrobe back` (ORDER: after
the announcement schedule — §6 T8); the four handler lines; `Listed wardrobe
history`; the two `[Chats v1]` INFOs and the ERROR. Use `global_capture` for
anything logged on the writer thread (memory note *Capture rig vs the writer
thread*).

---

## 5. Fixtures

- **The table in every v4-side fixture that reaches a settle.** Builders:
  `fixtures/build-outfit-change-effects-fixture.ts`,
  `build-pascal-wardrobe-effects-fixture.ts`, `build-wardrobe-tools-fixture.ts`
  (shared by `wardrobe-tools`, `wardrobe-tools-avatar-trigger`,
  `wardrobe-wear-commit-tier2` — memory note *Shared fixture rebuilds*), the
  chat-create / chat-cast / chat-admin / chat-dialogs builders (the committed
  `crates/quilltap-web/tests/fixtures/chat-dialogs-*.db`), the wardrobe-routes
  fixture. Create it from v4's `CHAT_WARDROBE_HISTORY_DDL`
  (`lib/database/backends/sqlite/chat-wardrobe-history-ddl.ts`, the single
  source — the fence test forbids spelling it elsewhere IN v4, not in the
  harness) or by running v4's real migrations in the builder (memory note
  *Rebuilt fixture pair vintage* — `runV4Migrations`). The v5 side must see the
  SAME table (v5 boot-creates it through the keystone's ensure — the 2026-10-19
  ruling — but harness fixtures opened without a boot must carry it). Decide
  ONCE (keystone) and re-bake the pairs together.
- **Seeded timelines for rollback / history:** plant through v4's REAL
  `repos.wardrobeHistory.settle` (appends + a cursor move) or `upsertRows`
  (restore-only in v4's fence, fine in a builder); include: a 0..4 timeline with
  the cursor at 3; a cursor at 0; a timeline whose entry 2 names an item later
  ARCHIVED and an item later DELETED; a multi-slot archived item worn in one of
  its slots; a pre-history seat (no rows) for the first-touch seed path (the
  keystone's `seeded` arm, reached from a rollback's `no_history` refusal NOT —
  `findEntries` empty refuses before any settle).
- **Operator provenance:** a chat with `activeTypingParticipantId` on a
  user-controlled seat; one with an impersonated LLM seat
  (`impersonatingParticipantIds`); one with no user seat; one with a user seat
  whose `characterId` is `''`.
- Seed UUIDs everywhere (memory note *Memory raw reads drop non-UUID fixtures* —
  `ChatWardrobeHistoryRowSchema.chatId` is `UUIDSchema`; a `'chat-a'` id fails the
  keystone's row parse).

---

## 6. Traps and risks

- **T1 — the two orders.** Outside a turn: announce, THEN settle. At the turn-end
  drain: settle, THEN announce. Both pinned by v4 tests; a shared helper that
  picks one order for both is wrong.
- **T2 — last intent wins, by REPLACEMENT.** A wear after a rollback in the same
  turn replaces the cursor intent with an append; the append then compares the
  live hash against the PRE-rollback current entry (the cursor never moved).
  Faithful; do not "fix".
- **T3 — v5 sorts the drained Set** (`outfit_change_effects.rs:192`). With
  settles interleaved, the per-character settle order (and the `chats` realtime
  publishes) follow the sort, not insertion. No row or job differs; capture
  comparisons must sort per character (the family already does for jobs).
- **T4 — `settle` never fails in v4.** v5's after-commit settle is an async
  `Db` call that CAN return `Err`; map it to v4's WARN `Failed to settle wardrobe
  history` (the guard's catch), never propagate. Inside a write closure
  (arrivals), a settle error must not abort the commit's transaction — v4's
  `settle` runs its own transaction after the commit; in v5 decide whether the
  arrival settle runs in the SAME `Db::write` (then wrap it so its failure is a
  WARN, not a rollback of the outfit) — the keystone's `settle` contract.
- **T5 — the equip chat read is AFTER the write.** A failure there is a 500
  although the outfit WAS written and NOTHING was announced or recorded (v4's
  catch skips the notify). v5's `chat_equip` tail must reproduce the 500 + ERROR
  without undoing the write.
- **T6 — `actorCharacterId` truthiness.** Executor: `characterId ? {
  actorCharacterId } : {}` — `""` / `None` → the key absent. Operator: `seat?.characterId
  ?? undefined` then a truthy check for the provenance, but the DEBUG field logs
  `''` verbatim. Serde: `Option<String>` with `skip_serializing_if = "Option::is_none"`,
  and filter `""` to `None` at both sites.
- **T7 — a rollback with stale items moves the cursor to an entry whose hash ≠
  the live outfit.** The next change appends relative to THAT entry; a re-wear of
  the target entry's exact slots would compare against the target's hash.
  Faithful.
- **T8 — log order across v5's two halves.** v4's rollback logs `Rolled a
  wardrobe back` AFTER `notifyWardrobeChanged`; v5's table splits the handler
  (inside the write) from the effects (after commit). The INFO `Rollback left a
  stale item off` lines precede the commit (in-write half); the DEBUG must move
  to `finish_wardrobe_tool` (and to the route's post-notify tail). Similarly the
  tool's `current_state` is re-read AFTER the notify in v4 — reading it inside
  the write after the commit is equivalent (the notify writes no slots).
- **T9 — archive's notify usually settles `unchanged`.** Archiving never
  undresses (bug 191), so the live hash equals the current entry's → no row.
  BUT on a pre-history seat (no rows) with a non-empty outfit the keystone's
  first-touch seed writes an empty `0` + an entry `1` attributed to `character`
  — a row appears where nothing changed. Faithful; pin it.
- **T10 — `wardrobe_snapshot` flattens `None` → `""`** (`executor.rs:1877`).
  Derive the provenance's actor from `ctx.character_id` BEFORE that, or filter
  `""`.
- **T11 — `Now wearing:` vs `Summary:`.** The rollback formatter's last line is
  `Now wearing: …`; wear/take-off say `Summary: …`. Do not reuse v5's
  wear/take-off formatter tail.
- **T12 — `Wearing now: null`.** JS template-interpolates `null`; Rust's
  `Option` must render `null`, not `None` / empty.
- **T13 — tool input validation.** `safeParse(input ?? {})`: `null` input is
  valid (no index); `{ index: null }` INVALID (`.optional()` refuses null);
  `{ index: "2" }` invalid on native / simple-JSON paths (only the text-block
  converter numbers it); `.int()` in Zod 4 is SAFE-integer (2^53 refused,
  `maximum: 9007199254740991` in the schema); extra keys stripped; an array input
  invalid. `limit: 0` invalid (`exclusiveMinimum: 0`), `limit: 100` valid, `101`
  invalid.
- **T14 — the rollback tool's pool is the CHARACTER's** (`loadToolPool`, behind
  the project roster); the route's is the OPERATOR's. The same timeline rolled
  back by each can skip DIFFERENT items as `missing`. And `pool.get` includes
  archived items while `find_in_pool`-style helpers may not — use v5's
  `WearablePool::get` (`wearable_pool.rs:138`), and JS truthiness of
  `archivedAt` (`null` / `''` → not archived).
- **T15 — Zod 4.6.7 messages for the POST body.** Only `characterId is required`
  is authored; every other 400 text is Zod's own — measure through the oracle
  (`chat_outfits.rs:283` `received()` exists for the `expected X, received Y`
  family; the int / too-small texts need their own rows). A malformed JSON body
  is a 500, not a 400.
- **T16 — the POST key order in `availableActions`.** `wardrobe-rollback` sits
  after `equip`; a census test of `CHAT_POST_ACTIONS` will catch a misplaced key.
- **T17 — the history tool inside a turn reads a stale record** (the turn's
  intents settle at turn end) — v4's tool description says so; v5 must not
  "help" by flushing early. Two rollbacks in one turn both compute from the
  unmoved cursor (`already_current` / default target from the OLD current).
- **T18 — `formatWardrobeRollbackResults` name rule is TRUTHY** (`item.title ?
  the … : item …`); the SPA's `ToolMessage` rule is NULLISH (`title ?? item_id ??
  'An item'`). An archived item titled `''` renders `item <id>` in the model
  text and `` (empty) `` in the SPA line.
- **T19 — the first `exclusiveMinimum` in the catalog.** The canonical / serde
  round trip must keep `0` as integer `0` (not `0.0`) — `tool_definitions_equivalence`'s
  canonical spot-check covers it once the row exists.
- **T20 — `WARDROBE_TOOL_NAMES` is a fixed-size array** (`[&str; 7]`); several
  tests iterate it and assert `source_context(name) == "{name-with-dashes}-handler"`
  — both new tools follow that rule (`wardrobe-rollback-handler`,
  `wardrobe-history-handler`), so the test generalizes.
- **T21 — Brahma never flushes.** Minting containers there is cosmetic (wardrobe
  tools are off); do not add a flush v4 lacks.
- **T22 — the struct-literal union with the Pascal lane** (§2.8) — the only real
  merge hazard in the slice. Additive sibling fields, never renames, never
  reordering.
- Memory notes that apply: *Unify review finding shapes* (false "pinned" claims
  — every new line must be capture-pinned by a red-first test; union erasure of
  a sibling's record — the orchestrator's containers must not be minted twice),
  *Capture rig vs the writer thread*, *Shared fixture rebuilds + jest.setup
  mocks*, *v4 oracle job host + log capture* (`enqueueJob` spawns a runner —
  hold it off as the existing cases do), *Throwing plants must fail both reads*,
  *serde_json Map == ignores key order* (byte-compare the tool rows),
  *Rebuilt fixture pair vintage*, *Lane gate hygiene*.

---

## 7. v4 tests to mirror

| v4 test (pin) | what it pins | v5 home |
|---|---|---|
| `lib/wardrobe/__tests__/rollback.test.ts` (174) — 9 cases: steps back one by default, commits once with no bundles, hands a cursor intent; `no_history`; `at_first`; `out_of_range` naming the range; `already_current`; rolls forward; skips archived + missing but restores the rest; keeps an archived item worn in the same slot; `filterStaleItems` drops an archived item worn in a different slot | the whole of §1.7 | NEW `tools/wardrobe_rollback.rs` / the rollback module's `#[cfg(test)]` + `wardrobe_rollback_tier2_equivalence` |
| `__tests__/unit/lib/tools/handlers/wardrobe-history-rollback-handlers.test.ts` (202) — rollback: restored outfit as a tool-sourced change with the turn context (`source: 'tool'`, the ctx passed through), `Returned to outfit 2 (from 3).`, `the Greatcoat could not be put back on: it has been archived.`; a refusal → `{ success: false, error }`; a negative index touches nothing. History: ascending with labels and stale tags; own change "you"; current kept outside the window; `windowHistory` adds nothing when inside; nothing recorded; `limit` over 100 refused | §1.9 | the two tool modules' unit tests + `wardrobe_tools_equivalence` rows |
| `__tests__/unit/lib/tools/handlers/wardrobe-tool-table.test.ts` — "covers exactly the nine"; `it.each` forwards BOTH containers (`pendingWardrobeHistory` identity asserted) and the provenance | §1.10 | `wardrobe_tool_table.rs` tests `:428-506` |
| `lib/tools/__tests__/tool-definitions-snapshot.test.ts` + `.snap` +30 | the two parameter schemas | `tool_definitions_equivalence` (bytes, not the sorted snapshot) |
| `__tests__/unit/lib/wardrobe/outfit-change-effects.test.ts` (177 changed) — the factory makes both; record: defers both in a turn; announces + settles without; cursor intent; still announces when settle throws; settles at once with a Set but no Map; coalesces / last wins; separate characters; notify: refresh + join / announce + settle; flush: settles each intent, one announcement each, clears both; settles BEFORE announcing; no-op without containers; idempotent | §1.3 | `outfit_change_effects.rs` unit tests + the tier-3 family |
| `__tests__/unit/lib/wardrobe/outfit-history-fence.test.ts` (fence 4: "no turn creates the announcement set by hand") | the factory is the only minter | a v5 census guard: `grep` for `HashSet::new()` beside `pending_wardrobe_announcements` outside the factory (today: `tool_execution.rs:337`, `orchestrator.rs:3375`, `brahma_console/mod.rs:295`, `brahma_console/orchestrator.rs:717`, `scenario_builder/mod.rs:214`) |
| `__tests__/unit/lib/wardrobe/apply-outfit-selections.ledger.test.ts` (+26): "records each arrival in the outfit history with the same source, after the commit"; "records nothing when the commit fails" | §1.6 | `outfit_selections.rs` tests; `wardrobe_wear_commit_tier2` |
| `__tests__/unit/app/api/v1/chats/[id]/actions/outfit.test.ts` (+8 / +26): "records the operator as the character the user is speaking as" | §1.4 #1 | `chat_outfits.rs` tests; the routes family |
| `__tests__/unit/app/api/v1/chats/[id]/actions/wardrobe-history.test.ts` POST block: rolls back as an operator change (one commit, a cursor move, skipped reported); a refusal → 400, nothing changed; requires an index | §1.14 | the web routes family |
| `__tests__/unit/lib/pascal/wardrobe-effects.test.ts` (±16): the notify's provenance carries `source: 'pascal'`, the rolling character and `toolName` | §1.4 #7 | `pascal_wardrobe_effects_equivalence` |
| `__tests__/unit/lib/services/chat-message/tool-execution.service.test.ts` (+2): `createToolContext` mints both | §1.12 | `tool_execution.rs` test |

---

## 8. Open questions / ledger disagreements

- **Q1 (blocking order) — who puts the table into the harness fixtures?** Every
  recording family needs `chat_wardrobe_history` on both sides; the 2026-10-19
  ruling boot-creates it in v5 through a ported ensure, but harness fixtures are
  opened without a boot, and v4's fixtures (built from generateDDL) lack it. The
  keystone should own ONE builder helper (v4 side: `CHAT_WARDROBE_HISTORY_DDL`;
  v5 side: the ensure) and re-bake every pair in §4.3 BEFORE this lane's family
  rows go red-first. Otherwise every re-aimed case pins the repo's `no such
  table` WARN.
- **Q2 — where does the rollback's commit + cursor intent cross v5's
  write/after-commit seam?** Recommended: the in-write half does
  `findEntries` → refusals → `filterStaleItems` → `commit_equipped_outfit` (one
  `Db::write` with both connections, like the other wardrobe tools) and returns a
  stage carrying `(character_id, Cursor { index })`; the after-commit half runs
  `notify_wardrobe_changed` with `history_intent` and THEN logs `Rolled a
  wardrobe back`. The route reuses the same two halves.
- **Q3 — `WardrobeToolStage.changed`'s shape** (per-change intent vs one sibling
  `history_intent`). Either is faithful; the per-change tuple is more general
  (only rollback uses a non-default intent, at most one change per call).
- **Q4 — ToolMessage ownership.** `components/chat/ToolMessage.tsx` is in
  `ebb480edd`; the SPA twin `apps/web/src/app/chat/tool-message.ts` is small
  (two `TOOL_INFO` rows, one Set member, one summary arm). Either this order
  carries it (with a `tool-message.spec.ts` row) or the SPA lane takes it with
  the History tab — decide at planning; it must not land twice.
- **Q5 — v5's Scenario Builder mints an announcement set v4 does not**
  (`services/scenario_builder/mod.rs:214` vs v4 `scenario-builder.service.ts:231-236`,
  no `pendingWardrobeAnnouncements`). Harmless (wardrobe tools off), but a v5
  fence twin of v4's "made only by the factory" would flag it. Recommend
  converting it to v4's shape (`None`) in this lane, recorded as a convergence.
- **Q6 — the arrival settle inside the same `Db::write` as the commit** (T4):
  v4's commit and settle are two transactions; a settle failure there cannot
  roll the outfit back. The keystone's in-write `settle` must therefore be
  SAVEPOINT-wrapped or run after the closure — a contract decision for the
  keystone survey.
- **Ledger disagreements (measured):**
  - The task brief cites ledger lines 327 / 328 / 329 / 332 for `05005c562` /
    `ebb480edd` / `8d8618819` / `4ae20977f`; at `2e89d291a` the rows are at
    **329 / 330 / 331 / 334** (327 = `fcab3f112`, 328 = `83387d0e5`, 332 =
    `7953da3a0`).
  - `05005c562`'s row names **`chats_outfits_tier2_equivalence`** as a moving
    family — it is NOT: that family drives the chats REPOSITORY's
    `get/setEquippedOutfit` / `removeEquippedItemFromAllChats`
    (`crates/quilltap-harness/tests/chats_outfits_tier2_equivalence.rs:1-27`),
    never `handleEquipSlot`. The equip route is driven by
    `outfit_change_effects_tier3_equivalence` (the `equip` step),
    `wardrobe_wear_commit_tier2_equivalence` (`route` ops) and
    `quilltap-web`'s `wardrobe_routes_equivalence`. The same misattribution
    appears in `8d8618819`'s row for the POST.
  - `05005c562`'s row omits that `recordPendingWardrobeAnnouncement` gained a
    LEADING `repos` argument (the positional break in
    `outfit-change-effects-tier3.ts`, §4.3), and that the two orders differ (T1).
  - `05005c562`'s row lists `services/chat_create.rs` for the arrivals; the
    arrival SETTLE lands in `services/outfit_selections.rs` `commit_selection`
    (chat-start + merge) and `services/chat_participants.rs:1517-1545`
    (participant-added — v5 inlines that commit), not in `chat_create.rs`.
  - `ebb480edd`'s row says "seven → nine" of the executor list + tool table; v5
    has TWO executor lists (`BUILT_IN_TOOLS` and `PORTED_TOOLS`) plus the table's
    fixed-size `WARDROBE_TOOL_NAMES`, and `tools_inventory.rs`'s `len() == 43`
    and `definitions/mod.rs`'s `len() == 61` asserts.
  - `8d8618819`'s row lists the POST's refusals generically; `at_first` is
    unreachable on the route (index required), and a malformed body is a 500,
    not a Zod 400; the 404 comes first from `handlePost`'s gate.
  - `4ae20977f`'s row names `participant_filters.rs`
    `find_active_user_participant` as the resolver twin — correct; the
    impersonation overlay read (`chats_impersonation::read_impersonating`) and
    the participants projection are what `user_identity_resolver.rs:140-152`
    already composes — reuse that composition rather than a second one.
