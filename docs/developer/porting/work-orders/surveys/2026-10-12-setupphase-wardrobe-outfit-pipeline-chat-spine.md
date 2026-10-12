# Survey — "one outfit pipeline": the chat equip action, the outfit summary, the equipped-outfit resolver, the initial-avatar profile, and the avatar refresh on join / open (`7a8d6dacc` + `9ecebf986`, at the pin `4a38222e6`)

Read-only planning survey, 2026-10-12. v4 = `~/source/quilltap-server`, read
ONLY through `git show 4a38222e6:<path>` (the PIN — every v4 line number below
is AT `4a38222e6`), `git show <sha> -- <path>` for one commit's hunk, and
`git diff 3e6e2f590 4a38222e6 -- <paths>` for the span; the working tree was
never read and no commit message or doc prose was trusted (ledger §5.3).
v5 = `/Volumes/External 1/source_overflow/quilltap-v5` main at `f84d34150`,
cited `path:line`.

Scope (this lane): v4 `app/api/v1/chats/[id]/actions/outfit.ts` (the three
handlers), `lib/wardrobe/resolve-equipped.ts`, `outfit-displacement.ts`,
`outfit-change-effects.ts` (NEW), `avatar-generation.ts`,
`lib/image-gen/profile-resolution.ts`'s `resolveAvatarImageProfile` (NEW) and
its three consumers (`avatar-generation.ts`, `toggle-avatar-generation.ts`,
`wardrobe/preview-avatar` — the last is the item-images lane's), the chat-open
and participant-join refresh sites, `apply-outfit-selections.ts`, and the
smalls `default-outfit.ts` / `expand-composites.ts` / `composed-outfits.ts` /
`group-equipped.ts` / `outfit-description.ts` / `staged-live-outfits.ts`.
**Out of scope, NAMED only:** the keystone's definitions (`lib/wardrobe/
{pool,wear-ops,wearable,slot-ops,composite-types}.ts`, `wearable-pool.ts`),
the seven tools, the route factory, the item-image / preview-avatar routes,
`4a38222e6`'s folders, every SPA file, `help/` + `docs/v4/`.

Every fact below is from the hunks; where the ledger's §3 row and the hunks
disagree, the hunk wins and the correction is called out (§0).

---

## 0. Three corrections to the ledger's §3 rows, from the hunks

1. **The `No usable image profile; skipping initial avatar generation` DEBUG
   does NOT live in `chats` POST.** The `9ecebf986` row says "`chats` POST
   resolves the initial avatar profile through `resolveAvatarImageProfile(
   user.id, repos, { chat })` (debug …)". The hunk is in
   `app/api/v1/chats/[id]/actions/toggle-avatar-generation.ts:54-62` (the
   toggle-ON sweep). `app/api/v1/chats/route.ts`'s only avatar hunks are the
   `refreshAvatarForOutfit` rename (`:68`, `:609`) and the whisper's pool swap
   (`:585-588`) — §8 below.
2. **`[Chats v1] Refused to wear an archived item` is GONE at the pin.** It is
   `7a8d6dacc`'s intermediate line (three sites, `outfit.ts:317/335/353` AT
   `7a8d6dacc`); `9ecebf986` rewrote the three put-on modes through
   `wear-ops`, and the pin's line is `[Chats v1] Refused to put on a wardrobe
   item` with a `reason` key (`'not_found' | 'archived' | 'slot'`). Only the
   `set_all` line `Refused a fitting that puts on an archived item` survives
   from `7a8d6dacc`. Port the pin, never the intermediate.
3. **The pin's `set_all` 400 for an unreachable id has no log line** (the
   row implies one). It never did: `outfit.ts:190-194` returns `badRequest`
   silently, both before and after.

---

## 1. v4 at the pin — `app/api/v1/chats/[id]/actions/outfit.ts` (283 lines; −264 / +119 over the span)

### 1.1 Header, imports, body schema — what did NOT move

- `:16-29` imports: `applyDisplacement` (`outfit-displacement`),
  `isComposite` / `WARDROBE_SLOT_TYPES` / `EquippedSlotsSchema` /
  `WardrobeItemTypeEnum` / `bySlot` (`schemas/wardrobe.types`),
  `loadCastPools` / `loadWearablePool` / `WearablePool` (`wardrobe/pool`),
  `resolveEquippedOutfitForCharacter` (`wardrobe/resolve-equipped`),
  `notifyWardrobeChanged` (`wardrobe/outfit-change-effects`),
  `wornBundlesFor` / `WornBundle` (`wardrobe/slot-ops`),
  `resolveWearable` / `wearItem` / `PutOnMode` (`wardrobe/wear-ops`),
  `newlyWornArchivedItems` / `wearRefusal` (`wardrobe/wearable`).
  GONE imports: `equipItem` / `replaceItem` / `addToSlot` / `removeFromSlot`
  / `wornBundlesFor` from `outfit-displacement`, `loadBundleLookup`
  (`hydrate-components`), `isBundle` (`dissolve-bundles`),
  `expandComposites`, `triggerAvatarGenerationIfEnabled`,
  `enqueueWardrobeOutfitAnnouncement`,
  `resolveGroupMountPointIdsForCharacter` / `resolveProjectMountPointIds`,
  `resolveSharedWardrobeTiersForChat` (`shared-tiers`).
- `:31-83` `equipBodySchema` — **byte-identical** to `3e6e2f590:40-92`: the
  seven-member `mode` enum (`wear`, `replace`, `equip`, `add_to_slot`,
  `remove_from_slot`, `clear_slot`, `set_all`), `slot: WardrobeItemTypeEnum.
  optional()`, `itemId: z.string().nullable().optional()`,
  `slots: EquippedSlotsSchema.optional()`, `wornBundleIds: z.array(z.string().
  min(1)).optional()`, the three superRefine arms (`slot is required for mode
  "<mode>"`, `itemId is required for mode "<mode>"`, `slots is required for
  mode "set_all"`). v5 `api/chat_outfits.rs:406-543` `parse_equip_body` —
  untouched by this round.
- `:88-101` `handleGetOutfit` — unchanged (`{ equippedOutfit: … ?? {} }`;
  ERROR `[Chats v1] Error fetching equipped outfit` `{chatId}` → 500 `Failed
  to fetch equipped outfit`). v5 `chat_outfits.rs:115-126`.

### 1.2 `handleGetOutfitSummary` (`:112-151`) — REWRITTEN

```ts
const chat = await repos.chats.findById(chatId);
if (!chat) return notFound('Chat');                                   // 404 "Chat not found"
const equippedOutfit = (await repos.chats.getEquippedOutfit(chatId)) ?? {};
const characterIds = Object.keys(equippedOutfit);
const pools = await loadCastPools(repos, chat.projectId, characterIds);  // ALWAYS called
…
for (const [characterId, slots] of Object.entries(equippedOutfit)) {
  const pool = pools.get(characterId);
  if (!slots || !pool) { summary[characterId] = bySlot<SummaryEntry[]>(() => []); continue; }
  const { leafItemsBySlot } = resolveEquippedOutfitForCharacter(pool, slots);
  summary[characterId] = bySlot((slot) =>
    leafItemsBySlot[slot].map((leaf) => ({ itemId: leaf.id, title: leaf.title })));
}
logger.debug('[Chats v1] Built outfit summary', { chatId, characterCount: characterIds.length, context: 'wardrobe' });
return NextResponse.json({ summary });
```
catch → ERROR `[Chats v1] Error fetching equipped outfit summary` `{chatId}`
+ 500 `Failed to fetch equipped outfit summary` (unchanged).

**Response shape / key order (unchanged bytes where the inputs agree):**
`{ summary: { [characterId in equippedOutfit's own key order]: { top, bottom,
footwear, accessories, hair: [{ itemId, title }, …] } } }` — `bySlot`
(`wardrobe.types.ts:336-338`) is `Object.fromEntries(WARDROBE_SLOT_TYPES.map(
(s) => [s, fn(s)]))`, so the slot keys are in declaration order; each entry
is `{ itemId, title }` in that order.

**The resolution rule (the one to record):** each character's slots resolve
against **that character's own `WearablePool`** (own vault > their groups >
the chat's project stores > General — the project tier is NOT roster-gated
here: `loadCastPools` resolves `resolveProjectMountPointIds(projectId ??
null)` directly, the header comment says "The operator is reading … so the
project roster doesn't apply"), by `resolveEquippedOutfitForCharacter`
(§5): composites expand transitively over the pool, leaves are de-duplicated
across the WHOLE equipped set in first-seen order (slot order, then array
order), and **each leaf lands under EVERY slot its own `types` cover** — a
`top+bottom` dress equipped only to `top` appears under `top` AND `bottom`;
a composite's blouse / slacks / loafers spread to `top` / `bottom` /
`footwear` whatever slot the composite id sat in. Archived items resolve
(the pool's `byId` is archived-inclusive). An id outside the character's pool
is dropped (DEBUG, §5).

**Deltas vs `3e6e2f590` (`:119-212` then) — the wire-visible ones:**

| | `3e6e2f590` | pin `4a38222e6` |
|---|---|---|
| item pool | ONE cast-wide map: `findArchetypes(true, {groupMountPointIds: UNION of every outfit-keyed character's groups, projectMountPointIds})`, then EVERY outfit-keyed character's `findByCharacterId(id, true)` layered on top (later characters overwrite same ids) | one pool PER character (`loadCastPools`): own vault + OWN groups only; project + General read once and shared |
| cross-character ids | a character resolved another participant's vault item (and a co-participant's group livery) | dropped — "drops an id outside the character's own pool rather than borrowing another character's item" (`outfit.test.ts:392`) |
| leaf routing | per INPUT slot: `expandComposites(equippedIds)`, keep a leaf only `if leaf.types.includes(slotKey)` — a dress equipped to `top` alone showed under `top` only; a composite's loafers in `top` were DROPPED | every slot the leaf's `types` cover — the dress shows under `top` AND `bottom`; the loafers land in `footwear` |
| dedupe | per input slot (`seen` reset per slot) | across the whole equipped set (`seenLeafIds`), then routed |
| zero equipped ids | no reads at all (`allItemIds.size > 0` guard) | `loadCastPools` still runs: `resolveProjectMountPointIds` + `createSharedTierLoader(…)()` is INVOKED at `pool.ts:355`, so General + project tiers are read once even for an empty cast (no pool per character, no `Loaded wearable pool` line) |
| log | none | DEBUG `[Chats v1] Built outfit summary` `{chatId, characterCount, context: 'wardrobe'}` + the pool's and resolver's lines (§3, §5) |

v5 today: `api/chat_outfits.rs:142-297` ports the OLD body exactly — the
union-of-groups `:192-210` (with a comment citing `8600c83f` as "the one
documented cast-spanning exception" — that exception is now GONE in v4), the
cast-wide pool `:215-233`, the per-input-slot coverage filter `:268-275`, no
DEBUG line. The whole body is replaced.

### 1.3 `resolveWornBundles(pool, wornBundleIds)` (`:159-175`) — now sync, pool-based

```ts
const ids = Array.from(new Set(wornBundleIds)); if (ids.length === 0) return [];
const result = pool.getMany(ids).filter(isComposite).flatMap((bundle) => wornBundlesFor(bundle, pool.byId));
if (result.length !== ids.length) logger.debug('[Chats v1] Some claimed worn bundles were not credited',
  { characterId: pool.characterId, claimed: ids.length, resolved: result.length, context: 'wardrobe' });
```
Same DEBUG text and keys as before (v5 `chat_outfits.rs:833-839` logs it from
the caller thread — keep). Deltas: `pool.getMany` (first-seen dedupe, order of
`ids`, archived INCLUDED, shadowed character > group > project > general) in
place of `findByIdsForCharacter(…, tiers)`; `wornBundlesFor(bundle, pool.byId)`
in place of a per-bundle `loadBundleLookup` read — so a claimed composite whose
parts live anywhere in the pool now expands with NO extra read and no
`[loadBundleLookup] Failed to load bundle components` WARN. A composite with an
archived part credits the archived leaf too (the lookup is `byId`).

### 1.4 `setAll(ctx, chatId, pool, slots, wornBundleIds)` (`:178-215`)

```ts
const allIds = Array.from(new Set(WARDROBE_SLOT_TYPES.flatMap((key) => slots[key])));
const found = pool.getMany(allIds);
if (found.length !== allIds.length) {
  const foundIds = new Set(found.map((i) => i.id));
  const missing = allIds.find((id) => !foundIds.has(id));
  return badRequest(`Wardrobe item ${missing} not available to this character`);   // no log (unchanged sentence)
}
const current = await repos.chats.getEquippedOutfitForCharacter(chatId, characterId);   // NEW read
const archived = newlyWornArchivedItems(found, current);
if (archived.length > 0) {
  logger.info('[Chats v1] Refused a fitting that puts on an archived item',
    { chatId, characterId, itemIds: archived.map((i) => i.id), context: 'wardrobe' });
  return badRequest(wearRefusal(archived[0])!);          // `Item "<title>" is archived and cannot be worn`
}
const wornBundles = resolveWornBundles(pool, wornBundleIds);
await repos.wardrobeWear.commitEquippedOutfit({ chatId, characterId, nextSlots: slots, wornBundles, source: 'ui' });
logger.info('[Chats v1] Equipped outfit replaced (set_all)', { chatId, characterId, wornBundleCount: wornBundles.length, context: 'wardrobe' });
return slots;
```
- `allIds` order: slot order, then array order, first-seen (same as before);
  the first missing in that order is the one named (same).
- The old `if (allIds.size > 0)` guard is gone — `pool.getMany([])` is `[]`
  and `0 === 0` passes; but the POOL is loaded for `set_all` regardless
  (`:246`), so an all-empty `set_all` now costs the four tier reads.
- **`newlyWornArchivedItems(found, current)`** (`wearable.ts:33-44`): the
  archived items among `found` whose id is NOT already in any slot of
  `current` (`currentSlots[slot] ?? []` over `WARDROBE_SLOT_TYPES`; `null`
  current → every archived item is "new"). An archived item already worn
  passes — "archiving never undresses anyone". `archived[0]` (the first in
  `found` order = `allIds` order) names the sentence.
- `commitEquippedOutfit` input and the INFO line: unchanged (v5
  `chat_outfits.rs:692-702`, `:843-849`).

### 1.5 `handleEquipSlot` (`:224-283`)

```ts
const { characterId, mode, slot, itemId, slots: bodySlots, wornBundleIds } = equipBodySchema.parse(body);
let updatedSlots: EquippedSlots;
if (mode === 'remove_from_slot' || mode === 'clear_slot') {
  updatedSlots = await applyDisplacement(repos, chatId, characterId, {
    mode, slot: slot as WardrobeItemType,
    itemId: mode === 'remove_from_slot' ? itemId ?? undefined : undefined,
  });                                                                    // source defaults 'ui' → commits as 'take-off'
  logger.info('[Chats v1] Wardrobe slot changed', { chatId, characterId, mode, slot, itemId: itemId ?? null, context: 'wardrobe' });
} else {
  const pool = await loadWearablePool(repos, characterId, undefined, { chatId, operator: true });
  if (mode === 'set_all') {
    const outcome = await setAll(ctx, chatId, pool, bodySlots!, wornBundleIds ?? []);
    if (outcome instanceof NextResponse) return outcome;
    updatedSlots = outcome;
  } else {
    const putOn: PutOnMode = mode === 'equip' ? 'wear' : mode;
    const resolved = resolveWearable(pool, { itemId }, putOn, slot);
    if (!resolved.ok) {
      logger.info('[Chats v1] Refused to put on a wardrobe item',
        { chatId, characterId, mode, itemId, reason: resolved.reason, context: 'wardrobe' });   // mode = RAW body mode ('equip' stays 'equip')
      return resolved.reason === 'not_found' ? notFound('Wardrobe item') : badRequest(resolved.message);
    }
    const outcome = await wearItem(repos, chatId, pool, resolved.item, putOn, slot, 'ui');
    updatedSlots = outcome.slots;
    logger.info('[Chats v1] Wardrobe item put on', { chatId, characterId, mode: putOn, itemId: resolved.item.id,
      slotsAffected: outcome.slotsAffected, effect: outcome.effect, context: 'wardrobe' });
  }
}
await notifyWardrobeChanged(repos, { userId: ctx.user.id, chatId, characterId }, '[Chats v1] outfit-equip');
return NextResponse.json({ equippedSlots: updatedSlots });
```
catch: `ZodError` → 400 `issues.map(e => e.message).join(', ')`; else ERROR
`[Chats v1] Error equipping wardrobe slot` `{chatId}` + 500 `Failed to equip
wardrobe slot` (unchanged; v5 `:857-864`).

**Per-mode contract at the pin:**

| mode | reads before the write | refusals (status · sentence · INFO line) | commit source | success line |
|---|---|---|---|---|
| `remove_from_slot` | `getEquippedOutfitForCharacter` ONLY (no chat read, no tiers, no pool) | none | `'take-off'`, `wornBundles: []` | INFO `Wardrobe slot changed` `{chatId, characterId, mode, slot, itemId: itemId ?? null, context}` |
| `clear_slot` | same | none | same | same line — NB `itemId` is the BODY's `itemId ?? null`, not forced null (a `clear_slot` body carrying an `itemId` logs it; the gesture ignores it) |
| `wear` / `equip` / `replace` | the pool (chat → project tier, operator-ungated; groups; own vault; General) | 404 `Wardrobe item not found` (reason `not_found`) · 400 `Item "<title>" is archived and cannot be worn` (`archived`) — each with INFO `Refused to put on a wardrobe item` `{chatId, characterId, mode (raw), itemId, reason, context}` | `'ui'`; `wornBundles` = `wornBundlesFor(item, pool.byId)` | INFO `Wardrobe item put on` `{chatId, characterId, mode: 'wear'|'replace', itemId, slotsAffected: [...item.types], effect: (mode==='replace' \|\| item.replace) ? 'replaced' : 'layered', context}` |
| `add_to_slot` | the pool | the two above, plus 400 `Item "<title>" (types: <types.join(', ')>) cannot be added to the "<slot>" slot` (reason `slot`) | `'ui'`; `wornBundlesFor(item, pool.byId, slot)` | INFO `Wardrobe item put on` `{…, mode: 'add_to_slot', slotsAffected: [slot], effect: 'layered'}` |
| `set_all` | the pool, then `getEquippedOutfitForCharacter` | 400 `Wardrobe item <id> not available to this character` (silent) · 400 `Item "<title>" is archived and cannot be worn` with INFO `Refused a fitting that puts on an archived item` `{chatId, characterId, itemIds, context}` | `'ui'`, the claimed bundles | INFO `Equipped outfit replaced (set_all)` `{chatId, characterId, wornBundleCount, context}` |

Then, every success path: `notifyWardrobeChanged` (§6) = the avatar trigger
(`callerContext: '[Chats v1] outfit-equip'`) then the announcement enqueue
with its own DEBUG / WARN lines.

**Subtle:** for `wear` / `replace` the body's `slot` (if any) is ignored by
`resolveWearable` but PASSED to `wearItem` → `applyDisplacement` `options.
slot` → it appears as the `slot` key of the `[applyDisplacement] Outfit change
committed` DEBUG (§4); `computeDisplacedSlots` ignores it for those modes.

### 1.6 GONE at the pin (never port; retire where v5 has them)

- INFO `[Chats v1] Wardrobe item worn` `{chatId, characterId, itemId,
  slotsAffected, effect, context}`; `… Wardrobe item force-replaced`; `…
  Wardrobe item layered into slot` `{chatId, characterId, slot, itemId,
  context}`; `… Wardrobe item removed from slot`; `… Wardrobe slot cleared`
  `{chatId, characterId, slot, context}`. **v5 never emitted any of these**
  (grep over `crates/` for `Wardrobe item worn` / `force-replaced` / `layered
  into slot` / `removed from slot` / `Wardrobe slot cleared`: none) — so the
  pin's `Wardrobe item put on` / `Wardrobe slot changed` / `Refused to put on
  a wardrobe item` / `Refused a fitting …` are NEW lines to add, not retitles.
- WARN `[Chats v1] Failed to schedule outfit announcement` `{chatId,
  characterId, error}` → replaced by `outfit-change-effects`' WARN `Failed to
  schedule wardrobe outfit announcement` `{context: '[Chats v1] outfit-equip',
  chatId, characterId, error}` (§6). v5 has NEITHER (`chat_outfits.rs:882` is
  a bare `let _ = enqueue…`).
- The 400 `Wardrobe item "<title>" does not cover the <slot> slot` → wear-ops'
  `Item "<title>" (types: …) cannot be added to the "<slot>" slot`. v5
  `chat_outfits.rs:778` has the OLD sentence; `tools/wardrobe_wear.rs:293`
  already has the new one (word-identical at the pin — the tools lane's).
- The `if (!updatedSlots) return serverError('Failed to update equipped
  slot')` guard (v5 never ported it — header `chat_outfits.rs:30-38`).
- The pre-mutation `resolveSharedWardrobeTiersForChat(chatId, characterId,
  {operator: true})` for ALL modes → the pool for put-on modes only;
  take-off modes read nothing but the slots. v5 `chat_outfits.rs:633-639`
  resolves tiers for every mode (incl. the chat read inside
  `resolve_project_tier_for_chat`, `wardrobe_tiers.rs:156`).

---

## 2. What this lane CONSUMES from the keystone (`lib/wardrobe/{pool,wear-ops,wearable,slot-ops,composite-types}.ts`) — named, with the exact surface used

Definitions are the keystone's; this is the contract the outfit pipeline
needs from it, verbatim where the words are wire- or log-visible.

### 2.1 `pool.ts` (362 lines, NEW)

- `loadWearablePool(repos, characterId, projectMountPointIds?, opts: { operator?, chatId?, sharedTiers?, ownItems? })` → `WearablePool`
  (`:255-322`). This lane's calls: `loadWearablePool(repos, characterId,
  undefined, { chatId, operator: true })` (the equip route);
  `loadWearablePool(repos, characterId, equippedProjectMountPointIds)`
  (chat-open whisper, `chats/route.ts:586`); `loadWearablePool(repos,
  characterId, projectMountPointIds ?? [])` (`describeEquippedOutfitTitleOnly`,
  `resolve-equipped.ts:153`); `loadWearablePool(repos, characterId,
  projectMountPointIds, { sharedTiers: sharedTiers() })`
  (`apply-outfit-selections.ts:250-252`).
- `loadCastPools(repos, projectId, characterIds)` → `Map<characterId, pool>`
  (`:350-363`): `resolveProjectMountPointIds(projectId ?? null)`, ONE
  `createSharedTierLoader(repos, projectMountPointIds)()` (read once, shared),
  `Promise.all(characterIds.map(id => loadWearablePool(repos, id,
  projectMountPointIds, { sharedTiers })))`. The outfit summary's reader.
- `WearablePool` (`:53-71`): `characterId`, `tiers: { groupMountPointIds,
  projectMountPointIds }`, `byId: ReadonlyMap` (every reachable item, ARCHIVED
  INCLUDED, shadowed general → project → group → own), `wearable()` (archived
  dropped PER TIER BEFORE shadowing; memoised), `get(id)`, `getMany(ids)`
  (found ids in the order asked, deduped, unknown skipped), `findByTitle`
  (own first incl. archived, then the wearable shared set), `owns(item)`.
- `componentGraph(pool, rootIds)` (`:330-341`) — the transitive component
  walk the resolver uses (`:80`).
- `buildWearablePool(characterId, tierMounts, { own, group, project, general })`
  (`:196-246`) — the pure builder the v4 route test uses in place of the
  loader; the v5 harness will want its twin to build pools from planted items.
- The tier precedence sentence: **character > group > project > general**;
  `shadow()` is Map-`set` over weakest-first tiers (a shadowed id KEEPS its
  first insertion position — the order of `wearable()` is general-first).
- Log lines the pool emits on this lane's paths (the capture pins must know
  them): WARN `[WearablePool] Failed to read shared wardrobe tiers;
  continuing without them` `{projectMountCount, context: 'wardrobe', error}`
  (`:133`); WARN `[WearablePool] Project lookup for chat failed` `{chatId,
  context, error}` (`:162`); DEBUG `[WearablePool] Character off project roster
  — project wardrobe withheld` `{chatId, projectId, characterId, context}`
  (`:172`); WARN `[WearablePool] Failed to read the character wardrobe; using
  shared tiers alone` `{characterId, context, error}` (`:272`); WARN
  `[WearablePool] Failed to read the group wardrobe tier; skipping it`
  `{characterId, context, error}` (`:290`); DEBUG `[WearablePool] Loaded
  wearable pool` `{characterId, ownCount, groupCount, projectCount,
  generalCount, reachable, groupMountCount, projectMountCount, context}`
  (`:311`). The old `[Wardrobe] Project lookup for chat failed` and
  `[Wardrobe] Character off project roster — project wardrobe withheld`
  (`shared-tiers.ts:93/102` at `3e6e2f590`; v5 `wardrobe_tiers.rs:176-181`)
  are retagged `[WearablePool]` and live in the pool.
- `resolveProjectTierForChat(repos, chatId, characterId, { operator })`
  (`:150-181`) — `chats.findById` → `projectId`; `operator ? projectId :
  rosterGatedProjectId(projectId, characterId)`; `resolveProjectMountPointIds`.

### 2.2 `wear-ops.ts` (132 lines, NEW)

- `type PutOnMode = 'wear' | 'replace' | 'add_to_slot'` (`:30`).
- `wardrobeItemNotFoundMessage(itemId, itemTitle)` (`:43-48`): `Wardrobe item
  not found` + `` with ID "<id>"`` when itemId + `` with title "<title>"`` when
  itemTitle (the route never shows it: `not_found` → `notFound('Wardrobe
  item')`).
- `findInPool(pool, ref)` (`:51-60`): `pool.get(itemId)` then
  `pool.findByTitle(itemTitle)`; archived FOUND.
- `resolveWearable(pool, ref, mode, slot?)` (`:66-86`) → `{ ok: true, item }` |
  `{ ok: false, reason: 'not_found' | 'archived' | 'slot', message }`:
  not found → `wardrobeItemNotFoundMessage`; `wearRefusal(item)` → `archived`;
  `mode === 'add_to_slot' && slot && !item.types.includes(slot)` → `slot`
  with `` `Item "${item.title}" (types: ${item.types.join(', ')}) cannot be added to the "${slot}" slot` ``.
- `wearItem(repos, chatId, pool, item, mode, slot, source)` (`:95-116`) →
  `applyDisplacement(repos, chatId, pool.characterId, { mode, item, slot,
  itemsById: pool.byId }, source)`; returns `{ slots, effect, slotsAffected }`:
  `add_to_slot` → `{ effect: 'layered', slotsAffected: slot ? [slot] : [] }`;
  else `effect = mode === 'replace' || item.replace ? 'replaced' : 'layered'`,
  `slotsAffected: [...item.types]`.
- `takeOffItem(repos, chatId, characterId, itemId, slots)` (`:122-132`) — one
  `applyDisplacement(remove_from_slot)` per slot (the tool's `remove`; not the
  route's).

### 2.3 `wearable.ts` (44 lines, NEW, client-safe)

`archivedWearMessage(title)` = `` `Item "${title}" is archived and cannot be worn` ``;
`wearRefusal(item)` = `item.archivedAt ? archivedWearMessage(item.title) : null`;
`newlyWornArchivedItems(items, currentSlots)` (§1.4).

### 2.4 `slot-ops.ts` (358 lines, NEW — the old `dissolve-bundles.ts` + the pure half of `outfit-displacement.ts`, renamed bundle → composite)

Used by this lane: `computeDisplacedSlots(current, { mode, item, slot,
itemId, itemsById })` (`:332-349`, the one mode → primitive table — `wear` →
`wearItemIntoSlots`, `replace` → `replaceItemIntoSlots`, `add_to_slot` →
`addItemToSlot`, `remove_from_slot` → `removeIdFromSlot(current, slot,
itemId)`, `clear_slot` → `removeIdFromSlot(current, slot)`);
`wornBundlesForMode(options)` (`:352-358`); `analyzeComposite(item, itemsById)`
→ `{ leaves | null, cycles, truncated, unresolved }` (`:88-113` — the server
logs from this, §4); `dissolveCompositeToLeaves` (`:121-126`),
`dissolveCompositesInSlots(current, itemsById, only?)` → `{ slots, wornBundles }`
(`:172-222`, replaces `dissolveBundlesInSlotsWithCredit`), `wornBundlesFor(item,
itemsById, onlySlot?)` (`:228-238`, moved from `outfit-displacement.ts`),
`layLeavesIntoSlots` (`:135-156`), `wearItemIntoSlots` / `addItemToSlot`
(`:245-262`, `:287-303`; the SPA's composer gestures import them —
`staged-live-outfits.ts:173-229`). **Pure and silent** — no logger (the
`[dissolveBundleToLeaves] …` WARNs are gone from here; §4 has their heirs).
`wardrobe.types.ts` gains the four pure helpers `isComposite`, `addIdToSlot`,
`removeIdFromSlot`, `equippedSlotsEqual` (`:382-421`) — `removeIdFromSlot`
with `itemId === undefined` clears; `addIdToSlot` is the no-op-if-present
append. The gesture MATH is unchanged from `3e6e2f590`; v5's
`wardrobe.rs:679-751` + `dissolve_bundles.rs` already compute it.

### 2.5 `composite-types.ts` — `buildCompositeTypes(components, designated = [])` (`:31-37`) NEW ("widen, never narrow"); `unionTypes` unchanged. Tools / routes lanes; not consumed here.

### 2.6 `wearable-pool.ts` — `mergeWearableTiers(tiersWeakestFirst)` NEW (`:27-35`, archived dropped per tier, shadow by `Map.set`); `mergeWearablePool(shared, own)` = `mergeWearableTiers([shared, own])` (`:38-40`). Client-safe; the server pool applies the same rule. v5 `wearable_pool.rs:45` `merge_wearable_pool` is the old two-list form.

---

## 3. `lib/wardrobe/outfit-displacement.ts` (115 lines; −430 / +115) — `applyDisplacement`

The ONLY export besides `DisplacementRepos` (`:37-45` — `chats.
getEquippedOutfitForCharacter` + `wardrobeWear.commitEquippedOutfit`; the
optional `wardrobe` member for component hydration is GONE):

```ts
export async function applyDisplacement(repos, chatId, characterId, options: ComputeDisplacedOptions,
  source: Extract<EquipSource, 'ui' | 'tool'> = 'ui'): Promise<EquippedSlots> {
  const current = await loadSlots(repos, chatId, characterId);        // clone or makeEmptyEquippedSlots()
  logCompositeIssues(options, characterId);
  const nextSlots = computeDisplacedSlots(current, options);
  const takeOff = options.mode === 'remove_from_slot' || options.mode === 'clear_slot';
  await repos.wardrobeWear.commitEquippedOutfit({ chatId, characterId, nextSlots,
    wornBundles: wornBundlesForMode(options), source: takeOff ? 'take-off' : source });
  logger.debug('[applyDisplacement] Outfit change committed', { context: 'wardrobe', chatId, characterId,
    mode: options.mode, itemId: options.item?.id ?? options.itemId ?? null, slot: options.slot ?? null,
    source: takeOff ? 'take-off' : source });
  return nextSlots;
}
```
`logCompositeIssues` (`:57-77`) — only when `options.item && options.itemsById`;
`analyzeComposite`: `cycles > 0 || truncated` → WARN `[applyDisplacement]
Malformed composite graph; expansion truncated` `{context: 'wardrobe',
characterId, itemId, cycles, truncated}`; `unresolved` → WARN
`[applyDisplacement] Composite resolved to no wearable parts; wearing it whole`
`{context, characterId, itemId, componentCount: item.componentItemIds?.length
?? 0}`. **ONCE per gesture** (the old `[dissolveBundleToLeaves] Malformed
bundle graph; expansion truncated` `{context, itemId, cycles, truncated}` and
`[dissolveBundleToLeaves] Bundle resolved to no wearable parts; wearing it
whole` `{context, itemId, componentCount}` fired on EVERY `dissolveBundleToLeaves`
call — i.e. twice per gesture, from the compute AND from `wornBundlesFor`;
they had no `characterId`). The `[expandComposites] Max depth reached, treating
as leaf` WARN is GONE (`expand-composites.ts` is pure now; v5
`wardrobe.rs:431` has no log line either — nothing to retire).

GONE exports: `equipItem`, `replaceItem`, `addToSlot` (which THREW
`` `Item ${id} (types=[…]) cannot occupy slot '${slot}'` `` on a slot miss — the
route guarded first), `removeFromSlot`, `wornBundlesFor` (→ slot-ops),
`wearItemIntoSlots` / `replaceItemIntoSlots` / `addItemToSlot` /
`computeDisplacedSlots` / `DisplacementMode` / `ComputeDisplacedOptions` (→
slot-ops), `EquipOptions`, `PutOnSource`, `WornBundle` (→ slot-ops),
`lookupForBundle` (→ the pool's `byId`), the private `commit` (its comment
"from the job child the result is synthetic" survives in the doc comment).

**The source rule is unchanged**: take-off modes ALWAYS `'take-off'`
(`wornBundles: []` falls out of `wornBundlesForMode` — no `item`); put-on modes
the caller's `'ui' | 'tool'`. The commit chokepoint input shape is unchanged.

v5 today: `tools/wardrobe_shared.rs:364-461` `equip_item` / `replace_item` /
`add_to_slot` / `remove_from_slot` (each: `load_current_wardrobe_state` →
`lookup_for_bundle` [`:294-330`, a `find_by_ids_for_character` read + `hydrate_
component_graph` per gesture, WARN `[loadBundleLookup] Failed to load bundle
components`] → the `wardrobe.rs` math → `commit` `:465-485` → `commit_
equipped_outfit`), with the per-call WARNs in `dissolve_bundles.rs:146-162`
(no `context` / `characterId` keys). The pin's shape: ONE `apply_displacement
(main, chat_id, character_id, options, source)` taking `items_by_id: Option<&
WearableLookup>` = the pool's `by_id` (NO per-gesture read), `log_composite_
issues` once, the `[applyDisplacement] Outfit change committed` DEBUG, the
chokepoint unchanged. `tools/wardrobe_wear.rs:297-340` and `wardrobe_take_off.
rs:174/294` call the same primitives — the tools lane consumes the new
`apply_displacement` (meeting point §13).

---

## 4. `lib/wardrobe/resolve-equipped.ts` (160 lines; −139 / +… over the span) — the equipped resolver

```ts
export function resolveEquippedOutfitForCharacter(pool: WearablePool, slots: EquippedSlots): ResolvedEquippedOutfit
```
(`:70-138`; was `async (repos, characterId, slots, opts?: SharedWardrobeTiers)`).
- `equippedItemIds = allEquippedItemIds(slots)`; empty → `emptyResolved()`.
- `itemsById = componentGraph(pool, equippedItemIds)` (the pool's transitive
  walk — composites' parts from ANY tier the character reaches, archived
  included). `unresolved = equippedItemIds.filter(id => !itemsById.has(id))`;
  non-empty → DEBUG `[resolveEquippedOutfitForCharacter] Equipped ids not in
  the wearable pool` `{context: 'wardrobe', characterId: pool.characterId,
  unresolvedCount}` (NEW line).
- Per slot in `WARDROBE_SLOT_TYPES`: `expanded = expandComposites(slots[slot]
  ?? [], itemsById)`; `expanded.cycles.length > 0 || expanded.truncated` →
  WARN `[resolveEquippedOutfitForCharacter] Malformed composite graph;
  expansion truncated` `{context, characterId, slot, cycles, truncated}` (NEW
  line — replaces the `[expandComposites]` WARN that fired inside the walk);
  leaves deduped in first-seen order into `orderedLeaves` (an id with no item
  skipped).
- Routing (`:126-135`, UNCHANGED since `d25dacc1`): for each ordered leaf, for
  each `slot of item.types` that is a known slot, `leafItemsBySlot[slot].push
  (item)`, `outfitValues[slot].push(item.title)`.
- Returns `{ outfitValues, leafItemsBySlot, itemsById }`.
- GONE: WARN `[resolveEquippedOutfitForCharacter] findByCharacterId failed;
  proceeding with findByIds only` `{context, characterId, error}`; WARN
  `[resolveEquippedOutfitForCharacter] findByIds fallback failed` `{context,
  characterId, missingCount, error}`; `hydrateComponentGraph` + WARN
  `[hydrateComponentGraph] Component hydration failed`; the
  `ResolveEquippedRepos` / `ResolveEquippedOptions` types.
- `describeEquippedOutfitTitleOnly(repos, characterId, equippedSlots,
  projectMountPointIds)` (`:147-160`): `loadWearablePool(repos, characterId,
  projectMountPointIds ?? [])` then the resolver, then `describeOutfit(
  buildOutfitSlotValues(slot => decorateOutfitItems(leafItemsBySlot[slot],
  { titleOnly: true })))` — unchanged text; the group tier is the pool's.

**Consumers at the pin (the sweep the signature change forces):**
`outfit.ts:135` (summary), `apply-outfit-selections.ts:270` (the progress
`wardrobeResult`), `lib/tools/handlers/wardrobe-handler-shared.ts:244`
(`describeOutfit(resolveEquippedOutfitForCharacter(pool, slots).outfitValues)`),
`lib/background-jobs/handlers/wardrobe-announcement.ts:72`,
`lib/image-gen/appearance-resolution.ts:108`, `lib/image-gen/avatar-prompt.ts:166`
(both via a pool), and `describeEquippedOutfitTitleOnly` from
`lib/background-jobs/handlers/scene-state-tracking.ts:196` +
`lib/chat/context-manager.ts:1717`.

v5 today: `tools/wardrobe_shared.rs:575-645` `resolve_equipped_leaves` (the OLD
head: `find_by_character_id(…, true)` → `find_by_ids_for_character` fallback for
missing ids → `hydrate_component_graph`, each failure swallowed WITHOUT the
old WARNs) feeding `resolve_equipped_outfit_leaf_values` (`:652`) and
`resolve_equipped_leaf_items_by_slot` / `resolve_equipped_outfit_values`.
Callers: `api/chat_outfits.rs` (NOT — the summary re-implements the old walk
inline, §1.2), `services/outfit_selections.rs:538`, `services/aurora_
notifications.rs:347-360` (the announcement job: `shared_wardrobe_tiers_for_
character` + `resolve_equipped_outfit_values`), `services/avatar_prompt.rs:100`,
`services/build_context.rs:1317`, `services/story_background_job.rs:1383`,
`tools/generate_image.rs:1710`, `services/wardrobe_item_image_generation.rs:
206-212` (hydrates the component graph directly), the tools' describe path.
The pin's head is `component_graph(pool, ids)` over a `WearablePool`; every
caller builds or receives a pool first.

---

## 5. `lib/wardrobe/outfit-change-effects.ts` (125 lines, NEW)

```ts
export interface OutfitChangeContext { userId; chatId; characterId; pendingWardrobeAnnouncements?: Set<string> }
export async function refreshAvatarForOutfit(repos, context: Pick<…,'userId'|'chatId'|'characterId'>, sourceContext: string)
  → triggerAvatarGenerationIfEnabled(repos, { userId, chatId, characterId, callerContext: sourceContext })   // :41-52, never throws
export async function scheduleWardrobeAnnouncement(sourceContext, { userId, chatId, characterId })            // :55-77
  try { await enqueueWardrobeOutfitAnnouncement(userId, { chatId, characterId });
        logger.debug('Wardrobe outfit announcement scheduled', { context: sourceContext, chatId, characterId }); }
  catch (error) { logger.warn('Failed to schedule wardrobe outfit announcement', { context: sourceContext, chatId, characterId, error: message }); }
export async function recordPendingWardrobeAnnouncement(context, sourceContext)                                // :83-92
  → with a Set: add(characterId); without: scheduleWardrobeAnnouncement(sourceContext, context)
export async function notifyWardrobeChanged(repos, context, sourceContext)                                     // :98-105
  → await refreshAvatarForOutfit(…); await recordPendingWardrobeAnnouncement(…)        // avatar FIRST, then the announcement
export async function flushPendingWardrobeAnnouncements(context)                                               // :111-125
  → drain the Set (clear first), one scheduleWardrobeAnnouncement('orchestrator-turn-end', …) per character
```
`scheduleWardrobeAnnouncement` / `recordPending…` / `flushPending…` MOVED here
from `lib/tools/handlers/wardrobe-handler-shared.ts` (the orchestrator's
import, `orchestrator.service.ts:98`, is the only chat-spine hunk of this
family); `refreshAvatarForOutfit` and `notifyWardrobeChanged` are NEW names
over the two existing effects. The route calls `notifyWardrobeChanged(repos,
{ userId, chatId, characterId }, '[Chats v1] outfit-equip')` — no Set, so the
announcement is enqueued at once (the job itself is debounced) with DEBUG
`Wardrobe outfit announcement scheduled` `{context: '[Chats v1] outfit-equip',
chatId, characterId}`. The other `notifyWardrobeChanged` callers are the tools
(`wardrobe-handler-shared.ts:214`, `wardrobe-create-handler.ts:261` — bug 193,
`wardrobe-archive-handler.ts:102`) — the tools lane's.

v5 today: `api/chat_outfits.rs:867-882` does the two legs inline (the trigger,
then `let _ = enqueue_wardrobe_outfit_announcement(…)` with NO debug / warn
line); the flush lives in `services/aurora_notifications.rs:265-282` (sorted
drain, `let _ =` per character); the tools' shared trigger loop is
`services/avatar_generation.rs:222-244` `trigger_for_characters`. The pin
wants ONE `outfit_change_effects` home (`refresh_avatar_for_outfit`,
`schedule_wardrobe_announcement` with its two lines, `record_pending…`,
`notify_wardrobe_changed`, `flush_pending…`) that the route, the tools and the
orchestrator share — the module is this lane's; the tools lane consumes it.
(grep: neither `Wardrobe outfit announcement scheduled` nor `Failed to schedule
wardrobe outfit announcement` exists anywhere in `crates/` today.)

---

## 6. `lib/wardrobe/avatar-generation.ts` + `lib/image-gen/profile-resolution.ts` `resolveAvatarImageProfile` + the toggle sweep

### 6.1 `triggerAvatarGeneration(repos, params)` (`avatar-generation.ts:52-117`)

```ts
const chat = await repos.chats.findById(chatId);
if (!chat) return { queued: false, reason: 'chat-not-found', message: 'Chat not found.' };
// Override → chat-level → the user's default, each checked for an API key.
const profile = await resolveAvatarImageProfile(userId, repos, { override: imageProfileIdOverride, chat });
const imageProfileId = profile?.id ?? null;
logger.debug('Avatar image profile resolved', { context: callerContext, chatId, characterId, imageProfileId,
  overrideRequested: !!imageProfileIdOverride,
  overrideHonoured: !!imageProfileIdOverride && imageProfileId === imageProfileIdOverride });
if (!imageProfileId) return { queued: false, reason: 'no-image-profile',
  message: 'No image profile is configured. Set one in Settings → Images before generating avatars.' };
await enqueueCharacterAvatarGeneration(userId, { chatId, characterId, imageProfileId,
  ...(equippedSlotsOverride ? { equippedSlotsOverride } : {}), ...(force ? { force: true } : {}) });
return { queued: true };
```
catch → WARN `Failed to enqueue avatar generation` `{context, chatId,
characterId, error}` + `{ queued: false, reason: 'error', message }`
(unchanged). **GONE:** WARN `Avatar generation override profile not found,
falling back` `{context, chatId, imageProfileIdOverride}`; the chat-profile
lookup with NO key check; `repos.imageProfiles.findAll().find(p => p.isDefault)`.
**NEW:** the DEBUG `Avatar image profile resolved` line (every call, success
or not). `triggerAvatarGenerationIfEnabled` (`:125-149`) unchanged
(`!chat?.avatarGenerationEnabled` → return; `chatType === 'autonomous'` →
return; WARN `Failed to enqueue avatar generation after outfit change` — v5
records it unreachable, `avatar_generation.rs:191-197`).

### 6.2 `resolveAvatarImageProfile(userId, repos, { override?, chat? })` (`profile-resolution.ts:173-210`, NEW)

```ts
const usable = (profile) => !!profile && profile.userId === userId && !!profile.apiKeyId;
if (options.override) { const profile = await repos.imageProfiles.findById(options.override); if (usable(profile)) return profile;
  logger.debug('[ProfileResolution] Avatar override profile unusable; falling back', { context: 'image-gen.profile-resolution', override: options.override, found: !!profile }); }
const chatProfileId = options.chat?.imageProfileId;
if (chatProfileId) { const profile = await repos.imageProfiles.findById(chatProfileId); if (usable(profile)) return profile;
  logger.debug('[ProfileResolution] Chat image profile unusable for avatars; falling back', { context: LOG_CONTEXT, chatProfileId, found: !!profile }); }
const fallback = await repos.imageProfiles.findDefault(userId);
if (fallback && fallback.apiKeyId) return fallback;
logger.debug('[ProfileResolution] No usable avatar image profile', { context: LOG_CONTEXT, userId });
return null;
```
`findDefault(userId)` (`image-profiles.repository.ts:39-47`) =
`safeQuery(() => findOneByFilter({ userId, isDefault: true }), 'Error finding
default image profile', { userId })` — a FALLBACK read (ERROR `Error finding
default image profile` `{collection: 'image_profiles', userId, error}` → null)
in place of `findAll()` (ERROR `Error finding all entities`). Three checks per
candidate: exists, `userId` matches, `apiKeyId` truthy — a keyless chat profile
is PASSED OVER to the default (before: taken, and the job skipped for want of a
key). The default needs only the key (`findDefault` already scopes by user).

v5 today: `services/avatar_generation.rs:113-163` (override → WARN `Avatar
generation override profile not found, falling back` `{context, chat_id,
image_profile_id_override}` → chat profile with NO key / owner check →
`find_all` + `isDefault`), under the P4.123 fallback homes (`find_by_id_or_
none("image_profiles")`, `find_all_or_empty`). `db/image_profiles.rs` has NO
`find_default` (`:259 find_by_user_id`, `:346 find_by_id`, `:380 find_all`).
The existing resolver home is `services/image_profile_resolution.rs`
(`resolve_image_profile_for_chat` `:19-`, `resolve_wardrobe_image_profile`
`:129-` — whose default leg is `find_all_or_empty … isDefault && userId ==
user_id` `:164-170`, i.e. a hand-rolled `findDefault`) — `resolve_avatar_
image_profile(main, user_id, override, chat) -> Option<Value>` belongs beside
them, with a real `image_profiles::find_default(conn, user_id)` + fallback
form (`find_default_or_none`) so the ERROR text is v4's. The wardrobe resolver's
default leg should move onto the same `find_default` (consistency; the
item-images lane's file — coordinate, §13).

### 6.3 `toggle-avatar-generation.ts:51-94` (the toggle-ON sweep)

```ts
const imageProfileId = (await resolveAvatarImageProfile(user.id, repos, { chat }))?.id ?? null;   // chat = the PRE-toggle row read at :28
if (!imageProfileId) {
  logger.debug('[Chats v1] No usable image profile; skipping initial avatar generation', { chatId, context: 'avatar-generation' });
} else { … for each CHARACTER participant with characterId && controlledBy !== 'user': enqueueCharacterAvatarGeneration(user.id, { chatId, characterId, imageProfileId }) … }
```
The rest (INFO `[Chats v1] Avatar generation toggled` `{chatId,
avatarGenerationEnabled, context: 'avatar-generation'}`, the per-character WARN
`[Chats v1] Failed to enqueue avatar generation for character`, the outer WARN
`[Chats v1] Failed to enqueue initial avatar generation after toggle-on`,
`successResponse({ avatarGenerationEnabled })`) is unchanged. v5 `services/
chat_avatars.rs:314-333` resolves chat profile (no key check) → `find_all`
isDefault, no DEBUG line. `regenerate-avatar.ts` is UNCHANGED in the span (its
`triggerAvatarGeneration` call inherits the new resolution); v5 `chat_outfits.
rs:974-1038`.

### 6.4 `wardrobe/preview-avatar/route.ts:77` also calls `resolveAvatarImageProfile(user.id, repos, {…})` — the item-images / preview lane's (400 `No image profile is configured. Set one in Settings → Images before generating avatars.`); it shares the resolver this lane adds (§13).

---

## 7. Chat-open and participant-join — the two refresh sites

### 7.1 `app/api/v1/chats/route.ts:563-617` (`createInitialMessagesScenarioAndStaff`, the Aurora opening whisper)

```ts
const equippedProjectMountPointIds = options.projectMountPointIds;   // string[] (:464), resolved once per create (:1353)
for (const participant of allCharacterParticipants) { try {
  const character = await repos.characters.findById(characterId); if (!character) continue;
  const equippedSlots = await repos.chats.getEquippedOutfitForCharacter(chatId, characterId); if (!equippedSlots) continue;
  const equippedItemIds = allEquippedItemIds(equippedSlots).filter((id) => typeof id === 'string' && id.length > 0);
  const pool = equippedItemIds.length > 0 ? await loadWearablePool(repos, characterId, equippedProjectMountPointIds) : null;
  const equippedItemsMap = new Map(pool ? pool.getMany(equippedItemIds).map((item) => [item.id, item]) : []);
  const titlesFor = (slot) => (equippedSlots[slot] ?? []).map(id => equippedItemsMap.get(id)?.title).filter(Boolean);   // RAW ids, no expansion (unchanged)
  const outfit = buildOutfitSlotValues((slot) => titlesFor(slot));
  await postOpeningOutfitWhisper({ chatId, characterName: character.name, outfit });
  await refreshAvatarForOutfit(repos, { userId, chatId, characterId }, '[Chats v1] chat-open');
} catch (error) { logger.warn('[Chats v1] Failed to post opening outfit whisper', { chatId, characterId, error }); } }
```
Deltas: `findByIdsForCharacter(characterId, ids, sharedWardrobeTiersForCharacter
(characterId, projectMountPointIds))` → `loadWearablePool(repos, characterId,
projectMountPointIds).getMany(ids)` — titles identical in every case the old
read covered (own incl. archived first, then the shared tiers incl. archived;
the pool shadows the same way), but FOUR tier reads now happen per character
(+ the `[WearablePool] Loaded wearable pool` DEBUG per character) where one
vault read (+ an archetype read only on a miss) happened before; the project
tier is the create's own list (not roster-gated — same). `triggerAvatar
GenerationIfEnabled({…, callerContext: '[Chats v1] chat-open'})` →
`refreshAvatarForOutfit(…, '[Chats v1] chat-open')` — identical effect.

v5: `services/chat_create.rs:2214-2239` + `post_opening_outfit_and_avatar`
`:2336-2422` (`find_by_ids_for_character` over `shared_wardrobe_tiers_for_
character(main, mount, character_id, equipped_project_mount_point_ids)`
`:2363-2377`; `trigger_avatar_generation_if_enabled` with `"[Chats v1]
chat-open"` `:2404-2420`). The create path's `applyOutfitSelections` call
(`route.ts:1380/1392`) is §9.

### 7.2 `app/api/v1/chats/[id]/actions/participants.ts:252-280` (`refreshAvatarForArrivingCharacter`)

Only the call moved: `refreshAvatarForOutfit(repos, { userId, chatId,
characterId }, '[Chats v1] participant-join')` (`:268`); the surrounding
DEBUG `[Chats v1] No equipped outfit for arriving character, avatar left as-is`
`{chatId, characterId}`, DEBUG `[Chats v1] Avatar refresh requested for
arriving character` `{chatId, characterId}`, WARN `[Chats v1] Failed to request
avatar refresh for arriving character` are unchanged. `applyOutfitForAddedParticipant`
(`:196-238`) unchanged (its `applyOutfitSelections` body is §9). v5 `api/chat_
cast.rs:170-215` (`"[Chats v1] participant-join"` `:198`) — no behaviour change;
a rename onto `refresh_avatar_for_outfit` only.

---

## 8. `lib/wardrobe/apply-outfit-selections.ts` (540 lines; −130/+…) — the pool swap

- `:247-258`: the three private loaders (`getSharedPool` via `findArchetypes
  (false, { projectMountPointIds })`, `getGroupTier` via `sharedWardrobeTiers
  ForCharacter` + `findArchetypesInMounts`, the merged `getPool` with
  `mergeWearablePool(shared, own)`) collapse to `const sharedTiers =
  createSharedTierLoader(repos, projectMountPointIds)` + `getPool(characterId)
  = loadWearablePool(repos, characterId, projectMountPointIds, { sharedTiers:
  sharedTiers() })`, memoised per character. **GONE lines:** WARN
  `[applyOutfitSelections] Failed to read shared wardrobe tiers; using the
  character vault alone` `{chatId, projectMountCount, error}`, WARN
  `[applyOutfitSelections] Failed to read group wardrobe tier; skipping it`
  `{chatId, characterId, error}`, WARN `[applyOutfitSelections] Failed to read
  character wardrobe; using shared tiers alone` `{chatId, characterId, error}`,
  DEBUG `[applyOutfitSelections] Resolved wearable pool` `{chatId, characterId,
  poolSize, sharedCount, groupCount, ownCount, projectMountCount}` → the pool's
  `[WearablePool]` lines (§2.1). v5 `services/outfit_selections.rs:617-703`
  has all four verbatim.
- `default` / `manual`-fallback / `llm_choose`-fallback: `buildDefaultOutfit
  WithCredit((await getPool(characterId)).wearable())` (`:301`, `:338`, `:483`)
  — the candidate set is `wearable()` (archived dropped per tier before
  shadowing). Equivalent SET to the old merge (both inputs were already
  non-archived reads); ORDER = general → project → group → own by first
  insertion — verify against v5 `merge_wearable_pool`'s order (it feeds the
  stable `sortForDefaultOutfit` tie order and the `llm_choose` candidate list,
  which is model-visible).
- `manualWornBundles(wornBundleIds, pool: WearablePool)` (`:178-192`): `bundle
  = pool.wearable().find(item => item.id === id)` (archived bundles NOT
  creditable — unchanged), `dissolveCompositeToLeaves(bundle, pool.byId)` (the
  lookup is now archived-INCLUSIVE: an archived PART now appears in the credit's
  `leafIds`; before it was absent from the lookup and the leaf was skipped).
  v5 `outfit_selections.rs:234-260` uses the wearable pool as the lookup.
- `llm_choose` (`:355-449`): `pool.wearable()` as the candidate list;
  `resolveWardrobeInstructions({ characterMountPointId, groupMountPointIds:
  pool.tiers.groupMountPointIds, projectMountPointIds })` (was a second
  `sharedWardrobeTiersForCharacter(characterId, [])` read); `chosen =
  dissolveCompositesInSlots(result.result.slots, pool.byId)` (was over the
  wearable list — **a picked composite with an archived part now dissolves the
  archived part INTO the slots**, where before the part was unresolvable: if it
  was the composite's only part the composite was stored whole, otherwise the
  part was dropped). v5 `:1396` dissolves over `wardrobe_items` (wearable).
- The progress `wardrobeResult` (`:267-275`): `resolveEquippedOutfitForCharacter
  (await getPool(characterId), slots)` — forces the pool read even for `manual`
  / `none` selections when `context.progress` is set (before: a separate
  vault + `findByIds` read). v5 `:538 resolve_equipped_outfit_leaf_values`.
- `toOutfitPreviewSlots` `isComposite: isComposite(i)` (same predicate);
  `OUTFIT_LLM_TIMEOUT_MS` un-exported (`:102`).
- Smalls elsewhere: `default-outfit.ts` → `dissolveCompositesInSlots` (same
  math; `sortForDefaultOutfit` un-exported — v5 `wardrobe.rs:381` is `pub`,
  harmless); `expand-composites.ts` loses its logger (§3); `composed-outfits.ts`
  / `group-equipped.ts` → `isComposite` (same predicate); `outfit-description.ts`
  un-exports `OutfitSlotName` (v5 `wardrobe.rs:271` keeps it `pub`, harmless);
  `staged-live-outfits.ts` is SPA-only (the six composer gestures `:173-229`,
  `equippedSlotsEqual` moved to `wardrobe.types.ts`, `rebaseStagedSlots`
  folded into `rebaseStagedGestures`) — the SPA lane's.

---

## 9. v4 jest coverage at the pin (names + case counts)

| file | it / describe | span | covers |
|---|---|---|---|
| `__tests__/unit/app/api/v1/chats/[id]/actions/outfit.test.ts` | 19 / 4 | REWRITTEN (+446/−…) | `pool lookup` (6: equip vault item; equip a General item; 404 not in pool; `add_to_slot` slot refusal; `remove_from_slot` WITHOUT loading a pool; `clear_slot`), `wear ledger` (3 + 3: set_all forwards reachable bundles expanded; set_all without claims; set_all refuses an unreachable id; wearing a bundle claims it; set_all failed save → 500 + no announcement; take-off committed as `'take-off'`), `archived items (bug 191)` (3: add_to_slot refuses archived; set_all refuses newly-worn archived; set_all lets an already-worn archived item stay), `outfit-summary` (3: own pool + own-types routing [a legacy composite in `top` spreads; a `top+bottom` dress in `top` lands in both]; drops an id outside the character's pool; 404 missing chat + `loadCastPools` not called). Mocks `loadWearablePool` / `loadCastPools` at the seam, builds REAL pools with `buildWearablePool`; mocks `avatar-generation` + `queue-service`. |
| `__tests__/unit/lib/wardrobe/wear-ops.test.ts` | 12 / 4 | NEW | `findInPool` (3), `resolveWearable` (4 — incl. the exact not-found and slot sentences), `wearItem` (3 — commits through `applyDisplacement` with `pool.byId`; `replaced` for the flag OR the mode; `add_to_slot` layers one slot), `takeOffItem` (1) |
| `__tests__/unit/lib/wardrobe/resolve-equipped.test.ts` | 14 / 1 | REWRITTEN (+279/−…) | the routing rule (spread a multi-slot leaf; no double count; composite parts to their own slots; layered on top), shared-tier / group-store / nested composites, unknown ids skipped, a cycle, a legacy four-key bag, a garment archived while worn |
| `__tests__/unit/lib/wardrobe/outfit-change-effects.test.ts` | 10 / 4 | NEW | the WARN on enqueue failure; `recordPending…` (4); `notifyWardrobeChanged` (2 — avatar + Set; immediate enqueue without a Set); `flush…` (3) |
| `__tests__/unit/lib/wardrobe/avatar-generation.test.ts` | 4 / 1 | mocks `findAll` → `findDefault`; profiles carry `userId` + `apiKeyId` | skip when disabled; chat profile; default fallback; swallow + WARN |
| `__tests__/unit/lib/image-gen/profile-resolution.test.ts` | 16 / 3 | +42 | `resolveAvatarImageProfile` (3: override → chat → default; passes over keyless / foreign / missing; null when the default has no key) |
| `__tests__/unit/api/toggle-avatar-generation.test.ts` | 3 / 1 | `findAll` → `findDefault` mock | on + LLM-only enqueue; off; "still succeeds when no usable image profile is available" |
| `__tests__/unit/app/api/v1/chats/[id]/actions/participants-join-avatar.test.ts` | 4 / 1 | 2-line hunk (the mock target) | add → refresh; reactivate → refresh; nothing equipped → left alone; a throwing refresh never fails the join |
| `__tests__/unit/lib/wardrobe/slot-ops.test.ts` (← `dissolve-bundles.test.ts`) | 37 / 10 | renamed + 198 lines | keystone's |
| `__tests__/unit/lib/wardrobe/pool.test.ts` | 35 / 9 | NEW (+495) | keystone's (`wardrobe.repository.pool.test.ts` −257 DELETED) |
| `__tests__/unit/lib/wardrobe/apply-outfit-selections.{tiers,ledger,concurrency}.test.ts` + `lib/wardrobe/__tests__/apply-outfit-selections.progress.test.ts` | 13 / 7 / 11 / 4 | tiers +291; the progress test now mocks `tiered-mount-pool` + `instance-settings`, `readSharedTiers`, gives the Jacket `types: ['top']` | the pool swap |
| `__tests__/unit/lib/wardrobe/{default-outfit,expand-composites,composed-outfits,group-equipped,outfit-hash,outfit-utils,staged-live-outfits}.test.ts` | 8 / 15 / 7 / 10 / 7 / 43 / 10 | `outfit-utils` +178, `staged-live-outfits` +17 | smalls |
| `__tests__/unit/lib/wardrobe/equip-chokepoint-fence.test.ts` | 2 / 1 | unchanged | the fence — two sanctioned `setEquippedOutfit(` files |

---

## 10. v5 today — the files this lane touches, with what changes

| v5 file | lines | today | at the pin |
|---|---|---|---|
| `crates/quilltap-core/src/api/chat_outfits.rs` | 1104 | summary `:142-297` (old cast-wide pool + per-input-slot filter); equip `:619-885` (tiers for every mode `:633-639`; `set_all` `:643-707` via `find_by_ids_for_character` + `resolve_worn_bundles` `:561-598` with per-bundle `lookup_for_bundle`; `wear/equip/replace` `:708-757` via `find_by_id_for_character` + `equip_item`/`replace_item`; `add_to_slot` `:758-799` with the OLD sentence `:778`; take-off `:800-817`; the two post legs inline `:867-882`); parse `:406-543` unchanged | summary over `load_cast_pools` + the pool resolver + the DEBUG; equip: take-off modes straight to `apply_displacement` (no tiers, no pool) + INFO `Wardrobe slot changed`; put-on modes over ONE `load_wearable_pool(…, chat_id, operator: true)` → `resolve_wearable` → `wear_item` + the three INFO lines; `set_all` over `pool.get_many` + `newly_worn_archived_items` + the INFO; `notify_wardrobe_changed`. Header `:11-55` rewritten (the "seven modes" + "set_all" sections stay; the "pool" rule joins). |
| `tools/wardrobe_shared.rs` | 786 | `resolve_wardrobe_item_across_tiers` `:100-128`; `hydrate_component_graph` `:2xx-284` + `lookup_for_bundle` `:294-330`; `worn_bundles_for` `:338-`; `equip_item` / `replace_item` / `add_to_slot` / `remove_from_slot` / `commit` `:364-485`; the resolver `resolve_equipped_leaves` `:575-645` + `resolve_equipped_outfit_leaf_values` `:652-` | the four primitives → ONE `apply_displacement(main, chat_id, character_id, &DisplacementOptions, source)` over `Option<&WearableLookup>` (= the pool's `by_id`), `log_composite_issues` + the committed DEBUG; the resolver's head → `component_graph(pool, ids)` with the two new lines; `hydrate_component_graph` / `lookup_for_bundle` / `resolve_wardrobe_item_across_tiers` retire once every caller is on the pool (the tools lane owns the title-match callers — `find_in_pool` is the keystone's). Whether `apply_displacement` / the resolver live in a NEW `services/outfit_displacement.rs` + `services/resolve_equipped.rs` (v4's files) or stay in `wardrobe_shared.rs` is the order's call — v4's file split favours new modules. |
| `dissolve_bundles.rs` | — | the two per-call WARNs `:146-162` (`[dissolveBundleToLeaves]`, no `context` / `characterId`) | silent `analyze_composite` returning `{leaves, cycles, truncated, unresolved}`; the WARNs move to `apply_displacement` as `[applyDisplacement] …` with `context` + `characterId`, once per gesture. (Renaming the module `slot_ops.rs` to follow v4 is optional; the keystone may own the rename — §13.) |
| `wardrobe.rs` `:679-751` | — | `wear_item_into_slots(current, node, replace, lookup)` / `replace_item_into_slots` / `add_item_to_slot` — the math | unchanged math; a `compute_displaced_slots(current, &options)` + `worn_bundles_for_mode` table (keystone's `slot-ops`, or this lane if the keystone leaves it) |
| `wardrobe_tiers.rs` | 397 | `resolve_shared_wardrobe_tiers_for_chat` `:110-131` + `resolve_project_tier_for_chat` `:146-185` (the `[Wardrobe] Character off project roster` DEBUG) + `shared_wardrobe_tiers_for_character` `:193-` | v4 deleted `shared-tiers.ts`; the pool's `resolveProjectTierForChat` (keystone) carries the roster gate with the `[WearablePool]` tags. This lane's callers stop calling it; retirement is the keystone's / last caller's. |
| `wearable_pool.rs` | 172 | `merge_wearable_pool(shared, own)` `:45` (old two-list form) | `merge_wearable_tiers(tiers_weakest_first)` + the thin wrapper — keystone's. |
| `db/wardrobe_read.rs` | — | `find_by_ids_for_character` `:141-185`, `find_by_id_for_character` `:191-206`, `find_wearable_pool_for_character` `:124-133` | the pool replaces all three at this lane's sites; `find_by_character_id(…, true)` + `read_shared_tiers` remain the pool's reads (keystone). |
| `services/avatar_generation.rs` | 244 | `:113-163` override WARN + chat (no key check) + `find_all` isDefault | `resolve_avatar_image_profile` + DEBUG `Avatar image profile resolved` `{context, chatId, characterId, imageProfileId, overrideRequested, overrideHonoured}`; the override WARN retired; `trigger_for_characters` → the effects module's `refresh_avatar_for_outfit` loop (or kept as a thin wrapper) |
| `services/image_profile_resolution.rs` | — | `resolve_image_profile_for_chat`, `resolve_wardrobe_image_profile` (`:129-`) | + `resolve_avatar_image_profile(main, user_id, override, chat) -> Option<Value>` with the three `[ProfileResolution]` DEBUGs; `db/image_profiles.rs` + `find_default(conn, user_id)` (`WHERE userId = ? AND isDefault = 1`, first row) + its fallback home (`Error finding default image profile` `{collection: 'image_profiles', userId}`) |
| `services/chat_avatars.rs` `:283-375` | — | toggle: chat profile (no key check) → `find_all` isDefault `:319-333` | `resolve_avatar_image_profile(user, None, Some(&chat))` + DEBUG `[Chats v1] No usable image profile; skipping initial avatar generation` `{chatId, context: 'avatar-generation'}` |
| `services/chat_create.rs` `:2336-2422` | 4921 | `find_by_ids_for_character` over `shared_wardrobe_tiers_for_character` `:2363-2377`; direct trigger `:2404-2420` | `load_wearable_pool(…, character_id, Some(project_mount_point_ids))` → `get_many`; `refresh_avatar_for_outfit(…, "[Chats v1] chat-open")` |
| `api/chat_cast.rs` `:170-215` | — | direct trigger `:192-207` | `refresh_avatar_for_outfit(…, "[Chats v1] participant-join")` — no behaviour change |
| `services/outfit_selections.rs` | 2077 | the three-loader pool `:600-703` with the four `[applyOutfitSelections]` lines; `manual_worn_bundles` `:234-260`; `resolve_llm_pick` `:268-281` + `resolve_wearable_pool` `:285-295`; instructions' group ids via `shared_wardrobe_tiers_for_character(…, &[])` `:1110`; `dissolve_bundles_in_slots_with_credit(&slots, &lookup_from(&wardrobe_items))` `:1396`; the progress resolve `:538` | one `create_shared_tier_loader` + memoised `load_wearable_pool` per character; `.wearable()` for candidates / defaults; `by_id` for the two dissolves; `pool.tiers.group_mount_point_ids` for the instructions; the four lines retired. `outfit_instructions_wiring_guard` (source census: both `llm_choose` entrances pass a dressing-instructions resolver + a subprompts resolver) must stay green — keep the two entrances' shape. |
| `services/aurora_notifications.rs` `:265-282` | — | the per-turn flush (sorted drain, bare `let _ =`) | the effects module's `flush_pending_wardrobe_announcements` with `schedule_wardrobe_announcement('orchestrator-turn-end', …)`'s DEBUG / WARN; the announcement JOB `:340-360` re-points its resolver call onto a pool |
| `services/avatar_prompt.rs:100`, `services/build_context.rs:1317`, `services/story_background_job.rs:1383`, `tools/generate_image.rs:1710`, `services/wardrobe_item_image_generation.rs:206-212`, `services/character_avatar_job.rs` | — | `shared_wardrobe_tiers_for_character` + the resolver / `hydrate_component_graph` | the resolver-signature sweep: each builds a pool (`load_wearable_pool(…, character_id, Some(project_mount_point_ids))` — the project tier passed, group tier resolved inside) then the pure resolver. Which lane edits each file is the order's ownership table (§13). |
| `services/wardrobe_wear_commit.rs` + `set_equipped_outfit_census_guard` | 679 | the chokepoint; census = `db/chats_outfits.rs` + this file | UNCHANGED — every new path commits through it (v4's `DisplacementRepos.wardrobeWear.commitEquippedOutfit`, `setAll`'s direct call, `applyOutfitSelections`' persist). The census must stay exactly two files. |
| `crates/quilltap-web/src/wardrobe_routes.rs` `:358-364`, `:531-537` | — | `outfit-summary` GET + `equip` POST arms → `CoreRequest::{ChatOutfitSummary, ChatEquip}` | unchanged (the bodies / statuses the core answers change; the transport does not) |
| `apps/web/src/app/core/core-contract.ts` `:641`, `:7937-7950`, `:8047` | — | `ChatOutfitSummaryRequest`, `ChatEquipMode` / `ChatEquipRequest`, `equippedSlots` | unchanged shapes; the SPA lane consumes the new 400 sentences (`wardrobe-control-dialog` gates archived client-side per bug 191) |

---

## 11. Harness — predicted colour under a pin regen, and the cases this lane owes

| family (oracle) | prediction | why |
|---|---|---|
| `chats_outfits_tier2_equivalence` (`chats-outfits-tier2.ts`, v4's REAL `ChatsRepository` set/get/remove) | GREEN | `chats.repository.ts` is untouched in the span; the repo ops are below this lane. |
| `outfit_hash_equivalence` (`outfit-hash.ts`) | GREEN | `lib/wardrobe/outfit-hash.ts` not in the span's stat. |
| `composed-outfits.test.ts` (the `generated_wardrobe_items` / composed-outfits corpus) | GREEN | `isBundle` → `isComposite`, same predicate (`componentItemIds.length > 0`). |
| `set_equipped_outfit_census_guard` | GREEN iff the lane keeps every write in the chokepoint | a new `set_equipped_outfit(` site is a red by name. |
| `outfit_instructions_wiring_guard` | GREEN iff the two `llm_choose` entrances keep passing both resolvers | source census, not behaviour. |
| `wardrobe_wear_commit_tier2_equivalence` (`wardrobe-wear-commit-tier2.ts`, 12 `route` ops + commit/apply ops, CAPTURED = the six ledger lines) | **GREEN on bytes, but it pins NONE of the new behaviour** | the gestures' math and the chokepoint are unchanged; `route_set_all_unreachable_item`'s sentence unchanged; the new INFO/DEBUG lines (`Wardrobe item put on`, `Wardrobe slot changed`, `[applyDisplacement] Outfit change committed`, `[WearablePool] …`) are outside CAPTURED; `route_wear_missing_chat` still lands (the pool loads from the vault with `[]` project tier). **Owe:** add ops for `wear` of an archived item (400 + the `Refused to put on a wardrobe item` INFO with `reason=archived`), `set_all` newly-worn archived (400 + `Refused a fitting …`) vs already-worn archived (200), `add_to_slot` slot miss (400 — the new sentence), the 404 arm, `equip` alias logging `mode=equip` in the refusal but `mode=wear` in the success line, `clear_slot` with a stray `itemId` in the body; widen CAPTURED to the four new `[Chats v1]` lines + `[applyDisplacement] Outfit change committed` + the pool's `Loaded wearable pool`. |
| `chat_export_equivalence` (`chat-dialogs-export.test.ts` — `outfit_summary_rich` / `_empty` / `_chat_missing`) | **RED predicted at `outfit_summary_rich`** | the fixture (`build-chat-dialogs-fixture.ts:583-601`) equips Pip with `accessories: [N_HAT]` — **Nora's** hat (`:410`), and Nora is an outfit key (the bare entry), so the OLD cast-wide pool resolved it (the fixture comment "unresolvable, silently dropped" notwithstanding — the oracle's bytes decide; no NDJSON is committed). Under the pin each character has their OWN pool → Pip's `accessories` is `[]`. `P_SET` (coat + boots) in `top` + direct `P_BOOTS`: old `top:[coat]`, `footwear:[boots]`; new: the same bytes (boots dedupe). The multi-slot-leaf spread arm (a `top+bottom` dress equipped to `top` alone) is NOT exercised — **owe a case**, plus a composite whose parts spread to a slot the composite id never sat in, and the empty-cast chat (reads happen, bytes `{summary: {}}`). The oracle drives v4's REAL route, so the regen reflects the pin whatever the comment says. |
| `chat_create_capstone_equivalence` (`chat-create-capstone.test.ts`, v4's real POST /chats) | GREEN likely | DB dumps + frames compared, not the `[applyOutfitSelections]` lines; the whisper titles are identical through the pool; the avatar trigger only moves if a capstone chat has `avatarGenerationEnabled` with a keyless default image profile — check `build-chat-create-capstone.ts` (its `isDefault: true` at `:429` is a project wardrobe item, not an image profile). The `wardrobe_wear_stats` dump (`:139`) is unchanged by the pool swap. |
| `chat_cast_routes_equivalence` (`chat-cast-routes.test.ts` — `toggle_avatar_generation_{on,off,chat_missing}`, the join refresh) + its five in-binary join tests (`:1755-1870`) | GREEN | the fixture's image profile carries `apiKeyId: I.apiKey` + `isDefault: true` (`build-chat-cast-fixture.ts:230-254`; `:586` flips `isDefault: false` for one case → `findDefault` null = `findAll().find` null → no job either way); the join's `REQUESTED` / `ENQUEUED` lines are unchanged text. The toggle's new DEBUG is not compared. |
| `wardrobe_tools_avatar_trigger_equivalence` — `trigger_read_failures_take_v4_fallback_arms` (`:449-560`) | **RED by design once `resolve_avatar_image_profile` lands** | it asserts WARN `override profile not found` (`:77`) and ERROR `Error finding all entities` (`:71`) — both GONE at the pin (DEBUG `[ProfileResolution] Avatar override profile unusable; falling back`; `findDefault`'s ERROR `Error finding default image profile`). Rewrite the arms to the pin. The oracle half (`wardrobe-tools-avatar-trigger.ts`, v4's real tool handlers) is the tools lane's: under the pin `wardrobe_create` + `equip_now` ALSO enqueues an announcement (bug 193) — its `jobs_by_chat` dump gains a row (tools lane's red, noted as a meeting point). |
| `avatar_job_tier3_equivalence` | GREEN likely | the job handler's own `if (!imageProfile.apiKeyId)` (`character-avatar.ts:124`) is unchanged; the prompt's resolver call (`avatar-prompt.ts:166`) yields the same leaves unless an equipped id crossed characters. |
| `outfit_llm_choose_tier3_equivalence` (jest `chat-dialogs-llm-choose-tier3`) | GREEN likely; ONE arm to inspect | DB dumps compared (no line capture — `CAPTURED` absent). Candidate SET unchanged (both old reads excluded archived); candidate ORDER is `wearable()`'s general → project → group → own — verify against the old `findArchetypes` order. The `archive_tier_garments` cases (`:1012`): under the pin a picked composite with an archived PART dissolves the part INTO the slots (`pool.byId`), where today the part is dropped — if any canned pick names such a composite, the `chats` dump flips. |
| `wardrobe_tools_equivalence`, `wardrobe_routes` / the four tiers' item routes, `wardrobe_transfers`, `preview_avatar`, `vault_wardrobe_*` | tools / routes / images lanes | the `wardrobe_wear` sentences are already word-identical (`tools/wardrobe_wear.rs:267`, `:293`). |

No v4 jest test in §9 is an oracle today: `outfit.test.ts`'s 19 cases (the
real-pool seam) are the natural shape for a NEW tier-2 family over the equip
route + summary (`chat-outfit-routes` or a widened `wardrobe-wear-commit`),
and `resolve-equipped.test.ts`'s 14 for a tier-1 `resolve-equipped` corpus over
`buildWearablePool`-built pools.

---

## 12. What this lane NEEDS from the keystone (the v4 functions, by name)

From `lib/wardrobe/pool.ts`: `loadWearablePool` (with `operator`, `chatId`,
`sharedTiers`, `ownItems` options), `loadCastPools`, `createSharedTierLoader`
(+ `loadSharedTiers`), `resolveProjectTierForChat`, `buildWearablePool` (pure —
the harness builds pools from planted items with it), `componentGraph`, the
`WearablePool` trait/struct surface (`character_id`, `tiers`, `by_id`,
`wearable()`, `get`, `get_many`, `find_by_title`, `owns`), and the six
`[WearablePool]` log lines exactly (§2.1).

From `lib/wardrobe/wear-ops.ts`: `resolve_wearable` (the three refusal
reasons + sentences), `wear_item` (→ `apply_displacement` — **this lane's**;
so `wear_item` is the one keystone function that calls INTO this lane: agree
the `apply_displacement` signature up front, or let this lane own `wear-ops`
too), `find_in_pool`, `wardrobe_item_not_found_message`, `take_off_item`.

From `lib/wardrobe/wearable.ts`: `archived_wear_message`, `wear_refusal`,
`newly_worn_archived_items`.

From `lib/wardrobe/slot-ops.ts`: `compute_displaced_slots` + `DisplacementMode`
/ `ComputeDisplacedOptions`, `worn_bundles_for_mode`, `analyze_composite`
(returning `{leaves, cycles, truncated, unresolved}` SILENTLY — the logging is
this lane's), `dissolve_composite_to_leaves`, `dissolve_composites_in_slots`,
`worn_bundles_for`, `WornBundle`; and `wardrobe.types.ts`'s `is_composite`,
`add_id_to_slot`, `remove_id_from_slot`, `equipped_slots_equal`.

From `lib/wardrobe/composite-types.ts`: nothing (the tools / routes lanes).
From `wearable-pool.ts`: `merge_wearable_tiers` (only if `outfit_selections`
keeps a list-merge anywhere; the pin's `apply-outfit-selections` uses the pool).

Also from the keystone's RETIREMENTS: `hydrate_component_graph`,
`lookup_for_bundle`, `find_by_ids_for_character`, `find_by_id_for_character`,
`find_wearable_pool_for_character`, `resolve_wardrobe_item_across_tiers`,
`resolve_shared_wardrobe_tiers_for_chat`, `shared_wardrobe_tiers_for_
character` — this lane removes ITS callers; who deletes the definitions (and
when) must be sequenced so neither lane's tree fails to build mid-round.

---

## 13. Cross-lane meeting points

1. **`apply_displacement` ↔ the tools lane.** `wardrobe_wear` / `wardrobe_
   take_off` / `wardrobe_create`'s `equip_now` call the same primitives
   (`tools/wardrobe_wear.rs:297-340`, `wardrobe_take_off.rs:174/294`,
   `wardrobe_create.rs:388`). At the pin the tools go through `wear-ops`'
   `wearItem` / `takeOffItem` → `applyDisplacement` with `source: 'tool'`.
   One owner for `apply_displacement` (this lane); the tools consume it.
2. **`notify_wardrobe_changed` / the effects module ↔ the tools lane + the
   chat spine.** The tools' `notifyWardrobeChanged` (with the per-turn Set —
   bug 193 adds it to `wardrobe_create`), the orchestrator's `flushPending…`
   (v5 `services/aurora_notifications.rs:265-282`; the executor's pending set).
   This lane writes the module; the tools lane re-points three call sites.
3. **The resolver's signature sweep ↔ the images / avatar / announcement
   owners.** `resolve_equipped_*` is called from `avatar_prompt.rs:100`,
   `aurora_notifications.rs:347-360`, `build_context.rs:1317`,
   `story_background_job.rs:1383`, `generate_image.rs:1710`,
   `wardrobe_item_image_generation.rs:206-212`, `outfit_selections.rs:538`,
   the tools' describe path. Either this lane sweeps them all (one commit, the
   pure resolver + a pool per call site) or each owner re-points after the
   resolver lands — the order must say which. The avatar-prompt / item-image
   prompt TEXT is the `(d) PROMPTS` lane's; only the pool plumbing moves here.
4. **`resolve_avatar_image_profile` ↔ the item-images / preview lane.**
   `wardrobe/preview-avatar/route.ts:77` consumes it; `resolve_wardrobe_image_
   profile`'s default leg (`image_profile_resolution.rs:164-170`) should sit on
   the same new `find_default`. One `services/image_profile_resolution.rs`
   edit — agree the function name + `Option<Value>` return before both lanes
   start.
5. **`wardrobe_tiers.rs` / `wardrobe_read.rs` / `wearable_pool.rs` /
   `dissolve_bundles.rs` retirements ↔ the keystone.** The keystone defines the
   pool; the last caller deletes the old resolvers. `dissolve_bundles.rs`'s two
   WARNs move INTO this lane's `apply_displacement` — if the keystone renames
   the module to `slot_ops.rs`, this lane's import follows.
6. **The outfit-summary ↔ the SPA lane.** The SPA's outfit summary consumer
   (`chat-keys.ts`, `merge-conversation-modal.ts`, `tab-refetch.ts`) reads the
   same shape; the new routing (a dress under two slots) is what the dialog
   renders — no contract change, a display change the SPA lane's Playwright
   may pin.
7. **The bug-191 client gate** (`wardrobe-control-dialog.tsx`) — the SPA
   lane's; it pre-empts the 400 the server now answers.
8. **`chat_create.rs` ↔ the scenarios round.** Not this round, but
   `chat_create.rs` is also the scenarios lane's file next round — keep this
   lane's edit to `post_opening_outfit_and_avatar` (`:2336-2422`) only.

---

## 14. Open questions for the order (rulings or decisions)

1. **Where do `apply_displacement` and the resolver live?** v4 split them
   (`outfit-displacement.ts`, `resolve-equipped.ts`, `outfit-change-effects.
   ts`); v5 today keeps both in `tools/wardrobe_shared.rs`. Following v4's
   files (three new `services/` modules) is the DRY-er end state and keeps
   `wardrobe_shared.rs` from growing a fourth responsibility; recommend the
   split.
2. **Does this lane own `wear-ops.ts`?** It is the keystone's by the brief,
   but `wearItem` is `applyDisplacement`'s only non-route caller and `takeOff
   Item` the tools'. If the keystone ships `wear-ops` before `apply_
   displacement` exists, it has nothing to call — stacking order or a shared
   signature stub (NOT a `TODO`) is needed.
3. **`chat_export_equivalence`'s `outfit_summary_rich`.** If the regen flips
   at `N_HAT` as predicted, that is v4's intended fix landing (the v4 test
   `:392`), not a v5 bug — record it as the family's tripwire, re-pin, and
   grow the fixture with the dress / composite-spread / empty-cast arms.
4. **`llm_choose` over `pool.byId`.** A picked composite's ARCHIVED part now
   goes on (§8). That is what the pin does (and arguably contradicts bug 191's
   spirit); port it as v4 has it and consider a v4 filing — not a v5 fix.
5. **`describeEquippedOutfitTitleOnly`'s project tier is not roster-gated**
   (`projectMountPointIds` passed explicitly) — same as today; no ruling, just
   don't "fix" it onto `operator: false`.

---

## Appendix — verbatim sentence / line census at the pin (this lane's wire + log surface)

Wire sentences (equip route): `Chat not found` (404, summary) · `Wardrobe item
not found` (404) · `Item "<title>" is archived and cannot be worn` (400) ·
`Item "<title>" (types: <t1, t2>) cannot be added to the "<slot>" slot` (400) ·
`Wardrobe item <id> not available to this character` (400) · `Failed to equip
wardrobe slot` (500) · `Failed to fetch equipped outfit` / `Failed to fetch
equipped outfit summary` (500) · the Zod joins (unchanged). Avatar: `Chat not
found.` · `No image profile is configured. Set one in Settings → Images before
generating avatars.` · `Avatar regeneration queued` (unchanged).

Log lines (level · text · keys): INFO `[Chats v1] Wardrobe item put on`
`{chatId, characterId, mode, itemId, slotsAffected, effect, context}` · INFO
`[Chats v1] Wardrobe slot changed` `{chatId, characterId, mode, slot, itemId,
context}` · INFO `[Chats v1] Refused to put on a wardrobe item` `{chatId,
characterId, mode, itemId, reason, context}` · INFO `[Chats v1] Refused a
fitting that puts on an archived item` `{chatId, characterId, itemIds,
context}` · INFO `[Chats v1] Equipped outfit replaced (set_all)` `{chatId,
characterId, wornBundleCount, context}` · DEBUG `[Chats v1] Some claimed worn
bundles were not credited` `{characterId, claimed, resolved, context}` · DEBUG
`[Chats v1] Built outfit summary` `{chatId, characterCount, context}` · ERROR
`[Chats v1] Error equipping wardrobe slot` `{chatId}` · ERROR `[Chats v1]
Error fetching equipped outfit summary` `{chatId}` · DEBUG `[applyDisplacement]
Outfit change committed` `{context, chatId, characterId, mode, itemId, slot,
source}` · WARN `[applyDisplacement] Malformed composite graph; expansion
truncated` `{context, characterId, itemId, cycles, truncated}` · WARN
`[applyDisplacement] Composite resolved to no wearable parts; wearing it
whole` `{context, characterId, itemId, componentCount}` · DEBUG
`[resolveEquippedOutfitForCharacter] Equipped ids not in the wearable pool`
`{context, characterId, unresolvedCount}` · WARN
`[resolveEquippedOutfitForCharacter] Malformed composite graph; expansion
truncated` `{context, characterId, slot, cycles, truncated}` · DEBUG `Wardrobe
outfit announcement scheduled` `{context, chatId, characterId}` · WARN `Failed
to schedule wardrobe outfit announcement` `{context, chatId, characterId,
error}` · DEBUG `Avatar image profile resolved` `{context, chatId,
characterId, imageProfileId, overrideRequested, overrideHonoured}` · DEBUG
`[ProfileResolution] Avatar override profile unusable; falling back`
`{context: 'image-gen.profile-resolution', override, found}` · DEBUG
`[ProfileResolution] Chat image profile unusable for avatars; falling back`
`{context, chatProfileId, found}` · DEBUG `[ProfileResolution] No usable avatar
image profile` `{context, userId}` · ERROR `Error finding default image
profile` `{collection: 'image_profiles', userId, error}` (the fallback form) ·
DEBUG `[Chats v1] No usable image profile; skipping initial avatar generation`
`{chatId, context: 'avatar-generation'}` · plus the six `[WearablePool]` lines
(§2.1, keystone's text).

GONE (retire wherever v5 spells them): `Wardrobe item worn` · `Wardrobe item
force-replaced` · `Wardrobe item layered into slot` · `Wardrobe item removed
from slot` · `Wardrobe slot cleared` · `[Chats v1] Failed to schedule outfit
announcement` · `Wardrobe item "<title>" does not cover the <slot> slot` ·
`Failed to update equipped slot` · `Avatar generation override profile not
found, falling back` · `[dissolveBundleToLeaves] Malformed bundle graph;
expansion truncated` · `[dissolveBundleToLeaves] Bundle resolved to no wearable
parts; wearing it whole` · `[expandComposites] Max depth reached, treating as
leaf` · `[resolveEquippedOutfitForCharacter] findByCharacterId failed;
proceeding with findByIds only` · `[resolveEquippedOutfitForCharacter]
findByIds fallback failed` · `[hydrateComponentGraph] Component hydration
failed` · `[loadBundleLookup] Failed to load bundle components` · the four
`[applyOutfitSelections]` pool lines · `[Wardrobe] Project lookup for chat
failed` / `[Wardrobe] Character off project roster — project wardrobe withheld`
(retagged `[WearablePool]` in the pool) · `[Chats v1] Refused to wear an
archived item` (`7a8d6dacc`'s intermediate, never at the pin).
