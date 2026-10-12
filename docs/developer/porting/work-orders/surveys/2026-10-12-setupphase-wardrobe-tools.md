# Survey — the seven model-visible wardrobe TOOLS across `7a8d6dacc` → `9ecebf986` → `4a38222e6`

Read-only planning survey, 2026-10-12. v4 = `~/source/quilltap-server` read
ONLY through `git show 4a38222e6:<path>` (the PIN, `4.10.0-dev.148`) and
`git diff 3e6e2f590 4a38222e6 -- <paths>` (the span); v5 = `main` at
`f84d34150`. **Every line number below is AT THE PIN `4a38222e6`** unless
marked `(pre)` (= `3e6e2f590`, the docs-only commit right after the current
baseline `01a83539d`, so `(pre)` IS the shape v5 ported). Nothing here comes
from commit messages or docs prose — the ledger's §5.3 rule; the drift
ledger's §3 rows for the three shas (`drift-ledger.md:460`, `:461`, `:466`)
were read first and are not restated.

Scope: `lib/tools/wardrobe-{list,read,create,update,archive,wear,take-off}-tool.ts`,
`lib/tools/handlers/wardrobe-*-handler.ts`, `wardrobe-tool-table.ts`,
`wardrobe-handler-shared.ts`, the executor's dispatch, `lib/wardrobe/
outfit-change-effects.ts`, `lib/wardrobe/tool-image-generation.ts`, the
OpenAI-parameters snapshot, and the `lib/wardrobe/` seams the handlers call
(recorded as NEEDS from the keystone, §6 — not as this lane's port). **Out of
scope** (other surveys / lanes): the route factory + every tier's item
routes, `chats/[id]/actions/outfit.ts` (the operator's `?action=equip`),
transfers, the item-image JOB's Concierge half (bug 189), the vault parser /
projection (bug 187, the recursive predicate, the `folder` derivation), the
carriers, the SPA, `help/` + `docs/v4/`.

---

## 0. The span for the tools, in one paragraph

`7a8d6dacc` made `wardrobe_archive` idempotent (bug 188: `already_archived`),
split the archived-wear refusal into client-safe `wearable.ts` (bug 191), and
touched the create handler only in prose. `9ecebf986` rewrote every handler
onto ONE per-request `WearablePool` (`lib/wardrobe/pool.ts`), ONE
`WardrobeLocation` writer seam (`location.ts` + `item-mutations.ts`), ONE
displacement gesture (`applyDisplacement`) behind `wear-ops.ts`, ONE
`WARDROBE_TOOL_TABLE` in the executor with ONE `WardrobeToolContext` (bug
193: `wardrobe_create` + `equip_now` now joins the turn's announcement set),
dropped `include_presets` from `wardrobe_list`'s schema (the snapshot moves),
changed the create's two not-found sentences and added two refusals, and made
composite `types` widen-never-narrow (bug 195). `4a38222e6` added the
READ-ONLY `folder` key to `wardrobe_list` / `wardrobe_read` (` [folder: …]`
after the title in both renderings, `folder: null` in the read failure) and
one sentence to each description — NO input-schema change. The deleted
repository surface the (pre) handlers leaned on — `resolveWardrobeItemAcrossTiers`,
`findByIdForCharacter`, `findByIdsForCharacter`, `findWearablePoolForCharacter`,
`findArchetypes`, `isOwnWardrobeItem`, `equipItem` / `replaceItem` / `addToSlot` /
`removeFromSlot`, `shared-tiers.ts` — is exactly what v5's
`tools/wardrobe_shared.rs` + `db/wardrobe_read.rs` + `wardrobe_tiers.rs` are.

---

## 1. v4 at the pin — the seven definitions (`lib/tools/wardrobe-*-tool.ts`)

### 1.1 `wardrobe-list-tool.ts`

- `:15-36` `wardrobeListToolInputSchema = z.object({ type_filter, appropriateness_filter, include_equipped })` — **`include_presets` is GONE** (the span diff removes `:36-41 (pre)`:
  `include_presets: z.boolean().describe('Whether to include saved outfit presets in the response. Defaults to true.').optional()`).
  `z.object` is non-strict, so an `include_presets` key of ANY type is now
  STRIPPED by the parse — it no longer fails validation (v5 `wardrobe_list.rs:100-104`
  still refuses a non-bool `include_presets`; see §7.1).
- `:46-92` `WardrobeListItemResult` — the interface gains `folder: string | null`
  (`:60-65`, after `is_own`, before `is_equipped`) with the doc "The operator's
  folder for the item under its wardrobe (e.g. `Formal/Evening`), null at the
  root. Filing only — it says nothing about when to wear it."
- `:120-143` `wardrobeListToolDefinition` — the description gains ONE sentence
  (`:138-140`), inserted before `'Archived items are excluded from results.'`:
  > `An item's folder is how the operator has filed it for their own browsing; it carries no meaning for when or how to wear it. `
  (The description's first sentence still reads "from their own wardrobe plus
  shared items in the project and Quilltap General" — no "groups".)

### 1.2 `wardrobe-read-tool.ts`

- `:18-31` schema unchanged (`item_id` / `item_title` + the `.refine` `'item_id or item_title is required'`).
- `:37-77` `WardrobeReadToolOutput` gains `folder: string | null` (`:65-69`, after `is_own`, before `is_equipped`), same doc.
- `:109-125` description — `:117-118` the clause `'whether you own it, the slots it is '` becomes
  > `'whether you own it, the folder the operator ' + 'filed it in (their browsing aid, not a hint about when to wear it), the slots it is '`
  (the rest byte-identical; still ends "Items from your own wardrobe, the project, and Quilltap General all resolve.").

### 1.3 `wardrobe-archive-tool.ts`

- `:37-50` `WardrobeArchiveToolOutput` gains `already_archived?: boolean` (`:44-48`) — "True when the item was already archived — nothing was written and its original `archivedAt` stands." Schema (`:19-32`) and description (`:55-67`) unchanged.

### 1.4 `wardrobe-create-tool.ts`, `wardrobe-update-tool.ts`, `wardrobe-wear-tool.ts`, `wardrobe-take-off-tool.ts`

**Byte-identical across the span** (the `--stat` lists none of them). Their
descriptions still say "the character's own wardrobe, the project, and
Quilltap General" (create `:184`, wear `:133-134`, take-off `:29-30`) — the
"their groups" wording appears ONLY in the create handler's two not-found
sentences (§5.3), never in a description.

### 1.5 The OpenAI-parameters snapshot

`lib/tools/__tests__/tool-definitions-snapshot.test.ts` (`:1-80` imports every
`*ToolDefinition`; `ALL_TOOLS` at `:78`) with
`lib/tools/__tests__/__snapshots__/tool-definitions-snapshot.test.ts.snap`.
**The span's ONLY snapshot hunk** (`exports[\`tool definitions: derived OpenAI parameters wardrobeList: parameters snapshot\`]`, snap `:1921-1927 (pre)`):

```
-    "include_presets": {
-      "description": "Whether to include saved outfit presets in the response. Defaults to true.",
-      "type": "boolean",
-    },
```

Nothing else in the snapshot moves — `4a38222e6` changes no input schema
(descriptions are not in the `parameters` snapshot). The v5 twin is the
byte-pinned catalog `tools/definitions/data.rs` (§8.1) + `tool_definitions_equivalence` (§9.4).

---

## 2. The dispatch — `wardrobe-tool-table.ts` + `WardrobeToolContext` + the executor

### 2.1 `lib/tools/handlers/wardrobe-handler-shared.ts:16-22` (NEW)

```ts
export interface WardrobeToolContext {
  userId: string;
  chatId: string;
  characterId: string;
  /** Per-turn announcement queue the orchestrator threads through. */
  pendingWardrobeAnnouncements?: Set<string>;
}
```
Every handler file now `export type Wardrobe<X>ToolContext = WardrobeToolContext`
(list `:25`, read `:38`, create `:43`, update `:49`, archive `:35`, wear `:40`,
take-off `:40`) — the seven per-handler context interfaces are gone, and
`list` / `read` / `create` / `update` now RECEIVE the set (only archive / wear /
take-off did `(pre)`).

### 2.2 `lib/tools/handlers/wardrobe-tool-table.ts` (NEW, 109 lines)

`:20-26` `WardrobeToolRun { success; result: Record<string, unknown> | null; error? }`,
`WardrobeToolRunner = (input: unknown, context: WardrobeToolContext) => Promise<WardrobeToolRun>`.
`:28-109` `WARDROBE_TOOL_TABLE: Record<string, WardrobeToolRunner>` — the
payload projections are the executor's `(pre)` arms VERBATIM:

| tool | `result` on success (key order) | on failure |
|---|---|---|
| `wardrobe_list` `:29-38` | `{ formattedText, items, total_count }` | `null`, `error: r.error` |
| `wardrobe_read` `:39-46` | `{ formattedText, ...r }` (the whole output spread AFTER `formattedText`; `success: true` rides along) | `null` |
| `wardrobe_create` `:47-63` | `{ formattedText, item_id, title, equipped, recipient_name, current_state }` (`undefined` values → keys absent in JSON) | `null` |
| `wardrobe_update` `:64-71` | `{ formattedText, ...r }` | `null` |
| `wardrobe_archive` `:72-81` | `{ formattedText, item_id, title, action }` — **`already_archived` is NOT projected**; it reaches the model through `formattedText` only | `null` |
| `wardrobe_wear` `:83-95` | ALWAYS `{ formattedText, operations, current_state, coverage_summary }` ("a partial run still changed slots"), `error` set iff `!success` | same object |
| `wardrobe_take_off` `:96-108` | same shape as wear | same |

### 2.3 `lib/chat/tool-executor.ts`

- `:79` `import { WARDROBE_TOOL_TABLE } from '@/lib/tools/handlers/wardrobe-tool-table'` replaces seven handler imports.
- `:180-187` `ToolExecutionContext.pendingWardrobeAnnouncements?: Set<string>` (doc unchanged).
- `:253-260` the seven names in the built-in list, unchanged.
- `:718-729` the ONE arm (replacing `(pre) :752-917`, seven arms):
  ```ts
  const wardrobeTool = WARDROBE_TOOL_TABLE[toolCall.name];
  if (wardrobeTool) {
    const { success, result, error } = await wardrobeTool(toolCall.arguments, {
      userId, chatId, characterId: characterId || '',
      pendingWardrobeAnnouncements: context.pendingWardrobeAnnouncements,
    });
    return { toolName: toolCall.name, success, result, error };
  }
  ```
  `characterId || ''` unchanged. The `result`/`error` bytes per tool are
  unchanged from `(pre)` — only `wardrobe_create` / `list` / `read` / `update`
  now see the set (bug 193 is the create arm; list/read/update never fire it).
- The orchestrator's drain import moves: `lib/services/chat-message/orchestrator.service.ts:98`
  `import { flushPendingWardrobeAnnouncements } from '@/lib/wardrobe/outfit-change-effects'`
  (was `…/wardrobe-handler-shared`), call at `:1671` unchanged.

---

## 3. `wardrobe-handler-shared.ts` at the pin (261 lines; was 447)

**Deleted** (the span diff): `resolveWardrobeItemAcrossTiers`, `isOwnWardrobeItem`,
`WardrobeReposForSummary`, `emptyEquippedState`, `scheduleWardrobeAnnouncement`,
`notifyWardrobeChanged`, `recordPendingWardrobeAnnouncement`,
`flushPendingWardrobeAnnouncements` (the last four MOVED to
`outfit-change-effects.ts`, §4), the local `wardrobeItemNotFoundMessage`
(now `:9` `export { wardrobeItemNotFoundMessage } from '@/lib/wardrobe/wear-ops'`),
the `SharedWardrobeTiers` / `ToolExecutionContext` imports. `NO_ITEM_SENTINELS`
(`:31`) is no longer exported (same set `['none','null','']`;
`normalizeNoItemSentinel` `:33-36` unchanged).

**Kept byte-identical:** `findEquippedSlots` `:55-67`; `WardrobeEffect` `:79`;
`describeWardrobeEffect` `:86-106` (the four sentences: `Layered ${title} into ${slotList}. The item's replace flag is off, so whatever was already in ${those} was kept.` /
`Replaced ${slotList} with ${title} — anything previously in ${those} was cleared.` /
`Took ${title} off ${slotList}; any other layers there stayed.` else `Cleared ${slotList}.` /
`Cleared ${slotList} entirely.`); `formatEquippedSlotLines` `:125-135` (hair
omitted when empty); `formatWardrobeMutationResults` `:142-163`
(`Wardrobe Error: …` only when `!success && operations.length === 0`; then
per-op `Failed: ${op.error}` or `effect_summary`, blank, `Current outfit:`,
slot lines, blank, `Summary: ${coverage_summary}`); `buildWardrobeMutationFailure`
`:169-183` (now `makeEmptyEquippedSlots()` directly); `loadCurrentWardrobeState`
`:229-236`; `sharedWardrobeItemReadOnlyMessage(title, verb)` `:253-261` —
> `"${title}" is a shared wardrobe item — you can wear it but not edit or retire it. Only items in your own wardrobe can be ${verb}.`

**New / changed:**
- `:43-49` `loadToolPool(repos, chatId, characterId) = loadWearablePool(repos, characterId, undefined, { chatId })` — the project tier resolved from the chat behind the roster (`operator` NOT set), the group tier per character, General always. One read per tier per tool call.
- `:191-227` `finalizeWardrobeMutation(repos, context, sourceContext, { appliedCount, results, failedError, pool })` — `tiers` → `pool`; `if (appliedCount > 0) await notifyWardrobeChanged(repos, context, sourceContext)` (the whole context object, set included); then `loadCurrentWardrobeState`; then the SYNC `buildWardrobeCoverageSummaryFromState(pool, currentState)`; returns `{ success: failedError === undefined, operations, current_state, coverage_summary, ...(failedError ? { error } : {}) }`.
- `:243-245` `buildWardrobeCoverageSummaryFromState(pool, slots) = describeOutfit(resolveEquippedOutfitForCharacter(pool, slots).outfitValues)` — no I/O.

---

## 4. `lib/wardrobe/outfit-change-effects.ts` (NEW in `9ecebf986`, 125 lines)

- `:32-38` `OutfitChangeContext { userId; chatId; characterId; pendingWardrobeAnnouncements?: Set<string> }`.
- `:41-52` `refreshAvatarForOutfit(repos, context, sourceContext)` → `triggerAvatarGenerationIfEnabled(repos, { userId, chatId, characterId, callerContext: sourceContext })`. Never throws.
- `:55-77` `scheduleWardrobeAnnouncement(sourceContext, { userId, chatId, characterId })` — `enqueueWardrobeOutfitAnnouncement(userId, { chatId, characterId })`; **NEW on success** DEBUG `Wardrobe outfit announcement scheduled` `{ context: sourceContext, chatId, characterId }`; on throw WARN `Failed to schedule wardrobe outfit announcement` `{ context: sourceContext, chatId, characterId, error }` (the `(pre)` `extraLogFields` spread is gone).
- `:83-92` `recordPendingWardrobeAnnouncement(context, sourceContext)` — with a set: `add(characterId)`, return; else schedule now.
- `:98-105` `notifyWardrobeChanged(repos, context, sourceContext)` = `refreshAvatarForOutfit` THEN `recordPendingWardrobeAnnouncement` (avatar first, as `(pre)`).
- `:111-125` `flushPendingWardrobeAnnouncements(context)` — unchanged logic: `Array.from(pending)`, `pending.clear()`, one `scheduleWardrobeAnnouncement('orchestrator-turn-end', …)` per id in insertion order.

Callers at the pin: the archive / wear / take-off handlers (via `finalizeWardrobeMutation`), **the create handler (bug 193)**, `app/api/v1/chats/[id]/actions/outfit.ts:26` (`notifyWardrobeChanged`), `participants.ts:33` + `chats/route.ts:68` (`refreshAvatarForOutfit` — the routes lane).

---

## 5. The seven handlers at the pin

Common to all seven: `const repos = getRepositories()`; validation failure →
WARN `Wardrobe <x> tool validation failed` `{ context: 'wardrobe-<x>-handler', userId, [chatId,] characterId, input }`
(list omits `chatId`, as `(pre)`); the outer catch → ERROR
`Wardrobe <x> tool execution failed` `{ context, userId, chatId, characterId }` + the Error
(list: `userId, characterId, chatId` order) and `error.message` else
`Unknown error during wardrobe <x>` (list: `…wardrobe list operation`; create:
`…wardrobe item creation`; update: `…wardrobe update`; read: `…wardrobe read`;
archive: `…wardrobe archive`). The formatted failure is always
`Wardrobe Error: ${error || 'Unknown error'}`. The item-lookup sentinel
(`normalizeNoItemSentinel`) applies to `item_id` / `item_title` before lookup;
the not-found message uses the RAW input values.

### 5.1 `wardrobe-list-handler.ts` (265 lines)

- `:56-68` validation sentence unchanged: `Invalid input: type_filter must be a string array, appropriateness_filter must be a string, include_equipped must be a boolean`.
- `:76-77` `pool = await loadToolPool(…)`; `allItems = pool.wearable()` — **archived dropped PER TIER before shadowing** (an archived own copy no longer hides the shared namesake; `(pre)` `findWearablePoolForCharacter` = `mergeWearablePool(findArchetypes(false, tiers), findByCharacterId(id))` dropped archived AFTER shadowing). Order of first appearance: General → project → group → own (same as `(pre)`; see §7.9).
- `:79-82` equipped slots; `:86-100` the type / appropriateness filters unchanged.
- `:105-108` `findSummariesForWearer(filteredItems.map(i => i.id), context.characterId)` unchanged (P4.D275).
- `:116-121` **component titles from the WHOLE pool**: `pool.getMany(item.componentItemIds).map(c => c.title)` — archived or filtered-out parts still name themselves (`(pre)` built `itemsById` from the wearable `allItems`, so an archived part's title was DROPPED). `isComposite = itemIsComposite(item)` (`wardrobe.types.ts:390-392`: `(componentItemIds?.length ?? 0) > 0`).
- `:122-148` **result key order (runtime, the tier-1 JSON):** `item_id, title, description, image_prompt, image_file_id, types, appropriateness, is_own, folder, is_equipped, equipped_slot, wear_count, last_worn_at, worn_by_you, last_worn_by_you_at` then the composite spread `is_composite, component_item_ids, component_titles`. `is_own: item.characterId === context.characterId` (NOT `pool.owns` — list keeps the field compare); **`folder: item.folder || null`** (`''` root → `null`).
- `:152-155` `include_equipped === false` filter; `:157-171` INFO `Wardrobe list completed` `{ context, userId, characterId, chatId, totalItems: allItems.length, filteredCount, hasTypeFilter: !!type_filter, hasAppropriatenessFilter: !!appropriateness_filter, includeEquipped: include_equipped !== false, compositeCount, neverWornCount, neverWornByCallerCount, withPictureCount }` — unchanged keys; `totalItems` is now the pool's wearable count.
- `:196-198` `listTimes`, `:211-223` `formatWardrobeListWearNote` unchanged (P4.D275).
- `:232-265` `formatWardrobeListResults(output, nowMs)`: `:249` **`folderTag = item.folder ? \` [folder: ${item.folder}]\` : ''`**; `:261` the line is
  `\`  ${typeTags} ${item.title}${folderTag}${equippedTag}${sharedTag}${appropriatenessTag}${compositeTag}${cueTag}${description}${wearTag}${pictureTag}\``
  — the tag sits between the title and ` (EQUIPPED in …)`. Everything else unchanged (`Wardrobe (${n} item${s}):`, `Wardrobe: No items found matching the specified filters.`).

### 5.2 `wardrobe-read-handler.ts` (302 lines)

- `:44-80` `buildWardrobeReadOutput(repos, pool, chatId, item)` (was `(repos, characterId, chatId, item, tiers)`): `characterId = pool.characterId`; `componentTitles = isComposite ? pool.getMany(item.componentItemIds).map(c => c.title) : []` (archived included — `(pre)` `findByIdsForCharacter(…, tiers)` ALSO included archived, so no delta here beyond the source); equipped; `wear = buildWardrobeReadWear(repos, characterId, item.id)` (unchanged, DEBUG `Wardrobe read resolved wear history` `{ context: 'wardrobe-read-handler', characterId, itemId, wearCount, wearerCount }` `:96-102`).
  **Output key order:** `success, item_id, title, description, image_prompt, image_file_id, types, appropriateness, is_default, replace, is_composite, component_item_ids, component_titles, archived, is_own, folder, is_equipped, equipped_slots, wear`. **`is_own: pool.owns(item)`** (`characterId === pool.characterId && ownIds.has(item.id)`); **`folder: item.folder || null`**.
- `:204-226` `buildWardrobeReadFailure(error)` — same key order with `folder: null` (`:221`) and `error` last; `wear` absent.
- `:228-271` `executeWardrobeReadTool`: `:244` `Invalid input: item_id or item_title is required.`; `:249` pool; `:251-254` `findInPool(pool, { itemId, itemTitle })` (§6.2 — by id over `byId` archived INCLUDED, then by title); `:256` not found → `buildWardrobeReadFailure(wardrobeItemNotFoundMessage(item_id, item_title))`.
- `:278-302` `formatWardrobeReadResults(output, nowMs)`: `:286-287` **head line `\`${title} (${item_id})${folderTag}\``** with `folderTag = output.folder ? \` [folder: ${output.folder}]\` : ''`; the rest unchanged (`  types:`, `  appropriateness:`?, `  description:`?, `  portrait cue: … | (none — falls back to title)`, `  picture: … | (none)`, `  composite: … (replace=…)`?, `  default: yes|no | own: yes|no (shared — read-only)`, `  archived: yes (hidden from listings, cannot be worn)`?, `  equipped: …|no`, `  wear: …`?).

### 5.3 `wardrobe-create-handler.ts` (388 lines)

- `:56-87` `resolveRecipientFromChat` unchanged (participants, `status !== 'removed'`, `name.trim().toLowerCase()` equality).
- `:94-130` `resolveComponentItems(pool, componentIds, componentTitles)` — SYNC, ids first via `pool.get(id)` (shadowed `byId`, **archived included**), then titles via `pool.findByTitle(title)` (own first incl. archived, then the wearable shared set — case-insensitive, trimmed), deduped by id in resolution order. **Both sentences change** (`(pre)` ended `…wardrobe, the project, or Quilltap General`):
  > `Component item with ID "${id}" was not found in this character's wardrobe, their groups, the project, or Quilltap General`
  > `Component item titled "${title}" was not found in this character's wardrobe, their groups, the project, or Quilltap General`
  (thrown as `WardrobeCreateError(…, 'NOT_FOUND')`). Mechanically the group tier was ALREADY reachable `(pre)` through `findArchetypes(false, tiers)` with tiers keyed on the target — the behavioural delta is that a by-id ref to an ARCHIVED shared component now resolves (`(pre)` `findArchetypes(false)` excluded them).
- `:144-160` validation sentence unchanged (`Invalid input: title (string) is required. Either supply types (non-empty array of valid slot types) for a leaf item, or component_item_ids / component_titles for a composite item.`); `:185-197` recipient not found → WARN `Wardrobe create recipient not found in chat` `{ context, userId, chatId, recipientName }` + `Could not find a character named "${recipient}" in this chat` (unchanged).
- `:206` `pool = await loadWearablePool(repos, targetCharacterId, undefined, { chatId: context.chatId })` — keyed on the TARGET (gift), roster applies.
- `:210-212` **NEW refusal**: `componentItemIds.length === 0 && (types?.length ?? 0) === 0` → `WardrobeCreateError('A wardrobe item must cover at least one slot', 'VALIDATION_ERROR')` (reachable only past the schema's `.refine`, i.e. `types: []`… which `.nonempty()` refuses — so effectively unreachable from the model; port for fidelity).
- `:214-217` **NEW**: `location = await resolveWardrobeLocation('character', targetCharacterId, repos, context.userId)`; `null` → `WardrobeCreateError('That character has no wardrobe to hang this in', 'NOT_FOUND')` — null when the character is missing, **not `userId`'s** (`location.ts:237-238`), or has no `characterDocumentMountPointId` (`:239-240`). **`userId` is now load-bearing in the tool** (v5's handlers discard it: `wardrobe_create.rs:247` `let _ = user_id;`).
- `:219-232` `createItem(location, { title, description, imagePrompt: image_prompt, types: types ?? [], componentItemIds, appropriateness, isDefault: false, replace: replace ?? false }, { lookup: pool.byId })` — through `wardrobeItemFromCreateBody` (§6.8: **blank prose → `null`**, `componentItemIds` deduped, **`replace` stored for LEAVES too** — `(pre)` forced `replace: isComposite ? replace ?? false : false`), then for a composite `types = buildCompositeTypes(components, designated)` (§6.4 — union ∪ supplied, canonical order; empty-union fallback to `designated`). The `(pre)` refusal `Composite components do not cover any slots — this should not happen` is **DELETED** (unreachable both sides; v5 carries it at `wardrobe_create.rs:325`). `isComposite = itemIsComposite(newItem)`; `resolvedTypes = newItem.types`.
- `:240-271` `equip_now`: `:242` `withNew = { ...pool, byId: new Map([...pool.byId, [newItem.id, { ...newItem, origin: location.origin }]]) }` (the pool predates the item); `:243-251` `outcome = await wearItem(repos, chatId, withNew, { ...newItem, origin }, 'wear', undefined, 'tool')` → `effect = outcome.effect` (= `item.replace ? 'replaced' : 'layered'`, same value as `(pre)`); `:257-259` `currentState` through `getEquippedOutfitForCharacter` ?? `makeEmptyEquippedSlots()` (unchanged); **`:261-270` `notifyWardrobeChanged(repos, { userId, chatId, characterId: targetCharacterId, pendingWardrobeAnnouncements: context.pendingWardrobeAnnouncements }, 'wardrobe-create-handler')`** — bug 193: `(pre)` fired `triggerAvatarGenerationIfEnabled` ALONE; now the recipient ALSO joins the turn's set (or an announcement is enqueued at once without a set).
- `:274-282` `maybeQueueWardrobeToolImage(repos, { userId, chatId, characterId: targetCharacterId, itemId, requested: generate_image, defaultWhenEnabled: true, callerContext: 'wardrobe-create-handler' })` unchanged.
- `:284-298` INFO `Wardrobe create completed` `{ context, userId, chatId, characterId, targetCharacterId, recipientName, itemId, title, isComposite, componentCount, equipped, effect, imageGeneration }` unchanged.
- `:300-317` **output key order** unchanged: `success, item_id, title, equipped, [effect, effect_summary], is_composite, resolved_types, [resolved_component_item_ids], [recipient_name], [current_state], [image_generation]`; `effect_summary = describeWardrobeEffect(effect, resolvedTypes, newItem.title)`.
- `:319-335` a `WardrobeCreateError` → WARN `Wardrobe create error` `{ context, userId, chatId, characterId, code, message }` + `{ success: false, item_id: '', title: '', equipped: false, error }`.
- `:357-388` `formatWardrobeCreateResults` unchanged (`Created ${composite outfit|wardrobe item} "${title}" (${id})${ for ${recipient}}`, `- Bundles N item(s); covers …`, `- Equipped immediately[ on ${recipient}]` + `- Current outfit:\n…` | `- Not equipped (added to wardrobe[ of ${recipient}] only)`, `- Picture: …`).

### 5.4 `wardrobe-update-handler.ts` (166 lines)

- `:35-46` `patchChangesLook(item, patch)` unchanged — but it now sees the HANDLER's patch only (no recomputed `types`; §7.6).
- `:67` `Invalid input: item_id or item_title is required.`; `:85` pool; `:87-93` `findInPool`; `:95-97` **`!pool.owns(item)`** → `buildWardrobeReadFailure(sharedWardrobeItemReadOnlyMessage(item.title, 'changed'))`.
- `:99-107` the patch: only supplied fields (`title, description, imagePrompt, appropriateness, types, isDefault, replace, componentItemIds`). The `(pre)` block that recomputed `patch.types = unionTypes(comps)` when components changed without `types` is GONE.
- `:109-115` `location = await resolveWardrobeLocation('character', context.characterId, repos, context.userId)`; `updated = location ? await updateItem(location, item, patch, { lookup: pool.byId }) : null`; `!updated` → `Failed to update wardrobe item "${item.title}"` (the same sentence now also covers a null location, where `(pre)` threw the repository's `Cannot update wardrobe item: no Character Vault or Quilltap General mount is available.`).
- `updateItem` (§6.6) applies **widen-never-narrow (bug 195)**: for a composite (components non-empty after the patch) when `componentItemIds` OR `types` is in the patch, `next.types = buildCompositeTypes(components, patch.types ?? current.types)` — so (a) a components-only edit keeps every slot the composite already designated; (b) a `types`-only edit on a composite is WIDENED over the components (the `(pre)` stored `types` as given); (c) a leaf's `types` are stored as given; (d) an edit touching neither leaves `types` alone.
- `:118-127` picture claim unchanged (`defaultWhenEnabled: changesLook`); `:129-138` INFO `Wardrobe update completed` `{ context, userId, chatId, characterId, itemId, fields: Object.keys(patch), changesLook, imageGeneration }` — **`fields` no longer contains `types` for a components-only edit** (it was added to `patch` `(pre)`).
- `:140-141` `buildWardrobeReadOutput(repos, pool, chatId, updated)` + `image_generation`; `:158-166` format unchanged (`Updated "${title}" (${id}).` [+ `\n- Picture: …`]).

### 5.5 `wardrobe-archive-handler.ts` (149 lines)

- `:37-39` `buildFailureResponse` → `{ success: false, item_id: '', title: '', action: 'archived', error }`; `:57` `Invalid input: item_id or item_title is required.`.
- `:62-73` pool, `findInPool`, **`!pool.owns(item)`** → `sharedWardrobeItemReadOnlyMessage(item.title, 'archived')`.
- `:76-79` **NEW, BEFORE the equipped read**: `location = await resolveWardrobeLocation('character', context.characterId, repos, context.userId)`; `null` → `Failed to archive wardrobe item "${item.title}"`.
- `:83-84` `wasEquipped = findEquippedSlots(item.id, equipped).length > 0`.
- `:86-99` **`{ item: archived, changed } = await setItemArchived(location, item, true)`** (§6.6 — `archivedPatch(current.archivedAt, true, now)` is `null` when already archived → NO write). `!changed && archived` → DEBUG `Wardrobe item already archived; keeping its original date` `{ context: 'wardrobe-archive-handler', chatId, characterId, itemId, archivedAt: item.archivedAt }` and **`{ success: true, item_id, title, action: 'archived', already_archived: true }`** — no announcement, no INFO line; `!changed && !archived` (the item vanished under it) → `Failed to archive wardrobe item "${item.title}"`.
- `:101-112` `wasEquipped` → `notifyWardrobeChanged(repos, { userId, chatId, characterId, pendingWardrobeAnnouncements }, 'wardrobe-archive-handler')`; `:114-122` INFO `Wardrobe item archived` `{ context, userId, chatId, characterId, itemId, itemTitle, wasEquipped }`; `:124` `{ success: true, item_id, title, action: 'archived' }`.
- `:141-149` `formatWardrobeArchiveResults`: failure → `Wardrobe Error: …`; **`already_archived` → `"${title}" (${item_id}) was already archived; nothing changed.`**; else `Archived "${title}" (${item_id}). It's hidden from listings and can't be worn; a human can restore it.`

### 5.6 `wardrobe-wear-handler.ts` (149 lines)

- `:60-63` validation sentence unchanged (`Invalid input: provide a non-empty "operations" array. Each operation needs an item_id or item_title; mode=add_to_slot also needs a slot.`).
- `:66` pool; per op `:73-75` `mode = op.mode ?? 'wear'`, sentinel-normalised id/title; `:78-81` **`resolved = resolveWearable(pool, { itemId, itemTitle }, mode, op.slot)`**; `!resolved.ok` → throw `WardrobeWearError(resolved.message)` — the three refusals, words unchanged (§6.2): not found `Wardrobe item not found[ with ID "…"][ with title "…"]`, archived `Item "${title}" is archived and cannot be worn` (via `wearRefusal`, §6.3), slot `Item "${title}" (types: ${types.join(', ')}) cannot be added to the "${slot}" slot`. `:83-91` `{ effect, slotsAffected } = await wearItem(repos, chatId, pool, item, mode, op.slot, 'tool')`.
- `:93-99` op result `{ mode, effect, effect_summary: describeWardrobeEffect(effect, slotsAffected, item.title), item: { item_id, title }, slots_affected }`; `:102-112` INFO `Wardrobe item worn` `{ context: 'wardrobe-wear-handler', userId, chatId, characterId, itemId, itemTitle, mode, effect, slotsAffected }`; `:114-132` catch → `{ mode, effect: 'layered', effect_summary: '', item: null, slots_affected: [], error }`, WARN `Wardrobe wear operation failed` `{ context, userId, chatId, characterId, mode, message }`, `break`.
- `:136-141` `finalizeWardrobeMutation(repos, context, 'wardrobe-wear-handler', { appliedCount, results, failedError, pool })`.

### 5.7 `wardrobe-take-off-handler.ts` (165 lines)

- `:60-63` validation sentence unchanged (`… mode=remove needs an item_id or item_title; mode=clear_slot needs a slot.`).
- `:78-97` `clear_slot` → **`applyDisplacement(repos, chatId, characterId, { mode: 'clear_slot', slot })`** (was `removeFromSlot(…, slot)`); result `{ mode, effect: 'cleared', effect_summary: describeWardrobeEffect('cleared', [slot]), item: null, slots_affected: [slot] }`; INFO `Wardrobe slot cleared` `{ context, userId, chatId, characterId, slot }`.
- `:100-103` `remove` → `findInPool(pool, { itemId, itemTitle })` (archived items ARE found — taking an archived garment off is allowed); not found → throw `WardrobeTakeOffError(wardrobeItemNotFoundMessage(itemId, itemTitle))` (the SENTINEL-normalised values here, as `(pre)`).
- `:107-110` `slotsAffected = op.slot ? [op.slot] : item.types`; **`takeOffItem(repos, chatId, characterId, item.id, slotsAffected)`** (§6.2 — one `applyDisplacement({ mode: 'remove_from_slot', slot, itemId })` per slot).
- `:112-128` result `{ mode, effect: 'removed', effect_summary: describeWardrobeEffect('removed', slotsAffected, item.title), item, slots_affected }`; INFO `Wardrobe item taken off` `{ context, userId, chatId, characterId, itemId, itemTitle, slotsAffected }`; `:129-148` catch → `{ mode, effect: 'removed', effect_summary: '', item: null, slots_affected: [], error }` + WARN `Wardrobe take off operation failed`, `break`.
- `:152-157` `finalizeWardrobeMutation(…, 'wardrobe-take-off-handler', …)`.

---

## 6. The `lib/wardrobe/` seams the handlers call — what this lane NEEDS from the keystone

These are `9ecebf986` / `4a38222e6` modules the tools import; the tools lane
PORTS NONE OF THEM — it consumes the keystone's Rust twins. Recorded here so
the keystone's contract C1 can be checked against the tools' call shapes.

### 6.1 `pool.ts` (362 lines) — `WearablePool`

- `:53-71` interface: `characterId`; `tiers: WardrobeTierMounts { groupMountPointIds; projectMountPointIds }`; `byId: ReadonlyMap<string, WardrobeItemWithOrigin>` (every reachable item, **archived included**, shadowed character > group > project > general); `wearable()` (archived dropped per tier BEFORE shadowing, cached); `get(id)`; `getMany(ids)` (asked order, unknowns skipped, deduped); `findByTitle(title)` (`:237-243`: `trim().toLowerCase()`, empty → `undefined`; own items first — **archived included** — then `wearable()` excluding own ids); `owns(item)` (`:244`: `item.characterId === characterId && ownIds.has(item.id)`).
- `:195-246` `buildWearablePool(characterId, tierMounts, { own, group, project, general })` — PURE; `weakestFirst = [general, project, group, own]`; `shadow()` = Map insertion (a shadowed id keeps its FIRST insertion position with the LAST value).
- `:254-322` `loadWearablePool(repos, characterId, projectMountPointIds?, opts { operator?, chatId?, sharedTiers?, ownItems? })`: project ids passed, else `resolveProjectTierForChat(repos, chatId, characterId, { operator })` (`:149-180`: `chats.findById` → `projectId`; WARN `[WearablePool] Project lookup for chat failed` `{ chatId, context: 'wardrobe', error }` → `[]`; `rosterGatedProjectId(projectId, characterId)` unless operator; off-roster → DEBUG `[WearablePool] Character off project roster — project wardrobe withheld` `{ chatId, projectId, characterId, context }` → `[]`; else `resolveProjectMountPointIds(gated)`); in parallel `resolveGroupMountsForCharacter(characterId)`, the shared tiers (`createSharedTierLoader` `:124-142` → `loadSharedTiers` `:102-117`: General via `getGeneralMountPointId()` + the project mounts through `repos.wardrobe.readSharedTiers(mountIds, true, originOf)` — archived INCLUDED; a throw → WARN `[WearablePool] Failed to read shared wardrobe tiers; continuing without them` `{ projectMountCount, context, error }` → empty), `repos.wardrobe.findByCharacterId(characterId, true)` (WARN `[WearablePool] Failed to read the character wardrobe; using shared tiers alone` `{ characterId, context, error }` → `[]`); then the group tier via `readTier` (WARN `[WearablePool] Failed to read the group wardrobe tier; skipping it`); own items tagged `{ ...item, characterId, origin: { scope: 'character', id, name: '' } }`; DEBUG `[WearablePool] Loaded wearable pool` `{ characterId, ownCount, groupCount, projectCount, generalCount, reachable: byId.size, groupMountCount, projectMountCount, context: 'wardrobe' }`.
- `WardrobeRepository.readSharedTiers(mountPointIds, includeArchived, originOf)` (`wardrobe.repository.ts:69-98`): per mount `readSharedWardrobe(mountPointId, includeArchived)` into one Map (later mount wins), WARN `Failed to read shared wardrobe tier; skipping` `{ mountPointId, context: 'wardrobe', error }` per failure, DEBUG `Read shared wardrobe tiers` `{ mountCount, itemCount, includeArchived, context }`.
- `:329-341` `componentGraph(pool, rootIds)` — transitive walk over `componentItemIds`, cycle-safe (the `resolve-equipped` lookup).
- `:349-362` `loadCastPools` — the routes' outfit summary, not the tools.

### 6.2 `wear-ops.ts` (132 lines)

- `:43-48` `wardrobeItemNotFoundMessage(itemId, itemTitle)` → `Wardrobe item not found${itemId ? \` with ID "${itemId}"\` : ''}${itemTitle ? \` with title "${itemTitle}"\` : ''}` (moved here; the handlers' not-found words unchanged).
- `:51-60` `findInPool(pool, { itemId?, itemTitle? })` — `pool.get(itemId)` if truthy and found; else `pool.findByTitle(itemTitle)` if truthy; else `null`. Archived items ARE found.
- `:66-86` `resolveWearable(pool, ref, mode, slot?)` → `{ ok: true, item }` | `{ ok: false, reason: 'not_found' | 'archived' | 'slot', message }`; archived via `wearRefusal(item)`; slot check only for `add_to_slot` with a slot: `!item.types.includes(slot)`.
- `:95-116` `wearItem(repos, chatId, pool, item, mode, slot, source: 'ui' | 'tool')` → `applyDisplacement(repos, chatId, pool.characterId, { mode, item, slot, itemsById: pool.byId }, source)`; `add_to_slot` → `{ effect: 'layered', slotsAffected: slot ? [slot] : [] }`; else `effect = mode === 'replace' || item.replace ? 'replaced' : 'layered'`, `slotsAffected: [...item.types]`.
- `:122-132` `takeOffItem(repos, chatId, characterId, itemId, slots)` — `for slot of slots: applyDisplacement(…, { mode: 'remove_from_slot', slot, itemId })`.

### 6.3 `wearable.ts` (44 lines, NEW in `7a8d6dacc`, client-safe)

`:18-20` `archivedWearMessage(title)` → `Item "${title}" is archived and cannot be worn`; `:23-25` `wearRefusal(item)` → the message iff `item.archivedAt`; `:33-44` `newlyWornArchivedItems(items, currentSlots)` (the route's `set_all` — not the tools).

### 6.4 `composite-types.ts:31-37` `buildCompositeTypes(components, designated = [])`

`WARDROBE_SLOT_TYPES.filter(s => set.has(s))` over `unionTypes(components) ∪ designated` — canonical order `top → bottom → footwear → accessories → hair`. **Widens, never narrows.** (`unionTypes` `:17-23` unchanged; v5 `wardrobe.rs:177`.)

### 6.5 `archived-patch.ts:18-26` `archivedPatch(currentArchivedAt, archived, now)`

`Boolean(current) === archived` → `null` (no write); else `{ archivedAt: archived ? now : null }`. (v5 already has `wardrobe.rs:1308 archived_patch` — P4.D256's routes port.)

### 6.6 `item-mutations.ts` (234 lines)

- `:86-95` `compositeTypes(lookup, ids, designated)` — no lookup → `designated`; else `buildCompositeTypes(validateComponentRefs(lookup, ids).components, designated)`, empty → `designated`.
- `:106-137` `createItem(location, body, { lookup?, preserve? })`: `fields = wardrobeItemFromCreateBody(body, location.characterId)` (§6.8); `fields.folder = fields.folder ? await location.resolveFolder(fields.folder) : ''` (the tools pass none → `''`); composite → `fields.types = compositeTypes(lookup, componentItemIds, fields.types)`; `id: randomUUID()`, `archivedAt: null`, `imageFileId: null`, `migratedFromClothingRecordId: null`, `createdAt = updatedAt = now`; `location.create(item)`; DEBUG `[ItemMutations] Created wardrobe item` `{ scope, mountPointId, itemId, composite, types, context: 'wardrobe' }`.
- `:149-182` `updateItem(location, current, patch, { lookup? })`: strips `archived`; a `folder` equal to the current one is dropped, else resolved; `archived` → `archivedPatch`; **`componentItemIds = patch.componentItemIds ?? current.componentItemIds; if (componentItemIds.length > 0 && (patch.componentItemIds !== undefined || patch.types !== undefined)) next.types = compositeTypes(lookup, componentItemIds, patch.types ?? current.types)`**; `location.update(current.id, next)`; DEBUG `[ItemMutations] Updated wardrobe item` `{ scope, mountPointId, itemId, fields: Object.keys(next), found, context }`.
- `:188-203` `setItemArchived(location, current, archived)` → `{ item: current, changed: false }` when `archivedPatch` is null; else `location.update(id, stamp)`, DEBUG `[ItemMutations] Wardrobe item archive state changed` `{ scope, itemId, archivedAt, context }`, `{ item, changed: item !== null }`.
- `:49-65` `componentLookupFor` / `:214-234` `moveItemsToFolder` — routes / folders, not the tools.

### 6.7 `location.ts` (288 lines)

- `:59-105` `WardrobeLocation { scope; id; mountPointId; characterId; origin; readItems; findItem; create; update; updateMany; delete; listFolders; resolveFolder; createFolder; renameFolder; deleteFolder; writableMountPointId }`.
- `:121-189` `locationForMount(mount, id, origin)`: writes go through `writableMount()` (`:126-131` — a character mount is RE-RESOLVED at write time via `resolveWardrobeMount(characterId)`, which throws `CharacterArchivedError` for an archived character; a vanished vault throws `Character ${id} has no linked vault`); `updateMany` (`:134-143`) strips `id` / `createdAt` / `updatedAt` from every patch then `updateManyInMount`; `update` = `updateMany([{ id, patch }])[0] ?? null`.
- `:197-214` `resolveWardrobeLocation(scope, id, repos, userId, { ensure? })` → `resolve(…)` then DEBUG `[WardrobeLocation] Resolved wardrobe location` `{ scope, id: id ?? null, ensure, found, mountPointId, context: 'wardrobe' }`. The character arm `:236-245`: `repos.characters.findByIdRaw(id)`; **`!character || character.userId !== userId` → `null`**; `mountPointId = character.characterDocumentMountPointId` (null → `null`); origin `{ scope: 'character', id, name: character.name }`. The tools always call `('character', <id>, repos, context.userId)` with no `ensure`.

### 6.8 `create-body.ts` (59 lines)

`:30-32` `prose(value)` → `value && value.trim().length > 0 ? value : null`;
`:41-59` `wardrobeItemFromCreateBody(body, characterId)` → `{ characterId, title, description: prose(…), imagePrompt: prose(…), types, componentItemIds: Array.from(new Set(body.componentItemIds ?? [])), appropriateness: prose(…), isDefault: body.isDefault ?? false, replace: body.replace ?? false, migratedFromClothingRecordId: null, imageFileId: null, folder: body.folder ?? '' }`.
**Deltas vs `(pre)` for the tool:** blank (whitespace-only) `description` / `image_prompt` / `appropriateness` now store `null` (v5 `wardrobe_create.rs:349-353` filters EMPTY only); `replace` is stored for a leaf (v5 `:357-361` forces `false` for leaves); `components` deduped (already deduped upstream).

### 6.9 `outfit-displacement.ts` (now 115 lines) — `applyDisplacement`

`:87-115` `applyDisplacement(repos: DisplacementRepos, chatId, characterId, options: ComputeDisplacedOptions, source: 'ui' | 'tool' = 'ui')`: `loadSlots` (clone or empty), `logCompositeIssues` (`:57-77`: WARN `[applyDisplacement] Malformed composite graph; expansion truncated` `{ context: 'wardrobe', characterId, itemId, cycles, truncated }`; WARN `[applyDisplacement] Composite resolved to no wearable parts; wearing it whole` `{ context, characterId, itemId, componentCount }`), `nextSlots = computeDisplacedSlots(current, options)` (`slot-ops.ts:332`), `repos.wardrobeWear.commitEquippedOutfit({ chatId, characterId, nextSlots, wornBundles: wornBundlesForMode(options), source: takeOff ? 'take-off' : source })`, DEBUG `[applyDisplacement] Outfit change committed` `{ context, chatId, characterId, mode, itemId: options.item?.id ?? options.itemId ?? null, slot: options.slot ?? null, source }`. The `(pre)` primitives `equipItem` / `replaceItem` / `addToSlot` / `removeFromSlot` / `commit` and `EquipOptions` are DELETED; the pure halves live in `slot-ops.ts` (`wearItemIntoSlots :245`, `addItemToSlot :287`, `computeDisplacedSlots :332`, `wornBundlesForMode :352`, `wornBundlesFor :228`). `commitEquippedOutfit` itself (`wardrobe-wear.repository.ts`) is **unchanged** in the span (not in the `--stat`) — v5's `services/wardrobe_wear_commit.rs` stands.

### 6.10 `resolve-equipped.ts:70-138` `resolveEquippedOutfitForCharacter(pool, slots)` — SYNC

`componentGraph(pool, allEquippedItemIds(slots))`; unresolved ids → DEBUG `[resolveEquippedOutfitForCharacter] Equipped ids not in the wearable pool` `{ context: 'wardrobe', characterId: pool.characterId, unresolvedCount }`; per slot `expandComposites(slots[slot] ?? [], itemsById)`; cycles/truncated → WARN `[resolveEquippedOutfitForCharacter] Malformed composite graph; expansion truncated` `{ context, characterId, slot, cycles, truncated }`; leaves deduped in first-seen order, routed into every slot their own `types` declare. **`expand-composites.ts` LOST its WARN `[expandComposites] Max depth reached, treating as leaf`** (the span diff removes the logger import) — v5 `wardrobe.rs:431 expand_composites` should be checked for a twin line.

### 6.11 `folders.ts` — the tools read only `item.folder` (a `string`, `''` = root, set by the parser from the file path; `WardrobeFolderPathSchema.default('')` in `wardrobe.types.ts`). No folder rule touches the tools.

### 6.12 `tool-image-generation.ts` — **UNCHANGED across the span** (not in the `--stat`). `maybeQueueWardrobeToolImage` `:60-132`, `formatWardrobeToolImageLine` `:135-137`, `formatWardrobeImageHandle` `:144-146` all stand; v5 `services/tool_image_generation.rs:186` is current. (The task's named `wardrobe_item_image_generation.rs` is the JOB — bug 189's `chatId` / Concierge half at v5 `:387` / `:432` `chat_id: None` — the images lane.)

### 6.13 `wardrobe.types.ts:390-392` `isComposite(item)` → `(item.componentItemIds?.length ?? 0) > 0` (plus `addIdToSlot` / `removeIdFromSlot` / `equippedSlotsEqual`, pure).

---

## 7. The behaviour deltas a port of the TOOLS must hit (numbered, each with its v5 site)

1. **`include_presets` dropped from the schema → an `include_presets` of ANY type is stripped, never a validation failure.** v5 `wardrobe_list.rs:100-104` refuses a non-bool → DROP the arm; `definitions/data.rs:277` loses the property (§8.1).
2. **`folder` on `wardrobe_list` items** (after `is_own`), **on `wardrobe_read` output** (after `is_own`), **`folder: null` in `buildWardrobeReadFailure`** (and so in every `wardrobe_update` failure); ` [folder: …]` after the title in both renderings. v5 `wardrobe_list.rs:23-56` / `:295` / `:460`, `wardrobe_read.rs:29-61` / `:288-311` / `:373` / `:556`. Depends on the keystone's `WardrobeItem.folder` (`vault_overlay.rs:334-367` has no `folder` today).
3. **Descriptions** of `wardrobe_list` / `wardrobe_read` gain one sentence each — `data.rs:277` / `:282` via the generator (§8.1), never by hand.
4. **`wardrobe_archive` idempotent**: already archived → NO write, `already_archived: true`, the DEBUG line, the formatted sentence; the location-null `Failed to archive…` sentence BEFORE the equipped read. v5 `wardrobe_archive.rs:27-35`, `:71-181`, `:183-194` (today it unconditionally patches `archivedAt = now`).
5. **`wardrobe_create`**: the two not-found sentences (`…, their groups, the project, or Quilltap General`), `A wardrobe item must cover at least one slot`, `That character has no wardrobe to hang this in` (location null — missing character, **not the user's**, or no vault mount), the deleted `Composite components do not cover any slots…` arm, `prose()` blank→null, `replace` stored for leaves, by-id component refs resolving ARCHIVED shared items, `buildCompositeTypes` (same as v5's union ∪ supplied), **`equip_now` → `notifyWardrobeChanged` (bug 193)**. v5 `wardrobe_create.rs:247` (`let _ = user_id`), `:321-328`, `:356-376`, `:505-565`, `executor.rs:1666-1760` (`run_wardrobe_create` triggers the avatar for `target_character_id` but never calls `fold_announcements`).
6. **`wardrobe_update` widen-never-narrow** (§5.4 (a)–(d)); `fields` in the INFO line no longer carries a recomputed `types`; `pool.owns` for the read-only refusal; location-null → `Failed to update…`. v5 `wardrobe_update.rs:252-273` (the `(pre)` union block), `:220-227`.
7. **`wardrobe_wear` / `take_off`** through `resolveWearable` / `wearItem` / `takeOffItem` / `applyDisplacement` — same words, same slot math; the displacement DEBUG/WARN lines replace whatever v5's `equip_item` / `replace_item` / `add_to_slot` / `remove_from_slot` (`wardrobe_shared.rs:364-463`) log today (they log nothing; `commit :465-485`). `source: 'tool'` for put-on, `'take-off'` for both take-off modes — as v5 already has (`EquipSource::Tool` / `TakeOff`).
8. **`wardrobe_read` `is_own` = `pool.owns(item)`** (field compare AND membership in the own layer); `wardrobe_list` keeps the field compare. Identical on well-formed data; differs only for a shared item that CLAIMS the character's id (pool test `:306`).
9. **`wardrobe_list` ordering + archived shadowing**: `pool.wearable()` drops archived per tier BEFORE shadowing (an archived own copy no longer hides a shared namesake — it RESURFACES in the list); first-appearance order General → project → group → own is the same as `(pre)` `findArchetypes` (General, then project+group mounts) then own. v5 `db/wardrobe_read.rs:124 find_wearable_pool_for_character` + `wearable_pool.rs:45 merge_wearable_pool` (archived dropped AFTER shadowing) — the keystone's `pool`.
10. **Component titles from the WHOLE pool** in list AND read (`pool.getMany` over `byId`, archived included) — list `(pre)` dropped an archived part's title; read `(pre)` already included archived (`findByIdsForCharacter(…)` → `findArchetypes(true)`; v5 `wardrobe_read.rs:323-343` + `db/wardrobe_read.rs:141-186` match that), so only the LIST side moves.
11. **One context, one table**: the executor hands every wardrobe tool the set. v5's seven `run_wardrobe_*` arms (`executor.rs:1630-1960`) may stay seven Rust functions (the payload projections are already per-tool and byte-faithful) — the fidelity point is (5)'s fold, and `wardrobe_create` / `wardrobe_update` taking `user_id` for the location resolve.
12. **`scheduleWardrobeAnnouncement`'s NEW success DEBUG `Wardrobe outfit announcement scheduled`** — v5 `aurora_notifications.rs:262-291 flush_pending_wardrobe_announcements` logs nothing on success (the `enqueue_wardrobe_outfit_announcement` result is discarded). A tools-lane line only if the capture family pins it; otherwise the unifier's.
13. `userId` now reaches `resolveWardrobeLocation` in create / update / archive — a tool call for a character not owned by `userId` fails with the sentences in (5)/(6)/(4). Unreachable from the Salon (the chat's characters are the user's), reachable from the harness.

---

## 8. v5 on `main` (`f84d34150`) — where each lands

### 8.1 `crates/quilltap-core/src/tools/definitions/data.rs`

- `:275-278` `ToolDef { key: "wardrobeList", name: "wardrobe_list", json: r#"…"include_presets":{"type":"boolean","description":"Whether to include saved outfit presets in the response. Defaults to true."}…"# }` — the property leaves the `properties` object (it is the LAST property; the preceding `include_equipped` object loses its trailing comma) and the description gains the sentence before `Archived items are excluded from results.`.
- `:280-283` `wardrobeRead` — the description's `whether you own it, the slots it is` → `whether you own it, the folder the operator filed it in (their browsing aid, not a hint about when to wear it), the slots it is`.
- **Regen path, never hand-edit** (`harness/oracle/tools/gen-tool-catalog.mjs:17-24`): from the PINNED worktree `npx tsx $V5W/harness/oracle/cases/tool-definitions.ts > /tmp/oracle-tool-definitions.ndjson` then `node $V5W/harness/oracle/tools/gen-tool-catalog.mjs /tmp/oracle-tool-definitions.ndjson $V5W/crates/quilltap-core/src/tools/definitions/data.rs`. The generator rewrites the WHOLE file, so any other definition drift between `01a83539d` and the pin lands too (none expected — `git diff 3e6e2f590 4a38222e6 --stat -- lib/tools` names only the seven wardrobe files + the tool table + `image-generation-handler.ts`; the `(pre)` `tool-definitions-snapshot` hunk is the only schema move).
- Consumers of the catalog that move with it: `services/tools_inventory.rs:116-118` (`includeSchemas=true` resolves `wardrobeList` by key → the inventory's schema arm changes bytes) — see §9.6.

### 8.2 `tools/wardrobe_list.rs` (596 lines)

- `:1-6` header prose (the pool, archived-before-shadowing).
- `:23-56` `WardrobeListItemResult` gains `pub folder: Option<String>` between `is_own` (`:33`) and `is_equipped` (serde order = v4's runtime order; `None` → `null`, NOT skipped).
- `:100-104` DROP the `include_presets` type check (keep accepting it silently).
- `:188-360` `run`: `resolve_shared_wardrobe_tiers_for_chat` + `find_wearable_pool_for_character` (`:197-206`) → the keystone's pool loader; the `titles_by_id` map over `all_items` (`:235-243`) → `pool.get_many(component_item_ids)` over the FULL pool; `:295` add `folder: item.folder.filter(non-empty)`; the INFO's `totalItems` stays the wearable count.
- `:428-500` `format_at`: a `folder_tag` between the title and `equipped_tag` (`:~460-470`).
- Tests `:500-596` (the `the_list_completed_line` unit etc.) follow.

### 8.3 `tools/wardrobe_read.rs` (671 lines)

- `:29-61` `WardrobeReadToolOutput` gains `pub folder: Option<String>` after `is_own` (`:46`); `:288-311` `build_read_failure` sets `folder: None`; `:315-385` `build_read_output(main, docs, character_id, chat_id, item, tiers)` → takes the pool: `component_titles` via `pool.get_many` (`:323-343` replace `find_by_ids_for_character`), `is_own: pool.owns(item)` (`:373`), `folder` after it; `:509-542` `run` → `find_in_pool`; `:550-620` `format_at` head line tag (`:556`).
- `validate_locate` `:434-452` and `not_found_message` `:457-467` (RAW inputs) unchanged — v4 still passes the raw `item_id` / `item_title` to the not-found message in read / update / archive and the SENTINEL-normalised ones in wear / take-off (`wardrobe_wear.rs:382`, `wardrobe_take_off.rs:311` — unchanged).

### 8.4 `tools/wardrobe_create.rs` (660 lines)

- `:1-12` header; `:41-83` `WardrobeCreateToolOutput` (`target_character_id`, `picture_character_id`, `generate_image` ride-outs stay; `target_character_id` becomes the ANNOUNCE id as well).
- `:238-254` `execute` must thread `user_id` (today `let _ = user_id;`).
- `:269-452` `run`: `:299-308` tiers → the pool keyed on the target (`load_wearable_pool(.., target, None, chat_id)`); `:310-317` `resolve_component_items` over the pool; `:319-347` the composite-types block → `build_composite_types` (identical maths) and DELETE the `Composite components do not cover any slots…` arm (`:321-328`, the sentence at `:325`); NEW `A wardrobe item must cover at least one slot` guard; NEW `resolve_wardrobe_location(Character, target, user_id)` → `That character has no wardrobe to hang this in`; `:356-376` the `WardrobeItem` literal → the keystone's `create_item(location, body, lookup)` (prose/blank → `None` — today `.filter(|s| !s.is_empty())` drops EMPTY only; `replace: input.replace.unwrap_or(false)` for leaves too — today forced `false` for a leaf; `folder: ""`); `:378-379` `create_vault_wardrobe_item` → `location.create`; `:385-400` `wardrobe_shared::equip_item(…)` → `wear_item(pool_with_new, item, Wear, None, Tool)`; the `effect` from the outcome.
- `:505-565` `resolve_component_items` — the two sentences (`:542`, `:554`) + pool lookups (by id over `by_id`, archived included; by title via `find_by_title`).
- `executor.rs:1666-1760` `run_wardrobe_create`: fold `out.target_character_id` into `ctx.pending_wardrobe_announcements` (the existing `fold_announcements` helper, `:2385-2392`) — the P4.123 trigger and the announce become ONE `notify_wardrobe_changed` twin, as the other three arms already pair them.

### 8.5 `tools/wardrobe_update.rs` (314 lines)

- `:1-7` header (bug 195); `:146-178` `execute` / `execute_with_picture_claim` thread `user_id`; `:181-298` `run`: `:189-197` tiers → pool; `:199-206` `find_in_pool`; `:220-227` the read-only refusal via `pool.owns`; `:252-273` DELETE the `(pre)` union block; `:276-288` `update_vault_wardrobe_item(…, Some(character_id))` → `resolve_wardrobe_location` + the keystone's `update_item(location, item, patch, lookup)` (which owns the widen rule); location `None` → `Failed to update wardrobe item "<title>"`; `:290-294` `UpdatePictureClaim.changes_look = patch_changes_look(&item, &patch)` — over the HANDLER's patch (no recomputed types), as v4; `:296` `build_read_output(pool…)`.

### 8.6 `tools/wardrobe_archive.rs` (194 lines)

- `:27-35` `WardrobeArchiveToolOutput` gains `#[serde(skip_serializing_if = "Option::is_none")] pub already_archived: Option<bool>` (the executor's projection at `executor.rs:1851-1858` must NOT add it — `formattedText` carries it).
- `:50-69` `execute` threads `user_id`; `:71-181` `run`: `:80-87` tiers → pool; `:89-96` `find_in_pool`; `:110-117` `pool.owns`; NEW location resolve + `Failed to archive…` BEFORE `:128-135` (the equipped read); `:138-156` the unconditional `archived_at: Some(Some(clock::now_iso()))` patch → `set_item_archived(location, item, true)`; `!changed && archived` → the DEBUG + `already_archived: Some(true)` + NO announce; `:183-194` `format` gains the already-archived sentence.

### 8.7 `tools/wardrobe_wear.rs` (626 lines) / `tools/wardrobe_take_off.rs` (368 lines)

- wear `:156-243` `run` + `:245-357` `apply_op`: `resolve_wardrobe_item_across_tiers` → `resolve_wearable(pool, ref, mode, slot)` (three reasons; `:267` archived + `:293` slot sentences stay byte-identical, now sourced from `wearable::wear_refusal` / `wear_ops`); `add_to_slot` / `replace_item` / `equip_item` (`:297-343`) → `wear_item` → `apply_displacement`.
- take-off `:145-249` `run` + `:251-309` `apply_remove`: `remove_from_slot(…, None)` (`:174`) → `apply_displacement(ClearSlot)`; `find_in_pool`; the `remove_from_slot(…, Some(&id))` loop (`:294`) → `take_off_item`.
- Both: the trailing `build_wardrobe_coverage_summary_from_state(main, docs, character_id, state, tiers)` → the SYNC pool-based `resolve_equipped_outfit_for_character(pool, slots)`; the `announce` id lists stay (the executor's `fold_announcements` + `trigger_for_characters` IS `notifyWardrobeChanged`).

### 8.8 `tools/wardrobe_shared.rs` (786 lines) — the surface that EMPTIES

- `:56-98` `resolve_project_mount_point_ids[_for_chat]` — v4 moved the chat→project lookup into `pool.ts:149-180 resolveProjectTierForChat` (roster-gated, with the two `[WearablePool]` lines) and KEPT `resolveProjectMountPointIds` in `tiered-mount-pool.ts`; `tiered_mount_pool_equivalence.rs:494-500` pins both v5 functions — keystone's call (see §11).
- `:100-133` `resolve_wardrobe_item_across_tiers` → DELETE (→ `find_in_pool`); `:156` `is_own_wardrobe_item` → DELETE (→ `pool.owns`; `wardrobe_list` keeps an inline field compare); `:225` `hydrate_component_graph` → `component_graph(pool)` (also used by `services/wardrobe_item_image_generation.rs` — the images lane; §11); `:294` `lookup_for_bundle` → `pool.by_id`; `:364-463` the four primitives + `:465-485` `commit` → ONE `apply_displacement` (shared with `api/chat_outfits.rs:71-73` and `services/aurora_notifications.rs:46-48` — §11); `:504` `resolve_equipped_outfit_values` / `:752` `build_wardrobe_coverage_summary_from_state` → pool-based.
- Keep: `find_equipped_slots :163`, `empty_equipped_state :181`, `load_current_wardrobe_state :187`, `worn_bundles_for :338` (v4 `slot-ops.ts:228` keeps it; `dissolve_bundles_equivalence.rs:35` pins it), `public_error_message :135` (its `NoMount` arm goes unreachable for the tools once the location seam answers the sentences — keep for the carriers' owner-hint adapters, v4 `wardrobe.repository.ts:105`'s `create` comment).

### 8.9 `wearable_pool.rs` (172 lines) → v4 `mergeWearableTiers(tiersWeakestFirst)` + `mergeWearablePool(shared, own)` (`wearable-pool.ts:27-40`, archived dropped PER TIER inside the loop) — the keystone's pure leaf; `wardrobe_tiers.rs:55 SharedWardrobeTiers` → `WardrobeTierMounts`.

### 8.10 Unchanged by this lane

`services/tool_image_generation.rs` (v4 file unchanged), `services/wardrobe_wear_commit.rs` (v4 `commitEquippedOutfit` unchanged), `services/avatar_generation.rs:222 trigger_for_characters`, `tools/wardrobe_list.rs:364-420` (`list_times`, the wear note), `tools/wardrobe_read.rs:90-260` (the wear paragraph), `services/tools_inventory.rs:66-72` (the `BUILT_IN_TOOLS` rows — v4 `app/api/v1/tools/route.ts` is not in the span).

---

## 9. Harness — families, what they compare, predicted reds, corpus rows to ADD

Recipe headers are the leading `//!` block of each `crates/quilltap-harness/tests/<family>.rs`
(`harness/tools/recipe_sweep.py:285-400` extracts lines indented ≥ 2 past the
marker; a `.rs` header that elides the jest stage is restored from the oracle
case's own `/** … */` header — `ok_restored`). Every regen below runs from the
PINNED worktree (`recipe_sweep.py --v4 <pin> --run <family>`, `--v5w ~/source/quilltap-v5`
— the spaced path breaks the regen). All seven families were `ok` in the last
sweep (`sweep-results/2026-10-12-01a83539d-memory-unify.json`).

### 9.1 `wardrobe_tools_equivalence.rs` (940 lines) — tier 2/3, the main family

- Fixture: `harness/oracle/fixtures/wardrobe-tools.json` (caller Clio + recipient Basil with full vaults, General archetypes, TWO group stores — "The Household" holding the caller with a group-only garment + a copy SHADOWING a General item by id, and the recipient's group with `Regimental Sash` —, a chat, a seeded equipped outfit, the P4.D262 `wearPlant` / `picturePlant` / `pictureScenario`), built by `fixtures/build-wardrobe-tools-fixture.ts` through v4's REAL repos into `QT_FIXTURE_WT_MAIN` / `QT_FIXTURE_WT_MOUNT`; oracle `cases/wardrobe-tools.ts` (272 lines) runs v4's REAL `execute*` + `format*(…, NOW_MS)` with ONE `pendingWardrobeAnnouncements` set for archive / wear / take-off (`:139-168`) — **list / read / create / update get `base` only** (`:142-153`), so the set stays absent for `create_composite_equip` → v4 at the pin would ENQUEUE an announcement at once (`recordPendingWardrobeAnnouncement` without a set) — with the job host held off (`__quilltapJobHost.shuttingDown`) the row lands PENDING in `background_jobs`, which this case does NOT read back (only `pictureScenario`'s `jobs` are read, `:254-264`). Compared: `returns` (per op `{ name, tool, output, formatted }`, UUIDs → positional tokens, ISO → `<ts>`), `readback` (both wardrobes, archetypes, equipped, the ledger rows), `pictures` + `jobs`.
- 36 ops + 12 picture ops (`fixtures/wardrobe-tools.json` `ops`). **Predicted reds under the pin:**
  - every successful `wardrobe_list` row (`list_all`, `list_type_top`, `list_after`, `list_as_recipient`, `list_after_pictures`) — `folder: null` on every item (v5 emits no key); `list_after` additionally if any composite's part was archived (none is — `Red Hat` is a leaf);
  - every `wardrobe_read` row incl. failures (`read_composite`, `read_invalid`, `read_notfound`, `read_gift_as_recipient`, `read_group_item`, `read_shadowed_scarf`, `read_group_item_as_recipient`, `read_pictured_item`) — the `folder` key; the head line unchanged (all at the root);
  - every `wardrobe_update` row (`update_own`, `update_shared`, `update_invalid`, the eight `update_picture_*`) — the read-shaped echo / failure's `folder`;
  - `create_unknown_component` — the sentence;
  - `create_composite_equip` — the `.md` / readback should NOT move (the wearItem path writes the same slots; the fixture's oracle passes no set so v4 enqueues an announcement the case never reads) — GREEN unless the `[ItemMutations]` / `[applyDisplacement]` lines are captured (they are not — `the_list_completed_line_is_v4s` `:826` captures only the list INFO);
  - the `readback` of created items: `description` for a blank input (none in the corpus), `replace` on a leaf (none passes `replace`) — GREEN today, red once the rows below are added.
  - `archive_own` — GREEN (a fresh archive); `archive_shared` — GREEN (the refusal sentence unchanged).
- **Corpus rows to ADD** (the fixture's `ops`, both sides through the same spec): `archive_twice` (archive `Red Hat` again → `already_archived: true` + the sentence, no ledger / `.md` change); `list_folder_item` + `read_folder_item` (needs a fixture item FILED under `Wardrobe/Formal/Evening/` — the builder must write the file at that path through v4's REAL store, `4a38222e6`'s `isWardrobeItemDocumentPath` reads it; the ` [folder: Formal/Evening]` tag + `folder` key); `list_include_presets_wrong_type` (`{ include_presets: 5 }` → now SUCCESS); `create_leaf_replace` (`replace: true` on a leaf → stored); `create_blank_description` (`"   "` → `null`); `create_no_wardrobe` (a recipient character with no vault → `That character has no wardrobe to hang this in`); `archive_component_then_list` + `read_composite_archived_part` (archive `Loafers` (a `Casual Outfit` part) then list/read → `component_titles` still names it); `update_composite_types_narrow` (`types: ["top"]` on `Casual Outfit` → WIDENED to the union ∪ top; `fields` without recomputed types) and `update_components_keep_designated` (a composite designating an extra slot, components changed, no `types` → the slot KEPT — bug 195); `create_archived_shared_component_by_id` (a by-id ref to an ARCHIVED General item now resolves). Each is a one-line `ops` entry; the handler INFO lines' `fields` need the capture extended if pinned.

### 9.2 `wardrobe_tools_avatar_trigger_equivalence.rs` (609 lines) — tier 2, THROUGH THE EXECUTOR

- Fixture: the flag-ON rebuild (`QT_WT_AVATAR_SPEC=…/wardrobe-tools-avatar-trigger.json` into `QT_FIXTURE_WTA_*`); oracle `cases/wardrobe-tools-avatar-trigger.ts` passes ONE set to EVERY tool (`:112-117`) and records **`pendingAnnouncements: Array.from(set).sort()` per scenario** (`:147`) + the `background_jobs` rows + equipped. v5 drives `BuiltInToolRunner` (`:150`) and compares the per-scenario set.
- **Predicted reds (bug 193):** `create_equip_self` (the caller now in the set), `create_equip_gifted` + `autonomous_create_gifted` (Basil), `picture_switch_on_equip_and_redraw` (the caller) — v5's `run_wardrobe_create` never folds. `create_no_equip` stays empty both sides. The avatar `background_jobs` rows are UNCHANGED (the trigger fired `(pre)` too). GREEN again once `run_wardrobe_create` folds `target_character_id`.
- `the_committed_wardrobe_corpus_stays_flag_off` (`:397`) reads the builder — unaffected.

### 9.3 `project_roster_access_equivalence.rs` — runs `wardrobe_list::execute` (`:683-698`) and compares `{ output, formatted }` → **RED on `folder: null`** for every `wardrobe_list` op; also captures the resolver's WARN lines (`:699-705`) — the pool's `[WearablePool] Character off project roster — project wardrobe withheld` DEBUG and `Project lookup for chat failed` WARN REPLACE the `(pre)` `shared-tiers.ts` lines this family pins (the keystone's), so expect the log compare to redden on the pool's line names too.

### 9.4 `tool_definitions_equivalence.rs` (108 lines) — byte-exact — **RED** on `wardrobeList` (schema + description) and `wardrobeRead` (description) until `data.rs` is regenerated (§8.1); `QT_ORACLE_TOOL_DEFINITIONS_CANONICAL` (the ICU-sorted whole catalog) moves with it.

### 9.5 `chat_admin_routes_equivalence.rs` (`:1043-1065`) — `?action=run-tool` with `wardrobe_list` through `BuiltInToolRunner` (`:1066-1083`), `check(name, &r, tables)` on the response — the stored tool result's `items[*]` gain `folder` → **likely RED** on the three `run_tool_*` rows (confirm what `check` compares — the response data embeds `result`).

### 9.6 `tools_inventory_equivalence` — oracle `cases/chat-dialogs-tools.test.ts:208` / `:233` run `?includeSchemas=true`, and `services/tools_inventory.rs:116` resolves `wardrobeList`'s `parameters` from the catalog → **RED on the two schema rows** until `data.rs` moves (then GREEN with no v5 code change).

### 9.7 Stay GREEN (predicted)

`generated_wardrobe_items_equivalence.rs` (the span's `generated-items.ts` hunk is a doc comment on `replace`); `tool_image_generation_equivalence.rs` (`format_wardrobe_list_wear_note` / `format_wardrobe_wear_paragraph` unchanged); `dissolve_bundles_equivalence.rs` (`worn_bundles_for` kept in `slot-ops.ts`); `wardrobe_wear_commit_tier2_equivalence` (the chokepoint unchanged); `tiered_mount_pool_equivalence.rs:494-500` (both v5 resolvers unchanged in v4 — `resolveProjectMountPointIds` kept; the chat variant's deletion is the keystone's call, see §11).

### 9.8 v4 side of the regen

`cases/wardrobe-tools.ts` imports the handlers by path (`:109-115`) — all still exist at the pin; `build-wardrobe-tools-fixture.ts` imports `ensureGroupOfficialStore` from `@/lib/mount-index/ensure-group-store` (`:269`) — **DELETED in `9ecebf986`** (→ `ensure-owner-store.ts` `ensureOwnerOfficialStore(scope, ownerId, name)`), so the fixture builder FAILS under the pin until repointed (the keystone / carriers survey owns the builder; note it here because this family cannot regen without it). The builder also seeds through `repos.wardrobe.create` (kept at the pin, `wardrobe.repository.ts:105-108`) — fine.

---

## 10. v4 jest tests covering these (at the pin; names + case counts)

- `__tests__/unit/lib/tools/handlers/wardrobe-handlers.test.ts` — **33 cases** (mocks `logger`, `repositories/factory`, `outfit-change-effects`, `tool-image-generation`, `tiered-mount-pool`, `instance-settings`, `roster-access`, `location`, `outfit-displacement`; drives `buildWearablePool` directly): `executeWardrobeListTool` 6 (merges own + General + flags ownership; folds group stores; own overrides shared on id; **an archived personal copy does not hide the shared item**; `image_file_id`; type filter), `executeWardrobeReadTool` 3, `executeWardrobeCreateTool` 6 (imagePrompt; null imagePrompt; picture default-on for the owner; equips on `equip_now`; **fires the outfit-change effects with the turn's set on `equip_now` (bug 193)**; no effects when only hung up), `executeWardrobeUpdateTool` 4 (incl. refuses a shared item and never writes), `executeWardrobeArchiveTool` 4 (stamps `archivedAt` never deletes; effects when equipped; **keeps the original date when already archived (bug 188)**; refuses shared), `executeWardrobeWearTool` 7 (layered; replace; add_to_slot; a General item; multi-op fires effects ONCE; fails fast; **rejects an archived item and fires nothing**), `executeWardrobeTakeOffTool` 3.
- `wardrobe-handler-shared.test.ts` — 4 (empty state; sentinels; coverage summary from the pool's expanded leaves; the Friday four-slot composite regression).
- `wardrobe-tool-table.test.ts` — **1 + `it.each` × 7 = 8** ("covers exactly the seven wardrobe_* tools"; "%s forwards the turn's announcement set to its handler (bug 193)" — `toBe(pending)` identity).
- `wardrobe-wear-readout.test.ts` — 12 (list 3, read 7, **`folders in wardrobe_list and wardrobe_read` 2**: `'  [top] Greatcoat [folder: Formal/Evening] · worn by you 3×, last 3 days ago (4× in the household)'` / `'  [footwear] Spats · never worn'`; `'Greatcoat (item-1) [folder: Formal/Evening]'` and `'Spats (item-2)'`, `folder` `null` at the root).
- `__tests__/unit/lib/wardrobe/outfit-change-effects.test.ts` — 10; `pool.test.ts` — 35 (precedence 5, archived 4, lookups 10 incl. `owns` ×3 / `findByTitle` ×5, project tier 4, I/O shape 4, degradation 3, `readSharedTiers` 5); `wear-ops.test.ts` — 11; `item-mutations.test.ts` — 26 (`buildCompositeTypes` 2, `validateComponentRefs` 1, `createItem` 5, **`updateItem — composite types widen, never narrow (bug 195)` 4**, the archived flag 3, **`setItemArchived — idempotent (bug 188)` 4**, `componentLookupFor` 3, folders 4).
- `lib/tools/__tests__/tool-definitions-snapshot.test.ts` — the `wardrobeList` parameters snapshot (§1.5).
- Deleted: `__tests__/unit/lib/database/repositories/wardrobe.repository.pool.test.ts` (−257, the `(pre)` pool semantics), `mount-wardrobe-route-factory.test.ts` (routes).

---

## 11. Cross-lane meeting points

1. **The keystone (pool / location / item-mutations / wear-ops / wearable / composite-types / archived-patch / `applyDisplacement` / pool-based `resolve_equipped`):** the tools lane consumes, by name, `load_wearable_pool(repos, character_id, project_mounts: Option<&[String]>, { operator, chat_id, shared_tiers, own_items })` → a `WearablePool { character_id, tiers, by_id, wearable(), get, get_many, find_by_title, owns }`; `find_in_pool`, `resolve_wearable`, `wear_item`, `take_off_item`, `wardrobe_item_not_found_message`; `wear_refusal` / `archived_wear_message`; `build_composite_types`; `resolve_wardrobe_location(scope, id, user_id, ensure)` → `WardrobeLocation { scope, id, mount_point_id, character_id, origin, create, update, update_many, … }`; `create_item(location, body, lookup)` / `update_item(location, current, patch, lookup)` / `set_item_archived(location, current, archived)`; `apply_displacement(…, options, source)`; `resolve_equipped_outfit_for_character(pool, slots)`; `component_graph(pool, roots)`; `WardrobeItem.folder` (`vault_overlay.rs:334`); `is_composite`. The tools lane should expose ITS needed shapes as the C1 checklist and stack on the keystone branch (§R.14).
2. **`tools/wardrobe_shared.rs` is shared with the ROUTES lane** (`api/chat_outfits.rs:71-73` imports `add_to_slot, equip_item, lookup_for_bundle, remove_from_slot, replace_item, worn_bundles_for`) and **`services/aurora_notifications.rs:46-48`** (`resolve_equipped_outfit_values, resolve_project_mount_point_ids_for_chat`), and **`services/wardrobe_item_image_generation.rs`** (`hydrate_component_graph`) — the IMAGES lane. Deleting the primitives here breaks three lanes' compiles unless the keystone lands `apply_displacement` FIRST and each lane repoints; the safest ownership is: the keystone owns `wardrobe_shared.rs`'s deletions (or leaves thin forwarding shims the unifier removes), the tools lane only edits the seven handler files + `executor.rs`'s arms + `data.rs`.
3. **`executor.rs:1630-1960`** — the tools lane's only executor edit is `run_wardrobe_create` (fold + `user_id`); no other lane touches the wardrobe arms.
4. **`help/wardrobe.md:251`** (`Retiring a garment twice changes nothing --- it keeps the date it was first put away.`) and `:170` (the folders-as-labels paragraph) — the help / `docs/v4/` re-vendor lane; the tools lane cites them, does not edit them.
5. **The fixture builder** (`build-wardrobe-tools-fixture.ts:269` `ensure-group-store` → `ensure-owner-store`) — whoever owns the builder (carriers / keystone) must repoint it before THIS family can regen under the pin; the tools lane adds the `Wardrobe/Formal/Evening/` item and the new ops.
6. **`tiered_mount_pool_equivalence.rs:494-500`** pins `resolve_project_mount_point_ids_for_chat`, which v4 folded into `pool.ts:149-180 resolveProjectTierForChat` (roster-gated); `aurora_notifications.rs` still calls the un-gated form (the announcement job — v4's `handlers/wardrobe-announcement.ts` is NOT in the span, so its read stays un-gated). The keystone decides whether the v5 helper survives beside the pool's gated variant.
7. **`services/tools_inventory.rs`** moves with `data.rs` (no code edit); `chat_admin_routes_equivalence` / `project_roster_access_equivalence` redden from this lane's `folder` key — list them in the lane's expected-red set so the unifier does not chase them as union reds.

---

## 12. Open points for the order (not rulings — observations)

- v4's `wardrobe-tools.ts` oracle passes NO set to `wardrobe_create` (`:148`) — under the pin a `create_composite_equip` enqueues an announcement job the case never reads; harmless, but the lane should pass the set to all seven (as `wardrobe-tools-avatar-trigger.ts:112-117` does) so the differential matches the executor's shape, or leave it and rely on §9.2 for bug 193.
- `A wardrobe item must cover at least one slot` is unreachable from a schema-valid input (`types.nonempty()` + the `.refine`); port the arm for fidelity, no corpus row can reach it without bypassing the validator (the oracle calls `execute*` with raw input, so a `{ title, types: [] , component_item_ids: [] }`… still fails the `.refine`; `{ title, types: [] }` fails `.nonempty()`). Record as unreachable both sides, like the deleted `Composite components…` arm.
- `wardrobe_list`'s INFO `totalItems` is `pool.wearable().length` — identical to `(pre)` except when an archived own copy shadows a shared item (the shared one now counts).
- `expandComposites` lost its `Max depth reached` WARN (§6.10) — if v5 `wardrobe.rs:431` logs one, the keystone drops it; the tools' coverage summaries route through it.
