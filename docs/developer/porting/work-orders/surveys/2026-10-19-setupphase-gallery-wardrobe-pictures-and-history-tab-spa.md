# Survey — the Aurora gallery's Wardrobe Pictures (`1cfc9b504`, server + client) and the Wardrobe dialog's History tab (the CLIENT half of `8d8618819` + `4ae20977f`)

Read-only planning survey, **2026-10-19**. v4 = `~/source/quilltap-server`,
read ONLY through `git show e8e17b2d9:<path>` (**THE PIN** = v4 `main`
`e8e17b2d92095bda008f03bb963eb4b1c470e680`, `4.11.0-dev.26`) and
`git show <sha> -- <paths>` / `git diff 25b8fab6a e8e17b2d9 -- <paths>`. The v4
working tree is at `a6292f169` (PAST the pin, carrying the Inventory programme)
and was never read. **The oracle baseline v5 has ported is `25b8fab6a`**; v5 =
`main` at **`fb4a4c264`** (`/Volumes/External 1/source_overflow/quilltap-v5`).
Every v4 line number below is AT THE PIN; every v5 line number is at
`fb4a4c264`. Nothing is taken from commit prose (ledger §5.3) — every claim is
from a shipped hunk. The ledger rows `8d8618819` (`drift-ledger.md:331`),
`4ae20977f` (`:334`) and `1cfc9b504` (`:335`) were read first and are built on;
where a hunk disagrees with a row this survey says so (§8).

**Measured: nothing past these commits touches the slice before the pin.**
`git diff --stat 1cfc9b504 e8e17b2d9 -- components/wardrobe components/images
'app/api/v1/chats/[id]' 'app/api/v1/characters/[id]' lib/query/keys.ts
lib/realtime/topic-map.ts lib/wardrobe` is EMPTY — the three trailing commits
(`54bc17a3c`, `f5db10034` docs; `e8e17b2d9` bug 252) leave every file here
alone. So "at the pin" = "at `1cfc9b504`" for this slice.

**Scope + the proposed cut (TWO lanes, disjoint files apart from the fenced
`core-contract.ts` regions):**

- **LANE G (gallery, whole):** the new `GET /api/v1/characters/[id]/wardrobe-images`
  route + its core service (`listWardrobeGalleryImages`, `findByLinkedToAny`,
  a server `wardrobe_origin_label` twin) + the dispatch verb + the SPA's
  Wardrobe Pictures section under Avatar Rolls + the caption CSS. Both halves in
  one lane (no sibling dependency).
- **LANE H (history tab, SPA only):** `outfit-history-tab.ts` (new), the
  dialog's third tab, the dialog's staging `isDirty` / `discard`, the history
  query keys, the realtime refresh. It CONSUMES the two chat verbs a sibling
  server lane (the outfit-history programme — another survey) adds; its LIVE
  beats are gated on that lane's flag.

---

## 0. The span for this slice, in one paragraph

`8d8618819` (phase 4 of v4's outfit-history programme) adds two chat actions —
`GET ?action=wardrobe-history&characterId=` (one character's numbered outfit
timeline in a chat, slots resolved against the operator pool, a ready-made
`changedBy` label and a one-line `summary` per entry) and `POST
?action=wardrobe-rollback` `{ characterId, index }` (an operator change that
returns a character to a recorded outfit, moving the history cursor) — and,
client-side, the Wardrobe dialog's **History** tab beside *Live outfit* and
*Outfit Builder* (chat only): entries newest first, *Wearing now* on the current
one, *Wear again* on the rest with a stale-garment tooltip; when the Live tab
holds unsaved staging for the character, a confirm, then the staging is
DISCARDED before the rollback and RE-SEEDED after the refetch so Done never
flushes it over the restored outfit (`useStagedLiveOutfits` gains `isDirty` /
`discard`); the `chats` realtime topic refetches every timeline in the chat
(`queryKeys.wardrobe.histories(chatId)`). `4ae20977f` adds `actorToolName` to the
history wire (between `actorName` and `changedBy`) and re-words `changedBy`
("the operator, as Bertie", "Pascal's dress_for_dinner (rolled by Jeeves)") —
the tab's only client hunk is the `OutfitHistoryEntry.actorToolName` type field
(it renders `changedBy` verbatim and never the raw actor fields).
`1cfc9b504` adds a read-only `GET /api/v1/characters/[id]/wardrobe-images`
(every GENERATED `IMAGE` file linked to any item in the character's wearable
pool — own vault, group stores, Quilltap General, NO project tier — archived
items included and flagged, newest first, each tagged with its item title and a
tier label) and the Aurora gallery's third, collapsible shelf **Wardrobe
Pictures** below Avatar Rolls: captioned tiles (title + *(archived)*, then
*Own wardrobe* / *Group · X* / *Shared · Quilltap General*), Download on hover,
click → the shared detail modal; hidden entirely when there is nothing to show.

---

## 1. v4 AT THE PIN — file by file

### 1.A `1cfc9b504` — Wardrobe Pictures (23 files, +716 / −18)

#### 1.A.1 `app/api/v1/characters/[id]/wardrobe-images/route.ts` (NEW, 46 lines) — VERBATIM behaviour

- `:19` `export const GET = createContextParamsHandler<{ id: string }>(async (_req, { user, repos }, { id }) => { … })`
  — the standard context handler (auth + repos); NO ownership check of the
  character (single-user), NO query parameters read.
- `:22` `if (!id) return badRequest('Missing character id');` → **400**
  `{ "error": "Missing character id" }` (unreachable through a real path
  segment; the avatar-rolls edge carries the same guard —
  `crates/quilltap-web/src/characters_routes.rs:390-392`).
- `:24` `const result = await listWardrobeGalleryImages(repos, id);`
- `:26-30` DEBUG **`[Characters/WardrobeImages v1] Listed wardrobe pictures`**
  `{ userId: user.id, characterId: id, total: result.total }`.
- `:32` `return successResponse(result);` → **200**, the BARE body (v4
  `successResponse` = `NextResponse.json(data)`, `lib/api/responses.ts:63-68`, no
  `data` wrapper): `{ entries, total }` in that key order.
- `:33-44` the catch:
  - `const message = error instanceof Error ? error.message : 'Failed to list wardrobe pictures';`
  - `if (message.startsWith('Character not found')) return notFound('Character');`
    → **404** `{ "error": "Character not found" }` (`notFound(r)` =
    `` `${resource} not found` ``, `responses.ts:150-155`). Any thrown Error
    whose message STARTS with `Character not found` maps here — not only the
    service's own throw.
  - else ERROR **`[Characters/WardrobeImages v1] Error listing wardrobe pictures`**
    `{ userId: user.id, characterId: id }` + the error object (third arg, only
    when an `Error`), then **500** `serverError(message)` →
    `{ "error": "<the thrown message>" }` (or `Failed to list wardrobe pictures`
    for a non-Error throw).

#### 1.A.2 `lib/wardrobe/gallery-images.ts` (NEW, 126 lines) — the service

- `:30` `LOG_CONTEXT = 'wardrobe.gallery-images'`; `:61` `export const OWN_WARDROBE_LABEL = 'Own wardrobe'`.
- `:33-54` `WardrobeGalleryImageEntry` — **the entry, key order is the literal at `:94-112`:**
  `fileId, fileName, url, thumbnailUrl, mimeType, fileSizeBytes, width, height,
  createdAt, generationPrompt, generationModel, itemId, itemTitle, scope,
  locationLabel, itemArchived, isCurrentItemImage`.
- `:63-126` `listWardrobeGalleryImages(repos, characterId)`:
  1. `:67-70` `repos.characters.findById(characterId)`; null → `throw new Error(`Character not found: ${characterId}`)`.
  2. `:71-79` `character.archivedAt` truthy → DEBUG
     **`[WardrobeGallery] Archived character — no wardrobe pictures`**
     `{ context: 'wardrobe.gallery-images', characterId }` and return
     `{ entries: [], total: 0 }` — the pool is NEVER read (v4 test pins it).
  3. `:82` `loadWearablePool(repos, characterId, [])` — project tier passed
     EXPLICITLY EMPTY; **not** operator mode (no 4th arg). `byId` holds every
     reachable item, **archived included** (`lib/wardrobe/pool.ts:57`), shadowed
     character > group > project > general.
  4. `:83-84` `items = Array.from(pool.byId.values())` (Map insertion order);
     empty → `{ entries: [], total: 0 }` WITHOUT the file read (v4 test pins it).
  5. `:87` ONE `repos.files.findByLinkedToAny(items.map((item) => item.id))` —
     the id argument is in pool order (v4 test pins `['i-own','i-grp','i-gen']`).
  6. `:90-113` per file, in the read's order: skip unless
     `file.category === 'IMAGE' && file.source === 'GENERATED'` (UPLOADED /
     IMPORTED / DOCUMENT rows dropped); `item = file.linkedTo.map((id) =>
     itemsById.get(id)).find(Boolean)` — the FIRST `linkedTo` id that is in the
     pool; none → skip. Field sources: `fileName: file.originalFilename`,
     `url: wardrobeImageUrl(file.id)` (= `/api/v1/files/<id>`,
     `lib/wardrobe/item-images.ts:122-124`), `thumbnailUrl:
     wardrobeImageThumbnailUrl(file.id)` (= `/api/v1/files/<id>?action=thumbnail`,
     `:126-128`), `mimeType: file.mimeType ?? null`, `fileSizeBytes: file.size`,
     `width: file.width ?? null`, `height: file.height ?? null`,
     `createdAt: String(file.createdAt)`, `generationPrompt:
     file.generationPrompt ?? null`, `generationModel: file.generationModel ??
     null`, `itemId: item.id`, `itemTitle: item.title`, `scope:
     item.origin.scope`, `locationLabel: wardrobeOriginLabel(item.origin) ??
     'Own wardrobe'`, `itemArchived: !!item.archivedAt`, `isCurrentItemImage:
     item.imageFileId === file.id`.
  7. `:115` `entries.sort((a, b) => (a.createdAt < b.createdAt ? 1 : a.createdAt > b.createdAt ? -1 : 0))`
     — a **string** comparison of `createdAt`, DESCENDING; ties return 0, and
     V8's sort is stable, so ties keep the file-read order.
  8. `:117-123` DEBUG **`[WardrobeGallery] Listed wardrobe pictures`**
     `{ context: 'wardrobe.gallery-images', characterId, itemCount: items.length,
     linkedFileCount: files.length, returned: entries.length }`.
  9. `:125` `return { entries, total: entries.length }`.
- `wardrobeOriginLabel` (`lib/wardrobe/wardrobe-container.ts:66-78`, unchanged by
  the span): `general` → `` `Shared · ${GENERAL_WARDROBE_NAME}` `` (=
  `Shared · Quilltap General`); `project` → `` `Project · ${origin.name}` ``;
  `group` → `` `Group · ${origin.name}` ``; `character` → `null` (→ the
  service's `Own wardrobe`). The `·` is U+00B7 with a space each side.
  `Project · …` is unreachable here (no project tier).

#### 1.A.3 `lib/database/repositories/files.repository.ts` (+27) — `findByLinkedToAny`

```ts
async findByLinkedToAny(entityIds: readonly string[]): Promise<FileEntry[]> {
  if (entityIds.length === 0) return [];
  return this.safeQuery(async () => {
    const CHUNK = 500;
    const seen = new Set<string>(); const out: FileEntry[] = [];
    for (let i = 0; i < entityIds.length; i += CHUNK) {
      const chunk = entityIds.slice(i, i + CHUNK);
      const files = await this.findByFilter({ linkedTo: { $in: [...chunk] } });
      for (const file of files) { if (seen.has(file.id)) continue; seen.add(file.id); out.push(file); }
    }
    return out;
  }, 'Error finding files linked to entities', { entityCount: entityIds.length });
}
```

- `safeQuery` with NO fallback = log-and-rethrow (`base.repository.ts:76-106`),
  but `findByFilter` is itself a FALLBACK read (`[]` + `Error finding entities
  by filter`, rows failing `FileEntrySchema` dropped with ERROR `Data validation
  failed` / WARN `Safe validation failed`) — exactly the shape v5's
  `find_by_linked_to` already records (`crates/quilltap-core/src/db/files.rs:484-495`:
  "A read failure is `findByFilter`'s own fallback … which is why the method's
  outer … wrap never fires"). So the outer `Error finding files linked to
  entities` line is UNREACHABLE through v4's real code; a per-chunk read failure
  yields that chunk's `[]` silently-but-logged.
- **Row order** within a chunk = v4's `findByFilter` translation of
  `linkedTo: {$in: [...]}` over the JSON array column — MEASURE it (the ledger
  says so too); v5's single-entity twin reads in rowid order. Dedupe is
  first-seen across chunks.

#### 1.A.4 `lib/query/keys.ts` (+2)

`:39-40` `/** Generated pictures of every wardrobe item the character can reach. */`
`wardrobeImages: (id: string) => ['characters', id, 'wardrobe-images'] as const,`
— under `queryKeys.characters`, after `avatarRolls`. **`lib/realtime/topic-map.ts`
is NOT touched**: the scoped `characters` row (`:96-104`) invalidates `detail` /
`prompts` / `subprompts` / `photos` only, so a scoped `characters` hint does NOT
refetch wardrobe pictures; the UNSCOPED hint (`characters.all`) does, by prefix.

#### 1.A.5 `components/images/embedded-gallery/hooks/useWardrobeImages.ts` (NEW, 109)

- `:11-28` the client's entry type — the wire MINUS `scope` (the client never
  reads `scope`).
- `:35-58` `toWardrobeGalleryImage(entry)` → `{ id: fileId, filename: fileName,
  filepath: url, url: thumbnailUrl /* the tile shows the thumbnail */, fullUrl:
  url, mimeType, size: fileSizeBytes, width: width ?? undefined, height: height
  ?? undefined, createdAt, caption: null, tags: [], itemId, itemTitle,
  locationLabel, itemArchived, isCurrentItemImage, generationPrompt,
  generationModel }`.
- `:69-76` `useQuery({ queryKey: queryKeys.characters.wardrobeImages(characterId),
  queryFn: … apiFetch(`/api/v1/characters/${characterId}/wardrobe-images`) })`
  — default options (no `enabled`, no `staleTime`).
- `:80-83` `handleImageError(imageId)` adds to `missingImages` and
  `console.warn('Wardrobe picture failed to load', { imageId })`.
- `:85-99` `downloadImage(image)` — `fetch(image.fullUrl)`; `!res.ok` →
  `throw new Error(`Failed to fetch image (${res.status})`)`; `res.blob()` →
  `triggerDownload(blob, image.filename)`; catch → toast **`Failed to download image`**
  + `console.error('Error downloading wardrobe picture', { characterId, imageId:
  image.id, error: message })`.
- `:101-108` returns `{ images, total: data?.total ?? 0, loading: isLoading,
  missingImages, handleImageError, downloadImage }`.

#### 1.A.6 `components/images/embedded-gallery/WardrobeImagesSection.tsx` (NEW, 137) — every string

- Props `{ characterId, entityName, onRefresh?, thumbnailSize }` (`:10-15`);
  `expanded` starts **false** (`:35`); `selectedIndex` −1.
- `:43-49` `captionFor(image)` → `{ title: w.itemArchived ? `${w.itemTitle} (archived)` : w.itemTitle, detail: w.locationLabel }`.
- `:51-54` download handler: `e.stopPropagation()` then `downloadImage`.
- `:56` **`if (!loading && total === 0) return null`** — absent entirely when
  nothing to show (and on a read error: `isLoading` false, `total` 0).
- `:59` `<section className="mt-8 border-t qt-border-default pt-6">`
- `:60-73` the toggle `<button className="flex w-full items-center gap-2 text-left" aria-expanded={expanded}>`:
  `<Icon name={expanded ? 'chevron-down' : 'chevron-right'} className="w-4 h-4 qt-text-secondary" />`,
  `<span className="qt-text-label">`**`Wardrobe Pictures`**`</span>`,
  `<span className="qt-text-label-xs">{loading ? '…' : `${total} picture${total === 1 ? '' : 's'}`}</span>`
  → `…` / `1 picture` / `N pictures`.
- `:75-80` `<p className="qt-text-label-xs mt-1 ml-6">` — rendered text
  (JSX whitespace collapsed; `&rsquo;` = U+2019; `{' '}` before the third name):
  > **Every likeness the house has sketched of the garments within {entityName}’s reach — from {entityName}’s own wardrobe, the wardrobes of any company {entityName} keeps, and the Quilltap General collection. Each plate bears a little card naming the garment and the wardrobe it hangs in.**
- `:82-104` expanded: `<div className="mt-4">`; loading → the spinner
  `<div className="flex items-center justify-center py-8"><div className="animate-spin rounded-full h-6 w-6 border-b-2 qt-border-primary"></div></div>`
  (identical to Avatar Rolls); else `GalleryGrid` with `settingAvatar={null}`,
  `deletingImage={null}`, `confirmDelete={null}`, NO `currentAvatarId` (so no
  tile is ever `isAvatar`), NO `onSetAvatar`, NO `onDeleteImage`, NO
  `onSaveToAlbum`, `captionFor`.
- `:106-134` the viewer door — v4's shared `ImageDetailModal` (NOT the wardrobe
  picture viewer), `image = { id, filename, filepath: selected.fullUrl, url:
  selected.fullUrl, mimeType: selected.mimeType ?? 'image/webp', size, width,
  height, createdAt, tags: [] }` — NO `linkId` (comment `:111-112`: "A `files`
  row, so no `linkId`: the modal's save actions post `{ fileId }` and the server
  re-links from the file's blob."); `onPrev` when `selectedIndex > 0`, `onNext`
  when `< images.length - 1`; `onAvatarSet={() => onRefresh?.()}`. No
  generation prompt is passed to the modal (v4 `ImageData` has none — the help
  page's "with its generation prompt" is NOT what the code does; §8).

#### 1.A.7 `GalleryGrid.tsx` / `GalleryImage.tsx` / `types.ts` / `index.ts` / `EmbeddedPhotoGallery.tsx`

- `GalleryGrid` (`:16-20,:33-34,:81-89`): `onSetAvatar?` / `onDeleteImage?`
  optional ("Omit for a read-only grid"), new `captionFor?`, forwarded as
  `caption={captionFor?.(image)}`.
- `GalleryImage` (`:59-68`): when `caption`:
  `<div className="qt-gallery-caption absolute inset-x-0 bottom-0 px-1.5 pt-4 pb-1 text-left pointer-events-none" title={`${caption.title} — ${caption.detail}`}>`
  `<div className="qt-gallery-caption-title truncate">{caption.title}</div>`
  `<div className="qt-gallery-caption-detail truncate">{caption.detail}</div></div>`
  — inside the tile button, AFTER the `<img>`, BEFORE the Avatar badge. The
  `title` separator is ` — ` (U+2014).
- `:79` the action overlay moves: `` `absolute ${caption ? 'top-1' : 'bottom-1'} right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity` ``.
- `:81` Set-as-avatar only when `!isAvatar && onSetAvatar`; `:135` Delete only
  when `onDeleteImage && (!isAvatar || isMissingImage)`. So a wardrobe tile
  shows ONLY the Download button (`title="Download image"`
  `aria-label="Download image"`, `:121-132`, hidden for a missing image).
  Missing image → the `image` icon placeholder (`:42-45`).
- `<img alt={image.filename}>`; `onError` →
  `console.warn('Image failed to load in gallery', { imageId })` then the
  section's handler (v5's gallery tab does not carry the generic line —
  `avatar-rolls-section.ts:61-64` records that; mirror it).
- `types.ts:100-127` `GalleryImageCaption { title, detail }`;
  `WardrobeGalleryImage extends GalleryImage { fullUrl, itemId, itemTitle,
  locationLabel, itemArchived, isCurrentItemImage, generationPrompt,
  generationModel }`.
- `EmbeddedPhotoGallery.tsx:187-192` mounts `<WardrobeImagesSection
  characterId={entityId} entityName={entityName} onRefresh={onRefresh}
  thumbnailSize={thumbnailSize} />` IMMEDIATELY AFTER `<AvatarRollsSection …/>`
  and BEFORE the album's own `ImageDetailModal`.
- Note `isCurrentItemImage`, `generationPrompt`, `generationModel`, `itemId`
  reach the client but **nothing renders them** at the pin.

#### 1.A.8 CSS — `app/styles/qt-components/_content.css` (+22, appended at the end of the layer, after `:1746`) + the theme-storybook mirror

```css
  .qt-gallery-caption {
    background: var(--qt-gallery-caption-bg, linear-gradient(to top, rgba(0, 0, 0, 0.78), rgba(0, 0, 0, 0.45) 60%, transparent));
    color: var(--qt-gallery-caption-fg, #fff);
    line-height: 1.2;
  }
  .qt-gallery-caption-title { font-size: 0.6875rem; font-weight: 600; }
  .qt-gallery-caption-detail { font-size: 0.625rem; opacity: 0.85; }
```

(with v4's block comment: "Gallery tile source caption — a scrim and two lines of
small text laid over the foot of a photo-gallery tile … Light text on a dark
scrim regardless of theme, since it sits on a photo.") The theme-storybook copy
is NO-PORT.

#### 1.A.9 Docs / help

- `help/character-gallery.md` +13 — section **"Wardrobe Pictures — likenesses
  of every garment within reach"** (the help lane / `p4.9i2` carries it; v5
  vendors `help/character-gallery.md` at the repo root).
- `docs/developer/API.md` +8 (`### Character Wardrobe Pictures`) → `docs/v4/`.
- `docs/CHANGELOG.md` — NO-PORT.

#### 1.A.10 Tests — `lib/wardrobe/__tests__/gallery-images.test.ts` (NEW, 123)

`describe('listWardrobeGalleryImages')`:
1. `captions generated pictures across tiers, newest first`
2. `returns nothing for an archived character without reading the pool`
3. `throws for a missing character`
4. `skips the file lookup when the pool is empty`

There is NO route test and NO client test in the commit.

### 1.B `8d8618819` + `4ae20977f` — the History tab (CLIENT half)

#### 1.B.1 `components/wardrobe/outfit-history-tab.tsx` (NEW, 228 lines + 2 from `4ae20977f`) — WHOLE

**Types (`:30-75`), verbatim:**

```ts
export interface OutfitHistorySlotItem { itemId: string; title: string | null; archived: boolean; missing: boolean }
export interface OutfitHistoryEntry {
  index: number; at: string; source: string;
  actorCharacterId: string | null; actorName: string | null;
  /** Pascal entries: the custom tool that made the change. */
  actorToolName: string | null;            // 4ae20977f
  changedBy: string; isCurrent: boolean; summary: string;
  slots: Record<WardrobeItemType, OutfitHistorySlotItem[]>
}
export interface OutfitHistoryResponse { currentIndex: number | null; entries: OutfitHistoryEntry[] }
interface RollbackResponse {
  equippedSlots: EquippedSlots; previousIndex: number; restoredIndex: number;
  skipped: Array<{ itemId: string; title: string | null; reason: 'archived' | 'missing' }>
}
export interface OutfitHistoryTabProps {
  chatId: string; characterId: string; characterName: string;
  /** True when the Live tab holds unsaved staged edits for this character. */
  hasStagedEdits: boolean;
  /** Throw away this character's staged Live-tab edits (after the operator agrees); with `reseed`, re-seed the Live tab from the refreshed snapshot. */
  discardStagedEdits: (options?: { reseed?: boolean }) => void;
  requestConfirmation: (message: string) => Promise<boolean>;
}
```

**`staleCount(entry)` (`:77-86`):** the number of DISTINCT `itemId`s across all
five slots (`WARDROBE_SLOT_TYPES` order) whose `archived || missing`.

**`EntrySlots` (`:88-112`):**
- no non-empty slot → `<p className="qt-text-xs qt-text-muted italic">`**`Nothing at all.`**`</p>`.
- else `<div className="flex flex-wrap gap-1">`, slots in canonical order
  (`top, bottom, footwear, accessories, hair`), one `<span>` per item, key
  `` `${slot}:${item.itemId}` ``:
  - class `` `qt-badge ${stale ? 'qt-badge-secondary' : WARDROBE_SLOT_META[slot].badgeClass} text-xs` ``
    (badge classes `qt-badge-wardrobe-{top,bottom,footwear,accessories,hair}`);
  - `title` = `` `${WARDROBE_SLOT_META[slot].label}${stale ? (item.missing ? ' — no longer exists' : ' — archived') : ''}` ``
    → e.g. `Top`, **`Top — archived`**, **`Top — no longer exists`** (labels
    `Top` / `Bottom` / `Footwear` / `Accessories` / `Hair`; ` — ` = U+2014);
  - text `{item.title ?? item.itemId}` + `{item.missing ? ' · gone' : item.archived ? ' · archived' : ''}`
    (U+00B7).

**The component (`:114-230`):**
- `:123` `nowMs = useNow(60_000)`.
- `:124-131` `useQuery({ queryKey: queryKeys.wardrobe.history(chatId, characterId),
  queryFn: apiFetch(`/api/v1/chats/${chatId}?action=wardrobe-history&characterId=${encodeURIComponent(characterId)}`) })`.
- `:133-158` `rollback = useMutation({ mutationFn: (index) => POST
  `/api/v1/chats/${chatId}?action=wardrobe-rollback` body
  `JSON.stringify({ characterId, index })` (`Content-Type: application/json`),
  onSuccess, onError, onSettled })`:
  - **onSuccess:** `skipped = result.skipped ?? []`; none →
    toast **`` `Back in outfit ${result.restoredIndex}` ``**; some →
    `names = skipped.map((s) => s.title ?? s.itemId).join(', ')`, toast
    **`` `Back in outfit ${result.restoredIndex}. ${skipped.length} item${skipped.length === 1 ? '' : 's'} could not be put back on: ${names}` ``**
    (success toast, not error).
  - **onError:** error toast `apiErrorMessage(error, 'Could not return to that outfit')`
    (v4's helper returns the server's `{ error }` first; the fallback
    **`Could not return to that outfit`** only for a non-Error).
  - **onSettled (success AND failure):** `await queryClient.invalidateQueries({ queryKey: queryKeys.wardrobe.all })`
    THEN `discardStagedEdits({ reseed: true })`.
- `:160-169` `wearAgain(index)`: if `hasStagedEdits` → `ok = await requestConfirmation(
  `Discard the staged changes to ${characterName}'s outfit and return to outfit ${index}?`)`
  — **`Discard the staged changes to Bertie's outfit and return to outfit 0?`**
  (ASCII apostrophe); `!ok` → return (nothing discarded, no POST);
  `discardStagedEdits()` (NO reseed) → then `rollback.mutate(index)`. Order
  pinned by v4's test: `discard`, `rollback`, `reseed`.
- `:171` `entries = [...(history.data?.entries ?? [])].sort((a, b) => b.index - a.index)`
  — **newest first by `index`** (the wire is ascending).
- Render (`:173-228`):
  - wrapper `<div className="space-y-2 mb-3">` (the Live tab's own wrapper);
  - `:175-178` `<p className="qt-text-xs qt-text-secondary px-1">` —
    **`Every change of clothes in this chat, numbered from the outfit they arrived in. Wear again returns to one — Aurora will say so.`**
    (one space at the JSX line break; ` — ` U+2014);
  - `:180` `history.isLoading` → `<p className="qt-text-xs qt-text-muted px-1">`**`Consulting the record…`**`</p>` (U+2026);
  - `:181-185` `history.isError` → `<p className="qt-text-xs qt-text-danger px-1">{apiErrorMessage(history.error, 'The outfit record could not be read.')}</p>`
    (fallback **`The outfit record could not be read.`**);
  - `:186-190` `history.isSuccess && entries.length === 0` →
    `<p className="qt-text-xs qt-text-muted px-1">`**`No outfits recorded yet. The first change of clothes starts the record.`**`</p>`;
  - `:192` `<ul className="space-y-2">`, per entry
    `<li key={entry.index} className="qt-card p-2" data-testid={`outfit-history-entry-${entry.index}`}>`:
    - row `<div className="flex items-center gap-2 mb-1">`:
      `<span className="qt-badge text-xs">#{entry.index}</span>`;
      `<span className="qt-text-xs qt-text-secondary" title={new Date(entry.at).toLocaleString()}>{formatRelativeDate(entry.at, nowMs)}</span>`;
      `<span className="qt-text-xs qt-text-secondary">· {entry.changedBy}</span>`
      (the ONLY actor rendering — `changedBy` verbatim from the server; the
      tab never reads `actorName` / `actorToolName` / `source`);
      `<span className="flex-1" />`;
    - `entry.isCurrent` → `<span className="qt-badge text-xs">`**`Wearing now`**`</span>`;
      else `<button type="button" className="qt-button-secondary qt-button-sm" disabled={rollback.isPending} onClick={() => void wearAgain(entry.index)} title={…}>`**`Wear again`**`</button>`
      with `title` = stale > 0 →
      **`` `${stale} item${stale === 1 ? ' in this outfit is' : 's in this outfit are'} no longer available and will be left off.` ``**
      (→ `1 item in this outfit is no longer available and will be left off.` /
      `2 items in this outfit are no longer available and will be left off.`),
      else **`` `Return ${characterName} to outfit ${entry.index}` ``**.
      `rollback.isPending` disables EVERY Wear again button.
    - `<p className="qt-text-xs mb-1">{entry.summary}</p>`, then `<EntrySlots entry={entry} />`.
- The tab issues NO realtime subscription of its own — the global topic map
  does it (1.B.5).

#### 1.B.2 `components/wardrobe/wardrobe-control-dialog.tsx` (+27 / −4)

- `:56` imports `OutfitHistoryTab`.
- `:72` `type RightTab = 'live' | 'builder' | 'history'`.
- `:500-501` comment: "RIGHT: Live outfit / Outfit Builder / History — Builder
  always present; Live outfit and History only when there is a chat."
- `:526-534` the third tab button, rendered `{isInChat && (…)}` AFTER
  *Outfit Builder*: `<button type="button" onClick={() => setRightTab('history')} className={`qt-tab ${rightTab === 'history' ? 'qt-tab-active' : ''}`}>`**`History`**`</button>`
  (no name suffix, unlike *Live outfit*'s `· {name} in this chat`).
- `:537-546` the body branch comes FIRST: `rightTab === 'history' && isInChat && chatId`
  → `<OutfitHistoryTab chatId={chatId} characterId={selectedCharacterId}
  characterName={selectedCharacter?.name ?? 'this character'}
  hasStagedEdits={live.isDirty(selectedCharacterId)}
  discardStagedEdits={(options) => live.discard(selectedCharacterId, options)}
  requestConfirmation={requestConfirmation} />`; otherwise the existing
  `live` / `builder` arms. The default tab is unchanged (`live` in chat).

#### 1.B.3 `components/wardrobe/hooks/useStagedLiveOutfits.ts` (+52 / −1)

- `isDirty(cid)` (`:198-207`):
  `staged = stagedByChar[cid]`; **no staged entry** → `(pendingRef.current[cid]?.length ?? 0) > 0`;
  **no baseline** → `true`; else `!equippedSlotsEqual(staged, baseline) || (wornBundlesByChar[cid]?.length ?? 0) > 0`.
- `discard(cid, options?)` (`:209-230`):
  - `{ reseed: true }` → ONLY `if (!seededRef.current.has(seedKeyFor(cid))) setReseedTick((t) => t + 1)`
    and return ("Only if nothing has re-seeded it since the staging was dropped").
  - otherwise: `seededRef.current.delete(seedKeyFor(cid))`; `delete pendingRef.current[cid]`;
    `delete baselineByCharRef.current[cid]`; drop `cid` from `stagedByChar` and
    from `wornBundlesByChar`.
- `:94-96` new `const [reseedTick, setReseedTick] = useState(0)`, added to the
  seeding effect's deps (`:118`) so a discarded character re-seeds "even if the
  snapshot has not changed yet".
- `equippedSlotsEqual` (`lib/schemas/wardrobe.types.ts:436-442`): per slot,
  same length and same id at each index (order-sensitive; absent slot = `[]`).

#### 1.B.4 `lib/query/keys.ts` (+4, under `queryKeys.wardrobe`)

```ts
/** Prefix of every history read for a chat — the `chats` realtime topic refetches it. */
histories: (chatId: string) => ['wardrobe', 'history', chatId] as const,
/** One character's outfit timeline in a chat (`?action=wardrobe-history`). */
history: (chatId: string, characterId: string) => ['wardrobe', 'history', chatId, characterId] as const,
```

(inserted after `outfit: (chatId) => ['wardrobe', 'outfit', chatId]`; both sit
under `wardrobe.all = ['wardrobe']`, which the rollback's `onSettled`
invalidates — refreshing the outfit snapshot AND every history together.)

#### 1.B.5 `lib/realtime/topic-map.ts` (+4)

In the row-scoped `chats` case, after `queryKeys.wardrobe.outfit(id)`:
`queryKeys.wardrobe.histories(id),` with the comment "Every character's outfit
timeline in the chat. A tool call or a parent-side replay of the turn's history
settle publishes `chats`, so the History tab stays honest under an open dialog."
Test (`__tests__/unit/realtime/topic-map.test.ts`, +12): the scoped list grows
by `histories('chat-1')` before `scenarioSource`, plus
`it('refreshes every outfit timeline in the chat on the chats topic')` (asserts
`toContainEqual(histories)` AND `history('chat-1','char-1').slice(0,3)` equals
`histories('chat-1')` — the prefix property).

#### 1.B.6 `4ae20977f` client hunks (the whole of them)

- `outfit-history-tab.tsx:43-44` — the `actorToolName: string | null` field
  with its doc comment (above).
- `components/wardrobe/__tests__/outfit-history-tab.test.tsx` — every fixture
  entry gains `actorToolName: null`.
- Nothing else client-side: the new labels arrive as `changedBy` text.

#### 1.B.7 The server wire the tab consumes (owned by the sibling server lane — transcribed for §3)

From `app/api/v1/chats/[id]/actions/outfit.ts` at the pin:

**`GET ?action=wardrobe-history&characterId=…` — `handleGetWardrobeHistory` (`:307-364`)**
1. `:312-313` `characterId = req.nextUrl.searchParams.get('characterId')`;
   falsy (absent or `''`) → **400** `{ "error": "characterId is required" }` —
   BEFORE any chat read.
2. `:315-316` `chats.findById(chatId)` null → **404** `{ "error": "Chat not found" }`.
3. `:318-321` `wardrobeHistory.findEntries(chatId, characterId)` empty →
   **200** `{ "currentIndex": null, "entries": [] }` (pool never loaded).
4. `:323` `loadWearablePool(repos, characterId, undefined, { chatId, operator: true })`.
5. `:324-335` names via `characters.findByIdRaw(id)` (cached; a throw → `null`);
   `selfName = nameOf(characterId) ?? 'the character'`.
6. `:338-353` entries in row order (ascending `entryIndex`), KEY ORDER:
   **`index, at, source, actorCharacterId, actorName, actorToolName, changedBy, isCurrent, summary, slots`**
   — `index: row.entryIndex`, `at: row.createdAt`, `source: row.source`,
   `actorCharacterId: row.actorCharacterId`, `actorName` (only when
   `actorCharacterId`, else `null`), `actorToolName: row.actorToolName`,
   `changedBy: changedByLabel(row, actorName, selfName)`, `isCurrent:
   row.isCurrent`, `summary: summarizeHistorySlots(slots)`, `slots:
   resolveHistorySlots(pool, row.slots)` — slot keys in `top, bottom, footwear,
   accessories, hair` order (`bySlot`), each item `{ itemId, title, archived,
   missing }` (pool hit → `{ itemId, title: item.title, archived:
   Boolean(item.archivedAt), missing: false }`; miss → `{ itemId, title: null,
   archived: false, missing: true }`).
7. `:355` DEBUG **`[Chats v1] Read wardrobe history`** `{ chatId, characterId, entries: entries.length, context: 'wardrobe' }`.
8. `:356-359` **200** `{ currentIndex: rows.find((r) => r.isCurrent)?.entryIndex ?? null, entries }` — key order `currentIndex, entries`.
9. `:360-363` catch → ERROR **`[Chats v1] Error reading wardrobe history`** `{ chatId, characterId }` + err; **500** `{ "error": "Failed to read wardrobe history" }`.

`changedByLabel` (`lib/wardrobe/history/describe-entry.ts:84-118`, at the
`4ae20977f` shape): `operator` → no actor `the operator`; else
`` `the operator, as ${actorName ?? (actor === self ? selfLabel : null)}` `` or
`the operator` when unresolvable; `pascal` → `` `Pascal's ${actorToolName}` `` or
`Pascal`, plus `` ` (rolled by ${actorName})` `` when the actor is someone else
and named; `character` → `selfLabel` (no actor / self) else `actorName ??
'another character'`; `chat-start` → `chat opened`; `participant-added` →
`joined the chat`; `merge` → `merged`; `seeded` → `before history was kept`.
`summarizeHistorySlots` (`:61-73`, the `f46cf40a2` shape): per item `missing`
→ `` `${itemId} (no longer exists)` ``, archived → `` `${title} (archived)` ``,
else `title ?? itemId`; through `describeOutfit`, split on `\n`, strip a leading
`- ` / `* ` and every `**`, trim, drop empties, join `; `.

**`POST ?action=wardrobe-rollback` — `handleWardrobeRollback` (`:371-416`)**
1. `:296-299` body schema `z.object({ characterId: z.string().min(1, 'characterId is required'), index: z.number().int().nonnegative() })`;
   `:378` parsed FIRST (before the chat read). A `ZodError` → **400**
   `{ "error": issues.map((e) => e.message).join(', ') }` — Zod 4's own
   sentences for the type/int/min arms (measure, never transcribe — the
   avatar-rolls precedent); `characterId: ''` → `characterId is required`. A
   non-JSON body throws a `SyntaxError` → the 500 arm below.
2. `:379-380` `chats.findById` null → **404** `{ "error": "Chat not found" }`.
3. `:382-393` pool `{ chatId, operator: true }`; `rollbackWardrobe(repos, { chatId,
   pool, index, context: { userId, chatId, characterId, provenance:
   operatorWardrobeProvenance(chat) }, source: 'ui' }, '[Chats v1] wardrobe-rollback')`.
4. `:394-397` refusal → INFO **`[Chats v1] Wardrobe rollback refused`**
   `{ chatId, characterId, index, reason, context: 'wardrobe' }`; **400**
   `{ "error": "<outcome.message>" }`. The three reachable messages
   (`lib/wardrobe/history/rollback.ts:110-140`; `<Name>` =
   `findByIdRaw(...).name || 'This character'`):
   - `no_history`: **`No outfit history yet for <Name> in this chat.`**
   - `out_of_range`: **`There is no outfit <index>; this chat's outfits are numbered 0 to <max>.`**
   - `already_current`: **`<Name> is already wearing outfit <index>.`**
   (`at_first` — `<Name> is already in the first outfit of this chat (0); there
   is nothing earlier to return to.` — fires only when `index` is undefined,
   which the route's schema forbids: UNREACHABLE here, the tool's arm.)
5. `:399-402` INFO **`[Chats v1] Wardrobe rolled back`** `{ chatId, characterId, previousIndex, restoredIndex, skipped: <count>, context: 'wardrobe' }`.
6. `:403-408` **200** — KEY ORDER **`equippedSlots, previousIndex, restoredIndex, skipped`**;
   `equippedSlots` in `bySlot` order; `skipped` items `{ itemId, title, reason }`
   (`missing` → `title: null`; `archived` → the item title), de-duplicated by
   `itemId`, first-seen in slot order; an archived id ALREADY worn in the same
   slot is kept, not skipped.
7. `:409-415` catch: ZodError → 400 (above); else ERROR
   **`[Chats v1] Error rolling back wardrobe`** `{ chatId }` + err; **500**
   `{ "error": "Failed to roll back the outfit" }`.

Both actions join the dispatch maps: `get.ts:74` `'wardrobe-history'` (after
`outfit-summary`, before `photo-albums`); `post.ts:106` `'wardrobe-rollback'`
(after `equip`, before `toggle-avatar-generation`) — the order the
`availableActions` refusal arrays carry.

#### 1.B.8 Tests that pin the client side

`components/wardrobe/__tests__/outfit-history-tab.test.tsx` (143), `describe('OutfitHistoryTab')`:
1. `lists newest first, marks the current entry, and offers Wear again on the rest`
2. `warns ahead of time when an entry holds items that will be left off` (title contains `1 item in this outfit is no longer available`; a badge titled `Top — archived`)
3. `Wear again posts the rollback for that index` (body `{ characterId: 'char-1', index: 0 }`; no confirm)
4. `confirms and discards staged Live-tab edits before rolling back` (confirm text verbatim; order `['discard','rollback','reseed']`)
5. `does nothing when the operator keeps the staged edits`
6. `says so when nothing is recorded` (`/No outfits recorded yet/`)

Fixture: three entries 0/1/2; entry 1 `Greatcoat` archived in `top`, `changedBy: 'Bertie'`; entry 2 current, `changedBy: 'the operator'`, all slots empty.

`components/wardrobe/__tests__/use-staged-live-outfits-discard.test.tsx` (64), `describe('useStagedLiveOutfits — isDirty and discard')`:
1. `is clean when seeded, dirty after a gesture`
2. `discard drops the staging and re-seeds from the next snapshot`
3. `discard with reseed re-seeds at once when no new snapshot arrived`

---

## 2. The v5 seams (file:line at `fb4a4c264`)

### 2.1 LANE G — server

- **Verb.** Add `Request::CharacterWardrobeImages { character_id: String }`
  beside the P4.D185 block — `crates/quilltap-core/src/api/types.rs:761-790`
  (`CharacterAvatarRollList` at `:766`); a response arm beside `:4520`. If the
  round's keystone freezes `types.rs` / `engine.rs`, record the HANDOFF
  (memory `frozen-engine-seams-and-web-censuses`).
- **Engine arm** — beside `Request::CharacterAvatarRollList`
  (`crates/quilltap-core/src/api/engine.rs:2622-2634`), `self.ready_db()` (a
  pure read; no bytes seam).
- **Service** — new `list_wardrobe_gallery_images(db, character_id)` in the
  characters API family (`crates/quilltap-core/src/api/characters*.rs`, where
  `character_avatar_roll_list` lives) or a new `services/wardrobe_gallery_images.rs`.
  It reuses:
  - `crate::wearable_pool::load_wearable_pool(main, mount, character_id, Some(&[]), opts)`
    (`crates/quilltap-core/src/wearable_pool.rs:372`) — `Some(&[])` is v4's
    explicit empty project tier; default (non-operator) opts; iterate
    `WearablePool::by_id_ordered` (`:21-24` — v4's Map insertion order; the crate
    has no `indexmap`).
  - `wardrobe_image_url` / `wardrobe_image_thumbnail_url`
    (`crates/quilltap-core/src/services/wardrobe_item_images/service.rs:103-110`).
  - A NEW `FilesRepository::find_by_linked_to_any(&[&str])` beside
    `find_by_linked_to` (`crates/quilltap-core/src/db/files.rs:495`): chunks of
    500, the same `find_by_filter_or_empty` fallback + `file_row_issues` drop
    per chunk, first-seen dedupe; SQL shape `… WHERE EXISTS (SELECT 1 FROM
    json_each(files.linkedTo) WHERE value IN (…))` — ROW ORDER MEASURED against
    v4's `findByFilter({$in})` first.
  - A NEW server-side `wardrobe_origin_label(origin)` (no core twin exists —
    `grep "Group · "` over `crates/quilltap-core/src` is empty; the SPA's is
    `apps/web/src/app/wardrobe/wardrobe-container.ts:61-74`).
  - The character read: the vault-overlaid `findById` the avatar-rolls family
    already uses (archived → empty; absent → the `Character not found: <id>`
    error that the edge maps to 404).
- **Web edge** — `crates/quilltap-web/src/characters_routes.rs`: a
  `wardrobe_images_get` beside `avatar_rolls_collection_get` (`:379-401`, with
  its `bad_request("Missing character id")`), mounted in
  `crates/quilltap-web/src/lib.rs` beside `:660-667`
  (`"/api/v1/characters/{id}/wardrobe-images"`, `get(...)`). The 404 sentence
  is `Character not found`; the 500 carries the error's own message.
- **`COLLAPSE_CENSUS`** (`crates/quilltap-harness/tests/web_edge_body_parse_guard.rs:107`):
  `characters_routes.rs` already has rows (`:44`, `:130`, `:223`); a body-less
  GET adds no `and_then(Value::as_` site, so the count should NOT move — the
  lane must re-run the guard and say so (a new routes FILE would need a row).
  The dispatch wrong-type census (`crates/quilltap-web/tests/dispatch_wrong_type_census.rs:671-681`
  is the avatar-rolls precedent) takes one row for the new variant's
  `characterId`.

### 2.2 LANE G — SPA

- **Gallery tab** — `apps/web/src/app/screens/characters/view/tabs/gallery-tab.ts`:
  imports `:29`/`:63`; the Avatar Rolls mount `:233-244` (comment + element). The
  new `<qt-wardrobe-pictures-section [characterId] [entityName] [thumbnailSize]
  (refresh)>` goes IMMEDIATELY AFTER `<qt-avatar-rolls-section …/>` (`:244`)
  and BEFORE the album's detail modal (`:249`), a sibling of the grid (the
  same `mt-8` reason `:233-237` records). `refresh` → the tab's existing
  `onRollRefresh()` (v4 passes the same `onRefresh`).
- **Section component** — NEW `…/view/tabs/wardrobe-pictures-section.ts`
  modelled on `avatar-rolls-section.ts` (518 lines): template `:71-223`
  (section / toggle / description / spinner / grid / modal door), class `:225+`,
  `rollsQuery` `:246-249`, `countLabel` `:258-262`, `handleRollError` `:345-348`,
  `toImageData` `:360-373` (`url: apiUrl(...)` — keep that for Tauri),
  `handleDownload` `:450-467`. The tile copy is an inline template (the
  recorded P4.D188 divergence, `:49-54`): the wardrobe tile = the rolls tile
  minus set-avatar / keep / delete, plus the caption div, with the action
  overlay at `top-1`. Use `[attr.class]` where byte-for-byte class order matters
  (memory `spa-component-harness-gotchas`).
- **API + key** — `apps/web/src/app/screens/characters/characters.api.ts`:
  `characterKeys` `:44-82` gains
  `wardrobeImages: (id: string) => ['characters', 'wardrobe-images', id] as const`
  (THIS file's segment order, the recorded deviation `:58-66,:71-80`); a
  `WardrobeGalleryImage` view type + `toWardrobeGalleryImage` + a
  `fetchWardrobeImages(core, characterId)` beside `fetchAvatarRolls` (`:233-248`,
  `core.dispatchData({ type: 'characterWardrobeImages', characterId })`, BARE
  `{ entries, total }`).
- **Contract** — `apps/web/src/app/core/core-contract.ts`: the request + DTO
  types beside the avatar-roll block (`:1455-1500`) and the union entry beside
  `:2749` — inside a `// === P4.Dxxx: … ===` / `// === end P4.Dxxx ===` fence.
- **Realtime** — NO hunk (v4 adds none). `characterKeys.all` = `['characters']`
  already reaches the new key on the unscoped hint, and the scoped `characters`
  row (`apps/web/src/app/core/realtime-topic-map.ts:112-124`) does NOT list it —
  the same as v4. Pin both in `realtime-topic-map.spec.ts`.
- **Modal** — `apps/web/src/app/images/image-detail-modal.ts` (`image` input
  `:139`, `onPrev`/`onNext` `:141-142`, `closeModal` `:144`, `avatarSet`
  `:146`); `ImageData` `apps/web/src/app/images/images.api.ts:38-51` (no
  `linkId` → the modal's `{ fileId }` save branch, as for rolls). NOT the
  P4.D277 wardrobe picture viewer (`wardrobe/wardrobe-image-viewer.ts`) — v4
  opens the generic detail modal here.
- **Download** — `triggerBlobDownload` (`core/download-utils`), as
  `avatar-rolls-section.ts:450-467`.
- **CSS** — append the three `qt-gallery-caption*` rules to
  `apps/web/src/styles/qt-components/_content.css` (1842 lines; v4 appends after
  its `qt-status-info` rules; v5's equivalent block is at `:1760`, but the file
  now ends with the Almanack rules — append at the layer's end).
- **Help** — `help/character-gallery.md` (repo root) takes v4's +13 —
  route it to the help/docs lane, not LANE G.

### 2.3 LANE H — SPA

- **Dialog** — `apps/web/src/app/wardrobe/wardrobe-control-dialog.ts` (1246 lines):
  - `:72` `type RightTab = 'live' | 'builder'` → add `'history'`.
  - `:289-317` the tab group: add the History button AFTER *Outfit Builder*,
    `@if (isInChat)`; update the `:289-290` comment to v4's.
  - `:319` the body: insert the `@if (rightTab() === 'history' && isInChat && chatId())`
    arm FIRST (v4's precedence), rendering `<qt-outfit-history-tab>`.
  - **No hook layer** — v5 holds v4's `useStagedLiveOutfits` state INLINE:
    `liveStagedByChar` `:545`, `liveBaselineByChar` `:546`, `liveSeededByChar`
    `:547`, `pendingLiveMutators` `:556`, `liveWornBundlesByChar` `:563`; the
    seeding effect `:621-649`; `applyLive` `:812-837`; the flush
    `flushStagedLiveOutfits` `:982-1029`. Add `isLiveDirty(cid)` and
    `discardLive(cid, { reseed? })` here with v4's exact semantics (1.B.3), and
    a `liveReseedTick = signal(0)` READ inside the seeding effect (`:629-630`)
    — v4's `reseedTick` dep: deleting from `liveSeededByChar` alone re-runs
    nothing, because the effect only re-runs when a tracked signal changes.
  - `requestConfirmation`: the dialog's recorded divergence is the synchronous
    `window.confirm` (`:94-95`, `:502-510`); pass
    `(message) => Promise.resolve(window.confirm(message))`.
  - The rollback's `onSettled` twin of `invalidateQueries(wardrobe.all)`: v5's
    lists and the outfit are SIGNAL-held, so it is `void this.data.reload()` +
    `await this.outfit.refreshOutfit()` (the flush's own tail, `:1022-1027`) +
    `queryClient.invalidateQueries({ queryKey: <histories prefix> })`, THEN
    `discardLive(cid, { reseed: true })`.
  - The existing `chats` subscription `:676-680` re-reads the worn snapshot on
    an Aurora announcement — unchanged; the history refetch rides the topic map
    (below).
- **Pure helpers** — `apps/web/src/app/wardrobe/staged-live-outfits.ts`:
  `equippedSlotsEqual` `:128` (v4's twin, already ported) and
  `classifyStagedOutfits` `:178` — `isLiveDirty` must be v4's predicate, NOT
  `classifyStagedOutfits`'s (they differ: v4's `isDirty` is `true` for a staged
  character with no baseline and for a pending-only character).
- **New component** — `apps/web/src/app/wardrobe/outfit-history-tab.ts`
  (`selector: 'qt-outfit-history-tab'`), `injectQuery` for the read,
  `injectMutation` (or an awaited call + a `pending` signal) for the rollback.
  Twins: `formatRelativeDate` `apps/web/src/app/shared/format-date.ts:101`;
  `NowService.now(60_000)` `apps/web/src/app/shared/now.service.ts:177` (v4
  `useNow(60_000)`); `coreErrorMessage` `apps/web/src/app/core/core-client.ts:49`
  (v4 `apiErrorMessage` — a `CoreDispatchError`'s message IS the server
  sentence); `WARDROBE_SLOT_META` `apps/web/src/app/wardrobe/slot-meta.ts:76`;
  `ToastService.showSuccess` / `showError`.
- **Wire calls** — through `dispatchWardrobe` (`apps/web/src/app/wardrobe/wardrobe.api.ts:123`)
  or `core.dispatchData` with the sibling server lane's verb names (§3.1 — the
  contract lane FIXES the names; proposed `chatWardrobeHistory` /
  `chatWardrobeRollback`, following `chatOutfitGet` / `chatEquip`,
  `core-contract.ts:8201-8226`).

### 2.4 The query-keys collision plan (v4's ONE `lib/query/keys.ts` vs v5's per-feature consts)

v4 touches `lib/query/keys.ts` in BOTH commits (`characters.wardrobeImages`;
`wardrobe.histories` / `wardrobe.history`). v5 has no central key module
(`apps/web/src/app/core/realtime-topic-map.ts:9-12`): keys live beside their
feature. So the two lanes land in **different files** and cannot collide:

- **LANE G** → `characterKeys.wardrobeImages` in
  `apps/web/src/app/screens/characters/characters.api.ts:44-82` (owned by G).
- **LANE H** → **recommended:** `chatKeys.wardrobeHistories(chatId) =
  ['chat', chatId, 'wardrobe-history']` and `chatKeys.wardrobeHistory(chatId,
  characterId) = ['chat', chatId, 'wardrobe-history', characterId]` in
  `apps/web/src/app/chat/chat-keys.ts:33-67` (owned by H). They sit under
  `chatKeys.detail(id)` = `['chat', id]`, which the `chats` topic already
  invalidates (`realtime-topic-map.ts:104`) — so **no `realtime-topic-map.ts`
  hunk at all**, exactly the P4.D206 `informs` / P4.D302 `scenarioSource`
  precedent (`chat-keys.ts:45-66`, `realtime-topic-map.ts:95-103`): what v4
  spends a topic-map row on, v5 spends an assertion on
  (`chat-keys.spec.ts` + `realtime-topic-map.spec.ts`, mirroring v4's
  `it('refreshes every outfit timeline in the chat on the chats topic')`
  incl. the prefix property). The unscoped `chats` hint (`chatKeys.all`,
  `['chats']`) does not reach `['chat', …]` — the same as v4, whose unscoped
  row does not reach `['wardrobe', …]`.
  *Alternative (if the order prefers v4's spelling):* a new
  `apps/web/src/app/wardrobe/wardrobe-keys.ts` with `['wardrobe','history',…]`
  plus one `realtime-topic-map.ts` row — still H-only, still no collision; but
  it adds a topic-map row, a new `ALL_REALTIME_PREFIXES` question, and a key
  family nothing else in the signal-held dialog uses.
- **`core-contract.ts`** is the only file both lanes write: each lane its own
  fenced region (`// === P4.Dxxx: … (append-only) ===` / `// === end P4.Dxxx ===`)
  in BOTH places — the type block and the `CoreRequest` union (the P4.D277 /
  P4.D285 / P4.D309 pattern, `core-contract.ts:4524-4691`, `:6633-6647`). LANE H's
  types belong beside `ChatEquipRequest` (`:8224-8244`); LANE G's beside the
  avatar-roll types (`:1455-1500`). The sibling SERVER lane owns the chat verbs'
  Rust variants; whichever lane writes the TS request types must take the
  names from the Shared contract verbatim (§3.1).

---

## 3. Shared contracts — VERBATIM

### 3.1 The two chat wires LANE H consumes (from the sibling server lane)

**Read — `GET /api/v1/chats/{chatId}?action=wardrobe-history&characterId={characterId}`**
(proposed dispatch: `{ "type": "chatWardrobeHistory", "chatId": "…", "characterId": "…" }`;
`characterId` carried RAW / optional so the core answers v4's 400 itself)

```jsonc
// 200 — key order is contractual
{
  "currentIndex": 2,                      // number | null
  "entries": [                            // ASCENDING by index
    {
      "index": 0,
      "at": "2026-10-16T10:00:00.000Z",   // the row's createdAt
      "source": "chat-start",             // chat-start | participant-added | merge | operator | character | pascal | seeded
      "actorCharacterId": null,           // string | null
      "actorName": null,                  // string | null (resolved only when actorCharacterId)
      "actorToolName": null,              // string | null (pascal entries)
      "changedBy": "chat opened",         // the server's label (1.B.7)
      "isCurrent": false,
      "summary": "Green Dress; Riding Boots",
      "slots": {
        "top": [{ "itemId": "…", "title": "Green Dress", "archived": false, "missing": false }],
        "bottom": [], "footwear": [], "accessories": [], "hair": []
      }
    }
  ]
}
// 200 empty record:      { "currentIndex": null, "entries": [] }
// 400 (before any read): { "error": "characterId is required" }
// 404:                   { "error": "Chat not found" }
// 500:                   { "error": "Failed to read wardrobe history" }
```

**Write — `POST /api/v1/chats/{chatId}?action=wardrobe-rollback`**, body
`{ "characterId": "…", "index": 3 }` (proposed dispatch:
`{ "type": "chatWardrobeRollback", "chatId": "…", "characterId": …, "index": … }`,
the body flattened RAW — the `ChatEquip` idiom, `types.rs:2672-2676` — so the
core's Zod-faithful parse sees what v4's did)

```jsonc
// 200 — key order is contractual
{
  "equippedSlots": { "top": [], "bottom": [], "footwear": [], "accessories": [], "hair": [] },
  "previousIndex": 2,
  "restoredIndex": 0,
  "skipped": [{ "itemId": "…", "title": "Greatcoat", "reason": "archived" }]   // reason: archived | missing; title null for missing
}
// 400 Zod:     { "error": "<Zod 4 issue messages joined with ', '>" }   (measured, never transcribed; characterId '' → "characterId is required")
// 400 refusal: { "error": "No outfit history yet for <Name> in this chat." }
//              { "error": "There is no outfit <index>; this chat's outfits are numbered 0 to <max>." }
//              { "error": "<Name> is already wearing outfit <index>." }
// 404:         { "error": "Chat not found" }   (AFTER the body parse)
// 500:         { "error": "Failed to roll back the outfit" }
```

Side effects the SPA relies on: one commit through the wear chokepoint, the
avatar refresh, Aurora's announcement (→ a `chats` realtime hint), the cursor
moved to `index` (nothing appended), ledger source `ui`, operator provenance.

SPA-side TS (LANE H's fenced contract region), mirroring v4's tab types:
`OutfitHistorySlotItem`, `OutfitHistoryEntry` (incl. `actorToolName`),
`OutfitHistoryResponse`, `WardrobeRollbackResponse` (v4's `RollbackResponse`),
`ChatWardrobeHistoryRequest`, `ChatWardrobeRollbackRequest`.

### 3.2 The gallery wire (LANE G, both halves — written anyway)

**`GET /api/v1/characters/{id}/wardrobe-images`** (proposed dispatch:
`{ "type": "characterWardrobeImages", "characterId": "…" }`; no query params)

```jsonc
// 200 — BARE (no data wrapper); key order is contractual
{
  "entries": [                            // createdAt string DESC; ties keep file-read order
    {
      "fileId": "…",
      "fileName": "…",                    // files.originalFilename
      "url": "/api/v1/files/<fileId>",
      "thumbnailUrl": "/api/v1/files/<fileId>?action=thumbnail",
      "mimeType": "image/webp",           // string | null
      "fileSizeBytes": 1234,
      "width": 512,                       // number | null
      "height": 512,                      // number | null
      "createdAt": "2026-10-03T00:00:00.000Z",
      "generationPrompt": "…",            // string | null
      "generationModel": "…",             // string | null
      "itemId": "…",
      "itemTitle": "Guild Sash",
      "scope": "group",                   // character | group | general (never project here)
      "locationLabel": "Group · The Guild", // "Own wardrobe" | "Group · <name>" | "Shared · Quilltap General"
      "itemArchived": true,
      "isCurrentItemImage": false
    }
  ],
  "total": 1                              // = entries.length
}
// archived character: { "entries": [], "total": 0 }
// 400: { "error": "Missing character id" }
// 404: { "error": "Character not found" }
// 500: { "error": "<the thrown error's message>" }   (+ ERROR "[Characters/WardrobeImages v1] Error listing wardrobe pictures")
```

Lines: DEBUG `[Characters/WardrobeImages v1] Listed wardrobe pictures`
`{ userId, characterId, total }` (edge); DEBUG `[WardrobeGallery] Listed wardrobe
pictures` `{ context, characterId, itemCount, linkedFileCount, returned }` and
`[WardrobeGallery] Archived character — no wardrobe pictures` `{ context,
characterId }` (service).

---

## 4. Verification plan

### 4.1 LANE G — the route (harness)

- **Closest recipe:** the item-images ROUTES family —
  `crates/quilltap-web/tests/wardrobe_item_images_routes_equivalence.rs` over
  `harness/oracle/cases/wardrobe-item-images-routes.test.ts` (v4's REAL route
  handlers under jest, the real cipher binding, per-request fresh module graph +
  fixture copy, per-request `sql` / `mountSql` plants, bodies compared as BYTES —
  `serde_json` map equality is order-blind, memory
  `serde-json-map-eq-ignores-order`). Its fixture
  (`harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts` +
  `wardrobe-item-images-tier2.json`) already has what this route needs: a live
  character, an ARCHIVED character, a stranger, items in all four tiers
  (`coat`/`scarf`/`hat`/`bare` own, `apron` general, `smock` PROJECT — the
  exclusion row, `sash` group, `cloak`), and GENERATED / UPLOADED / IMPORTED
  pictures plus a DOCUMENT `files` row linked to an item, timestamps pinned one
  second apart. Build a NEW `wardrobe_images_routes_equivalence.rs` +
  `wardrobe-images-routes.test.ts` + `wardrobe-images-routes.json` corpus over
  the SAME builder, into the family's OWN `/tmp/qt-wgi-*` paths (the
  `f5e953a3f` shared-`/tmp` re-mint catch, memory `parallel-lane-shared-resources`),
  importing v4's `app/api/v1/characters/[id]/wardrobe-images/route.ts` GET.
  Cases: the live character (generated across own / group / general; uploaded,
  imported and document rows dropped; project item excluded; `isCurrentItemImage`
  both ways; an archived ITEM flagged); the archived character (empty, and no
  pool read — capture the DEBUG line); an absent character (404); a character
  with an empty pool (no file read); a planted `createdAt` TIE (file-read order);
  a file linked to TWO pool items (first `linkedTo` wins) and to a non-pool id
  first; a planted row failing `FileEntrySchema` (dropped with v4's two lines); a
  `>500`-item chunk boundary is optional (measure the dedupe with a planted
  duplicate link instead). Capture the log lines (memory
  `v4-oracle-job-host-and-log-capture`).
- **Service tier-1/2:** a unit corpus transcribing v4's four
  `gallery-images.test.ts` cases (§1.A.10) against the Rust service.
- **`find_by_linked_to_any` order:** a real-DB differential of v4's
  `findByLinkedToAny` vs v5's over the same fixture (row order + dedupe) before
  the route family relies on it.

### 4.2 LANE G — SPA specs

`apps/web/src/app/screens/characters/view/tabs/wardrobe-pictures-section.spec.ts`
(model: `avatar-rolls-section.spec.ts`; `retry: false` QueryClient):
- `renders nothing at all when the character can reach no wardrobe pictures`
- `shows the Wardrobe Pictures heading, the count (… while loading, 1 picture / N pictures) and the description, collapsed by default`
- `expands to a captioned grid: the item title (with " (archived)") over the tier label, the hover actions at the top corner`
- `offers ONLY Download — no set-as-avatar, keep or delete button`
- `the caption's tooltip reads "<title> — <tier>"`
- `the tile shows the thumbnail; the detail modal and the download use the full-size URL`
- `a click opens the shared detail modal with no linkId; Prev/Next walk the list`
- `a failed download toasts "Failed to download image"`
- `a tile whose image fails to load shows the placeholder and drops Download`

`gallery-tab.spec.ts` — `mounts the Wardrobe Pictures section after Avatar Rolls`
(its stub `:44-60` answers `{}` for unknown verbs → empty → hidden, so existing
cases stay green; add a `wardrobeImages` option). `characters.api.spec.ts` —
the verb and the `toWardrobeGalleryImage` mapping. `realtime-topic-map.spec.ts`
— the unscoped `characters` hint reaches `characterKeys.wardrobeImages(id)`; the
scoped one does not (v4's shape).

### 4.3 LANE G — Playwright

NEW `apps/web/e2e/character-wardrobe-pictures-flow.spec.ts` cloned from
`character-avatar-rolls-flow.spec.ts` (own server + instance over the committed
`characters-*` pair, user-id rewrite, spec-private port, no LLM). Beat:
`the Wardrobe Pictures shelf: hidden with none; expands to captioned plates; Download; the detail modal`.
No gate flag is needed (both halves are this lane's) — but if the order
splits server/SPA across a unify boundary, gate it behind
`const P4Dxxx_SERVER_LANDED`. §5 has the seed.

### 4.4 LANE H — SPA specs

`apps/web/src/app/wardrobe/outfit-history-tab.spec.ts` — transcribe v4's six
cases name for name (memory `react-closure-reads-in-signal-ports`: v4's
interaction tests catch ordering bugs), plus the strings v4's test does not
assert:
1. `lists newest first, marks the current entry, and offers Wear again on the rest`
2. `warns ahead of time when an entry holds items that will be left off`
3. `Wear again posts the rollback for that index`
4. `confirms and discards staged Live-tab edits before rolling back` (order `['discard','rollback','reseed']`, confirm text verbatim)
5. `does nothing when the operator keeps the staged edits`
6. `says so when nothing is recorded`
7. (v5-added) `renders changedBy verbatim, the relative time with the full date as its title, and the summary`
8. (v5-added) `toasts "Back in outfit N" and the skipped-item sentence, singular and plural`
9. (v5-added) `a refused rollback toasts the server's sentence and still re-seeds`
10. (v5-added) `a stale badge reads "<title> · gone" / "· archived" with the slot-label title; an empty entry reads "Nothing at all."`
11. (v5-added) `every Wear again is disabled while a rollback is in flight`
12. (v5-added) `a read failure shows the server's sentence`

Dialog (`wardrobe-control-dialog.spec.ts`, which already provides TanStack +
`coreStreamStub`, `:111-125`) — transcribe v4's three
`use-staged-live-outfits-discard` cases against the inline staging, and add:
- `the History tab exists only in chat, after Outfit Builder` (and the asTab /
  chat-less arm at `:620` stays History-free)
- `isDirty: clean when seeded, dirty after a gesture` (v4 case 1)
- `discard drops the staging and re-seeds from the next snapshot` (v4 case 2)
- `discard with reseed re-seeds at once when no new snapshot arrived` (v4 case 3)
- `Wear again with staged edits: confirm → discard → rollback → reseed; Done then flushes NOTHING`
- `after a rollback the Live tab shows the restored outfit` (the §8 Q3 question — measure it)

`chat-keys.spec.ts` / `realtime-topic-map.spec.ts` — the `chats` hint for chat
`c` reaches `wardrobeHistory(c, x)` through `chatKeys.detail(c)` (v4's
`refreshes every outfit timeline in the chat on the chats topic`, prefix
property included).

### 4.5 LANE H — Playwright

In `apps/web/e2e/wardrobe-flow.spec.ts` (characters pair, Aria + "Solo Voyage",
no LLM — `:36-40`), a new beat after `in chat: staging + the one-shot set_all flush
persists the outfit` (`:1117`), gated on the SERVER lane's flag
(`const P4Dxxx_SERVER_LANDED = false;` beside `:87-117`, flipped at unify —
the house convention):
`in chat: History lists the operator's change; Wear again returns to the earlier outfit and Aurora says so`
— open the dialog in Solo Voyage → Live → Wear a garment → Done (operator
change, recorded) → reopen → **History** → the current entry reads
*Wearing now* and `· the operator…` → *Wear again* on the earlier entry → toast
`Back in outfit N` → that entry now *Wearing now*; then
`… with staged Live edits, Wear again confirms and discards them` (accept the
native confirm with `page.once('dialog', d => d.accept())`). Assert rendered
text, not `waitForResponse` (memory `spa-component-harness-gotchas`, the
P4.D300 cache trap). Keep the describe non-serial.

---

## 5. Fixtures

- **LANE H e2e:** NO plant needed if the server lane's settle path is live —
  the operator Done flush records the history (first touch seeds entry 0 + the
  change). If the round's boot seed (`seed-chat-wardrobe-history-v1`) gives the
  fixture chat a `seeded` entry first, assert RELATIVE positions, never fixed
  indices. If a plant is preferred, rows must go in AFTER the boot ensure creates
  `chat_wardrobe_history` (the table does not exist in the committed pair —
  `quilltap db --write` before boot cannot insert into it): boot, stop, plant,
  reboot. Memory `own-instance-e2e-fixture-choice`: the `salon` pair + a row
  copy is the fallback if the characters pair's chat cannot carry it; prefer
  `target/release/quilltap` for CLI plants (and delete stale cloned release bins
  in a lane, memory `lane-spa-env-node-modules-and-release-bins`).
- **LANE G e2e:** the characters pair has NO generated wardrobe picture, and no
  UI path makes one without a provider (an editor upload is `UPLOADED` —
  excluded). Two options for the lane to measure:
  (a) the avatar-rolls plant recipe (`character-avatar-rolls-flow.spec.ts:36-110`
  — a `files` row + `doc_mount_blobs` + a `doc_mount_file_links` row planted
  with the CLI BEFORE boot), extended with a wardrobe item in Aria's vault
  (`Wardrobe/<title>.md` document + link) and `files.linkedTo = ["<itemId>"]`,
  `category = 'IMAGE'`, `source = 'GENERATED'`, `generationPrompt`; or
  (b) boot, create an item and upload a picture through the REST edges, stop,
  flip that row's `source` to `GENERATED` with the CLI, reboot. Plant a second,
  OLDER plate on a group or General item to see two captions and the order.
- **LANE G harness:** the existing item-images builder (§4.1), extended only by
  per-case `sql` plants — no builder rebuild (memory
  `rebuilt-fixture-pair-vintage`).

---

## 6. Traps

1. **Zoneless signals + React closures.** v4's `wearAgain` reads
   `hasStagedEdits` from the render; in v5 read `isLiveDirty(cid)` ONCE into a
   local before any `.set()` (memory `react-closure-reads-in-signal-ports`).
   The reseed MUST be a signal the seeding effect reads — deleting from
   `liveSeededByChar` (a plain `Set`) re-runs nothing.
2. **TanStack `data()` lags an awaited invalidate by a macrotask** (memory
   `tanstack-angular-data-lags-invalidate`). Anything that acts on the refetched
   history in the same tick reads `queryClient.getQueryData(...)`. The reseed
   itself reads the OUTFIT store (signal-held, awaited `refreshOutfit()`), not
   TanStack — keep it that way.
3. **The P4.D277 save-dialog seeding finding** (the order's Unification
   paragraph): a default chosen inside a `queryFn` is skipped on a cache-primed
   second open. The gallery's detail modal → *Save to my gallery* opens that
   same shared dialog — the seed is now an effect over the query's data; do not
   re-introduce a `queryFn` seed in the section, and walk the save twice in the
   e2e beat if it touches it.
4. **`[class]` re-sorts tokens** — use `[attr.class]` for v4's badge / caption /
   overlay strings where byte order is pinned (memory
   `spa-component-harness-gotchas`).
5. **Default TanStack retry** hides the read-failure arm; specs use
   `retry: false`.
6. **Dispatch errors carry no HTTP status** — a 404 vs 400 vs 500 is
   indistinguishable to the SPA; every arm surfaces the message
   (`coreErrorMessage`), which is all v4's tab shows anyway.
7. **`window.confirm` is synchronous** in the v5 dialog; `requestConfirmation`
   returns a resolved Promise. In Playwright, register the dialog handler
   BEFORE the click.
8. **A spec that defines `navigator.clipboard`** must add `writable: true` (the
   detail modal's Copy) — memory `spa-component-harness-gotchas`.
9. **Gallery `isPending` vs `isLoading`** — v4 hides the section when
   `!isLoading && total === 0`; on an error `isLoading` is false, so the shelf
   VANISHES on a failed read (no error text). The rolls section uses
   `isPending()` — on a TanStack error `isPending` is false too; keep the same
   hide-on-error behaviour and pin it.
10. **The dialog's chat-less tab** (`asTab`, chatId null) must never show
    History (v4 `isInChat &&`) — `wardrobe-control-dialog.spec.ts:620` pins the
    Live-free twin.
11. **`core-contract.ts` is shared** — fenced regions, append-only, and the
    names taken from the Shared contract; a union entry outside a fence will
    conflict at unify.

---

## 7. v4 tests to mirror (`it` names, verbatim)

- `lib/wardrobe/__tests__/gallery-images.test.ts` — `captions generated pictures across tiers, newest first`; `returns nothing for an archived character without reading the pool`; `throws for a missing character`; `skips the file lookup when the pool is empty`.
- `components/wardrobe/__tests__/outfit-history-tab.test.tsx` — the six in §1.B.8.
- `components/wardrobe/__tests__/use-staged-live-outfits-discard.test.tsx` — `is clean when seeded, dirty after a gesture`; `discard drops the staging and re-seeds from the next snapshot`; `discard with reseed re-seeds at once when no new snapshot arrived`.
- `__tests__/unit/realtime/topic-map.test.ts` — `refreshes every outfit timeline in the chat on the chats topic` (+ the scoped list's new member).
- Server, for the sibling lane (context only): `__tests__/unit/app/api/v1/chats/[id]/actions/wardrobe-history.test.ts` — `lists entries with labels, the cursor, and stale items marked`; `answers an empty record without loading the pool`; `requires characterId`; `rolls back as an operator change: one commit, a cursor move, skipped items reported`; `answers a refusal with 400 and changes nothing`; `requires an index`; and `4ae20977f`'s `outfit.test.ts` addition `records the operator as the character the user is speaking as`.

---

## 8. Open questions / ledger disagreements

1. **Ledger `1cfc9b504` row — accurate, three refinements.** (a) The 404 is any
   thrown Error whose message STARTS WITH `Character not found`, not only the
   service's throw. (b) `FilesRepository.findByLinkedToAny`'s
   `Error finding files linked to entities` wrap is UNREACHABLE through v4's
   real code (`findByFilter` is a fallback read) — port it as v5's
   `find_by_linked_to` records its twin, not as a live line. (c) The help
   page's "click a plate for the enlarged view, with its generation prompt" is
   NOT what the code does — the section passes no prompt into `ImageDetailModal`
   (v4 `ImageData` has no prompt field). Port the code; carry the help text
   as-is (the help lane).
2. **Ledger `8d8618819` row — accurate.** One addition: the 400
   `characterId is required` precedes the chat read, and the rollback's body
   parse precedes its chat read (a bad body on a missing chat answers 400). The
   row says the ledger source is `ui` — confirmed (`outfit.ts:390`).
3. **Possible v4 defect, to MEASURE (not assert):** with NO staged edits,
   `wearAgain` skips `discardStagedEdits()`, and `onSettled`'s
   `discardStagedEdits({ reseed: true })` is a no-op while the character is
   already seeded (`useStagedLiveOutfits.ts:211-213`). The seeding effect never
   re-seeds a seeded character (bug 61's contract). So after a clean Wear again
   the Live tab's `displaySlots` = the STALE pre-rollback staging, and a later
   Live gesture stages onto the old outfit and Done's `set_all` would UNDO the
   rollback. Measure it in v4 (a jsdom dialog test or the browser) before the
   lane ports it; if confirmed, ask the human for a ruling (port v4's behaviour
   + file the bug, or fix in v5 by discarding unconditionally) — do not decide
   in the lane.
4. **Verb names** — `chatWardrobeHistory` / `chatWardrobeRollback` /
   `characterWardrobeImages` are PROPOSALS; the server survey / order fixes the
   two chat names, and the Shared contract must state them once.
5. **Key spelling** — the recommended `['chat', id, 'wardrobe-history', …]`
   placement (§2.4) is a recorded deviation from v4's `['wardrobe','history',…]`
   (only DISTINCTNESS and the `chats`-topic reach are contractual — the
   `characterKeys` file's own rule). The rollback must then invalidate BOTH the
   history prefix and re-read the signal-held outfit/lists, since no single
   `wardrobe.all` prefix spans them in v5.
6. **`isCurrentItemImage` / `generationPrompt` / `generationModel` / `scope`**
   ride the wire and are never rendered by v4's client — carry them on the DTO;
   render nothing extra.
7. **Help + docs** (`help/character-gallery.md` +13, `help/wardrobe.md` from the
   programme, `docs/v4/developer/API.md` hunks) belong to the round's help/docs
   lane, not to G or H.
