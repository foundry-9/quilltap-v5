# Survey — the wardrobe CARRIERS one round on: `9ecebf986`'s backup-carrier half + `4a38222e6`'s side channels, `folder`, and the export schema

Read-only planning survey, 2026-10-12. v4 = `~/source/quilltap-server` read ONLY through `git show 4a38222e6:<path>` (the PIN — every v4 line number below is AT `4a38222e6`), `git show <sha> -- <path>` for one commit's hunk, and `git diff 3e6e2f590 4a38222e6 -- <paths>` for the span; never the working tree, never commit prose (ledger §5.3). v5 = `quilltap-v5` main at `f84d34150`. Builds on `2026-10-08-setupphase-wardrobe-carriers.md` (the SAME surface, one round ago — P4.D264 landed everything that survey ordered); what is NOT repeated here is still true there unless this file says otherwise.

**The one-sentence finding:** across the span the carriers' OWN code barely moves — `lib/export/ndjson-writer.ts`, `lib/import/quilltap-import/import-wardrobe-wear.ts`, `import-characters.ts`, `quilltap-import-stream.ts`, `import-wardrobe-images.ts`, `execute.ts`, `lib/backup/restore/uuid-remap.ts`, `lib/backup/uuid-remapper.ts`, `restore/preview.ts` and `lib/export/types.ts` are **byte-for-byte UNCHANGED** between `3e6e2f590` and `4a38222e6` (`git diff --stat` on all ten: empty). What changes their BEHAVIOUR is (a) the ONE predicate they all import, `isWardrobeItemDocumentPath`, going recursive in `parsers.ts`, and (b) `WardrobeItem.folder` appearing on every parsed item (so on every `wardrobe_item` record) and being honoured by the writer every importer/restorer hands items to (`createInMount` → `safeFolder`). The carrier files that DO move are the backup collect (`wardrobeItems` ALWAYS `[]`), the restore's delete service (no wardrobe loop, no count), five `folder: ''` fills, and the export schema's `WardrobeItem.folder`.

The span's carrier-surface stat (`git diff 3e6e2f590 4a38222e6 --stat -- lib/backup lib/export lib/import lib/startup migrations public/schemas lib/wardrobe/legacy-presets.ts`): 17 files, +103 / −28 — `lib/backup/backup-service.ts` (7), `restore/archive.ts` (+1), `restore/delete-service.ts` (5), `restore/restore.ts` (4), `lib/import/quilltap-import/legacy-presets.ts` (+1), `lib/startup/{backfill-group-stores,backfill-project-stores,enqueue-headshoulders-backfill,ensure-group-scenarios,ensure-project-scenarios}.ts` (the `ensureOwnerOfficialStore` / `hasPhysicalDescription` import renames — NOT carriers, listed to be complete), `lib/startup/move-shared-wardrobe-to-general.ts` (+1), `lib/startup/refresh-vault-wardrobe.ts` (3), NEW `migrations/lib/legacy-wardrobe-rows.ts` (68), `migrations/scripts/{collapse-duplicate-avatar-rolls-v1,cutover-characters-to-vault,cutover-projects-to-store}.ts` (import renames / `readLegacyWardrobeRows`), `public/schemas/qtap-export.schema.json` (+5). `lib/export/` does NOT appear in the stat. **`migrations/scripts/index.ts` is untouched → NO new migration in the span; `lib/startup/` adds NO new ensure** (the `restore_vintage_state` mirror-boot list needs nothing — confirmed).

---

## 1. Backup / restore

### v4 at the pin — the `9ecebf986` half (hunks verbatim)

**`lib/backup/backup-service.ts`** (`git show 9ecebf986 -- lib/backup/backup-service.ts`, the whole hunk):
```
@@ -154,7 +154,6 @@ async function collectUserData(userId: string): Promise<Omit<BackupData, 'manife
     pluginConfigs,
     chatSettingsResult,
     folders,
-    wardrobeItems,
   ] = await Promise.all([
@@ -180,9 +179,11 @@
     globalRepos.folders.findByUserId(userId),
-    // Get wardrobe items (composites are wardrobe items now; outfit presets retired)
-    globalRepos.wardrobe.findAll(),
   ]);
+  // Wardrobe items live in document stores (`Wardrobe/*.md`), which the backup
+  // carries as documents; the legacy `wardrobeItems` array stays empty so old
+  // restorers still read the format.
+  const wardrobeItems: never[] = [];
```
At the pin: `:183-186` the comment + `const wardrobeItems: never[] = []`; `:386` `wardrobeItems` still returned (same key position); `:488` `manifest.counts.wardrobeItems: data.wardrobeItems?.length || 0` (ALWAYS 0 now); `:683` `writeJsonArrayFile(… 'data/wardrobe-items.json', data.wardrobeItems || [])` (ALWAYS `[]`); `:296-300` the wear ledger read + `debug 'Collected wardrobe wear ledger for backup' { rowCount }` and `:408` / `:512` / `:723` (`wardrobeWear`, unchanged from last round). **The format is kept, the content is fixed:** every backup from the pin on writes `data/wardrobe-items.json` = `[]` and `counts.wardrobeItems: 0`. The `WardrobeRepository` at the pin (`lib/database/repositories/wardrobe.repository.ts`) has ONLY `findByCharacterId(characterId, includeArchived = false)` `:48`, `readSharedTiers` `:69`, `create(data, options?)` `:105-123` (→ `createInMount(await this.mountFor(item.characterId ?? null, 'create'), item)`), `update(id, data, ownerCharacterId)` `:125`, `delete(id, ownerCharacterId)` `:138` and the private `mountFor` `:142-150` (`logger.error('Wardrobe write has no resolvable folder', { op, ownerCharacterId })` + `throw new Error(\`Cannot ${op} wardrobe item: no Character Vault or Quilltap General mount is available. Wardrobe items are stored exclusively in the document store.\`)`); `findAll`, `findByCharacterIdRaw`, `findByIdForCharacter`, `findByIdsForCharacter`, `findWearablePoolForCharacter`, `findArchetypes*`, `findArchetypeById` are DELETED by `9ecebf986` (the class no longer extends the SQL base — `super('wardrobe_items', …)` is gone).

**`lib/backup/restore/delete-service.ts`** (whole hunk): the destructure `:138` drops the trailing `wardrobeItems`; the `Promise.all` `:151-154` drops `globalRepos.wardrobe.findAll()`; the delete batch `:184-187` drops `...wardrobeItems.map((w) => globalRepos.wardrobe.delete(w.id, w.characterId ?? null))`; the `moduleLogger.info('Deleted existing user data', { userId, deletedCounts: { … } })` at `:225-242` drops `wardrobeItems: wardrobeItems.length` — the object now ends `…, chatSettings: chatSettings ? 1 : 0, folders: folders.length }`. Vault-held wardrobe documents are wiped by `clearFormat3Entities` (`:34-76`: `mainTables` ends `'text_replacement_rules', 'wardrobe_wear_stats'`; then the mount-index truncate unless degraded) — unchanged. The jest mocks at `__tests__/unit/lib/backup/delete-service.test.ts:81` and `lib/backup/restore/__tests__/delete-service-annotations.test.ts:53` STILL stub `wardrobe: { findAll, delete }` — stale, harmless, nothing asserts on them.

**`lib/backup/restore/restore.ts`** (confirmed RENAME ONLY): `:44` `import { updateMountWardrobeItem } from '@/lib/database/repositories/vault-overlay/wardrobe-writes'` (was `updateProjectWardrobeItem`); `:928` `if (await updateMountWardrobeItem(fix.mountPointId, fix.itemId, { imageFileId: fix.imageFileId }))` (22f-ter). Everything else is as last round: `:98-104` the new-account plan, 22f-bis `:891-909` — `for (const item of data.wardrobeItems || []) { try { const { id, createdAt, updatedAt, ...itemData } = item; await globalRepos.wardrobe.create(itemData, { id: item.id }); wardrobeItemsRestored++; } catch … warnings.push(\`Failed to restore wardrobe item "${item.title}": ${msg}\`); moduleLogger.warn('Failed to restore wardrobe item', { wardrobeItemId: item.id, error }) }` — NOTE `itemData` carries whatever `folder` the archive row had (legacy archives: none → `undefined`), and `wardrobe.create` → `createInMount` → `safeFolder(item.folder)` → `''`; summary `:1384` `wardrobeItems: wardrobeItemsRestored` KEPT; `preview.ts:51` `wardrobeItems: data.wardrobeItems?.length || 0` and `:75` `wardrobeWear` KEPT.

### v4 at the pin — the `4a38222e6` half

**`lib/backup/restore/archive.ts`** `:209-242` the legacy `data/outfit-presets.json` fold — the ONE hunk:
```
@@ -233,6 +233,7 @@
         replace: true,
         migratedFromClothingRecordId: null,
         archivedAt: null,
+        folder: '',
         createdAt: preset.createdAt,
         updatedAt: preset.updatedAt,
```
Folded composite key ORDER at the pin: `id, characterId, title, description, types, componentItemIds, appropriateness, isDefault, replace, migratedFromClothingRecordId, archivedAt, folder, createdAt, updatedAt` (`:220-236`). The `info 'Folded legacy outfit presets into composite wardrobe items' { legacyPresetCount, existingWardrobeItemCount }` `:238-241` is unchanged. `:206` `readJsonArrayFileOptional<WardrobeItem>(rootPath, 'data/wardrobe-items.json', [])` is unchanged (the restore side still READS legacy rows — only the backup side stopped writing them).

**`lib/backup/restore/uuid-remap.ts`** — UNCHANGED file; `buildWardrobeItemIdRemap` `:95-122` and `planWardrobeImagePointerFixes` `:147-172` call `isWardrobeItemDocumentPath(link.relativePath)` at `:104` / `:158`, so BOTH now walk `Wardrobe/<any>/<depth>/*.md` links (except under `Wardrobe/images/`, except `instructions.md` at any depth). Behavioural consequences: (i) a NESTED id-less item's ledger `itemId` is now recomputed against the remapped mount (was passed through unchanged); (ii) a nested item's `imageFileId` pointer fix is now PLANNED (was skipped); (iii) `remapper.remap(link.mountPointId)` at `:112` now fires for a mount whose only wardrobe documents are nested → **the counter-id order of every later minted id can move for such an archive** (none committed). The predicate's source: `parsers.ts:257-266`:
```ts
export function isWardrobeItemDocumentPath(relativePath: string): boolean {
  const prefix = `${CHARACTER_WARDROBE_FOLDER}/`.toLowerCase();
  const pathLower = relativePath.toLowerCase();
  if (!pathLower.startsWith(prefix)) return false;
  const rest = pathLower.slice(prefix.length);
  if (rest.length === 0 || !rest.endsWith('.md')) return false;
  const segments = rest.split('/');
  if (segments.some((segment) => segment.length === 0)) return false;
  if (segments.length > 1 && WARDROBE_RESERVED_FOLDER_SEGMENTS.includes(segments[0])) return false;
  return !isWardrobeInstructionsFileName(segments[segments.length - 1]);
}
```
(`WARDROBE_RESERVED_FOLDER_SEGMENTS = ['images']`, `folders.ts:25`; the check runs on the LOWER-CASED first segment, so `Wardrobe/Images/x.md` and `Wardrobe/IMAGES/x.md` are excluded too; `Wardrobe//x.md` is NOT an item; `Wardrobe/Formal/instructions.md` is NOT an item; `Wardrobe/images.md` IS an item — the reserved test needs `segments.length > 1`.) NEW `isWardrobeItemDocument(doc)` `:276-279` (path + `parseWardrobeTypesField(parseFrontmatter(doc.content ?? '').data?.types) !== null`) and `hasStableWardrobeItemId(content)` `:287-290` (`typeof id === 'string' && /^[0-9a-f-]{36}$/i.test(id)`) are the KEYSTONE's (projection sweep / rename); no carrier calls them. `wardrobeItemIdForDocument` `:297-307` and `resolveWardrobeItemId` are unchanged.

**No new restore.ts line, no new delete-service line, no new warning text** in the span → `RESTORE_TS_MESSAGES` (both `harness/oracle/cases/system-restore.test.ts` and `crates/quilltap-harness/tests/system_restore_state.rs:5795`) needs NO new entry. The only log line REMOVED is a key inside `Deleted existing user data`'s `deletedCounts` bag — which v5 does not emit at all today (`delete_all.rs` has no such line — a pre-existing gap, not this span's).

### v5 main (file:line) — what moves
- `crates/quilltap-core/src/services/backup/collect.rs:733` `wardrobe_items: query_all(main, "wardrobe_items", WARDROBE_ITEMS, "", &[])?` → becomes `Vec::new()` with v4's comment; the `WARDROBE_ITEMS` column table `:354-368` goes dead (remove it, or clippy `dead_code`). `manifest.rs:57` `n(c, "wardrobeItems", data.wardrobe_items.len())` and `staging.rs:98` `("wardrobe-items.json", &data.wardrobe_items)` stay as they are (they emit `0` / `[]`). The header's "Thirty-eight collections" prose is still right (the key stays).
- `services/delete_all.rs`: the header's divergence 3 (`:31-37`, "v5 ports the loop faithfully") is now FALSE for v4 at the pin — rewrite it (v4 has no wardrobe step at all); drop `wardrobe_rows` `:178-188`, the read `:353`, and the loop `:474-483` (`delete_vault_wardrobe_item` per SQL row). `DeleteSummary` never had a wardrobe count — nothing else moves. `clear_format3_entities` already carries `"wardrobe_wear_stats"` (`:225`).
- `services/backup/restore/archive.rs:290-348` `fold_legacy_outfit_presets` — insert `m.insert("folder".into(), Value::String(String::new()))` between `archivedAt` (`:339`) and `createdAt` (`:340`) to keep v4's key order (observable only through the uuid-remap `wardrobeItems` pass-through on an outfit-presets archive — none committed — but port it; it is the item the restore hands to `create`).
- `services/backup/restore/orchestrator.rs:2633-2657` 22f-bis builds `crate::vault_overlay::WardrobeItem { … archived_at, created_at, updated_at }` — once the keystone adds `folder` to the struct this constructor must fill it; v4 semantics here are `safeFolder(itemData.folder)` INSIDE `createInMount` (`wardrobe-writes.ts:200-206`), so either v5's `create_vault_wardrobe_item` applies `safe_folder` (the keystone's call) and 22f-bis passes the row's `folder` string as-is (absent → `""`), or 22f-bis normalises — ONE owner, agree with the keystone (recommendation: the writer, as v4). 22f-ter `:2708-2744` (`update_project_wardrobe_item`) is v4's renamed `updateMountWardrobeItem` — a rename in v5 is optional; the signature `(main, links, docs, mount_point_id, id, patch)` (`vault_wardrobe_public.rs:547`) already addresses by mount.
- `services/backup/uuid_remap.rs:533-556` `build_wardrobe_item_id_remap` + `:570-586` `wardrobe_link` + `:633-…` `plan_wardrobe_image_pointer_fixes` — NO code change; they call `crate::vault_overlay::is_wardrobe_item_document_path` (`:42`, `:575`) and inherit the recursion from the keystone's `vault_overlay.rs:1227-1236` rewrite.
- `services/backup/restore/preview.rs`, `restore/mod.rs` (`RestoreSummary.wardrobe_items`), `restore/rows.rs`: untouched.

### Harness (backup/restore)
- `backup_uuid_remap_equivalence.rs` (tier-1 EXACT, `QT_ORACLE_UUID_REMAP`; corpus `harness/oracle/fixtures/uuid-remap-corpus.json` + NDJSON written by `harness/oracle/cases/backup-uuid-remap.test.ts` in one invocation, sha-pinned `:41-47`). **REDDENS at the regen by design, then greens with the keystone alone:** the `wardrobe_wear_path_derived` case (`backup-uuid-remap.test.ts:877-913`) plants `{ id: 'wl-nested', fileId: 'wf-nested', mountPointId: 'mount-hat', relativePath: 'Wardrobe/Old/Coat.md' }` with content `'---\ntitle: Old Coat\ntypes: [top]\n---\n'` (id-less → path-derived) — at the pin v4 now PUTS `oldId → newId` for it into the map (the case's ledger row names `hatItemId` only, so the row bytes do not move, but the pointer plan and the mint ORDER may: `mount-hat` is minted by the `Wardrobe/Hat.md` link first either way, so in THIS case the counter order holds). Its `note` `:879` says "the item-path test is case-insensitive and non-recursive — … a nested Wardrobe/Old/*.md … are not items" — the corpus NOTE must be re-worded at the regen (the oracle writes the corpus; edit the case, not the JSON). The `wide` case (v4's real `createBackup` over `system-data-*`) now carries `wardrobeItems: []` — it already did (the fixture's `wardrobe_items` table holds ZERO rows: `derive-restore-archive-wardrobe.py:157` asserts `counts.wardrobeItems == 0` on `restore-archive.zip`), so no move there. The header's "40 keys" pin (`:21-26`, `:64-65`) is unchanged (`wardrobeItems` key still returned).
- `system_backup_equivalence.rs` (tree diff over `system-data-*`): **does NOT redden** on `wardrobeItems: []` — both sides already emit `[]` / `counts.wardrobeItems: 0` for the committed fixture (above). The ledger's prediction ("`system_backup_equivalence` (`wardrobeItems: []`)") is therefore a port OBLIGATION with no committed tripwire; add a unit test in `collect.rs` that plants a `wardrobe_items` row and asserts the archive carries `[]` (the shape `wardrobe_wear_collect_tests :1146-1235` already uses).
- `system_restore_equivalence.rs` (preview over 29 archives): unchanged (preview keys unchanged).
- `system_restore_state.rs` (tier-2 both modes): `restore-archive-wardrobe-picture.zip` (`derive-restore-archive-wardrobe.py` §2) carries ONE legacy `wardrobeItems` row on Riya — 22f-bis still restores it at the pin; the file lands at `Wardrobe/<title>.md` (root) on both sides; **no flip**, PROVIDED v5's `create` fills `folder` as `''` for a row that has none. The archive-literal set `:725-760` lists `&d.wardrobe_items` — fine. A replace-mode restore over the `system-data-*` substrate: v4 no longer touches `wardrobe_items` SQL rows (none exist) — no flip.
- `restore_vintage_state.rs`: NO new boot ensure in the span (`migrations/scripts/index.ts` untouched) → the boot-align list `:95-146` needs nothing. (The memory note "every new boot ensure must be added" is satisfied vacuously; say so in the lane record.)
- `build-restore-archives.test.ts:65` lists `'wardrobe-items'` among the data files a rebuild writes — a rebuild at the pin still writes `data/wardrobe-items.json` (as `[]`). No rebuild is needed for this span.

---

## 2. `.qtap` export

### v4 at the pin
**`lib/export/ndjson-writer.ts`** — UNCHANGED file. The `wardrobe_item` record (`:244-263`): `globalRepos.wardrobe.findByCharacterId(id, true)` `:250` (archived included — last round); `const { origin: _origin, ...rest } = item` `:254`; `imageFiles = rest.characterId ? await exportedWardrobeImageFiles(globalRepos, rest.id) : []` `:257`; `data = imageFiles.length > 0 ? { ...rest, _imageFiles: imageFiles } : rest` `:258`; `yield { kind: 'wardrobe_item', characterId: id, data }` `:259`; `if (data.characterId) wardrobeItemIds.add(data.id)` `:262`. **The record now carries `folder`** because the item comes off the vault parser whose return object (`parsers.ts:447-464`) is, IN ORDER: `id, characterId, title, description, imagePrompt, types, appropriateness, isDefault, replace, componentItemIds, migratedFromClothingRecordId, imageFileId, archivedAt, folder, createdAt, updatedAt` — `folder` sits between `archivedAt` and `createdAt`, always present (`folderOfWardrobePath(doc.relativePath)` `:428` — `''` for a root item, `A/B` for `Wardrobe/A/B/x.md`; a path outside `Wardrobe/` reads `''`; the prefix test is case-insensitive, the folder keeps STORED casing). A frontmatter `folder:` key is ignored with `logger.debug('Wardrobe/*.md frontmatter carries a \`folder\` key; the file path wins', { characterId, mountPointId, relativePath, frontmatterFolder, folder })` `:430-437` (keystone's parser; recorded here because the export byte stream is where it is observable). `_imageFiles` still lands LAST when non-empty; `origin` is still stripped. The store walk `:728-730` (`if (opts?.wardrobeItemIds && isWardrobeItemDocumentPath(d.relativePath)) opts.wardrobeItemIds.add(wardrobeItemIdForDocument(d))`, AFTER the `doc_mount_document` yield + bump) now collects nested items; `streamWardrobeWear` `:810-834` is unchanged (`warn 'Failed to load wardrobe wear ledger for export' { itemCount, error }`; `debug 'Exported wardrobe wear ledger rows' { itemCount, rowCount }`). Document-store exports still emit NO `wardrobe_item` records (only `doc_mount_document` + the trailing `wardrobe_wear`), so `folder` on the wire appears in CHARACTERS exports only — and for a nested shared item the ledger is now carried where it was dropped before.

**`lib/export/types.ts`** — UNCHANGED (`ExportedWardrobeItem = WardrobeItem & { _imageFiles? }` `:171-173` picks `folder` up from the schema type). No key-order template exists for `wardrobe_item` in v4 (nothing named `schema-key-order` / `keyOrder` anywhere in `lib/`, `public/`, `scripts/`, `__tests__/` at the pin — `git grep` empty); the record is the parser's object as-is.

**`lib/schemas/wardrobe.types.ts`** (`4a38222e6` hunk): `WardrobeItemSchema` gains, after `archivedAt` and before `createdAt` (`:266-272`):
```ts
  folder: WardrobeFolderPathSchema.default(''),
```
with `WardrobeFolderPathSchema = z.string().max(900).refine(isValidWardrobeFolderPath, { message: \`Wardrobe folder must be a clean path of at most ${WARDROBE_FOLDER_MAX_DEPTH} segments, none named "images"\` })` (`:132-137`); `wardrobeItemFieldsSchema.folder: WardrobeFolderInputSchema.optional()` (`z.string().max(900)`, `:143`, `:171`) for the create/update bodies — routes lane. **Consequence for carriers:** `WardrobeItemSchema.parse(...)` of a record WITHOUT `folder` yields `folder: ''`; a record whose `folder` is not in normal form FAILS the refine — but no carrier parses a `wardrobe_item` record through `WardrobeItemSchema` (the import passes `...itemData` straight to `create`, which does NOT Zod-parse; see §3), so the refine is not on any carrier path.

**`public/schemas/qtap-export.schema.json`** — THE hunk, exact (`git show 4a38222e6 -- public/schemas/qtap-export.schema.json`):
```
@@ -1437,6 +1437,11 @@
           ],
           "description": "When the item was archived; null or absent means active. An archived garment is hidden from every wardrobe list and picker unless the surface asks for archived entries, and is never offered to the outfit-selection LLM. It does not undress a character already wearing it."
         },
+        "folder": {
+          "type": "string",
+          "maxLength": 900,
+          "description": "The operator's folder for the item under its wardrobe's Wardrobe/ folder (e.g. \"Formal/Evening\"); empty or absent means the top level. Derived from the item file's path, never stored in its frontmatter. Filing only: it has no bearing on what a character can wear. Importers file the item at the same path, normalising one that is not a clean wardrobe folder (at most 8 segments, none named \"images\") or falling back to the top level."
+        },
         "createdAt": { "$ref": "#/$defs/Timestamp" },
         "updatedAt": { "$ref": "#/$defs/Timestamp" }
       }
```
Position: inside `$defs.WardrobeItem.properties`, immediately after `archivedAt`'s closing `}` and before `createdAt` — in the VENDORED copy (`crates/quilltap-core/src/generators/qtap-export.schema.json`, byte-equal to v4 `01a83539d`: `cmp` clean) that is after line 1438 (`$defs.WardrobeItem` opens at `:1349`, `archivedAt` at `:1433-1438`, `createdAt` at `:1439`). Size: **102,523 → 103,075 bytes (+552)**; `$defs.WardrobeItem` is `additionalProperties: true`, so acceptance changes ONLY for a present `folder` that is not a string or exceeds 900 characters (a tightening, never a new-key refusal). No other schema hunk in the span (the `9ecebf986` commit does not touch the file).

### v5 main
- `services/qtap_export/records.rs:108-132` reads items via `wardrobe_read::find_by_character_id(main, &docs, id, true)` → `vault_read_overlay::read_character_vault_wardrobe :522` → `parse_wardrobe_item_file :547` and emits the read VALUE "NOT reordered" (`:126-132`) — so **`folder` appears on the wire the moment the keystone's parser emits it**, in the right slot, with no carrier edit. `:936-937` the store-walk collector calls `vault_overlay::is_wardrobe_item_document_path` — inherits the recursion. `key_order.rs:110-154` asserts `wardrobe_item` is untemplated — stays true; `schema-key-order.json` needs NO change (v4 has no template either).
- `generators/qtap_schema.rs:188` `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(), 102_523)` and `crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:54` `const VENDORED_BYTES: usize = 102_523` → both **103_075** with a P4.Dxxx comment line in the ladder (`:36-53` / `:170-187` show the shape); `the_embedded_schema_equals_the_v4_checkouts :84-…` byte-compares to `<QT_V4_CHECKOUT>/public/schemas/qtap-export.schema.json` (SKIP without the env) → reddens against a PINNED worktree until re-vendored. **`public_schemas_vendor_guard.rs` does NOT cover this file** (it guards `apps/web/public/schemas/{qtap-custom-tool,qtap-progression}.schema.json`, `:49-58`) — the ledger's `4a38222e6` row names it wrongly, as last round's survey already recorded for `f5e953a3f`.

### Harness (export)
- `system_export_equivalence.rs` (byte-exact per line over `system-data-*`): the fixture's one wardrobe item (`build-system-data-fixture.ts:535-551`, "Travelling Coat" on Lorian, written through v4's `repos.wardrobe.create` → a vault document) → **every `wardrobe_item` line in the characters-type cases gains `"folder":""` between `archivedAt` and `createdAt`** → reddens at the regen, greens with the keystone's parser (no carrier edit). Document-store cases: unchanged (no nested items in the fixture; the one root item's id was already collected).
- `system_import_equivalence.rs` (assembled `{manifest,data}` exact): `data.characters[].wardrobeItems[].folder: ""` — same cause, same cure.
- `qtap_schema_validate_equivalence.rs` (`valid` / `errorSections` over the VENDORED schema; seeds built by v4's exporter+importer at the pin now carry `folder: ''`): `$defs.WardrobeItem` is `additionalProperties: true` on BOTH the old and the new schema → `valid` agrees before and after the re-vendor; no flip expected. The re-vendor is still owed (the two size pins + the checkout compare).
- `qtap_import_equivalence.rs` (the COMMITTED `lorian-and-riya.qtap` + the bug-75/117/158/two-link-blob bundles): unchanged inputs; the imported root items land at the same path → no flip.

---

## 3. `.qtap` import

### v4 at the pin
**The `wardrobe_item` importer** is `importCharacterWardrobeItems` in `lib/import/quilltap-import/import-characters.ts:232-350` — UNCHANGED file. Per item `:306-315` the destructure strips `id, characterId, createdAt, updatedAt, migratedFromClothingRecordId, imageFileId, _imageFiles` and keeps `...itemData` — **`folder` is NOT stripped**, so `globalRepos.wardrobe.create({ ...itemData, componentItemIds: remappedComponentIds, characterId: newCharacterId, migratedFromClothingRecordId: null }, { id: newIdByOldId.get(item.id) })` `:326-333` carries the record's `folder` (or `undefined` for a pre-folders bundle). `wardrobe.repository.ts:105-123 create` spreads `...data`, fills `id` / `characterId ?? null` / `componentItemIds ?? []` / `replace ?? false` / timestamps, and calls `createInMount(mount, item)`. **The filing happens in the writer** (`wardrobe-writes.ts:209-226 createInMount`): `const stored: WardrobeItem = { ...item, folder: safeFolder(item.folder), characterId: mount.characterId }` `:213`, projected via `projectVaultWardrobe` (whose mapper writes `joinFolder(item.folder ?? '', \`${sanitizeFileName(item.title)}.md\`)` — `wardrobe-sync.ts:121`), then `logger.debug('Wardrobe item created in folder', { mountPointId, scope, wardrobeItemId: stored.id, folder: stored.folder, context: 'wardrobe' })` `:218-224`. `safeFolder` `:193-206`:
```ts
function safeFolder(folder: string | null | undefined): string {
  if (!folder) return '';
  if (isSafeWardrobeFolderPath(folder)) return folder;
  const normalised = normalizeWardrobeFolderPathOrRoot(folder);
  logger.debug('Wardrobe write normalised an incoming folder', { folder, normalised, context: 'wardrobe' });
  return normalised;
}
```
with `isSafeWardrobeFolderPath` (`folders.ts:160-166`: `''` → true; `> 900` chars or a `\` → false; any segment empty / untrimmed / `.` / `..` → false; a RESERVED FIRST segment → false — so a hand-made `Hats: Summer` or `Formal/images` is kept AS-IS, `../escape` is not) and `normalizeWardrobeFolderPathOrRoot` (`:123-129` — `normalizeWardrobeFolderPath` `:88-115` in a try, `''` on throw: split on `/` and `\`, trim, drop empties, `.`/`..` → throw, `sanitizeFileName` each segment, a reserved segment → throw, > 8 segments → throw, join with `/`). So an imported `wardrobe_item` with `folder: "Formal/Evening"` lands at `Wardrobe/Formal/Evening/<title>.md` (the projection ensures the subfolder — recursive `projectArrayIntoVaultFolder`, keystone); `folder: "../x"` → `''` + the debug line; `folder: ""` / absent → `''`. ⚠ A NON-STRING `folder` (a crafted record, e.g. `5`) passes `!folder` and reaches `path.includes` → a `TypeError` → caught by the per-item `catch` `:343-…` → `warnings.push(\`Failed to import wardrobe item "${item.title}": path.includes is not a function\`)` — an exotic shape; decide whether to mirror or rule.

**Archetype items** (`!item.characterId`) are still skipped at `:268-275` with `debug 'Skipping archetype wardrobe item during import' { wardrobeItemId, title }`; shared items therefore travel ONLY as `doc_mount_document`s whose path IS the folder — nothing to normalise (the store importer writes documents at their carried `relativePath`).

**`lib/import/quilltap-import/import-wardrobe-wear.ts`** — UNCHANGED (298 lines; the algorithm is in last round's survey §3). `buildImportedWardrobeItemIdMap` `:207-221` skips `!isWardrobeItemDocumentPath(doc.relativePath)` `:213` → a nested shared item's ledger row now maps and imports where it was `droppedMissingItem` before. `importWardrobeWear` `:235-298`, its one warning `Dropped ${n} malformed wardrobe wear-ledger row(s).` `:281`, and `moduleLogger.info('Imported wardrobe wear ledger', { incoming, written, mergedIntoLive, droppedMissingItem, droppedInvalid, foldedWearers, clearedChats, importedItemCount })` `:286-295` are unchanged. `lib/import/quilltap-import-stream.ts:242-253` attaches `wi.data` WHOLE to `parent.wardrobeItems` (so `folder` rides). `import-wardrobe-images.ts`, `execute.ts` (phase 7e `:894-916`, the images step `:979-984`): unchanged.

**`lib/import/quilltap-import/legacy-presets.ts`** `legacyPresetToComposite` `:43-76` — the ONE hunk `+    folder: '',` after `archivedAt: null,` `:72` and before `createdAt` → the folded composite has `folder: ''` explicitly (it then goes through the same `create` → `safeFolder('')` → `''`).

### v5 main
- `services/quilltap_import/ndjson.rs:279-295` attaches the `wardrobe_item` record whole → `folder` rides already.
- `services/quilltap_import/characters.rs:533-560` builds `WardrobeItem { … archived_at, created_at, updated_at }` field by field (the implicit `imageFileId` / `_imageFiles` drop) → once the keystone adds `folder: String`, fill it from `item.get("folder")` (a string → that string; absent/null → `""`; a non-string → decide per the exotic note above). The filing (`safe_folder`) belongs in the keystone's `create_vault_wardrobe_item` (`db/vault_wardrobe_public.rs:496`) / the projection, exactly as v4's `createInMount`; the importer passes the raw value.
- `services/quilltap_import/legacy_presets.rs:49-63` `json!({ … "archivedAt": Value::Null, "createdAt": …})` → insert `"folder": ""` between them (the `json!` macro preserves written order under `preserve_order`).
- `services/quilltap_import/wardrobe_wear.rs:245-293` `build_imported_wardrobe_item_id_map` — NO code change (calls `is_wardrobe_item_document_path` `:263`); `wardrobe_images.rs` — untouched (v4's is).
- `import_warning_text_guard.rs`: no new warning literal in the span; nothing to add.

### Harness (import)
- `wardrobe_wear_import_remap_equivalence.rs` (tier-1 EXACT, `QT_ORACLE_WARDROBE_WEAR_IMPORT_REMAP`; oracle `harness/oracle/cases/wardrobe-wear-import-remap.ts`): the `documents_win_over_scaffold_ids` map case (`:219-233`) plants `{ mountPointId: 'mp-src', relativePath: 'Wardrobe/Old/Coat.md', content: FM(ITEM_B, 'Old Coat') }` `:226` — at the pin v4's map GAINS `ITEM_B → ITEM_B` (frontmatter id, maps to itself) → **reddens at the regen; greens with the keystone's predicate alone** (the family compares the map entry for entry, in insertion order — the new entry lands after `not-a-uuid`'s path-derived pair and before nothing, since `Notes/Hat.md` and `mp-skipped` contribute nothing). The case's comment, if any, about nested paths being ignored must be re-worded. Consider ADDING a `Wardrobe/images/<id>/Coat.md` and a `Wardrobe/Formal/instructions.md` document to the case (v4's own test did exactly that — `import-wardrobe-wear.test.ts:258-259`) so the exclusions are pinned too.
- `system_import_state.rs` (tier-2 all tables, four strategies; payload built by v4's REAL exporter at the pin → `wardrobe_item` records carry `folder: ''`): root items file at the same path either side → **no flip** expected; v5's importer ignoring an unknown `folder` key today gives the same bytes as honouring `''`. The family stays green but proves nothing about filing — add a payload with `folder: "Formal/Evening"` on one item (v4 at the pin writes `Wardrobe/Formal/Evening/<title>.md` + a `doc_mount_folders` row for `Wardrobe/Formal` and `Wardrobe/Formal/Evening`) so the filing is tier-2-proven, and one with `folder: "../x"` for the fallback.
- `qtap_import_equivalence.rs`: unchanged inputs, no flip.

---

## 4. The legacy-row readers / startup movers / migrations lib (the `folder: ''` fills that have NO v5 home)

- **`lib/startup/move-shared-wardrobe-to-general.ts`** (+1: `rowToWardrobeItem` `:90` gains `folder: ''` after `archivedAt`) and **`lib/startup/refresh-vault-wardrobe.ts`** (`:25` imports `readLegacyWardrobeRows` from `@/migrations/lib/legacy-wardrobe-rows`; `:68` `const items = readLegacyWardrobeRows(character.id)` replaces `repos.wardrobe.findByCharacterIdRaw(character.id)`) are v4's ONE-TIME Phase-3.2 startup cutover tasks (flags `shared_wardrobe_moved_to_general_v1` / `wardrobe_folder_migrated_v1` in `instance_settings`). **v5 has never ported either** — `status-log.md:1947-1955` (P4.D119 Tier 2 item 9) records it: "v5 has not ported that migration at all (the migration runner is a standing Phase-4 deferral; `ggrep` over `crates/` finds no counterpart)"; `phase-4.md:482` lists `findByCharacterIdRaw` as "Deprecated pre-cutover read; port only if a consumer materializes". A fresh `ggrep` over `crates/quilltap-host/src` and `services/builtin_mounts.rs` for `shared.wardrobe|legacy_wardrobe|refresh_vault_wardrobe|move_shared` finds nothing. → **NO-PORT, ratify in the lane record** (the standing deferral, unchanged by the span).
- **NEW `migrations/lib/legacy-wardrobe-rows.ts`** (68 lines): `readLegacyWardrobeRows(characterId)` — `getRawDatabase()`; `SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'wardrobe_items'` → `[]` when absent; `SELECT * FROM "wardrobe_items" WHERE "characterId" = ?` → each row mapped to `{ id, characterId, title, description ?? null, types: parseIdList(row.types), componentItemIds: parseIdList(row.componentItemIds), appropriateness ?? null, isDefault: row.isDefault === 1, replace: false, migratedFromClothingRecordId ?? null, archivedAt ?? null, folder: '', createdAt, updatedAt }` (`parseIdList`: `JSON.parse`, arrays filtered to strings, else `[]`). Consumers: `refresh-vault-wardrobe.ts` and `migrations/scripts/cutover-characters-to-vault.ts:594` — both the migration runner's territory → **NO-PORT** (same deferral). v5's `db/wardrobe.rs` (the Phase-2 SQL repo over `wardrobe_items`, header `:1-60`) is NOT a consumer of this and needs nothing; it stays as the tier-2 marshaling bank it is.
- `migrations/scripts/collapse-duplicate-avatar-rolls-v1.ts` (`lib/wardrobe/avatar-cache` → `lib/image-gen/avatar-cache` import) and `cutover-projects-to-store.ts` (a comment) — import renames; NO-PORT.
- `lib/startup/backfill-{group,project}-stores.ts`, `ensure-{group,project}-scenarios.ts` (`ensureOwnerOfficialStore('group'|'project', id, name)` replacing the two per-tier ensurers), `enqueue-headshoulders-backfill.ts` (`hasPhysicalDescription(pd, 'scene')`) — `9ecebf986`'s refactor plumbing; the behaviour at these sites is unchanged by the rename; the `hasPhysicalDescription` predicate is the PROMPTS/images lane's (`lib/characters/physical-description.ts`). Not carriers.

---

## 5. Every line / text to carry VERBATIM (this span, carrier-reachable)

New in the span and reachable from a carrier path (all are the KEYSTONE's code, observable through import/restore):
- DEBUG `Wardrobe write normalised an incoming folder` `{ folder, normalised, context: 'wardrobe' }` — `wardrobe-writes.ts:204` (`safeFolder`), fired by an import/restore `folder` that is not structurally safe.
- DEBUG `Wardrobe item created in folder` `{ mountPointId, scope, wardrobeItemId, folder, context: 'wardrobe' }` — `:218-224` (was `… created in folder` without `folder` before `4a38222e6`; `9ecebf986` had already retitled it from INFO `… in vault`).
- DEBUG `Wardrobe items updated in folder` `{ mountPointId, scope, wardrobeItemIds, count, movesFolder, context: 'wardrobe' }` — `:306-313` (`updateManyUnserialized`; 22f-ter's `updateMountWardrobeItem` → `updateInMount` → one-patch batch → THIS line replaces the old `Wardrobe item updated in folder`).
- DEBUG `Wardrobe/*.md frontmatter carries a \`folder\` key; the file path wins` `{ characterId, mountPointId, relativePath, frontmatterFolder, folder }` — `parsers.ts:430-437` (every reader, hence the export).
- DEBUG `Wardrobe read skipped non-item documents` `{ characterId, mountPointId, skipped }` — `vault-readers.ts:353-358` (fires when `Wardrobe/instructions.md` or `Wardrobe/images/**` exists — i.e. on the export of ANY vault with dressing instructions).
- DEBUG `Recursive vault projection ensured subfolders` `{ characterId, mountPointId, folder, subfolderCount }` — `vault-projection.ts:97-104` (an import filing into a subfolder).
Unchanged but confirm still pinned: `Collected wardrobe wear ledger for backup`, `Folded legacy outfit presets into composite wardrobe items`, `Failed to restore wardrobe item "<title>": <msg>` + WARN `Failed to restore wardrobe item` `{ wardrobeItemId, error }`, `Failed to repoint a wardrobe item's picture (<itemId>): <msg>`, `Repointed wardrobe item pictures after new-account restore`, `Failed to load wardrobe wear ledger for export`, `Exported wardrobe wear ledger rows`, `Dropped N malformed wardrobe wear-ledger row(s).`, `Imported wardrobe wear ledger`, `Skipping archetype wardrobe item during import`, `Failed to import wardrobe item "<title>": <msg>`, `Wardrobe item "<title>" referenced N component item(s) not present in the import; those references were dropped.`
Removed: the `wardrobeItems` key from `Deleted existing user data`'s `deletedCounts` (v5 emits no such line — nothing to remove).
No NEW warning text (the `import_warning_text_guard` census is unaffected).

---

## 6. v4 jest coverage at the pin (names + case counts; `it(`/`test(` grep)

Changed in the span:
- `__tests__/unit/lib/backup/uuid-remapper.test.ts` — **56 cases**; `4a38222e6` adds `describe('remapBackupData() - wardrobe items filed in subfolders')` `:940-1008` (2 cases): a `Wardrobe/Formal/Evening/Coat.md` id-less item's ledger `itemId` is recomputed against the remapped mount (`newItemId !== oldItemId`, `result.wardrobeWear[0].itemId === newItemId`); `planWardrobeImagePointerFixes` returns ONE fix for the nested item (`sourceMountPointId: 'mount-old'`, `imageFileId: mapping['file-old']`). The mock keeps `crypto`'s real `createHash` and uses `jest.requireActual` for `wardrobeItemIdForDocument`.
- `__tests__/unit/lib/export/ndjson-writer.test.ts` — **18 cases**; the store-ledger case is RETITLED `emits the ledger for a store's Wardrobe items only — not notes, pictures or instructions` (`:551`) and its plants change: `Wardrobe/Old/Coat.md` (formerly a NON-item) is REPLACED by `Wardrobe/Formal/instructions.md` (null id) and `` `Wardrobe/images/${HAT_ID}/Hat.md` `` — `findRowsForItems` still asked for `[SCARF_ID]` only; NEW `emits the ledger for an item filed in a wardrobe subfolder` `:575-594` (`Wardrobe/Formal/Evening/Coat.md` → `findRowsForItems([SCARF_ID, COAT_ID])`, two `wardrobe_wear` records).
- `__tests__/unit/lib/import/quilltap-import/import-wardrobe-wear.test.ts` — **15 cases**; the "ignores" case swaps `Wardrobe/Old/Coat.md` for `Wardrobe/Old/instructions.md` + `` `Wardrobe/images/${SRC_ITEM}/Coat.md` `` (`:258-259`, `map.size === 0`); NEW `maps an item filed in a wardrobe subfolder` `:265-271` (`Wardrobe/Formal/Evening/Coat.md` → `map.get(SRC_ITEM) === SRC_ITEM`).
Unchanged, still the carrier contract: `wardrobe-wear-backup.test.ts` (4), `delete-service.test.ts` (6), `restore-service.test.ts` (13), `chat-informs-backup.test.ts` (6), `backup-service-streaming.test.ts` (7), `compact-backup.test.ts` (4), `delete-service-annotations.test.ts` (2), `character-vault-export.test.ts` (11), `ndjson-roundtrip.test.ts` (11), `import-characters-vault.test.ts` (4 — `idMaps.wardrobeItems.get('source-coat')` is the minted id), `import-wardrobe-images.test.ts` (6). The keystone-side tests the carriers LEAN on: `__tests__/unit/lib/wardrobe/folders.test.ts` (19 — incl. `normalizeWardrobeFolderPathOrRoot('Formal/images') === ''` `:76`, `isSafeWardrobeFolderPath` `:130-138`: `''`, `Hats: Summer`, `Formal/images` true; `../escape` false), `database/repositories/wardrobe-folder-writes.test.ts` (12), `wardrobe-folder-read.test.ts` (4), `vault-projection-recursive.test.ts` (9), `wardrobe/archived-survives-projection.test.ts` (8). No backup/import/export test file is NEW in the span.

---

## 7. Harness families predicted to redden at the `4a38222e6` regen, and why

| Family | Red? | Cause | Cure |
|---|---|---|---|
| `backup_uuid_remap_equivalence` | YES (corpus-hash + the `wardrobe_wear_path_derived` map / plan) | nested `Wardrobe/Old/Coat.md` now an item | keystone predicate; re-word the case `note` `:879` |
| `wardrobe_wear_import_remap_equivalence` | YES (`documents_win_over_scaffold_ids` gains `ITEM_B → ITEM_B`) | same | keystone predicate; add the `images/` + nested `instructions.md` plants |
| `system_export_equivalence` | YES (every `wardrobe_item` line, characters cases) | `"folder":""` between `archivedAt` and `createdAt` | keystone parser (`folder` on the parsed item) |
| `system_import_equivalence` | YES (`wardrobeItems[].folder`) | same | same |
| `qtap_schema_embed_guard` (2 pins) + `generators/qtap_schema.rs:188` | YES | 102,523 → 103,075 | re-vendor the schema from the PIN, bump both literals |
| `qtap_schema_validate_equivalence` | no | `$defs.WardrobeItem` `additionalProperties: true` | — (re-vendor anyway) |
| `system_backup_equivalence` | no | the fixture's `wardrobe_items` table is EMPTY (`derive-restore-archive-wardrobe.py:157`) | add a planted-row unit test in `collect.rs` |
| `system_restore_equivalence` / `system_restore_state` / `restore_vintage_state` | no | preview keys unchanged; 22f-bis unchanged; no new ensure | fill `folder` on 22f-bis' constructor when the struct gains it |
| `system_import_state` / `qtap_import_equivalence` | no | root items file identically | grow the payload with a filed item + a `../x` fallback |
| `vault_wardrobe_item_file_equivalence` (`Wardrobe/sub/Coat.md` `:124`), `vault_wardrobe_read_equivalence`, `vault_wardrobe_write_equivalence`, `vault_wardrobe_emit_equivalence`, `vault_wardrobe_public_equivalence` | YES | the keystone's parser / reader / projection | the keystone's |
| `help_tree_embed_guard` (131 at `01a83539d`) | content moves on `help/wardrobe.md` / `project-wardrobe.md` (count unchanged) | `9ecebf986` + `4a38222e6` | the help/docs lane |

Regen rule: PIN REQUIRED at `01a83539d` today (ledger §1); this round's regens run from a pinned worktree at `4a38222e6` (`--v5w ~/source/quilltap-v5`, Python 3.13 for the sweep driver). `system-import-execute.test.ts` builds payloads from v4's REAL exporter, so `folder` rides into the import corpora automatically.

---

## 8. What this lane NEEDS from the keystone (the `vault_overlay` / `vault_wardrobe_write` lane)

1. `vault_overlay::is_wardrobe_item_document_path` REWRITTEN to v4 `parsers.ts:257-266` (recursive; `images` as a lower-cased reserved FIRST segment with `segments.length > 1`; empty segment → false; `instructions.md` by LAST segment). All four v5 carrier call sites (`uuid_remap.rs:42,575`, `records.rs:936`, `wardrobe_wear.rs:27,263`) inherit it with no edit. Also `wardrobe_item_id_for_document` unchanged.
2. `vault_overlay::WardrobeItem` gains `folder: String` (serde name `folder`, ALWAYS serialized — v4's `.default('')`), declared between `archived_at` and `created_at`; `parse_wardrobe_item_file` fills it from `folder_of_wardrobe_path(relative_path)` (`folders.ts:214-220`: case-insensitive `Wardrobe/` prefix; `''` outside it; the text after the prefix up to the LAST `/`, stored casing) and emits the `folder key` debug line. This is what puts `folder` on the `wardrobe_item` export record and makes `from_read_value` carry it for transfers.
3. The writer's `safe_folder` (v4 `wardrobe-writes.ts:200-206`) INSIDE `create_vault_wardrobe_item` / the update path, built on `is_safe_wardrobe_folder_path` (`folders.ts:160-166`) and `normalize_wardrobe_folder_path_or_root` (`:123-129`, over `normalize_wardrobe_folder_path` `:88-115` + `sanitize_file_name` — v4 moved `sanitizeFileName` to `lib/mount-index/sanitize-file-name.ts`, same body). The carriers (import `characters.rs`, restore 22f-bis, the legacy-preset folds) then pass the record's `folder` RAW; one owner of the rule, as in v4. If the keystone instead expects callers to normalise, say so in the HANDOFF — the four constructors in this lane are the callers.
4. The recursive projection (`project_array_into_vault_folder` with `recursive: { exclude_subfolders: ["images"] }` + `owns_file: is_wardrobe_item_document` + the lower-cased sweep compare — `vault-projection.ts` hunk) so an imported `folder: "Formal/Evening"` actually lands at `Wardrobe/Formal/Evening/<title>.md` with the subfolder rows ensured. Without it the tier-2 filing row in §3 cannot be green.
5. `is_wardrobe_item_document` / `has_stable_wardrobe_item_id` — NOT needed by any carrier (projection sweep / rename only); listed so nobody adds them here.

Nothing in this lane needs the routes lane (the five folder actions), the tools lane (`[folder: …]`), the transfers lane (`normalizeWardrobeFolderPathOrRoot` on `component.folder` / `source.item.folder`, `transfers/route.ts:301,311`), or the SPA.

---

## 9. Cross-lane meeting points

- **`vault_overlay::WardrobeItem { folder }`** — every constructor in THIS lane must compile the day the keystone lands: `restore/orchestrator.rs:2635-2656` (22f-bis), `quilltap_import/characters.rs:533-558`, plus the two `Value` folds `restore/archive.rs:339-340` and `quilltap_import/legacy_presets.rs:60-61` (insert the key). Sequence this lane AFTER the keystone or land a one-line `folder: String::new()` fill at unification.
- **`db/vault_wardrobe_public.rs`** is a CONSUMER here only (`create_vault_wardrobe_item :496`, `update_project_wardrobe_item :547`, `WardrobePatch :91`); the `updateMountWardrobeItem` rename is cosmetic — do not touch the file from this lane.
- **The export schema re-vendor** (`generators/qtap-export.schema.json` + the two 102_523 pins) is THIS lane's (the schema hunk is `4a38222e6`'s only carrier-schema move); `help/` + `docs/v4/` re-vendors are the docs lane's (`help/wardrobe.md`'s tags line gains `folders`, the +35 folders section; `project-wardrobe.md`; `API.md`; `features/complete/wardrobe-folders.md`).
- **`services/delete_all.rs`** — also the restore's replace-mode wipe; the loop removal touches no other lane's file but the header's divergence list is read by `restore` reviewers — record the ruling.
- **The oracle cases** `backup-uuid-remap.test.ts` and `wardrobe-wear-import-remap.ts` are regenerated by THIS lane (the nested plants flip); `vault-wardrobe-item-file.ts` (`Wardrobe/sub/Coat.md`) by the keystone. One regen per family (the standing rule); the uuid-remap corpus is sha-pinned, so the NDJSON and the corpus move together.
- **`system_import_state` payload growth** (a filed item + a `../x` fallback) depends on the keystone's recursive projection being on main — stage it as a HANDOFF to unification if the lanes run in parallel.

---

## 10. Premise corrections / surprises (vs the task's framing and the ledger)

1. **`lib/backup/uuid-remapper.ts` did NOT change** — `buildWardrobeItemIdRemap` lives in `lib/backup/restore/uuid-remap.ts` and is byte-identical across the span; it "became recursive" only through `parsers.ts`'s predicate. Likewise `ndjson-writer.ts`, `import-wardrobe-wear.ts`, `import-characters.ts`, `quilltap-import-stream.ts`: unchanged. **v5's three side channels need NO code edit** for the recursion — only the keystone's predicate — and the harness proves it (the two families redden on the plants and green on the predicate alone).
2. There is NO `safeFolder` in any importer — it is the WRITER's (`wardrobe-writes.ts:200`), reached by import / restore / transfers / routes alike. The importer's only contribution is NOT stripping `folder` from `...itemData`.
3. `system_backup_equivalence` does NOT redden on `wardrobeItems: []`: the committed `system-data-main.db` has an EMPTY `wardrobe_items` table (the fixture's one item was written as a vault document; `derive-restore-archive-wardrobe.py:157` asserts `counts.wardrobeItems == 0`). The change is a real port obligation (read nothing, write `[]`) with no committed tripwire — add one.
4. The ledger's `4a38222e6` row predicts `public_schemas_vendor_guard` reddening — wrong file; the pins are `qtap_schema_embed_guard.rs:54` and `generators/qtap_schema.rs:188` (as last round's survey §4.2 already said).
5. The five `folder: ''` fills split: two are carriers (`restore/archive.ts`, `legacy-presets.ts` → v5 `archive.rs`, `legacy_presets.rs`); three are v4's one-time startup/migration readers (`move-shared-wardrobe-to-general.ts`, `legacy-wardrobe-rows.ts`, via `refresh-vault-wardrobe.ts`) with NO v5 counterpart — NO-PORT under the standing migration-runner deferral (`status-log.md:1947-1955`).
6. `WardrobeItemSchema.folder` has a `.refine` that REFUSES a non-normal path — but no carrier parses through the schema; the carriers' contract is `safeFolder` (keep-if-safe, else normalise, else root), which is what the export schema's description promises importers do. A `.qtap` `folder` of `"Hats: Summer"` is KEPT verbatim (structurally safe), not sanitised — do not "fix" that in v5.
7. The 22f-bis constructor in v5 mints `created_at`/`updated_at` `now()` (`orchestrator.rs:2655-2656`) — v4 strips the row's `createdAt`/`updatedAt` and lets `create` mint them (`restore.ts:901-902`, `wardrobe.repository.ts:120-121`): same shape, confirmed unchanged.
8. The span adds NO migration, NO boot ensure, NO new restore.ts / delete-service line, NO new warning — the restore census, the vintage mirror-boot list and the import-warning guard all stand as they are.
9. `Deleted existing user data` (delete-service's INFO with `deletedCounts`) has never been emitted by v5's `delete_all.rs` — a pre-existing omission the span merely shrinks by one key; not this lane's to add unless ordered.
10. The export schema's size at the pin is **103,075** bytes (v4 `4a38222e6:public/schemas/qtap-export.schema.json | wc -c`); the vendored copy is byte-equal to `01a83539d` (102,523). `545e299bb` (scenarios, UNPROCESSED, a later round) moves the same file again — vendor from THIS pin, not HEAD.

---

## 11. Suggested shape of the work order (for the planner)

Small lane, mostly verification + four fills + one re-vendor, sequenced AFTER (or with a HANDOFF to) the keystone:
1. `collect.rs:733` → `Vec::new()` + v4's comment; drop `WARDROBE_ITEMS`; a planted-row test proving `[]`.
2. `delete_all.rs`: drop `wardrobe_rows` + the loop; rewrite divergence 3 in the header.
3. `archive.rs` fold + `legacy_presets.rs`: `"folder": ""` in slot.
4. 22f-bis + `characters.rs` importer: fill `folder` from the row / record (raw; the writer normalises).
5. Re-vendor `qtap-export.schema.json` from `4a38222e6`; 102_523 → 103_075 in both pins with the ladder comment.
6. Regen (from the pin) `backup-uuid-remap`, `wardrobe-wear-import-remap` (re-word the nested notes; add the `images/` + nested `instructions.md` exclusion plants), `system-export`, `system-import`, `qtap-schema-validate`; confirm `system-backup`, `system-restore*`, `system-import-state`, `qtap-import` green unchanged; grow `system_import_state` with a filed item + a `../x` row once the keystone's projection is on main (HANDOFF).
7. Ratify NO-PORT: `move-shared-wardrobe-to-general.ts`, `refresh-vault-wardrobe.ts`, `migrations/lib/legacy-wardrobe-rows.ts`, the three `migrations/scripts` import renames, the five `lib/startup` renames.
