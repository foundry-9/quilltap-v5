# Survey — the wardrobe ROUTE FACTORY (all four tiers), the five folder actions, transfers, preview-avatar and `link-image` — `7a8d6dacc` → `9ecebf986` → `4a38222e6`

Read-only planning survey, 2026-10-12. v4 = `~/source/quilltap-server` read ONLY
at the PIN `4a38222e6` (`4.10.0-dev.148`, 2026-10-12, "Wardrobe folders — file
items into a tier's closet (#85)") via `git show 4a38222e6:<path>` and the span
diff `git diff 3e6e2f590 4a38222e6`; v5 = main at `f84d34150`.

Scope: the wire-visible ROUTE half of the wardrobe catch-up — v4's one
`lib/wardrobe/routes/wardrobe-route-factory.ts` behind all four tiers'
collection + item endpoints (incl. the `?action=instructions` arms and the
`?action=wear-history` arm that ride it), the five folder actions
(`4a38222e6`), the rewritten `wardrobe/transfers` route, the rewritten
`wardrobe/preview-avatar` route, and the PORT-NEW `POST
/wardrobe/[itemId]/images?action=link-image`. **Out of scope** (other
surveys / lanes): the keystone — `lib/wardrobe/{location,pool,wear-ops,
folders,folder-mutations,item-mutations,composite-types}.ts` and
`vault-overlay/wardrobe-writes.ts` (this survey records what the routes NEED
from them, §2, never how to port them); the parsers / recursive predicate /
projection sweep; the seven tools; `chats/[id]/actions/outfit.ts`; the
carriers (backup / `.qtap` / restore); the SPA (`WardrobeBrowser`, the folder
picker, bug 229); `help/` + `docs/v4/`; `7a8d6dacc`'s four bugs (none of its
30 files is a route file — see §0).

Every fact below is from the shipped hunks at the pin; line numbers are at
`4a38222e6` unless marked `OLD` (= `9ecebf986^`) or `v5`.

---

## 0. Which commit moved which of THIS lane's files

| file (v4) | `7a8d6dacc` | `9ecebf986` | `4a38222e6` |
|---|---|---|---|
| `lib/wardrobe/routes/wardrobe-route-factory.ts` | — | **NEW** (+381) | +164 (folders) → 529 lines |
| `lib/mount-index/mount-wardrobe-route-factory.ts` | — | **DELETED** (−461) | — |
| `lib/wardrobe/resolve-container.ts` | — | **DELETED** (−113) | — |
| `app/api/v1/characters/[id]/wardrobe/route.ts` | — | 173 → 21 lines (a registration) | — |
| `app/api/v1/characters/[id]/wardrobe/[itemId]/route.ts` | — | 155 → 19 | — |
| `app/api/v1/wardrobe/route.ts` (General) | — | 112 → 25 | — |
| `app/api/v1/wardrobe/[itemId]/route.ts` | — | 138 → 19 | — |
| `app/api/v1/projects/[id]/wardrobe/{route,[itemId]/route}.ts` | — | → 24 / 19 | — |
| `app/api/v1/groups/[id]/wardrobe/{route,[itemId]/route}.ts` | — | → 24 / 19 | — |
| `app/api/v1/wardrobe/transfers/route.ts` | — | −209/+… (locations) | +7 (`folder` carry) |
| `app/api/v1/wardrobe/preview-avatar/route.ts` | — | 227 lines rewritten | — |
| `app/api/v1/wardrobe/[itemId]/images/route.ts` | — | +25 (`link-image`) | — |
| `lib/wardrobe/item-images.ts` | — | +164/−… (`linkWardrobeItemImage`, home over location) | — |
| `lib/wardrobe/item-route-steps.ts` | — | `applyArchiveFlag` REMOVED (moved into `item-mutations.updateItem`) | — |
| `lib/wardrobe/wardrobe-instructions-handlers.ts` | — | — | — (unchanged since `b86bb1a5`) |
| `lib/schemas/wardrobe.types.ts` | — | +64 (`isComposite` etc.; shapes unchanged) | +60 (`folder`, the four folder schemas) |
| `docs/developer/API.md` | — | +23 (the factory contract) | +15 (the folders table) |

`7a8d6dacc` touches NONE of this lane's files (its `lib/` hunks are
`vault-overlay/parsers.ts`, `wardrobe-archive-handler.ts`,
`item-image-generation.ts`, NEW `wearable.ts`, `chats/[id]/actions/outfit.ts`
— the parser / tools / outfit / item-image lanes). This lane ports the
`9ecebf986` + `4a38222e6` route shape directly; `7a8d6dacc` is reached only
through the keystone functions the factory calls (§2).

---

## 1. v4 at the pin — `lib/wardrobe/routes/wardrobe-route-factory.ts` (529 lines)

### 1.1 The config and the eight registrations

```ts
export interface WardrobeRouteConfig<P extends Params> {   // :91-99
  scope: WardrobeScope;                       // 'character' | 'group' | 'project' | 'general'
  paramsToId: (params: P) => string | null;   // null for General
  logTag: string;                             // e.g. '[Projects v1]'
  logIdKey: string;                           // e.g. 'projectId'
}
```

| tier | collection file | item file | `scope` | `logTag` | `logIdKey` |
|---|---|---|---|---|---|
| character | `characters/[id]/wardrobe/route.ts:16-21` | `…/[itemId]/route.ts:14-19` | `character` | `[Wardrobe v1]` | `characterId` |
| General | `wardrobe/route.ts:15-25` (`paramsToId: () => null`; `noParams = { params: Promise.resolve({}) }`; `GET = (req) => handlers.GET(req, noParams)`) | `wardrobe/[itemId]/route.ts:14-19` | `general` | `[Wardrobe Archetypes v1]` | `generalId` |
| project | `projects/[id]/wardrobe/route.ts:18-23` | `…/[itemId]/route.ts:14-19` | `project` | `[Projects v1]` | `projectId` |
| group | `groups/[id]/wardrobe/route.ts:18-23` | `…/[itemId]/route.ts:14-19` | `group` | `[Groups v1]` | `groupId` |

Collection = `createWardrobeCollectionHandlers(config)` → `{ GET, POST }`
(:167-415); item = `createWardrobeItemHandlers(config)` → `{ GET, PUT, DELETE }`
(:421-529). Every handler is `createContextParamsHandler<P>(withActionDispatch<P>({…}, default))`.

### 1.2 The 404 labels (:102-107)

```ts
const LABELS = {
  character: { owner: 'Character',        item: 'Wardrobe item' },
  group:     { owner: 'Group',            item: 'Group wardrobe item' },
  project:   { owner: 'Project',          item: 'Project wardrobe item' },
  general:   { owner: 'Quilltap General', item: 'Archetype wardrobe item' },
};
```
`notFound(x)` renders `{ error: '<x> not found' }` 404 (`lib/api/responses.ts:150-155`).

### 1.3 Serialization — `origin` then `wear` on EVERY item body (:110-121)

`serializeWardrobeItems(items, origin, repos) = attachWear(withOrigin(items, origin), repos)`;
`serializeOne` is the one-item form. Applied to collection GET, `move-items`,
item GET, POST (201) and PUT. `withOrigin` = `{ ...item, origin }`
(`wardrobe-container.ts:81-86`); `attachWear` = `{ ...item, wear }` from ONE
`repos.wardrobeWear.findSummaries(ids)` call (`wear-history.ts:50-65`; empty
list → `[]` with no ledger read; DEBUG `Attached wear summaries to wardrobe read`
`{itemCount, wornCount, context:'wardrobe'}`). **Wire key order of one item** =
the parser's object order (`vault-overlay/parsers.ts:445-462`): `id,
characterId, title, description, imagePrompt, types, appropriateness,
isDefault, replace, componentItemIds, migratedFromClothingRecordId,
imageFileId, archivedAt, folder, createdAt, updatedAt`, then `origin`
(`{scope, id, name}`), then `wear` (`{wearCount, firstWornAt, lastWornAt,
lastWornChatId}`). The keystone's reader owns the first sixteen (NOTE `folder`
sits between `archivedAt` and `createdAt`); the factory owns the two tails.

### 1.4 The shared write-error map (:124-134) — `writeErrorResponse`

| thrown | status / body |
|---|---|
| `WardrobeComponentCycleError` (`wardrobe-writes.ts:173`) | **400** `error.message` |
| `WardrobeFolderPathError` (`folders.ts:31`) | **400** `error.message` |
| `WardrobeItemNotInLocationError` (`folders.ts:61`) | **400** `Not in this wardrobe (shared items are filed where they live): <id>, <id>` |
| `WardrobeFolderNotFoundError` (`folders.ts:39`) | **404** `Wardrobe folder not found` (via `notFound('Wardrobe folder')` — the error's own message is NOT used) |
| `WardrobeFolderConflictError` (`folders.ts:50`) | **409** `error.message` |
| `CharacterArchivedError` | **409** `An archived character's wardrobe cannot be changed` |
| anything else | rethrown → the context middleware's generic 500 `Internal server error` |

Used by: create, PUT, DELETE (both the `writableMountPointId()` pre-check and
the `delete`), `create-folder`, `rename-folder`, `delete-folder`, `move-items`.
NOT used by the instructions POST (its own 409 sentence, §1.8) nor by GET.

### 1.5 `makeResolver` → `resolveLocation(ctx, params, ensure)` (:136-161)

Calls `resolveWardrobeLocation(scope, paramsToId(params), repos, user.id, { ensure })`
(§2.1). `null` → `general` answers **500** `Quilltap General store is not
provisioned yet`; every other scope **404** `LABELS[scope].owner`. `ensure`
is `true` for: create, instructions POST, `create-folder`; `false` for:
`rename-folder`, `delete-folder`, `move-items`, item GET/PUT/DELETE (`findItem`
:429-441). The list GET and instructions GET call `resolveWardrobeLocation`
DIRECTLY with `ensure: scope !== 'character'` and their own null handling.

### 1.6 Collection GET (list) (:350-376)

- action map `{ instructions: handleGetInstructions, folders: handleGetFolders }`; default = list.
- `resolveWardrobeLocation(…, { ensure: scope !== 'character' })` — a project /
  group list PROVISIONS the store + `Wardrobe/` folder (unchanged from OLD);
  General ensures the folder when provisioned.
- `!location` → `general`: **200 `{ wardrobeItems: [] }`** (NEW — OLD General
  caught and answered 500 `Failed to fetch archetype wardrobe items`); other
  scopes 404 owner.
- `scope === 'character' && ?scope=group` → `listGroupTier` (:327-348):
  `resolveGroupMountsForCharacter(characterId)` → `originByMount` (later mount
  overwrites — v4 Map.set) → `repos.wardrobe.readSharedTiers(mountIds,
  readIncludeArchived(req), mp => origin)` → `attachWear` → **200
  `{ wardrobeItems }`** (NO `mountPointId`); DEBUG `<tag> Group-tier wardrobe
  read` `{characterId, groupCount, groupMountCount, itemCount, context}`.
- else `serializeWardrobeItems(await location.readItems(readIncludeArchived(req)), location.origin, repos)`
  → DEBUG `<tag> Listed wardrobe items` `{[idKey]: location.id, mountPointId,
  count, context:'wardrobe'}` → **200 `{ mountPointId: location.mountPointId, wardrobeItems }`**
  — `mountPointId` FIRST, on EVERY tier now (OLD: only project/group carried it;
  character and General answered `{ wardrobeItems }` alone). For General
  `location.id` is `null` → the log key `generalId: null`.
- `readIncludeArchived` (`query-params.ts:18-28`): `includeArchived` `=== 'true' || === ''`.

### 1.7 Collection POST (create) (:378-412)

- action map `{ instructions, 'create-folder', 'rename-folder', 'delete-folder', 'move-items' }`; default = create.
- **Parse BEFORE resolve** (`createWardrobeSchema.parse(await req.json())` — an
  invalid body never provisions; a ZodError → the middleware's flat 400
  `{ error: 'Validation error', details: issues }`).
- `resolveLocation(ctx, params, true)`; `lookup = componentItemIds.length > 0 ?
  await componentLookupFor(repos, location) : undefined`; `stored = await
  createItem(location, body, { lookup })` (§2.2) inside the `writeErrorResponse`
  try.
- INFO `<tag> Created wardrobe item` `{[idKey], userId, mountPointId, itemId,
  title, context}` (OLD titles: `[Wardrobe v1] Wardrobe item created`, `[Wardrobe
  Archetypes v1] Archetype item created`, `[Projects v1] Created project wardrobe
  item`, `[Groups v1] Created group wardrobe item`).
- **201 `{ wardrobeItem }`** serialized with `origin` + `wear` — on every tier.
  OLD project/group POST answered `{ mountPointId, wardrobeItem, wardrobeItems }`
  (the refreshed list); OLD character/General `{ wardrobeItem }` WITHOUT origin/wear.

### 1.8 `?action=instructions` riding the factory (:171-213)

- GET: `resolveWardrobeLocation(…, { ensure: scope !== 'character' })`;
  `!location && scope !== 'general'` → 404 owner; `mountPointId = location?.mountPointId ?? null`
  → `handleReadWardrobeInstructions(mountPointId, log)` (`wardrobe-instructions-handlers.ts:60-67`:
  null mount reads `null`) → **200 `{ instructions }`**; DEBUG `<tag> Read dressing
  instructions` `{[idKey]: location?.id ?? null, mountPointId, present, context}`
  (OLD per-tier titles: `Read character dressing instructions`, `Read General
  dressing instructions`, `Read project dressing instructions`, `Read group
  dressing instructions`). An archived character's GET still answers (not tombstoned).
- POST: `parseWardrobeInstructionsBody(req)` FIRST (`{ instructions: z.string().nullable() }`
  → `{ instructions, cleared }`; a missing / non-string non-null key is the flat
  400 `Validation error`); `resolveLocation(ctx, params, true)`; `!ok` → `body.cleared
  && scope === 'general'` → **200 `{ instructions: null }`** (the harmless
  no-op) else the resolver's response (General write → 500 `Quilltap General
  store is not provisioned yet`; others 404 owner); `location.writableMountPointId()`
  → `CharacterArchivedError` → **409 `Character is archived; dressing instructions
  cannot be edited`** (its OWN sentence, not §1.4's) → `handleWriteWardrobeInstructions`
  → **200 `{ instructions: trimmed | null }`**; INFO `<tag> Dressing instructions
  updated` `{[idKey], userId, mountPointId, cleared, context}` (OLD: `Character
  dressing instructions updated`, `General dressing instructions updated`, …).
  **GONE from OLD:** the character tier's `serverError('Character has no vault to
  hold dressing instructions')` (OLD char route :78) — a vault-less character's
  location is `null` → 404 `Character` now (even when clearing — the no-op is
  General-only).

### 1.9 Item GET / PUT / DELETE (:421-529)

- `findItem` (:429-441): `resolveLocation(ctx, params, false)` (no provisioning
  on ANY item route) → `location.findItem(itemId)` (archived included) → 404
  `LABELS[scope].item`. A character tier: `resolveWardrobeLocation` checks
  `character.userId === userId` and `characterDocumentMountPointId` (§2.1) — a
  foreign or vault-less character is 404 `Character`.
- GET (:456-464): action map `{ 'wear-history': handleGetWearHistory }`; default
  → **200 `{ wardrobeItem }`** with origin + wear. `wear-history` (:443-454):
  `findItem` first (404) → `buildWearHistoryPayload(itemId, repos)` → **200
  `{ history, wearers, lastWornChat }`**; DEBUG `<tag> Read wardrobe item wear
  history` `{[idKey], itemId, wearCount, context}`. (OLD titles per tier:
  `Read wardrobe item wear history` / `Read archetype item wear history` /
  `Read project wardrobe item wear history` / `Read group wardrobe item wear history`.)
- PUT (:466-497): `findItem` (404) → `const { archived, ...fields } =
  updateWardrobeSchema.parse(await req.json())` → `imageChoiceError(repos,
  current.id, fields.imageFileId)` → 400 `imageFileId must name one of this
  item's own pictures` (`item-route-steps.ts:31-46`, INFO `[WardrobeItem]
  Refused an imageFileId that is not the item's own` `{itemId, imageFileId}`).
  **Order: 404 → Zod → imageFileId** (the "PUT answers 404 for a missing item
  BEFORE it checks imageFileId" rule). Then `composite = (fields.componentItemIds
  ?? current.componentItemIds).length > 0; touchesComposition = fields.componentItemIds
  !== undefined || fields.types !== undefined; lookup = composite && touchesComposition
  ? await componentLookupFor(repos, location) : undefined; item = await updateItem(location,
  current, { ...fields, archived }, { lookup })` (§2.2) in the `writeErrorResponse`
  try; `!item` → 404 item (vanished mid-flight). INFO `<tag> Updated wardrobe
  item` `{[idKey], userId, mountPointId, itemId, context, …(archived !== undefined
  && { archivedAt: item.archivedAt ?? null })}` → **200 `{ wardrobeItem }`**
  with origin + wear (OLD: without them).
- DELETE (:499-526): `findItem` (404) → `meta = {[idKey]: location.id, itemId,
  context}` → **`location.writableMountPointId()` FIRST** (409 archived, before
  any chat is touched) → `cleanupEquippedRefs(repos, itemId, logTag, meta)`
  (`item-route-steps.ts:57-84`: `chats.removeEquippedItemFromAllChats` WARN
  `<tag> Cleanup of equipped references had issues, proceeding with delete`
  `{...meta, cleanupError}`; `wardrobeWear.deleteByItemIds([itemId])` DEBUG
  `<tag> Dropped wear-ledger rows for deleted item` `{...meta, itemId}` / WARN
  `<tag> Cleanup of wear-ledger rows had issues, proceeding with delete`
  `{...meta, ledgerError}`) → `location.delete(itemId)` (writeErrorResponse)
  → `!success` → 404 item → `cleanupItemImages(repos, itemId, logTag, {...meta,
  mountPointId})` → INFO `<tag> Deleted wardrobe item` `{...meta, mountPointId}`
  → **200 `{ success: true }`**. (OLD titles: `Wardrobe item deleted`,
  `Archetype item deleted` `{itemId}` only, `Deleted project wardrobe item`,
  `Deleted group wardrobe item`.)
- NO per-route try/catch any more: OLD character/General routes answered
  `serverError('Failed to fetch wardrobe item')` / `'Failed to create wardrobe
  item'` / `'Failed to delete wardrobe item'` / `'Failed to fetch archetype
  wardrobe item(s)'` / `'Failed to create archetype wardrobe item'` / `'Failed to
  delete archetype wardrobe item'` and ERROR-logged; at the pin an unexpected
  throw is the middleware's 500 `Internal server error` (as project/group always were).

### 1.10 The five folder actions (:219-324; `4a38222e6`)

| verb / action | body (Zod, `wardrobe.types.ts`) | resolve | success | refusals | log |
|---|---|---|---|---|---|
| `GET ?action=folders` | — | `resolveWardrobeLocation(scope, id, …, { ensure: false })` — **never provisions** | **200 `{ folders }`** — `location.listFolders()`: `[{ path, name, itemCount }]` sorted by path case-insensitively, `images/` excluded, `itemCount` = non-archived items DIRECTLY inside | `!location`: `general` → 200 `{ folders: [] }`; project/group whose OWNER exists (`repos.projects.findById` / `repos.groups.findById`) → DEBUG `<tag> No store yet; no wardrobe folders` `{[idKey]: id, context}` + 200 `{ folders: [] }`; else 404 owner (a character with no vault / foreign → 404 `Character`) | DEBUG `<tag> Listed wardrobe folders` `{[idKey], mountPointId, count, context}` |
| `POST ?action=create-folder` | `createWardrobeFolderSchema = { path: z.string().max(900).min(1, 'A folder path is required') }` (:193-195) — parsed BEFORE resolve | `ensure: true` (provisions like a create) | **200 `{ folder }`** (`{ path, name, itemCount }`; idempotent; an existing folder in any casing is answered as stored) | `WardrobeFolderPathError` 400 (`'A folder needs a name'` when the path resolves to the root — `folder-mutations.ts:30`; `'The wardrobe root already exists'` — `wardrobe-writes.ts:419`; the normaliser's `A wardrobe folder name may not be "."`, `"images" is reserved for item pictures and cannot be a wardrobe folder`, `Wardrobe folders nest at most 8 deep` — `folders.ts:99-114`); archived character 409 | INFO `<tag> Created wardrobe folder` `{[idKey], userId, mountPointId, folder: folder.path, context}` |
| `POST ?action=rename-folder` | `{ path: …min(1), name: z.string().min(1, 'A folder name is required').max(200) }` (:198-201) | `ensure: false` | **200 `{ folder }`** | `name` with `/` or `\` → 400 `A folder name may not contain "/"`; empty after normalising → 400 `A folder needs a name`; root → 400 `The wardrobe root cannot be renamed`; missing → **404 `Wardrobe folder not found`**; sibling holding the name (any casing) → **409 `A wardrobe folder named "<to>" already exists`**; archived 409 | INFO `<tag> Renamed wardrobe folder` `{[idKey], userId, mountPointId, from: body.path, to: folder.path, context}` |
| `POST ?action=delete-folder` | `{ path: …min(1) }` (:204-206) | `ensure: false` | **200 `{ movedItems, movedFolders }`** (the location's `parent` is logged, NOT answered) | root → 400 `The wardrobe root cannot be deleted`; missing → 404 `Wardrobe folder not found`; **409** `Folder "<from>" holds N file(s) that are not wardrobe items; move them in the Scriptorium first` / `Folder "<from>" holds a subfolder of the same name; rename one of them first` (both BEFORE anything moves) / `Folder "<from>" is not empty yet; its items were moved up` (`DatabaseStoreError` `NOT_EMPTY` after the move); archived 409 | INFO `<tag> Deleted wardrobe folder` `{[idKey], userId, mountPointId, folder: body.path, movedItems, movedFolders, context}` (`...result` spread) |
| `POST ?action=move-items` | `moveWardrobeItemsSchema = { itemIds: z.array(z.string().min(1)).min(1, 'At least one item is required').max(500), folder: z.string().max(900) }` (:209-212; `''` = root) | `ensure: false` | **200 `{ items }`** — the moved items SERIALIZED (origin + wear), in patch order | borrowed id → 400 `Not in this wardrobe (shared items are filed where they live): <ids>` (refused BEFORE anything moves — `updateMany({ requireAll: true })`); path rule 400; archived 409 | INFO `<tag> Moved wardrobe items to folder` `{[idKey], userId, mountPointId, folder: body.folder (RAW, as requested), count: moved.length, context}` |

`rename-folder` / `delete-folder` trim the request path and strip leading /
trailing slashes (`folder-mutations.ts:51,69`) — the folder named must EXIST
as stored (hand-made spellings included); only `name` is normalised
(`normalizeWardrobeFolderName`). `create-folder` and `move-items` resolve
through `location.resolveFolder` → `resolveWardrobeFolder(existingPaths,
requested)` (`folders.ts:169-191`: an existing folder in any casing answers as
stored; else the longest existing ancestor keeps its spelling and the remainder
is normalised; the whole must stay ≤ 8 deep). The client builds every action URL
with `wardrobeCollectionActionUrl(container, action)` =
`${baseCollectionUrl}?action=${encodeURIComponent(action)}`
(`wardrobe-container.ts:227-229`).

### 1.11 The `withActionDispatch` rules every arm inherits (`lib/api/middleware/actions.ts`)

- no `action` param → the default handler; a bare `?action=` or an unknown
  name → **400 `{ error: 'Unknown action: <a>', availableActions: [...] }`**
  (`availableActions` at the TOP level, in the map's literal order) with WARN
  `Unknown action requested` `{action, availableActions, method, path}`
  (:82-98). The dispatch runs BEFORE the handler loads anything, so an unknown
  action on a MISSING owner is the 400, never the 404 (the factory test
  `refuses an unknown action rather than serving the item`).
- Collection GET's list: `["instructions", "folders"]`; collection POST's:
  `["instructions", "create-folder", "rename-folder", "delete-folder", "move-items"]`;
  item GET's: `["wear-history"]`. (Item PUT / DELETE take no actions.)
- A route with actions and NO default (the images POST) answers **400
  `{ error: 'Action parameter required', availableActions }`** (:103-119).

### 1.12 The Zod shapes (`lib/schemas/wardrobe.types.ts`)

- `wardrobeItemFieldsSchema` (:155-172): `title: z.string().min(1, 'Title is
  required')`, `description`/`imagePrompt`/`appropriateness: z.string().nullable().optional()`,
  `types: z.array(WardrobeItemTypeEnum).min(1, 'At least one type is required')`,
  `isDefault: z.boolean().optional()`, `componentItemIds: z.array(z.string()).optional()`,
  `replace: z.boolean().optional()`, **`folder: z.string().max(900).optional()`**
  (`WardrobeFolderInputSchema`, NEW `4a38222e6`). `createWardrobeSchema =
  wardrobeItemFieldsSchema` (:175). `updateWardrobeSchema = …partial().extend({
  archived: z.boolean().optional(), imageFileId: UUIDSchema.nullable().optional() })` (:182-190).
- `WardrobeFolderPathSchema = z.string().max(900).refine(isValidWardrobeFolderPath,
  { message: 'Wardrobe folder must be a clean path of at most 8 segments, none
  named "images"' })` (:132-137) and `WardrobeItemSchema.folder:
  WardrobeFolderPathSchema.default('')` (:272) — the STORED shape (the keystone's
  reader; never a request body's).
- The four folder-action schemas: §1.10.
- `WardrobeScopeSchema = z.enum(['character', 'group', 'project', 'general'])`
  (`location.ts:56`) — the transfers' source AND destination now (§4).

### 1.13 Log-line census at the pin (tag per tier; keys)

All DEBUG unless marked; every one carries `context: 'wardrobe'` and the
tier's `[logIdKey]` (`generalId: null` on General):

`<tag> Read dressing instructions` {idKey, mountPointId, present} · INFO
`<tag> Dressing instructions updated` {idKey, userId, mountPointId, cleared} ·
`<tag> No store yet; no wardrobe folders` {idKey} · `<tag> Listed wardrobe
folders` {idKey, mountPointId, count} · INFO `<tag> Created wardrobe folder`
{idKey, userId, mountPointId, folder} · INFO `<tag> Renamed wardrobe folder`
{idKey, userId, mountPointId, from, to} · INFO `<tag> Deleted wardrobe folder`
{idKey, userId, mountPointId, folder, movedItems, movedFolders} · INFO `<tag>
Moved wardrobe items to folder` {idKey, userId, mountPointId, folder, count} ·
`<tag> Group-tier wardrobe read` {characterId, groupCount, groupMountCount,
itemCount} · `<tag> Listed wardrobe items` {idKey, mountPointId, count} · INFO
`<tag> Created wardrobe item` {idKey, userId, mountPointId, itemId, title} ·
`<tag> Read wardrobe item wear history` {idKey, itemId, wearCount} · INFO
`<tag> Updated wardrobe item` {idKey, userId, mountPointId, itemId, [archivedAt]}
· WARN `<tag> Cleanup of equipped references had issues, proceeding with
delete` · `<tag> Dropped wear-ledger rows for deleted item` · WARN `<tag>
Cleanup of wear-ledger rows had issues, proceeding with delete` · INFO `<tag>
Deleted wardrobe item` {idKey, itemId, mountPointId}. Beneath the factory (the
keystone's lines, same request): `[WardrobeLocation] Resolved wardrobe location`
{scope, id, ensure, found, mountPointId}, `[ItemMutations] Created / Updated /
Moved wardrobe items to folder`, `[FolderMutations] Created / Renamed /
Deleted wardrobe folder`, `Wardrobe item created / updated / deleted in folder`
(DEBUG — were INFO `… in vault`), `Wardrobe folders listed`, `Wardrobe folder
ensured / renamed / deleted; contents sent up`, `Attached wear summaries to
wardrobe read`, `[WardrobeItem] Refused an imageFileId that is not the item's own` (INFO).

---

## 2. What the factory NEEDS from the keystone (the functions it calls)

### 2.1 `lib/wardrobe/location.ts` (231 lines)

- `WardrobeScopeSchema` / `WardrobeScope` (:56-57).
- `interface WardrobeLocation` (:59-105): `scope, id: string|null, mountPointId:
  string, characterId: string|null, origin: WardrobeOrigin`; `readItems(includeArchived=false)`,
  `findItem(id)` (archived included), `create(item)`, `update(id, patch)`,
  `updateMany(patches, { requireAll? })`, `delete(id)`, `listFolders()`,
  `resolveFolder(requested)`, `createFolder(path)`, `renameFolder(path, newName)`,
  `deleteFolder(path) → { movedItems, movedFolders, parent }`, `writableMountPointId()`.
- `locationKey(loc) = \`${scope}:${mountPointId}\`` (:113-115) — the transfers' same-folder test.
- `locationForMount(mount, id, origin)` (:121-189): writes re-resolve a
  character's mount at write time (`resolveWardrobeMount(characterId)` throws
  `CharacterArchivedError` — the tombstone; `Character <id> has no linked vault`
  when gone); `updateMany` strips `id`/`createdAt`/`updatedAt` from every patch;
  `readItems` filters `!archivedAt` unless asked; folder ops delegate to
  `wardrobe-writes.ts` (`listFoldersInMount`, `listFolderPathsInMount`,
  `createFolderInMount`, `renameFolderInMount`, `deleteFolderInMount`,
  `updateManyInMount`, `createInMount`, `deleteInMount`, `readMountItems`).
- `resolveWardrobeLocation(scope, id, repos, userId, { ensure })` (:197-269):
  `general` → `getGeneralMountPointId()` null → `null`; `ensure` →
  `ensureSharedWardrobeFolder(mp)`; origin `GENERAL_WARDROBE_ORIGIN`
  (`{scope:'general', id:null, name:'Quilltap General'}`). `!id` → `null`.
  `character` → `repos.characters.findByIdRaw(id)`; `!character || character.userId
  !== userId` → `null`; `characterDocumentMountPointId` null → `null`; origin
  `{scope:'character', id, name: character.name}`. project/group →
  `repos.projects.findById` / `repos.groups.findById` → `null` if missing; `name =
  owner.name || 'Project'|'Group'`; `ensure` → `ensureOwnerOfficialStore(scope,
  owner.id, name)` (null → `null`) + `ensureSharedWardrobeFolder`; else
  `owner.officialMountPointId ?? null` (null → `null`); origin `{scope, id,
  name: owner.name}` (the RAW name, possibly `''`). Always DEBUG
  `[WardrobeLocation] Resolved wardrobe location` `{scope, id, ensure, found,
  mountPointId, context}`.
- `groupLocationsForCharacter(characterId)` (:276-288): one location per group
  mount in `resolveGroupMountsForCharacter` order (later wins).

### 2.2 `lib/wardrobe/item-mutations.ts` (234 lines)

- `componentLookupFor(repos, location, projectMountPointIds=[])` (:49-65): a
  character → `loadWearablePool(repos, characterId, projectMountPointIds).byId`
  (`pool.ts:254`); a shared tier → its own `readItems(true)` over
  `readGeneralWardrobe(true)` (the folder winning); General → itself.
- `createItem(location, body, { lookup?, preserve? })` (:106-137):
  `wardrobeItemFromCreateBody(body, location.characterId)` (`create-body.ts:41-58`
  — the create key order `characterId, title, description, imagePrompt, types,
  componentItemIds (deduped), appropriateness, isDefault, replace,
  migratedFromClothingRecordId: null, imageFileId: null, folder: body.folder ?? ''`;
  `prose()` turns blank → null); `fields.folder = fields.folder ? await
  location.resolveFolder(fields.folder) : ''`; composite → `fields.types =
  compositeTypes(lookup, componentItemIds, fields.types)` (`buildCompositeTypes`
  WIDEN-never-narrow; no lookup → the body's types); `id = preserve?.id ??
  randomUUID()`, `archivedAt/imageFileId/migratedFromClothingRecordId` from
  `preserve` else null, timestamps `preserve ?? now`; `location.create(item)`;
  DEBUG `[ItemMutations] Created wardrobe item` `{scope, mountPointId, itemId,
  composite, types, context}`.
- `updateItem(location, current, patch /* ItemPatch: fields + archived? */, { lookup })`
  (:149-182): `fields.folder !== undefined` → `isSameFolder(fields.folder,
  current.folder)` ? drop : `resolveFolder`; `archived !== undefined` →
  `archivedPatch(current.archivedAt, archived, now)` (idempotent, bug 188);
  composite + (components or types touched) → `next.types = compositeTypes(lookup,
  componentItemIds, fields.types ?? current.types)`; `location.update(current.id,
  next)`; DEBUG `[ItemMutations] Updated wardrobe item` `{scope, mountPointId,
  itemId, fields: Object.keys(next), found, context}`.
- `moveItemsToFolder(location, itemIds, folder)` (:214-234): `target = await
  location.resolveFolder(folder)`; dedupe ids; `location.updateMany(ids.map(id =>
  ({ id, patch: { folder: target } })), { requireAll: true })`; DEBUG
  `[ItemMutations] Moved wardrobe items to folder` `{scope, mountPointId, folder:
  target, requested, moved, context}`.
- re-exports `WardrobeItemNotInLocationError`.

### 2.3 `lib/wardrobe/folder-mutations.ts` (81 lines)

`createWardrobeFolder(location, path)` (:25-41: `resolveFolder` → `''` throws
`WardrobeFolderPathError('A folder needs a name')` → `location.createFolder`;
DEBUG `[FolderMutations] Created wardrobe folder` `{scope, mountPointId,
requested, folder, itemCount}`), `renameWardrobeFolder(location, path, name)`
(:44-62: `from = path.trim().replace(/^\/+|\/+$/g, '')`; `location.renameFolder(from,
normalizeWardrobeFolderName(name))`; DEBUG `… Renamed wardrobe folder` `{scope,
mountPointId, from, to, itemCount}`), `deleteWardrobeFolder(location, path)`
(:65-81: same trim; returns `{ movedItems, movedFolders }` (drops `parent`);
DEBUG `… Deleted wardrobe folder` `{scope, mountPointId, folder, parent,
movedItems, movedFolders}`).

### 2.4 `lib/wardrobe/folders.ts` (271 lines, client-safe) — the four error
classes (§1.4), `WardrobeFolder { path, name, itemCount }`,
`normalizeWardrobeFolderPath` (split on `/` and `\`, trim, drop empties,
`sanitizeFileName` each segment, refuse `.`/`..`/`images` (any casing), > 8
segments), `normalizeWardrobeFolderPathOrRoot` (the transfers' carry),
`normalizeWardrobeFolderName`, `isSafeWardrobeFolderPath`, `resolveWardrobeFolder`,
`isValidWardrobeFolderPath`, `folderOfWardrobePath`, `wardrobeFolderStorePath`,
`parentFolder`, `folderName`, `joinFolder`, `isSameFolder` (case-insensitive),
`isUnderFolder`, `rebaseFolder`, `folderCrumb`. `sanitizeFileName` MOVED to
`lib/mount-index/sanitize-file-name.ts` (v5: `mount_index` sanitize).

### 2.5 `lib/database/repositories/vault-overlay/wardrobe-writes.ts` — the folder ops' contract

`updateManyInMount(mount, patches, { requireAll })` (:260-321: throws
`WardrobeItemNotInLocationError(missing)` BEFORE writing; one `readMountItems`,
one `projectVaultWardrobe`; `folder: safeFolder(patch.folder)`; cycle check per
composite; DEBUG `Wardrobe items updated in folder` `{mountPointId, scope,
wardrobeItemIds, count, movesFolder}`); `listFoldersInMount` (:333-381: folder
rows ∪ item-implied folders − `images/`, sorted case-insensitively; DEBUG
`Wardrobe folders listed`); `listFolderPathsInMount` (:383); `createFolderInMount`
(:418-434: `''` → `WardrobeFolderPathError('The wardrobe root already exists')`;
DEBUG `Wardrobe folder ensured`); `renameFolderInMount` (:444-508: root →
`'The wardrobe root cannot be renamed'`; missing → `WardrobeFolderNotFoundError`;
collision → `WardrobeFolderConflictError(to)`; stamps UUID ids into id-less
files first — DEBUG `Stamping ids into wardrobe files before a folder rename`;
DEBUG `Wardrobe folder renamed`); `deleteFolderInMount` (:518-627: root →
`'The wardrobe root cannot be deleted'`; the two up-front 409 sentences
(:555-558, :573-576); `NOT_EMPTY` → the third (:606-607); DEBUG `Wardrobe folder
deleted; contents sent up`); `deleteInMount` (:631); `WardrobeComponentCycleError`
(:173, `COMPONENT_CYCLE_MESSAGE = 'component cycle'` :170 — the 400's text is
the error's message); `WardrobeMount { mountPointId, scope, characterId }` (:64);
`resolveWardrobeMount(characterId)` (:92 — throws `CharacterArchivedError`);
`readMountItems(mount)` (:122). `readSharedTiers(mountIds, includeArchived,
originOf)` is on `WardrobeRepository` (`wardrobe.repository.ts`) — the
`?scope=group` read.

### 2.6 Elsewhere

`attachWear` / `buildWearHistoryPayload` (`wear-history.ts`, unchanged in the
span — already ported, v5 `services/wardrobe_wear_history*`), `withOrigin` /
`GENERAL_WARDROBE_ORIGIN` / `wardrobeCollectionActionUrl` (`wardrobe-container.ts`),
`imageChoiceError` / `cleanupEquippedRefs` / `cleanupItemImages`
(`item-route-steps.ts` — `applyArchiveFlag` GONE, folded into `updateItem`),
`resolveGroupMountsForCharacter` (`tiered-mount-pool.ts`), `readIncludeArchived`.

---

## 3. The OLD shape (`9ecebf986^`) — what v5 main still mirrors

- `lib/mount-index/mount-wardrobe-route-factory.ts` (461 lines, DELETED): the
  project/group-only factory — `MountWardrobeRouteConfig { ownerLabel: 'Group'|'Project',
  logTag, logIdKey, findOwner, ensureOfficialStore, readWardrobe }` +
  `ensureWardrobeFolder`; `resolveOwnerStore` → 404 owner / 500 `Failed to ensure
  <owner> document store` (:134); list `successResponse({ mountPointId, wardrobeItems })`
  (:247); create `created({ mountPointId, wardrobeItem: stored, wardrobeItems })`
  (:296) INFO `<tag> Created <owner> wardrobe item`; item GET `{ wardrobeItem }`
  (origin + wear); PUT checked `imageChoiceError` BEFORE the item lookup (:392 →
  :400 — the 400-before-404 order the pin inverts); DELETE `{ success: true }`;
  cycle → 400 (`:157 badRequest(error.message)`); item routes `ensureOfficialStore`
  on every verb (provisioning on read).
- OLD character routes: `?scope=group` via `repos.wardrobe.findArchetypesInMountsAttributed`;
  list `{ wardrobeItems }` (no `mountPointId`); create `{ wardrobeItem }` without
  origin/wear; per-route try/catch → `Failed to …` 500s; instructions no-vault
  → 500 `Character has no vault to hold dressing instructions`; a thrown
  `CharacterArchivedError` from `repos.wardrobe.create/update/delete` fell into
  the catch (500), never a 409.
- OLD General routes: unprovisioned list → 500 `Failed to fetch archetype
  wardrobe items`; create threw past (no-mount / cycle → 500); `{ wardrobeItem }`
  without origin/wear.
- OLD `lib/wardrobe/resolve-container.ts` (DELETED): `readItems()` ALWAYS
  archived-included; project/group resolution ALWAYS ensured the store + folder
  (the transfers' probe-time provisioning, bug 192).

---

## 4. `app/api/v1/wardrobe/transfers/route.ts` at the pin (453 lines)

The span diff (`9ecebf986` then `4a38222e6`):

- Imports: `resolveWardrobeContainer` / `readGroupWardrobe` /
  `resolveGroupMountPointIdsForCharacter` / `createProjectWardrobeItem` /
  `deleteProjectWardrobeItem` / `wardrobeItemFromCreateBody` /
  `resolveContainerMountPointId` GONE; `normalizeWardrobeFolderPathOrRoot`,
  `createItem`, `WardrobeScopeSchema`, `groupLocationsForCharacter`,
  `locationKey`, `resolveWardrobeLocation` IN.
- Schema (:67-108): `source.scope` AND `destination.scope` are
  `WardrobeScopeSchema` = `['character','group','project','general']` — OLD
  source `['character','project','group','general']`, OLD destination
  `['general','project','group','character']`; so the Zod `Invalid option:
  expected one of "character"|"group"|"project"|"general"` sentence changes on
  BOTH (v5 `api/wardrobe.rs:687` and `:731,740` carry the OLD orders). The
  rest unchanged: `action: z.enum(['move','copy'])`, `itemId: min(1)`,
  `sourceCharacterId: min(1).optional()`, `sourceProjectId: nullable().optional()`,
  `components: z.enum(['move','copy','none']).optional()`, the two refines
  (`Either sourceCharacterId or source is required`; `Copying an outfit cannot
  move its components — the original outfit still needs them`); a ZodError →
  400 `issues.map(m).join('; ')` (the route's OWN catch :444-446).
- `ResolvedSource { location, item, containerItems }` (:56-65);
  `findInLocation(location, itemId)` = `location.readItems(true)` (:141-149).
- **`resolveSourceItem` probe order (bug 192) (:157-188): vault → GROUPS
  (`groupLocationsForCharacter(...).reverse()` — the LATER group mount first,
  "strongest first") → the named project (`resolveWardrobeLocation('project',
  sourceProjectId, …)` with NO `ensure` — a project without a store is SKIPPED,
  never provisioned) → General.** OLD: vault → project (PROVISIONING it) →
  groups (in forward order) → General. The character ownership gate is inside
  `resolveWardrobeLocation('character', …)` (foreign → null → 404 `Wardrobe item`).
- Explicit source (:241-246): `resolveWardrobeLocation(body.source.scope,
  body.source.id, repos, user.id)` (no ensure) then `findInLocation`.
- Destination (:257-266): `resolveWardrobeLocation(…, { ensure: true })` —
  provisioned ONLY here; `null` → 400 `Invalid destination`.
- Same-folder (:268-270): `locationKey(source.location) === locationKey(destination)`
  → 400 `Source and destination are the same` (keyed `scope:mountPointId` —
  OLD keyed `general` / `scope:characterId|mountPointId`; same answers).
- Plan (:296-314): every planned component and the item carry **`folder:
  normalizeWardrobeFolderPathOrRoot(x.folder)`** (`4a38222e6`) — a hand-made
  folder the rules cannot normalise lands at the ROOT rather than failing the
  transfer; `characterId: destination.characterId`; copies mint ids + `now`.
- Collision (:319-327): `destination.readItems(true)`; 400 `An item with the
  ID of "<title>" already exists at the destination`.
- Writable gates (:332-341): move or components-move → `source.location.writableMountPointId()`
  (archived source → the catch's **409 `An archived character's wardrobe cannot
  be changed`**); `destination.writableMountPointId()` (archived destination →
  same 409) — before any picture or item write.
- Pictures (`carryItemImages` / `commitMovedImages`, unchanged) :350-364.
- Writes (:369-372): `createAtDestination(destination, planned)` =
  **`createItem(destination, planned, { preserve: { id, createdAt, updatedAt,
  archivedAt, imageFileId, migratedFromClothingRecordId } })`** (:194-205) — the
  destination folder is created as needed by `location.resolveFolder` inside
  `createItem` (an existing destination folder of the same name in any casing is
  used as stored); no `lookup`, so a composite keeps its `types` as planned;
  `[ItemMutations] Created wardrobe item` fires per landed item. Moves
  `source.location.delete(id)` (:374-389; `Failed to remove a component from
  source after move` / `Failed to remove item from source after move` 500).
- Read-back (:402-419) unchanged (`destination.readItems(true)`; ERROR
  `[WardrobeTransfers v1] Transferred outfit did not read back with its planned
  component references`).
- INFO `[WardrobeTransfers v1] Wardrobe item transferred` (:421-435): keys
  `userId, action, componentMode, itemId, resultItemId, folder: stored.folder ?? ''`
  (**NEW**), `componentsTransferred, sourceScope, destinationScope,
  sourceCharacterId, destinationCharacterId, sourceMountPointId,
  destinationMountPointId`.
- Response (:437-442): `{ wardrobeItem: stored, action, componentsTransferred,
  ...(unresolvedComponentIds.length > 0 && { unresolvedComponentIds }) }` —
  `stored` is the RAW item (no origin / wear) and now carries `folder`.
- GET destinations (:207-235) unchanged: `{ destinations: { general: { available:
  true, label: 'Quilltap General' }, projects, groups, users } }` sorted by
  `localeCompare`; `Untitled project` / `Untitled group` / `Unnamed user`.

---

## 5. `app/api/v1/wardrobe/preview-avatar/route.ts` at the pin (240 lines)

Rewritten by `9ecebf986` onto the avatar job's pipeline. Response doc
`{ fileId, url, mimeType, prompt, rerouted, trail }`.

- Zod (:52-56) unchanged: `characterId: min(1, 'characterId is required')`,
  `equippedSlots: EquippedSlotsSchema`, `imageProfileId: min(1).optional()`;
  ZodError → 400 `issues.map(m).join(', ')` (comma-space).
- `repos.characters.findById(characterId)`; `!character || userId !== user.id`
  → 400 `Character not found` (unchanged).
- **Profile (:76-82): `resolveAvatarImageProfile(user.id, repos, { override:
  imageProfileId ?? null })` (`lib/image-gen/profile-resolution.ts:173-210`:
  override → `findById`, usable iff `userId === user && apiKeyId` (DEBUG
  `[ProfileResolution] Avatar override profile unusable; falling back`
  `{context, override, found}`) → (no chat here) → `findDefault(userId)` with
  `apiKeyId` → else DEBUG `[ProfileResolution] No usable avatar image profile`
  → null); `!imageProfile?.apiKeyId` → 400 `No image profile is configured.
  Set one in Settings → Images before generating avatars.`** OLD: override
  `findById` else `findAll().find(isDefault) ?? all[0]` → 400 `No image profile
  available for avatar generation` / 400 `Selected image profile has no API key
  configured` (v5 `api/wardrobe.rs:1151,1162` carry the OLD two).
- `repos.connections.findApiKeyByIdAndUserId(apiKeyId, user.id)`; `!key_value`
  → 400 `API key for image profile is missing or invalid` (unchanged).
- `resolveAesthetic({ kind: 'aurora' })`; `buildCharacterAvatarPrompt(repos,
  character, { equippedSlots, characterAesthetic })` — MOVED to
  `lib/image-gen/avatar-prompt.ts`, text byte-identical (ledger §3(d)); `!hasAppearance`
  → 400 `No appearance data available — add a physical description or equip
  wardrobe items first` (unchanged).
- DEBUG `[Avatar Preview] Generating preview` `{context:
  'api.v1.wardrobe.preview-avatar', characterId, profileId, profileOverride,
  leafCounts}` (NEW).
- `conciergePolicy = resolveConciergeSettings(await repos.chatSettings.findByUserId(user.id)
  ?? null, null)` (no chat → the GLOBAL policy; nothing Locked, nothing announced).
- `attempt = makeLoggedImageAttempt({ userId, logType: 'IMAGE_GENERATION',
  prompt, primaryProfileId: imageProfile.id, characterId, params: { overrides: { n: 1,
  style: 'natural' }, orientation: 'portrait', logContext: { context, characterId } } })`
  (`image-attempt.ts:75`) — the SAME params builder as before (LoRAs / residual
  options / the provider's own portrait shape), now with the llm-log row written
  by the attempt (OLD: `createImageProvider(...).generateImage(previewParams, key)`
  directly, NO llm-log row).
- **`generateImageWithConciergeFailover({ profile, apiKey }, attempt, { userId,
  chatId: null, purpose: 'avatar', conciergePolicy })`** (:133-139); on throw:
  `trail = getConciergeTrail(error)`; `refused = trail?.some(outcome === 'refused')`;
  **WARN `[Avatar Preview] Image generation failed` `{context, characterId,
  error, refused, conciergeTrail: trail?.map(({profileName, outcome}))}`**;
  **`errorResponse(refused ? 'The image provider declined to draw this portrait'
  : \`Image generation failed: ${message}\`, refused ? 422 : 502, { trail, refused })`**
  → body `{ error, details: { trail, refused } }`. OLD: a provider throw
  propagated to the middleware's 500.
- `decodeProviderImage(failover.result, \`avatar_preview_${name.replace(/[^a-zA-Z0-9]/g,'_')}\`)`
  (`image-attempt.ts:162`: base64 → WebP transcode, sha256, width/height,
  filename, revisedPrompt); null → 500 `Image provider returned no image data`
  (unchanged sentence). OLD filename carried `_${Date.now()}.${ext}`; the
  decoder owns the minted name now (v5's `AvatarPreviewRenderer` already models
  the post-transcode seam — `pv_ok_bytes`).
- Vault write unchanged (`getCharacterVaultStore` null → throw → the catch's
  500 `Failed to save avatar preview`; `writeCharacterAvatarToVault({ characterId,
  kind: 'history', filename, content, contentType })` — **the `description:
  \`${name} — outfit preview\`` label is GONE** (bug 132: no `description` on the
  files row either).
- **`createGeneratedFileRow(repos, { userId, sha256, originalFilename, stored:
  written, width: decoded.width, height: decoded.height, linkedTo: [characterId],
  tags: [characterId], generation: { prompt, model: failover.profile.modelName,
  revisedPrompt } })`** (`lib/files/generated-file-row.ts:86`) — OLD wrote
  `repos.files.create({...}, { id })` by hand with `width: 1024, height: 1792`
  (hard-coded), `description`, `generationModel: imageProfile.modelName`. The
  model is the ANSWERING profile's now (a reroute's understudy).
- INFO `[Avatar Preview] Preview saved` `{context, characterId, fileId,
  profileId: failover.profile.id, rerouted}` (OLD `{context: 'wardrobe.preview-avatar',
  characterId, fileId}`).
- **200 `{ fileId, url: '/api/v1/files/<id>?action=download', mimeType:
  written.storedMimeType, prompt, rerouted, trail: failover.trail.length > 0 ?
  trail : null }`** — two NEW keys.
- ERROR `[Avatar Preview] Failed to save preview image` `{context, characterId}` + 500 `Failed to save avatar preview`.
- `POST = createContextHandler((req, ctx) => trackActivity('image', …))` unchanged.

---

## 6. `POST /api/v1/wardrobe/[itemId]/images?action=link-image` (PORT-NEW, `9ecebf986`)

- Route arm (`images/route.ts:259-279`): `findHome(req, ctx, itemId)` (the query
  gate 400 `id is required for this scope` / scope enum, then
  `resolveWardrobeItemHome` → 404 `Wardrobe item` — unchanged) → `meta = { itemId,
  scope }` → `fileIdBodySchema.parse(await req.json())` (`{ fileId: z.string().min(1,
  'fileId is required') }`) → `linkWardrobeItemImage(repos, home, { userId,
  sourceFileId: fileId })` → INFO **`[Wardrobe Images v1] Linked an existing
  picture to a wardrobe item`** `{itemId, scope, sourceFileId, fileId: stored.id}`
  → **201 `{ image: toWardrobeImageSummary(stored), current: stored.id }`**;
  `UnlinkableWardrobeImageError` → **400 `That picture cannot be linked`**; else
  `mapWriteError` (archived → 409 `This character is archived; their wardrobe
  cannot be changed` with INFO `Refused a picture write on an archived character`;
  `ForeignWardrobeImageError` → 400; ZodError → 400 joined `; `).
- Registered in the POST map AFTER `upload` and BEFORE `set-current`
  (`:371-377`): `generate, upload, link-image, set-current, delete-image,
  save-to-store` — the `availableActions` order of the 400s moves.
- `linkWardrobeItemImage` (`item-images.ts:264-302`): `source = repos.files.findById(sourceFileId)`;
  `bytes = source?.category === 'IMAGE' && source.storageKey &&
  parseMountBlobStorageKey(source.storageKey) ? await readMountBlob(storageKey) : null`;
  `!source || !bytes` → INFO **`[WardrobeImages] Refused to link a picture with
  no readable blob`** `{context, itemId, sourceFileId, found}` + throw
  `UnlinkableWardrobeImageError(fileId)` (message `File <id> is not a stored
  picture that can be linked`, `:248-253`); DEBUG `[WardrobeImages] Linking an
  existing picture to a wardrobe item` `{context, itemId, sourceFileId, bytes}`;
  then `addWardrobeItemImage(repos, home, { userId, kind: source.source ===
  'GENERATED' ? 'generated' : 'UPLOADED' ? 'uploaded' : 'imported', content: bytes,
  contentType: source.mimeType, width, height, generationPrompt, generationModel,
  generationRevisedPrompt })` — the sha256 de-dup makes the new link share the
  blob; the new `files` row is the item's own (linked + current).
- `WardrobeItemHome` (`item-images.ts:57-74`) now carries `location: WardrobeLocation`
  and `resolveMount = () => location.writableMountPointId()`, `update = patch =>
  location.update(itemId, patch)`; `resolveWardrobeItemHome` reads `location.readItems(true)`
  (no provisioning).
- v4 tests: `__tests__/unit/lib/wardrobe/item-images.link.test.ts` (3 cases — §8).
  The images ROUTE test has NO `link-image` case.

---

## 7. `docs/developer/API.md` cross-check (NOT a source — two stale spots)

- `:1992-2000` + the folders table `:2002-2014` agree with the hunks (the
  `GET ?action=folders` row says "sorted by path" — `listFoldersInMount` sorts
  case-insensitively).
- **Stale:** `:2078` says preview-avatar answers `{ fileId, url, mimeType, prompt }`
  — the route answers `rerouted` + `trail` too (`:215-222`). **Stale:** `:2084`
  says the probe order is "own vault, the source project's store, the group
  stores …, then Quilltap General" — the route probes GROUPS before the project
  (bug 192, `:157-188`). The hunks win; note both when the docs re-vendor lane
  copies `API.md`.

---

## 8. v4 jest coverage at the pin (names + `it(` counts)

- `__tests__/unit/lib/wardrobe/routes/wardrobe-route-factory.test.ts` — **77**:
  `collection GET — listing` (11), `character collection GET ?scope=group` (1),
  `collection POST — create` (10), `?action=instructions does not fall through
  to item CRUD` (6), `item GET` (5), `item GET ?action=wear-history` (6), `item
  PUT` (10), `item DELETE` (7), `the tier label follows the scope` (3), `item
  pictures` (4), `folder actions` (13 — `GET ?action=folders` ×4 incl. "never
  provisions" + "unprovisioned General has no folders", `move-items into an
  existing hand-made folder files it as stored, unrespelled`, `create-folder
  normalises … / refuses the reserved images segment with a 400`, `rename-folder
  passes one normalised segment and maps a collision to 409`, `delete-folder
  answers the counts the writer moved, and 404s a missing folder`, `move-items
  files every item in one batch and answers them serialised`, `move-items
  refuses a borrowed id, naming it, before anything moves`, `item PUT carries a
  normalised folder`, `collection POST files a new item in the requested folder,
  root by default`).
- `__tests__/unit/app/api/v1/groups/[id]/wardrobe/route.test.ts` — 14 (the
  registration through the factory; `4a38222e6` +29/−… lines).
- `__tests__/unit/app/api/v1/characters/[id]/wardrobe/[itemId]/route.test.ts` — 4;
  `…/wardrobe/instructions.test.ts` — 7; `__tests__/unit/app/api/v1/wardrobe/wear-ledger-routes.test.ts` — 6.
- `__tests__/unit/app/api/v1/wardrobe/transfers/route.test.ts` — **30**, incl.
  `source probing (bug 192)` (8: `the vault wins over every shared tier`, `a
  group copy wins over General`, `a group copy wins over the project — groups are
  probed before the project`, `the later group shadows the earlier one, so it is
  probed first`, `the project wins over General`, `falls through to General…`,
  `probing never provisions: a project with no store is skipped, not created`,
  `an explicit source is resolved without provisioning either`), the two
  `4a38222e6` folder cases (`keeps each item's folder path at the destination —
  outfit and components alike`, `lands an item whose hand-made folder cannot be a
  wardrobe folder at the root`), `pictures travel with the item` (4).
- `__tests__/unit/app/api/v1/wardrobe/[itemId]/images/route.test.ts` — 28 (no link case);
  `__tests__/unit/lib/wardrobe/item-images.link.test.ts` — 3.
- Keystone tests the factory leans on: `location.test.ts` 13, `item-mutations.test.ts`
  26, `folders.test.ts` 19, `wardrobe-folder-writes.test.ts` 13, `wardrobe-folder-read.test.ts` 4,
  `wardrobe-instructions-handlers.test.ts` 16. The DELETED
  `mount-wardrobe-route-factory.test.ts` (732 lines) and `wardrobe.repository.pool.test.ts` are gone.
- No preview-avatar unit test under `__tests__/unit/app/api/v1/wardrobe/` at the
  pin (the oracle `wardrobe-routes.test.ts`'s seven `pv_*` cases are v5's only pin).

---

## 9. v5 today (main `f84d34150`)

### 9.1 `crates/quilltap-core/src/api/types.rs` — the wardrobe `Request` variants

Character tier (dispatch-only except the list GET): `CharacterWardrobeList
{ character_id, scope: Option<String>, include_archived }` :675;
`CharacterWardrobeInstructionsGet` :692; `CharacterWardrobeInstructionsSet
{ character_id, instructions: Option<Option<Value>> }` :700;
`CharacterWardrobeCreate { character_id, item: Value }` :706;
`CharacterWardrobeGet` :711; `CharacterWardrobeUpdate { …, item }` :716;
`CharacterWardrobeDelete` :722. Group: `GroupWardrobeList` :951,
`…InstructionsGet` :958, `…InstructionsSet` :963, `…Create` :970, `…Get` :976,
`…Update` :982, `…Delete` :989. Project: `ProjectWardrobeList` :1571,
`…InstructionsGet` :1578, `…InstructionsSet` :1583, `…Create` :1590, `…Get`
:1596, `…Update` :1602, `…Delete` :1609. General + the rest (the P4.9f1 block):
`WardrobeTransferDestinations` :2733, `WardrobeTransferApply { #[serde(flatten)]
body }` :2737, `WardrobeList { include_archived }` :2743, `WardrobeInstructionsGet`
:2751, `WardrobeInstructionsSet` :2755, `WardrobeCreate { item }` :2760,
`WardrobeItemGet { item_id }` :2767, `WardrobeUpdate { item_id, item }` :2772,
`WardrobeDelete` :2779, `WardrobePreviewAvatar { flatten body }` :2785,
`WardrobeAnalyzeImage` :2791 (REFUSAL-ARMED). P4.D255's frozen block:
`WardrobeItemWearHistory { scope: WardrobeContainerScope, container_id, item_id }`
:2802, `WardrobeItemImagesList` :2811, `WardrobeItemImageGenerate` :2820,
`WardrobeItemImageSetCurrent` :2830, `WardrobeItemImageDelete` :2840,
`WardrobeItemImageSaveTargets` :2883, `WardrobeItemImageSaveToStore` :2895
(`upload` is a web-only binary leg, no verb). Response variants:
`Response::Wardrobe(Value)` :4609, `WardrobeWearHistory(Value)` :4611,
`WardrobeItemImages(Value)` :4613, `ChatOutfit(Value)` :4617; the per-tier
routes answer `Response::Character` / `Project` / `Group`.

**No variants exist for** the five folder actions (any tier) or `link-image`.
**Needed (names are proposals, the four tiers × five):**
`{Character,Project,Group,}WardrobeFolders { <owner_id> }`,
`…WardrobeFolderCreate { <owner_id>, path }`, `…WardrobeFolderRename { …, path, name }`,
`…WardrobeFolderDelete { …, path }`, `…WardrobeMoveItems { …, item_ids, folder }`
(or ONE scoped family `WardrobeFolders { scope: WardrobeContainerScope,
container_id, … }` on the P4.D255 pattern — a HUMAN CHOICE; the SPA's
`core-contract.ts` mirrors whichever); `WardrobeItemImageLink { scope,
container_id, item_id, file_id }`. Body keys that must stay Zod-shaped
(`path` / `name` / `folder` strings, `itemIds` array) should ride RAW
(`Option<Value>` / flattened) per the P4.D57 idiom so the handler answers v4's
flat `Validation error` — see §9.6's censuses.

### 9.2 `api/engine.rs` dispatch arms

Character :2075 (list), :2089, :2095 (instructions), :2460 (create), :2472
(get), :2484 (update), :2501 (delete); group :2833-2872; project :3703-3751;
General/transfers/preview :5455 (destinations), :5459 (`WardrobeTransferApply`
→ `ready_db_and_blob_webp`), :5479 (list), :5483/:5487 (instructions), :5491
(create), :5496 (`WardrobeItemWearHistory`), :5509-5634 (the images family),
:5671 (item get), :5675 (update), :5679 (delete), :5683 (`WardrobePreviewAvatar`
→ `ready_avatar_preview()` :6595), :5700 (`WardrobeAnalyzeImage` → the refusal).

### 9.3 `api/wardrobe.rs` (1541 lines) — General + transfers + preview + instructions

- `parse_archetype_body` :134; `wardrobe_list` :246 (→ `{ wardrobeItems }`
  :261; a read error → 500 `Failed to fetch archetype wardrobe items` :263 — the
  OLD catch); `wardrobe_create` :275 (:312-313: no-mount / **cycle → 500
  `Internal server error`** — v4 pin: cycle 400, no-mount 500 `Quilltap General
  store is not provisioned yet`; answers `{ wardrobeItem }` WITHOUT origin /
  wear :314); `wardrobe_item_get` :327 (`{ wardrobeItem }` :343 — with origin?
  it reads through `find_archetype_by_id`; 404 `Archetype wardrobe item` :345;
  500 `Failed to fetch archetype wardrobe item` :347); `wardrobe_item_wear_history_general`
  :357 (DEBUG `[Wardrobe Archetypes v1] Read archetype item wear history` :375);
  `wardrobe_update` :395 (404s :431/:478; `{ wardrobeItem }` :486 — no origin /
  wear); `wardrobe_delete` :506 (`LOG_TAG` :507; INFO `[Wardrobe Archetypes v1]
  Archetype item deleted` :564; `{ success: true }` :565); `wardrobe_transfer_destinations`
  :578; `parse_transfer_request` :605 (the OLD enum orders :668-669 / :687 /
  :731,740 — §4); `wardrobe_transfer_apply[_with_codec]` :820/:833 (`{ wardrobeItem,
  action, componentsTransferred, [unresolvedComponentIds] }` :864-874; `TransferError::NotFound`
  → 404 `Wardrobe item` :876; the archived conflict :878); the preview seam
  `AvatarPreviewRenderRequest` :901 / `AvatarPreviewImage` :916 / `AvatarPreviewError`
  :928 / `ErasedAvatarPreview` :954 / `parse_preview_body` :1002 (Zod-faithful
  sentences :1008-1051) / `wardrobe_preview_avatar` :1079 / `run_wardrobe_preview_avatar`
  :1098 (**OLD** profile pick + sentences: `Character not found` :1121/:1125,
  `No image profile available for avatar generation` :1151, `Selected image
  profile has no API key configured` :1162, `API key for image profile is missing
  or invalid` :1170, `No appearance data available …` :1206, `Image provider
  returned no image data` :1265, a renderer failure → 500 `Internal server error`
  :1268 (v4 pin: 422 / 502 with `details`), `description = "<name> — outfit
  preview"` :1277 (GONE at the pin), response `{ fileId, url, mimeType, prompt }`
  :1337-1343 (pin adds `rerouted`, `trail`), `Failed to save avatar preview` :1343);
  `wardrobe_analyze_image` :1355 (the refusal — OUT OF SCOPE, stays);
  `wardrobe_instructions_get` :1407 (`[Wardrobe Archetypes v1] Read General
  dressing instructions` :1423) / `_set` :1434 (clear no-op :1447; 500 `Quilltap
  General store is not provisioned yet` :1450; INFO `General dressing instructions
  updated` :1462).

### 9.4 The per-tier routes

- `api/characters.rs`: `character_wardrobe_list` :853 (`?scope=group` through
  `find_archetypes_in_mounts_attributed` :879 with DEBUG `[Wardrobe v1]
  Group-tier wardrobe read` :895; own vault `wardrobe_read::find_by_character_id`
  :900; answers `{ wardrobeItems }` :910 — NO `mountPointId`); `wardrobe_err`
  :2696 (`NoMount` → 500 `Character has no wardrobe vault to store the item`;
  **`Cycle(m)` → 500**; `Db(e)` → 500 — so a `DbError::CharacterArchived` from
  the vault layer (`db/vault_wardrobe_public.rs:164`) is a **500**, v4 pin 409);
  `character_wardrobe_create` :2709 (`_user_id` UNUSED — no ownership gate at the
  route; `{ wardrobeItem }` :2789 without origin / wear); `_get` :2794
  (`owned_wardrobe_item` :2844 filters `characterId === id` — v4's OLD guard;
  `{ wardrobeItem }` :2830); `_wear_history` :2854; `_update` :2894 (404 :2933
  BEFORE `image_choice_error` :2944 — already v4's pin order; `archived_patch`
  :2991-3000; `{ wardrobeItem }` :3024); `_delete` :3036 (`LOG_TAG` `[Wardrobe
  v1]` :3042; INFO `Wardrobe item deleted` :3108); instructions :3488 / :3525
  (409 `Character is archived; dressing instructions cannot be edited` :3551;
  the clear no-op :3558 — v4 pin: General-only).
- `api/projects.rs`: `wardrobe_err` :1443 (**cycle → 400** already);
  `ensure_project_wardrobe_mount` :1460 (ensures store + folder; 500 `Failed to
  ensure project document store` :1493); `project_wardrobe_list` :1504
  (`{ mountPointId, wardrobeItems }` :1524); `_create` :1534 (INFO `[Projects v1]
  Created project wardrobe item` :1628; **201 `{ mountPointId, wardrobeItem,
  wardrobeItems }`** :1631-1633 — the OLD body); `_get` :1642 (**`ensure_project_wardrobe_mount`
  on a READ** :1646 — v4 pin's item routes never provision; 404 `Project
  wardrobe item` / `Project` :1666-1667); `_wear_history` :1677 (ensure :1681);
  `_update` :1712 (ensure :1723; **`image_choice_error` :1736 BEFORE the 404
  :1763** — the OLD order the pin inverts; INFO `Updated project wardrobe item`
  :1807; `{ wardrobeItem }` :1809); `_delete` :1825 (ensure :1832; INFO `Deleted
  project wardrobe item` :1887); `build_wardrobe_patch` :1898; instructions
  :2377 / :2404 (ensure :2426).
- `api/groups.rs`: `parse_wardrobe_fields` :1069; `group_wardrobe_write_err`
  :1163 (cycle → 400); `ensure_group_wardrobe_mount` :1174 (500 `Failed to
  ensure group document store` :1205) / `resolve_group_wardrobe_mount` :1216
  (no ensure — item routes :1342/:1377/:1417/:1516 already read-only); `_list`
  :1237 (`{ mountPointId, wardrobeItems }` :1258); `_create` :1270 (**201
  `{ mountPointId, wardrobeItem, wardrobeItems }`** :1328-1330); `_get` :1338;
  `_wear_history` :1373 (`[Groups v1] Read group wardrobe item wear history` :1394);
  `_update` :1408 (**`image_choice_error` :1440 BEFORE the 404 :1465**);
  `_delete` :1508 (INFO `Deleted group wardrobe item` :1571); instructions :1588 / :1618.
- v5 log titles today (`[Wardrobe v1] Wardrobe item deleted`, `[Wardrobe
  Archetypes v1] Archetype item deleted`, `[Projects v1] Created / Updated /
  Deleted project wardrobe item`, `[Groups v1] Deleted group wardrobe item`, the
  per-tier `Read <owner> dressing instructions` / `<Owner> dressing instructions
  updated`) are the OLD per-tier spellings; the pin's one factory spells `<tag>
  Listed / Created / Updated / Deleted wardrobe item`, `Read dressing
  instructions`, `Dressing instructions updated`, `Read wardrobe item wear history`
  (§1.13). (Character create / update have NO INFO line in v5 today.)

### 9.5 Services

- `services/wardrobe_item_route_steps.rs` (371): `ItemRouteMeta` :35,
  `run_cleanup_equipped_refs` :90 / `log_cleanup_equipped_refs` :108 /
  `cleanup_equipped_refs` :142, `image_choice_error` :160 (INFO `[WardrobeItem]
  Refused an imageFileId that is not the item's own` :171; the 400 sentence :340),
  `parse_image_file_id` :185. (v4's `applyArchiveFlag` moved into
  `item-mutations.updateItem` — v5 inlines `archived_patch` per tier.)
- `services/wardrobe_container.rs` (431): `ResolvedWardrobeContainer` :60 /
  `resolve_wardrobe_container` :75 (= the DELETED `resolve-container.ts` — the
  pre-location shape), `resolve_container_mount_point_id` :195 (500 `Quilltap
  General is not provisioned` :213), `WardrobeItemHome { scope, character_id,
  container_mount_point_id, item: Value, container_items }` :230 with
  `resolve_mount` :265 (the tombstone gate → `DbError::CharacterArchived`) and
  `update` :278, `resolve_wardrobe_item_home` :317. **This module is the v5
  stand-in for v4's `WardrobeLocation` — the keystone lane decides whether it
  becomes `services/wardrobe_location.rs` (`resolve_wardrobe_location(scope, id,
  user_id, ensure)` + the folder ops) or is replaced; this lane consumes it.**
- `services/wardrobe_transfers.rs` (1102): `ARCHIVED_CONFLICT` :72,
  `TransferRequest` :122, `enumerate_destinations` :189, `location_key(scope,
  id)` :309 (OLD keying), **`resolve_source_item` :328 — TRY 1 vault → TRY 2 the
  named project THROUGH `ensure_official_store::<ProjectEntity>` +
  `ensure_project_wardrobe_folder` (:358-363 — PROVISIONS on a probe) → TRY 3
  groups in FORWARD `resolve_group_mount_point_ids_for_character` order (:400-405)
  → TRY 4 General** (bug 192's pre-fix shape on both counts); `resolve_explicit_source`
  :443 (ALSO provisions — :511-515 project, :546-550 group); `resolve_destination`
  :613 (provisions — correct); `create_at_destination` :717 / `delete_from_source`
  :742 (the scope switch v4 replaced with `createItem(preserve)` /
  `location.delete`); `transfer_wardrobe_item` :773 (sentences :808, :824, :903,
  :997, :1005 match v4); no `folder` anywhere.
- `services/wardrobe_item_images/`: `service.rs` `AddWardrobeItemImageInput`
  :142 / `add_wardrobe_item_image(main, mount, home, input, mint, blob_webp)`
  :193 / `ItemImageError { Archived, Foreign, Failed }` :38 — `link` lands beside
  it as `link_wardrobe_item_image(main, mount, home, user_id, source_file_id,
  mint, blob_webp)` reading the source through `db::files` (`FileEntry.category
  == 'IMAGE'`, `storage_key`) + `services::file_storage::parse_mount_blob_storage_key`
  :239 + `db::doc_mount_blobs::read_data_by_file_id` :272 (or the blob-id form),
  with a NEW `ItemImageError::Unlinkable { file_id }` → the route's 400;
  `carry.rs` (`carry_item_images` :114, `commit_moved_images` :230) unchanged.
- `services/image_profile_resolution.rs`: `resolve_image_profile_for_chat` :19,
  `resolve_wardrobe_image_profile` :129 — **NO `resolve_avatar_image_profile`**
  (v4 `profile-resolution.ts:173`); the chat-create initial avatar and the
  avatar job also move onto it (ledger §3(b)) — a shared helper, see §12.
- `services/dangerous_content/image_failover.rs`: `generate_image_with_concierge_failover(primary:
  (FailoverProfile, String), attempt, &ImageFailoverContext { db, chat_id,
  purpose, user_id, concierge_policy, chat, understudy, profile_kind, primary_via })`
  :348; `ImagePurpose::Avatar` exists :101; `services/wardrobe_item_image_generation.rs:426-470`
  is the worked pattern (`chat_id: None, chat: None, purpose: Wardrobe`, the
  trail JSON on failure) the preview rewrite copies with `purpose: Avatar`.
- `quilltap-host/src/avatar_preview.rs:40-86` `HostAvatarPreviewRenderer` /
  `render_over_provider` — the ONE provider call with no failover; the new shape
  either threads the failover INTO the core (`run_wardrobe_preview_avatar`
  calling `generate_image_with_concierge_failover` with the host renderer as the
  `attempt`) or re-homes the renderer seam one level down. The harness's
  `CannedRenderer` (web `wardrobe_routes_equivalence.rs`) replays the
  post-transcode `pv_ok_bytes` — the seam to keep.

### 9.6 The web crate (`crates/quilltap-web/src`)

- `lib.rs`: `/api/v1/characters/{id}/wardrobe` GET only (:364 →
  `characters_routes::characters_wardrobe_get` :820, action map `["instructions"]`
  :836); `/api/v1/wardrobe` GET+POST :475 (`wardrobe_routes::wardrobe_get` :113
  — `dispatch_action(&pairs, &["instructions"], "GET", "/api/v1/wardrobe")`;
  `wardrobe_post` :140); `/api/v1/wardrobe/transfers` :479; `…/preview-avatar`
  :484; `…/analyze-image` :488; `/api/v1/wardrobe/{itemId}` GET/PUT/DELETE :492
  (`wardrobe_item_get` :198 — `?action=wear-history` + the 404-first unknown-action
  rule :205-222); `/api/v1/wardrobe/{itemId}/images` :500
  (`wardrobe_images_routes.rs`: `GET_ACTIONS = ["save-targets"]`, POST `ACTIONS =
  ["generate","upload","set-current","delete-image","save-to-store"]` :66 — in
  v4's literal order, asserted by `the_action_maps` :567; `link-image` goes
  between `upload` and `set-current`). **No REST edge exists for the character
  wardrobe POST / item routes, nor for ANY project / group wardrobe route** —
  those ride `POST /api/dispatch` only (the SPA calls dispatch verbs, §9.7). The
  five folder actions therefore need: the `folders` arm on the character GET
  edge and the General GET edge (`["instructions", "folders"]`), the four POST
  arms on the General POST edge (`["instructions","create-folder","rename-folder",
  "delete-folder","move-items"]`), and dispatch verbs for every tier.
- Census guards a new route / verb must satisfy:
  `crates/quilltap-web/tests/dispatch_wrong_type_census.rs` (every TYPED field of
  every `Request` variant needs a row — `census_covers_every_typed_request_field`
  :2713; the wardrobe rows :626-637, :794, :1034, :1889, :2308-2376);
  `tests/tri_state_edges_share_the_decoder.rs` (`TYPED_ONLY_HAND_BUILT_CONSTRUCTIONS_BY_FILE`
  — `("wardrobe_routes.rs", 17)` :561 and the total `117` :599 move with every
  hand-built `CoreRequest::X {` in a `*_routes.rs`; `Option<Option<…>>` fields
  must go through `request_envelope` instead); `crates/quilltap-harness/tests/web_edge_body_parse_guard.rs`
  `COLLAPSE_CENSUS` :107 (`wardrobe_images_routes.rs` → 1 :186-196; any new
  `.and_then(Value::as_…)` body read in a routes file needs a row with a
  verdict — prefer a ported Zod parser, zero new sites); `PARSER_WIRING` :35.
  The `activity_span_sites_guard.rs` / `lora_log_anchor_guard.rs` families
  (harness) name preview-avatar — re-measure if the `trackActivity` span or the
  LoRA log anchor moves.

### 9.7 The SPA meeting point (read-only here)

`apps/web/src/app/wardrobe/wardrobe.api.ts` dispatches
`{character,project,group,}Wardrobe{List,Create,Update,Delete}` (+ `chatGet`);
`wardrobe-instructions.api.ts` the eight instructions verbs; `item-images.api.ts`
`wardrobeItemImage{Delete,SaveTargets,SaveToStore,SetCurrent}` + `wardrobeItemImagesList`
(`generate` / `upload` over HTTP). `apps/web/src/app/core/core-contract.ts`
mirrors the variants (`CharacterWardrobeListRequest` :1346 …
`GroupWardrobeDeleteRequest` :2133). The SPA lane (the folder browser, bug 229)
needs the new verb names + body shapes from THIS lane's `types.rs` — the
contract C1 of the round.

---

## 10. v4-at-pin vs v5-main — the port's work list

| surface | v4 at `4a38222e6` | v5 main | lane action |
|---|---|---|---|
| Collection GET body | `{ mountPointId, wardrobeItems }` EVERY tier | char `{ wardrobeItems }` (:910), General `{ wardrobeItems }` (:261) | add `mountPointId` first on char + General |
| General unprovisioned GET | 200 `{ wardrobeItems: [] }` | 500 `Failed to fetch archetype wardrobe items` | flip |
| GET read failure | middleware 500 `Internal server error` | the OLD `Failed to fetch …` 500s (:263, :347, :382, char `Failed to fetch wardrobe item` :2888) | retire the per-route catch sentences (char/General) |
| POST body | 201 `{ wardrobeItem }` + origin + wear, every tier | project/group `{ mountPointId, wardrobeItem, wardrobeItems }`; char/General `{ wardrobeItem }` bare | one shape |
| PUT body | `{ wardrobeItem }` + origin + wear | bare `{ wardrobeItem }` | add tails |
| PUT ordering | 404 → Zod → imageFileId | char ✓; **project :1736/:1763 and group :1440/:1465 imageFileId first** | reorder |
| Cycle | 400 every tier | char :2701 **500**, General :312 **500**; project/group 400 ✓ | map |
| Archived character write | 409 `An archived character's wardrobe cannot be changed` (create / PUT / DELETE / folders) | char `Db(CharacterArchived)` → 500 (:2698-2702) | map (the sentence already exists at `wardrobe_transfers.rs:72`) |
| Item routes provisioning | never (`ensure: false`) | project item GET / wear-history / PUT / DELETE `ensure_project_wardrobe_mount` (:1646/:1681/:1723/:1832) | read-only resolve (`officialMountPointId`), 404 `Project` when absent |
| General write unprovisioned | 500 `Quilltap General store is not provisioned yet` | create: 500 `Internal server error` (:313) | sentence |
| Instructions no-vault char | 404 `Character` (even clearing) | clear no-op `{ instructions: null }` :3558 | restrict the no-op to General |
| Instructions log titles | `<tag> Read dressing instructions` / `Dressing instructions updated` | per-tier spellings | retitle (capture-pinned) |
| Item log titles | `<tag> Listed / Created / Updated / Deleted wardrobe item`, `Read wardrobe item wear history`, `Group-tier wardrobe read` | OLD spellings; no char create/update INFO | retitle + add |
| `folder` on create / update bodies | optional, resolved via `location.resolveFolder`; `isSameFolder` no-op | no key (parsers refuse? — `parse_archetype_body` :134 / `parse_wardrobe_fields` :1069 / `build_wardrobe_patch` :1898 — unknown keys are ignored by Zod `z.object` default strip, so a `folder` today is silently DROPPED) | accept + resolve (needs the keystone's `resolve_folder`) |
| Five folder actions | §1.10 | absent | NEW verbs + edges + handlers |
| Unknown action lists | GET `["instructions","folders"]`; POST `["instructions","create-folder","rename-folder","delete-folder","move-items"]` | GET `["instructions"]`; POST `["instructions"]` | widen both edges' `availableActions` |
| Transfers probe order | vault → groups (later first) → project (no ensure) → General | vault → project (ENSURING) → groups (forward) → General; explicit source ensures | reorder; stop provisioning on probes |
| Transfers Zod enum order | both `character|group|project|general` | source `character|project|group|general`, dest `general|project|group|character` | the sentences |
| Transfers `folder` | planned items carry `normalizeWardrobeFolderPathOrRoot(folder)`; destination folder created via `createItem`; INFO gains `folder` | none | carry + log key (needs keystone `normalize…OrRoot`) |
| Transfers `createAtDestination` | `createItem(location, planned, { preserve })` → `[ItemMutations] Created wardrobe item` DEBUG per item | the scope switch :717 | route through the keystone's create |
| Preview profile | `resolveAvatarImageProfile` → 400 `No image profile is configured. Set one in Settings → Images before generating avatars.` | OLD two sentences :1151/:1162 | port the helper (shared) |
| Preview failure | 422 `The image provider declined to draw this portrait` / 502 `Image generation failed: <m>` + `details { trail, refused }` + WARN `[Avatar Preview] Image generation failed` | 500 `Internal server error` :1268 | the failover + the envelope (the web edge restores 502 from `Internal+details` — the frozen-boundary idiom at `wardrobe_images_routes.rs::render`) |
| Preview response | `{ fileId, url, mimeType, prompt, rerouted, trail }`; files row via `createGeneratedFileRow` (no description; answering model; decoded w/h); llm-log row via the logged attempt | 4 keys; `description` :1277; 1024×1792? (check `run_wardrobe_preview_avatar` :1098-1340) | extend |
| Preview logs | DEBUG `Generating preview` NEW; INFO `Preview saved` gains `profileId`, `rerouted`, `context: 'api.v1.wardrobe.preview-avatar'` | check :1240 | align |
| `link-image` | 201 `{ image, current }`; 400 `That picture cannot be linked`; two `[WardrobeImages]` lines + the route INFO | absent | NEW |
| `WardrobeItemHome` | carries `location`; `resolveWardrobeItemHome` = `location.readItems(true)` | `services/wardrobe_container.rs:230` (mount-point shape) | keystone's call; this lane consumes |

Unchanged and already right: the 404 labels per tier; `imageFileId must name
one of this item's own pictures`; the DELETE cleanup order (writable gate FIRST
— verify v5 char/project/group delete resolve the tombstone before
`cleanup_equipped_refs`; project :1832 ensures instead); `?scope=group` body
`{ wardrobeItems }` (no `mountPointId`); the wear-history payload; the
instructions 409 sentence; the transfers' success / 400 / 500 sentences; the
preview's `Character not found` / `API key …` / `No appearance data …` / `Image
provider returned no image data` / `Failed to save avatar preview`.

---

## 11. Harness — families, corpora, predicted reds

| Rust family | oracle | corpus / fixture | cases | predicted under the pin |
|---|---|---|---|---|
| `quilltap-web/tests/wardrobe_routes_equivalence.rs` (`QT_ORACLE_WARDROBE_ROUTES`) | `harness/oracle/cases/wardrobe-routes.test.ts` (one jest test; the oracle stubs `createImageProvider`, captures real `convertToWebP` → `pv_ok_bytes`) | `harness/oracle/fixtures/wardrobe-routes.json#cases` (119) over `crates/quilltap-web/tests/fixtures/wardrobe-routes-{main,mount}.db` (+ `.meta.json`) | `list_*` (4), `create_*` (6), `item_get_*` (4), `update_*` (21 incl. `update_cycle`, the archive seven, the `imageFileId` six), `delete_*` (4), `dest_list`, `tr_*` (15), `outfit_*`/`eq_*`/`regen_*` (28 — the outfit lane's), `pv_*` (7), `cw_*` (18 — the character tier), `wh_*` (9) | **RED**: every `list_*` / `cw_no_scope*` (`mountPointId`), `create_ok` / `create_full` (origin + wear), `update_*` (tails), `update_cycle` (400), `tr_zod_bad_action`-class (enum order; `tr_dest_invalid`), `tr_*` bodies gain `folder`, `pv_no_key` (sentence), `pv_bad_key`?, `pv_ok` (two new keys; files row width/height; no description; the llm-log row), `cw_item_get*` / `cw_update_*` (tails), the oracle's own `pv_*` stub must move to `generateImageWithConciergeFailover`'s seam (the stubbed `createImageProvider` still underlies it — verify). Also `p4d256_route_lines_are_v4s` (log titles). |
| `quilltap-harness/tests/group_wardrobe_routes_equivalence.rs` (`QT_ORACLE_GROUP_WARDROBE`, `QT_FIXTURE_GW_*`) | `group-wardrobe.test.ts` (`npx jest --silent`) | `group-wardrobe.json#cases` (33) over `qt-gw-{main,mount}.db` (built, /tmp) | `gw_list*` (6), `gw_create*` (5), `gw_get*` (3), `gw_update*` (11), `gw_delete*` (3), `gw_wear_history*` (4) | **RED**: `gw_create*` (body shape), `gw_update*` (tails; the `imageFileId` × 404 order — `gw_update_image_file_id_*` + `gw_update_missing_item`), `gw_get*` (origin/wear present? check), log titles. `4a38222e6` changed this v4 test file (+29) — the oracle may need the folders cases added. |
| `wardrobe_instructions_routes_equivalence.rs` (`QT_ORACLE_WARDROBE_INSTRUCTIONS_ROUTES`) | `wardrobe-instructions-routes.test.ts` | `wardrobe-instructions.json#routeCases` (46) over `wardrobe-instructions-{main,mount}.db` (committed, web fixtures) | `char_*` (22), `group_*` (8), `project_*` (8), `general_*` (8) | **RED**: `char_post_no_vault_clear_is_a_noop` (now 404 `Character`), `char_post_no_vault_write_500` (now 404), log titles if capture-pinned; `general_*` unchanged. |
| `wardrobe_transfers_tier2_equivalence.rs` (`QT_ORACLE_WTR`, `QT_FIXTURE_WTR_*`) | `wardrobe-transfers.test.ts` | `wardrobe-transfers-tier2.json#scenarios` (24) + `__destinations` over `qt-wtr-{main,mount}.db` | `copy_character_to_general … copy_to_archived_destination_conflict` | **RED**: every landing item gains `folder` in the dump + response; `components_null_rejected`/`source_null_rejected` (enum order in the joined Zod sentence); `move_group_to_character` / `copy_group_to_general` (probe order only matters when an id exists in two tiers — add a `group_shadows_project` + `probe_never_provisions_project` scenario); the INFO line's `folder` key. |
| `wardrobe_item_images_tier2_equivalence.rs` (`QT_ORACLE_WII`) + web `wardrobe_item_images_routes_equivalence.rs` (`QT_ORACLE_WIIR`) | `wardrobe-item-images.test.ts` / `wardrobe-item-images-routes.test.ts` | `wardrobe-item-images-tier2.json` / `wardrobe-item-images-routes.json#requests` (87) | `post_unknown_action` / `post_no_action` (the `availableActions` list) | **RED**: `post_no_action`, `post_unknown_action`, `post_bare_action` (the list gains `link-image`); ADD `link_image_ok_201`, `link_image_unreadable_400`, `link_image_foreign_scope_404`, `link_image_archived_409`, `link_image_zod`. |
| `wardrobe_public_read_equivalence.rs`, `wardrobe_tier2_equivalence.rs` (`wardrobe-tier2.json#ops` 5), `vault_wardrobe_*`, `wardrobe_tools_*` | — | — | — | keystone / tools lanes' (the `folder` key on every parsed item reddens them all); this lane only CONSUMES. |
| `wardrobe_wear_history_equivalence.rs` (`QT_ORACLE_WARDROBE_WEAR_HISTORY`, reads `wardrobe-routes-{main,mount}.db`) | `wardrobe-wear-history.test.ts` | `wardrobe-wear-history.json#cases` (13) | `attach_*`, `resolve_*`, `payload_*` | green (unchanged seam) unless the fixture pair is rebuilt. |
| web `characters_wardrobe_route.rs`, `wardrobe_instructions_routes.rs`, `wardrobe_item_wear_history_route.rs` | — (edge unit tests) | materialized instance | the action maps | **RED**: the GET edges' `availableActions` (`["instructions","folders"]`). |
| `web_edge_body_parse_guard.rs` / `dispatch_wrong_type_census.rs` / `tri_state_edges_share_the_decoder.rs` | — | source census | — | **RED on every new variant / hand-built construction** until the rows are added (§9.6). |

**Recipes** live in each family's `//!` header (resolved through the `.ts`
header by `harness/tools/recipe_sweep.py`; run with `/opt/homebrew/bin/python3.13`
and `--v5w ~/source/quilltap-v5`, the spaceless path). The fixture pairs the
routes families read (`wardrobe-routes-{main,mount}.db`,
`wardrobe-instructions-{main,mount}.db`) are COMMITTED — regen copies them to
/tmp; the `gw` / `wtr` / `wii` / `wpr` pairs are rebuilt per regen (minted ids —
mask, never `ORDER BY`). Every regen must run from a worktree PINNED at
`4a38222e6` (the ledger §5.1 — the live checkout is past it).

**New differential families this lane owes** (tier 2, v4's REAL factory as the
oracle, all four tiers): `wardrobe_folders_routes_equivalence` — `GET
?action=folders` × {provisioned, no store (project/group owner exists),
unprovisioned General, missing owner, foreign/vault-less character};
`create-folder` × {new, existing-any-casing, nested-with-existing-ancestor,
reserved `images`, `..`, 9-deep, root (`''` → 400 `A folder needs a name`),
archived character}; `rename-folder` × {ok, case-only, collision 409, missing
404, `/` in name, root}; `delete-folder` × {ok with items + subfolders (counts),
missing, stray non-item file 409, same-named subfolder 409, root}; `move-items`
× {ok batch serialized, borrowed id 400 (character view), 501 ids Zod, hand-made
target as stored, archived 409}; plus `create`/`PUT` with `folder` (new, same —
no move, hand-made as stored). A `wardrobe_item_images_routes` extension for
`link-image` (§6). The preview-avatar's `pv_*` cases grow: `pv_refused_422`,
`pv_provider_502`, `pv_rerouted_ok` (the understudy answers; `rerouted: true`,
`trail` non-null) — the oracle's provider stub must drive the failover's
refusal classifier, as `wardrobe-item-images-routes.test.ts` already does for
`generate_refused_422` / `generate_provider_502`.

---

## 12. Cross-lane meeting points

1. **Keystone → this lane (HANDOFF IN):** `resolve_wardrobe_location(scope,
   owner_id, user_id, ensure) -> Option<WardrobeLocation>` with `read_items(include_archived)`,
   `find_item`, `create`, `update`, `update_many(patches, require_all)`, `delete`,
   `list_folders`, `resolve_folder`, `create_folder`, `rename_folder`,
   `delete_folder -> { moved_items, moved_folders, parent }`, `writable_mount_point_id`
   (the tombstone); `group_locations_for_character`; `location_key`; the error
   enum carrying `Cycle`, `FolderPath(msg)`, `FolderNotFound`, `FolderConflict(msg)`,
   `ItemNotInLocation(ids)`, `CharacterArchived`, `NoMount` (§1.4's map is this
   lane's); `create_item(location, body, { lookup, preserve })`, `update_item`,
   `move_items_to_folder`, `component_lookup_for`; `create_wardrobe_folder` /
   `rename_wardrobe_folder` / `delete_wardrobe_folder`; `normalize_wardrobe_folder_path[_or_root]`;
   `WardrobeItem.folder` in the struct (`vault_overlay.rs:334-368` — between
   `archived_at` and `created_at`) and the parser deriving it from the path; the
   `wardrobe_err` maps in the three tier files move onto that enum. Until it
   lands this lane cannot compile its folder arms — **sequence after the
   keystone, or build on a shared contract file (the P4.D255 C1 idiom).**
2. **Item-images lane ↔ this lane:** `WardrobeItemHome` gaining the location;
   `link_wardrobe_item_image` in `services/wardrobe_item_images/service.rs`;
   the images POST action order. One owner — propose THIS lane (the route arm
   and the service fn are 60 lines together).
3. **Avatar / chat-create lane ↔ this lane:** `resolve_avatar_image_profile(user_id,
   override, chat)` in `services/image_profile_resolution.rs` (v4
   `profile-resolution.ts:173-210`, three DEBUG lines) is needed by preview-avatar
   HERE, by the chat POST's initial avatar and by the avatar job (ledger §3(b)
   `chats` POST, `refreshAvatarForOutfit`). One owner writes it; the other
   consumes — name it in both orders.
4. **Backup / `.qtap` lane:** `backup-service.ts` no longer reads `wardrobe.findAll()`
   (`wardrobeItems: []`) — not this lane's, but the `system_backup_equivalence`
   red is theirs.
5. **SPA lane:** the new verb names + body shapes (§9.1) and the `availableActions`
   lists; `wardrobeCollectionActionUrl` is a v4 client helper (the SPA uses
   dispatch, not URLs).
6. **Tools lane:** `wardrobe_create` files at the root (`createItem` with no
   `folder`) — shares `create_item`; `wardrobe_list` / `wardrobe_read`'s `folder`
   READ key is theirs.
7. **Help / docs vendor lane:** `API.md`'s two stale spots (§7) ride the re-vendor as-is (we vendor, never fix v4 prose).

---

## 13. Open questions for the order

1. **Variant shape** for the five folder actions: four tiers × five verbs
   (twenty variants, the SPA mirrors twenty) vs ONE scoped family carrying
   `WardrobeContainerScope` (the P4.D255 wear-history idiom, five variants). The
   dispatch census and `core-contract.ts` both grow either way.
2. **Ownership gate** (`character.userId !== userId → null`): v5 is single-user
   (`SINGLE_USER_ID`) and the character tier passes `_user_id` unused; the
   oracle plants `otherUserId` (`cw_item_get_wrong_owner`). Confirm the keystone's
   `resolve_wardrobe_location` carries the gate so the factory need not.
3. **The web edge for project / group wardrobe:** none exists (dispatch-only).
   v4's REST paths are not reachable over HTTP in v5 today — in scope to add, or
   stays a dispatch-only surface (the P4.9f1 decision)? The folder actions follow
   whichever.
4. **Preview-avatar failover seam:** thread `generate_image_with_concierge_failover`
   into the core around the host renderer (`attempt = |profile, key| renderer.render(…)`),
   or keep the renderer whole and add a core-side refusal classification — the
   first keeps v4's shape (`pv_refused_422` becomes a real case); the second
   cannot answer `rerouted: true`.
5. **`[ItemMutations]` / `[WardrobeLocation]` / `[FolderMutations]` DEBUG lines**
   are the keystone's — but this lane's route-line capture pins (`p4d256_route_lines_are_v4s`)
   will see them in the same request; agree which family pins which.
6. **`move-items` log key `folder: body.folder`** is the RAW request string
   (v4 `:319`), while `create-folder` logs the RESOLVED `folder.path` — port
   both as written, flag as a v4 asymmetry (not a bug to file).
