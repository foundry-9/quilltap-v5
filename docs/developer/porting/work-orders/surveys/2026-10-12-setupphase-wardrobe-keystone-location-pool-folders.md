# Survey — the wardrobe KEYSTONE data layer: one location, one wearable pool, the recursive predicate, the projection, the folder rules

Read-only planning survey, 2026-10-12. v4 = `~/source/quilltap-server`, read ONLY
at the pin `4a38222e6` (`4.10.0-dev.148`, "Wardrobe folders — file items into a
tier's closet (#85)") via `git show`; v5 = main at `f84d34150`.

Scope: the data layer the whole wardrobe catch-up round stands on — v4 commits
`7a8d6dacc` (bugs 187/188/189/191), `9ecebf986` (the seven-phase refactor),
`4a38222e6` (folders) — restricted to `lib/wardrobe/{location,pool,wearable-pool,
wear-ops,wearable,folders,folder-mutations,folder-view,composite-types,
archived-patch,item-mutations,wardrobe-container,slot-ops,create-body,
next-copy-title}.ts`, `lib/database/repositories/vault-overlay/{parsers,
vault-projection,vault-readers,wardrobe-sync,wardrobe-writes}.ts`,
`lib/database/repositories/wardrobe.repository.ts`, `lib/mount-index/{sanitize-
file-name,ensure-owner-store,store-file,character-vault,shared-wardrobe,
general-wardrobe}.ts`, `lib/schemas/wardrobe.types.ts`,
`migrations/lib/legacy-wardrobe-rows.ts`. **Out of scope** (the five lanes that
branch from this keystone's commit): the tools (`wardrobe-tool-table.ts`, the
seven handlers), the route factory + transfers + `link-image` + preview-avatar,
`outfit.ts` + `apply-outfit-selections` + `outfit-displacement` + the chat
spine, item images / prompts / Almanack (`item-images.ts`,
`item-image-generation.ts`, `lib/image-gen/*`, `physical-description.ts`,
`vision-request.ts`), the carriers (backup / restore / `.qtap` / `uuid-remapper`),
`e23fb095b` (SPA only) and every SPA file. `folder-view.ts` is surveyed here
because it is the pure half of the browser and the SPA lane ports it to TS; it
has NO server consumer and needs NO Rust twin.

Every fact below is from the shipped hunks (`git show 4a38222e6:<path>`, `git
diff 3e6e2f590 4a38222e6 -- <paths>`); line numbers are AT THE PIN. v5 is cited
`path:line` on main. The ledger's §3 rows for the four shas (`drift-ledger.md:
460-466`) are the map this survey fills in; nothing here contradicts them.

---

## 0. What moved in this area (the span `3e6e2f590..4a38222e6`)

`git diff --stat -- lib/wardrobe lib/vault-overlay lib/database lib/schemas
lib/mount-index migrations/lib`: **76 files, +4,712 / −3,886.** NO schema / DDL
/ migration / index move anywhere in the span (`wardrobe.types.ts` adds a Zod
field on a FILE-backed entity and four pure helpers; `legacy-wardrobe-rows.ts`
is a raw read of the retired table for the two cutover scripts).

**NEW (non-test, this area):** `lib/wardrobe/{location,pool,wear-ops,wearable,
folders,folder-mutations,folder-view,item-mutations,item-draft,slot-ops,
outfit-change-effects}.ts`, `lib/wardrobe/routes/wardrobe-route-factory.ts`,
`lib/mount-index/{ensure-owner-store,sanitize-file-name}.ts`,
`migrations/lib/legacy-wardrobe-rows.ts`. (`composite-types.ts`,
`archived-patch.ts`, `create-body.ts`, `next-copy-title.ts`,
`wardrobe-container.ts`, `wearable-pool.ts` PRE-EXIST; `next-copy-title.ts` is
UNTOUCHED by the span — no work.)

**DELETED:** `lib/wardrobe/{shared-tiers,resolve-container,dissolve-bundles,
hydrate-components}.ts`, `lib/mount-index/{project-wardrobe,group-wardrobe,
ensure-project-store,ensure-group-store,mount-wardrobe-route-factory}.ts`.
Their exports (at `3e6e2f590`), for the v5 twins they retire:
`resolveSharedWardrobeTiersForChat` / `sharedWardrobeTiersForCharacter` /
`SharedWardrobeTiers` / `SharedWardrobeTierOptions` (→ `pool.ts`);
`resolveWardrobeContainer` / `ResolvedWardrobeContainer` (→ `location.ts`);
`ensureProjectWardrobeFolder` / `readProjectWardrobe` /
`ensureGroupWardrobeFolder` / `readGroupWardrobe` (→ `ensureSharedWardrobeFolder`
+ `readSharedWardrobe` through a location); `ensureProjectOfficialStore` /
`ensureGroupOfficialStore` (→ `ensureOwnerOfficialStore(kind, …)`);
`hydrateComponentGraph` / `loadBundleLookup` (→ `pool.byId` / `componentGraph`);
`slotsCoveredBy` / `isBundle` / `dissolveBundleToLeaves` / `layLeavesIntoSlots` /
`dissolveBundlesInSlots` / `dissolveBundlesInSlotsWithCredit` (→ `slot-ops.ts`,
the OUTFIT lane's).

**RENAMED:** `lib/wardrobe/avatar-{cache,prompt}.ts` → `lib/image-gen/`
(+ their three tests) — the IMAGES lane's.

**`wardrobe.repository.ts` shrank 563 lines** (§10). **`wardrobe-writes.ts`
grew +697** (§4). **`parsers.ts` +89, `vault-projection.ts` +104,
`vault-readers.ts` 65 changed, `wardrobe-sync.ts` 30 changed** (§5–§6).

---

## 1. `lib/schemas/wardrobe.types.ts` — the schema and the four pure helpers

- `:33` `WARDROBE_SLOT_TYPES = ['top','bottom','footwear','accessories','hair']`;
  `:36` `WardrobeItemTypeEnum = z.enum(WARDROBE_SLOT_TYPES)` — unchanged.
- `:16` NEW import `{ isValidWardrobeFolderPath, WARDROBE_FOLDER_MAX_DEPTH }
  from '@/lib/wardrobe/folders'` (the schema module now depends on the rules
  module; `folders.ts` depends only on `sanitize-file-name.ts` — no cycle).
- `:132-137` NEW `WardrobeFolderPathSchema = z.string().max(900).refine(
  isValidWardrobeFolderPath, { message: \`Wardrobe folder must be a clean path
  of at most ${WARDROBE_FOLDER_MAX_DEPTH} segments, none named "images"\` })`
  — the message interpolates to **`Wardrobe folder must be a clean path of at
  most 8 segments, none named "images"`**.
- `:143` private `WardrobeFolderInputSchema = z.string().max(900)` — what a
  REQUEST body carries (any string up to the cap; the route normalises).
- `:155-172` `wardrobeItemFieldsSchema` gains `:171` `folder:
  WardrobeFolderInputSchema.optional()` (doc: "Create defaults to the root;
  update leaves the folder unchanged when omitted"). `:165` `replace`'s doc
  reworded to "composites and leaves alike". `:175` `createWardrobeSchema =
  wardrobeItemFieldsSchema`; `:182` `updateWardrobeSchema = …partial().extend({
  imageFileId: UUIDSchema.nullable().optional() })`.
- `:193-212` NEW body schemas (the ROUTES lane parses them; recorded here
  because the keystone's error sentences and the `≤ 500` cap meet them):
  `createWardrobeFolderSchema = { path: WardrobeFolderInputSchema.min(1, 'A
  folder path is required') }`; `renameWardrobeFolderSchema = { path: …min(1,
  'A folder path is required'), name: z.string().min(1, 'A folder name is
  required').max(200) }`; `deleteWardrobeFolderSchema = { path: …min(1, 'A
  folder path is required') }`; `moveWardrobeItemsSchema = { itemIds:
  z.array(z.string().min(1)).min(1, 'At least one item is required').max(500),
  folder: WardrobeFolderInputSchema }`.
- `:218-275` `WardrobeItemSchema`: `:272` NEW **`folder:
  WardrobeFolderPathSchema.default('')`** between `archivedAt` and `createdAt`
  — so Zod's output object order is `id, characterId, title, description,
  imagePrompt, types, componentItemIds, appropriateness, isDefault, replace,
  migratedFromClothingRecordId, imageFileId, archivedAt, folder, createdAt,
  updatedAt`, and `folder` is ALWAYS materialised (a `.default()` key).
  `:238-240` `componentItemIds` doc: cycles "rejected at save time by the
  folder writer (`wardrobe-writes.ts`) and dropped at read time by the vault
  parser"; `:247-253` `replace` doc: now applies to leaves too.
- `:390-393` NEW `isComposite(item: { componentItemIds?: readonly string[] |
  null }): boolean` → `(item.componentItemIds?.length ?? 0) > 0`.
- `:398-402` NEW `addIdToSlot(slots, slot, itemId)` → clone; append `itemId`
  to `next[slot]` unless already present.
- `:408-412` NEW `removeIdFromSlot(slots, slot, itemId?)` → clone;
  `itemId === undefined` → `[]`, else filter `!== itemId`.
- `:415-421` NEW `equippedSlotsEqual(a, b)` → every slot of
  `WARDROBE_SLOT_TYPES`: `(a[slot] ?? [])` and `(b[slot] ?? [])` same length
  and element-wise `===` in order.
- Also: `:86-95` (`schema.ts`) `LegacyVaultWardrobeJsonSchema.items =
  z.array(WardrobeItemSchema)` — **a legacy `wardrobe.json` parse now emits
  `folder: ''` on every item** (the `.default`). `vault_legacy_wardrobe_
  equivalence` reddens on a pin regen for that key alone.

## 2. `lib/wardrobe/folders.ts` (NEW, 271 lines, client-safe) — the rules, verbatim

- `:19` `WARDROBE_FOLDER_MAX_DEPTH = 8`; `:25` `WARDROBE_RESERVED_FOLDER_SEGMENTS:
  readonly string[] = ['images']`; `:28` private `WARDROBE_ROOT = 'Wardrobe'`.
- Error classes (the factory maps them — `wardrobe-route-factory.ts:125-131`:
  cycle → 400, path → 400, not-in-location → 400, folder-not-found → 404
  `notFound('Wardrobe folder')`, conflict → 409 `conflict(error.message)`,
  `CharacterArchivedError` → 409 `"An archived character's wardrobe cannot be
  changed"`):
  - `:31-36` `WardrobeFolderPathError(message)`, `name = 'WardrobeFolderPathError'`.
  - `:39-44` `WardrobeFolderNotFoundError(folder)` — message **`Wardrobe folder
    not found: ${folder}`**.
  - `:50-55` `WardrobeFolderConflictError(folder, message?)` — default message
    **`A wardrobe folder named "${folder}" already exists`**.
  - `:61-66` `WardrobeItemNotInLocationError(itemIds)` — **`Not in this
    wardrobe (shared items are filed where they live): ${itemIds.join(', ')}`**.
- `:69-76` `WardrobeFolder { path: string /* never '' */; name: string /* last
  segment */; itemCount: number /* non-archived items DIRECTLY inside */ }`.
- `:78-81` `isReservedSegment(segment)` — lower-cased equality against the list.
- `:90-116` **`normalizeWardrobeFolderPath(input)`**: `null`/`undefined` → `''`;
  split on `/[\\/]/`, trim each, drop empties; per raw segment: `'.'` or `'..'`
  → throw `A wardrobe folder name may not be "${raw}"`; `segment =
  sanitizeFileName(raw)` (§10.3 — separators and `:*?"<>|` → `_`, whitespace
  runs → one space, trim, ≤ 100 chars, `'untitled'` fallback); reserved (after
  sanitising, case-insensitive) → throw `"${segment}" is reserved for item
  pictures and cannot be a wardrobe folder`; more than 8 → throw `Wardrobe
  folders nest at most 8 deep`; join with `/`. `''`/whitespace = the root.
- `:123-129` `normalizeWardrobeFolderPathOrRoot(input)` — the above, any throw
  → `''`.
- `:135-142` `normalizeWardrobeFolderName(input)` — a `/` or `\` anywhere →
  throw `A folder name may not contain "/"`; then normalise; empty → throw
  `A folder needs a name`.
- `:152-158` **`isSafeWardrobeFolderPath(path)`** (structural, spelling-blind):
  `''` → true; `length > 900` or contains `\` → false; any segment empty,
  `trim() !== s`, `.` or `..` → false; `!isReservedSegment(segments[0])` (ONLY
  the first level is reserved here). Passes hand-made `Hats: Summer`,
  `Formal/images`; refuses `../x`.
- `:169-191` **`resolveWardrobeFolder(existing, requested)`**: split/trim/drop
  empties; none → `''`; `byKey = Map(existing.toLowerCase → stored)`; for
  `depth = raw.length … 1`: the longest existing ancestor (lower-cased join)
  wins — full match → return the STORED spelling; partial → `joinFolder(stored,
  normalizeWardrobeFolderPath(rest))`, then if `folderSegments(joined).length >
  8` throw `Wardrobe folders nest at most 8 deep`; no ancestor →
  `normalizeWardrobeFolderPath(raw.join('/'))`.
- `:197-203` `isValidWardrobeFolderPath(path)` → `normalize(path) === path`
  (any throw → false).
- `:206-208` `folderSegments('')` → `[]`, else `split('/')`.
- `:214-220` **`folderOfWardrobePath(relativePath)`**: prefix `Wardrobe/`
  compared LOWER-CASED; not under it → `''`; else the part before the LAST `/`
  of the remainder (`Wardrobe/A/B/x.md` → `A/B`, `Wardrobe/x.md` → `''`).
  Note: a path `wardrobe/A/x.md` (lower-case root) yields `A` — the predicate
  (§6.1) is equally case-blind.
- `:223-225` `wardrobeFolderStorePath('')` → `Wardrobe`; `A/B` → `Wardrobe/A/B`.
- `:228-231` `parentFolder` (root's parent = root); `:234-237` `folderName`;
  `:240-242` `joinFolder(...parts)` skips empty parts; `:245-247`
  `isSameFolder(a, b)` lower-cased, `null` → `''`; `:250-255`
  `isUnderFolder(folder, ancestor)` — ancestor `''` → true, else lower-cased
  equal or `startsWith(a + '/')`; `:262-266` `rebaseFolder(folder, from, to)` —
  not under `from` → unchanged, else `joinFolder(to, folder.slice(from.length)
  .replace(/^\//, ''))` (NB: the slice is by `from.length`, case-blind through
  `isUnderFolder`); `:269-271` `folderCrumb(folder)` → segments joined by
  `' › '` (U+203A).

## 3. `lib/wardrobe/location.ts` (NEW, 288 lines, server-only)

- `:56-57` `WardrobeScopeSchema = z.enum(['character','group','project',
  'general'])`.
- `:59-105` `interface WardrobeLocation { scope; id: string | null /* null for
  General */; mountPointId: string /* always set */; characterId: string |
  null; origin: WardrobeOrigin; readItems(includeArchived = false);
  findItem(id) /* archived included */; create(item); update(id, patch);
  updateMany(patches, opts?: { requireAll? }); delete(id); listFolders();
  resolveFolder(requested); createFolder(path); renameFolder(path, newName);
  deleteFolder(path) → { movedItems, movedFolders, parent };
  writableMountPointId() }`.
- `:113-115` `locationKey(loc)` → `` `${scope}:${mountPointId}` `` (the
  transfer route's same-folder check).
- `:121-189` **`locationForMount(mount, id, origin)`**: `writableMount()` —
  non-character → the mount as given; character → `resolveWardrobeMount(
  characterId)` RE-RESOLVED at write time (throws `CharacterArchivedError`;
  `null` → throw `` `Character ${id} has no linked vault` ``). `updateMany`
  strips `id` / `createdAt` / `updatedAt` from EVERY patch ("Identity and
  timestamps are the writer's, never a patch's") then `updateManyInMount`.
  `readItems` → `readMountItems(mount)` filtered by `!archivedAt` unless
  asked; `findItem` → archived included; `update` = `updateMany` of one;
  `listFolders` → `listFoldersInMount(mount)` (a READ mount — no tombstone);
  `resolveFolder(requested)`: empty/whitespace → `''`, else
  `resolveWardrobeFolder(await listFolderPathsInMount(mount), requested)`;
  `createFolder`/`renameFolder`/`deleteFolder` through the WRITABLE mount.
- `:197-214` **`resolveWardrobeLocation(scope, id, repos, userId, { ensure })`**
  → `resolve(...)`, then ALWAYS debug **`[WardrobeLocation] Resolved wardrobe
  location`** `{ scope, id: id ?? null, ensure, found, mountPointId: … ??
  null, context: 'wardrobe' }`.
- `:216-269` `resolve`: `general` → `getGeneralMountPointId()`; none → `null`;
  `ensure` → `ensureSharedWardrobeFolder(mountPointId)`; origin
  `GENERAL_WARDROBE_ORIGIN`, id `null`. Non-General without an id → `null`.
  `character` → `repos.characters.findByIdRaw(id)`; missing or `userId !==
  character.userId` → `null`; no `characterDocumentMountPointId` → `null`;
  origin `{ scope: 'character', id, name: character.name }`. `project` /
  `group` → `findById(id)`; missing → `null`; `name = owner.name || ('Project'
  | 'Group')`; `ensure` → `ensureOwnerOfficialStore(scope, owner.id, name)`
  (null → `null`) then `ensureSharedWardrobeFolder(mountPointId)`; else
  `owner.officialMountPointId ?? null`; none → `null`; origin `{ scope, id:
  owner.id, name: owner.name }` (the RAW name, not the fallback).
  **A read probe (`ensure` off) never provisions; `ensure: true` is for writers
  and may WRITE on a GET-shaped caller.** The archived-character tombstone is
  NOT checked at resolve — reads work; every write throws.
- `:276-288` `groupLocationsForCharacter(characterId)` → one location per
  group mount from `resolveGroupMountsForCharacter` (`tiered-mount-pool.ts:
  237`), weakest first, origin `{ scope: 'group', id: group.id, name:
  group.name }`.

## 4. `vault-overlay/wardrobe-writes.ts` — the ONE folder writer, every tier

- `:61` `WardrobeMountScope = 'character' | 'group' | 'project' | 'general'`;
  `:64-69` `WardrobeMount { mountPointId; scope; characterId: string | null }`.
- `:71-84` `runSerialized` — a per-mount promise chain (Node concurrency guard;
  v5's writer task already serialises — NO-PORT, as `vault_wardrobe_public.rs:
  38-40` already records).
- `:92-107` `resolveWardrobeMount(characterId)`: `null`/`undefined` → General
  (`null` when unprovisioned); character → `findByIdRaw`; `archivedAt` →
  throw `CharacterArchivedError`; no mount → `null`. (v5 twin:
  `vault_wardrobe_public.rs:200-228` `resolve_wardrobe_mount` + `:147-173`.)
- `:110-115` `mountWardrobeLocation(mountPointId, scope = 'project')` — a
  shared-tier mount (`Exclude<…,'character'>`).
- `:122-129` **`readMountItems(mount)`** → `readCharacterVaultWardrobe(
  mountPointId, characterId ?? undefined)` (NO seed option any more), `null` →
  `[]`, every item's `characterId` coerced to `mount.characterId`.
- `:137-167` **`buildCyclePeers(mount, current)`**: the folder's items, then
  — character scope — `loadWearablePool(repos, characterId, undefined, {
  ownItems: current })` (the pool's `byId`, project tier NOT resolved: no
  chat), or — project/group scope — `readGeneralWardrobe(true)`; General → the
  folder alone. Any throw → WARN **`Wardrobe cycle check could not load peer
  tiers; checking the folder alone`** `{ mountPointId, scope, context:
  'wardrobe', error }` and the folder alone.
- `:170` `COMPONENT_CYCLE_MESSAGE = 'component cycle'`; `:173-182`
  `WardrobeComponentCycleError(itemId, cycles)` — **`Wardrobe item ${itemId}
  would create a component cycle: ${cycles.map(c => c.join(' → ')).join('; ')}`**
  (v5 has the bytes: `vault_wardrobe_public.rs:246-249`). `:184-191`
  `assertNoCycles` — non-composite → return; `peers.set(item.id, item)` then
  `detectComponentCycles`.
- `:200-206` **`safeFolder(folder)`**: falsy → `''`; `isSafeWardrobeFolderPath`
  → as given; else `normalizeWardrobeFolderPathOrRoot` with debug **`Wardrobe
  write normalised an incoming folder`** `{ folder, normalised, context }`.
  "The last line of defence" for callers that skipped `resolveFolder` (the
  `.qtap` import's `wardrobe_item` records, legacy presets).
- `:209-227` **`createInMount(mount, item)`**: read, cycle-check against
  peers, `stored = { ...item, folder: safeFolder(item.folder), characterId:
  mount.characterId }`, project `[...current, stored]`, debug **`Wardrobe item
  created in folder`** `{ mountPointId, scope, wardrobeItemId, folder,
  context }` (was INFO `… in vault`).
- `:230-237` `updateInMount(mount, id, patch)` = `updateManyInMount` of one,
  `[updated] ?? null`.
- `:240-253` `WardrobeItemPatch { id; patch: Partial<WardrobeItem> }`;
  `UpdateManyOptions { requireAll?: boolean }`.
- `:260-321` **`updateManyInMount` / `updateManyUnserialized`**: empty →
  `[]`; read once; `indexById`; `requireAll` → the ids not in the folder
  (deduped) → throw `WardrobeItemNotInLocationError` BEFORE any write; `now`
  once for the batch; per patch — unknown id SKIPPED; `merged = { ...before,
  ...patch, ...(patch.folder !== undefined && { folder: safeFolder(
  patch.folder) }), id: before.id, characterId: mount.characterId, createdAt:
  before.createdAt, updatedAt: now }`; a composite → peers built LAZILY ONCE
  (`peers ??= await buildCyclePeers(mount, current)` — against the PRE-batch
  `current`) and `assertNoCycles(merged, peers)` (NB: `assertNoCycles` mutates
  `peers` with each merged item, so later patches in the batch see earlier
  ones); `movesFolder` when `(merged.folder ?? '') !== (before.folder ?? '')`
  (exact-case compare — a case-only folder change counts as a move); no
  updates → `[]` with NO projection; else ONE `projectVaultWardrobe` and debug
  **`Wardrobe items updated in folder`** `{ mountPointId, scope,
  wardrobeItemIds, count, movesFolder, context }`; returns the merged items in
  patch order.
- **Folders:**
  - `:333-375` **`listFoldersInMount(mount)`**: `docMountFolders.findBy
    MountPointId` + `readMountItems` in parallel; `add(folder)` registers EVERY
    ancestor (`segments.slice(0, depth)`), first spelling wins per lower-cased
    key; rows pass through `folderUnderWardrobe` (§ below); items contribute
    `item.folder`; `counts` by lower-cased folder of NON-archived items with a
    folder; output `{ path, name: folderName(path), itemCount }` sorted by
    `path.localeCompare(b.path, undefined, { sensitivity: 'base' })`; debug
    **`Wardrobe folders listed`** `{ mountPointId, scope, folderRowCount,
    folderCount, context }`.
  - `:383-388` `listFolderPathsInMount(mount)` — folder ROWS alone, no item
    parse (what `resolveFolder` resolves against — so an item-implied folder
    with no row is invisible to resolution; writes always create rows).
  - `:394-402` `folderUnderWardrobe(storePath)` — `Wardrobe/` prefix
    lower-cased; `Wardrobe` itself / outside → `null`; first segment reserved
    (lower-cased) → `null`.
  - `:405-411` `findStoredFolder(mountPointId, folder)` →
    `docMountFolders.findByMountPointAndPath(mp, wardrobeFolderStorePath(
    folder))` (the store's path lookup is case-insensitive) → the stored
    casing or `null`.
  - `:418-435` **`createFolderInMount(mount, folder)`**: `''` → throw
    `WardrobeFolderPathError('The wardrobe root already exists')`;
    `ensureFolderPath(mp, Wardrobe/<folder>)` (creates ancestors);
    `stored = findStoredFolder ?? folder`; `itemCount` of live items
    `isSameFolder(i.folder, stored)`; debug **`Wardrobe folder ensured`**
    `{ mountPointId, scope, requested, folder, itemCount, context }`.
  - `:444-509` **`renameFolderInMount(mount, folder, newName)`**: `''` → throw
    `'The wardrobe root cannot be renamed'`; `from = findStoredFolder` else
    throw `WardrobeFolderNotFoundError(folder)`; `to = joinFolder(parentFolder(
    from), newName)`; if `to !== from` (a case-only rename proceeds): list
    `.md` docs under `Wardrobe/<from>` RECURSIVELY; those that are
    `isWardrobeItemDocument(doc) && !hasStableWardrobeItemId(doc.content)` →
    debug **`Stamping ids into wardrobe files before a folder rename`**
    `{ mountPointId, folder: from, count, context }` and ONE
    `projectVaultWardrobe(mp, characterId ?? mp, await readMountItems(mount))`
    (the whole tier, which writes `id:` into every file) — BEFORE
    `moveDatabaseFolder(mp, Wardrobe/<from>, Wardrobe/<to>)`;
    `DatabaseStoreError` `CONFLICT` → `WardrobeFolderConflictError(to)`,
    `NOT_FOUND` → `WardrobeFolderNotFoundError(folder)`, else rethrow; then
    `itemCount` of live items in `to`; debug **`Wardrobe folder renamed`**
    `{ mountPointId, scope, from, to, itemCount, context }`.
  - `:518-628` **`deleteFolderInMount(mount, folder)`**: `''` → throw `'The
    wardrobe root cannot be deleted'`; `from = findStoredFolder` else 404;
    `parent = parentFolder(from)`; `storePrefix = 'Wardrobe/<from>/'`
    lower-cased. CHECKS BEFORE ANY MOVE: (1) strays — every
    `docMountFileLinks.findByMountPointId` link under the prefix whose path
    has no `.md` doc (recursive listing) or whose doc fails
    `isWardrobeItemDocument` → throw `WardrobeFolderConflictError(from,
    \`Folder "${from}" holds ${n} file(s) that are not wardrobe items; move
    them in the Scriptorium first\`)` (a `.txt`, a picture, a slotless note, a
    nested `instructions.md` — the test's `it.each`); (2) a same-named
    subfolder — rows under the prefix sorted DEEPEST FIRST (`b.path.length -
    a.path.length`), `moving = items under from`; if any `rebaseFolder(sub,
    from, parent)` lands back under `from` → throw `…(from, \`Folder "${from}"
    holds a subfolder of the same name; rename one of them first\`)`. Then:
    items → ONE `updateManyUnserialized` (same chain) re-rooting each
    `folder` via `rebaseFolder` (archived included); subfolders →
    `ensureFolderPath` of each rebased row (empty ones included); then
    `deleteDatabaseFolder` each old row (deepest first) then `storeFrom`;
    `NOT_EMPTY` → throw `…(from, \`Folder "${from}" is not empty yet; its
    items were moved up\`)`; `movedFolders` = rows at depth `segments(from) +
    1`; debug **`Wardrobe folder deleted; contents sent up`** `{ mountPointId,
    scope, folder: from, parent, movedItems, movedFolders, context }`;
    returns `{ movedItems, movedFolders, parent }`.
- `:631-645` `deleteInMount(mount, id)` — not present → `false` (no
  projection); else project and debug **`Wardrobe item deleted from folder`**
  `{ mountPointId, scope, wardrobeItemId, context }`.
- `:648-668` the three mount adapters: `createMountWardrobeItem(mp, item)`,
  `updateMountWardrobeItem(mp, id, patch)` ("the restore pass uses this for
  every tier"), `deleteMountWardrobeItem(mp, id)` — scope defaults to
  `'project'` (`mountWardrobeLocation`), which only steers the cycle-peer
  branch (`scope !== 'general'` → General peers).

## 5. `vault-overlay/vault-projection.ts` — `projectArrayIntoVaultFolder` (+104)

- `:17-38` NEW `ProjectArrayOptions { preserveFileNames?; recursive?: {
  excludeSubfolders: readonly string[] }; ownsFile?: (doc: { relativePath;
  content? }) => boolean }`.
- `:58-157` body: `existing = findManyByMountPointsInFolder([mp], folder,
  '.md', recursive ? { recursive: true } : undefined)`; `existingByPath` keyed
  by EXACT `relativePath`; `mappedItems` computed up front; **subfolders**: per
  mapped `fileName`, `sub = recursive && lastSlash >= 0 ? before-slash : ''`,
  first spelling per lower-cased key; `occupied` = lower-cased parent of
  every existing doc; for each sub: `target = sub ? folder/sub : folder`;
  `recursive && occupied.has(target)` → skip, else `ensureFolderPath` (flat
  mode: ONE unconditional ensure of `folder` when `items.length > 0`, as
  before; an EMPTY projection ensures nothing in either mode); debug
  **`Recursive vault projection ensured subfolders`** `{ characterId,
  mountPointId, folder, subfolderCount }` when `subfolders.size > 1 ||
  (recursive && size === 1 && !has(''))`. **Name pool**: `seen` seeded with
  every preserved name IN EVERY folder written into (`key ? key/name : name`);
  collision loop suffixes `-1`, `-2`, … with `hasExt = dot > slash` (so
  `A.B/Coat` is handled); `writtenPaths` LOWER-CASED. **Sweep**: skip
  `writtenPaths.has(relPath.toLowerCase())` (the case-insensitive fix: the
  store's upsert keeps stored casing, so an exact compare could delete a file
  just rewritten with only a case change — applies to FLAT callers too); skip
  a preserved file segment (any depth in recursive mode); skip
  `ownsFile && !ownsFile(doc)`; recursive → skip when the first segment under
  `folder` is in `excludeSubfolders` (lower-cased); `deleteDatabaseDocument`,
  a throw → WARN `Failed to delete stale vault file during folder projection`
  `{ characterId, mountPointId, relativePath, docId, error }` (unchanged).
  Folders are never deleted by the projection.

## 6. `wardrobe-sync.ts` + `vault-readers.ts` + `parsers.ts`

### 6.1 `parsers.ts`
- `:257-267` **`isWardrobeItemDocumentPath(relativePath)`** — the ONE
  predicate, now recursive: lower-case the whole path; must start with
  `wardrobe/`; `rest` non-empty and ends `.md`; split on `/`; ANY empty segment
  → false; `segments.length > 1 && WARDROBE_RESERVED_FOLDER_SEGMENTS.includes(
  segments[0])` → false (so `Wardrobe/images.md` at the root IS an item;
  `Wardrobe/images/x.md` is not; `Wardrobe/A/images/x.md` IS an item — only
  the first level is reserved); `!isWardrobeInstructionsFileName(last
  segment)` (so `instructions.md` at ANY depth is excluded). Shared by
  `readCharacterVaultWardrobe`, the `.qtap` wear-ledger export / import and
  the restore id remap.
- `:276-279` NEW **`isWardrobeItemDocument(doc)`** → path predicate AND
  `parseWardrobeTypesField(parseFrontmatter(content ?? '').data?.types) !==
  null`. Quiet. The projection's `ownsFile`; the writer's stray / stamp checks.
- `:287-290` NEW **`hasStableWardrobeItemId(content)`** → frontmatter `id` is
  a string matching `/^[0-9a-f-]{36}$/i` (the same shape test as
  `resolveWardrobeItemId`, `:239-247`).
- `:326-463` `parseWardrobeItemFile`: `:426-437` NEW — `folder =
  folderOfWardrobePath(doc.relativePath)`; a frontmatter `folder` key
  (`hasOwnProperty`) → debug **``Wardrobe/*.md frontmatter carries a `folder`
  key; the file path wins``** `{ characterId, mountPointId, relativePath,
  frontmatterFolder, folder }`; `:459` `folder` in the literal between
  `archivedAt` and `createdAt`. Everything else unchanged (title ladder,
  types-required WARN `Wardrobe/*.md frontmatter has no valid \`types\` list;
  skipping`, `archived: true` → `archivedAt = doc.updatedAt`, etc.).
- `:505` NEW `COMPONENT_UUID_REF = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-
  [0-9a-f]{4}-[0-9a-f]{12}$/i` (the STRICT 8-4-4-4-12 form, unlike the id
  check's `[0-9a-f-]{36}`).
- `:525-586` **`resolveAndCheckComponentItems(items, itemBySlug, itemById,
  characterId, mountPointId)`** — the `archetypes` seeding is GONE from the
  reader (§6.2); pass 1 per ref: slug → id; id → id; else `COMPONENT_UUID_REF`
  → KEPT as written with debug **`Wardrobe item references a component
  outside this folder; keeping ref`** `{ characterId, mountPointId, itemId,
  ref }` (bug 187); else WARN **`Wardrobe item references unknown component
  slug; dropping ref`** `{ characterId, mountPointId, itemId, title, ref }`
  (retitled from `… unknown component; dropping ref`). Pass 2 unchanged: WARN
  `Wardrobe item declares a component cycle in vault; dropping its
  components` `{ characterId, mountPointId, itemId, title, cycles }` and the
  list cleared (a kept cross-tier UUID is unknown to `itemById`, so
  `detectComponentCycles` treats it as a leaf).

### 6.2 `vault-readers.ts` `readCharacterVaultWardrobe` (`:331-402`)
- `:331-334` signature `(mountPointId, characterId?)` — the `{ seedArchetypes
  }` option DELETED; `charId = characterId ?? mountPointId`.
- `:340-345` `findManyByMountPointsInFolder([mp], 'Wardrobe', '.md', {
  recursive: true })`; `:351` `itemDocs = allDocs.filter(isWardrobeItemDocument
  Path(relativePath))`; `:352-358` when anything was filtered → debug
  **`Wardrobe read skipped non-item documents`** `{ characterId: charId,
  mountPointId, skipped }`. Filter BEFORE the length check (a folder holding
  only `instructions.md` still falls through to legacy `wardrobe.json`).
- `:361-365` sort by `relativePath.localeCompare` (code-unit — Decision B in
  v5), parse, drop nulls; `:367-387` the slug/id maps (first-claimer; WARN
  `Wardrobe items share a slug; later item only addressable by UUID`); the
  `hasComponentRefs && seedArchetypes → findArchetypes(true)` block
  (`3e6e2f590:378-394`) DELETED — "The reader never calls back into the
  repository"; `:393` `resolveAndCheckComponentItems(items, itemBySlug,
  itemById, charId, mountPointId)`.
- Consequence (the ledger's (c)): a hand-written SLUG naming a General item
  is now dropped at parse with the WARN; a UUID survives. `shared-wardrobe.ts
  :56-60` `readSharedWardrobe(mp, includeArchived)` → `readCharacterVaultWardrobe(
  mp)` (no options), `characterId: null`; `general-wardrobe.ts:54-58`
  `readGeneralWardrobe(includeArchived)` unchanged in shape.

### 6.3 `wardrobe-sync.ts`
- `:35-38` `WardrobeOverlayOptions` loses `defaultsOnly` (and
  `getOverlaidWardrobeItems` its filter, `3e6e2f590:91-93`) — v4 filters
  defaults AFTER the pool merge now.
- `:106-152` `projectVaultWardrobe(mp, characterId, items)`: mapper
  `fileName: joinFolder(item.folder ?? '', \`${sanitizeFileName(item.title)}
  .md\`)`; options `{ preserveFileNames: [WARDROBE_INSTRUCTIONS_FILENAME],
  recursive: { excludeSubfolders: WARDROBE_RESERVED_FOLDER_SEGMENTS },
  ownsFile: isWardrobeItemDocument }`; the legacy `wardrobe.json` delete +
  WARN unchanged.

## 7. `lib/wardrobe/pool.ts` (NEW, 362 lines) + `wearable-pool.ts`

- `:40-45` `WardrobeTierMounts { groupMountPointIds: string[] /* weakest
  first */; projectMountPointIds: string[] }`; `:48-51` `SharedTierItems {
  general: WardrobeItemWithOrigin[]; project: WardrobeItemWithOrigin[] }`
  (archived INCLUDED, origin-tagged).
- `:53-71` **`WearablePool { characterId; tiers; byId: ReadonlyMap /* every
  reachable item, archived included, shadowed character > group > project >
  general */; wearable(); get(id); getMany(ids); findByTitle(title);
  owns(item) }`**.
- `:73-89` `LoadWearablePoolOptions { operator?; chatId?; sharedTiers?:
  Promise<SharedTierItems>; ownItems?: readonly WardrobeItem[] }`.
- `:92-99` `readTier(repos, mountPointIds, originOf)` → `[]` for none, else
  `repos.wardrobe.readSharedTiers(mountPointIds, true, originOf)` (§10.1).
- `:102-117` `loadSharedTiers(repos, projectMountPointIds, projectOrigin?)`:
  General (`getGeneralMountPointId` — none → `[]`) with
  `GENERAL_WARDROBE_ORIGIN`; project stores with `projectOrigin ?? { scope:
  'project', id: null, name: 'Project' }` (a POOL-level project origin has
  `id: null` — the pool never knows which project).
- `:124-142` `createSharedTierLoader(repos, projectMountPointIds)` — memoised
  lazy promise; a throw → WARN **`[WearablePool] Failed to read shared
  wardrobe tiers; continuing without them`** `{ projectMountCount, context:
  'wardrobe', error }` → `{ general: [], project: [] }`.
- `:149-180` **`resolveProjectTierForChat(repos, chatId, characterId, {
  operator })`**: no `chatId` → `[]`; `chats.findById` throw → WARN
  **`[WearablePool] Project lookup for chat failed`** `{ chatId, context,
  error }` → `[]` (UNREACHABLE behind v4's fallback `_findById`, as v5 already
  records for its `[Wardrobe]`-tagged twin — `wardrobe_tiers.rs:132-143`); no
  `projectId` → `[]`; `operator ? projectId : rosterGatedProjectId(projectId,
  characterId)`; withheld → debug **`[WearablePool] Character off project
  roster — project wardrobe withheld`** `{ chatId, projectId, characterId,
  context }` → `[]`; else `resolveProjectMountPointIds(gated)`. (v5's line is
  tagged `[Wardrobe]` today — the TAG changes.)
- `:183-189` `shadow(tiers)` — weakest first into one `Map` (`set` keeps the
  FIRST insertion position, replaces the value).
- `:195-246` **`buildWearablePool(characterId, tierMounts, { own, group,
  project, general })`** (pure): `weakestFirst = [general, project, group,
  own]`; `byId = shadow(weakestFirst)`; `wearable()` = memoised
  `shadow(weakestFirst.map(tier => tier.filter(!archivedAt)))` — **archived
  dropped PER TIER BEFORE shadowing**, so an archived own copy never hides a
  live shared one; `ownIds = Set(own ids)`; `get` → `byId.get`; `getMany(ids)`
  → found items in the ORDER ASKED, duplicates and unknowns skipped;
  `findByTitle(title)` → `wanted = title.trim().toLowerCase()`, empty →
  `undefined`; `layers.own.find(title.toLowerCase() === wanted)` (archived
  INCLUDED) first, else `wearable().find(!ownIds.has(id) && title match)` (so
  an archived SHARED item is never found by title); `owns(item)` →
  `item.characterId === characterId && ownIds.has(item.id)`.
- `:254-322` **`loadWearablePool(repos, characterId, projectMountPointIds?,
  opts)`**: `projectIds` = the explicit list (even `[]`) else
  `resolveProjectTierForChat(repos, opts.chatId, characterId, { operator })`;
  in parallel: `resolveGroupMountsForCharacter(characterId)`, `opts.sharedTiers
  ?? createSharedTierLoader(repos, projectIds)()`, `opts.ownItems` or
  `repos.wardrobe.findByCharacterId(characterId, true)` with catch → WARN
  **`[WearablePool] Failed to read the character wardrobe; using shared tiers
  alone`** `{ characterId, context, error }` → `[]`; group origin per mount
  `{ scope: 'group', id, name }`; `group = readTier(groupMountPointIds,
  originOf)` catch → WARN **`[WearablePool] Failed to read the group wardrobe
  tier; skipping it`** `{ characterId, context, error }` → `[]`; own items
  re-tagged `{ ...item, characterId, origin: { scope: 'character', id:
  characterId, name: '' } }` (name EMPTY); then debug **`[WearablePool] Loaded
  wearable pool`** `{ characterId, ownCount, groupCount, projectCount,
  generalCount, reachable: byId.size, groupMountCount, projectMountCount,
  context }`.
- `:329-341` `componentGraph(pool, rootIds)` — BFS over `componentItemIds`,
  tolerates cycles / unknowns, insertion order = discovery order.
- `:349-362` `loadCastPools(repos, projectId, characterIds)` →
  `resolveProjectMountPointIds(projectId ?? null)`, ONE shared-tier loader,
  `loadWearablePool(…, { sharedTiers })` per character (NOT roster-gated — the
  operator reads).
- `wearable-pool.ts:27-40` `mergeWearableTiers(tiersWeakestFirst)` (drops
  archived per item as it merges) + `mergeWearablePool(shared, own)` =
  `mergeWearableTiers([shared, own])` — kept, client-safe; `wardrobe-
  container.ts:140-156` `mergeWardrobeTiers(tiers, { includeArchived })` is the
  client's nearest-first merge.

## 8. `lib/wardrobe/wear-ops.ts` (NEW, 132) + `wearable.ts` (NEW, 44)

- `wearable.ts:18-20` `archivedWearMessage(title)` → **`Item "${title}" is
  archived and cannot be worn`**; `:23-25` `wearRefusal(item)` → that or
  `null`; `:33-44` `newlyWornArchivedItems(items, currentSlots)` → items with
  `archivedAt` whose id is NOT in any slot of `currentSlots` (so `set_all`
  keeps an archived item already worn).
- `wear-ops.ts:30` `PutOnMode = 'wear' | 'replace' | 'add_to_slot'` (`equip`
  is the ROUTE's alias for `wear`); `:32-36` `WearableRef { itemId?; itemTitle?
  }`; `:38-40` `WearableResolution = { ok: true; item } | { ok: false; reason:
  'not_found' | 'archived' | 'slot'; message }`.
- `:43-48` `wardrobeItemNotFoundMessage(itemId, itemTitle)` → **`Wardrobe item
  not found${itemId ? \` with ID "${itemId}"\` : ''}${itemTitle ? \` with title
  "${itemTitle}"\` : ''}`** (v5 `tools/wardrobe_wear.rs:267` already carries
  the sentence).
- `:51-60` **`findInPool(pool, ref)`** — `itemId` → `pool.get` (miss falls
  through); then `itemTitle` → `pool.findByTitle`; archived items ARE found.
- `:66-86` **`resolveWearable(pool, ref, mode, slot?)`** — not found →
  `not_found`; `wearRefusal` → `archived`; `mode === 'add_to_slot' && slot &&
  !item.types.includes(slot)` → `slot` with **`Item "${title}" (types:
  ${types.join(', ')}) cannot be added to the "${slot}" slot`** (the route's
  old `Wardrobe item "<title>" does not cover the <slot> slot` is GONE —
  `chat_outfits.rs:778` in v5).
- `:88-92` `WearOutcome { slots; effect: 'layered' | 'replaced'; slotsAffected
  }`; `:95-116` **`wearItem(repos, chatId, pool, item, mode, slot, source: 'ui'
  | 'tool')`** → `applyDisplacement(repos, chatId, pool.characterId, { mode,
  item, slot, itemsById: pool.byId }, source)`; `add_to_slot` → `layered`,
  `[slot]`; else `effect = mode === 'replace' || item.replace ? 'replaced' :
  'layered'`, `slotsAffected = [...item.types]`.
- `:122-131` **`takeOffItem(repos, chatId, characterId, itemId, slots)`** → one
  `applyDisplacement({ mode: 'remove_from_slot', slot, itemId })` per slot (NO
  `source` argument — v4 passes none).
- Consumers: `outfit.ts` (`resolveWearable`, `wearItem`, `wearRefusal`,
  `newlyWornArchivedItems`, `loadWearablePool`, `loadCastPools`),
  `wardrobe-wear-handler.ts` (`resolveWearable`, `wearItem`),
  `wardrobe-create-handler.ts` (`wearItem` for `equip_now`),
  `wardrobe-take-off-handler.ts` (`findInPool`, `takeOffItem`),
  `wardrobe-{archive,read,update}-handler.ts` (`findInPool`).

## 9. `item-mutations.ts` (NEW, 234) + `composite-types.ts` + `archived-patch.ts` + `create-body.ts` + `folder-mutations.ts`

- `composite-types.ts:17-23` `unionTypes` (unchanged; v5 `wardrobe.rs:177`);
  `:31-37` NEW **`buildCompositeTypes(components, designated = [])`** →
  `WARDROBE_SLOT_TYPES.filter(s => set.has(s))` over `unionTypes(components)
  ∪ designated` — WIDEN, NEVER NARROW (bug 195).
- `archived-patch.ts:18-26` `archivedPatch(currentArchivedAt, archived, now)`
  — UNTOUCHED by the span; v5 `wardrobe.rs:1308-1322` is already the twin.
- `create-body.ts:41-58` `wardrobeItemFromCreateBody(body, characterId)` gains
  `:57` `folder: body.folder ?? ''`; `componentItemIds: Array.from(new Set(
  body.componentItemIds ?? []))`, `prose()` for `description` / `imagePrompt`
  / `appropriateness` (blank → `null`), `isDefault ?? false`, `replace ??
  false`, `migratedFromClothingRecordId: null`, `imageFileId: null`. (v5 has
  NO named twin — the api routes inline it; grep found no
  `wardrobe_item_from_create_body`.)
- `item-mutations.ts`:
  - `:42` `ComponentLookup = ReadonlyMap<string, WardrobeItem>`; `:49-65`
    **`componentLookupFor(repos, location, projectMountPointIds = [])`** —
    a character location → `loadWearablePool(repos, characterId,
    projectMountPointIds).byId`; a shared tier → General (`readGeneralWardrobe
    (true)`, skipped for General itself) then the location's own items
    (archived included; own folder wins on id).
  - `:68-83` `validateComponentRefs(lookup, ids)` → `{ components, missing }`
    deduped in order.
  - `:86-95` private `compositeTypes(lookup, componentItemIds, designated)` —
    no lookup → `[...designated]`; else `buildCompositeTypes(found,
    designated)`, EMPTY → `[...designated]`.
  - `:97-103` `CreateItemOptions { lookup?; preserve?: Pick<id, createdAt,
    updatedAt> & Partial<Pick<archivedAt, imageFileId,
    migratedFromClothingRecordId>> }`.
  - `:106-137` **`createItem(location, body, opts)`**: `fields =
    wardrobeItemFromCreateBody(body, location.characterId)`; `fields.folder =
    fields.folder ? await location.resolveFolder(fields.folder) : ''`;
    composite → `fields.types = compositeTypes(...)`; `now`; `item = {
    ...fields, id: preserve?.id ?? randomUUID(), archivedAt: preserve?.
    archivedAt ?? null, imageFileId: preserve?.imageFileId ?? null,
    migratedFromClothingRecordId: preserve?.… ?? null, createdAt, updatedAt
    }`; `location.create(item)`; debug **`[ItemMutations] Created wardrobe
    item`** `{ scope, mountPointId, itemId, composite, types, context }`.
  - `:140-142` `ItemPatch = Partial<Omit<WardrobeItem, 'id' | 'createdAt' |
    'updatedAt' | 'archivedAt'>> & { archived?: boolean }`.
  - `:149-182` **`updateItem(location, current, patch, { lookup })`**:
    `folder !== undefined` → `isSameFolder(fields.folder, current.folder)` →
    DELETE the key (moves nothing — a hand-made folder is never re-resolved),
    else `resolveFolder`; `archived !== undefined` → `archivedPatch(current.
    archivedAt, archived, now)` → `next.archivedAt` when non-null;
    `componentItemIds = fields.componentItemIds ?? current.componentItemIds`;
    if non-empty AND (`componentItemIds` or `types` is in the patch) →
    `next.types = compositeTypes(lookup, componentItemIds, fields.types ??
    current.types)` (so an edit restating neither leaves `types` alone, and a
    leaf's `types` are never touched); `location.update(current.id, next)`;
    debug **`[ItemMutations] Updated wardrobe item`** `{ scope, mountPointId,
    itemId, fields: Object.keys(next), found, context }`.
  - `:188-203` **`setItemArchived(location, current, archived)`** →
    `archivedPatch`; `null` → `{ item: current, changed: false }` (NO write);
    else `location.update(current.id, stamp)`, debug **`[ItemMutations]
    Wardrobe item archive state changed`** `{ scope, itemId, archivedAt,
    context }`, `{ item, changed: item !== null }` (the `wardrobe_archive`
    tool's bug-188 idempotence).
  - `:214-234` **`moveItemsToFolder(location, itemIds, folder)`** → `target =
    resolveFolder(folder)`; `uniqueIds`; `location.updateMany(ids → { folder:
    target }, { requireAll: true })`; debug **`[ItemMutations] Moved wardrobe
    items to folder`** `{ scope, mountPointId, folder: target, requested,
    moved, context }`.
- `folder-mutations.ts` (NEW, 81): `:25-41` `createWardrobeFolder(location,
  path)` → `resolveFolder(path)`; `''` → throw `WardrobeFolderPathError('A
  folder needs a name')`; `location.createFolder(resolved)`; debug
  **`[FolderMutations] Created wardrobe folder`** `{ scope, mountPointId,
  requested, folder, itemCount, context }`. `:44-62` `renameWardrobeFolder(
  location, path, name)` → `from = path.trim().replace(/^\/+|\/+$/g, '')`
  (matched AS STORED — hand-made spellings included); `location.renameFolder(
  from, normalizeWardrobeFolderName(name))`; debug **`[FolderMutations]
  Renamed wardrobe folder`** `{ scope, mountPointId, from, to, itemCount,
  context }`. `:65-81` `deleteWardrobeFolder(location, path)` → same trim;
  `location.deleteFolder`; debug **`[FolderMutations] Deleted wardrobe
  folder`** `{ scope, mountPointId, folder, parent, movedItems, movedFolders,
  context }`; returns `{ movedItems, movedFolders }` (the `parent` is logged,
  not returned).

## 10. The repository shrink, the owner store, the sanitiser, `store-file.ts`, the legacy rows

### 10.1 `wardrobe.repository.ts` (151 lines, was 563+)
- KEPT: `:48-59` `findByCharacterId(characterId, includeArchived = false)` →
  `getOverlaidWardrobeItems` with ERROR `Error finding wardrobe items by
  character ID` `{ characterId, includeArchived, error }` and RETHROW.
- NEW `:69-98` **`readSharedTiers(mountPointIds, includeArchived, originOf)`**
  → `[]` for none; per mount in order `readSharedWardrobe(mp, includeArchived)`
  tagged `{ ...item, origin: originOf(mp) }` into a `Map` (later mount wins);
  a throw → WARN **`Failed to read shared wardrobe tier; skipping`**
  `{ mountPointId, context: 'wardrobe', error }`; then debug **`Read shared
  wardrobe tiers`** `{ mountCount, itemCount, includeArchived, context }`.
  (v5's `archetype_wardrobe.rs:323-342` `merge_mounts` has the WARN but no
  origin tagging and no debug line.)
- KEPT as owner-hint ADAPTERS (`:105-150`): `create(data, options)` (mints
  `id` / `createdAt` / `updatedAt`, defaults `componentItemIds: []`, `replace:
  false`, `characterId ?? null`) → `createInMount(mountFor(characterId,
  'create'), item)`; `update(id, data, ownerCharacterId)` (strips
  id/createdAt/updatedAt) → `updateInMount`; `delete(id, ownerCharacterId)` →
  `deleteInMount`; `mountFor` → ERROR **`Wardrobe write has no resolvable
  folder`** `{ op, ownerCharacterId }` + throw **`Cannot ${op} wardrobe item:
  no Character Vault or Quilltap General mount is available. Wardrobe items are
  stored exclusively in the document store.`** (v5 `NO_MOUNT_MESSAGE`,
  `vault_wardrobe_public.rs:84-85`, already the suffix). `dbTarget: 'main'`
  kept for the job child's partitioner.
- DELETED: `findAll`, `findByCharacterIdRaw`, `findByIdForCharacter`,
  `findByIdsForCharacter`, `findWearablePoolForCharacter`, `findArchetypes`,
  `findArchetypesInMounts`, `findArchetypesInMountsAttributed` (+ its debug
  `Attributed group wardrobe read`), `findArchetypeById`, `archive` (INFO
  `Wardrobe item archived`), `unarchive` (INFO `Wardrobe item unarchived`),
  the three INFO `Wardrobe item created / updated / deleted in vault` lines
  and the three ERROR `Wardrobe create/update/delete has no resolvable vault
  mount; refusing SQL/DB fallback` lines, the constructor (no base class any
  more — the table is unknown to it).

### 10.2 `ensure-owner-store.ts` (NEW, 81) — `StoreOwnerKind = 'project' |
'group'`; `ensureOwnerOfficialStore(kind, ownerId, ownerName)` →
`ensureOfficialStore(config, ownerId, ownerName)` with the per-kind config
(`entityLabel`, `entityLabelCapitalized`, `entityIdLogKey: 'projectId' |
'groupId'`, prefix, `findByIdRaw`, `setOfficialMountPointId`, `findLinks`,
`link`, `isOwnStoreName`); `{ mountPointId, created } | null`. v5 twin:
`db/ensure_official_store.rs:86` `ensure_official_store<E: StoreEntity>` —
a `kind`-keyed wrapper is new.

### 10.3 `sanitize-file-name.ts` (NEW, client-safe) — `sanitizeFileName`
MOVED verbatim out of `character-vault.ts:267-270` (re-exported there);
v5 `vault_overlay.rs:641-677` is already the byte twin (JS whitespace, UTF-16
truncation). No behaviour change.

### 10.4 `store-file.ts` — the unique-path reservation (+86): `reservedBlobPaths`
(a module `Set` keyed `<mountPointId>:<path>`), `reserveUniqueBlobPath` loops
`resolveUniqueRelativePath(mp, desired, isReserved)` (the helper gains an
`isReserved` predicate, `bridge-path-helpers.ts:49-70`) until it claims a
path, debug **`storeMountFile: unique path claimed while resolving; trying
again`** `{ mountPointId, candidate }`; the reservation is released in a
`finally` once `linkBlobContent` lands. **A Node in-process concurrency guard
of the same class as `runSerialized`** — v5's writer task serialises every
store write (`services/mount_index/store_file.rs:137,382`), so the race is
unreachable and the debug line never fires. Recommend RULED NO-PORT with the
line recorded (the IMAGES lane owns `store_file.rs` — two pictures of one item
in the same second is its scenario).

### 10.5 `migrations/lib/legacy-wardrobe-rows.ts` (NEW, 68) —
`readLegacyWardrobeRows(characterId)`: `getRawDatabase()`; the table absent
from `sqlite_master` → `[]`; `SELECT * FROM "wardrobe_items" WHERE
"characterId" = ?` mapped to `WardrobeItem` with `types` / `componentItemIds`
via `JSON.parse` (non-array / bad JSON → `[]`), `isDefault === 1`, `replace:
false`, **`folder: ''`**, no `imagePrompt` / `imageFileId` keys. Consumers:
`lib/startup/refresh-vault-wardrobe.ts` and the `cutover-characters-to-vault`
migration ONLY. **v5 has neither consumer** (`db/wardrobe.rs` is the
`wardrobe_items` SQL repo ported as `wardrobe_tier2`, reached only from
`api/types.rs` / `api/engine.rs`); see §16 ruling R1.

## 11. `folder-view.ts` (SPA-side pure), `wardrobe-container.ts`, `slot-ops.ts`, `resolve-equipped.ts`

- `folder-view.ts` (NEW, 149, client-safe; NO server consumer → NO Rust twin;
  the SPA lane ports it to TS beside `apps/web/src/app/wardrobe/`): `:37`
  `OWN_TIER = 'own'`; `:65-68` `viewTierOf(item, manageable, characterView)` →
  `own` unless a character view's borrowed item → `encodeWardrobeContainer(
  containerForListedItem(item))`; `:71-76` `tierLabel(origin)` → `'General'`
  (null or general), `` `Project: ${name}` ``, `` `Group: ${name}` ``, else
  `origin.name || 'Quilltap General'`; `:79-84` `childOf(folder, current)`;
  `:92-120` `childFolderRows(current, items, tree = [])` — union of the
  server tree's direct children and the children the items imply (lower-cased
  dedupe, first spelling), count = the server's `itemCount` when it knows the
  folder else live items `isSameFolder`, sorted `localeCompare(…, { sensitivity:
  'base' })` by NAME; `:127-149` `tierRows(items, tierOf)` — one row per
  borrowed tier in first-seen order, `itemCount` of live items, `scope =
  origin?.scope ?? 'general'`.
- `wardrobe-container.ts` (+80 net): `:227-229` NEW `wardrobeCollectionAction
  Url(container, action)` → `` `${base}?action=${encodeURIComponent(action)}` ``;
  `:140-156` `mergeWardrobeTiers` doc; the rest unchanged (`GENERAL_WARDROBE_
  NAME`, `GENERAL_WARDROBE_ORIGIN`, `withOrigin`, `containerForListedItem`,
  `encode/decodeWardrobeContainer` — v5 `archetype_wardrobe.rs:53-130`).
- `slot-ops.ts` (NEW, 358) — `dissolve-bundles.ts` rebuilt around the four
  pure helpers (§1): `WearableNode`, `WearableLookup`, `DissolvedLeaf`,
  `WornBundle`, `analyzeComposite` → `CompositeAnalysis { leaves, cycles,
  truncated, unresolved }`, `dissolveCompositeToLeaves`, `layLeavesIntoSlots`,
  `dissolveCompositesInSlots(current, itemsById, only?)` → `{ slots,
  wornBundles }`, `wornBundlesFor`, `wearItemIntoSlots`, `replaceItemIntoSlots`,
  `addItemToSlot`, `DisplacementMode` (5 modes), `computeDisplacedSlots`,
  `wornBundlesForMode`. **The OUTFIT lane's** (v5 `dissolve_bundles.rs` +
  `wardrobe.rs:679-752`); recorded here only because its vocabulary (`isComposite`,
  `addIdToSlot`, `removeIdFromSlot`) is the keystone's §1 helpers. Also
  `expand-composites.ts` dropped its logger: the WARN `[expandComposites] Max
  depth reached, treating as leaf` is GONE (v5 never pinned it — no family
  reddens).
- `resolve-equipped.ts` (139 changed): `resolveEquippedOutfitForCharacter(pool,
  slots)` is now PURE over a `WearablePool` (`componentGraph(pool,
  equippedItemIds)`; debug `[resolveEquippedOutfitForCharacter] Equipped ids
  not in the wearable pool` `{ context, characterId, unresolvedCount }`; WARN
  `… Malformed composite graph; expansion truncated`); `describeEquippedOutfit
  TitleOnly(repos, characterId, slots, projectMountPointIds)` loads the pool.
  The OUTFIT lane's; the pool is its input.

## 12. The v4 jest tests the port mirrors (at the pin; `it`/`test` counts)

Keystone-owned (the C1 lane's unit mirrors): `__tests__/unit/lib/wardrobe/
pool.test.ts` **35** (precedence ×5, archived ×4, lookups ×11 incl.
`findByTitle` ×5 / `owns` ×3, project tier ×4, I/O shape ×4, degradation ×3,
`readSharedTiers` ×5); `location.test.ts` **13**; `folders.test.ts` **19**;
`wear-ops.test.ts` **12** (`findInPool` ×3, `resolveWearable` ×5 incl. the
`it.each` over three modes, `wearItem` ×3, `takeOffItem` ×1);
`item-mutations.test.ts` **26** (`buildCompositeTypes` ×2, `validateComponent
Refs` ×1, `createItem` ×5, `updateItem` widen ×4 + archived ×3, `setItemArchived`
×4, `componentLookupFor` ×3, folders ×4); `__tests__/unit/lib/database/
repositories/wardrobe-folder-writes.test.ts` **13** (`updateManyInMount` ×3,
`listFoldersInMount` ×1, `createFolderInMount` ×2, `renameFolderInMount` ×4,
`deleteFolderInMount` ×3 incl. the three-row `it.each` strays table);
`wardrobe-folder-read.test.ts` **4** (predicate ×2, recursive read ×2);
`vault-projection-recursive.test.ts` **9** (recursive ×7 incl. "does not
delete a file it just wrote when only the casing differs", flat callers
unchanged ×2); `vault-projection-preserve.test.ts` **3**;
`wardrobe-writes-archived.test.ts` **4** (`resolveWardrobeMount`);
`archived-survives-projection.test.ts` **8** (incl. "never sweeps a file that
is not an item — a note kept in a subfolder, or a half-written garment");
`archived-never-auditions.test.ts` **9** (incl. `buildWearablePool` ×3);
`cross-tier-component-roundtrip.test.ts` **8** (bug 187 end to end);
`store-file.test.ts` **15** (incl. "reserves a unique-suffix path so two
concurrent writes cannot both take it" — NO-PORT per §10.4);
`lib/schemas/__tests__/wardrobe.types.test.ts` **4** (unchanged). Adjacent,
other lanes': `slot-ops.test.ts` 37 (OUTFIT; renamed from
`dissolve-bundles.test.ts`), `resolve-equipped.test.ts` 14 (OUTFIT),
`routes/wardrobe-route-factory.test.ts` 77 (ROUTES), `wardrobe-handlers.test.ts`
33 + `wardrobe-handler-shared.test.ts` 4 + `wardrobe-tool-table.test.ts` 2
(TOOLS), `wardrobe/transfers/route.test.ts` 30 (ROUTES), `outfit-change-
effects.test.ts` 10 (TOOLS/OUTFIT), `item-draft.test.ts` 5 +
`wardrobe-browser.folders.test.tsx` 8 (SPA). DELETED in the span:
`wardrobe.repository.pool.test.ts` (−257), `mount-wardrobe-route-factory.
test.ts` (−732).

## 13. v5 today — per file, what exists and what the keystone changes

- **`crates/quilltap-core/src/vault_overlay.rs`** (2,769): `WardrobeItem`
  `:334-369` (Zod order; `.default()` keys materialised) — ADD `folder: String`
  after `archived_at` (`#[serde(default)]`-less, always emitted);
  `from_read_value :376-421` — read `folder` (absent → `''`);
  `WardrobeItemFromFile :1044-1079` — ADD `folder: String` after `archived_at`
  (v4 literal order); `parse_wardrobe_item_file :1093-1200` — derive `folder`
  from `doc.relative_path` via the new `folder_of_wardrobe_path`, emit the
  debug line on a frontmatter `folder` key; `is_wardrobe_item_document_path
  :1227-1238` — the doc comment says "DIRECTLY inside … not nested deeper" and
  the body `rest.contains('/') → false` is EXACTLY what flips (§6.1); ADD
  `is_wardrobe_item_document(relative_path, content)` +
  `has_stable_wardrobe_item_id(content)` (reuse `is_wardrobe_id_shaped`
  `:1085-1087` — the SAME `[0-9a-f-]{36}` shape) + `COMPONENT_UUID_REF` as a
  strict 8-4-4-4-12 matcher; `SeedArchetype :1260-1290` DELETE;
  `resolve_and_check_component_items :1322-~1440` — drop the `archetypes`
  parameter and both `arche_by_*` maps, split the unknown arm into the KEPT
  UUID (debug `… outside this folder; keeping ref` `{character_id,
  mount_point_id, item_id, component_ref}`) and the dropped slug (WARN
  retitled `… unknown component slug; dropping ref`, keys as today). Note the
  v5 doc at `:1340-1346` records dogfood #103 ("v5 did both drops in silence")
  — carry the why forward: the UUID arm is now a KEEP, not a silent drop.
  `sanitize_file_name :641-677` unchanged (v4 only moved its module).
  `build_wardrobe_item_file :2301` — does v4's `buildWardrobeItemFile` write
  `folder` into frontmatter? NO (never stored in frontmatter; the path is the
  truth) — no change here, but a pin to that effect belongs in
  `vault_wardrobe_emit_equivalence`.
- **`db/vault_read_overlay.rs`** `read_character_vault_wardrobe :522-600`:
  signature loses `seed_archetypes` + `fetch_archetypes` (both call sites —
  `db/wardrobe_read.rs:69-74`, `db/vault_wardrobe_public.rs:258-287`,
  `db/archetype_wardrobe.rs` (`read_shared_wardrobe`) — simplify); the read
  `:530-531` becomes `find_many_by_mount_points_in_folder_opts_or_empty(&mount,
  "Wardrobe", ".md", true)` (`db/doc_mount_documents.rs:482`, already exists
  with v4's `recursive`); the filter `:538` becomes
  `is_wardrobe_item_document_path(&doc.relative_path)` + the debug
  `Wardrobe read skipped non-item documents` `{characterId, mountPointId,
  skipped}`; the archetype block `:566-571` DELETED.
- **`db/vault_wardrobe_write.rs`** (152): `project_array_into_vault_folder
  :46-113` gains a `ProjectArrayOptions { preserve_file_names, recursive:
  Option<&[&str]> /* exclude_subfolders */, owns_file: Option<&dyn
  Fn(&str, Option<&str>) -> bool> }` (the current `preserve_file_names: &[&str]`
  parameter folds into it); the four flat callers
  (`db/vault_character_write.rs:346,364`, `db/vault_character_update.rs:269,289`,
  `wardrobe_instructions.rs` does NOT call it — the grep shows only the four)
  pass flat options; the recursive listing via `_opts_or_empty(.., true)`;
  `existing_paths` → `(relative_path, content)` so `owns_file` can parse; the
  per-folder `ensure_folder_path` (`db/doc_mount_file_links.rs:1563`) with the
  `occupied` skip (the comment at `:63-65` "the write primitive already
  find-or-creates the folder segments … Match: write-only" stays true for
  the FLAT path; recursive mode must ensure EMPTY projection targets and
  fire the debug `Recursive vault projection ensured subfolders`);
  `written_paths` lower-cased + the lower-cased sweep compare; `-n` suffix
  with `has_ext = dot > slash`. `project_vault_wardrobe :119-152` — mapper
  `join_folder(&item.folder, format!("{}.md", sanitize_file_name(title)))`,
  options `recursive: ["images"]`, `owns_file: is_wardrobe_item_document`.
- **`db/vault_wardrobe_public.rs`** (684): PROMOTE `WardrobeScope :178-182`
  (ADD `Group`) and `WardrobeLocation :186-194` to `pub` and rename the latter
  `WardrobeMount` (v4's name for this struct; the LOCATION is the richer thing
  in §15.2); `resolve_wardrobe_mount :200-228` → `pub`; `read_mount_items
  :257-287` → `pub`, no seeding; `create_at_location :427-445` →
  `create_in_mount` with `safe_folder`; `update_at_location :446-473` →
  `update_many_in_mount(mount, patches, UpdateManyOptions)` + the one-patch
  wrapper; `delete_at_location :475-489` → `delete_in_mount`; `WardrobePatch
  :91-141` gains `folder: Option<String>` (apply through `safe_folder`);
  `build_cycle_peers` → character scope through `load_wearable_pool(…,
  own_items: current)`, shared → `read_general_wardrobe(true)`, General →
  folder alone, with the WARN `Wardrobe cycle check could not load peer tiers;
  checking the folder alone`; the three `*_project_wardrobe_item` fns
  (`:511, :547, :583`) KEPT as thin wrappers over `mount_wardrobe_location(mp,
  WardrobeScope::Project)` for the lanes (callers: `api/projects.rs:1602,1776`,
  `api/groups.rs:1310,1475`, `services/wardrobe_transfers.rs`,
  `services/backup/restore/orchestrator.rs`, `services/wardrobe_container.rs:
  219`); `create_vault_wardrobe_item :496` / `update_vault_wardrobe_item :532`
  / `delete_vault_wardrobe_item :569` KEPT (the owner-hint adapters v4 kept);
  NEW: `list_folders_in_mount`, `list_folder_paths_in_mount`,
  `folder_under_wardrobe`, `find_stored_folder`, `create_folder_in_mount`,
  `rename_folder_in_mount`, `delete_folder_in_mount` over
  `db/doc_mount_folders.rs:117,182` + `db/database_store.rs:504,537`
  (`DbStoreErrorCode::{NotFound, Conflict, NotEmpty}` `:65-69`) +
  `doc_mount_file_links::ensure_folder_path :1563` + `find_by_mount_point_id`
  on the links repo (strays). The module header `:30-40` ("All three tiers")
  → four.
- **`db/wardrobe_read.rs`** (206): KEEP `find_by_character_id :53-98` (drop
  the seeding closure); DELETE `find_wearable_pool_for_character :124-133`,
  `find_by_ids_for_character :141-185`, `find_by_id_for_character :191-206`
  (callers in LANE files: `services/outfit_selections.rs:294`,
  `tools/wardrobe_list.rs`, `api/chat_outfits.rs`, `services/chat_create.rs`,
  `tools/wardrobe_{read,shared,update}.rs`, `api/characters.rs`,
  `db/memories_read.rs` — the last is a MEMORY read's use of
  `find_by_id_for_character`: check whether it is the memories repo's own
  same-named method (it is — `memories-read.ts:142` — not the wardrobe's);
  harness `archived_wearer_read_guard.rs`, `wardrobe_public_read_equivalence.rs`,
  `memories_read_equivalence.rs`); ADD `read_shared_tiers(docs, mount_point_ids,
  include_archived, origin_of)`.
- **`db/archetype_wardrobe.rs`** (678): KEEP `WardrobeOrigin` family `:53-130`,
  `read_shared_wardrobe :157`, `read_general_wardrobe :191`,
  `ensure_general_wardrobe_folder :233`, `ensure_shared_wardrobe_folder :247`;
  DELETE (v4 twins gone) `read_project_wardrobe :205`, `read_group_wardrobe
  :221`, `ensure_project_wardrobe_folder :255`, `ensure_group_wardrobe_folder
  :263`, `find_archetypes_in_mounts :308`, `merge_mounts :323`,
  `find_archetypes_in_mounts_attributed :365` (+ `merge_groups_attributed` and
  its debug `Attributed group wardrobe read`), `find_archetypes :423`,
  `find_archetype_by_id :450`, `OwnerStore :109` (check callers). Callers in
  lane files: `api/characters.rs` (the `?scope=group` read →
  `group_locations_for_character` + `read_shared_tiers`), `api/wardrobe.rs`,
  `api/chat_outfits.rs`, `services/outfit_selections.rs:656,680`,
  `tools/wardrobe_{create,shared}.rs`, `services/wardrobe_transfers.rs:378-550`,
  `services/wardrobe_container.rs:27-30`.
- **`wearable_pool.rs`** (172): KEEP `is_archived_truthy :18-25`,
  `merge_wearable_pool :45-` (v4 keeps `wearable-pool.ts`); GROW into the
  pool (§15.1).
- **`wardrobe_tiers.rs`** (397): `SharedWardrobeTiers :55-92` → v4's
  `WardrobeTierMounts` (same two fields; keep the name, add a type alias);
  `resolve_shared_wardrobe_tiers_for_chat :110-130` — v4 DELETED
  `shared-tiers.ts`; its body is `load_wearable_pool`'s (the eleven callers are
  the TOOLS / OUTFIT lanes' — keep the fn as a shim for one round, see §16);
  `resolve_project_tier_for_chat :145-185` → MOVE into the pool module with
  the `[WearablePool]` tag; `shared_wardrobe_tiers_for_character :193` →
  `load_cast_pools` / `load_wearable_pool(…, Some(project_ids))`.
- **`wardrobe.rs`** (1,359): ADD `build_composite_types(components, designated)`
  beside `union_types :177`; ADD `is_composite`, `add_id_to_slot`,
  `remove_id_from_slot`, `equipped_slots_equal` over `Slots :529-630` (v5's
  `dissolve_bundles::is_bundle :103` is the pre-rename twin of `is_composite`
  — the OUTFIT lane re-points it); `archived_patch :1308-1322` unchanged;
  `wear_item_into_slots :679` / `replace_item_into_slots :703` /
  `add_item_to_slot :723` — the OUTFIT lane's (`slot-ops.ts`).
- **`wardrobe_instructions.rs`** (241): `WARDROBE_INSTRUCTIONS_FILENAME :34`,
  `is_wardrobe_instructions_file_name :40` — consumed by the predicate;
  unchanged. (v4's `instructions.md`-at-any-depth exclusion needs only the
  LAST segment passed in.)
- **`services/wardrobe_container.rs`** (431): `resolve_wardrobe_container
  :75-95` (+ the debug `[WardrobeContainer] Resolved wardrobe container`) → v4
  deleted `resolve-container.ts`; becomes `resolve_wardrobe_location` with
  `ResolveWardrobeLocationOptions { ensure }` and the `[WardrobeLocation]`
  line (NB: v4's container resolver ALWAYS ensured a project/group store;
  the location only ensures on `ensure: true` — a GET through the location is
  now read-only); `resolve_container_mount_point_id :195-225` → the
  location's `writable_mount_point_id` (the IMAGES lane's `WardrobeItemHome
  :230-` keeps its shape — v4 `item-images.ts:82-102` builds the home FROM
  `resolveWardrobeLocation`, `resolveMount: () => location.writableMountPointId()`).
  Its only external caller is itself + the images route/job
  (`api/wardrobe_item_images.rs`, `services/wardrobe_item_image_job.rs`).
- **`services/mount_index/store_file.rs`** `:137,382` — §10.4 NO-PORT
  candidate (IMAGES lane).
- **`db/wardrobe.rs`** (258, the `wardrobe_items` SQL repo, `wardrobe_tier2`)
  — v4's repository no longer knows the table; see ruling R1.
- **`tools/wardrobe_shared.rs`** `resolve_wardrobe_item_across_tiers :100`,
  `hydrate_component_graph :225`, `lookup_for_bundle :294` — the TOOLS lane
  replaces them with `find_in_pool` / `pool.by_id` / `component_graph`
  (v4 deleted `resolveWardrobeItemAcrossTiers` and `hydrate-components.ts`).
- **`services/wardrobe_transfers.rs`** `resolve_source_item :328-421` probes
  personal `:347` → PROJECT `:378` → General `:421` (bug 192's wrong order:
  v4 now goes character → GROUP → project → General through
  `groupLocationsForCharacter`); `resolve_destination :613` provisions through
  `ensure_project_wardrobe_folder` — the ROUTES/TRANSFERS lane's, on the
  location + `ensure: true`.

## 14. Harness families / oracle cases / recipes / fixtures — and what reddens or DIES at the pin

Recipes live in each test's `//!` header (regen lines with `QT_FIXTURE_*` /
`QT_ORACLE_*` envs; `recipe_sweep.py:538` reads the `.ts` case header too);
fixtures are BUILT to `/tmp` by `harness/oracle/fixtures/build-*.ts` from the
committed `*.json` seeds (no committed `.db`). Every wardrobe seed today plants
FLAT `Wardrobe/<title>.md` files; the only nested `Wardrobe/x/y` paths in the
corpus are `Wardrobe/images/<id>/…` pictures (`wardrobe-item-images-routes.json`,
`uuid-remap-corpus.json`, `characters-scaffold-tier2.json`, the
`vault-wardrobe-item-file.ts` case `:135` `Wardrobe/images/x/pic.webp`).

**Two classes of trouble on a pin regen — distinguish them in the orders:**

(A) **IMPORT-DEAD** — the oracle case or fixture builder imports a module /
method v4 DELETED, so it fails to LOAD at the pin (not a diff, a crash):
- `cases/wardrobe-public-read.ts:81-85` → `repos.wardrobe.findByIdForCharacter`
  (DELETED) — `wardrobe_public_read_equivalence` must be re-aimed at
  `findByCharacterId` + `loadWearablePool(...).get(id)` (the id-then-archetype
  fallback is now the pool's `byId`).
- `cases/wardrobe-tier2.ts` + `fixtures/build-wardrobe-fixture.ts:13` →
  `findByCharacterIdRaw` / the SQL base repo (DELETED) — `wardrobe_tier2_
  equivalence` (R1).
- `cases/dissolve-bundles.ts:37` → `@/lib/wardrobe/dissolve-bundles` (DELETED;
  → `slot-ops.ts`) — `dissolve_bundles_equivalence` (OUTFIT lane); also
  `cases/composed-outfits.test.ts:5` mentions `isBundle` from it (check the
  import).
- `cases/project-roster-access.test.ts:236` → `@/lib/wardrobe/shared-tiers`
  (DELETED; → `pool.ts` `resolveProjectTierForChat`) —
  `project_roster_access_equivalence` (KEYSTONE: the pool owns this now).
- `cases/wardrobe-tools.ts:228` → `repos.wardrobe.findArchetypes(true)`
  (DELETED; → `readSharedTiers` / `loadWearablePool`) — `wardrobe_tools_
  equivalence` (TOOLS lane).
- Fixture builders importing `@/lib/mount-index/project-wardrobe` /
  `group-wardrobe` / `ensure-group-store` (DELETED): `build-chat-create-
  capstone.ts:415`, `build-chat-dialogs-fixture.ts:874`, `build-groups-projects-
  fixture.ts:562`, `build-project-roster-access-fixture.ts:270`,
  `build-wardrobe-item-image-generation-fixture.ts:238`, `build-wardrobe-item-
  images-fixture.ts:270-271`, `build-wardrobe-routes-fixture.ts:272,298`,
  `build-wardrobe-tools-fixture.ts:269`, `build-wardrobe-transfers-fixture.ts:
  326,336` — each `ensureProjectWardrobeFolder(id)` becomes
  `ensureOwnerOfficialStore('project', id, name)` + `ensureSharedWardrobe
  Folder(mountPointId)` (or `resolveWardrobeLocation('project', id, repos,
  userId, { ensure: true })`). **This is a cross-lane shared edit — the
  keystone should land ALL the builder re-aims in one commit so every lane's
  regen loads.** (`cases/query-param-semantics.test.ts:493-495` stubs
  `findArchetypes` on a fake repo — harmless, but the stub names a dead
  method; tidy.)

(B) **REDDENS** (loads, diffs):
- `vault_wardrobe_item_file_equivalence` (case `vault-wardrobe-item-file.ts`):
  the predicate over nested `.md` flips false→true; parsed items gain
  `folder`; add nested / `images/` / deep-`instructions.md` rows.
- `vault_frontmatter_parsers_equivalence`, `vault_wardrobe_read_equivalence`
  (case `:12` says "seeding (findArchetypes) returns [] and the seed is a
  verified no-op" — the seed path is gone, output gains `folder: ''`),
  `vault_legacy_wardrobe_equivalence` (`folder: ''` via the Zod default, §1),
  `vault_wardrobe_public_equivalence` (`folder` on every read-back; the
  `resolveWardrobeMount` lines; the retitled write lines are DEBUG now),
  `vault_wardrobe_write_equivalence` (likely byte-equal for flat seeds, but
  re-pin: the lower-cased sweep compare and a seeded non-item note under
  `Wardrobe/` that is now NOT swept), `vault_wardrobe_emit_equivalence` (pin
  that `folder` never reaches frontmatter), `vault_component_leaves_
  equivalence` (no logger in `expand-composites`), `doc_mount_fallback_sites_
  census` (a new recursive read call site), `wardrobe_wear_import_remap_
  equivalence` / `backup_uuid_remap_equivalence` / `system_export_equivalence`
  (CARRIERS: the recursive predicate), `wardrobe_transfers_tier2_equivalence`
  (ROUTES: group before project, `folder`), `group_wardrobe_routes_equivalence`
  / `projects_routes_equivalence` / `chat_cast_routes_equivalence` (ROUTES /
  OUTFIT), `wardrobe_tools_*` (TOOLS), `wardrobe_item_images_tier2_
  equivalence` / `wardrobe_item_image_generation_tier3_equivalence` (IMAGES —
  the home through the location), `archived_wearer_read_guard` (a v5-internal
  guard over the deleted finders — rewrite over the pool).
- UNTOUCHED by this area: `tiered_mount_pool_equivalence`,
  `wardrobe_wear_stats_*`, `wardrobe_wear_history_equivalence`,
  `wardrobe_instructions_*`, `provisioning_equivalence` (no DDL).

**NEW families the keystone owes** (tier-1 exact against real v4 modules, the
corpora lifted from §12's tests): `wardrobe_folders_equivalence`
(`folders.ts` — 19 rules incl. `resolveWardrobeFolder` over an existing list),
`wearable_pool_equivalence` (`buildWearablePool` pure: precedence, archived
per tier, `findByTitle`, `getMany`, `owns`, `componentGraph`),
`wear_ops_equivalence` (`resolveWearable` sentences × modes),
`composite_types_equivalence` (`buildCompositeTypes` widen); tier-2 real-DB:
`wardrobe_location_tier2` (resolve × 4 scopes × ensure, the tombstone, the
`[WardrobeLocation]` line), `wardrobe_folder_writes_tier2` (`updateMany`
requireAll, list/create/rename/delete with the stamp-before-move and the
three 409 sentences, dumping `doc_mount_folders` + `doc_mount_documents`
paths lower-cased), `vault_projection_recursive_tier2` (the nine
`vault-projection-recursive.test.ts` beats incl. the case-only rewrite),
`item_mutations_tier2` (`createItem` / `updateItem` / `setItemArchived` /
`moveItemsToFolder` read-back), plus the `pool.test.ts` load/degradation
beats as a tier-2 over a four-tier fixture (character vault + group store +
project store + General, with an archived own copy shadowing a live shared
one).

## 15. The proposed C1 contract (Rust spellings the five lanes compile against)

Conventions: crate-root modules stay flat (as `wardrobe.rs`, `wardrobe_tiers.rs`,
`wearable_pool.rs`, `dissolve_bundles.rs` are); connection-holding code lives
under `db/`; `Value`-shaped items stay `serde_json::Value` where v5 already
reads them that way (the pool shadows READ items), typed `WardrobeItem` where
v5 already writes them (`vault_overlay::WardrobeItem`). Log lines keep v4's
bytes and tags; every new fn carries v4's *why*-comment.

### 15.1 `crates/quilltap-core/src/wearable_pool.rs` (grows; v4 `pool.ts` + `wearable-pool.ts`)
```rust
pub type WardrobeTierMounts = crate::wardrobe_tiers::SharedWardrobeTiers; // same two fields
pub struct SharedTierItems { pub general: Vec<Value>, pub project: Vec<Value> } // origin-tagged, archived included
pub struct WearablePool {
    pub character_id: String,
    pub tiers: WardrobeTierMounts,
    /* private */ by_id: IndexMap<String, Value>, own_ids: HashSet<String>,
    layers_weakest_first: [Vec<Value>; 4], wearable_cache: OnceCell<Vec<Value>>,
}
impl WearablePool {
    pub fn by_id(&self) -> &IndexMap<String, Value>;          // v4 `byId` (insertion order = shadow order)
    pub fn wearable(&self) -> &[Value];                        // archived dropped PER TIER before shadowing
    pub fn get(&self, id: &str) -> Option<&Value>;
    pub fn get_many(&self, ids: &[String]) -> Vec<&Value>;     // order asked; unknowns + dupes skipped
    pub fn find_by_title(&self, title: &str) -> Option<&Value>;// own first (archived incl.), then wearable shared
    pub fn owns(&self, item: &Value) -> bool;                  // characterId == self && own_ids.contains(id)
}
pub struct LoadWearablePoolOptions<'a> {
    pub operator: bool, pub chat_id: Option<&'a str>,
    pub shared_tiers: Option<&'a SharedTierLoader>, pub own_items: Option<&'a [Value]>,
}
pub fn build_wearable_pool(character_id: &str, tiers: WardrobeTierMounts,
    own: Vec<Value>, group: Vec<Value>, project: Vec<Value>, general: Vec<Value>) -> WearablePool; // pure
pub fn load_shared_tiers(main: &Connection, mount: &Connection,
    project_mount_point_ids: &[String], project_origin: Option<&WardrobeOrigin>) -> SharedTierItems;
pub struct SharedTierLoader { /* OnceCell<SharedTierItems> + the WARN on failure */ }
impl SharedTierLoader { pub fn new(project_mount_point_ids: &[String]) -> Self;
    pub fn get(&self, main: &Connection, mount: &Connection) -> &SharedTierItems; }
pub fn resolve_project_tier_for_chat(main: &Connection, mount: &Connection, chat_id: Option<&str>,
    character_id: Option<&str>, operator: bool) -> Vec<String>; // MOVED from wardrobe_tiers.rs; `[WearablePool]` tag
pub fn load_wearable_pool(main: &Connection, mount: &Connection, character_id: &str,
    project_mount_point_ids: Option<&[String]>, opts: LoadWearablePoolOptions) -> WearablePool; // never fails
pub fn component_graph(pool: &WearablePool, root_ids: &[String]) -> IndexMap<String, Value>;
pub fn load_cast_pools(main: &Connection, mount: &Connection, project_id: Option<&str>,
    character_ids: &[String]) -> IndexMap<String, WearablePool>;
// kept: is_archived_truthy, merge_wearable_pool (+ merge_wearable_tiers(tiers_weakest_first))
```
Depends on `db::wardrobe_read::{find_by_character_id, read_shared_tiers}`,
`db::tiered_mount_pool::{resolve_group_mounts_for_character,
resolve_project_mount_point_ids}`, `project_roster_access::roster_gated_
project_id`, `db::archetype_wardrobe::{WardrobeOrigin, general_wardrobe_origin}`.
`origin` is written into each `Value` as v4 does (`{ ...item, origin }`, key
LAST — `with_origin` `archetype_wardrobe.rs:125`).

### 15.2 `crates/quilltap-core/src/db/wardrobe_location.rs` (NEW; v4 `location.ts`) — the owner of `WardrobeMount` is `db/vault_wardrobe_public.rs` (promoted, §13)
```rust
pub use crate::db::vault_wardrobe_public::{WardrobeMount, WardrobeScope};   // scope: Character|Group|Project|General
pub struct WardrobeLocation { pub scope: WardrobeScope, pub id: Option<String>, pub mount_point_id: String,
    pub character_id: Option<String>, pub origin: WardrobeOrigin, mount: WardrobeMount }
pub struct ResolveWardrobeLocationOptions { pub ensure: bool }
pub fn location_key(scope: WardrobeScope, mount_point_id: &str) -> String;         // "scope:mount"
pub fn location_for_mount(mount: WardrobeMount, id: Option<&str>, origin: WardrobeOrigin) -> WardrobeLocation;
pub fn resolve_wardrobe_location(main: &Connection, mount: &Connection, scope: WardrobeScope,
    id: Option<&str>, user_id: &str, opts: ResolveWardrobeLocationOptions) -> Result<Option<WardrobeLocation>, DbError>;
pub fn group_locations_for_character(main: &Connection, mount: &Connection, character_id: &str) -> Vec<WardrobeLocation>;
impl WardrobeLocation {
    pub fn read_items(&self, main: &Connection, mount: &Connection, include_archived: bool) -> Result<Vec<WardrobeItem>, DbError>;
    pub fn find_item(&self, main, mount, id: &str) -> Result<Option<WardrobeItem>, DbError>;
    pub fn create(&self, main, mount, item: &WardrobeItem) -> Result<WardrobeItem, WardrobeLocationError>;
    pub fn update(&self, main, mount, id: &str, patch: &WardrobePatch) -> Result<Option<WardrobeItem>, WardrobeLocationError>;
    pub fn update_many(&self, main, mount, patches: &[(String, WardrobePatch)], opts: UpdateManyOptions) -> Result<Vec<WardrobeItem>, WardrobeLocationError>;
    pub fn delete(&self, main, mount, id: &str) -> Result<bool, WardrobeLocationError>;
    pub fn list_folders(&self, main, mount) -> Result<Vec<WardrobeFolder>, DbError>;
    pub fn resolve_folder(&self, mount, requested: &str) -> Result<String, WardrobeFolderPathError>;
    pub fn create_folder(&self, main, mount, path: &str) -> Result<WardrobeFolder, WardrobeLocationError>;
    pub fn rename_folder(&self, main, mount, path: &str, new_name: &str) -> Result<WardrobeFolder, WardrobeLocationError>;
    pub fn delete_folder(&self, main, mount, path: &str) -> Result<DeletedFolder, WardrobeLocationError>;
    pub fn writable_mount_point_id(&self, main: &Connection) -> Result<String, DbError>; // CharacterArchived for the tombstone
}
pub struct DeletedFolder { pub moved_items: usize, pub moved_folders: usize, pub parent: String }
pub struct UpdateManyOptions { pub require_all: bool }
pub enum WardrobeLocationError { CharacterArchived { character_id: String }, NoLinkedVault { character_id: String } /* "Character {id} has no linked vault" */,
    Cycle(String), FolderPath(WardrobeFolderPathError), FolderNotFound { folder: String }, FolderConflict { folder: String, message: String },
    ItemNotInLocation { item_ids: Vec<String> }, Db(DbError) }
impl WardrobeLocationError { pub fn message(&self) -> String /* v4's sentence */; }
```
Writes re-resolve a character's mount (`resolve_wardrobe_mount`) at write time
(the tombstone); `WardrobePatch` (promoted from `vault_wardrobe_public.rs:91`,
+`folder`) replaces v4's `Partial<WardrobeItem>` and by construction cannot
carry `id` / `createdAt` / `updatedAt`. The mount-level primitives it calls
(`create_in_mount`, `update_many_in_mount`, `delete_in_mount`,
`list_folders_in_mount`, `list_folder_paths_in_mount`, `create_folder_in_mount`,
`rename_folder_in_mount`, `delete_folder_in_mount`, `read_mount_items`,
`mount_wardrobe_location(mp, scope)`) stay `pub` in `db/vault_wardrobe_public.rs`
(v4's `wardrobe-writes.ts`), each with its `Wardrobe … in folder` DEBUG line.
`ensure` provisions through `db::ensure_official_store::ensure_owner_official_
store(main, mount, kind: StoreOwnerKind, owner_id, owner_name) -> Result<Option<
EnsuredStore>, DbError>` (NEW kind-keyed wrapper; `StoreOwnerKind { Project,
Group }`) then `archetype_wardrobe::ensure_shared_wardrobe_folder`.

### 15.3 `crates/quilltap-core/src/wardrobe_folders.rs` (NEW, pure; v4 `folders.ts`)
```rust
pub const WARDROBE_FOLDER_MAX_DEPTH: usize = 8;
pub const WARDROBE_RESERVED_FOLDER_SEGMENTS: &[&str] = &["images"];
#[derive(Debug, Clone, PartialEq)] pub struct WardrobeFolderPathError(pub String);   // the 400 sentence
pub struct WardrobeFolder { pub path: String, pub name: String, pub item_count: usize } // serde: path,name,itemCount
pub fn normalize_wardrobe_folder_path(input: Option<&str>) -> Result<String, WardrobeFolderPathError>;
pub fn normalize_wardrobe_folder_path_or_root(input: Option<&str>) -> String;
pub fn normalize_wardrobe_folder_name(input: Option<&str>) -> Result<String, WardrobeFolderPathError>;
pub fn is_safe_wardrobe_folder_path(path: &str) -> bool;
pub fn resolve_wardrobe_folder(existing: &[String], requested: Option<&str>) -> Result<String, WardrobeFolderPathError>;
pub fn is_valid_wardrobe_folder_path(path: &str) -> bool;
pub fn folder_segments(folder: &str) -> Vec<&str>;
pub fn folder_of_wardrobe_path(relative_path: &str) -> String;
pub fn wardrobe_folder_store_path(folder: &str) -> String;
pub fn parent_folder(folder: &str) -> &str;  pub fn folder_name(folder: &str) -> &str;
pub fn join_folder(parts: &[&str]) -> String;
pub fn is_same_folder(a: Option<&str>, b: Option<&str>) -> bool;
pub fn is_under_folder(folder: &str, ancestor: &str) -> bool;
pub fn rebase_folder(folder: &str, from_ancestor: &str, to_ancestor: &str) -> String;
pub fn folder_crumb(folder: &str) -> String;      // " › "
pub fn not_found_message(folder: &str) -> String;                // "Wardrobe folder not found: {}"
pub fn conflict_default_message(folder: &str) -> String;         // "A wardrobe folder named \"{}\" already exists"
pub fn not_in_location_message(item_ids: &[String]) -> String;   // "Not in this wardrobe (shared items are filed where they live): a, b"
pub const FOLDER_SCHEMA_MESSAGE: &str = "Wardrobe folder must be a clean path of at most 8 segments, none named \"images\"";
```
`to_lowercase` everywhere v4 calls `toLowerCase()` (byte-identical for JS per
Phase 1); `split(/[\\/]/)`; `trim` = `jsstr::js_trim`. Uses
`vault_overlay::sanitize_file_name`.

### 15.4 `crates/quilltap-core/src/wear_ops.rs` (NEW) + `wearable.rs` (NEW, pure)
```rust
// wearable.rs
pub fn archived_wear_message(title: &str) -> String;                         // Item "{}" is archived and cannot be worn
pub fn wear_refusal(item: &Value) -> Option<String>;                         // archivedAt truthy → the message
pub fn newly_worn_archived_items<'a>(items: &'a [Value], current_slots: Option<&Slots>) -> Vec<&'a Value>;
// wear_ops.rs
pub enum PutOnMode { Wear, Replace, AddToSlot }   impl PutOnMode { pub fn as_str(self) -> &'static str }
pub struct WearableRef<'a> { pub item_id: Option<&'a str>, pub item_title: Option<&'a str> }
pub enum WearableRefusal { NotFound, Archived, Slot }   // v4 `reason`
pub enum WearableResolution<'p> { Ok(&'p Value), Refused { reason: WearableRefusal, message: String } }
pub fn wardrobe_item_not_found_message(item_id: Option<&str>, item_title: Option<&str>) -> String;
pub fn find_in_pool<'p>(pool: &'p WearablePool, r: WearableRef) -> Option<&'p Value>;
pub fn resolve_wearable<'p>(pool: &'p WearablePool, r: WearableRef, mode: PutOnMode, slot: Option<&str>) -> WearableResolution<'p>;
pub enum WearEffect { Layered, Replaced }
pub struct WearOutcome { pub slots: Slots, pub effect: WearEffect, pub slots_affected: Vec<String> }
pub fn wear_item(main: &Connection, chat_id: &str, pool: &WearablePool, item: &Value, mode: PutOnMode,
    slot: Option<&str>, source: WearSource /* Ui | Tool */) -> Result<WearOutcome, DbError>;   // over the OUTFIT lane's apply_displacement
pub fn take_off_item(main: &Connection, chat_id: &str, character_id: &str, item_id: &str, slots: &[String]) -> Result<(), DbError>;
```
`wear_item` / `take_off_item` call `services::outfit_displacement::apply_
displacement` (the v5 twin of `applyDisplacement` — the OUTFIT lane owns its
rewrite onto `slot-ops`; the keystone lands the two fns against TODAY's
`apply_displacement` signature and the OUTFIT lane re-points them: a
one-line meeting point, state it in both orders).

### 15.5 `crates/quilltap-core/src/services/wardrobe_item_mutations.rs` (NEW; v4 `item-mutations.ts` + `create-body.ts`) + `services/wardrobe_folder_mutations.rs` (NEW)
```rust
pub type ComponentLookup = IndexMap<String, Value>;
pub fn component_lookup_for(main, mount, location: &WardrobeLocation, project_mount_point_ids: &[String]) -> Result<ComponentLookup, DbError>;
pub fn validate_component_refs<'l>(lookup: &'l ComponentLookup, ids: &[String]) -> (Vec<&'l Value>, Vec<String>); // (components, missing)
pub struct WardrobeCreateBody { pub title, pub description: Option<String>, pub image_prompt, pub types: Vec<String>, pub appropriateness,
    pub is_default: Option<bool>, pub component_item_ids: Option<Vec<String>>, pub replace: Option<bool>, pub folder: Option<String> }
pub fn wardrobe_item_from_create_body(body: &WardrobeCreateBody, character_id: Option<&str>) -> WardrobeItem /* id/timestamps blank */;
pub struct PreservedIdentity { pub id: String, pub created_at: String, pub updated_at: String,
    pub archived_at: Option<Option<String>>, pub image_file_id: Option<Option<String>>, pub migrated_from_clothing_record_id: Option<Option<String>> }
pub struct CreateItemOptions<'a> { pub lookup: Option<&'a ComponentLookup>, pub preserve: Option<&'a PreservedIdentity> }
pub fn create_item(main, mount, location: &WardrobeLocation, body: &WardrobeCreateBody, opts: CreateItemOptions) -> Result<WardrobeItem, WardrobeLocationError>;
pub struct ItemPatch { /* every WardrobePatch field EXCEPT archived_at */ pub archived: Option<bool>, pub folder: Option<String>, … }
pub fn update_item(main, mount, location, current: &WardrobeItem, patch: &ItemPatch, lookup: Option<&ComponentLookup>) -> Result<Option<WardrobeItem>, WardrobeLocationError>;
pub struct ArchiveOutcome { pub item: Option<WardrobeItem>, pub changed: bool }
pub fn set_item_archived(main, mount, location, current: &WardrobeItem, archived: bool) -> Result<ArchiveOutcome, WardrobeLocationError>;
pub fn move_items_to_folder(main, mount, location, item_ids: &[String], folder: &str) -> Result<Vec<WardrobeItem>, WardrobeLocationError>;
// wardrobe_folder_mutations.rs
pub fn create_wardrobe_folder(main, mount, location, path: &str) -> Result<WardrobeFolder, WardrobeLocationError>;
pub fn rename_wardrobe_folder(main, mount, location, path: &str, name: &str) -> Result<WardrobeFolder, WardrobeLocationError>;
pub fn delete_wardrobe_folder(main, mount, location, path: &str) -> Result<(usize, usize) /* movedItems, movedFolders */, WardrobeLocationError>;
```
`build_composite_types(components: impl Iterator<Item = &[String]>, designated:
&[String]) -> Vec<String>` goes in `wardrobe.rs` beside `union_types` (§13),
with `is_composite(component_item_ids: &[String]) -> bool`, `add_id_to_slot(
&Slots, slot, id) -> Slots`, `remove_id_from_slot(&Slots, slot, Option<&str>)
-> Slots`, `equipped_slots_equal(&Slots, &Slots) -> bool`.

### 15.6 `vault_overlay.rs` + `db/vault_read_overlay.rs` + `db/vault_wardrobe_write.rs`
```rust
pub fn is_wardrobe_item_document_path(relative_path: &str) -> bool;                 // recursive (§6.1)
pub fn is_wardrobe_item_document(relative_path: &str, content: Option<&str>) -> bool; // + valid `types`
pub fn has_stable_wardrobe_item_id(content: Option<&str>) -> bool;
pub fn resolve_and_check_component_items(items: &mut [WardrobeItemFromFile], item_by_slug: &HashMap<String, usize>,
    item_by_id: &HashMap<String, usize>, character_id: &str, mount_point_id: &str);       // `archetypes` GONE
pub fn read_character_vault_wardrobe(repo: &DocMountDocumentsRepository, mount_point_id: &str, character_id: &str) -> Result<Option<Value>, DbError>;
pub struct ProjectArrayOptions<'a> { pub preserve_file_names: &'a [&'a str], pub recursive: Option<&'a [&'a str]> /* excludeSubfolders */,
    pub owns_file: Option<&'a dyn Fn(&str, Option<&str>) -> bool> }
pub fn project_array_into_vault_folder<T>(links, docs, mount_point_id, folder, items: &[T],
    mapper: impl Fn(&T) -> (String, String), opts: ProjectArrayOptions) -> Result<(), DbError>;
```
`WardrobeItem.folder: String` and `WardrobeItemFromFile.folder: String` (both
after `archived_at`). `SeedArchetype` deleted.

### 15.7 `db/wardrobe_read.rs` + `db/archetype_wardrobe.rs` (the shrink)
```rust
pub fn find_by_character_id(main, docs, character_id, include_archived) -> Result<Vec<Value>, DbError>;   // kept
pub fn read_shared_tiers(docs, mount_point_ids: &[String], include_archived: bool,
    origin_of: &dyn Fn(&str) -> WardrobeOrigin) -> Vec<Value>;   // WARN per unreadable mount + the debug line; never fails
```
DELETED per §13 (each with its v4 twin's deletion as the reason).
**Shim rule for the five lanes:** every v5 fn whose v4 twin is deleted but
which a LANE file still calls on main (`find_wearable_pool_for_character`,
`find_by_ids_for_character`, `find_by_id_for_character`, `find_archetypes`,
`find_archetype_by_id`, `find_archetypes_in_mounts[_attributed]`,
`read_project_wardrobe`, `read_group_wardrobe`, `ensure_project_wardrobe_folder`,
`ensure_group_wardrobe_folder`, `resolve_shared_wardrobe_tiers_for_chat`,
`shared_wardrobe_tiers_for_character`, `resolve_wardrobe_container`,
`resolve_container_mount_point_id`, the three `*_project_wardrobe_item`,
`tools::wardrobe_shared::{resolve_wardrobe_item_across_tiers,
hydrate_component_graph, lookup_for_bundle}`) is KEPT by the keystone as a
thin wrapper over the new API, marked `// LANE-REMOVE(<lane>): v4 deleted <twin>
in 9ecebf986`, and the keystone adds a `#[test]` census
(`no_wardrobe_lane_remove_markers_left`) that FAILS while any marker remains —
the unifier's tripwire, not a stub (the wrappers are complete, behaving code).

## 16. Cross-lane meeting points, build order, rulings owed

**Meeting points found:**
1. `wear_ops::wear_item` / `take_off_item` call `apply_displacement` — the
   OUTFIT lane rewrites `outfit-displacement.ts` onto `slot-ops.ts` (`−430
   +…` in the span) and re-points these two; both orders name the seam.
2. `build_cycle_peers` calls `load_wearable_pool(…, own_items)` — the pool is
   in the keystone, so no cross-lane edge; but the TOOLS lane's
   `wardrobe_create` cycle refusal now comes from the writer (400 on every
   tier) — the TOOLS order must not re-check.
3. `WardrobeItemHome` (IMAGES lane) is built FROM `resolve_wardrobe_location`
   + `writable_mount_point_id` (`item-images.ts:82-102`); the keystone's
   `services/wardrobe_container.rs` edit must leave `WardrobeItemHome` intact
   and the IMAGES lane re-homes its resolver.
4. The fixture-builder re-aims (§14 A) are shared by the TOOLS, ROUTES, IMAGES
   and roster families — the keystone lands them ALL in its commit.
5. `tools/wardrobe_shared.rs` is read by `api/chat_outfits.rs` (OUTFIT) and
   the seven tools (TOOLS): its pool-replacing fns are shims (§15.7) until
   BOTH lanes are in.
6. `services/wardrobe_transfers.rs` provisions through
   `ensure_project_wardrobe_folder` — the ROUTES/TRANSFERS lane moves it to
   `resolve_wardrobe_location(…, ensure: true)` + `group_locations_for_
   character` (bug 192's order) + `folder` on every planned item.
7. CARRIERS: `qtap_export/records.rs:936`, `quilltap_import/wardrobe_wear.rs:
   27`, `backup/uuid_remap.rs` use `is_wardrobe_item_document_path` — they get
   the recursive predicate FOR FREE from the keystone; their lane owns the
   recursive LISTING (their folder reads are flat today) and `folder` on the
   `wardrobe_item` record (`safe_folder` at the writer catches an unsafe one).
8. `migration_indexes.json` / `fresh_schema.json`: NO D23 move in this round.

**Build order inside the keystone (each step green before the next):**
(1) `wardrobe_folders.rs` + the four `wardrobe.rs` pure helpers +
`build_composite_types` (tier-1 families); (2) `vault_overlay.rs` predicate /
`folder` / parser / component arms + `read_character_vault_wardrobe`
recursive, no seed (re-pin the vault families); (3) the projection's options +
`project_vault_wardrobe` recursive (`vault_projection_recursive_tier2`); (4)
`db/vault_wardrobe_public.rs` promotion + `update_many_in_mount` + the folder
primitives (`wardrobe_folder_writes_tier2`); (5) `read_shared_tiers` + the pool
(`wearable_pool_equivalence` + the tier-2 load beats); (6) `wardrobe_location.rs`
+ `ensure_owner_official_store` (`wardrobe_location_tier2`); (7) `wear_ops.rs`
+ `wearable.rs`; (8) `wardrobe_item_mutations.rs` + `wardrobe_folder_mutations.rs`;
(9) the shrink + shims + the LANE-REMOVE census + the fixture-builder re-aims
+ the import-dead case re-aims that are the keystone's own
(`wardrobe-public-read.ts`, `project-roster-access.test.ts`).

**Rulings owed (human):**
- **R1 — `db/wardrobe.rs` + `wardrobe_tier2_equivalence`:** v4's repository no
  longer knows `wardrobe_items`; the only v4 reader is `migrations/lib/legacy-
  wardrobe-rows.ts` for two cutover scripts v5 never ported. Options: retire
  `db/wardrobe.rs` and the family (v4 has no twin to diff against), or
  re-aim the family at `readLegacyWardrobeRows` as a read-only tier-2 (keeps
  a path to port the cutover later). Recommend RETIRE with a status-log
  banked note; the family's seeds stay in git history.
- **R2 — `store-file.ts`'s reservation (§10.4):** RULED NO-PORT (writer-task
  serialisation), the debug line recorded as unreachable — confirm.
- **R3 — the `ensure`-on-GET change:** v4's old container resolver provisioned
  a project/group store on every resolve; the location provisions only on
  `ensure: true`, so a GET of an unprovisioned project's wardrobe now answers
  `{ wardrobeItems: [] }` without creating a store. v5's
  `services/wardrobe_container.rs:13-16` records the OLD behaviour as a
  deliberate write-on-read — the keystone flips it to v4's new shape; flag
  in the ROUTES order so the collection GET's `mountPointId` key (the
  factory's new field) is `null`-safe.
- **R4 — `movesFolder`'s exact-case compare** (`wardrobe-writes.ts:304`) vs
  `updateItem`'s `isSameFolder` case-blind skip (`item-mutations.ts:158`):
  faithful port keeps both as v4 has them (a case-only folder change through
  `updateMany` directly — `moveItemsToFolder` after `resolveFolder` returns the
  STORED spelling, so it never happens from the routes). No ruling needed;
  noted so the lane does not "unify" them.
