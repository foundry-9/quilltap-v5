# Survey — wardrobe COPY TO FOLDER + components when filing outfits (`bbbcd2388`), server AND SPA — one lane

Read-only planning survey, 2026-10-16. v4 = `~/source/quilltap-server` read ONLY
at the PIN `25b8fab6a` (`25b8fab6aa58636c6885ba63684c7f8fbfca9c3b`,
`4.11.0-dev.10`, clean) via `git show 25b8fab6a:<path>` and
`git diff 83334b9a5 25b8fab6a -- <paths>` / `git show bbbcd2388 -- <paths>`;
v5 = main at `56554d98c`. Every v4 line number is AT THE PIN; every v5 line
number is at main `56554d98c`. Port facts come from shipped hunks, never from
the commit message. The drift ledger's §3 row for `bbbcd2388`
(`drift-ledger.md:307`) is detailed and correct in the large; this survey
EXTENDS it (exact algorithm trace, every wire byte, the v5 seams that the
row only names) and corrects it in §9.

Scope: `bbbcd2388` "feat(wardrobe): copy to folder, and ask about components
when filing outfits" — 24 files, +872/−137, parent `809e5104d` — SERVER
(`lib/wardrobe/folder-filing.ts` NEW, `expand-composites.ts`,
`wardrobe-route-factory.ts`, `wardrobe.types.ts`, the transfers route's import)
AND SPA (the React picker/browser/row/hook/fieldset, ported to Angular). One
lane. **Out of scope** (other ledger rows, same files touched at the pin):
`7458ff94e` (hair slot — `outfit-description.ts`, `slot-guidance.ts`),
`f0db8cbb8` (Pascal wardrobe effects — `wear-ops.ts`), `01f508d4e` (prune debug
logging — removes the `console.debug` in `useWardrobeItemActions.duplicate`,
the same file this commit edits; see §6), `b82037d75` (bug 248).

No schema / migration / DDL / stored-field move (verified in §1.1: the
`wardrobe.types.ts` hunk is REQUEST-body Zod only). The `.qtap` / backup /
restore carriers are UNAFFECTED (§2.9).

---

## 0. The span — which files moved, and what each is for

`git show --stat bbbcd2388` (24 files). Grouped:

| group | files | change |
|---|---|---|
| request Zod | `lib/schemas/wardrobe.types.ts` | +23/−2 — `WardrobeFolderComponentModeSchema`, `moveWardrobeItemsSchema.components`, NEW `copyWardrobeItemsSchema` |
| the algorithm | `lib/wardrobe/folder-filing.ts` | NEW, 199 lines — `fileItemsToFolder`, `FolderFilingRequestError`, `logFiled` |
| the shared walk | `lib/wardrobe/expand-composites.ts` | +31 — `collectContainerComponents` (MOVED out of the transfers route, generalised from one composite to many) |
| transfers route | `app/api/v1/wardrobe/transfers/route.ts` | −30/+? — the private walk deleted; import + `collectContainerComponents([source.item], source.containerItems)` (`:256`). Same result for one composite. |
| route factory | `lib/wardrobe/routes/wardrobe-route-factory.ts` | +42 — `handleFileItems` (shared), `handleMoveItems`, NEW `handleCopyItems`, `FolderFilingRequestError` in `writeErrorResponse`, `'copy-items'` in the POST action table |
| client hook | `lib/hooks/use-wardrobe-folders.ts` | +39 — `WardrobeFolderComponentMode`, `fileItems`, `moveItems(…, components?)`, NEW `copyItems` |
| UI | `WardrobeComponentModeFieldset.tsx` NEW 83; `WardrobeFolderPickerDialog.tsx` +93; `WardrobeTransferDialog.tsx` −60 (+fieldset); `WardrobeBrowser.tsx` +50; `wardrobe-item-row.tsx` +20; `hooks/useWardrobeItemActions.ts` +8 (this commit; +15 at the pin incl. `01f508d4e`'s −7) |
| tests | `folder-filing.test.ts` NEW 207 (8 `it`); `wardrobe-route-factory.test.ts` +40 (**3** `it`, not 2 — §9); `wardrobe-browser.folders.test.tsx` +53 (2 `it`) |
| docs/help | `docs/developer/API.md` (2 rows), `features/complete/wardrobe-folders.md` (2 rows), `help/wardrobe.md` (+6/−?), `help/project-wardrobe.md` (1 line), `docs/CHANGELOG.md`, `README.md`, 3 version stamps | help COUNT unchanged (131) |

Between the baseline `83334b9a5` and the pin, the ONLY commit touching this
slice's code files is `bbbcd2388` (`git diff 83334b9a5 25b8fab6a --stat` over
the slice's paths shows exactly bbbcd2388's hunks plus the three out-of-scope
commits named above in `wear-ops.ts` / `outfit-description.ts` /
`slot-guidance.ts` / `outfit-utils.test.ts` / the `useWardrobeItemActions`
−7). So bbbcd2388's per-commit hunks and the span diff coincide for every
file in §1.

**What the commit does, in one paragraph (from the hunks).** The cross-wardrobe
transfer already asked "what about an outfit's components?" (move / copy /
leave). Folder filing (`move-items`, `4a38222e6`) moved only the outfit and
left its pieces. This commit (a) adds a COPY action to folder filing
(`copy-items`: a new id, a `(copy)` title, its own picture copies), (b) lets
BOTH `move-items` and `copy-items` take `components`, with the transfer's
all-or-nothing semantics and the same walk, (c) adds the UI for both
(**Copy to folder…** in the row menu and select bar; the picker grows the
components fieldset when outfits are among the items), (d) factors the radio
group into one component shared with the transfer dialog.

---

## 1. v4 at the pin — hunks, file by file

### 1.1 `lib/schemas/wardrobe.types.ts` (the REQUEST Zod; `:208-233`)

```ts
/**
 * What travels with an outfit filed into a folder (`move-items` / `copy-items`):
 * its same-wardrobe components moved, copied, or left where they are.
 */
export const WardrobeFolderComponentModeSchema = z.enum(['move', 'copy', 'none']);   // :212

export const moveWardrobeItemsSchema = z.object({                                     // :218
  itemIds: z.array(z.string().min(1)).min(1, 'At least one item is required').max(500),
  folder: WardrobeFolderInputSchema,
  components: WardrobeFolderComponentModeSchema.optional(),
});

export const copyWardrobeItemsSchema = z.object({                                     // :229
  itemIds: z.array(z.string().min(1)).min(1, 'At least one item is required').max(500),
  folder: WardrobeFolderInputSchema,
  components: z.enum(['copy', 'none']).optional(),
});
```

Consequences for the wire (the route does `schema.parse(await req.json())`
BEFORE it resolves the location — `:342`/`:346` — so a bad body never touches
the store):

- `z.object` STRIPS unknown keys — `{ …, "bogus": 1 }` parses. `components`
  absent → `undefined`; `components: null` → ZodError (`.optional()` is not
  `.nullable()`); `components: "Move"` / `5` → ZodError. All answer the
  context middleware's flat `400 {"error":"Validation error","details":[…]}`
  (the `details` array is the project-wide deferral — compare status + the
  `error` string only, as `wardrobe_folders_routes_equivalence` already does).
- **`copy-items` with `components: 'move'` is refused AT THE SCHEMA** (400
  Validation error), NOT with the `FolderFilingRequestError` sentence. The
  v4 route test pins exactly that (§1.9 test 3: `rejects.toThrow(/components/)`).
  `FolderFilingRequestError`'s sentence is therefore reachable only by a
  direct call to `fileItemsToFolder` (the unit test), never over HTTP.
- `WardrobeFolderInputSchema` (unchanged; `:~150`) is the existing folder-path
  input: a string, trimmed/limits as the move-items rows already pin
  (`mi_char_folder_missing_zod` = `{}` → 400; `folder: ''` = the root).
  v5's `folder_input(obj, "folder", false)` (`wardrobe_route_factory.rs:511`)
  already ports it.
- `itemIds`: `min(1)` items, `max(500)`, each `z.string().min(1)`; the
  messages (`'At least one item is required'`) never reach the wire (details).

### 1.2 `lib/wardrobe/expand-composites.ts` (`:154-183`)

```ts
export function collectContainerComponents<T extends CompositeNode & { id: string }>(
  composites: readonly T[],
  containerItems: readonly T[],
): T[] {
  const byId = new Map(containerItems.map((i) => [i.id, i]));
  const visited = new Set<string>(composites.map((c) => c.id));
  const result: T[] = [];
  const queue = composites.flatMap((c) => [...(c.componentItemIds ?? [])]);
  while (queue.length > 0) {
    const id = queue.shift() as string;
    if (visited.has(id)) continue;
    visited.add(id);
    const item = byId.get(id);
    if (!item) continue;
    result.push(item);
    queue.push(...(item.componentItemIds ?? []));
  }
  return result;
}
```

Order contract (the tests and the DB-state diff both depend on it): BFS from the
composites' component lists IN COMPOSITE ORDER, each composite's ids in
stored order; `visited` seeded with EVERY composite id (so a composite that is
also another composite's component is never returned — it is a "requested
item", not a component); an id absent from `containerItems` (a borrowed
General/group/project piece) is marked visited and dropped, and its own
components are NOT walked; the result lists each reachable same-wardrobe item
once, in first-reached order; cycles end at `visited`. The transfers route
was changed from `collectContainerComponents(source.item, …)` to the array
form with ONE composite (`transfers/route.ts:256`) — behaviour identical.

### 1.3 `lib/wardrobe/folder-filing.ts` (199 lines) — the algorithm, step by step

Types (`:43-66`):

```ts
export type FolderComponentMode = 'move' | 'copy' | 'none';
export interface FileItemsToFolderRequest {
  action: 'move' | 'copy';
  itemIds: readonly string[];
  folder: string;
  components?: FolderComponentMode;
}
export interface FileItemsToFolderResult {
  items: WardrobeItem[];          // the requested items as they now stand — moved originals, or their copies
  componentsCarried: number;      // how many components travelled along (moved or copied)
}
export class FolderFilingRequestError extends Error {   // name = 'FolderFilingRequestError'
  constructor(message: string) { super(message); this.name = 'FolderFilingRequestError'; }
}
```

Signature (`:73`): `fileItemsToFolder(repos, userId, location, request)`.
Imports worth noting: `WardrobeItemNotInLocationError` from
`@/lib/wardrobe/folders` (the same class `item-mutations` re-exports);
`carryItemImages` from `item-images`; `createItem` + `moveItemsToFolder` from
`item-mutations`; `nextCopyTitle` from `next-copy-title`; `isComposite` from
`wardrobe.types` (`(item.componentItemIds?.length ?? 0) > 0`, `:411`).

Steps (line numbers at the pin):

1. `:79-80` `componentMode = request.components ?? 'none'`.
2. `:81-85` **`action === 'copy' && componentMode === 'move'`** → throw
   `new FolderFilingRequestError('Copying an outfit cannot move its components — the original outfit still needs them')`
   (em dash U+2014; the same sentence as the transfers route's refine).
   Nothing has been read or written. (Over HTTP the schema refuses first, §1.1.)
3. `:88-91` **plain path** (`move` + `none`): `items = await moveItemsToFolder(location, request.itemIds, folder)`
   (the RAW folder — `moveItemsToFolder` resolves it itself, `item-mutations.ts:219`);
   return `{ items, componentsCarried: 0 }`. **No `[FolderFiling]` line.** The
   only DEBUG is `moveItemsToFolder`'s own `[ItemMutations] Moved wardrobe items to folder`.
4. `:93` `target = await location.resolveFolder(folder)` (`''`/blank → `''`; an
   existing folder in any casing → as stored; else the normalised new path;
   throws `WardrobeFolderPathError` — a 400 — BEFORE the missing-id check).
5. `:94` `uniqueIds = Array.from(new Set(request.itemIds))` (first-seen order).
6. `:95-96` `wardrobeItems = await location.readItems(true)` (ARCHIVED INCLUDED,
   this wardrobe's own mount only — a character's borrowed General/group/project
   items are NOT in it); `byId` map.
7. `:97-98` `missing = uniqueIds.filter(id => !byId.has(id))`; any →
   `throw new WardrobeItemNotInLocationError(missing)` — BEFORE any write.
   Message (`folders.ts:61-66`): `` `Not in this wardrobe (shared items are filed where they live): ${itemIds.join(', ')}` ``
   — ids joined `', '` in REQUEST order (first-seen).
8. `:99` `requested = uniqueIds.map(id => byId.get(id))`.
9. `:102-108` `requestedIds = new Set(uniqueIds)`;
   `components = componentMode === 'none' ? [] : collectContainerComponents(requested.filter(isComposite), wardrobeItems).filter(item => !requestedIds.has(item.id))`.
   (A requested item that is ALSO a component of a requested outfit is
   handled as a requested item, never as a component.) NOTE `isComposite` is
   true for any item with ≥1 component id — borrowed or not.
10. `:110-114` **`move` + `move`**: `moved = await moveItemsToFolder(location, [...uniqueIds, ...components.map(c => c.id)], target)`
    — ONE batch, requested first then components (BFS order); `target` (already
    resolved) is passed, `moveItemsToFolder` resolves it again (idempotent);
    `logFiled(location, action, componentMode, target, uniqueIds.length, components.length)`;
    return `{ items: moved.slice(0, uniqueIds.length), componentsCarried: components.length }`
    (updateMany returns patch order, so the slice is exactly the requested items).
    Two DEBUG lines in order: `[ItemMutations] Moved wardrobe items to folder`
    (with `requested: uniqueIds.length + components.length`, `moved`, `folder: target`)
    THEN `[FolderFiling] …`.
    **Reached for a leaf-only request too** (`components:'move'` over non-composites
    → `components = []`, `componentsCarried 0`, but the `[FolderFiling]` line is
    emitted and `readItems` + the missing check ran).
11. `:118` `toCopy = action === 'copy' ? [...components, ...requested] : components`
    (components `copy` or — for a copy — `none`). Components FIRST, then the
    requested items in `uniqueIds` order.
12. `:119-120` `idMap = new Map(toCopy.map(item => [item.id, randomUUID()]))`;
    `remap(ids) = ids.map(id => idMap.get(id) ?? id)` (unmapped ids — borrowed
    pieces, pieces left behind — keep pointing at the original).
13. `:121` `takenTitles = wardrobeItems.map(i => i.title)` — EVERY title in the
    wardrobe, archived included, original casing (nextCopyTitle lower-cases).
14. `:122` `mountPointId = await location.writableMountPointId()` — for a
    character this RE-RESOLVES the vault and throws `CharacterArchivedError`
    (409 `An archived character's wardrobe cannot be changed`) for a tombstoned
    character. **Order matters**: a missing id (400) beats an archived
    character (409) on this path; on the plain/`move`+`move` paths the archived
    409 comes from `updateMany`'s `writableMount()`.
15. `:123` ONE `now = new Date().toISOString()` for every copy's
    `createdAt`/`updatedAt`.
16. `:126-158` for each `original` of `toCopy` in order:
    - `title = nextCopyTitle(original.title, takenTitles)`; `takenTitles.push(title)`
      (so two copies of same-titled items get `(copy)` and `(copy 2)`).
    - `id = idMap.get(original.id)`.
    - `{ fileIdMap } = await carryItemImages(repos, { mode:'copy', sourceItemId: original.id, destinationItemId: id, destinationMountPointId: mountPointId, userId })`
      (§1.5: writes blob links + NEW `files` rows).
    - `stored = await createItem(location, { ...original, title, folder: target, componentItemIds: remap(original.componentItemIds) }, { preserve: { id, createdAt: now, updatedAt: now, archivedAt: original.archivedAt ?? null, imageFileId: original.imageFileId ? fileIdMap.get(original.imageFileId) ?? null : null, migratedFromClothingRecordId: original.migratedFromClothingRecordId ?? null } })`
      — NO `lookup` (a composite copy keeps `types` as given). `createItem`
      builds the stored fields through `wardrobeItemFromCreateBody` (§1.4):
      so `isDefault`, `replace`, `description`, `imagePrompt`,
      `appropriateness` ARE CARRIED from the original (a copy of a default
      item is a default item); blank prose → null; `componentItemIds` Set-deduped;
      `imageFileId`/`migrated…` come ONLY from `preserve`; an ARCHIVED original
      yields an ARCHIVED copy (same `archivedAt` string).
    - `copies.set(original.id, stored)`.
    Components are created before the outfit that names them, but a component's
    own sub-components may be created AFTER it (BFS order: `Suit (copy)` then
    `Shirt (copy)`, `Trousers (copy)`) — ids are pre-minted, so references
    resolve at the end; `createInMount` runs `assertNoCycles` against the
    current peers on each create (an unborn id is simply absent).
17. `:160-175` results: `action === 'copy'` → `items = requested.map(item => copies.get(item.id))`
    (the STORED copies, in request order); else (**`move` + `copy`**):
    `items = await location.updateMany(requested.map(item => ({ id: item.id, patch: { folder: target, ...(isComposite(item) ? { componentItemIds: remap(item.componentItemIds) } : {}) } })), { requireAll: true })`
    — the requested items MOVE (ids kept), outfits are rewired to the copies;
    a requested leaf only gets `folder`. (`types` is NOT recomputed — a plain
    patch.)
18. `:177-178` `logFiled(location, action, componentMode, target, uniqueIds.length, components.length)`;
    return `{ items, componentsCarried: components.length }`.
    **`componentsCarried` counts the components, not the copies** — for a copy
    with `components:'copy'` it is the number of component copies (the
    requested items' own copies are not counted).

`logFiled` (`:181-198`) — DEBUG, the message and keys in this order:

```ts
logger.debug('[FolderFiling] Filed wardrobe items into folder', {
  scope: location.scope, mountPointId: location.mountPointId, action, componentMode,
  folder,            // the RESOLVED target
  requested,         // uniqueIds.length (deduped)
  componentsCarried, context: 'wardrobe',
});
```

There is no ERROR/WARN in the module. There is no transaction: a failure
mid-copy (a cycle error from `createInMount`, a picture write failing)
leaves the earlier copies (and their picture links) behind. The missing-id
refusal, the copy+move refusal and the folder-path refusal are the only
"before any write" guarantees.

### 1.4 What `createItem` does with the spread (`item-mutations.ts:106-137`, `create-body.ts:41-59`)

`wardrobeItemFromCreateBody(body, location.characterId)` maps ONLY: `characterId`,
`title`, `description` (prose: blank → null), `imagePrompt` (prose),
`types`, `componentItemIds` (Set-deduped), `appropriateness` (prose),
`isDefault` (`?? false`), `replace` (`?? false`), `migratedFromClothingRecordId: null`,
`imageFileId: null`, `folder: body.folder ?? ''`. Then `createItem`:
`fields.folder = fields.folder ? await location.resolveFolder(fields.folder) : ''`;
composites get `compositeTypes(opts.lookup, …)` which with NO lookup is
`[...designated]` (= the body's `types`); then
`{ ...fields, id: preserve.id, archivedAt, imageFileId, migratedFromClothingRecordId, createdAt, updatedAt }`
→ `location.create(item)` → `createInMount` returns `{ ...item, folder: safeFolder(item.folder), characterId: mount.characterId }`.
Key ORDER of that object (it is what `copy-items` answers; §1.7): `characterId,
title, description, imagePrompt, types, componentItemIds, appropriateness,
isDefault, replace, migratedFromClothingRecordId, imageFileId, folder, id,
archivedAt, createdAt, updatedAt` (the spread's keys first — `imageFileId` /
`migrated…` keep their position when overridden — then the NEW keys `id`,
`archivedAt`, `createdAt`, `updatedAt`). v5 already has this order as
`item_create_value` (`wardrobe_route_factory.rs:316-337`). A `move`+`copy`'s
`updateMany` result and a `move`'s are in the READ order
(`item_read_value`, `:293-313`: `id` FIRST). **Three different key orders
ride the same `{ items }` field** — §6 trap 1.

DEBUG lines a copy emits per item (all already ported by the keystone):
`Wardrobe item created in folder` (bare, from `createInMount`),
`[ItemMutations] Created wardrobe item`, and the `[WardrobeImages]` lines of
`carryItemImages`; a `move`+`copy` also emits `updateMany`'s lines.

### 1.5 `carryItemImages` (`lib/wardrobe/item-images.ts:433-…`, unchanged by this commit)

`mode:'copy'`: for each of the source item's picture rows (`listWardrobeItemImages`),
parse the storage key + read the blob; unreadable → WARN
`[WardrobeImages] Wardrobe image has no readable blob; not carried` and skip;
else `writeWardrobeItemImage({ mountPointId: destinationMountPointId, itemId: destinationItemId, kind, content, contentType, description, leafName })`
and a NEW `files` row (`repos.files.create({...rest, originalFilename: written.leafName, linkedTo: [dest], tags: [dest], storageKey}, { id: randomUUID() })`);
returns `fileIdMap` (old file id → new file id; insertion order). v5's twin:
`services/wardrobe_item_images/carry.rs:114 carry_item_images` (§2.4).

### 1.6 `lib/wardrobe/next-copy-title.ts` (the Duplicate helper; unchanged, `:12-29`)

```ts
const COPY_SUFFIX = /\s*\(copy(?:\s+\d+)?\)\s*$/i
export function nextCopyTitle(sourceTitle: string, existingTitles: string[]): string {
  const base = sourceTitle.replace(COPY_SUFFIX, '').trim() || sourceTitle.trim()
  const taken = new Set(existingTitles.map((t) => t.trim().toLowerCase()))
  for (let n = 1; ; n++) {
    const candidate = n === 1 ? `${base} (copy)` : `${base} (copy ${n})`
    if (!taken.has(candidate.toLowerCase())) return candidate
  }
}
```

`\s` = JS whitespace (incl. U+FEFF, U+00A0, U+2028/9 …), `\d` = ASCII digits,
`/i` on ASCII letters only; leftmost match → the earliest start that can reach
`$` (so the whole whitespace run before `(copy` is eaten); `(copy2)` (no space)
and `(copy 0)`/`(copy 007)` behave per the regex (`(copy 007)` strips; `(copy2)`
does not); a title that is ONLY `(copy)` → base `''` → falls back to
`sourceTitle.trim()` → `(copy) (copy)`. Used by v4 ONLY client-side
(`useWardrobeItemActions.duplicate`, `:208`) until this commit; folder-filing is
its first server-side caller.

### 1.7 `lib/wardrobe/routes/wardrobe-route-factory.ts` (`:119-136`, `:306-347`, `:401-409`)

```ts
function writeErrorResponse(error: unknown): NextResponse {                  // :125
  if (error instanceof WardrobeComponentCycleError) return badRequest(error.message);
  if (error instanceof WardrobeFolderPathError) return badRequest(error.message);
  if (error instanceof WardrobeItemNotInLocationError) return badRequest(error.message);
  if (error instanceof FolderFilingRequestError) return badRequest(error.message);   // NEW :129
  if (error instanceof WardrobeFolderNotFoundError) return notFound('Wardrobe folder');
  if (error instanceof WardrobeFolderConflictError) return conflict(error.message);
  if (error instanceof CharacterArchivedError) return conflict("An archived character's wardrobe cannot be changed");
  throw error;                                                                // → context middleware 500
}
```

`handleFileItems(action, body, ctx, params)` (`:310-339`): `resolveLocation(ctx, params, false)`
(NEVER provisions; a missing owner/foreign character/unprovisioned General →
the resolver's response); `fileItemsToFolder(ctx.repos, ctx.user.id, location, { action, ...body })`
inside `try` (errors → `writeErrorResponse`); then **INFO** (only on success):

```ts
logger.info(`${logTag} ${action === 'move' ? 'Moved' : 'Copied'} wardrobe items to folder`, {
  [logIdKey]: location.id,       // characterId | generalId (null) | projectId | groupId
  userId: ctx.user.id,
  mountPointId: location.mountPointId,
  folder: body.folder,           // AS REQUESTED (not the resolved target — v4's asymmetry, already pinned for move-items)
  count: result.items.length,
  components: body.components ?? 'none',
  componentsCarried: result.componentsCarried,
  context: 'wardrobe',
});
return successResponse({
  items: await serializeWardrobeItems(result.items, location.origin, ctx.repos),
  componentsCarried: result.componentsCarried,
});
```

`successResponse` → **200** (a copy is NOT a 201). Body key order: `items`
then `componentsCarried`. `serializeWardrobeItems` = `attachWear(withOrigin(...))`
(origin then wear on every item — shared with every other item body).
`handleMoveItems` = `handleFileItems('move', moveWardrobeItemsSchema.parse(await req.json()), …)`;
`handleCopyItems` = `('copy', copyWardrobeItemsSchema.parse(…))`. The POST table
(`:401-409`), in its literal order: `instructions, create-folder, rename-folder,
delete-folder, move-items, copy-items` then the create fallback. So the
unknown-action envelope's `availableActions` becomes those SIX strings in that
order (§2.6 — two v5 tests pin the five-string list today).

Wire summary (all four tiers; `?action=copy-items` on
`/characters/{id}/wardrobe`, `/wardrobe` (General), `/projects/{id}/wardrobe`,
`/groups/{id}/wardrobe`):

| request | status | body |
|---|---|---|
| `move-items` `{itemIds, folder}` (no `components`) | 200 | `{ items: [...read-order, origin, wear], componentsCarried: 0 }` |
| `move-items` `components:'move'` | 200 | `{ items: [the requested only], componentsCarried: N }` |
| `move-items` `components:'copy'` | 200 | `{ items: [moved outfits, rewired], componentsCarried: N }` |
| `copy-items` `{itemIds, folder}` / `components:'none'` | 200 | `{ items: [copies, create-order, origin, wear], componentsCarried: 0 }` |
| `copy-items` `components:'copy'` | 200 | `{ items: [copies of the REQUESTED], componentsCarried: N components copied }` |
| `copy-items` `components:'move'` | 400 | flat `{"error":"Validation error", "details":[…]}` (schema) |
| any id not in this wardrobe | 400 | `{"error":"Not in this wardrobe (shared items are filed where they live): a, b"}` |
| bad folder path | 400 | the `WardrobeFolderPathError` sentence |
| archived character | 409 | `An archived character's wardrobe cannot be changed` |
| unknown owner / foreign character | 404 | the resolver's `Character not found`-style body (unchanged) |

### 1.8 Client (React) — every user-visible string verbatim

`lib/hooks/use-wardrobe-folders.ts`: `export type WardrobeFolderComponentMode = 'move' | 'copy' | 'none'` (`:34`);
`FolderRequest.action` gains `'copy-items'` (`:71`); NEW `fileItems(mode, container, itemIds, folder, components?)`
(`:138-155`): action `mode === 'move' ? 'move-items' : 'copy-items'`, body
`{ itemIds, folder, ...(components ? { components } : {}) }` (the key rides ONLY
when set), returns `data?.items ?? []`; `moveItems(container, itemIds, folder, components?)`
and NEW `copyItems(container, itemIds, folder, components?: 'copy' | 'none')`
(`:156-166`); the hook returns `{ createFolder, renameFolder, deleteFolder, moveItems, copyItems, pending }`.

`components/wardrobe/WardrobeComponentModeFieldset.tsx` (NEW, 83 lines):
`ComponentMode = 'move' | 'copy' | 'none'`; `defaultComponentMode(mode)` →
`move` → `'move'`, `copy` → `'copy'`; props `{ mode, value, onChange, legend, destination: 'wardrobe' | 'folder', name, disabled = false }`.
Markup: `<fieldset><legend className="text-sm qt-text-secondary mb-1">{legend}</legend><div className="space-y-1">` + per option
`<label className="flex items-center gap-2 cursor-pointer text-sm"><input type="radio" name={name} className="qt-radio" checked onChange disabled /><span>{label}</span></label>` +
`</div><p className="qt-text-xs qt-text-secondary mt-1">{footnote}</p></fieldset>`.
Radio labels (verbatim, in order):

- mode `move`: `Move the components along with it` / `Copy the components (originals stay behind)` / `Leave the components behind`
- mode `copy`: `Copy the components along with it` / `Copy the outfit alone`

Footnote (verbatim; the em dash is U+2014 in both):

- `destination === 'wardrobe'`: `All or nothing — the choice covers every component, nested pieces included. Only components living in the same wardrobe travel; pieces from a shared tier stay put. The transferred outfit is rewired to point at whichever components arrive with it.`
- `destination === 'folder'`: `All or nothing — the choice covers every component, nested pieces included. Only components filed in this wardrobe travel; pieces borrowed from a shared tier stay put. Copied components take a "(copy)" title, and the outfit is rewired to point at them.`

(The transfer dialog's old inline footnote was the same text wrapped across
source lines; the rendered string is identical to the `wardrobe` variant —
v5's `wardrobe-transfer-dialog.ts:158-162` already renders it.)

`WardrobeTransferDialog.tsx` — shows the fieldset when `isComposite` (its own
item): `legend={`This outfit bundles ${n} ${n === 1 ? 'component' : 'components'}`}`,
`destination="wardrobe"`, `name="wardrobe-transfer-components"`,
`disabled={working}`; default via `defaultComponentMode(mode)`. No behaviour
change (a pure extraction).

`WardrobeFolderPickerDialog.tsx` (props at `:38-62`): `mode: 'move' | 'copy'`
(NEW), `container`, `subject`, `outfits?: ReadonlyArray<{ componentItemIds: readonly string[] }>` (default `[]`),
`currentFolder`, `working?` (renamed from `moving`), `onClose`, `onSubmit(folder, components?)`
(renamed from `onMove`; `components` is passed ONLY when outfits are among the
items: `void onSubmit(target, hasOutfits ? componentMode : undefined)`).
Behaviour (all from the hunks):

- `componentMode` state starts at `defaultComponentMode(mode)` and resets on
  `[isOpen, mode]`; the browser mounts the dialog per open.
- `isCurrent(path) = mode === 'move' && currentFolder !== null && isSameFolder(path, currentFolder)`
  — **for a copy no folder is ever "here now"/disabled**, a copy may land beside
  its original; the `(here now)` span and `title="Already here"` follow `isCurrent`.
- `canSubmit = target !== null && !isCurrent(target) && !working`.
- `hasOutfits = outfits.length > 0`; `componentCount = outfits.reduce((n, o) => n + o.componentItemIds.length, 0)` (RAW id counts —
  borrowed pieces and duplicates included; it is a label, not what travels);
  legend: `outfits.length === 1` → `` `This outfit bundles ${componentCount} ${componentCount === 1 ? 'component' : 'components'}` ``;
  else `` `${outfits.length} of these are outfits, bundling ${componentCount} ${componentCount === 1 ? 'component' : 'components'} between them` ``.
- `verb = mode === 'move' ? 'Move' : 'Copy'`; title `` `${verb} ${subject} to a folder` ``
  (`Move "Greatcoat" to a folder` / `Copy 3 items to a folder`); primary button
  `working ? (mode === 'move' ? 'Moving…' : 'Copying…') : verb`; `closeOnClickOutside`/`closeOnEscape` = `!working`.
- helper line (`<p className="qt-text-xs qt-text-secondary mb-2">`): move →
  `Folders are for your browsing; what anyone can wear stays exactly as it was.`;
  copy → `Each copy is a new item with its own ID and a "(copy)" title, filed in the folder you choose.`
- the fieldset sits AFTER the folder list, in `<div className="mt-3">`, only when
  `hasOutfits`: `mode={mode}`, `destination="folder"`, `name="wardrobe-folder-components"`, `disabled={working}`.

`WardrobeBrowser.tsx`: `pickerOutfits = picker.items.filter(isComposite)` (`:325`);
`fileToFolder(folder, components?)` (`:327-354`):

```ts
const copying = picker.mode === 'copy'
setMoving(true)
const ids = picker.items.map((i) => i.id)
const filed = copying
  ? await folderOps.copyItems(pickerHome, ids, folder, components === 'move' ? undefined : components)
  : await folderOps.moveItems(pickerHome, ids, folder, components)
const where = folder ? `"${folderCrumb(filed[0]?.folder || folder)}"` : 'the top of the wardrobe'
showSuccessToast(
  copying
    ? picker.items.length === 1 ? `A copy of "${picker.items[0].title}" now hangs in ${where}`
                                : `Copies of ${plural(filed.length, 'item')} now hang in ${where}`
    : picker.items.length === 1 ? `"${picker.items[0].title}" now hangs in ${where}`
                                : `${plural(filed.length, 'item')} now hang in ${where}`)
actions.closeFolderPicker(); clearSelection(); setSelectMode(false)
// catch: showErrorToast(error instanceof Error ? error.message : copying ? 'The items would not be copied' : 'The items would not be moved')
// finally: setMoving(false)
```

The select bar (`:368-392`): `Move to folder…` → `openFolderPicker(selectedItems, 'move')`;
NEW `Copy to folder…` (same classes, `disabled={selectedItems.length === 0}`) →
`openFolderPicker(selectedItems, 'copy')`; then `Cancel`. Rows:
`onMoveToFolder={(i) => actions.openFolderPicker([i], 'move')}` +
`onCopyToFolder={(i) => actions.openFolderPicker([i], 'copy')}` (`:659-660`).
The dialog props: `mode={picker.mode}`, `outfits={pickerOutfits}`, `working={moving}`, `onSubmit={fileToFolder}`.

`wardrobe-item-row.tsx`: optional prop `onCopyToFolder?: (item) => void` (`:80`);
the menu item is `{manageable && onCopyToFolder && (…)}` placed IMMEDIATELY
AFTER `Move to folder…` (`:458-472`), `role="menuitem"`, text `Copy to folder…`,
`setKebabOpen(false); onCopyToFolder(item)`; header comment now lists
`Duplicate, Archive, Move to folder…, Copy to folder…, Move to another wardrobe…, Copy to another wardrobe…, Delete`.
(The nested component rows forward neither — unchanged.)

`useWardrobeItemActions.ts`: `FolderPickerRequest` gains `mode: 'move' | 'copy'`
(`:59`); `openFolderPicker(items, mode = 'move')` → `setFolderPicker({ items, mode })` (`:269`).

Help: `help/wardrobe.md` (new bullets, `Copy to folder…`, `Outfits ask after their
components`, the select bar sentence now `*N selected · Move to folder… · Copy to folder… · Cancel*`, the
"moving an outfit moves its pieces only if you say so when asked" clause) and
`help/project-wardrobe.md` (1 line) — hunks quoted in §5.4.

### 1.9 The three v4 test files — what each pins

`__tests__/unit/lib/wardrobe/folder-filing.test.ts` (207 lines, 8 `it`; mocks
`@/lib/logger`, `item-images#carryItemImages`, `pool`; a hand-rolled
`makeLocation()` over `ITEMS = [shirt(img-shirt), trousers, suit[shirt,trousers], outfit[suit, general-hat]]`,
`resolveFolder` = `resolveWardrobeFolder(['Formal'], requested)`, `writableMountPointId` = `'vault-1'`):

1. *move + none* (default): `outfit` → `updateMany([{id:'outfit',patch:{folder:'Formal'}}], {requireAll:true})` (`'formal'` resolves to `Formal`), no `create`, `componentsCarried 0`.
2. *move + move*: ONE `updateMany`, ids `['outfit','suit','shirt','trousers']` (BFS order; `general-hat` absent), result items `['outfit']`, `componentsCarried 3`.
3. *move + copy*: creates titled `['Suit (copy)','Shirt (copy)','Trousers (copy)']` in that order, all `folder 'Formal'`; suit copy's `componentItemIds` = the shirt/trousers copies' ids; shirt copy `imageFileId 'img-shirt-copy'`; `carryItemImages` called with `{ mode:'copy', sourceItemId:'shirt', destinationMountPointId:'vault-1' }`; the outfit patched `{ folder:'Formal', componentItemIds:[suitCopy.id,'general-hat'] }`.
4. *copy + none*: `itemIds:['outfit'], folder:''` → ONE create, `Outfit (copy)`, new id, `componentItemIds ['suit','general-hat']` (original pieces), no `updateMany`.
5. *copy + copy*: 4 creates, the outfit copy LAST, `result.items[0].componentItemIds = [suitCopy.id,'general-hat']`, `componentsCarried 3`.
6. *copy two related items, no components*: `['suit','shirt']` → suit copy names the SHIRT COPY (`[shirtCopy.id,'trousers']`) — the `idMap` remap over the requested items themselves.
7. *copy + move* → `rejects.toBeInstanceOf(FolderFilingRequestError)`.
8. *borrowed id, copy + copy, `['outfit','general-hat']`* → `rejects.toBeInstanceOf(WardrobeItemNotInLocationError)` and NO create.

NOT pinned by v4's own tests (so only the differential can pin): the missing-id vs archived-409 ORDER, `takenTitles` collisions/`(copy 2)`, archived-original → archived copy, `isDefault`/`replace`/`appropriateness` carried, pictures count, the DEBUG lines, the title order beyond case 3, a `components:'move'` leaf-only request, deduped ids, the 500-id edge, an `images`/bad folder, a NEW-folder target.

`__tests__/unit/lib/wardrobe/routes/wardrobe-route-factory.test.ts` (+40; **3 `it`s** inside `describe('folder actions')`; adds `carryItemImages` to the item-images mock):

1. *move-items with components: move* — `readItems` = `[outfit(['a','from-general']), a]`; POST `{itemIds:['outfit'],folder:'Formal',components:'move'}` → 200; `updateMany.mock.calls[0][1]` = `[{id:'outfit',patch:{folder:'Formal'}},{id:'a',patch:{folder:'Formal'}}]`; `body.items` ids `['outfit']`; `body.componentsCarried 1`.
2. *copy-items files a copy with a fresh id and a (copy) title* — `{itemIds:['a'],folder:'Formal'}` → 200; created `{ id ≠ 'a', title 'Cloak (copy)', folder 'Formal' }`; `updateMany` NOT called.
3. *copy-items refuses components: move at the schema* — `post('copy-items',{…components:'move'})` `rejects.toThrow(/components/)` (the ZodError is thrown through the mock middleware; the real middleware answers the flat 400); no create.

`__tests__/unit/components/wardrobe/wardrobe-browser.folders.test.tsx` (+53; 2 `it`; the harness's `openFolderPicker` gains `mode`):

1. *Copy to folder… may land in the folder the item already sits in* — inside `Formal` (`Opera Gloves`): kebab `More actions` → menuitem `Copy to folder…` → listbox `Folders`; the `Formal` option is NOT disabled; click it → button `Copy` enabled; **no radio** (a plain garment asks nothing).
2. *moving an outfit asks about its components and sends the choice* — `OUTFIT = item('outfit','Evening Ensemble','',{componentItemIds:['coat']})`, tab `Outfits`, `More actions`[0] → `Move to folder…`; text `This outfit bundles 1 component`; radio `Move the components along with it` CHECKED; click radio `Copy the components (originals stay behind)`; option `Formal`; button `Move`; the `fetch` to `…action=move-items` has body exactly `{ itemIds:['outfit'], folder:'Formal', components:'copy' }`.

---

## 2. v5 today (main `56554d98c`)

### 2.1 The wire surface — `copy-items` does NOT exist, and nothing refuses it typed

- `crates/quilltap-core/src/api/types.rs:2955-2964` `Request::WardrobeItemsMove { scope, container_id, #[serde(flatten)] body: Value }` (R-D scoped family; the body rides RAW so the handler answers v4's flat `Validation error`). No copy variant. A `{"type":"wardrobeItemsCopy",…}` today is a serde `unknown variant` 400 from `dispatch.rs`.
- `api/engine.rs:5791-5810` the `WardrobeItemsMove` arm → `ready_db()` → `wardrobe_route_factory::folder_action(&db, FolderAction::MoveItems, scope, container_id, SINGLE_USER_ID, &body)`. **No transcoder is passed** (the five folder arms use `ready_db()`; contrast `WardrobeItemImageLink` at `:5812` which uses `ready_db_and_blob_webp()` and `unwrap_or_else(RefusingWebpTranscoder)` — the model to copy).
- REST (`crates/quilltap-web/src/wardrobe_routes.rs`): `COLLECTION_POST_ACTIONS: [&str; 5] = ["instructions","create-folder","rename-folder","delete-folder","move-items"]` at `:124-130`; `folder_write_request(action, …)` at `:135-165` whose match arms are `"create-folder"`, `"rename-folder"`, `"delete-folder"` and a **`_ =>` fall-through to `WardrobeItemsMove`** — **adding `"copy-items"` to the constant without a new arm would silently turn a copy into a MOVE** (§6 trap 3). `collection_post` (`:224-278`) maps every folder action to `StatusCode::OK`. `?action=copy-items` is therefore refused today as `400 {"error":"Unknown action: copy-items","availableActions":[5 strings]}`.
- Only TWO REST edges exist in v5: `/api/v1/characters/{id}/wardrobe` (`characters_routes.rs:866-885 characters_wardrobe_post` forwards into `wardrobe_routes::collection_post`; the constant is NOT in `characters_routes.rs`, §9) and `/api/v1/wardrobe` (General, `lib.rs:478`). Projects' and groups' wardrobes are reached only through the dispatch verbs (`scope: project|group` + `containerId`) — there is no `/projects/{id}/wardrobe` or `/groups/{id}/wardrobe` axum route (grep `lib.rs`). The copy therefore needs no new REST path.
- SPA contract: `apps/web/src/app/core/core-contract.ts:4635 WardrobeItemsMoveRequest { type:'wardrobeItemsMove'; scope; containerId?; itemIds: string[]; folder: string }` (member of the union at `:6602-6607`, "P4.D285: wardrobe folders + link-image (append-only)"). No `components`, no copy request.
- Wire-contract/census pins of the move variant (all will want a copy sibling): `quilltap-harness/tests/p4_d279_wire_contract.rs:85-95` (decode asserts the flattened body), `quilltap-web/tests/dispatch_wrong_type_census.rs:2345-2350` (a `Row { variant: "WardrobeItemsMove", field: "scope", rust_type: "WardrobeContainerScope", v4: V4::Query, note }` — the census enumerates every typed field mechanically from `types.rs`, so a new `WardrobeItemsCopy { scope }` variant needs its own row or the census goes red), `quilltap-web/tests/tri_state_edges_share_the_decoder.rs:566` (a comment naming the family).
- CLI: no wardrobe-folder verb exists in `quilltap-cli` (grep `move-items`/`WardrobeItemsMove` under `crates/quilltap-cli` is empty) — no Tier R surface.

### 2.2 The route-factory twin — `crates/quilltap-core/src/api/wardrobe_route_factory.rs` (1,764 lines)

- Module doc `:1-48` — lists the folder actions without `copy-items`; the "never provision" sentence names `move-items`.
- `write_error_response` `:222-242` — maps `Cycle`/`FolderPath`/`ItemNotInLocation` → 400, `FolderNotFound` → 404 `Wardrobe folder not found`, `FolderConflict` → 409, `CharacterArchived` → 409 `An archived character's wardrobe cannot be changed`, `StoreUnavailable` → 503, `NoLinkedVault`/`Db` → 500. There is NO arm for a folder-filing request error (the new `FolderFilingRequestError` — a 400 `bad_request(msg)`), and `WardrobeLocationError` has no such variant (`db/wardrobe_location.rs:64-82`).
- `parse_move_body` `:496-513` — a hand-written Zod port: object; `itemIds` array 1..=500 of `zod_len_min_ok(s,1)` strings; `folder_input(obj,"folder",false)`. It IGNORES `components` today (a body with `components:"nonsense"` passes — v4 would 400). Returns `(Vec<String>, String)`.
- `folder_write` `:918-940` — `with_both_conns(db, move |main, mount| { resolve_location(...)?; op(main, mount, &location) })` with the `Send + 'static` closure; the closure runs ON the writer thread. The op's `Err` → `write_error_response`.
- `items_move` `:1041-1080` — parse → `folder_write(db,cfg,id,user_id,false, |main,mount,loc| { move_items_to_folder(...)?; items = moved.iter().map(item_read_value); Ok((moved.len(), serialize_wardrobe_items(main, items, &loc.origin))) })`; success logs `tier_line!(info, cfg, id_of(&location), "Moved wardrobe items to folder"; userId, mountPointId, folder = requested, count, context = "wardrobe")` ON THE CALLER THREAD; answers `Response::Wardrobe(json!({"items": items}))`. **No `componentsCarried`, no `components`/`componentsCarried` log fields.**
- `FolderAction { List, Create, Rename, Delete, MoveItems }` `:1458-1467`; `folder_action(db, action, scope, container_id, user_id, body)` `:1474-1495` (container-id gate → `config_for(scope)` → the match). **It takes no transcoder** — and `move-items` with `components:'copy'` now ALSO needs one (it copies component pictures), so the signature must grow for the move path too, not just a new copy arm (§4.2).
- `item_read_value` `:290-313` (read order, `id` first) / `item_create_value` `:316-337` (create order, `folder` then `id`) / `serialize_wardrobe_items` `:341-347` (= `attach_wear(with_origin(…))`).
- The capture-pinned module `folder_route_lines` `:1498-1700+`: the `move-items` INFO is pinned byte-wise at `:1640-1645`: `"INFO {HEAD} Moved wardrobe items to folder generalId=null userId={USER} mountPointId={mp} folder=coats count=1 context=wardrobe"` — **this assertion flips** (two new fields before `context`).
- `tier_line!` `:162-185` — the per-tier target + `<logIdKey>` field + `"{} {}", cfg.log_tag, $message`; a new message string is free, the macro is reusable.

### 2.3 The keystone — what the service port can call (all exist)

`services/wardrobe_item_mutations.rs` (644 lines):
- `WardrobeCreateBody` `:155-165` (`title, description: Option<String>, image_prompt, types, appropriateness, is_default: Option<bool>, component_item_ids: Option<Vec<String>>, replace: Option<bool>, folder: Option<String>`), `wardrobe_item_from_create_body` `:194-216` (prose via `jsstr::js_trim`; Set-dedupe via `unique_in_order`), `PreservedIdentity` `:226-234` (`archived_at`/`image_file_id`/`migrated…: Option<Option<String>>`), `CreateItemOptions { lookup, preserve }` `:237-242`, `create_item(main, mount, location, body, opts)` `:246-293` (uses `crate::clock::now_iso()` only when there is no `preserve`; emits `[ItemMutations] Created wardrobe item` with `itemId, composite, types` — `types` as JSON text), `move_items_to_folder(main, mount, location, item_ids, folder)` `:491-523` (resolve → `unique_in_order` → `update_many(.., require_all: true)` → DEBUG `[ItemMutations] Moved wardrobe items to folder`).
- The transfers twin shows the exact "planned item → `WardrobeCreateBody` + `PreservedIdentity`" mapping the copy path needs: `services/wardrobe_transfers.rs:443-484 create_at_destination` (`description: planned.description.clone().flatten()`, `is_default: Some(planned.is_default)`, `component_item_ids: Some(…)`, `folder: Some(planned.folder.clone())`; `preserve` from the planned item's id/stamps/archived/image/migrated). Reuse that shape (or lift it into a shared `item_as_create_body(&WardrobeItem)`; the transfers copy would then call the same fn — a DRY refactor only if it keeps the transfers family byte-green).
- `db/wardrobe_location.rs`: `read_items(&self, mount, include_archived) -> Result<Vec<WardrobeItem>, DbError>` `:225-240` (the mount's own items; `true` = archived included); `update_many(main, mount, &[WardrobeItemPatch], UpdateManyOptions{require_all})` `:285-293`; `resolve_folder(mount, requested) -> Result<String, WardrobeFolderPathError>` `:313-327` (blank → `''`); `writable_mount_point_id(main) -> Result<String, WardrobeLocationError>` `:362-367` (the archived-character 409 / `NoLinkedVault`); `WardrobeLocationError::ItemNotInLocation { item_ids }` `:78` with `.message()` (`not_in_location_message`) byte-equal to v4's sentence.
- `WardrobePatch` (`db/vault_wardrobe_public.rs:143-164`) carries `folder` (RAW — applied through `safe_folder`) and `component_item_ids` — the `move`+`copy` patch needs nothing new. `WardrobeItem` (`vault_overlay.rs:334-…`) with the `Option<Option<String>>` three-state fields.
- `wardrobe::is_composite(&[String]) -> bool` (`wardrobe.rs:648`), `jsstr::{js_trim, is_js_ws, JS_WS_CLASS}` (`jsstr.rs:12-41`).

### 2.4 Pictures — `services/wardrobe_item_images/carry.rs`

`carry_item_images(main, mount, &CarryArgs{ mode: CarryMode::Copy, source_item_id, destination_item_id, destination_mount_point_id, user_id }, &mut dyn FnMut() -> String /*mint file id*/, now_iso, blob_webp: &dyn WebpTranscoder) -> Result<CarriedImages, …>` (`:114`); `CarriedImages::mapped(file_id) -> Option<&str>` (`:100`) is the `fileIdMap.get` twin (insertion-ordered `Vec`). The transfers service drives it in a loop at `wardrobe_transfers.rs:640-690` (the model for the per-copy call, including the post-carry `image_file_id` rewrite: `planned.image_file_id = Some(current.and_then(|id| carried.mapped(id)).map(str::to_string))`). **Seam to thread:** the `&dyn WebpTranscoder` (`services/mount_index/blob_transcode.rs:154`, default `RefusingWebpTranscoder` `:160`) — `Engine::ready_db_and_blob_webp()` (`engine.rs:6404`) hands the production one; the folder-route closure currently has none.

### 2.5 `collect_container_components` (transfers) and `next_copy_title` (MISSING in Rust)

- `services/wardrobe_transfers.rs:404-437 collect_container_components(outfit: &WardrobeItem, container_items: &[WardrobeItem]) -> Vec<WardrobeItem>` — PRIVATE, one-composite form (the OLD v4 shape). Single caller `:549`. Must become the v4 array form (`composites: &[&WardrobeItem]` or `&[WardrobeItem]`, `visited` seeded with every composite id, `component_item_ids` already non-optional in Rust) and move next to its v4 twin: `crates/quilltap-core/src/wardrobe.rs` (the module that holds `expand_composites`, `:455`, the Rust twin of `expand-composites.ts`) — then `wardrobe_transfers.rs:549` calls `collect_container_components(&[source.item.clone()] / std::slice::from_ref(&source.item), …)`.
- **There is NO Rust `next_copy_title`** (grep of `crates` for `copy_title|nextCopyTitle|COPY_SUFFIX` is empty). The ONLY twin is the SPA's `apps/web/src/app/wardrobe/equipped-slots.ts:456-474` (`COPY_SUFFIX`, `nextCopyTitle`), used by `wardrobe-item-actions.ts:227` (Duplicate) and unit-pinned by `equipped-slots.spec.ts:241-250` (5 asserts). So the lane ADDS a Rust `next_copy_title(source_title: &str, existing_titles: &[String]) -> String` (suggest `wardrobe.rs`), built with `jsstr::js_trim` + a hand-rolled right-anchored suffix strip over `is_js_ws` (or `regex` with `JS_WS_CLASS`, which `wardrobe_folders` already does for similar rules), `str::to_lowercase` (byte-identical to JS per CLAUDE.md), ASCII-only `\d` and case-insensitive ASCII `copy` — and it is its first TIER-1 family vs v4's REAL `lib/wardrobe/next-copy-title.ts` (§4.1). The SPA's TS twin is not oracle-checked today (only 5 hand asserts) — the new tier-1 corpus is also the chance to add the same rows to `equipped-slots.spec.ts` (cheap).

### 2.6 Existing tests this lane flips (predicted reds — expect them, do not "fix" the old assertion by deleting)

| test | where | why it flips |
|---|---|---|
| `folder_route_lines` INFO pin | `wardrobe_route_factory.rs:1640-1645` | gains `components=none componentsCarried=0` before `context=wardrobe` |
| `wardrobe_folders_routes_equivalence` `mi_*` rows | `quilltap-web/tests/wardrobe_folders_routes_equivalence.rs` + `harness/oracle/fixtures/wardrobe-folders-routes.json` (86 cases; 16 `mi_*`) | once the oracle regenerates from the pin, every successful move body carries `componentsCarried` |
| unknown-action envelopes | `quilltap-web/tests/wardrobe_instructions_routes.rs:241-247` and `:292-298` | the POST `availableActions` becomes SIX strings, `"move-items","copy-items"` last (the GET list is unchanged: `["instructions","folders"]`) |
| `characters_wardrobe_route.rs:230` | the `?action=move-items` body assert | extra key |
| `p4_d279_wire_contract.rs`, `dispatch_wrong_type_census.rs` | new variant rows | additions, not flips |
| `wardrobe-browser.spec.ts:593-596` | the strict `toEqual` of the move request | stays green ONLY if a plain garment still sends no `components` key (it must) |
| `wardrobe_transfers_tier2_equivalence` | transfers family | MUST stay green through the `collect_container_components` move (one composite = old result) |

### 2.7 Harness families that carry this code (and their fixtures)

- `crates/quilltap-harness/tests/item_mutations_tier2_equivalence.rs` + `harness/oracle/cases/item-mutations-tier2.ts` + `fixtures/item-mutations-tier2.json` (+ `build-item-mutations-fixture.ts`): the DIRECT service-level tier-2 for `createItem/updateItem/setItemArchived/moveItemsToFolder/…` — per op `{op, result|error, items: readItems(true), folders, logs}`, the `[ItemMutations]`/`[FolderMutations]` DEBUG lines captured by splicing the v4 logger's transports (`CAPTURED_PREFIXES`) and v5's with `test_support::captured_with`; normaliser: ONE minted id per op → `<minted:N>`, ISO stamps not in `pinnedStamps` → `<ts>`. The fixture has NO pictures and no nested composites (4 seeded items: two in General, two in the vault).
- `crates/quilltap-harness/tests/wardrobe_transfers_tier2_equivalence.rs` + `harness/oracle/cases/wardrobe-transfers.test.ts` + `fixtures/wardrobe-transfers-tier2.json` + `build-wardrobe-transfers-fixture.ts`: the seven-table structural diff (`doc_mount_points/_files/_documents/_file_links/_folders` + `project_/group_doc_mount_links`) plus `doc_mount_blobs`, the MAIN `files` and `wardrobe_wear_stats`, with the full minted-id remap machinery (`normalize_all` `:457`, first-seen UUID map across tables, embedded `.md` content, recomputed `contentSha256`). Its fixture ALREADY seeds what a folder-filing DB-state diff needs: pictures through v4's REAL `addWardrobeItemImage` (uploaded/generated/imported, three webp byte samples), a four-item nested composite family (`compositeItems`: Coat, Cufflinks, Formal set[cufflinks, **shared goggles**], Outfit[…]), an archived source character, a filed item in `Formal`, a hand-made folder item, wear rows.
- `crates/quilltap-web/tests/wardrobe_folders_routes_equivalence.rs` (385 lines) + `harness/oracle/cases/wardrobe-folders-routes.test.ts` (304 lines; jest, v4's REAL four route files behind the real middleware, a fresh copy of the keystone four-tier seed per case, `steps` of `{op, body, items}`, per-step `{status, body}` + the mount-index write-set = every `doc_mount_file_links.relativePath` and `doc_mount_folders.path`; Rust normaliser blanks `createdAt`/`updatedAt` and a create's minted `wardrobeItem.id`) + `fixtures/wardrobe-folders-routes.json` (86 cases). Its seed (`wardrobe-keystone-tier2.json` + `build-wardrobe-keystone-fixture.ts`) has composites, a borrowed `generalWatch`/`generalBoots`, archived `archivedCape`, an archived character `zed`, a bare/null project — but NO pictures.
- `wardrobe_folder_writes_tier2_equivalence.rs` covers the folder MUTATION writers (create/rename/delete-folder over the document store), NOT `move-items` — it is not the home for this work (the survey brief named it; the right homes are the two above).
- Pure tier-1 precedent for the two leaves: `composite_types_equivalence.rs` + `harness/oracle/cases/composite-types.ts` (the v4 pure leaf of `wardrobe.rs`), `wardrobe_folders_equivalence.rs` + `cases/wardrobe-folders.ts`.

### 2.8 The SPA (apps/web/src/app/wardrobe/…)

- `wardrobe-folders.api.ts:107-123 moveWardrobeItemsToFolder(core, container, itemIds, folder)` — dispatches `{ type:'wardrobeItemsMove', ...containerFields(container), itemIds, folder }` and returns `data['items'] ?? []`; no `components`; no copy fn. Spec `wardrobe-folders.api.spec.ts:67` pins the dispatched object.
- `wardrobe-folder-picker-dialog.ts` (269 lines) — header comment says "unchanged by `97f0d8cb8`"; title `'Move ' + subject() + ' to a folder'` (`:80`), helper line (`:84-86`), footer `{{ moving() ? 'Moving…' : 'Move' }}` (`:123`), inputs `container/subject/currentFolder/moving`, outputs `closed/move/foldersChanged`, `isCurrent` (`:196-199`: NOT mode-gated), `canMove` (`:184-187`), `submitMove` (`:211-214`), Escape (`:240-244`), the folders signal store + inline New folder. No mode, no outfits, no fieldset.
- `wardrobe-transfer-dialog.ts` (409 lines) — carries its OWN inline `<fieldset>` (`:154-175`) + `componentOptions` (`:272-290`) + `componentCountLabel` (`:292-296`) + `componentMode` signal with `mode()==='move' ? 'move' : 'copy'` default set in the constructor effect (`:225`). P4.D286 did NOT extract the shared fieldset (the ledger row's "swaps its radio group" is still future work). `WardrobeTransferComponentMode` is declared in `wardrobe.api.ts:96`.
- `wardrobe-browser.ts` (964 lines) — select bar (`:488-509`): `Move to folder…` + `Cancel`; row binds `[canMoveToFolder]="true" (moveToFolder)="openPickerFor([$event])"` (`:483-484`); `openPickerFor(items)` (`:906-908`) → `actions().openFolderPicker(items)`; `pickerHome`/`pickerCurrentFolder`/`pickerSubject` (`:917-932`); `moveToFolder(folder)` (`:934-963`) with the toasts and the `finally { moving.set(false); void actions().refreshLists() }`; the dialog binding (`:528-540`).
- `wardrobe-item-actions.ts` — `FolderPickerRequest { items }` (`:76-78`), store interface `openFolderPicker(items)` (`:128`), impl `:305-307`; Duplicate at `:222-241` (with the `console.debug` that v4's `01f508d4e` deletes).
- `wardrobe-item-row.ts` — the kebab entry `Move to folder…` is `@if (manageable() && canMoveToFolder())` (`:300-311`); the input `canMoveToFolder` (`:447`, boolean-attribute, default false) and output `moveToFolder` (`:473`); nested rows forward neither (`:353-383` comment).
- `expand-composites.ts` (121 lines) has no `collectContainerComponents` and needs none: v4's React never calls the walk client-side (grep of `components app lib` at the pin: only `folder-filing.ts` and `transfers/route.ts`). Do NOT add a dead export.
- `folders.ts` — `isSameFolder`, `folderCrumb`, `folderName`, `joinFolder` (the picker/browser already use them). `plural` comes from the browser's imports.
- e2e: `apps/web/e2e/wardrobe-folders-flow.spec.ts` (510 lines, port 4336, `P4D279_SERVER_LANDED = true`, `P4D286_ROW_LANDED = true`; the `Move to folder…` beat at `:336-392` is the model — Aria's `Flight Jacket`, `Formal` folder, `toasts(page)`, `itemRow`, `folderRow`, `openAriasWardrobe`).

### 2.9 Carriers — confirmed unaffected

No stored field moves: the `.qtap` export/import, backup/restore and the wardrobe vault documents carry items by their EXISTING fields. A copy is an ordinary new item (new id, `(copy)` title) written through `createItem`; nothing records "copied from". `wardrobe_wear_stats` gets no row for a copy (a fresh id → empty ledger; the doc comment's "empty wear ledger"). No new `files` columns. The only persisted effects are ordinary item documents + picture `files`/`doc_mount_blobs`/`doc_mount_file_links` rows already produced by the transfer's copy mode. Equipped state/avatars are untouched by any filing action (no `notifyWardrobeChanged` / avatar refresh call in `fileItemsToFolder`), so none of the equip→avatar machinery (memory `shared-avatar-resolver-propagates`) is involved.

---

## 3. Tier split

| unit | tier | oracle | notes |
|---|---|---|---|
| `next_copy_title` | **1** (pure, exact strings) | v4's REAL `lib/wardrobe/next-copy-title.ts` via tsx | corpus §4.1; the SPA twin gets the same rows as a jest-free vitest/Karma assertion |
| `collect_container_components` (array form) | **1/2** | no pure v4 oracle needed — covered by the tier-2 service family (BFS order is observable in the result and in the log) plus a pure Rust unit test of order/visited/borrowed/cycle; ALSO keeps `wardrobe_transfers_tier2` green | optionally import v4's real fn in the tier-2 oracle and emit its output for the fixture graph |
| request-body parse (`components`, copy schema) | **2 (route)** | v4's REAL route files (zod throws → flat 400) | in `wardrobe-folders-routes` cases |
| `file_items_to_folder` (5 combos + refusals + logs + DB writes + pictures) | **2** (structural DB diff + log capture) | v4's REAL `fileItemsToFolder` over a REAL `resolveWardrobeLocation` (tsx, direct) | new family §4.2 |
| route factory: `copy-items`, `move-items.components`, status/bodies, INFO line, dispatch verb, REST action list | **2** (web) + capture-pinned unit | v4's REAL four route files | extend `wardrobe_folders_routes_equivalence` §4.3 |
| SPA picker/fieldset/browser/row/actions/api | **vitest/Karma component specs** (transcription of v4's jest) + 1 e2e beat | v4's `wardrobe-browser.folders.test.tsx` transcribed | §4.4/§4.5 |
| mocked-LLM | **3 — none** | no model call in this slice | |
| CLI Tier R | **none** | no CLI surface | |

---

## 4. Differential plan (what an unattended lane builds)

### 4.1 Tier-1: `next_copy_title`

New oracle `harness/oracle/cases/next-copy-title.ts` (pattern: `composite-types.ts`): imports the REAL `nextCopyTitle` from `@/lib/wardrobe/next-copy-title` and emits NDJSON `{ source, existing, out }` for a fixed corpus; new Rust `crates/quilltap-harness/tests/next_copy_title_equivalence.rs` (env `QT_ORACLE_NEXT_COPY_TITLE`, skip when unset — standard) compares exactly. Corpus (v4's has NO unit test file of its own — author these):

`Shirt`/[]; `Shirt`/[`shirt (COPY)`]; /[`Shirt (copy)`,`Shirt (copy 2)`] → `(copy 3)`; `Shirt (copy)`/[`Shirt (copy)`] → `(copy 2)`; `Shirt (copy 4)`/[] → `Shirt (copy)`; `Shirt  (copy)  ` (double space inside, trailing spaces); tab/NBSP (U+00A0)/U+FEFF/U+2028/U+3000 before and after the suffix; `Shirt(copy)` (no leading space: stripped); `Shirt (copy2)` (no space before digits: NOT stripped → `Shirt (copy2) (copy)`); `Shirt (copy 0)`, `(copy 007)`, `(copy 1234567890123)`; `(COPY 12)` upper; `Shirt (copy) (copy)` (only the LAST suffix strips → `Shirt (copy) (copy)`→ base `Shirt (copy)`); title ONLY `(copy)` and `  (copy 3)  ` (base empty → fallback `sourceTitle.trim()` → `(copy) (copy)`); empty title `''` → `' (copy)'`? (JS: base `''` → `|| ''.trim()` = `''` → `' (copy)'`; the corpus must pin whatever v4 emits); whitespace-only title; existing titles with padding/case (`  SHIRT (COPY)  `), non-ASCII (`Ünïcode`, `İstanbul` — `toLowerCase` expands to `i̇`; final sigma `ΑΣ`), astral chars, a 300-char title; existing list containing the candidate only in a different case at n=1 AND n=2 (chain to 3); a long chain 1..12.

Also add the same rows to `apps/web/src/app/wardrobe/equipped-slots.spec.ts` (the SPA twin) — same strings, no oracle needed (the NDJSON can be vendored as a committed fixture if the unifier wants).

### 4.2 Tier-2: `file_items_to_folder` (the heart)

**Rust shape to build** (suggested; mirror the neighbours):

- New `crates/quilltap-core/src/services/wardrobe_folder_filing.rs` (declare in `services/mod.rs`; alphabetically beside `wardrobe_folder_mutations`):
  `pub enum FolderFilingAction { Move, Copy }`, `pub enum FolderComponentMode { Move, Copy, None }` (+ `from_wire(&str)`), `pub struct FileItemsToFolderRequest<'a> { action, item_ids: &'a [String], folder: &'a str, components: Option<FolderComponentMode> }`, `pub struct FileItemsToFolderResult { items: Vec<WardrobeItem>, components_carried: usize }`, `pub enum FolderFilingError { Request(String), Location(WardrobeLocationError) }` with `From<WardrobeLocationError>` (the `?` ladder) and the exact sentence `Copying an outfit cannot move its components — the original outfit still needs them`.
  `pub fn file_items_to_folder(main:&Connection, mount:&Connection, user_id:&str, location:&WardrobeLocation, req:&FileItemsToFolderRequest, mint:&mut dyn FnMut()->String /*uuid*/, now_iso:&str, blob_webp:&dyn WebpTranscoder) -> Result<FileItemsToFolderResult, FolderFilingError>` — sync, like its siblings (injected `mint`/`now_iso` for determinism, the `transfer_wardrobe_item` idiom; production passes `uuid::Uuid::new_v4().to_string()` and `crate::clock::now_iso()`). `userId` is the single user (`SINGLE_USER_ID`).
  Port the 18 steps of §1.3 in order; carry v4's *why* comments (the missing-id-before-write guarantee, components-first, the `idMap`-over-requested rule, why the plain path delegates, the title-collision/UUID-fallback reason, archived-copy-stays-archived). Emit the `[FolderFiling] Filed wardrobe items into folder` DEBUG with fields `scope, mountPointId, action, componentMode, folder, requested, componentsCarried, context` (use the same `tracing::debug!(scope = location.scope.as_str(), mountPointId = %…, action = …, componentMode = …, folder = %target, requested = n, componentsCarried = n, context = "wardrobe", "[FolderFiling] Filed wardrobe items into folder")` idiom as `move_items_to_folder`). `action`/`componentMode` render as the lowercase words `move`/`copy` and `move`/`copy`/`none`.
  The plain path MUST call the existing `move_items_to_folder` and emit nothing else (so it stays byte-identical to today's `move-items`).
  Per copy: `next_copy_title(&original.title, &taken_titles)`; `carry_item_images(main, mount, &CarryArgs{ mode: Copy, source_item_id: &original.id, destination_item_id: &new_id, destination_mount_point_id: &mount_point_id, user_id }, &mut mint_file_id, now_iso, blob_webp)?`; `create_item(main, mount, location, &body, CreateItemOptions{ lookup: None, preserve: Some(&PreservedIdentity{ id: new_id, created_at: now, updated_at: now, archived_at: Some(original.archived_at.clone().flatten()), image_file_id: Some(original.image_file_id.clone().flatten().and_then(|f| carried.mapped(&f)).map(str::to_string)), migrated_from_clothing_record_id: Some(original.migrated_from_clothing_record_id.clone().flatten()) }) })?`. `mint_file_id` for the picture rows must be a SEPARATE closure from the item-id mint if the test wants stable tokens (the transfers service uses `|| uuid::Uuid::new_v4().to_string()`).
  The `move`+`copy` patch: `WardrobeItemPatch { id, patch: WardrobePatch { folder: Some(target.clone()), component_item_ids: if is_composite(item) { Some(remap(&item.component_item_ids)) } else { None }, ..Default::default() } }` with `UpdateManyOptions { require_all: true }`.
- Route factory (`api/wardrobe_route_factory.rs`): replace `parse_move_body` by a shared `parse_filing_body(body, allow_move_components: bool) -> Result<(Vec<String>, String, Option<FolderComponentMode>), ()>` (copy: `components` ∈ {`copy`,`none`}, else Err; move: ∈ {`move`,`copy`,`none`}; absent OK; `null`/non-string Err); `items_move` + NEW `items_copy` over one `file_items(action, …)` helper; the INFO message is `Moved wardrobe items to folder` / `Copied wardrobe items to folder`; fields `userId, mountPointId, folder (as requested), count, components (body.components ?? "none"), componentsCarried, context`; answer `Response::Wardrobe(json!({"items": items, "componentsCarried": n}))`. Item serialization: `move`/`move+copy` results through `item_read_value`; **`copy` results through `item_create_value`** (§1.4 — the three key orders). Map `FolderFilingError::Request(m)` → `bad_request(m)`, `Location(e)` → `write_error_response(e)` (add the arm; unreachable on the wire for copy+move because the parser refuses first — keep it for the service-level contract).
  `folder_action` / `FolderAction`: add `CopyItems`, and give the function a `blob_webp: Arc<dyn WebpTranscoder>` argument (the five engine arms + the web test's five call sites change mechanically; `List/Create/Rename/Delete` ignore it). The closure passed to `folder_write` is `Send + 'static`: move an `Arc` clone in and take `.as_ref()` inside (the `link_image` idiom, `wardrobe_item_images.rs:802-841`). **`move-items` with `components:'copy'` needs the transcoder too** — a design that adds the transcoder only to a new copy arm is wrong.
- Engine/types/web: `types.rs` NEW `Request::WardrobeItemsCopy { scope, container_id, #[serde(flatten)] body }` beside `:2958`; `engine.rs` arm beside `:5791` using `ready_db_and_blob_webp()` + `unwrap_or_else(|| Arc::new(RefusingWebpTranscoder))` for BOTH the move and copy arms (the move arm's current `ready_db()` call changes); `wardrobe_routes.rs`: `COLLECTION_POST_ACTIONS` → 6 entries (`"move-items"`, `"copy-items"` last) and `folder_write_request` gets an explicit `"copy-items" => CoreRequest::WardrobeItemsCopy {…}` arm AND the `_ =>` fall-through is replaced by an explicit `"move-items"` arm + an `unreachable`-free default (return an error response rather than defaulting to a move — a trap, §6). The web-edge body-parse census (`web_edge_body_parse_guard.rs` COLLAPSE_CENSUS) counts `serde_json::from_str`-style body reads per file: `parse_body` already exists in `wardrobe_routes.rs`; no new site → no census row change (verify by running it; if the lane adds a body read, add a row).
- If `engine.rs` is frozen for this round (memory `frozen-engine-seams-and-web-censuses`), land the typed variant + route-factory + web and record a HANDOFF for the engine arm — but an unwired variant is dead code the dispatch would reject, so prefer to do the arm; the arm is 20 lines.

**Oracle (v4 side):** NEW `harness/oracle/cases/folder-filing-tier2.ts` (tsx, modelled on `item-mutations-tier2.ts` — splice the logger transports, `LOG_LEVEL=debug`, `CAPTURED_PREFIXES = ['[FolderFiling]','[ItemMutations]','[FolderMutations]','[WardrobeImages]']` plus the bare `Wardrobe item created in folder`) importing the REAL `fileItemsToFolder` from `@/lib/wardrobe/folder-filing` and calling it with real `getRepositories()` (see how `item-mutations-tier2.ts` obtains `repos` for `componentLookupFor`) and a real `resolveWardrobeLocation('character'|'general'|'project'|'group', …)`. Per op record `{ op, result | error:{name,message}, items: location.readItems(true), folders: location.listFolders(), logs, tables? }` where `tables` = the transfers family's seven mount-index tables + `doc_mount_blobs` + MAIN `files` (copy-picture rows) + `wardrobe_wear_stats`. The error record keeps `error.name` (`FolderFilingRequestError` / `WardrobeItemNotInLocationError` / `CharacterArchivedError` / `WardrobeFolderPathError`) — the Rust side maps its enum to the same names.

**Fixture:** `build-folder-filing-fixture.ts` composes the item-mutations builder (character + vault + General) with the transfers builder's picture seeding (`addWardrobeItemImage`, three webp samples). Seed (UUID-shaped ids only — `d9…`/`e5…` style, vault parsers reject non-UUID ids silently; memory `memory-raw-reads-drop-non-uuid-fixtures` is the analogue):
  - `shirt` (`Shirt`, TWO pictures — uploaded + generated — `imageFileId` → the first), `trousers`, `suit` [shirt, trousers], `outfit` [suit, `generalHat`] with `generalHat` a REAL General archetype (borrowed), `lonely` leaf (no pictures), `defaultSash` (`isDefault:true, replace:true, appropriateness:"Rain."`), `archivedCape` (`archivedAt` set, one picture), a PRE-EXISTING `Shirt (copy)` (forces `(copy 2)`) and `Outfit (copy)`-style collision, an item already in `Formal` and a hand-made folder item (`hats: summer`), a diamond (`piece` named by TWO outfits), a cycle-tolerance row (old-data cycle planted raw, only if the keystone fixture already plants one), a composite whose component is an ARCHIVED item;
  - owners: the character vault, General, a project store, a group store (the filing code is tier-agnostic but `scope` appears in logs and `origin` in route bodies), an ARCHIVED character (the 409 ordering).
  Wear rows for `shirt` (a copy must NOT inherit them).

**Corpus (ops):** the 8 v4 unit cases + the matrix the unit tests omit:
`move/none` (plain; assert NO `[FolderFiling]`); `move/none` with dupes; `move/move` of a composite (closure order + ids), of a leaf (components `[]`, `componentsCarried 0`, `[FolderFiling]` still logged), of two outfits sharing a piece (closure dedupe), of an outfit AND its component together (component not double-moved); `move/copy` (outfit rewired, originals stay, a requested leaf only gets `folder`); `copy/none` (single, several, dupes, root target `''`, NEW-folder target `Brand New/Sub`, hand-made target, archived original → archived copy, default/replace/appropriateness carried, `imageFileId` repointed to the copy's own file, title collision `(copy 2)`, two same-titled items in one request → `(copy)` + `(copy 2)`); `copy/copy` (nested, shared piece copied ONCE, borrowed `generalHat` kept, pictures per copy, requested leaf + its parent outfit together → remap over requested items); `copy/move` (the service refusal — error name + message); borrowed id in a `move/none` (updateMany refusal), `move/move`, `copy/copy` (the filing-path refusal, NOTHING written — assert tables unchanged); folder-path refusal (`images`, `../x`) vs missing id ORDER (path first); archived character: missing-id vs 409 ORDER on the copy path (400 wins), plain `move/none` (409 from updateMany), `copy/none` (409 from `writableMountPointId`) with NO side effects (no picture rows); General / project / group tiers one case each of `move/move` and `copy/copy`.
`takenTitles` includes archived items' titles; the folder-casing rule (`formal` → `Formal`).

**Rust test:** `crates/quilltap-harness/tests/folder_filing_tier2_equivalence.rs`: same fixture copy per op-chain, run `file_items_to_folder` with deterministic `mint` (counter-based UUIDs `…-0000000000NN`) and a fixed `now_iso`, capture logs with `test_support::captured_with`, diff `{result, items, folders, logs, tables}` after normalisation. **Normaliser** (the item-mutations rule "at most one minted id per op" is INSUFFICIENT — a copy mints several): after the op, the newly appeared item ids (post-op `readItems` minus pre-op) are tokenised by their (deterministic) copy TITLE — `<minted:Shirt (copy)>` — in `items`, `componentItemIds`, results, log fields, `.md` content, `storageKey`s, `linkedTo`/`tags`, and the `Wardrobe/images/<id>/…` link paths; new `files` ids by `<minted-file:<item title>#<n>>`; stamps: every ISO that is not in `pinnedStamps` → `<ts>` but ALSO assert in Rust that every copy of one call shares ONE `createdAt == updatedAt` (v4's single `now`) and that the moved items' `updatedAt` is restamped. The transfers family's first-seen UUID map + recomputed `contentSha256` machinery is the template for the table diff; copy it or hoist `normalize_all` into a shared support module (`crates/quilltap-harness/tests/wardrobe_images_support` is the existing shared dir — check what it already exports before duplicating).
Order assertions: the `logs` array is compared as a LIST (the keystone lines' order per copy — `Created wardrobe item` per copy, components first — is a v4 invariant the copy order alone would hide), but items/tables are sorted.

### 4.3 Tier-2 (route): extend `wardrobe-folders-routes`

- `harness/oracle/cases/wardrobe-folders-routes.test.ts`: add a `case 'copy-items'` identical to `move-items` (`POST ${B}?action=copy-items`, `s.items` expanded the same way) and let `move-items` steps carry `components` through `s.body`. `fixtures/wardrobe-folders-routes.json#cases`: append (the Rust test asserts `oracle.len() == spec.cases.len()`, so BOTH sides regenerate): `mi_char_components_move` (composite family from the seed: `composite` + its same-folder pieces; borrowed `generalWatch` stays), `mi_char_components_copy`, `mi_char_components_none_explicit`, `mi_char_components_bad_enum_zod` (`"Move"`), `mi_char_components_null_zod`, `mi_char_components_leaf_only` (`components:'move'` over a leaf), `ci_char_ok` (copy a leaf), `ci_char_root_target`, `ci_char_new_folder`, `ci_char_copy_components_copy`, `ci_char_components_move_zod` (400 Validation error — the SCHEMA refusal, not the sentence), `ci_char_components_null_zod`, `ci_char_borrowed` (400 sentence naming the id), `ci_char_dupes`, `ci_char_501_zod`, `ci_char_empty_ids_zod`, `ci_char_folder_missing_zod`, `ci_char_archived` (zed: 409 — NOTE order: all ids exist), `ci_char_missing_id_archived` (400 wins over 409), `ci_general_ok`, `ci_project_ok`, `ci_group_borrowed_general`, `ci_unknown_owner_404`, `ci_foreign_owner_404`; plus a chained `ci_char_then_list` (copy then `list`) to see both items in the list order.
- Rust side: `FolderAction::CopyItems` step (`"copy-items"`), and a normaliser extension: a copy's response `items[*].id` (and `componentItemIds` pointing at copies) are minted — token them like the create case (`<minted:N>` by order of first appearance in `items`) and mask `…/images/<id>/…`-style link paths in the `links` dump (the dump compares paths verbatim today; a copy's `Wardrobe/<slug>.md` path derives from the `(copy)` TITLE and is deterministic, but a UUID-fallback path would not be — keep fixture titles unique so slugs never fall back).
- `status`: success 200 (`answer()` returns 201 only for `create`). Refusals: status + `error` string only.
- Capture-pinned INFO lines: extend `folder_route_lines` with (a) the updated `Moved …` line (`… folder=coats count=1 components=none componentsCarried=0 context=wardrobe`), (b) `Copied wardrobe items to folder` for a copy with `components=copy componentsCarried=N`, (c) silence on every refusal (400/404/409 log no route line). The oracle is not captured for these (the routes oracle sets `LOG_LEVEL=error`), so they are v5-side literals checked against §1.7 — say so in the test header and DO NOT call them oracle-pinned (memory `unify-review-finding-shapes`: false "pinned" claims).
- Dispatch-level: `p4_d279_wire_contract.rs` gets a `wardrobeItemsCopy` decode assert (flattened body + `components`); `dispatch_wrong_type_census.rs` gets the `WardrobeItemsCopy`/`scope` row; `wardrobe_instructions_routes.rs` two `availableActions` asserts → six strings; `characters_wardrobe_route.rs` move test gains `componentsCarried`; a REST test that `POST /api/v1/wardrobe?action=copy-items` really copies (and `?action=move-items` does not copy — the fall-through guard).

### 4.4 SPA specs (vitest/Karma — the repo's component-spec harness; memory `spa-component-harness-gotchas`)

Transcribe v4's two browser tests into `wardrobe-browser.spec.ts` ("ungated twin" convention) and add:
- `wardrobe-folders.api.spec.ts`: `copyWardrobeItemsToFolder(core, container, itemIds, folder, components?)` dispatches `{ type:'wardrobeItemsCopy', scope, containerId, itemIds, folder }` with `components` ONLY when set (strict `toEqual` — key absence), returns `data['items'] ?? []`; `moveWardrobeItemsToFolder` with `components` adds the key only when set.
- `wardrobe-folder-picker-dialog.spec.ts` (+8): copy-mode title `Copy "X" to a folder`, helper line verbatim, `Copy`/`Copying…`, the current folder NOT disabled in copy mode and no `(here now)`; fieldset absent for leaves, present for outfits (legend for 1 and for N outfits incl. singular `1 component`), default radios (`move` → `Move the components along with it` checked; `copy` → `Copy the components along with it`), the footnote text (folder variant), radios disabled while working, submit passes `components` only with outfits.
- New `wardrobe-component-mode-fieldset.spec.ts`: labels per mode, `defaultComponentMode`, both footnotes verbatim, `name` grouping, `disabled`.
- `wardrobe-transfer-dialog.spec.ts`: existing assertions stay green after the extraction (radio `name="wardrobe-transfer-components"`, labels, the `wardrobeTransferApply` body's `components`).
- `wardrobe-browser.spec.ts` (+4): select bar `Copy to folder…` disabled at 0 selected and wired to copy mode; row kebab `Copy to folder…` after `Move to folder…`; copy toasts (`A copy of "Greatcoat" now hangs in "Formal"` / `Copies of 2 items now hang in the top of the wardrobe`, using `filed[0]?.folder` for the crumb); copy error fallback `The items would not be copied`; copy dispatches `wardrobeItemsCopy` and drops `components:'move'` defensively; outfit move sends `components`; a plain garment still sends none (the existing strict assertion at `:593`).
- `wardrobe-item-actions.spec.ts`: `openFolderPicker(items, mode?)` defaults `'move'`; `folderPicker().mode`. `wardrobe-item-row.spec.ts`: the entry shows only when `manageable && canCopyToFolder`.
Angular specifics: mirror `canMoveToFolder` with a `canCopyToFolder` input (or reuse the one flag — v4 gates each by its own optional prop, so two inputs are faithful) and a `copyToFolder` output; the picker's `move` output becomes `submitted` with `{ folder, components? }` (or two outputs) and `moving` input → `working` (rename or keep — either passes the transcribed tests if the spec's host is updated; prefer v4's names for a clean future diff); new `WardrobeComponentModeFieldset` standalone component (`wardrobe-component-mode-fieldset.ts`) used by BOTH dialogs; `componentMode` signal reset when `mode` changes while open (v4's effect on `[isOpen, mode]`; the Angular dialog mounts per open so the constructor read suffices — say so in the comment as the transfer dialog does).

### 4.5 e2e beat

Extend `apps/web/e2e/wardrobe-folders-flow.spec.ts` (port 4336, Aria): after the Move beat, a beat gated by a new `P4DXXX_COPY_LANDED` flag (the file's own §S.9 convention — the unifier flips it): open Aria's wardrobe → `Flight Jacket` kebab → menuitem `Copy to folder…` → listbox `Folders`; title `Copy "Flight Jacket" to a folder`; the helper text; the folder the jacket is IN stays enabled (no `(here now)`); pick it → `Copy` enabled → click → toast `A copy of "Flight Jacket" now hangs in "Formal"` (or the root phrasing) → the list shows `Flight Jacket (copy)`; a second copy → `Flight Jacket (copy 2)`; the original is still there. An outfit-asks beat needs an outfit fixture (creating one in the UI is long; the component spec carries the radio contract — if the lane wants it live, compose via the item editor's component picker).

---

## 5. Fixtures (consolidated)

1. **tier-1:** none (NDJSON only).
2. **tier-2 service:** NEW `harness/oracle/fixtures/folder-filing-tier2.json` + `build-folder-filing-fixture.ts` (seed + `ops`), built into `/tmp/qt-ffl-main.db` / `/tmp/qt-ffl-mount.db` (UNIQUE `/tmp/qt-ffl-*` prefixes — memory `parallel-lane-shared-resources`: shared `/tmp` recipe paths collide across lanes). Recipe header: from `cd ~/source/quilltap-server` (the sweep driver maps only that `cd` to `--v4`; vars don't cross the regen/run split; pass `--v5w ~/source/quilltap-v5` — the spaced real path breaks regens). Oracle env names: `QT_FIXTURE_FFL_MAIN`, `QT_FIXTURE_FFL_MOUNT`, `QT_ORACLE_FOLDER_FILING`. Pictures need v4's real `addWardrobeItemImage` — copy the transfers builder's `webp` byte samples and seeding loop.
3. **tier-2 route:** `wardrobe-folders-routes.json` grows by ~24 cases; the keystone seed (`wardrobe-keystone-tier2.json`) may need ONE composite family with same-wardrobe pieces if the existing `composite` item has none (check `items` keys: `composite`, `rootCoat`, `filedGown`, `generalWatch`, `archivedCape`, `groupCloak`, `generalBoots`, `projectLivery`, `projectBoots`) — extend the builder rather than the committed JSON by hand.
4. **Help / docs vendoring** (byte-equal): `help/wardrobe.md` and `help/project-wardrobe.md` are today byte-identical to v4 `83334b9a5`. bbbcd2388's hunks (verbatim, to apply as a patch so the OTHER commits' hunks in the same files — `7458ff94e`, `f0db8cbb8` touch `help/wardrobe.md` — are left to their own orders):
   - `help/wardrobe.md`, in the Folders section: after the `Move to folder…` bullet add `- **Copy to folder…**, its neighbour in the menu, works the same way but files a *copy* and leaves the original hanging where it was. The copy is a garment in its own right --- a fresh identity, a ledger that starts at nothing, its own copies of any pictures --- and takes a *(copy)* title, just as **Duplicate** does, so the two are never confused. A copy may land in the very folder its original occupies.` and `- **Outfits ask after their components.** Move or copy an outfit to a folder and the picker poses the same all-or-nothing question it poses when an outfit is sent to another wardrobe (see *Shared Items* above): *move* the components along, *copy* them (originals stay put, and the outfit is rewired to the copies), or leave them be; a copy offers only to copy the components too, or not. Nested pieces are included, and pieces borrowed from a shared tier never stir. Copied components take *(copy)* titles of their own.`; the select-bar bullet's `*N selected · Move to folder… · Cancel*` → `*N selected · Move to folder… · Copy to folder… · Cancel*`; the closing paragraph's `(and moving an outfit moves only the outfit, never its pieces)` → `(and moving an outfit moves its pieces only if you say so when asked)`.
   - `help/project-wardrobe.md`: the **File** bullet → `**File** the collection into folders --- **New folder**, **Move to folder…** or **Copy to folder…** in a row's **⋮** menu (an outfit asks whether its components should come along), or **Select** to file several at once --- and browse it by breadcrumb. Folders are for your convenience only; see [Folders](wardrobe.md#folders-filing-the-closet).`
   - `docs/v4/developer/API.md` (2 rows: `move-items` rewritten, `copy-items` added — verbatim in `git show bbbcd2388 -- docs/developer/API.md`), `docs/v4/developer/features/complete/wardrobe-folders.md` (2 rows), the `docs/v4` CHANGELOG is not mirrored. The help COUNT stays 131.
5. **No `migration_tables.json` / `fresh_schema.json` / `migration_indexes.json` change** (verified: no DDL).

---

## 6. Traps (memory notes + what the code shows)

1. **Three key orders on one `items` field.** `move-items` plain/`move+move`/`move+copy` return `updateMany`'s `{...before,...patch}` — READ order (`item_read_value`, `id` first, `folder` after `archivedAt`); `copy-items` returns `createItem`'s stored object — CREATE order (`item_create_value`, `folder` then `id`). Compare bytes (`serde_json::to_string`, never `Value ==` — memory `serde-json-map-eq-ignores-order`; and "spread-over-undefined keeps schema key order" in `unify-review-finding-shapes`). A `move+copy`'s requested outfits are read-order; the COPIES are only reachable via a separate `list`.
2. **Minted ids are plural per op** (every component copy + every requested copy + every picture `files` row). The item-mutations "one minted id per op" normaliser cannot be reused as is; token by copy title (§4.2). Content-addressed `.md` bodies embed the minted id AND `createdAt`/`updatedAt` — `contentSha256`/`sha256` must be recomputed after normalisation (the transfers family does; memory `shared-fixture-rebuilds-and-jest-setup-mocks`: recipes that rebuild a shared fixture re-mint ids per oracle).
3. **`folder_write_request`'s `_ =>` fall-through (wardrobe_routes.rs:156)** makes any future folder action a MOVE; a copy added to `COLLECTION_POST_ACTIONS` first would copy-as-move silently (data corruption class). Add the explicit arms and a REST test that `copy-items` copies and leaves the original. Also dispatcher action LISTS are pinned in two other files (memory: "dispatcher action lists" in `unify-review-finding-shapes`): `wardrobe_instructions_routes.rs:241,292`.
4. **The transcoder seam is not optional for `move-items`.** `components:'copy'` on a MOVE copies pictures. `ready_db()` → `ready_db_and_blob_webp()` for both arms; `RefusingWebpTranscoder` stores the original bytes (v4's own encode-failure fallback) — the differential's picture rows use the SAME fixture webp samples, so the refusing encoder is what the tier-2 test passes (the transfers test does: `wardrobe_transfers_tier2_equivalence.rs:747`). Don't let the oracle side transcode: v4's jest/tsx side has a real `sharp`. VERIFY (not measured here) that the transfers fixture's picture bytes make v4's transcode a pass-through, as the transfers differential evidently relies on; reuse exactly those samples (`webp` in the transfers JSON) and, if a copy's stored bytes differ between sides, that is the cause.
5. **Error ORDER is part of the contract** (§1.3 steps 4/7/14): folder-path 400 → missing-id 400 (before any write) → archived 409 (copy path: `writableMountPointId`; plain/move+move: `updateMany`). v4's own unit tests pin none of the ordering; the corpus MUST (§4.2).
6. **`takenTitles`/`readItems(true)` includes archived items**, and the copy loop pushes each new title, so one request with two same-titled items yields `(copy)` and `(copy 2)`; a pre-existing `Shirt (copy)` in a DIFFERENT folder still collides (the whole wardrobe is the namespace — the reason given in the module doc: slug collisions make the vault fall back to UUID paths). Folder-qualified slugs are the keystone's business; the corpus's unique titles keep both sides off the UUID fallback.
7. **`isComposite` is "has ≥1 component id", not "has ≥1 resolvable component".** An outfit whose only components are borrowed is composite: `components:'move'` over it moves nothing extra (`componentsCarried 0`) but still logs `[FolderFiling]` and still goes down the `move+move` branch. And the picker's legend counts RAW ids (borrowed included).
8. **Single `now`** for all copies; copies' `createdAt === updatedAt`; moved items restamp `updatedAt` at write (`updateMany`'s own `now`). v5's `create_item` takes the preserve stamps verbatim (no `clock::now_iso()` when `preserve` is set) — good; but `carry_item_images` takes `now_iso` for the picture rows (pass the same). Memory `v4-safequery-writes-and-v5-date-helpers`: use the project's ISO helper (`crate::clock::now_iso`), never `iso_to_ms` on these; the dates are normalised away in the diff but a malformed injected `now` must still parse where code re-reads stamps.
9. **`isDefault`/`replace`/`appropriateness` are carried onto the copy** (`...original` → `wardrobeItemFromCreateBody`); `migratedFromClothingRecordId` carried via preserve; `imageFileId` only via the picture map. A lane that "cleans" the copy (resets `isDefault`) diverges — and no v4 test sees it. Corpus row `defaultSash`.
10. **Idempotent re-resolve:** `move+move` passes the already-resolved `target` into `moveItemsToFolder`, which resolves again; `createItem` resolves `fields.folder` again. With a NEW folder these re-resolutions happen against a tree where the first create already made the folder (create-before-resolve ordering can change the *casing* of a later resolve). Port the calls exactly as v4 sequences them; do not hoist a single resolution.
11. **Logs on the right thread.** The route INFO line is emitted on the CALLER thread after the writer answers (existing idiom, capture-pinned); the service's DEBUG lines are emitted INSIDE the writer closure (invisible to the process-global capture in the route-level test — memory `capture-rig-writer-thread`: `Db::write` closures are invisible to `captured_with`). Pin the service lines in the SYNC tier-2 test (call the service directly on the test thread), pin only the route lines in `folder_route_lines`.
12. **Frozen seams:** `engine.rs` and `types.rs` are shared; if frozen, `_with_…` sibling + HANDOFF (memory `frozen-engine-seams-and-web-censuses`); a NEW route action needs no `COLLAPSE_CENSUS` row unless a new `serde_json::from_str`-class body read is added (`web_edge_body_parse_guard.rs:107`).
13. **Oracle runner traps:** zsh `env $E` silently SKIPs; `cd ~/source/quilltap-server` header; `--testTimeout` in jest recipes (memory `oracle-runner-and-skip-traps`, `sweep-driver-recipe-header-rules`); the routes oracle sets `LOG_LEVEL=error` — a tsx log-capturing oracle must set `debug`. v4's `getRepositories()` singleton + the per-case copy trap (memory `v4-oracle-reinit-and-throwing-plants`, `jest-oracle-in-case-plants`): re-init between cases or run each chain in a fresh process; if plants are used (cycle, archived character), make the plant helper SELF-CHECK (`tier-r-vacuous-equal-rows`: a malformed plant fails both sides identically and passes).
14. **SPA signal ports** (memory `react-closure-reads-in-signal-ports`): `fileToFolder` reads `picker.mode`/`picker.items` BEFORE `closeFolderPicker()`/`clearSelection()`; the toast uses the captured `picker`, not a re-read signal. Memoise per-row bindings (NG0100). Component specs: "outfits live under the Outfits tab" (`spa-component-harness-gotchas`); `render between input+blur`; strict `toEqual` on dispatched requests catches a stray `components: undefined` key — `JSON`-style key absence, not `undefined` value (v4 uses a conditional spread).
15. **The adjacent `01f508d4e`** removes `console.debug('[useWardrobeItemActions] Duplicating wardrobe item', …)` in the very function neighbouring `openFolderPicker` (`wardrobe-item-actions.ts:228-234` in v5) — a separate ledger row; if the lane touches `wardrobe-item-actions.ts`, do NOT sweep that deletion in unannounced (it has its own order), but flag it.

---

## 7. Contract candidates

None beyond the slice's own wire:

- NEW `Request::WardrobeItemsCopy { scope, containerId?, ...flattened body }` + the TS `WardrobeItemsCopyRequest` (`type:'wardrobeItemsCopy'`) in `core-contract.ts` (append-only, beside `WardrobeItemsMoveRequest` `:4635` and the union at `:6607`), with `WardrobeItemsMoveRequest` gaining `components?: 'move' | 'copy' | 'none'` and the copy `components?: 'copy' | 'none'` as documentation (the Rust body rides raw; the TS types are hand-written, no codegen).
- The `{ items, componentsCarried }` response shape on both verbs (the SPA reads `items` only; `componentsCarried` is unused client-side by v4 too).
- Pin the new variant's decode in a wire-contract test (`p4_d279_wire_contract.rs` style) and its census row.
No shared-service signature changes for other lanes, EXCEPT the two mechanical ones the other wardrobe lanes may collide with: `folder_action(…, blob_webp)` (its five call sites) and the generalised `collect_container_components` (one call site). Announce both in the lane's HANDOFF.

---

## 8. Open questions for the order (decide in the work order, do not leave to the lane)

1. Dedicated new oracle file vs extending `item-mutations-tier2` for the service diff — this survey recommends a NEW `folder-filing-tier2` family (the item-mutations fixture has no pictures/nesting; the transfers machinery is the right normaliser source). Confirm.
2. Where the Rust `next_copy_title` lives (`wardrobe.rs`, with `expand_composites`/`collect_container_components`) — recommended.
3. Whether to hoist the transfers family's `normalize_all` into a shared harness support module (touches `wardrobe_transfers_tier2_equivalence.rs`; the transfers lane is closed) or duplicate it (cheap, drift risk). Recommend hoist only if `wardrobe_images_support` already exports it.
4. Angular naming: keep v4's `onSubmit`/`working` or the existing `move`/`moving`. Recommend v4's (the picker spec is rewritten anyway).
5. Whether the e2e outfit-asks beat is in scope (needs an outfit fixture).

---

## 9. What the ledger row (`drift-ledger.md:307`) got wrong or under-said

1. **Route-factory tests: THREE cases, not two.** The ledger lists "2 cases: `componentsCarried` 1; `Cloak (copy)` in `Formal`"; the commit also adds `copy-items refuses components: move at the schema (the middleware answers 400)` (`rejects.toThrow(/components/)`), which is the proof that the wire refusal is a Validation error.
2. **"`copy` + `move` → `FolderFilingRequestError` … a 400 via `writeErrorResponse`" is service-level only.** Over HTTP the Zod `copyWardrobeItemsSchema` (`components: z.enum(['copy','none'])`) refuses first with the flat `Validation error`; the sentence is unreachable on the wire. The ledger's phrasing would have a lane assert the sentence at the route.
3. **`COLLECTION_POST_ACTIONS` is in `crates/quilltap-web/src/wardrobe_routes.rs:124`, not `characters_routes.rs`** (which only forwards into `wardrobe_routes::collection_post`). The real hazard the row misses: `folder_write_request`'s `_ =>` arm defaults every unknown folder action to a MOVE (`:156`) — adding `"copy-items"` to the list alone yields copy-as-move.
4. **The two `availableActions` pins** (`wardrobe_instructions_routes.rs:241,292`) are not named in the row; they flip at the baseline move.
5. **`next_copy_title` has NO Rust twin.** The row says "the Duplicate `(copy)`-title helper" among v5 surfaces needing only wiring; Duplicate in v5 is SPA-only (`equipped-slots.ts:466`). A NEW Rust fn and its first tier-1 oracle family are required (§4.1).
6. **`move-items` needs the picture transcoder too** (`components:'copy'`), so `folder_action`'s signature changes for the MOVE arm as well; the row frames the copy arm as the only new thing. Also the engine's folder arms use `ready_db()` — they must switch to `ready_db_and_blob_webp()`.
7. **`wardrobe_folder_writes_tier2_equivalence` is not the carrier** (it covers the folder mutation writers). The row (and the brief) named it; the correct homes are `item_mutations_tier2`/a NEW filing family and `wardrobe_folders_routes_equivalence`.
8. **The SPA transfer dialog has not yet had its radio group extracted** (the row says P4.D286 "became" the shared fieldset path); v5's `wardrobe-transfer-dialog.ts` still carries the inline fieldset (`:154-175`) and `wardrobe.api.ts:96` the mode type — the extraction is part of this lane, not a rename.
9. **The row's `help/` claim "byte-equal to `4a38222e6` today"** is true but incomplete: `help/wardrobe.md` is byte-equal to `83334b9a5`, and at the pin it also carries `7458ff94e`'s and `f0db8cbb8`'s hunks — vendoring the pin's whole file in this lane would absorb two other ledger rows' text; apply bbbcd2388's hunks as a patch (§5.4) and leave the rest to the owning orders.
10. **`componentsCarried` counts components only** (not the requested items' own copies) — the row's "`componentsCarried`" is right but a lane could read it as "copies made"; with `copy`+`copy` of one outfit with 3 pieces it is 3, with 4 creates.
11. **Plain-path INFO line is NOT unchanged** — the row says "behaviourally unchanged except … the INFO line gains two fields", which is exactly right; the survey's addition is that the existing capture-pinned assertion (`wardrobe_route_factory.rs:1640`) therefore flips and must be updated in the same change.
12. The row says the stored-field search "no stored `WardrobeItem` field, no vault document field" — confirmed (§2.9); one nuance: `createItem` through `...original` carries `isDefault` to the copy (§6 trap 9), which the row does not mention and v4's tests never exercise.
