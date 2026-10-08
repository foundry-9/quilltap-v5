# Survey — wardrobe item images (`7c8572869`) + tool-drawn pictures (`b3f937076`), SERVER side (2026-10-08)

Read-only planning survey for the next round's work orders. v4 read at HEAD
`f5e953a3f` (hunks via `git show <sha> -- <path>`, post-commit files via
`git show HEAD:<path>`); v5 read on `main` (`4895d1200`). Scope: the server
surfaces of the two commits. **Out of scope here** (other surveys): the
export/import/backup/restore carriers (`lib/export/*`, `lib/import/*`,
`lib/backup/*`, `qtap-export.schema.json`) and every SPA component
(`components/**`, `hooks/**`, `lib/wardrobe/item-images-client.ts`,
`lib/query/keys.ts`).

Both commits are in the drift ledger's §3 as UNPROCESSED (`drift-ledger.md:192`,
`:194`); this survey supersedes those two rows' prose where they differ (see
§P below).

**Conventions.** v4 paths are relative to `~/source/quilltap-server`; v5 paths
to `crates/quilltap-core/src` unless prefixed `quilltap-web/`, `quilltap-host/`,
`harness/`. "v4 L" = post-commit HEAD line.

---

## 0. Commit shapes (from `--stat`, not the prose)

- `7c8572869` "Wardrobe item images: pictures of garments and outfits (#82)":
  86 files, +5940/−88. Server-side NEW: `app/api/v1/wardrobe/[itemId]/images/route.ts`
  (280), `lib/wardrobe/item-images.ts` (552), `lib/wardrobe/item-image-prompt.ts`
  (139), `lib/wardrobe/item-image-generation.ts` (328),
  `lib/file-storage/wardrobe-image-bridge.ts` (199 then; 211 at HEAD),
  `migrations/scripts/add-wardrobe-image-settings-field-v1.ts` (88),
  `help/wardrobe-images.md` (37). Server-side CHANGED: listed per area below.
- `b3f937076` "Wardrobe tools draw item pictures; listings expose picture ids;
  avatar picker generates portrait": 55 files, +15735/−5838 — of which
  **8 `plugins/dist/*/index.js` bundles = +14548/−5740** (`git show --stat
  b3f937076 -- plugins/src` is EMPTY: pure rebuild, no plugin source change);
  `package-lock.json` +70/−? = the version bump `4.10.0-dev.114 → .115` plus
  NEW optional `@tailwindcss/oxide-wasm32-wasi/node_modules/{@emnapi/*,
  @napi-rs/wasm-runtime, @tybys/wasm-util, tslib}` entries (a lockfile
  refresh, not a direct dep). Server-side NEW: `lib/wardrobe/tool-image-generation.ts`
  (146), `lib/background-jobs/handlers/wardrobe-item-image.ts` (119).

---

## 1. `/api/v1/wardrobe/[itemId]/images` (NEW route)

### v4 at HEAD — `app/api/v1/wardrobe/[itemId]/images/route.ts`

- Header L1-21: one route for all four tiers; container rides in the query.
  `GET …?scope=&id=` → `{ current, images: [{fileId,url,thumbnailUrl,source,createdAt,prompt?,model?}] }`;
  `POST ?action=generate|upload|set-current|delete-image`.
- `containerQuerySchema` L60-65: `scope: z.enum(['character','project','group','general'])`,
  `id: z.string().min(1).optional()`, refine `scope==='general' || !!id` with
  message `'id is required for this scope'`. `readContainerQuery` L79-89 joins
  issue messages with `'; '` → 400 (L101).
- `generateBodySchema` L67-69 `{ imageProfileId: z.string().min(1).nullable().optional() }`;
  `fileIdBodySchema` L71-73 `{ fileId: z.string().min(1, 'fileId is required') }`;
  `uploadKindSchema` L75 `z.enum(['uploaded','imported']).default('uploaded')`.
- `findHome` L95-106: 400 (joined query issues) → `resolveWardrobeItemHome`
  → `notFound('Wardrobe item')` (= `{error:'Wardrobe item not found'}`, 404).
- `mapWriteError` L109-121 (every write action): `CharacterArchivedError` →
  `logger.info('[Wardrobe Images v1] Refused a picture write on an archived character', meta)`
  + `conflict('This character is archived; their wardrobe cannot be changed')` (409);
  `ForeignWardrobeImageError` → `badRequest('That picture does not belong to this wardrobe item')`;
  `ZodError` → 400 joined issues; else rethrow (middleware 500).
- GET `handleList` L124-141: `listWardrobeItemImages` (IMAGE files
  `linkedTo` item, newest first); `current` = `item.imageFileId` ONLY if it is
  in the list, else `null` (L130-132); `logger.debug('… Listed wardrobe item images', {itemId, scope, count, current})`;
  `successResponse({ current, images: images.map(toWardrobeImageSummary) })`.
- `generate` L144-199: body read as TEXT (L149); empty/whitespace → `{}`;
  bad JSON → `badRequest('Invalid JSON body')` (L155); schema failure → joined
  400. `trackActivity('image', () => generateWardrobeItemImage(repos, {userId, home, containerId: query.id ?? null, imageProfileId: body.imageProfileId ?? null}))`
  (L164-171). Then `repos.files.findById(result.fileId)` and
  `created({ image: file ? toWardrobeImageSummary(file) : {fileId, url}, current: result.fileId, prompt, subject, profile, rerouted, trail })`
  — **201** (L173-181). Errors: `NoWardrobeImageProfileError` → `badRequest(error.message)`
  (L183-185, text = `NO_WARDROBE_IMAGE_PROFILE_MESSAGE`, see §3);
  `WardrobeImageGenerationError` → `errorResponse(refused ? 'The image provider declined to draw this garment' : \`Image generation failed: ${error.message}\`, refused ? 422 : 502, { trail, refused })`
  (L186-196) — `details` key carries `{trail, refused}` (responses.ts:36-45);
  else `mapWriteError`.
- `upload` L202-238: `req.formData()` throw → `badRequest('Expected a multipart upload')`;
  `form.get('file')` not a `File` → `badRequest('No file provided')`;
  `validateImageFile(file)` (images-v2.ts:218-226: `Invalid file type. Allowed types: image/jpeg, image/jpg, image/png, image/gif, image/webp, image/avif, image/svg+xml`;
  `File size exceeds maximum allowed size of 10 MB`) → 400 with the thrown
  message; `kind` parse failure → `badRequest('kind must be "uploaded" or "imported"')`.
  `convertToWebP(Buffer, file.type, file.name)` → `addWardrobeItemImage(repos, home, {userId, kind, content, contentType, width ?? null, height ?? null})`;
  `logger.info('… Uploaded wardrobe item image', {…meta, fileId, bytes})`;
  `created({ image, current: stored.id })` (201).
- `set-current` L241-253: `fileIdBodySchema.parse(await req.json())` (a
  ZodError → 400 via mapWriteError); `setCurrentWardrobeItemImage`;
  `logger.info('… Set current wardrobe item image', {…meta, fileId})`;
  `successResponse({ current })`.
- `delete-image` L256-268: same body; `deleteWardrobeItemImage` →
  `logger.info('… Deleted wardrobe item image', {…meta, fileId, current})`;
  `successResponse({ current })`.
- Exports L270-280: `GET = createContextParamsHandler(handleList)`;
  `POST = withActionDispatch({...})` with NO default → bare POST is
  `{ error: 'Action parameter required', availableActions }` 400
  (`lib/api/middleware/actions.ts:108-116`); unknown action →
  `{ error: \`Unknown action: ${action}\`, availableActions }` 400 (actions.ts:88-98).
- `LOG_TAG = '[Wardrobe Images v1]'` (L58).

### What `7c8572869` did NOT do here
- No per-tier route families gained an images arm (`characters/[id]/wardrobe/[itemId]/route.ts`,
  `projects/…`, `groups/…`, `wardrobe/[itemId]/route.ts` are untouched for
  images — only PUT/DELETE changed, §6). No `GET` for a single image (served by
  `/api/v1/files/[id]`). No DELETE verb (delete is `?action=delete-image`).
  No pagination. No `scope=group` reading through a character.

### v5 main
- No counterpart. The web router (`quilltap-web/src/lib.rs:474-494`) has
  `/api/v1/wardrobe`, `/transfers`, `/preview-avatar`, `/analyze-image`,
  `/{itemId}` (GET/PUT/DELETE) only; `quilltap-web/src/wardrobe_routes.rs:190-227`
  dispatches `CoreRequest::{WardrobeItemGet, WardrobeUpdate, WardrobeDelete}`.
  `CoreRequest` wardrobe verbs: `api/types.rs:662-709` (character tier),
  `:938-976` (group), `:1558-1596` (project), `:2713-2771` (global +
  transfers + preview + analyze). An images verb family is NEW.
- Multipart precedent: `quilltap-web/src/multipart.rs` (`FormData::from_request`,
  `file()`/`text()`; header L1-12 names images-v2 as a deferral the helper was
  built for); the upload leg pattern is `quilltap-web/src/images_routes.rs:185-215`.
- `validate_image_file` already ported: `api/images.rs:705-720` over
  `services::file_storage::{ALLOWED_IMAGE_TYPES, MAX_IMAGE_FILE_SIZE}` (private
  fn — would need `pub(crate)`).
- Activity span: `services/activity_registry.rs:196 track_activity(ActivityKind::Image, fut)`
  (re-entrant by kind, L187-194).
- Response helpers in the wardrobe api: `api/wardrobe.rs:69-83` (`internal`,
  `bad_request`, `not_found` → `'<resource> not found'`). No `conflict` helper
  there (grep `409`/`conflict` in `api/wardrobe.rs`: none).

### Harness
- `quilltap-web/tests/wardrobe_routes_equivalence.rs` (corpus
  `harness/oracle/fixtures/wardrobe-routes.json#cases`, oracle
  `harness/oracle/cases/wardrobe-routes.test.ts`): the archetype tier +
  transfers + equip + preview; **no images cases** (new family or new cases).
  Recipe: jest real-DB, `QT_ORACLE_OUT=/tmp/oracle-wardrobe-routes.ndjson npx jest -- wardrobe-routes`
  (see file header).
- `images_generate_route_equivalence.rs` (P4.76) is the tier-3 precedent for
  a route whose provider + classifier are mocked BELOW v4's real handler
  (header L1-30): provider calls `{provider, apiKey, params}`, status+body
  key order, post-mutation `files` rows + mount links.

---

## 2. `lib/wardrobe/item-images.ts` (NEW, 552 lines) — every export

### v4 at HEAD
- L49-54 `class ForeignWardrobeImageError` — message
  `\`File ${fileId} is not an image of wardrobe item ${itemId}\``.
- L61-76 `interface WardrobeItemHome { scope, characterId, item, containerItems, resolveMount(), update(patch) }`.
- L82-99 `resolveContainerMountPointId(scope, characterId, mountPointId)`:
  project/group → `mountPointId` or throw `\`No store mount resolved for ${scope} wardrobe\``;
  general → `getGeneralMountPointId()` or throw `'Quilltap General is not provisioned'`;
  character → `resolveWardrobeMount(characterId)` (THROWS `CharacterArchivedError`
  for a tombstone — the point) or throw `\`Character ${characterId} has no linked vault\``.
- L106-136 `resolveWardrobeItemHome(repos, userId, scope, containerId, itemId)`:
  `resolveWardrobeContainer` (`lib/wardrobe/resolve-container.ts:40`, since
  `0506517d3`) → `container.readItems()` → find by id AND (`scope !== 'character' || i.characterId === container.characterId`)
  (a General archetype merged into a character read is NOT in the character's
  own wardrobe, L104-105). `update`: project/group → `updateProjectWardrobeItem(mountPointId, itemId, patch)`;
  else `repos.wardrobe.update(itemId, patch, scope==='character' ? characterId : null)`.
- L143-171 `WardrobeItemImageSummary`, `wardrobeImageUrl` = `/api/v1/files/${fileId}`,
  `wardrobeImageThumbnailUrl` = `/api/v1/files/${fileId}?action=thumbnail`,
  `toWardrobeImageSummary` (adds `prompt` iff `generationPrompt`, `model` iff
  `generationModel` — key spread, so key ORDER is fileId,url,thumbnailUrl,source,createdAt,[prompt],[model]).
- L174-182 `listWardrobeItemImages(repos, itemId)`: `files.findByLinkedTo(itemId)`
  → filter `category==='IMAGE'` → sort `createdAt` DESC (string compare).
- L189-199 `assertItemImageChoice(repos, itemId, fileId)`: `undefined`/`null`
  pass; else must be in the list or throw `ForeignWardrobeImageError`.
- L205-209 `SOURCE_BY_KIND`: generated→`GENERATED`, uploaded→`UPLOADED`, imported→`IMPORTED`.
- L228-281 `addWardrobeItemImage(repos, home, input)`: `home.resolveMount()`
  FIRST; `description = \`Wardrobe image for “${home.item.title}”\`` (curly
  quotes); `writeWardrobeItemImage({mountPointId, itemId, kind, content, contentType, description})`;
  `repos.files.create({ userId, sha256: written.sha256, originalFilename: written.leafName, mimeType: written.storedMimeType, size: written.sizeBytes, width, height, linkedTo: [itemId], source, category: 'IMAGE', generationPrompt ?? null, generationModel ?? null, generationRevisedPrompt ?? null, description, tags: [itemId], storageKey: written.storageKey, projectId: null, folderPath: null }, { id: randomUUID() })`
  (L245-267 — note `tags: [itemId]` too); `home.update({ imageFileId: file.id })`
  NOT deferred (L269, L225-226); `logger.info('[WardrobeImages] Added wardrobe item image', {context:'wardrobe.item-images', scope, itemId, fileId, kind, sizeBytes})`.
- L284-299 `setCurrentWardrobeItemImage`: assert → `resolveMount()` (tombstone
  refuses before the write, L290) → `update({imageFileId})` →
  `logger.debug('[WardrobeImages] Set current wardrobe item image', …)`.
- L305-311 `removeImageFile` (private): `parseMountBlobStorageKey(storageKey ?? '')`
  → if parsed `deleteWardrobeItemImageLink(parsed.mountPointId, itemId, file.originalFilename)`;
  then `repos.files.delete(file.id)` ALWAYS.
- L317-343 `deleteWardrobeItemImage`: list → target or throw Foreign;
  `resolveMount()`; `removeImageFile`; **next-newest rule L330-334**:
  `current = item.imageFileId ?? null; if (current === fileId || (current && !images.some(f => f.id === current))) { current = images.find(f => f.id !== fileId)?.id ?? null; await home.update({imageFileId: current}) }`
  — i.e. the first (newest) remaining image; ALSO repairs a dangling pointer;
  `logger.info('[WardrobeImages] Deleted wardrobe item image', {…, current})`.
- L355-397 `cleanupItemImages(repos, itemId, logTag, meta)`: list failure →
  `logger.warn(\`${logTag} Could not list wardrobe item images for cleanup\`, {…meta, itemId, error})`
  and return; per file try `removeImageFile`, failure →
  `logger.warn(\`${logTag} Failed to remove a wardrobe item image; continuing\`, {…meta, itemId, fileId, error})`;
  if any → `logger.info(\`${logTag} Removed wardrobe item images with the item\`, {…meta, itemId, removed, failed})`.
  Never throws.
- L413-486 `carryItemImages(repos, {mode, sourceItemId, destinationItemId, destinationMountPointId, userId})`
  → `{ fileIdMap, pendingMove }`: per image `parseMountBlobStorageKey` +
  `readMountBlob(storageKey)`; unreadable →
  `logger.warn('[WardrobeImages] Wardrobe image has no readable blob; not carried', …)`
  + skip. `writeWardrobeItemImage({ mountPointId: dest, itemId: destinationItemId, kind: GENERATED→'generated' | IMPORTED→'imported' | else 'uploaded', content, contentType: file.mimeType, description: file.description ?? undefined, leafName: file.originalFilename })`
  (same leaf name, dedupes to one blob). **move**: push
  `{fileId, storageKey: build(dest, written.blobId), sourceLink: parsed.mountPointId !== dest ? {mountPointId, leafName} : null}`
  and `fileIdMap.set(id, id)` — nothing else changes yet (L450-458).
  **copy**: `{id,createdAt,updatedAt,...rest} = file; repos.files.create({...rest, originalFilename: written.leafName, linkedTo: [dest item], tags: [dest item], storageKey}, {id: randomUUID()})`;
  `fileIdMap.set(old, new)`. Count line
  `logger.info('[WardrobeImages] Carried wardrobe item images for transfer', {mode, sourceItemId, destinationItemId, destinationMountPointId, count})`
  only when `images.length > 0`.
- L489-496 `PendingImageMove { repoints: [{fileId, storageKey, sourceLink|null}] }`.
- L504-524 `commitMovedImages(repos, itemId, pending)`: per repoint
  `repos.files.update(fileId, {storageKey})`; success pushes `sourceLink`;
  failure → `logger.warn('[WardrobeImages] Failed to repoint a moved wardrobe picture; keeping its source link', …)`;
  then `dropSourceImageLinks(itemId, toDrop)`.
- L527-552 `dropSourceImageLinks(itemId, links)`: per link
  `deleteWardrobeItemImageLink` (counts `true`s); failure →
  `logger.warn('[WardrobeImages] Failed to drop a source image link after move', {…, mountPointId, leafName, error})`;
  if any links → `logger.info('[WardrobeImages] Dropped source image links after move', {itemId, dropped})`.

### What the commit did NOT do
- No frontmatter list of images (history = `files.findByLinkedTo`, L13-16).
- Does not rename/move pictures when the item's title changes (bridge header L19-21).
- Composite deletion takes only its OWN pictures (L352).
- `removeImageFile` never throws on a missing link (L303), and deletes the
  `files` row even when the storage key does not parse.
- `carryItemImages` does not touch `imageFileId` itself — the transfers route
  rewrites the pointer from `fileIdMap` (§7).

### v5 main
- No counterpart module. Building blocks that exist:
  - `db/files.rs:284 create(&FileCreate, &CreateOptions)` — `FileCreate` has
    every field used (L89-141: `linked_to`, `source`, `category`,
    `generation_prompt/_model/_revised_prompt`, `generation_key`,
    `description`, `tags`, `project_id`, `folder_path`, `storage_key`,
    `file_status`); `:332 update(&FileUpdate)` (has `linked_to`, `storage_key`);
    `:440 delete`. **No full-row `find_by_linked_to`** — only the sweep
    projections `:470 find_sweep_rows_by_linked_to` and
    `:489 find_link_meta_by_linked_to` (both `json_each(files.linkedTo)`).
    A `FileEntry`-validating `find_by_linked_to` is NEW (v4's drops
    `FileEntrySchema` failures — `api/images.rs:22-28` records the same rule
    for `findByCategory`).
  - `services/file_storage.rs:239 parse_mount_blob_storage_key`,
    `:253 build_mount_blob_storage_key`, `:574 read_mount_blob`.
  - `db/doc_mount_file_links.rs:1015 link_blob_content`, `:1513 delete_with_gc`
    (returns `fileGC`), `:1563 ensure_folder_path`, `:1837 find_by_mount_point_and_path`
    (case-insensitive path, L1833-1836).
  - `db/instance_settings.rs:134 get_general_mount_point_id`.
  - `db/vault_wardrobe_public.rs:192 resolve_wardrobe_mount` (private),
    `:227 resolve_character_wardrobe_mount_point` (pub), `:523 update_vault_wardrobe_item`,
    `:538 update_project_wardrobe_item`.
  - **No `resolveWardrobeContainer` port** (v4 `lib/wardrobe/resolve-container.ts`,
    interface L25-33 `{scope, characterId, mountPointId, readItems()}`). The
    transfers service re-derives it inline (`services/wardrobe_transfers.rs:277 resolve_source_item`,
    `:392 resolve_explicit_source`, `:562 resolve_destination`). A `WardrobeItemHome`
    port needs a shared container resolver — the natural home is a new
    `wardrobe_container.rs` the transfers could later adopt (not required this round).
- Writer-thread constraint: v4's `home.update` and the bridge write are one
  request; in v5 the `files` row + link + frontmatter patch run inside ONE
  `Db::write` closure (memory note: lines logged inside the closure are
  invisible to `captured_with` — log on the caller thread).

### Harness
- `vault_wardrobe_write_equivalence.rs` / `vault_wardrobe_public_equivalence.rs`
  (tier-2, frontmatter writes — the `imageFileId` patch rides these);
  `wardrobe_tier2_equivalence.rs`. A NEW tier-2 family for the item-images
  module (list/add/set-current/delete/cleanup/carry/commit/drop over a fixture
  with linked `files` rows + mount blobs) is the gap.

---

## 3. `lib/wardrobe/item-image-prompt.ts` + `item-image-generation.ts` (NEW)

### v4 at HEAD — prompt (139 lines)
- L36-37 `WardrobeImageSubject = 'worn' | 'catalogue'`; `WardrobeImageOrientation = 'portrait' | 'square'`.
- L44 `owner: Pick<Character,'name'|'physicalDescription'|'pronouns'|'archivedAt'> | null`.
- L56 `AESTHETIC_MAX_CHARS = 600`.
- L59-61 `primarySlot(item)` = first `WARDROBE_SLOT_TYPES` slot in `item.types`, else `'accessories'`.
- L64-66 `isHairOnly(items)`: non-empty AND every item has non-empty `types` all `'hair'`.
- L72-82 `buildWardrobeItemCue(item, components)`: garment →
  `decorateOutfitItems([item], {titleOnly:true})[0]` (= `imagePrompt ?? title`);
  outfit → `buildOutfitSlotValues(slot => decorateOutfitItems(components.filter(primarySlot===slot), {titleOnly:true}))`,
  `omit` = slots with no values, `describeOutfit(bySlot, {omit}).trimEnd()`.
- L87-139 `buildWardrobeItemImagePrompt`. `cueInline` L95: outfit →
  `\`the following ensemble:\n\n${cue}\n\n\``; garment → `\`${cue}. \``.
  `hairOnly` over components (outfit) or `[item]`.
  - **Worn** (L102-115, `owner && !owner.archivedAt`): `subject='worn'`,
    `orientation='portrait'`,
    `figure = buildFigureIdentityBlock(owner, hairOnly ? 'head-and-shoulders' : 'full-length')`;
    `framing` = hair → `'Head-and-shoulders portrait, facing the viewer, showing the hairstyle clearly'`
    else `'Full-length, standing, facing the viewer, head to toe in frame'`;
    `intro = \`Solo picture of a single ${figure.subjectNoun}: ${owner.name}. Show exactly one figure. ${framing}.\``;
    `physBlock = figure.physBlock ? \` ${figure.physBlock}\` : ''`;
    `wearing = hairOnly ? \`Wearing their hair as ${cueInline}\` : \`Wearing ${cueInline}\``;
    `rest` = hair → `'The hairstyle is the subject; neutral studio backdrop; even light.'`
    else `'The rest of the attire plain and unremarkable so the garment is the subject; neutral studio backdrop; even light. Only one person in the image.'`;
    `prompt = \`${intro}${physBlock} ${wearing}${rest}\``.
  - **Catalogue** (L116-127): `subject='catalogue'`, `orientation='square'`;
    `display` = hair → `'styled on a featureless mannequin head'`; outfit →
    `'arranged together on a dress form or laid flat'`; else `'on a dress form or laid flat'`;
    outfit → `\`Product photograph of ${cueInline}Shown ${display}, no person, neutral ground, even light. The clothing is the subject.\``;
    garment → `\`Product photograph of ${cue}, ${display}, no person, neutral ground, even light. The garment is the subject.\``.
  - Aesthetic L129-136: `resolveAesthetic({kind:'aurora', projectOfficialMountPointId})`
    `.trim()`, capped at 600 chars (`slice`), prepended as
    `\`Art direction (apply this overall style): ${capped}\n\n${prompt}\``.
    Return `{ prompt: prompt.trim(), orientation, subject }`.

### v4 at HEAD — generation (328 lines)
- L57-58 `NO_WARDROBE_IMAGE_PROFILE_MESSAGE = 'No image profile is configured. Set one in Settings → Images before generating wardrobe pictures.'`;
  L61-66 `NoWardrobeImageProfileError`; L72-81 `WardrobeImageGenerationError(message, trail: RouteAttempt[]|null, refused)`.
- L83-92 result `{ fileId, url, prompt, subject, profile:{id,name}, rerouted, trail, item }`.
- L98-131 `resolveComponentLeaves(repos, home)`: `[]` when no components;
  `itemsById` = container items + (scope≠general) `readGeneralWardrobe(true)`
  (failure → debug line `'[WardrobeItemImage] Shared archetypes unavailable for component resolution'`);
  character scope → `hydrateComponentGraph(repos, characterId, itemsById, await sharedWardrobeTiersForCharacter(characterId, []))`;
  `expandComposites([item.id], itemsById).leafIds` minus the item itself.
- L137-152 `resolveOwner`: character scope → `repos.characters.findById`;
  a throw → `logger.warn('[WardrobeItemImage] Owner unreadable; falling back to a catalogue shot', …)` + `null`.
- L155-161 `resolveProjectAestheticMount`: project scope → `getProjectOfficialMountPointId(containerId)`, else null.
- L169-328 `generateWardrobeItemImage(repos, {userId, home, containerId, imageProfileId?})`:
  L183 `await home.resolveMount()` FIRST (tombstone before spend);
  L185-189 `resolveWardrobeImageProfile(userId, repos, imageProfileId)` —
  `!profile || !profile.apiKeyId` → throw; `findApiKeyByIdAndUserId` —
  `!key_value` → throw (same error); L191-195 owner/components/aesthetic mount
  in `Promise.all`; L197-202 prompt; L204-214 debug line
  `'[WardrobeItemImage] Generating wardrobe item image' {itemId, scope, containerId, subject, orientation, componentCount, profileId, profileOverride}`;
  L216-217 `conciergePolicy = resolveConciergeSettings(chatSettings ?? null, null)`;
  L219-260 `attempt(profile, key)`: `createImageProvider(provider)`,
  `buildImageGenParams({profile, prompt, overrides:{n:1, style:'natural'}, orientation, logContext:{context, itemId, profileId}})`,
  `provider.generateImage(params, key)`, `logLLMCall({ userId, type: 'WARDROBE_ITEM_IMAGE', characterId: home.characterId ?? undefined, provider, modelName, imageProfileId, request:{messages:[{role:'user',content:prompt}]}, response:{content: revisedPrompt || \`Generated ${n} image(s)${rerouted?' (Concierge reroute)':''}\`}, durationMs })`
  on success; on throw the same with `response:{content:'', error}` then rethrow;
  L264-268 `generateImageWithConciergeFailover({profile, apiKey}, attempt, { userId, chatId: null, purpose: 'wardrobe', conciergePolicy })`;
  L269-280 catch → `trail = getConciergeTrail(error)`, `refused = trail?.some(outcome==='refused')`,
  `logger.warn('[WardrobeItemImage] Wardrobe item image generation failed', {itemId, error, refused, conciergeTrail:[{profileName,outcome,detail}]})`,
  throw `WardrobeImageGenerationError(msg, trail, refused)`;
  L282-286 no image → `WardrobeImageGenerationError('The image provider returned no picture', trail.length ? trail : null, false)`;
  L288-293 `convertToWebP(Buffer.from(rawData,'base64'), mimeType || 'image/png', \`wardrobe.${ext || 'png'}\`)`;
  L295-305 `addWardrobeItemImage(…, {kind:'generated', …, generationPrompt: prompt, generationModel: failover.profile.modelName, generationRevisedPrompt: revisedPrompt || null})`;
  L307-316 `logger.info('[WardrobeItemImage] Wardrobe item image generated', {itemId, fileId, bytes, subject, profileId, rerouted, durationMs})`;
  L318-327 return (`trail` = `[]` → `null`).

### What the commits did NOT do
- No chat, no Locked state, no announcement, no ledger row for a wardrobe
  refusal (`chatId: null`, header L9-12) — the Concierge failover's
  announcement/ledger branches are skipped by the chokepoint itself when
  `chatId` is null (so the `'wardrobe'` purpose strings in §9 are reachable
  only through the trail / type unions, not a written bubble).
- No preview-avatar-style bypass (L16-17). No `n > 1`. No caching (unlike the
  avatar cache). No orientation other than portrait/square.
- `b3f937076` did NOT change either module (the job handler reuses them).

### v5 main
- No counterpart. Pieces: `services/avatar_prompt.rs` (§5 — no shared
  identity block yet); `wardrobe.rs:882 decorate_outfit_items_title_only`,
  `:226 build_outfit_slot_values`, `:304 describe_outfit_with_omit`,
  `:431 expand_composites`, `:117 slot_meta` (slot order);
  `tools/wardrobe_shared.rs:216 hydrate_component_graph` (**private**);
  `wardrobe_tiers.rs:193 shared_wardrobe_tiers_for_character(main, mount, cid, &[])`;
  `db/archetype_wardrobe.rs:93 read_general_wardrobe`;
  `services/aesthetics.rs:100 resolve_aesthetic(db, AestheticKind, project_mount, max_chars)`
  (async over `&Db`) + `:83 get_project_official_mount_point_id(db, Option<&str>)`;
  `services/dangerous_content/resolver.rs:327 resolve_concierge_settings(global, chat)`;
  `services/dangerous_content/image_failover.rs:342 generate_image_with_concierge_failover(primary, attempt, &ImageFailoverContext)`
  with ctx fields L163-199 (`chat_id: Option`, `purpose: ImagePurpose`,
  `understudy`, `profile_kind`, `primary_via`, `announce_unresolved_refusal`)
  and `:220 get_concierge_trail`; `image_gen/params_builder.rs:201-208`
  (`orientation: Option<Orientation>`); `image_gen.rs:140 Orientation {Portrait, Landscape, Square}`;
  `services/file_storage.rs:404 convert_to_webp(&dyn PixelCodec, …)`.
- The host-side image seam precedent: `quilltap-host/src/images_generate.rs:89 images_generate_seams(version) -> ImagesGenerateSeams`
  (`api/images.rs:1354-1362`: `provider: ErasedImageGenerate`, `classifier`,
  `codec: Arc<dyn PixelCodec>`); the job-side precedent is
  `character_avatar_job.rs:150-170 AvatarJobDeps` (image_provider, completion,
  moderation, api_keys, transcoder, now_ms, declarations_for, blob_webp).

### Harness
- `image_failover_tier3_equivalence.rs` (recipe header L36-54: stage
  `image-failover.test.ts` outside `.claude/`, `build-refusal-ledger-fixture.ts`,
  jest `--testTimeout=120000`); `avatar_job_tier3_equivalence.rs`
  (`QT_ORACLE_AVATAR`, the mocked-provider job precedent);
  `image_gen_leaves_equivalence.rs` (params builder);
  `images_generate_route_equivalence.rs`. The prompt builder wants a NEW
  tier-1 family (pure over a fixture of items/owners/components — oracle via
  `npx tsx` importing `buildWardrobeItemImagePrompt`), and the generation fn a
  NEW tier-3 family.

---

## 4. `lib/file-storage/wardrobe-image-bridge.ts` (NEW) + the `b3f937076` child routing

### v4 at HEAD (211 lines)
- Path scheme L9 / L46-58: `WARDROBE_IMAGES_FOLDER = 'Wardrobe/images'`;
  `wardrobeItemImageFolder(itemId) = \`Wardrobe/images/${sanitizeLeafName(itemId)}\``;
  `wardrobeItemImagePath(itemId, leaf)`. Leaf L125:
  `\`${timestampStem(now)}-${kind}-${randomUUID().slice(0,8)}.webp\`` where
  `timestampStem` L61-64 = UTC `yyyymmdd-hhmmss` from `toISOString()`.
  With `leafName` given (transfer) → `sanitizeLeafName(leafName)` and NO
  unique-suffix bump (L126-128); without → `resolveUniqueRelativePath`.
- L105-116 **child routing (`b3f937076`)**: `if QUILLTAP_JOB_CHILD==='1'` →
  `callHost('writeWardrobeItemImage', input)` after
  `logger.debug('[WardrobeImageBridge] Routing wardrobe image write to the parent', {mountPointId, itemId})`.
  Pre-commit it REFUSED in the child (`refuseInJobChild`); the refusal
  message also changed from `'…wardrobe images are written from API routes…'`
  to `'…wardrobe image links are removed from API routes, never from the job child'`
  (L66-72) and now guards only `deleteWardrobeItemImageLink` (L186).
- L130 `ensureFolderPath(mountPointId, folder)`; L134-145
  `docMountFileLinks.linkBlobContent({ mountPointId, relativePath, fileName: basename, folderId, originalFileName: basename, originalMimeType: contentType, storedMimeType: contentType, sha256: sha256OfBuffer(content), description: description ?? '', data: content })`
  — **no transcode here** (L132-133: `linkBlobContent` is the normalization
  chokepoint and may rewrite `storedMimeType`/`relativePath`);
  L147-148 `emitDocumentWritten({mountPointId, relativePath: link.relativePath})` +
  `docMountPoints.refreshStats(mountPointId).catch(noop)`; L150 re-read blob
  for `storedMimeType`; result L152-161 `{ storageKey: mount-blob:{mp}:{blobId}, linkId, blobId, relativePath: link.relativePath, leafName: basename(link.relativePath), storedMimeType: blob?.storedMimeType ?? contentType, sha256: link.sha256, sizeBytes: link.fileSizeBytes }`;
  L163-171 `logger.debug('[WardrobeImageBridge] Wrote wardrobe item image', {mountPointId, itemId, kind, relativePath, blobId, sizeBytes})`.
- L181-211 `deleteWardrobeItemImageLink(mountPointId, itemId, leafName) → boolean`:
  `findByMountPointAndPath` → absent → `logger.debug('… No link to delete', …)` + `false`;
  `deleteWithGC(link.id)` → `emitDocumentDeleted` + `refreshStats`;
  `logger.debug('… Deleted wardrobe item image link', {…, blobCollected: fileGC})` + `true`.
- Host-RPC side (`b3f937076`): `lib/background-jobs/host/host-rpc-dispatcher.ts:92-100`
  — `case 'writeWardrobeItemImage'` dynamic-imports the bridge and passes
  `{ ...params, content: Buffer.from(params.content) }` (structured clone
  delivers a `Uint8Array`); `ipc-types.ts:161` adds the method name to the
  union, `:144` doc.

### What the commits did NOT do
- The projection sweep ignores these blobs (L17-21) — nothing re-indexes them
  as garments. No blob dedupe logic of its own (`linkBlobContent`'s sha dedupe).
- `deleteWardrobeItemImageLink` has no child caller and still refuses in the child.

### v5 main
- **No child process exists in v5** (single-writer runtime; jobs run in-process
  on the job runner). The host-RPC arm (`writeWardrobeItemImage`) and
  `refuseInJobChild` have NO counterpart and NEED none — the bridge write is a
  plain `Db::write` from either the route or the job handler. Record as a
  structural no-port (the same ruling the avatar/Lantern bridges took:
  `services/image_job_storage.rs` header L1-20).
- Closest existing bridge: `services/image_job_storage.rs` —
  `store_blob_to_mount(mount, mp, desired_path, content, content_type, original_file_name, description, blob_webp: &dyn WebpTranscoder)`
  (~L270-340: `normalise_relative_path` → `normalise_blob_relative_path` →
  `resolve_unique_relative_path` (L358, private) → `ensure_folder_path` →
  `link_blob_content`), `:105 write_character_avatar_to_vault`,
  `:234 write_lantern_background_to_mount_store`; `:390 sanitize_leaf_name`
  (private; other private copies at `tools/generate_image.rs:1197`,
  `photos/character_gallery_service.rs:438`, `services/conversation_summary_vault_bridge.rs:141`
  — a DRY candidate, not this round's job). `WrittenImage` L39-55 is the
  result shape to extend (needs `link_id`, `leaf_name`).
- Blob normalization: `services/mount_index/blob_transcode::WebpTranscoder`
  (the P4.104 seam the avatar handler wires at
  `quilltap-host/src/spine.rs:3405-3410`).
- `emit_document_written` / `emitDocumentDeleted`: no fn of that name in v5
  (grep empty) — check how `store_blob_to_mount` signals realtime (likely via
  the write-applier's table hints, `realtime/job_topics.rs:128-132` maps
  `docMount*` tables → `MountPoints`), so the two emits are probably covered
  structurally; VERIFY at port time.
- `db/doc_mount_points.rs:359 refresh_stats`.

---

## 5. `lib/image-gen/profile-resolution.ts` + `lib/wardrobe/avatar-prompt.ts` + `create-body.ts` + `item-route-steps.ts`

### v4 at HEAD
- **`resolveWardrobeImageProfile`** `profile-resolution.ts:119-141` (hunk
  `7c8572869`): `usable(p) = !!p && p.userId === userId && !!p.apiKeyId`;
  1. `override` → `findById`, usable → return; 2. `chatSettings.findByUserId(userId)?.wardrobeImageSettings?.imageProfileId`
  → `findById`, usable → return; 3. `findDefault(userId)` → `fallback && fallback.apiKeyId` → return;
  else `null`. L95-103 `WardrobeProfileRepos` (`imageProfiles.{findById,findDefault}`,
  `chatSettings.findByUserId → Pick<ChatSettings,'wardrobeImageSettings'>`).
  "No usable profile" = none of the three passes; the GENERATION additionally
  requires `profile.apiKeyId` AND a readable `key_value`
  (`item-image-generation.ts:186-189`) — both throw the same 400 text.
  `resolveImageProfileForChat` (L51-91) UNCHANGED; the Lantern's
  `storyBackgroundsSettings.defaultImageProfileId` is deliberately NOT consulted (L114-117).
- **`buildFigureIdentityBlock`** `avatar-prompt.ts:71-95` (type `FigureFraming`
  L51, `FigureIdentityBlock {subjectNoun, physicalText, physBlock}` L53-63):
  head-and-shoulders order `[headAndShoulders, medium, short, long, complete, fullDescription]`;
  full-length order `[complete, long, medium, short, fullDescription, headAndShoulders]`;
  `physicalText = (order.find(Boolean) || '').trim()`;
  `subjectNoun = genderNounFromPronouns(pronouns) ?? 'person'`;
  `physBlock = physicalText ? \`${physicalText.replace(/[.!?]+$/, '')}.\` : ''`.
  `buildCharacterAvatarPrompt` L124 calls it with `'head-and-shoulders'` and
  reads `figure.physicalText` / `figure.subjectNoun` (L190) / `figure.physBlock`
  (L199). **Avatar prompt bytes: unchanged by construction** — the hunk moves
  the same three computations (same ladder order, same regex, same `??
  'person'`) behind the helper; the two `Solo portrait…` template lines
  (L195/L199) and `outro` are not in the diff. Prove by regen of the avatar
  families (below), not by inspection.
- `create-body.ts:47` `imageFileId: null` (header L9-11: "A fresh item never
  carries an `imageFileId`"); `wardrobe.types.ts:156` `updateWardrobeSchema`
  gains `imageFileId: UUIDSchema.nullable().optional()` (update-only — the
  create schema did NOT gain it), `:208` `WardrobeItemSchema.imageFileId`.
- `item-route-steps.ts:27` re-exports `cleanupItemImages`; `:33-48 imageChoiceError(repos, itemId, imageFileId) → string|null`:
  catches `ForeignWardrobeImageError` →
  `logger.info('[WardrobeItem] Refused an imageFileId that is not the item\'s own', {itemId, imageFileId})`
  and returns `'imageFileId must name one of this item\'s own pictures'`
  (the PUT's 400 text); other errors rethrow.

### What the commits did NOT do
- Did not touch `resolveImageProfileForChat`; did not add a project-level
  wardrobe profile tier; did not make `createWardrobeSchema` accept `imageFileId`.
- `b3f937076` did not touch any of these four files.

### v5 main
- `services/image_profile_resolution.rs:19-113` — only `resolve_image_profile_for_chat(main, mount, user_id, chat, chat_settings) -> Option<String>`
  (sync, returns the ID). A `resolve_wardrobe_image_profile(main, user_id, chat_settings, override) -> Option<Value>`
  is NEW; v4 returns the PROFILE (the generation reads `.apiKeyId`, `.id`,
  `.provider`, `.modelName`, `.name`). `usable` closure at L26-39 is the
  reusable shape; default lookup at L95-109 scans `find_all` for `isDefault`.
- `services/avatar_prompt.rs:62 build_character_avatar_prompt`: the ladder is
  INLINE at L83-104 (`TIERS` const, head-and-shoulders order only),
  `subject_noun` at L190-191 via `pronoun_gender::gender_noun_from_pronouns(pronoun_subject(character))`
  (`pronoun_gender.rs:21`), `phys_block` at L205-210 over
  `strip_trailing_terminal_punct` (L256). No `FigureFraming`/identity-block
  fn — the refactor is a NEW pub fn here (two orders) with the avatar path
  re-pointed at it; the avatar prompt bytes must stay identical (regen
  `avatar_job_tier3_equivalence` + `wardrobe_tools_avatar_trigger_equivalence`).
- `WardrobeItem` struct `vault_overlay.rs:334-366`: **no `image_file_id`**;
  `migrated_from_clothing_record_id: Option<Option<String>>` with
  `skip_serializing_if = "Option::is_none"` (L351-355) is the exact pattern
  for the new key. `from_read_value` L370-411 and
  `db/vault_wardrobe_public.rs:284-300 item_from_read` both need the field.
  There is NO create-body fn: each tier builds its create inline
  (`api/wardrobe.rs:253-298`, `api/characters.rs:2669-2750`,
  `api/projects.rs:1512-1613`, `api/groups.rs:1240-1301`,
  `services/wardrobe_transfers.rs:666-688`) — five sites where
  `image_file_id: None` lands (the ledger's "create-body (P4.D163)" names no
  v5 file because there is none).
- `WardrobePatch` (used by `api/projects.rs:1758 build_wardrobe_patch`,
  `tools/wardrobe_update.rs:140-150`) needs `image_file_id: Option<Option<String>>`.

### Harness
- `avatar_job_tier3_equivalence.rs` (the avatar prompt rides it), `avatar_cache_key_equivalence.rs`,
  `wardrobe_tools_avatar_trigger_equivalence.rs` (`QT_WT_AVATAR_SPEC`,
  `build-wardrobe-tools-fixture.ts`); no standalone avatar-prompt tier-1
  family exists (grep `buildCharacterAvatarPrompt` in `harness/oracle/cases/*`:
  none) — the refactor's "bytes unchanged" proof is the avatar job family.
- `vault_wardrobe_item_file_equivalence.rs` (tier-1 parser; recipe
  `npx tsx …/cases/vault-wardrobe-item-file.ts`), `vault_wardrobe_emit_equivalence.rs`
  (writer), `vault_wardrobe_public_equivalence.rs`, `vault_wardrobe_write_equivalence.rs`.

---

## 6. The vault frontmatter key + the item PUT/DELETE arms (four tiers)

### v4 at HEAD
- `lib/mount-index/character-vault.ts:361-363` `buildWardrobeItemFile`:
  `if (item.imageFileId) data.imageFileId = item.imageFileId;` — emitted ONLY
  when set, placed after `migratedFromClothingRecordId` and before
  `createdAt`/`updatedAt`.
- `lib/database/repositories/vault-overlay/parsers.ts:376-379`
  `parseWardrobeItemFile`: `typeof === 'string' && length > 0 ? string : null`
  (an EMPTY string reads as `null` — unlike `migratedFromClothingRecordId`,
  which keeps any string); emitted at `:415` (always present, nullable).
- PUT arms: `imageChoiceError(repos, itemId, fields.imageFileId)` → `badRequest(msg)`
  BEFORE `applyArchiveFlag` — `app/api/v1/characters/[id]/wardrobe/[itemId]/route.ts:83-84`,
  `app/api/v1/wardrobe/[itemId]/route.ts:74-75`,
  `lib/mount-index/mount-wardrobe-route-factory.ts:391-392` (project + group).
- DELETE arms: `cleanupItemImages(repos, itemId, logTag, meta)` AFTER the
  item is gone and BEFORE the success log —
  characters route `:133` (`'[Wardrobe v1]'`, `{characterId: id, itemId}`),
  global route `:115` (`'[Wardrobe Archetypes v1]'`, `{itemId}`),
  factory `:443-447` (`logTag`, `{[logIdKey]: id, mountPointId, context: 'wardrobe'}`).

### What the commits did NOT do
- The transfers' `createAtDestination` writes `imageFileId` (§7) but the
  per-tier CREATE routes never accept it (create schema unchanged).
- No validation that `imageFileId` on a PUT is a UUID beyond `UUIDSchema`
  (the ownership check runs after Zod).

### v5 main
- Parser `vault_overlay.rs:1077-1190 parse_wardrobe_item_file` — no
  `imageFileId`; `WardrobeItemFromFile` struct `:1040-1060` (the
  `migrated_from_clothing_record_id: Option<String>` at L1048-1049 is the
  slot neighbour). Writer `:2228-2288 build_wardrobe_item_file` — insert
  after L2272-2276 (`migratedFromClothingRecordId`) and before `createdAt`.
- PUT arms (no image check anywhere): `api/characters.rs:2784-2890`
  (header comment L2794-2800 banks that v5 validates NOTHING else of the
  update body — pre-existing, wider than this round), `api/wardrobe.rs:327-406`,
  `api/projects.rs:1638-1719`, `api/groups.rs:1326-1399`.
- DELETE arms (equipped-refs cleanup, then delete; NO image cleanup):
  `api/characters.rs:2891-2930` (note L2918: `remove_equipped_item_from_all_chats(&iid)?` is
  PROPAGATED here, whereas the other three tiers `let _ =` it — a pre-existing
  asymmetry vs v4's warn-and-proceed; out of scope, bank it),
  `api/wardrobe.rs:413-449`, `api/projects.rs:1720-1756` (success line at
  L1743-1750 `[Projects v1] Deleted project wardrobe item`), `api/groups.rs:1400-1424`.
- Wear-ledger drop (`cleanupEquippedRefs`'s second half) — present? grep
  `wardrobeWear`/`delete_by_item_ids` was not run; the P4.D-era ledger says
  the wear ledger landed. Verify before writing the DELETE order.

### Harness
- `vault_wardrobe_item_file_equivalence` (parser), `vault_wardrobe_emit_equivalence`
  (writer), `characters_mutations_equivalence.rs` (39 `wardrobe` mentions —
  the character item PUT/DELETE echo), `group_wardrobe_routes_equivalence.rs`
  (recipe header L24-35), `projects_routes_equivalence.rs`,
  `quilltap-web/tests/wardrobe_routes_equivalence.rs` (archetype tier).

---

## 7. `app/api/v1/wardrobe/transfers/route.ts` — pictures travel

### v4 at HEAD
- Header L19-23. Imports L42-47 `carryItemImages, commitMovedImages, resolveContainerMountPointId, PendingImageMove`.
- `createAtDestination` L232 carries `imageFileId: item.imageFileId ?? null`
  (character/general tier through `repos.wardrobe.create`; project/group
  through `createProjectWardrobeItem(mp, item)` which takes the whole item).
- **L380-385** (move OR `components:'move'`): `await resolveContainerMountPointId(source.scope, source.characterId, source.mountPointId)`
  BEFORE any write — an archived source character throws `CharacterArchivedError`
  here → the catch L501-503 `conflict('An archived character\'s wardrobe cannot be changed')`
  (409). (Pre-commit the route had no 409 arm; the `CharacterArchivedError`
  import L30 is new.)
- L392-396 destination mount resolved (throws for an archived DESTINATION too — same 409).
- L397-419 `travellers` = components (mode `copy`→copy else move) THEN the
  item; per traveller `carryItemImages(…)`, then
  `planned.imageFileId = currentImage ? fileIdMap.get(currentImage) ?? null : null`
  (L414-415 — a pointer at a file that was NOT carried becomes `null`);
  moves push `pendingMove`.
- L421-427 components created, then item; L429-444 deletes (unchanged);
  **L446-449** `commitMovedImages(repos, itemId, pending)` for each moved
  traveller AFTER the source deletes.
- Order of errors: a `ZodError` 400 first, then `CharacterArchivedError` 409,
  else `logger.error('[WardrobeTransfers v1] Failed to transfer item', …)` + 500.

### What the commits did NOT do
- Did not change the request schema, the same-location/collision checks, the
  read-back verification, or the `GET` roster. Did not make the transfer
  transactional (a failure after `carryItemImages` leaves destination links
  behind; a failure after the deletes leaves rows pointing at the source blob
  — v4 documents only the "before the deletes" safety, item-images.ts:401-408).

### v5 main
- `services/wardrobe_transfers.rs:721-927 transfer_wardrobe_item` — steps
  1-9 match v4's pre-commit shape; **no picture carry, no pre-resolve of the
  source mount, no `Conflict` error** (`TransferError` L85-90: `NotFound`,
  `BadRequest`, `Server`, `Internal`); `api/wardrobe.rs:689-737 wardrobe_transfer_apply`
  maps those four — a 409 arm is NEW (`CharacterArchivedError` ⇄
  `WardrobePublicError` — see `db/vault_wardrobe_public.rs:192 resolve_wardrobe_mount`,
  whose archived-character error the character-tier routes already map to
  409 with v4's sentence at `api/characters.rs:3338-3341`).
- `create_at_destination` L666-688 passes the whole `WardrobeItem` (so the
  frontmatter key carries once the struct has it).

### Harness
- `wardrobe_transfers_tier2_equivalence.rs` (header L1-40: seven mount-index
  tables structural-diffed + outcome; fixture pair from
  `build-wardrobe-transfers-fixture.ts`; oracle `wardrobe-transfers.test.ts`,
  `QT_ORACLE_WTR`). Needs: fixture items WITH pictures (linked `files` rows
  + blobs under `Wardrobe/images/<id>/`), an archived-source 409 row, and the
  `files` table added to the diffed set.

---

## 8. Chat settings: `wardrobeImageSettings` (both commits)

### v4 at HEAD
- `lib/schemas/settings.types.ts:614-626` `WardrobeImageSettingsSchema = z.object({ imageProfileId: UUIDSchema.nullable().default(null), generateFromTools: z.boolean().default(false) })`
  (`generateFromTools` from `b3f937076`, doc L617-622); `:765`
  `ChatSettingsSchema.wardrobeImageSettings: WardrobeImageSettingsSchema.optional()`
  — placed AFTER `storyBackgroundsSettings` (L754-757) and BEFORE `conciergeSettings`.
- `app/api/v1/settings/chat/route.ts:70` param; `:183-188` arm:
  `if (typeof wardrobeImageSettings !== 'undefined') { updateData.wardrobeImageSettings = WardrobeImageSettingsSchema.parse(x); logger.debug('[Settings v1] Wardrobe image settings updated', { imageProfileId: … ?? null, generateFromTools }) }`
  — the arm sits after `storyBackgroundsSettings` (L178-181) and before
  `contextCompressionSettings`; a Zod failure propagates to the route's
  `includes('Invalid') ? 400 : 500` split → here the ZodError message does
  NOT contain `Invalid` for a bad uuid (`Invalid UUID`? — Zod 4's uuid
  message IS `Invalid UUID`; for a wrong-type `generateFromTools` it is
  `Invalid input: expected boolean, received …`) — VERIFY per row with the
  oracle; the `composer_settings` family (P4.D73) already pins this shape for
  sibling JSON bags. `:408`, `:443` destructure/pass.
- `lib/database/repositories/chat-settings.repository.ts:222-225` seed in
  `updateForUser`'s create branch: `wardrobeImageSettings: { imageProfileId: null, generateFromTools: false }`.
- Migration `migrations/scripts/add-wardrobe-image-settings-field-v1.ts`:
  id L21; `DEFAULT_WARDROBE_IMAGE_SETTINGS = JSON.stringify({ imageProfileId: null })`
  L27 (**NOT updated by `b3f937076`** — the column default lacks
  `generateFromTools`; the Zod `.default(false)` fills it on read);
  `introducedInVersion: '4.10.0'`, `dependsOn: ['sqlite-initial-schema-v1']`
  L32-33; `shouldRun` L35-43 = SQLite && table exists && column absent;
  `run` L50-55 `addColumnIfMissing('chat_settings','wardrobeImageSettings', \`TEXT DEFAULT '${…}'\`)`
  → ALTER text `ALTER TABLE chat_settings ADD COLUMN wardrobeImageSettings TEXT DEFAULT '{"imageProfileId":null}'`;
  logs `'Added wardrobeImageSettings column to chat_settings table'`
  (context `migration.add-wardrobe-image-settings-field`). Registered
  `migrations/scripts/index.ts:421, 855, 1267` (after `seedWardrobeWearStatsMigration`).
- `lib/startup/prettify.ts:158` `'add-wardrobe-image-settings-field-v1': 'Engaging a portraitist for the wardrobe'`.
- `lib/tools/handlers/help-settings-handler.ts:152` images category gains
  `wardrobeImageSettings: settings?.wardrobeImageSettings || null`.

### What the commits did NOT do
- No GET change (hydration is the schema's). No per-chat override. The
  migration default was not bumped for `generateFromTools` (two JSON shapes
  in the wild: `{"imageProfileId":null}` from the ALTER default, and the
  two-key object from the repo seed / any PUT).

### v5 main
- `api/settings.rs:914-919` `storyBackgroundsSettings` arm:
  `json_field::<chat_settings::StoryBackgroundsSettings>("story backgrounds settings", v)`
  — the arm shape to copy, inserted directly after it (v4 schema order);
  P4.D251's `impersonationVoiceMode` arm `:1031-1046` is the enum precedent
  (its 24-line comment documents the `typeof !== 'undefined'` / explicit-null
  / unknown-key-silence rules).
- `db/chat_settings.rs`: the six sites (`chat_settings_column_sites_guard`):
  struct `:730-812` (`story_backgrounds_settings` L808), typed struct
  `:375-383 StoryBackgroundsSettings` (new `WardrobeImageSettings {image_profile_id: Option<String>, generate_from_tools: bool}` with
  `#[serde(default)]` on both to absorb the one-key column default), INSERT
  bind `:980-981` + list `:1041`, UPDATE pattern `:1149-1151`, the tolerant
  SELECT array `:1300-1330` (**positional**: `storyBackgroundsSettings` is
  index 32 at L1505-1507; inserting after it shifts `conciergeSettings`,
  `autoLockSettings`, `timezone`, `createdAt`, `updatedAt` by one — every
  `r.get::<_, _>(33..)` after it must move), read `obj.insert` at
  `:1505-1507` (parse_json). `update_for_user` `:1737` (seed in the create branch).
- Boot ensure: NEW `db/chat_settings_wardrobe_image_settings_repair.rs` on
  the `db/chat_settings_composer_repair.rs:72-90` pattern (`ALTER TABLE
  "chat_settings" ADD COLUMN "wardrobeImageSettings" TEXT DEFAULT
  '{"imageProfileId":null}'` — v4's migration spelling), called from
  `quilltap-host/src/host.rs` beside `:1298` (composer) / `:1614`
  (informs permanent). Two shapes (ledger): generateDDL's schema-order
  nullable `"wardrobeImageSettings" TEXT` (no DEFAULT — `.optional()`
  without `.default`) for a FRESH table vs the migration's appended
  `TEXT DEFAULT '…'` — the `chat_settings_voice_mode_ensure_equivalence.rs`
  three-shape differential is the precedent to clone.
- D23 re-dump #6: `services/provisioning/fresh_schema.json` has 0 hits for
  `wardrobeImageSettings` (its `chat_settings` DDL goes
  `storyBackgroundsSettings … → conciergeSettings`); `chat_settings_seed.json`
  0 hits (the seed row gains `"wardrobeImageSettings": "{\"imageProfileId\":null,\"generateFromTools\":false}"`
  — v4's repo default, as the `impersonationVoiceMode` precedent at
  `provisioning/mod.rs:189-191`). Register in `provisioning/mod.rs:157-170`.
  Run `dump-fresh-schema.ts` FROM A PIN AT `b3f937076` (the ledger's regen
  rule; a HEAD dump carries `f5e953a3f`'s later moves if any).
- `tools/help.rs:345-360 fetch_images` → add `"wardrobeImageSettings": settings?.wardrobeImageSettings || null`
  (same materialized-default note as `story`, L349-354).
- Prettify labels: no v5 counterpart found (grep `who has worn what` in
  `crates/`: none) — v5 has no migration runner, so `PRETTY_LABELS` is a
  standing no-port; confirm, do not port.

### Harness
- `settings_routes_equivalence.rs` (`QT_ORACLE_SETTINGS_ROUTES`; jest
  `-- settings-routes`; families incl. `composer_settings`, `settings_zod`,
  `impersonation_voice` L18) — the oracle case `settings-routes.test.ts` has
  **0** `storyBackgroundsSettings` rows, so the JSON-bag arm precedent in the
  corpus is `smartTypographySettings` (P4.D73). New family `wardrobe_images`
  with PUT accept / reject arms + GET echo.
- `chat_settings_tier2_equivalence.rs` (header: nested typed structs in
  schema order; recipe L26-31), `chat_settings_column_sites_guard.rs`
  (source census — add the column), `chat_settings_voice_mode_ensure_equivalence.rs`
  (ensure precedent), `provisioning_equivalence.rs` (D23 tripwire;
  `QT_FRESH_SCHEMA_LIVE` REQUIRED), `help_tools_equivalence.rs`
  (`fetch_images` echo), `almanack_*` (§10).

---

## 9. Log type, Concierge purpose strings

### v4 at HEAD
- `lib/schemas/llm-log.types.ts:36` `'WARDROBE_ITEM_IMAGE'` inserted after
  `'WARDROBE_IMAGE_ANALYSIS'`, before `'ANSWER_CONFIRMATION'`.
- `lib/services/concierge-notifications/writer.ts:385`
  `ConciergeRefusalPurpose = 'tool' | 'lantern' | 'avatar' | 'dialog' | 'wardrobe' | 'text'`;
  `:415-416` `case 'wardrobe': return { voiced: 'the commission for a picture of a garment', plain: 'a wardrobe picture' }`.
- `lib/services/dangerous-content/image-failover.ts:68`
  `purpose: 'tool' | 'lantern' | 'avatar' | 'dialog' | 'wardrobe'`;
  `refusal-ledger.ts:44` `purpose: 'chat' | 'cheap' | 'tool' | 'lantern' | 'avatar' | 'dialog' | 'wardrobe'`.

### What the commits did NOT do
- Nothing WRITES a `'wardrobe'` announcement or ledger row today (`chatId: null`,
  §3) — the strings are reachable only if a future caller passes a chat.
  `b3f937076`'s job handler also passes no chat (it has `payload.chatId` but
  `generateWardrobeItemImage` never receives it).

### v5 main
- `services/llm_logging.rs:371-400 log_type` consts: no `WARDROBE_ITEM_IMAGE`
  (and no `WARDROBE_IMAGE_ANALYSIS` either — the analyze-image arm spells its
  own literal; the SPA union `apps/web/src/app/chat/llm-logs.api.ts:50` has
  `WARDROBE_IMAGE_ANALYSIS` and will need `WARDROBE_ITEM_IMAGE` next to it —
  SPA survey's).
- `services/concierge_notifications.rs:495-534 ConciergeRefusalPurpose {Tool, Lantern, Avatar, Dialog, Text}`
  + `as_wire`/`from_wire`/`commission` (L526-533); add `Wardrobe` →
  `"wardrobe"` → `("the commission for a picture of a garment", "a wardrobe picture")`.
- `services/dangerous_content/image_failover.rs:98-126 ImagePurpose {Tool, Lantern, Avatar, Dialog}`
  with `announcement()`/`ledger()` maps — add `Wardrobe` both ways;
  `refusal_ledger.rs:67-88 RefusalPurpose` add `Wardrobe => "wardrobe"`.

### Harness
- `task_type_log_mapping_equivalence.rs` (admittance arm: v4's `LLMLogTypeEnum`
  verdict rides each row — the new enum value matters only if a cheap-LLM
  task maps to it; it does not); `image_failover_tier3_equivalence.rs`
  (purpose spelled in the trail/log lines); `concierge_*` families for the
  commission strings only if a chat-bearing caller is ever added.

---

## 10. Almanack "Wardrobe Images"

### v4 at HEAD
- `lib/tools/almanack/types.ts:271` `wardrobeImages: { hasDesignatedImageProfile: boolean }`
  (after `storyBackgrounds`, before `timestamps`);
  `phase3-ledgers.ts:614` default `{ hasDesignatedImageProfile: false }`,
  `:719-721` collect `!!chatSettings?.wardrobeImageSettings?.imageProfileId`;
  `render.ts:707-708`: `push('#### Wardrobe Images', '')` then
  `` push(`- **Has Designated Image Profile**: ${yesNo(…)}`, '') `` — placed
  after the Lantern block's `Has Default Image Profile` line and before
  `#### Aurora (Core Whisper)`. Snapshot hunk confirms the rendered bytes
  (`__snapshots__/render.test.ts.snap:+302-305`); fixture
  `__tests__/helpers/almanack/fixture.ts:286`.

### What the commits did NOT do
- `generateFromTools` is NOT reported (`b3f937076` left the Almanack alone).

### v5 main
- `almanack/types.rs:441-446 StoryBackgroundsConfig` + `:518-531 FeatureConfigInfo`
  (field order = wire key order; add `wardrobe_images: WardrobeImagesConfig { has_designated_image_profile }`
  after `story_backgrounds`); `phase3_ledgers.rs:596-599` default,
  `:721-726` collect (`jtruthy(s, &["wardrobeImageSettings","imageProfileId"])`);
  `render.rs:1062-1072` the Lantern block → insert the two `push!` lines after
  L1072 and before `#### Aurora (Core Whisper)` (L1074).

### Harness
- `almanack_render_equivalence.rs` (`QT_ORACLE_ALMANACK_RENDER`, recipe pin
  recorded in the test L41), `almanack_tier2_equivalence.rs`
  (`QT_ORACLE_ALMANACK_TIER2`, `almanack-routes.test.ts` jest mirror) —
  both regen at the new pin.

---

## 11. `b3f937076` — the tool decision module + the four tool handlers/definitions

### v4 at HEAD — `lib/wardrobe/tool-image-generation.ts` (146 lines)
- L28-32 `WardrobeToolImageResult { status: 'queued'|'not-enabled'|'no-image-profile'|'failed', message }`.
- L35-39 `wardrobeToolImagesEnabled(settings) = settings?.wardrobeImageSettings?.generateFromTools === true`.
- L41-53 `QueueWardrobeToolImageArgs { userId, chatId, characterId, itemId, requested: boolean|undefined, defaultWhenEnabled, callerContext }`.
- L60-132 `maybeQueueWardrobeToolImage(repos, args) → result | undefined`:
  `enabled = wardrobeToolImagesEnabled(await chatSettings.findByUserId(userId))`;
  `wanted = requested ?? (enabled && defaultWhenEnabled)` (L69);
  `logger.debug('[WardrobeToolImage] Deciding on a tool-queued picture', {callerContext, itemId, enabled, requested, defaultWhenEnabled, wanted})`;
  `!wanted` → `undefined`; `!enabled` → `{ status:'not-enabled', message:'No picture was made: the operator has not allowed the wardrobe tools to generate pictures (Settings → Images → Wardrobe Images).' }`
  (so an explicit `generate_image: true` with the switch OFF is told so and
  spends nothing); `resolveWardrobeImageProfile(userId, repos)` null →
  `{ status:'no-image-profile', message:'No picture was made: no usable image profile is configured.' }`;
  `enqueueWardrobeItemImageGeneration(userId, {chatId, characterId, itemId})` →
  `logger.info('[WardrobeToolImage] Queued a picture for a wardrobe item', {callerContext, chatId, characterId, itemId, jobId, isNew, profileId})`
  → `{ status:'queued', message:'A picture of this item is being drawn in the background. Once it is ready, wardrobe_read and wardrobe_list show its image_file_id.' }`;
  any throw → `logger.error('[WardrobeToolImage] Could not queue a picture for a wardrobe item', {…, error}, err)`
  + `{ status:'failed', message:'No picture was made: queueing the picture failed.' }`.
- L135-137 `formatWardrobeToolImageLine(result) = result ? \`- Picture: ${result.message}\` : null`.
- L144-146 `formatWardrobeImageHandle(id) = \`${id} (pass to describe_image to see it, or keep_image to file it in your album)\``.

### v4 at HEAD — handlers
- `wardrobe-create-handler.ts:28` import; `:189` destructure `generate_image`;
  `:293-300` after the create/equip: `maybeQueueWardrobeToolImage(repos, { userId, chatId, characterId: targetCharacterId, itemId: newItem.id, requested: generate_image, defaultWhenEnabled: true, callerContext: 'wardrobe-create-handler' })`
  (the RECIPIENT for a gift); the success log gains `imageGeneration: status`
  (`:313`); output gains `...(imageGeneration ? { image_generation } : {})`
  (`:332`); `formatWardrobeCreateResults` appends the picture line LAST
  (`:403-404`, after the equipped/not-equipped part).
- `wardrobe-update-handler.ts:33-46 patchChangesLook(item, patch)`: true if
  `patch.title !== undefined && !== item.title`; or `patch.imagePrompt !== undefined && !== (item.imagePrompt ?? null)`;
  or `patch.types !== undefined && join(',') differs`; or
  `patch.componentItemIds !== undefined && join(',') differs from (item.componentItemIds ?? [])`.
  `:83` destructure; `:128-137` `changesLook = patchChangesLook(item, patch)`;
  `maybeQueueWardrobeToolImage(repos, { …, characterId: context.characterId, itemId: updated.id, requested: generate_image, defaultWhenEnabled: changesLook, callerContext: 'wardrobe-update-handler' })`;
  log gains `changesLook`, `imageGeneration` (`:146`); output =
  `{ ...buildWardrobeReadOutput(...), image_generation }` when present
  (`:148-149`); `formatWardrobeUpdateResults` `:172-175` →
  `` `Updated "${title}" (${item_id}).\n${imageLine}` `` or the old line.
- `wardrobe-list-handler.ts:136` `image_file_id: item.imageFileId ?? null`
  per item; `:175` log `withPictureCount`; `:247-249`
  `` pictureTag = image_file_id ? ` · picture: ${formatWardrobeImageHandle(id)}` : '' `` appended AFTER `wearTag`.
- `wardrobe-read-handler.ts:75` `image_file_id` in `buildWardrobeReadOutput`
  (after `image_prompt`, before `types`); `:197` `image_file_id: null` in
  `buildWardrobeReadFailure`; `:279` `` lines.push(`  picture: ${id ? handle : '(none)'}`) `` right after the `portrait cue:` line.

### v4 at HEAD — definitions (model-visible bytes)
- `wardrobe-create-tool.ts:102-110` `generate_image: z.boolean().describe('Whether to have a picture of the new item drawn in the background. Omit to follow the operator\'s setting (on when they allow the wardrobe tools to make pictures). Pictures are only ever made when the operator allows it; the response says what happened.').optional()`
  — placed between `component_titles` and `replace`; `:167` output type;
  description `:184-188` changes `'…(a Portrait Cue) to steer image generation. '`
  → `'…(a Portrait Cue) to steer image generation; if the operator allows it, a picture of the item is drawn in the background (generate_image). '`.
- `wardrobe-update-tool.ts:79-90` `generate_image` describe: `'Whether to have a new picture of the item drawn in the background. Omit to follow the operator\'s setting: when they allow the wardrobe tools to make pictures, a change to how the item looks (title, image_prompt, types or components) redraws it. Pass true to redraw anyway (for instance, to give an item its first picture). Pictures are only ever made when the operator allows it; the response says what happened.'`
  — LAST property (after `component_item_ids`); description `:115-121`
  `'…Only the fields you supply change. If the operator allows it, a new picture of the item is drawn in the background (generate_image). This does NOT put the item on — …'`.
- `wardrobe-list-tool.ts:62` `image_file_id: string | null`; description
  `:120-127`: `'…and an is_own flag (shared archetypes can be worn but not edited), when it was last worn (or that it never has been), and, when the item has a picture, its image_file_id — pass that to describe_image to see what the item looks like. Use wardrobe_wear…'`
  (replaces `'…edited), and when it was last worn (or that it never has been). Use…'`).
- `wardrobe-read-tool.ts:48`; description `:110-116`:
  `'…currently equipped in, its wear history (how often it has been worn, when, and by whom), and its picture\'s image_file_id when it has one (pass that to describe_image to see what it looks like). Items from your own wardrobe, the project, and Quilltap General all resolve.'`.
- Snapshot `lib/tools/__tests__/__snapshots__/tool-definitions-snapshot.test.ts.snap`
  +1868-1871 / +2026-2029: only the two `generate_image` properties (the
  derived OpenAI parameters; descriptions live in the definition JSON).

### What the commit did NOT do
- `wardrobe_wear` / `wardrobe_take_off` / `wardrobe_archive` unchanged; no
  `generate_image` on wear. `wardrobe_read`/`wardrobe_list` TOOL descriptions
  changed but their parameters did not. The job never receives the model's
  `image_prompt` directly — the handler re-reads the item. The create path
  draws by default (when enabled) EVEN when `equip_now` is false.

### v5 main
- `tools/wardrobe_create.rs:37-62 WardrobeCreateToolOutput` (no
  `image_generation`; `target_character_id` `#[serde(skip)]` L57-62 is the
  P4.123 carrier for the recipient — the same carrier the picture needs:
  the executor `tools/executor.rs:1666-1722 run_wardrobe_create` fires
  `trigger_for_characters` at L1711 AFTER the write commits; the queue call
  (async, `&Db`) goes there, not inside the writer closure);
  `:532 format` (append the picture line after L617-619).
- `tools/wardrobe_update.rs:128-250 execute` builds `WardrobePatch` at
  L140-150 (so `patch_changes_look(&item, &patch)` has both operands);
  `:252-259 format` (`Updated "{}" ({}).`); executor `run_wardrobe_update`
  `:1723-1790` (trigger at L1782). The output type is
  `WardrobeReadToolOutput` — v4's intersection type means a NEW
  `WardrobeUpdateToolOutput { read, image_generation }` or an
  `#[serde(skip_serializing_if)] image_generation` on the read struct (v4's
  `buildWardrobeReadOutput` never sets it, so the latter is byte-safe).
- `tools/wardrobe_list.rs:23-39 WardrobeListItemResult` (insert
  `image_file_id: Option<String>` after `image_prompt`, always serialized —
  v4 emits `null`); `:274-344 format` — `cue_tag` L317-319, the line
  L332-341 ends with `description` then (v4) `wearTag` — **v5's list line
  has NO `wearTag` segment** (`last worn` is absent from L332-341); check
  whether the wear-ledger round added it elsewhere before appending
  `pictureTag` — if not, that is a pre-existing drift to bank, not this
  round's fix.
- `tools/wardrobe_read.rs:28-46 WardrobeReadToolOutput`,
  `:76 build_read_failure`, `:100 build_read_output`, `:259-313 format`
  (portrait-cue line L274-279 → insert `picture:` after it).
- `tools/definitions/data.rs:276-292` the four definition JSON literals
  (byte-exact `JSON.stringify`); regenerate from v4 at the pin rather than
  hand-edit (`tool_definitions_equivalence` proves the round trip).
- `describe_image` / `keep_image` exist in v5 (`tools/executor.rs`,
  `tools/definitions/data.rs`, `services/tools_inventory.rs`), so the
  handle's pointer text is truthful.

### Harness
- `wardrobe_tools_equivalence.rs` (`QT_ORACLE_WT`; fixture
  `build-wardrobe-tools-fixture.ts` → `/tmp/qt-wt-{main,mount}.db`; oracle
  `cases/wardrobe-tools.ts`; compares Output + `format*` strings + state) —
  the corpus needs: `generate_image` rows with the switch off/on,
  `patchChangesLook` true/false edits, items with pictures for list/read.
  Mocking: `enqueueWardrobeItemImageGeneration` writes a `background_jobs`
  row (tier-2 diffable — add the table to the state read-back); the switch
  is `chat_settings.wardrobeImageSettings`, so the fixture builder must seed it.
- `tool_definitions_equivalence.rs` (`QT_ORACLE_TOOL_DEFINITIONS` +
  `_CANONICAL`; `npx tsx cases/tool-definitions.ts`).
- `wardrobe_tools_avatar_trigger_equivalence.rs` (the executor-side trigger
  ordering — the picture queue sits beside it).

---

## 12. `b3f937076` — the job: queue, handler, topics, activity, types

### v4 at HEAD
- `lib/schemas/job.types.ts:34` `'WARDROBE_ITEM_IMAGE_GENERATION'` after
  `CHARACTER_AVATAR_GENERATION`, before `CONVERSATION_RENDER`.
- `lib/background-jobs/activity-kinds.ts:67` `WARDROBE_ITEM_IMAGE_GENERATION: 'image'`
  (Img block, after `CHARACTER_AVATAR_GENERATION`, before `CHARACTER_HEADSHOULDERS_BACKFILL`).
- `lib/background-jobs/queue-service.ts:236-247 WardrobeItemImageGenerationPayload { chatId, characterId, itemId }`
  (payload key order chatId, characterId, itemId — L100-104 of the decision
  module builds it in that order); `:1246-1286 enqueueWardrobeItemImageGeneration(userId, payload)`:
  `findPendingForChat(chatId)` → existing where `type === 'WARDROBE_ITEM_IMAGE_GENERATION' && status === 'PENDING' && payload.itemId === itemId`
  (L1253-1256 — **PENDING only**; a PROCESSING job has already built its
  prompt, so a fresh one is enqueued, doc L1241-1244) →
  `logger.info('[WardrobeItemImage] Reusing existing pending job', {context:'background-jobs.queue', chatId, itemId, existingJobId})`
  + `{jobId, isNew:false}`; else `enqueueJob(userId, type, payload, { maxAttempts: 1 })`
  (L1271-1275, "One try: a refusal or a provider failure would only be paid
  for again") + `logger.info('[WardrobeItemImage] Wardrobe item image job enqueued', {context, chatId, characterId, itemId, jobId})`.
  No priority option (default).
- `lib/background-jobs/handlers/index.ts:55` registration;
  `handlers/wardrobe-item-image.ts:35-119 handleWardrobeItemImageGeneration(job)`:
  `logger.info('[WardrobeItemImage] Starting tool-queued wardrobe item image', {context:'background-jobs.wardrobe-item-image', jobId, chatId, characterId, itemId})`;
  `resolveWardrobeItemHome(repos, job.userId, 'character', payload.characterId, payload.itemId)`
  — null → `logger.info('… Item no longer in the character\'s wardrobe; nothing to draw', {jobId, characterId, itemId})` + return;
  `home.item.archivedAt` → `logger.info('… Item archived since the tool call; skipping', {jobId, itemId})` + return;
  `generateWardrobeItemImage(repos, { userId: job.userId, home, containerId: payload.characterId })`
  (no `imageProfileId` → designated/default; `containerId` = characterId,
  which `resolveProjectAestheticMount` ignores for character scope) →
  `logger.info('… Tool-queued wardrobe item image complete', {jobId, itemId, fileId, subject, profileId, rerouted})`;
  catch: `NoWardrobeImageProfileError` → `logger.warn('… No usable image profile; skipping', {jobId, itemId})` return;
  `CharacterArchivedError` → `logger.info('… Owner archived since the tool call; skipping', {jobId, characterId})` return;
  `WardrobeImageGenerationError` → `logger.warn('… Provider would not draw the item', {jobId, itemId, refused, error, trail:[{profileName,outcome}]})` return;
  else rethrow (the runner marks it failed; `maxAttempts: 1` means no retry).
- `lib/realtime/job-topics.ts:66-69`: `case 'WARDROBE_ITEM_IMAGE_GENERATION': return [{ topic: 'characters', id: str(payload,'characterId') }, { topic: 'mountPoints' }]`.
- Host-RPC: §4.

### What the commit did NOT do
- No realtime hint for the CHAT (`chatId` is in the payload but not a topic).
  No per-item dedupe across chats (dedupe is `findPendingForChat` — scoped
  to the chat). No `trackActivity` in the handler (the job runner counts it
  via `JOB_TYPE_ACTIVITY`). No priority. Project/group/General items are
  never drawn by a tool (tools only touch character-owned items, L229-233).

### v5 main
- `services/queue_service.rs:68-84 enqueue_job(db, user_id, type, payload, max_attempts: f64)`
  (so `1.0`); `:578-657 enqueue_character_avatar_generation` is the dedupe
  template (`find_pending_for_chat` → filter by type + payload field; INFO
  lines at L607-614 / L648-656; payload built as an ordered `Map`). v4's
  dedupe here is `status === 'PENDING'` — `find_pending_for_chat` returns
  PENDING + PROCESSING (per the avatar doc L570-571), so the new fn must
  filter `status == "PENDING"` explicitly.
- `services/activity_kinds.rs:80-120 JOB_TYPE_ACTIVITY` — add
  `("WARDROBE_ITEM_IMAGE_GENERATION", Some(ActivityKind::Image))` between
  the avatar and backfill rows (entry ORDER is pinned by
  `activity_tables_equivalence`, which also re-derives totality against v4's
  `BackgroundJobTypeEnum` and v5's enqueue gate — regen required).
- `realtime/job_topics.rs:50-113 topics_for_completed_job` — add the arm
  after `CHARACTER_HEADSHOULDERS_BACKFILL` (L76-79):
  `vec![TopicHint::scoped(Characters, characterId), TopicHint::collection(MountPoints)]`
  (both constructors exist, L109).
- Handler: `services/job_runner.rs:119-123 JobHandler` trait, `:177 register`,
  `:194-195` "No handler registered for job type: …" (an unregistered type
  fails loudly — so the host MUST register it). Host registration site
  `quilltap-host/src/spine.rs:3880-3886` (`"CHARACTER_AVATAR_GENERATION"` →
  `AvatarJobHandler { wire }` constructed per job at L3383-3410 with the
  real provider, moderation, `HostImageCodec`, `now_unix_ms()`,
  `image_declarations_for`, the blob `WebpTranscoder`). A
  `WardrobeItemImageJobHandler` follows that shape and calls the §3
  generation fn with the same deps bundle.
- `api/system_data.rs:658` lists `CHARACTER_HEADSHOULDERS_BACKFILL` — check
  whether that is a job-type census that must gain the new type (grep hit
  only; not read).

### Harness
- `activity_tables_equivalence.rs` (tier-1 static tables; regen at the pin),
  `avatar_job_tier3_equivalence.rs` + `story_background_job_tier3_equivalence.rs`
  (the mocked-provider job precedents; recipe in the oracle headers —
  `avatar-job.test.ts`), a NEW tier-3 `wardrobe_item_image_job_tier3` (mock
  provider → `files` row + link + frontmatter pointer + the skip arms).

---

## 13. `b3f937076` — `POST /api/v1/images?action=generate` `options.orientation`

### v4 at HEAD
- `app/api/v1/images/route.ts:73-82` `options.orientation: z.enum(['portrait','landscape','square']).optional()`
  (after `aspectRatio`; doc: "outranking `size` / `aspectRatio`");
  `:349-352` comment rewritten ("An explicit size is honoured as given unless
  the caller asked for a shape"); `:367` `orientation: options.orientation`
  passed to `buildImageGenParams`.
- `lib/image-gen/params-builder.ts:219-233` (pre-existing): `if (orientation) { resolved = resolveOrientation(provider, model, orientation); if (resolved.params.size) params.size = …; if (resolved.params.aspectRatio) params.aspectRatio = …; if (resolved.promptHint) params.prompt += '\n\n' + hint }`
  — the arm already outranked the raw size; the route simply stopped passing
  `undefined`. `lib/image-gen/orientation.ts:119 resolveOrientation` (model
  support → provider support → host fallback hint L54).

### What the commit did NOT do
- Did not change the builder or the orientation resolver; did not add
  orientation to the Salon tool's schema; the avatar picker's `portrait` is
  SPA-side (`components/images/image-generation-dialog.tsx`).

### v5 main
- `api/images.rs:1-35` header L29-35 and `:1290-1292` state the generate leg
  "resolves NO orientation (`params_builder`'s `orientation: None` arm exists
  for exactly this caller)" — that premise is now FALSE for v4 HEAD; both
  comments move. `parse_generate_body` `:1440-1487` parses `n`, `size`,
  `aspectRatio`, `quality`, `style` — add `orientation` (enum, else the
  flat Zod 400 `bad()`); `GenerateBody` `:1364-1375` gains
  `orientation: Option<Orientation>`; the call site that builds params with
  `orientation: None` (near `:1879`) passes it through.
  `image_gen/params_builder.rs:201-208` already takes `orientation: Option<Orientation>`
  and `:276-300` applies it (with the `size_inserted_by_orientation`
  bookkeeping L289-294).

### Harness
- `images_generate_route_equivalence.rs` (P4.76; `QT_ORACLE_OUT=/tmp/oracle-images-generate-route.ndjson`,
  staged `images-generate-route.test.ts`; compares provider `params`) — add
  `orientation` rows (each of the three values on a provider whose
  `orientationSupport` is `size`, one on `aspectRatio`, one on `prompt`
  fallback, plus `orientation` + explicit `size` to prove the outranking).
  `image_gen_leaves_equivalence.rs` already pins the builder arm.

---

## 14. Host-driver seam + SPA reach (list only)

- Host image generation: `quilltap-host/src/images_generate.rs:89 images_generate_seams`
  (provider + classifier + codec, one bundle); `quilltap-host/src/image_codec.rs`
  (`HostImageCodec`: `PixelCodec` + both `ImageTranscoder`s + thumbnail);
  `quilltap-host/src/spine.rs:3383-3410` (`AvatarJobHandler` — the job-side
  bundle: `RealImageProvider::with_bytes_fetch`, `wire.completion(db)`,
  `RealModerationProvider`, `DbApiKeys`, `HostImageCodec`, `now_unix_ms`,
  `image_declarations_for`, the blob `WebpTranscoder`); `quilltap-host/src/avatar_preview.rs`
  (the preview route's render seam); `quilltap-host/src/host.rs:1298, 1614`
  (boot ensures). The new route AND the new job both need the SAME bundle
  (provider + Concierge wires + codec) — one `wardrobe_item_image_seams`
  constructor on the `images_generate_seams` pattern, reused by the job handler.
- Core traits: `api/images.rs:1315 ImagePromptClassifier`, `:667 ImageImportFetch`;
  `services/image_job_common.rs:485 ProjectImageUpload` (not needed — vault
  writes only); `services/file_storage.rs:303 PixelCodec`.
- SPA image plumbing today (list only; SPA survey's):
  `apps/web/src/app/images/{images.api.ts, image-urls.ts (fileUrl, thumbnailUrl), image-actions.ts, image-gallery.ts, image-modal.ts, image-detail-modal.ts, image-metadata.ts, image-navigation.ts, image-profile-picker.ts, generate-image-dialog.ts, standalone-generate-image-dialog.ts, save-image-dialog.ts, chat-gallery-image-view-modal.ts, deleted-image-placeholder.ts}`,
  `apps/web/src/app/chat/hidden-image/{hidden-image-tile, images-hidden, hidden-inline-image}.ts`,
  `apps/web/src/app/screens/generate-image/generate-image-page.ts`,
  `apps/web/src/app/screens/settings/images/{images-tab, image-profiles-card, image-profile-form, image-profile-modal, image-profiles.api}.ts`,
  `apps/web/src/app/screens/settings/chat/image-description-settings.ts`,
  `apps/web/src/app/screens/prospero/cards/project-image-generation-card.ts`.

---

## 15. Harness families touched — recipe headers (from the test files)

| family | env var | regen (from the pin; `$N=~/.nvm/versions/node/v24.13.1/bin`, `$V5W=~/source/quilltap-v5`) |
|---|---|---|
| `settings_routes_equivalence` | `QT_ORACLE_SETTINGS_ROUTES` | jest real-DB `… QT_ORACLE_OUT=/tmp/oracle-settings-routes.ndjson npx jest -- settings-routes` (header L43-47); fixture `harness/oracle/fixtures/settings.json` |
| `chat_settings_tier2_equivalence` | `QT_ORACLE_CHAT_SETTINGS` | `QT_FIXTURE_OUT=/tmp/qt-chat-settings-fixture.db $N/npx tsx …/fixtures/build-chat-settings-fixture.ts`; `QT_FIXTURE_CHAT_SETTINGS=… $N/npx tsx …/cases/chat-settings-tier2.ts > /tmp/oracle-chat-settings.ndjson` |
| `chat_settings_column_sites_guard` | — | `cargo test -p quilltap-harness --test chat_settings_column_sites_guard` (source census; add the column) |
| `chat_settings_voice_mode_ensure_equivalence` | (see header) | the three-shape ensure precedent (`cases/chat-settings-voice-mode-ensure.ts`, `lib/v4-migrations.ts`) — clone for `wardrobeImageSettings` |
| `provisioning_equivalence` | `QT_ORACLE_PROVISION`, `QT_FRESH_SCHEMA_LIVE` (REQUIRED) | `build-provision-oracle.ts`, `verify-dbkey-crosscompat.ts`, `dump-fresh-schema.ts` (header L34-40) — D23 re-dump #6 |
| `help_tools_equivalence` | `QT_ORACLE_HELP` | `build-help-tools-fixture.ts` then `cases/help-tools.ts` (header L18-22) |
| `almanack_render_equivalence` / `almanack_tier2_equivalence` | `QT_ORACLE_ALMANACK_RENDER` / `QT_ORACLE_ALMANACK_TIER2` | jest mirrors `almanack-render.test.ts` / `almanack-routes.test.ts` (pinned commit recorded in the test, L41) |
| `avatar_job_tier3_equivalence` | `QT_ORACLE_AVATAR` | recipe in the oracle header (`avatar-job.test.ts`, stages `lib/blob-image-facts.ts`) — proves the avatar prompt bytes after the identity-block refactor |
| `wardrobe_tools_avatar_trigger_equivalence` | `QT_WT_AVATAR_SPEC` | `build-wardrobe-tools-fixture.ts` (header L37-39) |
| `wardrobe_tools_equivalence` | `QT_ORACLE_WT` | `QT_FIXTURE_WT_MAIN/MOUNT=/tmp/qt-wt-{main,mount}.db $N/node --import tsx …/fixtures/build-wardrobe-tools-fixture.ts`; `… cases/wardrobe-tools.ts > /tmp/oracle-wardrobe-tools.ndjson` |
| `tool_definitions_equivalence` | `QT_ORACLE_TOOL_DEFINITIONS`, `_CANONICAL` | `npx tsx …/cases/tool-definitions.ts`, `…/tool-definitions-canonical.ts` |
| `activity_tables_equivalence` | (see header) | tier-1 static tables vs v4's real exports |
| `task_type_log_mapping_equivalence` | (see header) | `npx tsx …/cases/task-type-log-mapping.ts` (admittance vs `LLMLogTypeEnum`) |
| `vault_wardrobe_item_file_equivalence` | `QT_ORACLE_VAULT_WARDROBE_ITEM_FILE` | `npx tsx …/cases/vault-wardrobe-item-file.ts` |
| `vault_wardrobe_emit_equivalence` | `QT_ORACLE_VAULT_WARDROBE_EMIT` | `$N/npx tsx …/cases/vault-wardrobe-emit.ts` |
| `vault_wardrobe_public_equivalence` / `vault_wardrobe_write_equivalence` / `wardrobe_tier2_equivalence` | `QT_ORACLE_WPUB` / `QT_ORACLE_VAULT_WARDROBE_WRITE` / `QT_ORACLE_WARDROBE` | builders + cases per headers (L19-23 / L31-36 / L27-32) |
| `wardrobe_transfers_tier2_equivalence` | `QT_ORACLE_WTR` | stage `wardrobe-transfers.test.ts`, `build-wardrobe-transfers-fixture.ts`, jest `-- wardrobe-transfers` (header L48-59) |
| `group_wardrobe_routes_equivalence` | `QT_ORACLE_GROUP_WARDROBE` | header L24-35 |
| `characters_mutations_equivalence` | `QT_ORACLE_CHARACTERS_MUTATIONS` | jest `-- characters-mutations` |
| `quilltap-web/tests/wardrobe_routes_equivalence` | (web) | corpus `wardrobe-routes.json#cases`, oracle `wardrobe-routes.test.ts` |
| `images_generate_route_equivalence` | `QT_ORACLE_OUT=/tmp/oracle-images-generate-route.ndjson` → test env (header L52-58) | staged `images-generate-route.test.ts` |
| `image_failover_tier3_equivalence` | `QT_ORACLE_IMAGE_FAILOVER` | header L36-54 |
| `image_gen_leaves_equivalence` | `QT_ORACLE_IMAGE_GEN_LEAVES` | header L24-32 |
| `help_tree_equivalence` / `help_tree_embed_guard` | — | v4 `help/wardrobe-images.md` (NEW, 37+22 lines), `help/wardrobe.md`, `help/project-wardrobe.md`, `help/image-generation-profiles.md`, `help/profile-avatar.md` re-vendored into `./help/` |

NEW families this round would add: (a) tier-1 `wardrobe_item_image_prompt`
(pure prompt builder), (b) tier-2 `wardrobe_item_images` (the module: list /
add / set-current / delete / cleanup / carry / commit / drop), (c) tier-3
`wardrobe_item_image_generation` (route `generate` + the job handler, mocked
provider), (d) a web/route family for the images route's four actions +
GET, (e) tier-1 `tool_image_generation` (the decision fn over
settings × requested × defaultWhenEnabled, `patchChangesLook`).

---

## P. Premise corrections / surprises

1. **"v5 has no child" is a structural NO-PORT, not a gap.** `b3f937076`'s
   host-RPC `writeWardrobeItemImage` (dispatcher `:92-100`, ipc-types `:161`)
   and the bridge's `QUILLTAP_JOB_CHILD` branch (`:108-116`) exist only
   because v4's job child cannot hold the RW connection. v5's job handlers run
   in-process against the writer; the same ruling already covers the avatar
   and Lantern bridges (`image_job_storage.rs` header). Record it; port nothing.
2. **The migration default and the repo seed disagree in v4 itself.**
   `add-wardrobe-image-settings-field-v1.ts:27` still writes
   `{"imageProfileId":null}`; `chat-settings.repository.ts:222-225` seeds
   `{imageProfileId:null, generateFromTools:false}`. Two JSON shapes reach
   the read path; the v5 typed struct needs `#[serde(default)]` on
   `generate_from_tools` (and on `image_profile_id`), and the boot ensure
   must spell v4's ONE-key default, not the two-key seed. The fresh DDL has a
   THIRD shape (schema-order nullable TEXT, no DEFAULT) — the inform
   `permanent` precedent, three-shape ensure differential.
3. **`chat_settings.rs`'s read is POSITIONAL.** `find_by_user_id` reads
   `storyBackgroundsSettings` at index 32 (`:1505-1507`) and the four
   columns after it by fixed index; inserting `wardrobeImageSettings` in
   schema order shifts them. The `chat_settings_column_sites_guard` exists
   for exactly this; it will not catch a wrong INDEX, only a missing name —
   the tier-2 family will.
4. **`api/images.rs`'s header claim is now false** (L34-35, L1290-1292:
   "resolves NO orientation … exists for exactly this caller"). The builder's
   `orientation: Option` arm (`params_builder.rs:276-300`) was built for this
   caller to pass `None`; v4 now passes the body's value. Comment + parse +
   call-site move together.
5. **v5 has no `resolveWardrobeContainer` port** (v4 `lib/wardrobe/resolve-container.ts`,
   `0506517d3`), so `resolveWardrobeItemHome` cannot be a thin wrapper; the
   transfers service re-derives containers inline (`wardrobe_transfers.rs:277/392/562`).
   A shared resolver is the cleanest seam for the route AND the job handler.
6. **v5 has no create-body fn** — the ledger's "create-body (P4.D163)" has
   no v5 file; `image_file_id: None` lands at FIVE inline create sites (§5).
7. **`avatar-prompt.ts` is a pure refactor and the bytes are provably
   unchanged only by regen** — no standalone avatar-prompt tier-1 family
   exists; `avatar_job_tier3_equivalence` + `wardrobe_tools_avatar_trigger_equivalence`
   are the proof. v5's `avatar_prompt.rs:83-104` inlines only the
   head-and-shoulders ladder; the full-length order is NEW.
8. **The `'wardrobe'` Concierge purpose strings are unreachable as written**
   (`chatId: null` at `item-image-generation.ts:267`; the job handler never
   passes its `chatId`). Port the enum arms and commission strings for type
   totality (`from_wire` round trip), but no differential can observe a
   `'wardrobe'` announcement or ledger row.
9. **The `generate` 400 for "no profile" and the 400 for a missing/unreadable
   API KEY share one sentence** (`NO_WARDROBE_IMAGE_PROFILE_MESSAGE`,
   `item-image-generation.ts:185-189`) — the resolver's `usable` already
   requires `apiKeyId`, so the second throw fires only when the key ROW is
   gone or has no `key_value`.
10. **v4's dedupe is PENDING-only and chat-scoped** (`queue-service.ts:1253-1256`),
    unlike the avatar's PENDING+PROCESSING chat+character dedupe
    (`queue_service.rs:570-571`); `find_pending_for_chat` returns both
    statuses, so the v5 fn must filter `status == "PENDING"` itself.
11. **`parseWardrobeItemFile` treats an EMPTY `imageFileId` as null**
    (`parsers.ts:376-379`) while the neighbouring
    `migratedFromClothingRecordId` keeps any string — do not copy the
    neighbour's arm.
12. **v5's `wardrobe_list` format line has no `wearTag` segment**
    (`tools/wardrobe_list.rs:332-341` ends at `description`; v4 L249 appends
    `${wearTag}${pictureTag}`). Verify the wear-ledger round's `last worn`
    landed in the list formatter before appending the picture tag; if absent
    it is a pre-existing drift to bank separately.
13. **`api/characters.rs:2918` PROPAGATES the equipped-refs cleanup error**
    (`?`) where the other three tiers `let _ =` it (v4 warns and proceeds in
    all four) — pre-existing, out of scope, bank.
14. **No `find_by_linked_to` full-row read exists** (`db/files.rs:470/489`
    are sweep projections). The history read is a new, `FileEntry`-validating
    query (v4 drops invalid rows — `api/images.rs:22-28` records the rule).
15. **`b3f937076`'s plugin dist churn is a rebuild** (no `plugins/src`
    change) and its `package-lock.json` churn is the version bump + optional
    `oxide-wasm32-wasi` entries — neither is drift to port; the ledger
    already says so (`drift-ledger.md:63-65`).
16. `help/wardrobe-images.md` is NEW and five existing help pages moved —
    `help_tree_embed_guard` / `help_tree_equivalence` go red at HEAD until
    `./help/` is re-vendored at the pin (ledger `:71-74` predicted it).
