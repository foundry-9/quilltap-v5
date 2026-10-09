# Survey — the wardrobe SERVER half of `ed8b15b50` (bugs 183 + 184) and `01a83539d` (the picture viewer's save-to-store)

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` HEAD
`01a83539d` (`4.10.0-dev.143`, tree clean); v5 = main at `96cfdaaf7`.

Scope: the Rust core + web side of (a) `ed8b15b50`'s bugs 183/184 — the wear
date wording (`formatWornRelative`), `WardrobeWearRepository.findSummariesForWearer`,
`wardrobe_list` / `wardrobe_read` text, result keys and model-visible
DESCRIPTIONS — and (b) `01a83539d`'s server — the wardrobe images route's
`GET ?action=save-targets` / `POST ?action=save-to-store`,
`lib/photos/photo-album-options.ts`, `lib/photos/save-image-response.ts`, and
the message Save Image route's 400 → 409. **Out of scope** (other surveys):
`ed8b15b50`'s bugs 185/186 (restore store binding, store names), every SPA
file (`FullScreenImageViewer`, `wardrobe-image-viewer`, `SaveImageDialog`,
`wear-display.ts`'s SPA port), the help tree / `docs/v4/` re-vendor.

Every fact below is from the hunks (`git show <sha> -- <path>`) and the HEAD
files (`git show 01a83539d:<path>`); line numbers are HEAD unless marked.

---

## 1. v4 at HEAD — `ed8b15b50` bug 183 (`formatWornRelative`)

### 1.1 `lib/wardrobe/wear-display.ts`

- `:79-84` NEW `formatWornRelative(ts: number, nowMs = Date.now()): string`:
  `const phrase = formatRelativeDays(ts, nowMs)`; `'last week'` → `'a week ago'`;
  `'last month'` → `'a month ago'`; every other rung passes through. The
  `formatRelativeDays` ladder itself (`lib/format-time.ts`) is UNCHANGED, so the
  memory-recall labels (`memory_weighting::format_relative_age`) do not move.
- `:65-69` `formatWornWhen(iso, nowMs)` — `!iso` → `''`; `Date.parse` NaN → `''`;
  else `formatWornRelative(ts, nowMs)` (was `formatRelativeDays`). Doc comment
  `:61-63` reworded ("a week ago").
- `formatWornWhen` / `formatWearLine` have **no server-side consumer**: v4's only
  callers are SPA components (`ProjectWardrobeManager.tsx:370`,
  `wardrobe-item-row.tsx:271`, `WardrobeWearHistorySection.tsx:45`). The item
  routes' `wear` block (`lib/wardrobe/wear-history.ts:55`) still reads the
  household `findSummaries` — **unchanged** (the file is not in the commit).
- Server consumers of `formatWornRelative`: `wardrobe-list-handler.ts:233` and
  `wardrobe-read-handler.ts:157` (both formerly imported `formatRelativeDays`
  from `@/lib/format-time`).

## 2. v4 at HEAD — `ed8b15b50` bug 184 (whose wear)

### 2.1 `lib/schemas/wardrobe-wear.types.ts:74-85`

```ts
export interface WardrobeWearPerspective {
  household: WardrobeWearSummary;   // every wearer's rows folded together
  yours: WardrobeWearSummary;       // the reading character's own row, or the zero summary
}
```
(a TS interface — no zod schema).

### 2.2 `lib/database/repositories/wardrobe-wear.repository.ts`

- `:434-464` NEW `findSummariesForWearer(itemIds, wearerCharacterId)`:
  - ids = `Array.from(new Set(itemIds.filter(Boolean)))`; `result` pre-filled
    `id → { household: neverWornSummary(), yours: neverWornSummary() }`;
    `ids.length === 0` → return the (empty) map with **no query, no log**.
  - `safeQuery(op, 'Error reading wear summaries for wearer', { itemCount:
    ids.length, wearerCharacterId }, result)` — the FALLBACK form: a throw logs
    the repository's collection-enriched ERROR `Error reading wear summaries
    for wearer` `{collection: 'wardrobe_wear_stats', itemCount,
    wearerCharacterId, error}` and answers `result` (possibly part-filled — the
    mutation is in-place, as `findSummaries`).
  - op: `rowsByItem = await this.findRowsByItem(ids)`; per `[itemId, rows]` (Map
    insertion order = SELECT order) `result.set(itemId, { household:
    summarize(rows), yours: summarize(rows.filter(row => row.wearerCharacterId
    === wearerCharacterId)) })`. The unattributed row (`wearerCharacterId`
    NULL) never matches.
  - then DEBUG `Read wear summaries for wearer` `{itemCount: ids.length,
    wornCount: rowsByItem.size, wearerCharacterId}` on `log =
    logger.child({ module: 'wardrobe-wear' })` (`:59`) — logged ONLY on success.
- `:491-499` NEW private `findRowsByItem(itemIds)` — groups `findRowsForItems`
  into a `Map<itemId, rows[]>`; `findSummaries` (`:409-426`) refactored onto it
  (**no behaviour change**).
- **No new SQL.** The read is `findRowsForItems` (`:502-514`), unchanged:
  `SELECT * FROM "wardrobe_wear_stats" WHERE "itemId" IN (?,…)` per
  `SQLITE_VARIABLE_CHUNK_SIZE` chunk. "One read" = the same chunked SELECT,
  split in memory.
- `summarize` (`:148-162`) unchanged: skips `wearCount <= 0`; first = min
  string, last = max string (strict `>`, so ties keep the first row's chat).

### 2.3 `lib/tools/handlers/wardrobe-list-handler.ts`

- `:20` import `formatWornRelative` (was `formatRelativeDays`).
- `:118-123` `repos.wardrobeWear.findSummariesForWearer(filteredItems.map(i =>
  i.id), context.characterId)` — over the type/appropriateness-filtered items,
  BEFORE the `include_equipped` filter (unchanged position). The executor passes
  `characterId || ''` (`lib/chat/tool-executor.ts:757`).
- `:128-130` `household = wear?.household ?? neverWornSummary()`, `yours =
  wear?.yours ?? neverWornSummary()`.
- `:137-162` the result literal — **runtime key order**: `item_id, title,
  description, image_prompt, image_file_id, types, appropriateness, is_own,
  is_equipped, equipped_slot, wear_count, last_worn_at, worn_by_you,
  last_worn_by_you_at`, then the composite spread `is_composite,
  component_item_ids, component_titles`. `wear_count`/`last_worn_at` =
  HOUSEHOLD; `worn_by_you` = `yours.wearCount`; `last_worn_by_you_at` =
  `yours.lastWornAt`. (The TS interface `wardrobe-list-tool.ts:52-93` lists
  the composite fields first — the interface order is NOT the wire order.)
- `:171-185` INFO `Wardrobe list completed` `{context: 'wardrobe-list-handler',
  userId, characterId, chatId, totalItems, filteredCount, hasTypeFilter,
  hasAppropriatenessFilter, includeEquipped, compositeCount, neverWornCount,
  neverWornByCallerCount (NEW — items with worn_by_you === 0), withPictureCount}`.
- `:209-212` NEW `listTimes(count)`: `1` → `'once'`, else `` `${count}×` ``
  (so 2 → `2×`, NOT "twice").
- `:225-237` `formatWardrobeListWearNote(item: {wear_count, last_worn_at,
  worn_by_you, last_worn_by_you_at}, nowMs)`:
  1. `!item.wear_count` → `' · never worn'`
  2. `!item.worn_by_you` → `` ` · never worn by you (worn ${listTimes(wear_count)} by others)` ``
     (no date read at all — `last_worn_at` is now unused by the note)
  3. `yourLastMs = item.last_worn_by_you_at ? Date.parse(…) : NaN`; `when =
     NaN ? '' : `, last ${formatWornRelative(yourLastMs, nowMs)}``
  4. `yours = `worn by you ${listTimes(worn_by_you)}${when}``
  5. `wear_count <= worn_by_you` → `` ` · ${yours}` ``
  6. else `` ` · ${yours} (${listTimes(wear_count)} in the household)` ``
  (Old: `' · never worn'` on a zero count OR an absent/unparseable date, else
  `' · last worn <relative>'`.)
- `:246-278` `formatWardrobeListResults` unchanged apart from `:243` doc; the
  note still precedes the picture handle (`:271-274`).

### 2.4 `lib/tools/handlers/wardrobe-read-handler.ts`

- `:33` import `formatWornRelative`; `:155-158` `relativeWearDate(iso, nowMs)` →
  `formatWornRelative` (unparseable → the iso itself, unchanged).
- `:97-127` `buildWardrobeReadWear` UNCHANGED (still `findHistory`, household
  totals + every wearer, `is_you` = `wearer.characterId !== null &&
  wearer.characterId === characterId`); `:105-111` DEBUG `Wardrobe read resolved
  wear history` unchanged. **The `wear` JSON object's shape does not change** —
  only the formatted paragraph does. `wardrobe_update` reuses
  `buildWardrobeReadOutput` (`wardrobe-update-handler.ts:150`) but its formatter
  (`:168-176`) prints only `Updated "<title>" (<id>).` — unaffected.
- `:182-211` `formatWardrobeWearParagraph(wear, nowMs)` — REWRITTEN:
  - guard unchanged: `!wear || wear_count === 0 || wearers.length === 0 ||
    !last_worn_at` → `'Never worn.'`
  - `you = wearers.find(w => w.is_you)`; `others = wearers.filter(w => !w.is_you)`;
    `latest = wearers[0]`; `othersList = joinPhrases(others.map(w =>
    `${wearerPhrase(w)} (${timesPhrase(w.wear_count)})`))`
  - **you present:** `mine` = `you.wear_count === 1` ?
    `` `You have worn it once, ${relativeWearDate(you.last_worn_at)}.` `` :
    `` `You have worn it ${timesPhrase(you.wear_count)}, first ${wearDate(you.first_worn_at)}, last ${relativeWearDate(you.last_worn_at)}.` ``;
    `others.length === 0` → `mine`; else `mostRecent = latest.is_you ? '' :
    `, most recently ${relativeWearDate(latest.last_worn_at)} by ${wearerPhrase(latest)}``
    → `` `${mine} Worn ${timesPhrase(wear.wear_count)} in all${mostRecent}; also by ${othersList}.` ``
  - **you absent:** `last = `${relativeWearDate(wear.last_worn_at)} by ${wearerPhrase(latest)}``;
    `wear_count === 1` → `` `You have never worn it. Worn once, ${last}.` ``;
    else `head = `You have never worn it. Worn ${timesPhrase(wear.wear_count)} by others, first ${wearDate(wear.first_worn_at ?? wear.last_worn_at)}, last ${last}``
    → `others.length > 1 ? `${head}: ${othersList}.` : `${head}.``
  - Note the household count now uses `timesPhrase` (`twice` for 2) where the
    old head printed `Worn ${n} times`; in the you-absent arm `othersList`
    includes `latest` again (the jest `departed` case pins the repeat).
- `wearerPhrase` / `timesPhrase` / `wearDate` (en-GB, UTC) / `joinPhrases` unchanged.

### 2.5 Tool DESCRIPTIONS (model-visible)

- `lib/tools/wardrobe-list-tool.ts:125-139` — the changed span (old → new):
  `…flag (shared archetypes can be worn but not edited), when it was last worn
  (or that it never has been), and, when the item has a picture, its …` →
  `…flag (shared archetypes can be worn but not edited), how often YOU have worn
  it and when you last did (worn_by_you / last_worn_by_you_at), with the whole
  household's total beside it as context (wear_count / last_worn_at — other
  characters' wears included, so not your own), and, when the item has a
  picture, its image_file_id — pass that to describe_image …`
- `lib/tools/wardrobe-read-tool.ts:108-116` — `…its wear history (how often it
  has been worn, when, and by whom), and its picture's…` → `…its wear history
  (how often you have worn it, then how often the whole household has and by
  whom), and its picture's…`
- Parameters unchanged. v5 holds the bytes in the GENERATED
  `crates/quilltap-core/src/tools/definitions/data.rs` (regen:
  `harness/oracle/cases/tool-definitions.ts` →
  `harness/oracle/tools/gen-tool-catalog.mjs`) — never hand-edit.

### 2.6 v4 jest tests `ed8b15b50` added/changed (mirror material)

- `__tests__/unit/lib/wardrobe/wear-display.test.ts` — `formatWearLine` at 9/13/30/59
  days → `Worn 8× · last a week ago` / `a month ago`; `formatWornRelative` at
  0/1/3/8/15/45/120 days.
- `__tests__/unit/lib/database/repositories/wardrobe-wear.repository.test.ts` —
  `findSummariesForWearer(['coat','hat','never'], CHAR_A)` over four increments
  (household 3 / yours 1 with CHAT_1; hat household 1 / yours zero; never → two zeros).
- `__tests__/unit/lib/tools/handlers/wardrobe-wear-readout.test.ts` — the list
  call `(['item-1','item-2'], CALLER)`, the five-tuple per item, line
  `  [top] Greatcoat · worn by you 3×, last 3 days ago (4× in the household)`,
  `never worn by you (worn 115× by others)`, `worn by you once, last 3 days
  ago`, the 9/40-day rungs; the read paragraph's six shapes incl. the crowded
  household (`You have worn it 13 times, first 13 Jun 2026, last today. Worn
  116 times in all; also by Laura (25 times) and Gary (78 times).`), the single
  other wear, the departed list.
- `wardrobe-handlers.test.ts` — the repo mock renamed only.

## 3. v4 at HEAD — `01a83539d` server

### 3.1 `lib/photos/photo-album-options.ts` (NEW, 108 lines)

- `:20` `export type PhotoAlbumKind = 'character' | 'project' | 'document-store' | 'general'`.
- `:22-35` `export interface PhotoAlbumOption { mountPointId; name; kind;
  characterId?; participantId?; isUserCharacter?; isDefault? }` — MOVED here
  from `app/api/v1/chats/[id]/actions/photo-albums.ts` (that file now
  `import type` + `export type { PhotoAlbumKind, PhotoAlbumOption }` at
  `:25,:27`; its `handleGetPhotoAlbums` body is UNCHANGED).
- `:37-42` `KIND_ORDER = { character: 0, project: 1, 'document-store': 2, general: 3 }`.
- `:53-107` `listAllPhotoAlbumOptions(repos: Pick<…,'docMountPoints'|'characters'|'projects'>)`:
  1. `Promise.all([repos.docMountPoints.findEnabled(),
     getArchivedCharacterVaultMountPointIds(), getGeneralMountPointId(),
     repos.characters.findAllRaw(), repos.projects.findAll()])`.
     - `findEnabled()` (`doc-mount-points.repository.ts:142-154`) = `findByFilter({enabled:true})`
       (no ORDER BY → rowid order) under `safeQuery` FALLBACK `[]`, ERROR
       `Error finding enabled mount points`. ALL mount types (database,
       filesystem, obsidian) are offered.
     - `getArchivedCharacterVaultMountPointIds()` (`lib/mount-index/character-vault.ts:121-131`)
       — raw characters with truthy `archivedAt` AND truthy
       `characterDocumentMountPointId`; DEBUG `Collected archived character
       vault mount points` `{archivedWithVault, charactersScanned}`.
  2. `archived = Set(archivedVaultIds)`; `characterByVault` = for each raw
     character with truthy `characterDocumentMountPointId` AND falsy
     `archivedAt`, `Map.set(vaultId, c)` (a shared vault → the LAST character
     in `findAllRaw` order wins).
  3. `projectStoreIds = Set(projects.map(p => p.officialMountPointId).filter(truthy))`.
  4. per enabled mp (rowid order): skip `archived.has(mp.id)`; then
     - character vault → `{ mountPointId: mp.id, name: character.name, kind:
       'character', characterId: character.id, isUserCharacter:
       character.controlledBy === 'user' }` (NO `participantId`, NO `isDefault`)
     - `mp.id === generalId` → `{ mountPointId, name: mp.name, kind: 'general', isDefault: true }`
     - in `projectStoreIds` → `{ mountPointId, name: mp.name, kind: 'project' }`
     - else → `{ mountPointId, name: mp.name, kind: 'document-store' }` (incl.
       an unlinked retired `<Name> Version … Store` — bug 186's names)
  5. `:97` `options.sort((a,b) => KIND_ORDER[a.kind] - KIND_ORDER[b.kind] ||
     a.name.localeCompare(b.name))` — V8 stable sort; ties keep rowid order.
     **General sorts LAST** (but is the default).
  6. `:98-100` no option flagged → `options[0].isDefault = true` (appended as
     the LAST key).
  7. `:102-106` DEBUG `[photo-album-options] Listed every store as a save
     target` `{enabledStores: mountPoints.length, archivedVaultsSkipped:
     archivedVaultIds.length, offered: options.length}` (`archivedVaultsSkipped`
     counts the archived vault ids COLLECTED, not the ones actually skipped).

### 3.2 `lib/photos/save-image-response.ts` (NEW, 45 lines)

- `:14-24` `savedImageResponse(saved)` → `successResponse({ saved: true,
  mountPoint: saved.mountPointName, relativePath, linkId, keptAt, fileId,
  sha256 })` — 200, byte-identical to the bodies the two Salon routes built inline.
- `:32-45` `saveImageErrorResponse(error)` — `ALREADY_SAVED` →
  `NextResponse.json({ error: message, code: 'ALREADY_SAVED', relativePath:
  error.existingRelativePath, keptAt: error.existingCreatedAt }, { status: 409 })`
  (undefined riders drop out of the JSON); any other code → `badRequest(message)`
  = 400 `{ error }`.

### 3.3 `app/api/v1/wardrobe/[itemId]/images/route.ts`

- `:14-15` header lines for the two actions; `:24-29` the why.
- `:59-66` imports `saveImageToAlbum`, `SaveImageToAlbumError`,
  `SaveImageRequestSchema`, the two response helpers, `listAllPhotoAlbumOptions`,
  `getArchivedCharacterVaultMountPointIds`. `LOG_TAG = '[Wardrobe Images v1]'` (`:75`).
- `:288-294` `handleSaveTargets`: `findHome` (400 query issue / 404 `Wardrobe
  item not found`) → `listAllPhotoAlbumOptions(ctx.repos)` → DEBUG
  `[Wardrobe Images v1] Listed save targets` `{itemId, scope, count}` → 200 `{ albums }`.
- `:297-338` `handleSaveToStore`, in order:
  1. `findHome` (400 / 404).
  2. `SaveImageRequestSchema.safeParse(await req.json().catch(() => ({})))` —
     invalid JSON is `{}` (NOT a 500, unlike set-current/delete-image); issues
     joined `'; '` → 400. Schema (`lib/photos/save-image-to-album.ts:52-57`):
     `fileId: z.string().min(1,'fileId is required')`, `mountPointId:
     z.string().min(1,'mountPointId is required')`, `caption:
     z.string().optional()`, `tags: z.array(z.string()).optional()`. A JSON
     `null`/array body reaches Zod as-is → `Invalid input: expected object,
     received null|array`.
  3. `meta = { itemId, scope, fileId, mountPointId }`.
  4. `listWardrobeItemImages(ctx.repos, itemId)`; `fileId` not among them →
     INFO `[Wardrobe Images v1] Refused to save a picture that is not the
     item's` meta → 400 `That picture does not belong to this wardrobe item`
     (the same sentence as `mapWriteError`'s foreign arm, `:132`).
  5. `getArchivedCharacterVaultMountPointIds()` includes `mountPointId` → INFO
     `[Wardrobe Images v1] Refused to save into an archived character's vault`
     meta → **409** `{ error: "That store belongs to an archived character and
     cannot be written to" }`.
  6. `saveImageToAlbum({ mountPointId, fileId, caption: caption ??
     found.home.item.title, tags: tags ?? [], attribution: { name:
     ctx.user.name ?? 'Quilltap', id: ctx.user.id ?? null, role: 'user' } })` —
     NO `chatId` (no sceneState snapshot); `caption: ''` stays `''` (`??`).
  7. success → INFO `[Wardrobe Images v1] Saved wardrobe picture to a store`
     `{...meta, relativePath, linkId}` → `savedImageResponse` (200).
  8. `SaveImageToAlbumError` → INFO `[Wardrobe Images v1] Save to store
     rejected` `{...meta, code, message}` → `saveImageErrorResponse` (409
     ALREADY_SAVED / 400 `Image not found: <id>` | `Mount point not found:
     <id>` | `Failed to read image bytes: …` | `Image <id> has empty bytes` |
     NOT_AN_IMAGE). Anything else rethrows → the middleware's 500.
  - The ITEM is never written, so the archived-character write guard does not
    apply: an archived character's item may be copied OUT (`:27-29`).
- `:341-343` `GET = withActionDispatch({ 'save-targets': handleSaveTargets },
  handleList)` — **behaviour move on a ported route:** before, GET ignored
  `?action`; now no action → list, `save-targets` → targets, any other action
  (or a bare `?action=`) → 400 `{ error: 'Unknown action: <a>',
  availableActions: ['save-targets'] }` + WARN `Unknown action requested`
  (`lib/api/middleware/actions.ts:83-99,163-183`).
- `:346-353` POST map order: `generate, upload, set-current, delete-image,
  save-to-store` — `availableActions` in the no-action / unknown-action 400s
  grows by `save-to-store`.

### 3.4 The Salon save doors

- `app/api/v1/chats/[id]/messages/[messageId]/route.ts:298-370` `handleSaveImage`
  — success `savedImageResponse(saved)` (`:350`, bytes unchanged); a
  `SaveImageToAlbumError` logs INFO `[SaveImage] rejected` (`:353`, unchanged)
  then `saveImageErrorResponse(error)` (`:359`) — **ALREADY_SAVED moves 400
  `{error}` → 409 `{error, code, relativePath, keptAt}`**; every other code
  stays 400.
- `app/api/v1/chats/[id]/actions/save-image.ts:100,108` — the gallery door
  refactored onto the helpers; **bytes unchanged** (it already 409'd).

### 3.5 v4 jest tests `01a83539d` added

- `__tests__/unit/app/api/v1/wardrobe/[itemId]/images/route.test.ts` (+116):
  save-targets 200 `{albums}` / 404 before listing; save-to-store 200
  `toMatchObject({saved, mountPoint, relativePath})` with
  `saveImageToAlbum` called `{mountPointId, fileId, caption: <title>, tags: [],
  attribution: {…role:'user'}}`, foreign 400, archived-vault 409, ALREADY_SAVED
  409 `{code, keptAt}`, body without a store 400 (all mocked).
- `__tests__/unit/lib/photos/photo-album-options.test.ts` (+82): classification
  + archived skip + General default; no-General → first option default
  (`[{mountPointId:'mp-notes', name:'Notes', kind:'document-store', isDefault:true}]`).

---

## 4. v5 today — file:line counterparts

### 4.1 Bugs 183 / 184 (ported by P4.D262 / P4.D256; Faithful to the bugs)

- `crates/quilltap-core/src/format_time.rs:235-270` `format_relative_days` —
  the ladder (unchanged target). **No `format_worn_relative` exists.**
- `crates/quilltap-core/src/tools/wardrobe_list.rs`
  - `:20-46` `WardrobeListItemResult` — fields end `…equipped_slot, wear_count,
    last_worn_at, is_composite?, component_item_ids?, component_titles?`
    (serde order = v4 runtime order today); **missing `worn_by_you: i64`,
    `last_worn_by_you_at: Option<String>`** (insert after `last_worn_at`,
    always serialized).
  - `:223-231` `find_summaries(&filtered_ids)` (household) → must become
    `find_summaries_for_wearer(&filtered_ids, character_id)`.
  - `:265-266` the two wear fields.
  - `:297-318` `format_wardrobe_list_wear_note(wear_count, last_worn_at,
    now_ms)` — the OLD rule (`last worn <relative>` via `format_relative_days`,
    `js_date_parse_ms`). Signature must grow to the four fields.
  - `:387-388` the call site in `format_at`.
  - **Pre-existing gap:** v5 logs NONE of v4's handler lines — no INFO
    `Wardrobe list completed` (now with `neverWornByCallerCount`), no ERROR
    `Wardrobe list tool execution failed` (`let _ = user_id; // logging only`,
    `:140`). Same for `wardrobe_read`'s WARN `Wardrobe read tool validation
    failed` / ERROR `Wardrobe read tool execution failed` (`:422-429`). Only
    the DEBUG `Wardrobe read resolved wear history` is ported (`:340-347`).
- `crates/quilltap-core/src/tools/wardrobe_read.rs`
  - `:138-147` `relative_wear_date` → `format_relative_days` (must call the new fn).
  - `:158-204` `format_wardrobe_wear_paragraph` — the OLD `Worn N times, first …,
    last … by <latest>. Also worn by …` shape. Rewrite whole.
  - `:332-367` `build_wardrobe_read_wear` — unchanged target (shape stays).
  - `:88-156` `wearer_phrase`, `times_phrase`, `wear_date` (en-GB `Sept`),
    `join_phrases` — reused as-is.
- `crates/quilltap-core/src/db/wardrobe_wear_stats.rs`
  - `:357` `pub fn summarize`; `:417` `distinct_ids`; `:451-479`
    `log_read_failure` (two-arm match: `itemCount` OR `itemId`; target
    `quilltap::db`, `collection` first) — needs a third arm `{collection,
    itemCount, wearerCharacterId, error}`.
  - `:647-684` `find_summaries` — inline Vec-grouping (`:663-670`, SELECT-order
    preserving); `:722-740` `find_rows_for_items` (the one SQL). **No
    `find_summaries_for_wearer`, no `WardrobeWearPerspective`.** DEBUG lines in
    this file use `target: "quilltap::wardrobe_wear", module = "wardrobe-wear"`
    (`:539-544`, `:632-637`) — the new DEBUG follows that pattern.
- `crates/quilltap-core/src/services/wardrobe_wear_history.rs:54` — the item
  routes' `find_summaries` (household) — **stays**.
- `crates/quilltap-core/src/tools/executor.rs:1630-1663` — `run_wardrobe_list`
  serializes `out.items` (new keys ride automatically); `run_wardrobe_read`
  spreads the output (shape unchanged).
- `crates/quilltap-core/src/tools/definitions/data.rs:277` (`wardrobe_list`),
  `:282` (`wardrobe_read`) — the OLD descriptions (GENERATED file).
- `help/wardrobe.md` (vendored, embedded) carries the old prose too — the help
  cluster's re-vendor, not this one.

### 4.2 The save doors (P4.6ab unit 1 — message; P4.D174 — gallery)

- `crates/quilltap-core/src/api/chat_media.rs:236-353` `message_save_image` —
  success body inline `:326-334`; **`:335-342` maps every
  `SaveImageErrorCode` incl. `AlreadySaved` → `bad_request(err.message)`** (the
  400 that moves). No `[SaveImage]` log lines at all (pre-existing gap).
- `chat_media.rs:412-574` `chat_save_gallery_image` — success body inline
  `:532-540`; ALREADY_SAVED `:551-568` → `Response::error(Conflict, msg)` with
  `e.code = Some("ALREADY_SAVED")` + `e.already_saved =
  Some(AlreadySavedRiders{relative_path, kept_at})` — **this is the helper to
  extract**, then reuse at the message arm and the wardrobe action.
- `crates/quilltap-core/src/api/types.rs:5068-5085` `CoreError.already_saved` /
  `AlreadySavedRiders` (each rider `skip_serializing_if None` — v4's undefined
  drop); `:5223-5243` `already_saved_wire_body()` → `{error, code?,
  relativePath?, keptAt?}` (v4's key order).
- `crates/quilltap-web/src/dispatch.rs:129-160` `merge_already_saved_riders` —
  the dispatch wire already flattens riders for ANY verb (private fn).
- `crates/quilltap-core/src/photos/save_image_to_album.rs:60-115` (input /
  output / error types), `:231` `save_image_to_album(main, mount, input, bytes,
  side_effects, kept_at)`; `photos/save_attribution.rs:46` `pub fn
  parse_save_image_request(body: &Value)` (Zod sentences; **no top-level object
  check** — a `null`/array body yields two `received undefined` issues where
  Zod says `expected object, received …`; harmless over dispatch, where the
  body is always the flattened object), `:153-251` `resolve_save_attribution`,
  `:272-282` PRIVATE `read_user_name` (filters `''` → `None`, where v4's
  `user.name ?? 'Quilltap'` keeps `''`).
- `api/engine.rs:4489-4542` the `MessageSaveImage` / `ChatSaveGalleryImage`
  arms (`ready_save_image()` `:6348-6366` + `MountEmbeddingSideEffects` +
  `clock::now_iso()`); `:4544-4547` `ChatPhotoAlbums`.

### 4.3 The chat album list vs v4's new instance-wide list

- `chat_media.rs:663-780` `chat_photo_albums` = v4 `handleGetPhotoAlbums`
  (unchanged by `01a83539d`): chat-scoped (participant vaults with
  `participantId`, project official + LINKED stores, General), NO sort, default
  = active impersonated user char → first user char → general → first. It is a
  `json!` literal list — **v5 has no `PhotoAlbumOption` type**.
  `listAllPhotoAlbumOptions` differs by design (instance-wide, every enabled
  store, archived vaults excluded, sorted by kind then name, General default,
  no `participantId`); only the option SHAPE is shared. Nothing to change on
  the chat side.
- Reusable reads for the new list:
  - `services/mount_index/document_text_search.rs:133-157`
    `archived_character_vault_mount_point_ids(main)` — v4's function, BUT its
    DEBUG keys are snake_case (`archived_with_vault`, `characters_scanned`,
    target `quilltap::document_text_search`) where v4 logs `archivedWithVault`
    / `charactersScanned` — a pre-existing divergence (P4.D122).
  - `db/doc_mount_points.rs:720-735` `find_enabled_for_search()` — `SELECT id,
    name, storeType … WHERE enabled = 1`, no ORDER BY (rowid), **no fallback**
    (v4's `findEnabled` falls back to `[]` + ERROR).
  - `db/characters_read.rs:319` `find_all_raw(main)`; `db/projects.rs:337`
    `ProjectsRepository::find_all` (overlay) /
    `projects::find_official_mount_point_id_raw`; `instance_settings::
    get_general_mount_point_id(main)`; `collation.rs:51` `locale_compare` (ICU4X
    en-US — v4's `localeCompare`).

### 4.4 The wardrobe images route (P4.D263)

- `crates/quilltap-core/src/api/wardrobe_item_images.rs` — `:41` `LOG_TAG`;
  `:92-133` `find_home` / `pub resolve_home` (the 400/404 gate, on the writer);
  `:177-190` `map_write_error` (`That picture does not belong to this wardrobe
  item` literal at `:185-187`); `:196-238` `list` / `list_on_home`
  (`Response::WardrobeItemImages(json!)`). No save-targets / save-to-store.
  `WardrobeItemHome::item_title()` exists (`services/wardrobe_container.rs:250`);
  `list_wardrobe_item_images(conn, item_id)` → `Vec<FileEntry>`.
- `crates/quilltap-core/src/api/types.rs:2796-2840` — five P4.D255 wardrobe
  verbs (`WardrobeItemImagesList {scope, container_id?, item_id}` …); no save verbs.
- `crates/quilltap-core/src/api/engine.rs:5478-5545` — their arms.
- `crates/quilltap-web/src/wardrobe_images_routes.rs` — `:49` `ACTIONS: [&str;4]
  = ["generate","upload","set-current","delete-image"]`; `:71-104` `render`
  builds `{error, details?}` ONLY — **it does not merge `code` / the
  ALREADY_SAVED riders** (the REST 409 would lose them); `:159-177` GET (no
  action dispatch at all — ignores `?action`); `:180-269` POST
  (`dispatch_required_action`, then query, then `resolve_home`, then body).
  `crates/quilltap-web/src/query.rs:152` `dispatch_action` (optional, with a
  default) exists for the new GET.
- The SPA (`apps/web/src/app/wardrobe/item-images.api.ts:117,170,186`) uses the
  dispatch verbs for list / set-current / delete and REST only for generate /
  upload; the save dialog (`apps/web/src/app/images/save-image-dialog.ts:208-
  218, 296-316`) reads `{albums}` and treats `kind === 'conflict' || code ===
  'ALREADY_SAVED'` as "already in this album".

## 5. What the ledger row / commit prose got wrong

1. **`01a83539d` row, "Reddens: the chat-media / message-route families where
   they pin the `ALREADY_SAVED` 400" — no family pins it.**
   `chat_gallery_equivalence` drives `message_save_image` only for
   `message_save_non_uuid` / `message_save_empty_body` (both refused before the
   save, `chat_gallery_equivalence.rs:726-745`); `courier_images_routes_
   equivalence` only for `save_image_riya` / `_general` / `_not_attached`
   (`:560-605`). The 400 → 409 move is **unpinned today**: it needs a NEW case
   (both sides), not a regen.
2. **`ed8b15b50` row, "Reddens: `wardrobe_*` tool / routes families" — the
   ROUTES families do not move for 183/184.** The item routes' `wear` block and
   `?action=wear-history` read `findSummaries` / `findHistory` (household),
   untouched (`wear-history.ts` is not in the commit); `formatWornWhen` is
   SPA-only. The server reds are `tool_image_generation_equivalence`
   (`list_note` + `paragraph` rows), `wardrobe_tools_equivalence` (output keys
   + formatted text), `project_roster_access_equivalence` (its `wardrobe_list`
   output gains two keys even with no ledger), `tool_definitions_equivalence`
   (two descriptions).
3. "`findSummariesForWearer` (household + the reader's own row in one read)" —
   accurate only as "one chunked SELECT"; there is NO new SQL (it reuses
   `findRowsForItems`, split in memory). It also carries a NEW DEBUG line and
   its own fallback ERROR sentence (`… for wearer`, with `wearerCharacterId`).
4. The ledger's "GET gains `withActionDispatch`" omits the consequence: an
   unknown / bare GET `?action` now 400s (`availableActions:
   ['save-targets']`) where it listed before; and every POST `availableActions`
   400 grows `save-to-store` — `wardrobe_item_images_routes_equivalence`'s
   `post_no_action` / `post_unknown_action` / `post_bare_action` redden on a
   HEAD regen.
5. "debug `[photo-album-options] Listed every store as a save target`" — right,
   but `archivedVaultsSkipped` is the COLLECTED archived-vault count, not the
   skipped one. "General `isDefault`" — right; note General sorts LAST
   (`KIND_ORDER.general = 3`); the new help text's "Quilltap General, which is
   offered first" is prose, not code (it is pre-SELECTED, not listed first).
6. Not in the ledger: **v4's own dialog renders the archived-vault 409 as
   "That picture is already in this album."** (`SaveImageDialog.tsx:188` keys
   on `res.status === 409` alone). Latent in v4 (save-targets never offers an
   archived vault; reachable only by archiving between list and save). v5's
   dialog (`kind === 'conflict'`) is the same shape — an SPA-cluster note.

## 6. Differential plan

### 6.1 Existing families — predicted at the `01a83539d` target

| family | predicted | why |
|---|---|---|
| `tool_definitions_equivalence` (`harness/oracle/cases/tool-definitions{,-canonical}.ts`) | RED → green on `data.rs` regen | the two descriptions |
| `tool_image_generation_equivalence` (`cases/tool-image-generation.ts`) | RED | `list_note` rows pass only `{wear_count, last_worn_at}` → at HEAD every non-zero row reads `never worn by you (worn N× by others)`; all `paragraph` rows re-worded; the `relative` rows stay green |
| `wardrobe_tools_equivalence` (`cases/wardrobe-tools.ts`) | RED → green after port | `list_all` / `list_type_top` / `list_after` / `list_as_recipient` items gain two keys and new notes; every `read_*` wear paragraph. The corpus already plants caller + recipient + departed + unattributed rows and has `list_as_recipient` — no corpus change strictly needed |
| `project_roster_access_equivalence` | RED → green after port | `wardrobe_list` `output` items gain `worn_by_you: 0`, `last_worn_by_you_at: null` |
| `wardrobe_item_images_routes_equivalence` (quilltap-web) | RED on HEAD regen | `availableActions` grows (3 POST rows) |
| `chat_gallery_equivalence`, `courier_images_routes_equivalence` | stay green | the gallery bytes are unchanged; no message ALREADY_SAVED row exists |
| `wardrobe_wear_stats_tier2_equivalence`, `wardrobe_wear_history_equivalence`, `wardrobe_routes_equivalence`, `group_wardrobe_routes_equivalence` | stay green | household reads unchanged |
| `dispatch_wrong_type_census` (quilltap-web) | RED once variants land | new `scope` rows + `EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE` 464 → 464+N (measure; ~+4 for two `item_id` + two `container_id` if the save body rides a flattened `Value`) |
| `web_edge_body_parse_guard` (`COLLAPSE_CENSUS` row for `wardrobe_images_routes.rs`, count 1) | RED only if the edge adds an `and_then(Value::as_…)` site | keep body parsing in the ported Zod parser |

### 6.2 New / grown cases

1. **tier-1 `formatWornRelative`** — add a `worn_relative` kind to
   `cases/tool-image-generation.ts` at the same 21 day offsets as `relative`
   (incl. the 7 / 13.99 / 30 / 59.99 boundaries); Rust row kind in
   `tool_image_generation_equivalence.rs`.
2. **tier-1 list note** — reshape `list_note` rows to the four fields:
   `[wear_count, last_worn_at, worn_by_you, last_worn_by_you_at]` covering
   `0/*`, `N/0` (115×), `1/1`, `N/N` (only caller), `N/M<N`, `worn_by_you>0`
   with `null` / `''` / `'not a date'` dates, the 9- and 40-day rungs, a
   date-only `'2026-06-12'`, an offset date.
3. **tier-1 paragraph** — keep the 12 rows (all re-render) and add v4's jest
   shapes: crowded household (you latest), single other wear, you-not-latest
   (`most recently … by`), departed-only (`othersList` repeats latest), exactly
   one other (no colon list), `wear_count 2` you-absent (`twice by others`),
   you `wear_count 1` with others.
4. **tier-2 `findSummariesForWearer`** — add an op to
   `cases/wardrobe-wear-stats-tier2.ts` (`case 'findSummariesForWearer'`,
   serialize `Array.from(map.entries())`) + the Rust arm in
   `wardrobe_wear_stats_tier2_equivalence.rs`: v4's jest corpus (coat × A/B,
   hat × B, never), the unattributed row, a `wearCount 0` row, duplicate / empty
   ids, the empty-ids early return, and the absent-table fallback (capture the
   ERROR `Error reading wear summaries for wearer` with `wearerCharacterId`).
   Capture-pin the DEBUG `Read wear summaries for wearer` in the core unit
   tests (`captured_with` — the ERROR twin precedent at
   `wardrobe_wear_stats.rs:1185-1196`).
5. **tier-2 `listAllPhotoAlbumOptions`** — NEW oracle case
   (`cases/photo-album-options.test.ts`, real v4 function over a real-DB
   fixture, the `wardrobe-item-images` builder or the photos fixture) + Rust
   family `photo_album_options_equivalence.rs`. Corpus: General; a live
   user-controlled vault and a live LLM vault; an ARCHIVED character's vault;
   a project official store; a plain document store; a retired unlinked
   `storeType='character'` vault; a DISABLED store; a filesystem mount; a
   shared vault pointer (last character wins); names that tie / differ only by
   case or accent (stable-sort + ICU); the no-General variant (first option
   default, `isDefault` LAST key). Comparand: the bytes of `{albums}` + the two
   DEBUG lines.
6. **route rows** — grow `harness/oracle/fixtures/wardrobe-item-images-routes.json`
   (+ the oracle `cases/wardrobe-item-images-routes.test.ts`, which already runs
   v4's REAL route): `get_save_targets_character` / `_general` / `_404` /
   `_400_query`; `get_unknown_action` (now 400) and `get_bare_action`;
   `save_to_store_ok` (default caption = title), `_with_caption`,
   `_empty_caption`, `_foreign` (400), `_archived_vault` (409),
   `_already_saved` (run twice → 409 riders), `_mount_not_found` (400),
   `_missing_store` / `_empty_body` / `_invalid_json` (`{}` → two issues) /
   `_null_body` (`expected object`), `_404_before_body`, `_400_query_first`,
   `_archived_item_copied_out` (an archived character's item saved to General —
   200). Comparands add the target store's new `photos/` link rows + the
   kept-image `.md` sidecar; **normalization must cover the timestamped
   filename** in `relativePath` (`photos/2026-…-slug.webp` — not an ISO
   timestamp) and the minted `linkId`.
7. **message 409** — add `message_save_already_saved` to
   `cases/chat-gallery.test.ts` (`MSG_ROUTE`, the same save twice) and the
   matching `msg_save` row in `chat_gallery_equivalence.rs` (409 `{error, code,
   relativePath, keptAt}` through the production rider home).

## 7. Proposed unit decomposition (one lane, or two: A = 183/184, B = viewer save)

**Lane A — the wear readout (bugs 183 + 184).** Owns
`crates/quilltap-core/src/format_time.rs`,
`crates/quilltap-core/src/db/wardrobe_wear_stats.rs`,
`crates/quilltap-core/src/tools/wardrobe_list.rs`,
`crates/quilltap-core/src/tools/wardrobe_read.rs`,
`crates/quilltap-core/src/tools/definitions/data.rs` (generated),
`harness/oracle/cases/tool-image-generation.ts`,
`crates/quilltap-harness/tests/tool_image_generation_equivalence.rs`,
`harness/oracle/cases/wardrobe-wear-stats-tier2.ts` (+ its fixture JSON if the
ops live there), `crates/quilltap-harness/tests/wardrobe_wear_stats_tier2_equivalence.rs`.
Regen-only (no edit): `wardrobe_tools_equivalence`,
`project_roster_access_equivalence`, `tool_definitions_equivalence`.

- **A1** `format_time::format_worn_relative(ts_ms, now_ms)` (doc-cite v4's home
  `lib/wardrobe/wear-display.ts:79-84`; a server home, since `formatWornWhen`
  has no server caller) + unit tests at the rungs → tier-1 row 1.
- **A2** `WardrobeWearPerspective { household, yours }` + `find_summaries_for_wearer(&[String],
  &str) -> HashMap<String, WardrobeWearPerspective>` + a private
  `find_rows_by_item` folding `find_summaries`' inline grouping (v4's refactor)
  + `log_read_failure`'s third arm + the DEBUG → tier-2 row 4.
- **A3** `wardrobe_list`: two struct fields after `last_worn_at`, the perspective
  read, `format_wardrobe_list_wear_note(wear_count, last_worn_at, worn_by_you,
  last_worn_by_you_at, now_ms)` with `list_times`; optionally the INFO
  `Wardrobe list completed` (with `neverWornByCallerCount`) and the ERROR —
  pre-existing gaps, rule port-or-record → tier-1 row 2 + the three regen families.
- **A4** `wardrobe_read`: `relative_wear_date` → A1; `format_wardrobe_wear_paragraph`
  rewritten → tier-1 row 3 + `wardrobe_tools_equivalence`.
- **A5** `data.rs` regen through `gen-tool-catalog.mjs` from the pin →
  `tool_definitions_equivalence`.

**Lane B — the viewer's save (`01a83539d` server).** Owns
NEW `crates/quilltap-core/src/photos/save_image_response.rs`,
NEW `crates/quilltap-core/src/photos/photo_album_options.rs`,
`crates/quilltap-core/src/photos/mod.rs`,
`crates/quilltap-core/src/photos/save_attribution.rs` (operator attribution helper /
`read_user_name` visibility; optionally the top-level-object check),
`crates/quilltap-core/src/api/chat_media.rs` (two arms only),
`crates/quilltap-core/src/api/wardrobe_item_images.rs`,
`crates/quilltap-core/src/api/types.rs` (two variants — hotspot),
`crates/quilltap-core/src/api/engine.rs` (two arms + maybe a pub save-image
seams accessor — hotspot),
`crates/quilltap-web/src/wardrobe_images_routes.rs`,
`crates/quilltap-web/tests/dispatch_wrong_type_census.rs`,
`crates/quilltap-web/tests/wardrobe_item_images_routes_equivalence.rs`,
`harness/oracle/fixtures/wardrobe-item-images-routes.json`,
`harness/oracle/cases/wardrobe-item-images-routes.test.ts`,
NEW `harness/oracle/cases/photo-album-options.test.ts` +
NEW `crates/quilltap-harness/tests/photo_album_options_equivalence.rs` (+ a fixture
or builder change), `harness/oracle/cases/chat-gallery.test.ts`,
`crates/quilltap-harness/tests/chat_gallery_equivalence.rs`; possibly
`crates/quilltap-harness/tests/web_edge_body_parse_guard.rs` (census count).

- **B1** `photos::save_image_response` — `saved_image_body(&SaveImageToAlbumOutput)
  -> Value` and `save_image_error_response(SaveImageToAlbumError) -> Response`
  (the gallery's 409 rider construction lifted verbatim); the gallery arm
  refactored onto it (bytes pinned green), the MESSAGE arm moved 400 → 409 →
  new row 7.
- **B2** `photos::photo_album_options` — `PhotoAlbumKind`, `PhotoAlbumOption`
  (serde camelCase, `Option`s `skip_serializing_if`, field order `mountPointId,
  name, kind, characterId, participantId, isUserCharacter, isDefault` —
  `isDefault` LAST matches both the General literal and the appended default),
  `list_all_photo_album_options(main, mount) -> Result<Vec<…>, DbError>` +
  DEBUG → row 5. Reads only (consume `find_enabled_for_search`,
  `archived_character_vault_mount_point_ids`, `characters_read::find_all_raw`,
  the project official ids, `get_general_mount_point_id`, `locale_compare`) —
  **edits neither `doc_mount_points.rs` nor `document_text_search.rs`**.
- **B3** core verbs in `api/wardrobe_item_images.rs`: `save_targets` /
  `save_targets_on_home` and `save_to_store` / `save_to_store_on_home(db,
  bytes, side_effects, user_id, scope, home, body, kept_at)` with v4's order
  (parse → foreign 400 → archived-vault 409 → save → B1), caption
  `caption.unwrap_or(home.item_title())`, `chat_id: None`, the operator
  attribution, the five `[Wardrobe Images v1]` lines capture-pinned; the two
  `Request` variants + engine arms (`ready_save_image()` +
  `MountEmbeddingSideEffects` + `now_iso()`), the census rows.
- **B4** web edge: GET via `dispatch_action(…, ["save-targets"], default list)`;
  POST `ACTIONS` + `save-to-store` (5, v4 order) with v4's
  `req.json().catch(() => ({}))` body semantics; `render` merges
  `already_saved_wire_body()` (or the edge's own `{error, code, relativePath,
  keptAt}`) for the 409 → row 6.

Order: A1 → A2 → A3/A4 → A5; B1 → B2 → B3 → B4. Lanes A and B share no file.

## 8. The wire contract the SPA lane needs (verbatim-ready)

Dispatch verbs (proposed names — the P4.D255 family pattern):

```ts
/** v4 GET /api/v1/wardrobe/[itemId]/images?scope=…&id=…&action=save-targets */
export interface WardrobeItemImageSaveTargetsRequest {
  type: 'wardrobeItemImageSaveTargets';
  scope: WardrobeContainerScope;      // 'character' | 'general' | 'project' | 'group'
  containerId?: string;               // required unless scope === 'general'
  itemId: string;
}
// → { albums: PhotoAlbumOption[] }   (bare body, 200)

/** v4 POST …/images?…&action=save-to-store */
export interface WardrobeItemImageSaveToStoreRequest {
  type: 'wardrobeItemImageSaveToStore';
  scope: WardrobeContainerScope;
  containerId?: string;
  itemId: string;
  fileId: string;                     // one of the item's own pictures (a files.id)
  mountPointId: string;
  caption?: string;                   // absent → the item's title; '' is kept as ''
  tags?: string[];
}
// 200 → { saved: true, mountPoint, relativePath, linkId, keptAt, fileId, sha256 }

export type PhotoAlbumKind = 'character' | 'project' | 'document-store' | 'general';
export interface PhotoAlbumOption {
  mountPointId: string;
  name: string;                       // character albums: the character's name
  kind: PhotoAlbumKind;
  characterId?: string;               // kind 'character'
  participantId?: string;             // chat list only — never in save-targets
  isUserCharacter?: boolean;          // kind 'character'
  isDefault?: boolean;                // exactly one option
}
```

Refusals (status / body; the dispatch envelope carries `kind`, `message`, `code`
and the riders): query issue → 400 (Zod sentences joined `'; '`, e.g. `id is
required for this scope`); item not in the container → 404 `Wardrobe item not
found`; body schema → 400 (`fileId is required`, `mountPointId is required`,
`Invalid input: expected string, received undefined`, …); a picture not the
item's → 400 `That picture does not belong to this wardrobe item`; an archived
character's vault → **409 `That store belongs to an archived character and
cannot be written to`** (no `code`); already in that store → **409 `{error,
code: 'ALREADY_SAVED', relativePath, keptAt}`**; other save errors → 400 with
the service message. **Message Save Image (`messageSaveImage`) now answers
ALREADY_SAVED as that same 409** (was 400). The SPA must tell the two 409s
apart by `code === 'ALREADY_SAVED'` (v4's dialog does not — §5.6).

REST (for the edge and the `qtap://` delegate): `GET
/api/v1/wardrobe/{itemId}/images?scope=…&id=…&action=save-targets`; `POST
…&action=save-to-store` with the JSON body above.

## 9. Meeting points

- **SPA cluster (viewer + dialog + `wear-display.ts`):** consumes §8 (variant
  names, `PhotoAlbumOption`, the 200 body, both 409s); ports its own
  `formatWornRelative` into `apps/web/src/app/wardrobe/wear-display.ts` (no
  shared code with A1). `core-contract.ts` is the SPA lane's.
- **Help / `docs/v4/` cluster:** `help/wardrobe.md` changes in BOTH commits
  (the tools' prose + "Viewing a Picture Properly"); `help_tree_equivalence`
  reddens on the HEAD regen there, not here.
- **Store-names cluster (bug 186):** it edits `db/doc_mount_points.rs`
  (`create`/`update` refusals, `findNameHolder`) and boot passes; B2 only CALLS
  `find_enabled_for_search` — keep it read-only so ownership does not collide.
  Retired `<Name> Version … Store` vaults surface as `document-store` options.
- **Restore cluster (bug 185) / memory cluster:** no shared file.
- **`api/types.rs` + `api/engine.rs`** are round hotspots — B3's two variants +
  two arms must be scheduled against any other lane adding verbs (the
  `dispatch_wrong_type_census` count is cumulative; the unifier re-measures).
- Dogfood **#156** (`with_both_conns` callers 500 on a degraded mount index):
  `save-targets` reads the mount index — on a degraded one v4's `findEnabled`
  falls back to `[]` (ERROR) while v5's `find_enabled_for_search` errors;
  decide with #156's ruling.

## 10. Open questions / measurements before coding

1. **`findEnabled` order + fallback** — confirm rowid order on a real-DB copy
   and v4's line on a failing read (`Error finding enabled mount points` keys);
   decide whether B2 answers `[]` + ERROR (v4) or propagates.
2. **`projects.findAll()` overlay vs the raw `officialMountPointId` column** —
   measure whether a broken project store drops the row in v4 (then the overlay
   read is the faithful one).
3. **Non-object save body** (`null`, `[]`, `"x"`) on the REST edge — Zod says
   `Invalid input: expected object, received …`; `parse_save_image_request`
   does not. Measure v4, then wrap at the edge (or add the check to the shared
   parser — it would not change the dispatch doors).
4. **Empty `users.name`** — v4 `ctx.user.name ?? 'Quilltap'` keeps `''`;
   v5's `read_user_name` maps `''` → `"Quilltap"` (shared with the Salon doors'
   attribution). Measure / rule once for all three doors.
5. **The REST edge's bytes seam** — `ready_save_image()` is private to the
   engine; either a pub accessor on `CoreEngine` (engine.rs hotspot) or the
   edge dispatches the verb after its own gate (double home-resolve; idempotent
   but resolves twice).
6. **Non-database mounts as targets** — v4 offers filesystem/obsidian stores;
   confirm both `saveImageToAlbum` and v5's `save_image_to_album` write there
   (or both fail the same way).
7. **Routes-family normalization** — `relativePath`'s timestamped filename and
   the sidecar's `keptAt`/frontmatter; reuse the photos fixture normalizer if
   one exists.
8. **Pre-existing log gaps** — `wardrobe_list` / `wardrobe_read` handler lines,
   `[SaveImage]` lines on the message door, the snake_case keys of
   `Collected archived character vault mount points`: port in this round or
   record as standing divergences.
9. **`isDefault` key position** for a non-General first option (appended last
   in v4) — pin it in row 5's no-General variant.
