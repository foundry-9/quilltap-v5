# Survey — the Inventory KEYSTONE, server side: storage, data model, the THIRTEENTH + FOURTEENTH D23 moves, routes, the chat spine's `## Carried and equipped`, the image phrase, the carriers, and the title lint

Read-only planning survey, **2026-10-20**. v4 pin **`322a5488e`** (v4 `main`,
2026-10-20, "Fix: custom-tool LLM consult can narrate a verdict the table
already reached (bug 257)", `4.11.0-dev.53`; the tree IS at the pin, so
`git show 322a5488e:<path>` equals the working file — every v4 line number
below is AT THE PIN). Oracle baseline **`e8e17b2d9`** (what v5 has ported). v5
`main` at **`5a94a9d1e`**. Nothing was built, run or written in either repo
except this file. Port facts come from SHIPPED HUNKS, never commit prose.

**This slice:** `4fbdad186` ("Feature: The Inventory — objects a character
carries (#100)") EXCEPT its SPA files (`components/**`, `app/**/*.tsx`,
`lib/hooks/*`) and EXCEPT the tools / Pascal half (the sibling "Inventory tools
+ Pascal" survey); PLUS `102a23648`'s non-tool lib hunks (`lib/inventory/*`,
`lib/projects/roster-access.ts`, `lib/chat/context-manager.ts`,
`lib/inventory/roster-gate.ts`, bug 253 — bug 254 is ALREADY PORTED as
P4.D322); PLUS `7263105c6`'s Inventory title-lint half (`lib/inventory/
title-lint*.ts`, `lint-scope.ts`, `item-mutations.ts`, `pool.ts`,
`inventory-writes.ts`, `lib/mount-index/database-store.ts`, `lib/pascal/
tool-gate-keys.ts`) but NOT its bug-256 half (the smalls survey); PLUS
`d88c1ce39` (NO server hunk — see §0); PLUS the FOURTEENTH move's migration and
schema hunk from `e1872c94f` (the keystone owns `fresh_schema.json` and the boot
ensures). It builds on the ledger's §1 "Schema / D23" block
(`drift-ledger.md:212-290`) and §3 rows `:401` (`4fbdad186`), `:402`
(`e1872c94f`), `:403` (`102a23648`), `:407` (`7263105c6`), `:421`
(`d88c1ce39`) — where a hunk disagrees with a row, §8 says so.

Shape follows `surveys/2026-10-19-setupphase-outfit-history-keystone.md` and
`surveys/2026-10-12-setupphase-wardrobe-keystone-location-pool-folders.md`.

---

## Headline findings (written last)

1. **ONE `fresh_schema.json` re-dump carries BOTH moves and nothing else of
   this slice:** `chats."equippedInventory" TEXT` (cid 66, between
   `equippedOutfit` and `characterAvatars`, no DEFAULT) and
   `characters."canManageInventory" INTEGER DEFAULT NULL` (between
   `canCreateOutfits` and `canChooseOutfit`). Both migrations
   (`add-chat-equipped-inventory-v1`, `add-character-inventory-flag-v1`) are
   column-gated `addColumnIfMissing` appends with `TEXT DEFAULT NULL` /
   `INTEGER DEFAULT NULL` — the P4.D249 / P4.D304 two-shape class; the boot
   ensure is the `chat_settings_4_11_repair.rs` twin (UNSTAMPED, FATAL, ONE
   chain, three homes). ⚠ The same re-dump WILL also carry `log_entries`
   (the dumper drives `logEntries.count()` — `lib/database/repositories/
   index.ts:130,195` at the pin — which lazily ensures the table) and
   `terminal_sessions.cols/rows`: the orchestrator must rule whether the
   keystone's dump is taken at the pin WITH those blocks (and the Log / bug-255
   lanes consume them) or the blocks are hand-pruned. §3, §8 Q1.
2. **The objects are FILES, not rows** (`Inventory/**/*.json`, one per object,
   `JSON.stringify(out, null, 2) + '\n'` in `INVENTORY_FILE_KEYS` order),
   projected whole through `projectArrayIntoVaultFolder` with a NEW `ext:
   '.json'` option — v5's `project_array_into_vault_folder`
   (`db/vault_wardrobe_write.rs:98`) hard-codes `.md` (`:124`) and must grow the
   option. The whole folder machinery was LIFTED out of the wardrobe into
   `lib/mount-index/item-folders.ts` + `vault-overlay/item-folder-ops.ts`
   (strings byte-identical, `noun` parameterised); v5's `wardrobe_folders.rs` /
   `vault_wardrobe_public.rs:666-873` are the pre-lift copies — the lane
   generalises them (an `ItemFolderRules` struct) rather than copying. §1.4, §2.
3. **The chat spine hunks are small and exactly placed:** `## Carried and
   equipped` is a TRAILING section built on EVERY turn (bug 253 — ungated),
   after progressions and before the inform block
   (`context-manager.ts:2190-2193, 2262-2266`); Carina joins `[question,
   progressions, inventory]` with `\n\n---\n\n` (`carina.service.ts:595-598`);
   the greeting carries it in the USER turn as `<instruction>\n\n---\n\n<sections>`
   through a NEW `trailingContextSections` (`initial-greeting.ts:160-176`,
   `chats/route.ts:786-788, 877`). Arrival defaults write `{charId: []}` EVEN
   WITH NO INVENTORY at chat create / seat add / merge (`apply-defaults.ts:
   144-148`) — a DB-state change the ported `chat_create_capstone`,
   `chat_cast_routes` and chat-merge tier-2s WILL see. §1.6, §4.
4. **Lane-cut forcing facts:** `equip.ts` / `equip-effects.ts` / `pool.ts` /
   `errors.ts` / `item-mutations.ts` / `context.ts` were each grown by THREE of
   the four commits — ONE lane must own each file WHOLE at the pin (§5 assigns
   them to the keystone, with `pendingInventoryAnnouncements` /
   `flushPendingInventoryAnnouncements` as a Shared contract the tools lane
   calls); the title lint (`7263105c6`) imports `resolveInventoryRef`
   (`refs.ts`, tools lane) AND `collectToolGateKeys` (`lib/pascal/
   tool-gate-keys.ts` over `listAllCustomTools`'s NEW `unread` /
   `storesConsulted` / `CustomToolLoadError.stage`), so the lint is a
   TRAILING lane behind BOTH the keystone and the tools/Pascal lane. §5.
5. **Two runtime breaks for existing oracles:** (a) `harness/oracle/fixtures/
   build-context-tier3.json` op `progressions_continue_mode` was already
   re-pinned by P4.D322, but EVERY `build_context_tier3` / `orchestrator_tier3`
   / `carina_query_tier3` / `initial_greeting` / `chat_create_capstone` case
   now runs `buildInventorySection` → `equippedIdsOf(chat)` on v4's side —
   `[]` on every existing fixture (no `equippedInventory` cell) so NO byte
   moves until a plant carries one; (b) v4's `applyInventoryDefaults` at chat
   create / seat add / merge writes `equippedInventory = {"<charId>": []}` on
   EVERY real-DB case that creates a chat with a character — the
   `chat_create_capstone_equivalence`, `chat_cast_routes_equivalence`,
   `chats_participants_tier2` and merge families go RED at the regen until v5
   writes the same cell. §4, §6.
6. **The positional-marshal trap:** v5's `chats_read.rs` SELECTs its own
   column list and marshals by `row.get(N)` (`:368` `equippedOutfit` = 64, the
   post-DDL additions appended at 99-102); `characters_read.rs` the same
   (`canCreateOutfits` = 19). APPEND `equippedInventory` / `canManageInventory`
   at the END of each list (the P4.D171 / P4.D179 precedent), never at DDL
   position, or forty marshal indices shift. §6.
7. **`SELECT *` sites measured:** `db/chats.rs:1709` (the moderation-refusal
   ledger reads by `column_names()` position — tolerant) and
   `db/store_name_reconcile.rs:687` `read_vault_owner_rows` (`SELECT * FROM
   "characters"` hydrated by name with `CHARACTER_BOOLEAN_COLUMNS` — a NEW
   `canManageInventory` INTEGER cell hydrates as a NUMBER unless the name is
   added to that boolean list; v4 reads it as a boolean through Zod). §2.1, §6.
8. **Already ported, say what remains:** P4.D322 landed the archive keep
   (`KEPT_FOLDERS [Wardrobe, Inventory]`, `character_archive/service.rs:
   282-295`) and bug 254's progressions guard. What remains of
   `archive-service.ts` is NOTHING; of `context-manager.ts` it is the
   `inventoryLLMContext` build + its slot in the trailing list + the
   `isContinueMode` doc comment's "inventory" word. §2.
9. **Tier-3 explicit refusals this round:** `?action=analyze-image` has no
   Inventory twin (nothing to refuse); the Inventory images route's
   `?action=generate` is SYNCHRONOUS in the route (as the wardrobe's is in v5
   — `wardrobe_item_image_generation.rs`), so no job-runner work. Nothing in
   the slice warrants a `not_available` refusal; the one candidate — the title
   lint's `AsyncLocalStorage` scope — ports as an explicit `linted: bool`
   argument (§6 T9), not a refusal.
10. **Rulings owed (§8):** the re-dump's extra blocks (Q1); whether the
    Inventory lane ALSO owns the e1872c94f `characters` PUT key + export schema
    property + import/restore field (`canManageInventory`) since they are
    column plumbing (Q2 — recommended YES); the `runSerialized` per-folder
    chain → v5's writer-task serialisation mapping (Q3); whether
    `findManyInFolderContaining`'s raw `instr(d.content, ?)` becomes a
    `doc_mount_documents.rs` method or an `Inventory`-module query (Q4).

---

## 0. The span for this slice, in one paragraph

`4fbdad186` (151 files, +13,094/−1,022) adds the Inventory: a Zod model
(`lib/schemas/inventory.types.ts`, 290 lines) over ONE `.json` file per object
under a tier's `Inventory/` folder (character vault / group store / project
store / Quilltap General), a reader + writer in the vault overlay
(`inventory-readers.ts` 240, `inventory-writes.ts` 306) that re-project the
whole folder through `projectArrayIntoVaultFolder` with a NEW `ext` option, the
wardrobe's folder rules and folder ops LIFTED into shared generic modules
(`lib/mount-index/item-folders.ts` 312, `vault-overlay/item-folder-ops.ts`
422) that the wardrobe now delegates to (`lib/wardrobe/folders.ts` −201 lines,
`wardrobe-writes.ts` −312), a location over the four tiers (`location.ts` 280),
a per-character pool with instance-wide archetype fallback (`pool.ts`,
`dependents.ts`), read-time resolution of sparse instances (`resolve.ts`), ONE
mutations module (`item-mutations.ts` 592), object pictures through the
wardrobe's picture steps with an `imagesFolder` parameter (`item-images.ts`,
`item-image-generation.ts`, `wardrobe-image-bridge.ts` +27, `wardrobe/
item-images.ts` +28), an allow-list SVG sanitiser (`svg.ts` 223), the
equip/unequip chokepoints with plan-then-write + kit expansion + the per-chat
kin cap (`equip.ts`), Pascal's one line per gesture + the avatar refresh
(`equip-effects.ts`, `pascal/writer.ts` +53, `outfit-change-effects.ts`
`refreshAvatarForAppearance` alias), arrival defaults at chat create / seat add
/ merge (`apply-defaults.ts`), the trailing `## Carried and equipped` section
on the chat spine + Carina + the greeting (`prompt-section.ts`,
`context-manager.ts`, `carina.service.ts`, `initial-greeting.ts`, `chats/
route.ts`), the `carrying …` image phrase through the appearance resolver, the
avatar prompt, the story background and the image tool (`image-description.ts`,
`appearance-resolution.ts` +93, `avatar-prompt.ts`, `prompt-expansion.ts`, two
job handlers), ONE route factory for four tiers + transfers + images + three
chat `?action=`s, the THIRTEENTH D23 move (`chats.equippedInventory`, patch-only,
ONE writer `setEquippedInventory`), `LLMLogTypeEnum` `INVENTORY_ITEM_IMAGE`
joining `IMAGE_SPEND_LOG_TYPES`, `docMountDocuments.findManyInFolderContaining`
(raw `instr`), and the carriers (`uuid-remap.ts` +88 with `remapLinkId`
keeping object ids verbatim and a `22f-quater` restore step repointing
`imageFileId`; `equippedInventory` in `CHARACTER_KEYED_CHAT_FIELDS`;
`qtap-export.schema.json` +8). `e1872c94f` adds the FOURTEENTH move
(`characters.canManageInventory`) and grows `equip.ts` (+153: `pickUpItems`,
`dropCarriedEverywhere`, the lock), `equip-effects.ts` (+133: the turn's
pending map + flush), `chats.repository.ts` (+45: `findChatIdsCarryingInventoryId`,
`removeEquippedInventoryId`), `transfers.ts` (141, extracted from the route),
`context.ts`. `102a23648` ungates the section in continue mode (bug 253 — the
progressions half is PORTED), adds the roster shelf gate (`roster-gate.ts` 54;
`OFF_ROSTER`), `looksPlural` / `indefiniteArticle`, the project tier's name, and
moves the inventory flush into a `finally` after the save. `7263105c6`'s lint
half adds `title-lint.ts` (411), `title-lint-documents.ts` (192),
`lint-scope.ts` (55), `pascal/tool-gate-keys.ts` (93), two refusal codes, the
lint calls in every mutation, `pool.consultedMounts` / `unreadMounts`, the
`withInventoryWritesLinted` wrap in `inventory-writes.ts`, and the four
`database-store.ts` hooks. `d88c1ce39` touches ONLY `components/inventory/
inventory-item-editor.tsx` (one hint string) + `help/inventory.md` + a test
pinning `renderCarryingPhrase`'s trimming (already so at `4fbdad186`) —
**it has NO server hunk**; the SPA survey owns its one string.

---

## 1. Hunk-by-hunk inventory — v4 AT THE PIN

### 1.1 `lib/schemas/inventory.types.ts` (290 lines, NEW) — the whole Zod model

- `:24` imports `isValidInventoryFolderPath, INVENTORY_FOLDER,
  INVENTORY_FOLDER_MAX_DEPTH` from `@/lib/inventory/folders`; `:25`
  `patchSchemaOf` (`lib/schemas/patch-schema.ts` — bug 244's write-side helper:
  strips every `.default()`/`.prefault()` then `.optional()`s each key; unknown
  keys STRIPPED, `patch-schema.ts:36-42`).
- `:31` `InventoryKindSchema = z.enum(['item', 'kit'])`.
- `:35` `INVENTORY_SVG_MAX_BYTES = 16 * 1024`; `:38` `INVENTORY_KIT_MAX_MEMBERS = 200`.
- `:41-46` `InventoryFolderPathSchema = z.string().max(900).refine(isValidInventoryFolderPath, { message: `Inventory folder must be a clean path of at most ${INVENTORY_FOLDER_MAX_DEPTH} segments, none named "images"` })`
  (the message is `Inventory folder must be a clean path of at most 8 segments, none named "images"`).
- `:49` `InventoryFolderInputSchema = z.string().max(900)` (request bodies).
- `:51-89` **`InventoryItemSchema`** (`.strict()`), keys IN THIS ORDER (this is
  `INVENTORY_FILE_KEYS` order minus `folder`, and the serialised file's key
  order):
  `id: UUIDSchema`; `kind: InventoryKindSchema.default('item')`;
  `name: z.string().min(1).max(200)`; `title: z.string().max(200).nullable().optional()`;
  `description: z.string().max(4000).nullable().optional()`;
  `imageDescription: z.string().max(2000).nullable().optional()`;
  `equippedByDefault: z.boolean().default(false)`;
  `maxEquipped: z.number().int().min(1).max(999).default(1)`;
  `canCopy: z.boolean().default(false)`; `isArchetype: z.boolean().default(false)`;
  `parentId: UUIDSchema.nullable().default(null)`;
  `itemIds: z.array(UUIDSchema).max(200).default([])`;
  `emoji: z.string().max(16).nullable().optional()`;
  `svg: z.string().max(16384).nullable().optional()`;
  `imageFileId: UUIDSchema.nullable().optional()`;
  `metadata: z.record(z.string(), z.unknown()).default({})`;
  `folder: InventoryFolderPathSchema.default('')`;
  `archivedAt: TimestampSchema.nullable().optional()`; `createdAt`; `updatedAt`.
- `:94-96` `INVENTORY_FILE_KEYS = Object.keys(InventoryItemSchema.shape).filter(k => k !== 'folder')`
  → `[id, kind, name, title, description, imageDescription, equippedByDefault,
  maxEquipped, canCopy, isArchetype, parentId, itemIds, emoji, svg,
  imageFileId, metadata, archivedAt, createdAt, updatedAt]` (19 keys).
- `:103-115` `INVENTORY_INHERITABLE_FIELDS = ['name','title','description','imageDescription','equippedByDefault','maxEquipped','canCopy','emoji','svg','imageFileId','metadata']` (11).
- `:119-123` `InventoryItemFileSchema = patchSchemaOf(InventoryItemSchema.omit({folder:true})).extend({ id, createdAt, updatedAt })` — sparse, nothing defaulted.
- `:132` `InventoryRecord = InventoryItemFile & { folder: string; fileStem?: string }`.
- `:135-139` `InventoryOrigin { scope: 'character'|'group'|'project'|'general'; id: string|null; name: string }`.
- `:142-150` `ResolvedInventoryItem = InventoryItem & { overriddenFields?; parentMissing?; parentName?; origin? }`.
- `:153` `EquippedInventorySchema = z.record(UUIDSchema, z.array(UUIDSchema))`.
- `:157` `KitState = 'all' | 'some' | 'none'`.
- `:164-183` **`inventoryShapeIssues`** — messages verbatim: `An archetype cannot derive from another archetype`; `A kit cannot be an archetype`; `A kit cannot be an instance`; `A kit needs at least one member`; `Only a kit names members`. Order: archetype checks first (parentId, then kind), then kit checks (parentId, then empty members), else the item-with-members check.
- `:190-200` `inventoryItemFieldsSchema = InventoryItemSchema.omit({id, createdAt, updatedAt, archivedAt, imageFileId, folder}).strip().extend({ folder: InventoryFolderInputSchema.optional() })`.
- `:203-208` `createInventorySchema` = fields + `superRefine`: each `inventoryShapeIssues` message as a `custom` issue, then `if (body.parentId)` issue `Create an instance with ?action=instantiate` at `path: ['parentId']`.
- `:217-222` `updateInventorySchema = patchSchemaOf(inventoryItemFieldsSchema).omit({parentId}).extend({ archived: z.boolean().optional(), imageFileId: UUIDSchema.nullable().optional() })`.
- `:226-228` `clearInventoryOverrideSchema = { fields: z.array(z.enum(INVENTORY_INHERITABLE_FIELDS)).min(1) }`.
- `:231-250` `instantiateInventorySchema = { archetypeId: UUIDSchema, folder?: InventoryFolderInputSchema, projectId?: z.string().nullable(), chatId?: z.string().nullable(), overrides?: patchSchemaOf(InventoryItemSchema.pick({name,title,description,imageDescription,equippedByDefault,maxEquipped,canCopy,emoji,svg,metadata})) }` (NB: `imageFileId` NOT pickable here).
- `:253-255` `inventoryFolderPathBodySchema = { path: InventoryFolderInputSchema.min(1, 'A folder path is required') }`.
- `:258-261` `renameInventoryFolderSchema = { path: …min(1,'A folder path is required'), name: z.string().min(1, 'A folder name is required').max(200) }`.
- `:264-267` `fileInventoryItemsSchema = { itemIds: z.array(z.string().min(1)).min(1, 'At least one item is required').max(500), folder: InventoryFolderInputSchema }`.
- `:270-273` `chatInventoryEquipSchema = { characterId: z.string().min(1), ids: z.array(UUIDSchema).min(1).max(200) }`.
- `:276-278` `chatInventoryResetSchema = { characterId: z.string().min(1) }`.
- `:281` `InventoryScopeSchema = z.enum(['character','group','project','general'])`.
- `:285-290` `inventoryTransferSchema = { action: z.enum(['move','copy']), itemId: UUIDSchema, from: { scope, id?: z.string().nullable() }, to: { scope, id? } }`.

### 1.2 The two migrations + registration + prettify (the D23 moves)

- `migrations/scripts/add-chat-equipped-inventory-v1.ts` (70 lines): id
  `add-chat-equipped-inventory-v1`, description `Add equippedInventory to chats (the Inventory objects each character carries)`,
  `introducedInVersion: '4.11.0'`, `dependsOn: ['add-equipped-outfit-field-v1']`;
  `shouldRun` = `isSQLiteBackend() && sqliteTableExists('chats') && !sqliteColumnExists('chats','equippedInventory')`
  (`:32-36`); `run` = `addColumnIfMissing('chats', 'equippedInventory', 'TEXT DEFAULT NULL')`
  (`:41`), INFO `Added equippedInventory column to chats` `{ context: 'migration.add-chat-equipped-inventory-v1' }` ONLY `if (added)` (`:42-44`), result `message: 'Added equippedInventory column to chats'`, `itemsAffected: added ? 1 : 0`; ERROR `Failed to add equippedInventory column to chats` (`:55`). No backfill (header `:7-9`).
- `migrations/scripts/add-character-inventory-flag-v1.ts` (67 lines): id
  `add-character-inventory-flag-v1`, description `Add canManageInventory to characters (gates the inventory_* tools)`,
  `introducedInVersion '4.11.0'`, `dependsOn: ['add-character-wardrobe-flags-v1']`;
  gate on `characters` / `canManageInventory` (`:29-33`);
  `addColumnIfMissing('characters', 'canManageInventory', 'INTEGER DEFAULT NULL')` (`:38`);
  INFO `Added canManageInventory column to characters` (`:40`); ERROR `Failed to add canManageInventory column to characters` (`:52`).
- `migrations/scripts/index.ts:451-453` imports; registration `:919-928` (the
  main array) and `:1371-1380` (the second array): `addChatWardrobeHistoryTableMigration`,
  `addChatWardrobeHistoryToolNameMigration`, `seedChatWardrobeHistoryMigration`,
  `// Inventory: chats.equippedInventory (the Inventory objects each character carries)`
  `addChatEquippedInventoryMigration`, `// Inventory tools: characters.canManageInventory (gates the inventory_* tools)`
  `addCharacterInventoryFlagMigration`, then `addTerminalSessionGeometryMigration`
  (bug 255, the smalls/terminal lane). Registration order IS run order; each
  `dependsOn` is earlier.
- `lib/startup/prettify.ts:162-164` (+3): `'add-chat-equipped-inventory-v1': "Fitting each chat with a croupier's ledger of what everyone is carrying"`,
  `'add-character-inventory-flag-v1': 'Granting characters leave to rummage through their own bags'`,
  `'add-terminal-session-geometry-v1': "Measuring each of Ariel's terminal windows, the better to read them back"` (NO-PORT — the deferred migration runner's labels).
- `lib/schemas/chat.types.ts:1042-1047` and `:1438-1443`: `equippedInventory: JsonSchema.nullable().optional()` in BOTH `ChatMetadataSchema` and `ChatMetadataBaseSchema`, AFTER `equippedOutfit`, BEFORE `characterAvatars` (doc: `Patch-only — written solely by ChatsRepository.setEquippedInventory`). `:542-549` the `MessageEventSchema.pascalMeta.effects` doc comment grows the `inventory.equip` / `inventory.unequip` / `inventory.pick_up` sentence (comment only; the records are the tools lane's).
- `lib/schemas/character.types.ts:231-232`: `canManageInventory: z.boolean().nullable().optional()` AFTER `canCreateOutfits`, BEFORE `canChooseOutfit`.
- `lib/schemas/llm-log.types.ts:37` `'INVENTORY_ITEM_IMAGE'` appended after `'WARDROBE_ITEM_IMAGE'` in `LLMLogTypeEnum`; `:52` `IMAGE_SPEND_LOG_TYPES = ['IMAGE_GENERATION', 'WARDROBE_ITEM_IMAGE', 'INVENTORY_ITEM_IMAGE']`.
- `public/schemas/qtap-export.schema.json`: `:479-482` character property `canManageInventory: { type: ["boolean","null"], description: "Whether this character may see and change what they carry with the inventory_* tools (null = enabled by default)" }` AFTER `canCreateOutfits`; `:603-610` chat property `equippedInventory: { type: ["object","null"], description: "Inventory objects each character carries in this chat, keyed by characterId: an array of Inventory object UUIDs (the `id` inside each `Inventory/**/*.json` file). Ids that no longer resolve are dropped on read.", additionalProperties: { type: "array", items: { $ref: "#/$defs/UUID" } } }` BEFORE `equippedOutfit`; `:974` the `effects` description sentence (tools lane's wording, same file — the keystone re-vendors the file WHOLE).

### 1.3 `lib/database/repositories/chats.repository.ts` (+138) — the ONE writer and the reads

- `:52-56` `const EQUIPPED_INVENTORY_FIELD = 'equippedInventory'` (doc: patch-only, written solely by `setEquippedInventory`).
- `:566-568` `patchOnlyFields()` → `[...CONCIERGE_MODE_FIELDS, EQUIPPED_INVENTORY_FIELD]` (a whole-row `_update` NEVER carries a stale copy back; `chats.create` still writes it — restore / import).
- `:780-790` `getEquippedInventory(chatId)` → `safeQuery(… normalizeEquippedInventory(chat?.equippedInventory), 'Failed to get equipped inventory', { chatId, context: 'inventory' }, {})`.
- `:802-830` **`setEquippedInventory(chatId, characterId, ids | null): Promise<boolean>`** — RMW: `collection.findOne({id})` → `normalizeEquippedInventory(row.equippedInventory)`; `ids === null` → `delete state[characterId]`; else `state[characterId] = Array.from(new Set(ids)).sort()` (**dedup + default JS SORT** — UTF-16 code-unit order of UUID strings); `updateOne({id}, {$set: {equippedInventory: state}})`; returns `matchedCount > 0`; fallback `false`, message `Failed to write equipped inventory` `{chatId, characterId, context:'inventory'}`; then DEBUG `Equipped inventory write` `{ chatId, characterId, count: ids?.length ?? null, cleared: ids === null, written, context: 'inventory' }` (count is the UN-deduped length).
- `:837-848` `findChatIdsCarryingInventoryId(characterId, itemId)` — `collection.find({ equippedInventory: { $like: `%${itemId}%` } })` then JS filter `normalizeEquippedInventory(row.equippedInventory)[characterId]?.includes(itemId)`; fallback `[]`, message `Failed to find chats carrying an inventory id`; DEBUG `Chats carrying an inventory id` `{characterId, itemId, chats: n}` (e1872c94f — used by `dropCarriedEverywhere`, tools lane, but the method is the keystone's repository).
- `:859-877` `removeEquippedInventoryId(chatId, characterId, itemId)` — RMW; `false` when the row is missing or `!ids.includes(itemId)`; `state[characterId] = ids.filter(id => id !== itemId)` (NOT re-sorted); `updateOne`; DEBUG `Equipped inventory id removed` `{chatId, characterId, itemId, written}`; message `Failed to remove an id from equipped inventory`.
- `:1005-1024` **`normalizeEquippedInventory(raw: unknown): Record<string, string[]>`** — a string is `JSON.parse`d (parse failure → `{}`); non-object / array / null → `{}`; per entry: non-array value DROPPED, array filtered to strings. Exported; `equipped-set.ts` and the child proxy import it.

### 1.4 The storage layer: readers, writes, the lifted folder machinery

**`lib/database/repositories/vault-overlay/inventory-readers.ts` (240, NEW).**
- `:38` `UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i`; `:40` `FullFileSchema = InventoryItemSchema.omit({ folder: true })`.
- `:45-48` `inventoryFileNameFor(record)` = `joinFolder(record.folder ?? '', `${sanitizeFileName(record.title || record.name || record.fileStem || `object-${record.id.slice(0, 8)}`)}.json`)`.
- `:63-139` **`parseInventoryItemFile(doc, {quiet})`**: path predicate first; `JSON.parse(doc.content ?? '')` — WARN `Inventory file is not valid JSON; skipped` `{error}`; non-object/array → WARN `Inventory file is not a JSON object; skipped`; `raw.id` not UUID-shaped → WARN `Inventory file has no UUID-shaped id; skipped`; keys not in `INVENTORY_FILE_KEYS` → WARN `Inventory file carries reserved-namespace keys outside metadata; dropped` `{itemId, keys}`; timestamps: `createdAt ??= toIso(doc.createdAt)`, `updatedAt ??= toIso(doc.updatedAt) ?? createdAt`, then `createdAt ??= new Date(0).toISOString()` (`1970-01-01T00:00:00.000Z`), `updatedAt ??= createdAt`; instance (`parentId` a non-empty string) → `InventoryItemFileSchema.safeParse` (sparse), else `FullFileSchema.safeParse` (defaults filled) — failure WARN `Inventory file failed validation; skipped` `{itemId, issues: "<path.joined>: <message>; …"}`; then `inventoryShapeIssues` + `An instance cannot be an archetype` → WARN `Inventory file breaks the object shape rules; skipped` `{itemId, issues}`; returns `{...file, folder: folderOfInventoryPath(relativePath), fileStem: fileName.replace(/\.json$/i, '')}`. Every WARN meta: `{mountPointId, relativePath, context: 'inventory', ...}`.
- `:141-145` `toIso(value)` — non-string/empty → undefined; `new Date(value)` NaN → undefined; else `toISOString()` (a V8 `Date` parse: `js_date_parse_ms` per the memory note, NOT `iso_to_ms`).
- `:152-154` `isInventoryItemDocument(doc)` = `parseInventoryItemFile(…, {quiet:true}) !== null` (the projection's `ownsFile`).
- `:157-164` `hasStableInventoryItemId(content)` = JSON `id` is a UUID string (try/catch → false).
- `:172-240` `readInventoryRecords(mountPointId)` / `readDuplicateInventoryPaths` over `scanInventoryFolder`: `docMountDocuments.findManyByMountPointsInFolder([mp], 'Inventory', '.json', {recursive:true})`, filter by path predicate, **sort `a.relativePath.localeCompare(b.relativePath)`** (default locale, default sensitivity — v5 `collation::locale_compare`), parse each (NOT quiet — WARNs fire), the winner per id = the file at its own projected name (`path.toLowerCase() === `Inventory/${inventoryFileNameFor(record)}`.toLowerCase()`) else first by path; losers WARN `Two Inventory files claim one id; this one is skipped and left alone` `{mountPointId, relativePath, keptPath, itemId}` and join `duplicatePaths`; DEBUG `Inventory folder read` `{mountPointId, documentCount, objectCount, duplicateCount, op:'read'}`.

**`lib/database/repositories/vault-overlay/inventory-writes.ts` (306, NEW).**
- `:58` `InventoryMount = VaultFolderMount`; `:65-67` `runSerialized(mp, fn)` = `runSerializedOnKey(chainKey(INVENTORY_FOLDER_RULES, mp), () => withInventoryWritesLinted(fn))` — key `Inventory:<mountPointId>` (7263105c6 added the `withInventoryWritesLinted` wrap).
- `:74-76` `resolveInventoryMount(characterId)` = `resolveVaultFolderMount` (delegate); `:79-81` `readMountInventory(mount)`.
- `:84-91` **`serializeInventoryRecord`**: known keys in `INVENTORY_FILE_KEYS` order, `undefined` skipped (an explicit `null` IS written), `JSON.stringify(out, null, 2) + '\n'`.
- `:95-101` `safeFolder` (`isSafeInventoryFolderPath` else normalise with DEBUG `Inventory write normalised an incoming folder` `{folder, normalised}`).
- `:107-130` `projectMountInventory(mount, records)`: reads duplicates (DEBUG `Inventory projection leaving duplicate-id files alone` `{mountPointId, paths}`), then `projectArrayIntoVaultFolder(mp, 'Inventory', records, r => ({fileName: inventoryFileNameFor(r), content: serializeInventoryRecord(r)}), mount.characterId ?? mount.mountPointId, { recursive: { excludeSubfolders: ['images'] }, ownsFile: doc => !duplicates.has(doc.relativePath) && isInventoryItemDocument(doc), preserveFileNames: [leaf names of the duplicate paths], ext: '.json' })`.
- `:133-148` `createInventoryInMount` → read, append `{...record, folder: safeFolder(record.folder)}`, project; DEBUG `Inventory object created in folder` `{scope, mountPointId, itemId, folder, op:'create'}`.
- `:154-158` `InventoryRecordPatch { id; patch; remove? }`; `:165-215` `updateInventoryManyInMount(mount, patches, {requireAll})` → `updateManyUnserialized`: missing ids with `requireAll` → `InventoryItemNotInLocationError(unique missing)` BEFORE any write; merge `{...before, ...patch, folder: safeFolder(patch.folder) if given, id: before.id, createdAt: before.createdAt, updatedAt: now}` then `delete merged[key]` for `remove`; nothing updated → `[]` with NO projection; DEBUG `Inventory objects updated in folder` `{scope, mountPointId, itemIds, count, op:'update'}`; returns records in PATCH order.
- `:222-253` `deleteInventoryInMount(mount, ids, followOn)` → deleted ids; `followOn(remaining)` patches applied with `updatedAt: now`; DEBUG `Inventory objects deleted from folder` `{scope, mountPointId, itemIds, followOnPatches, op:'delete'}`; `[]` short-circuits with no projection.
- `:259-274` `INVENTORY_TREE: ItemFolderTree<InventoryRecord> = { rules: INVENTORY_FOLDER_RULES, ext: '.json', context: 'inventory', isItemDocument: isInventoryItemDocument, hasStableId: hasStableInventoryItemId, read: readMountInventory, reproject: projectMountInventory, refile: (mount, patches) => updateManyUnserialized(mount, patches.map(({id, folder}) => ({id, patch: {folder}}))), errors: {path, notFound, conflict} }`.
- `:277-306` `listInventoryFoldersInMount` / `listInventoryFolderPathsInMount` / `createInventoryFolderInMount` / `renameInventoryFolderInMount` / `deleteInventoryFolderInMount` — delegates; the three writers wrapped in `withInventoryWritesLinted`.

**`lib/mount-index/item-folders.ts` (312, NEW — the LIFT).** Pure rules
parameterised by `ItemFolderRules { root; reserved; maxDepth; noun }` (`:23-32`).
Error classes `ItemFolderPathError`, `ItemFolderNotFoundError(folder, noun='item')`
→ `${capitalise(noun)} folder not found: ${folder}`, `ItemFolderConflictError(folder, message?, noun)` → `A ${noun} folder named "${folder}" already exists`,
`ItemNotInLocationError(itemIds, noun)` → `Not in this ${noun} (shared items are filed where they live): ${ids.join(', ')}` (`:35-70`).
`ItemFolder {path; name; itemCount}` (`:73-80`). Functions (each the wardrobe's
former body with `rules` threaded): `isReservedItemFolderSegment`, `folderSegments`,
`normalizeItemFolderPath(rules, input, makeError)` (`:99-125` — messages
`A ${noun} folder name may not be "${raw}"`, `"${segment}" is reserved for item pictures and cannot be a ${noun} folder`,
`${Capitalised noun} folders nest at most ${maxDepth} deep`),
`normalizeItemFolderPathOrRoot`, `normalizeItemFolderName` (`A folder name may not contain "/"`, `A folder needs a name`),
`isSafeItemFolderPath`, `resolveItemFolder` (`:175-200`), `isValidItemFolderPath`,
`folderOfItemPath`, `itemFolderStorePath`, `folderUnderItemRoot`,
`isItemDocumentPath(rules, relativePath, ext, skipFileName?)` (`:247-262`),
`parentFolder`, `folderName`, `joinFolder`, `isSameFolder`, `isUnderFolder`,
`rebaseFolder`, `folderCrumb`, `capitalise`. **With `noun: 'wardrobe'` every
string equals the pre-lift wardrobe string byte-for-byte** (verified against the
removed bodies in `lib/wardrobe/folders.ts`'s diff).

**`lib/database/repositories/vault-overlay/item-folder-ops.ts` (422, NEW — the LIFT).**
`VaultFolderMountScope` / `VaultFolderMount {mountPointId; scope; characterId}` /
`FiledItem {id; folder?; archivedAt?}` / `ItemFolderTree<T>` (`:44-83`); the
per-key write chain `runSerialized(key, fn)` over a module `Map` (`:86-103`) and
`chainKey(rules, mp)` = `${rules.root}:${mp}` (`:106-108`); `resolveVaultFolderMount(characterId)`
(`:118-133` — `null` → General; `characters.findByIdRaw`; `archivedAt` → `throw new CharacterArchivedError(characterId)`; no vault → null);
`listFoldersIn` (`:141-186` — union of folder rows + item folders, every ancestor
added, counts of non-archived items DIRECTLY inside, sort `a.path.localeCompare(b.path, undefined, {sensitivity:'base'})`,
DEBUG `${root} folders listed` `{mountPointId, scope, folderRowCount, folderCount, context}`);
`listFolderPathsIn` (`:192-200`); `findStoredFolder` (`:203-209`); `createFolderIn`
(`:216-237` — `The ${noun} root already exists`; `ensureFolderPath`; DEBUG `${root} folder ensured`);
`renameFolderIn` (`:245-305` — `The ${noun} root cannot be renamed`; stamps ids
into item files lacking a stable id BEFORE the move via `tree.reproject` (DEBUG
`Stamping ids into ${noun} files before a folder rename`); `moveDatabaseFolder`;
`DatabaseStoreError` `CONFLICT` → `errors.conflict(to)`, `NOT_FOUND` → `errors.notFound(folder)`; DEBUG `${root} folder renamed`);
`deleteFolderIn` (`:314-422` — `The ${noun} root cannot be deleted`; strays →
`Folder "${from}" holds ${n} file(s) that are not ${noun} items; move them in the Scriptorium first`;
same-named subfolder → `Folder "${from}" holds a subfolder of the same name; rename one of them first`;
`NOT_EMPTY` → `Folder "${from}" is not empty yet; its items were moved up`;
DEBUG `${root} folder deleted; contents sent up`). The wardrobe's `WARDROBE_TREE`
(`wardrobe-writes.ts:296-316`) delegates with `ext: '.md'`, `context: 'wardrobe'`,
`reproject: projectVaultWardrobe`.

**`lib/inventory/folders.ts` (145, NEW)** — delegates over
`INVENTORY_FOLDER_RULES = { root: 'Inventory', reserved: ['images'], maxDepth: 8, noun: 'inventory' }`
(`:54-59`); subclasses `InventoryFolderPathError` / `InventoryFolderNotFoundError`
(`Inventory folder not found: …`) / `InventoryFolderConflictError` / `InventoryItemNotInLocationError`
(`Not in this inventory (shared items are filed where they live): …`); `isInventoryItemDocumentPath(p)` = `isItemDocumentPath(rules, p, '.json')` — NO skip-file predicate (the wardrobe passes `isWardrobeInstructionsFileName`).

**The wardrobe-side lift hunks** (strings preserved, no behaviour):
- `lib/wardrobe/folders.ts`: `WARDROBE_FOLDER_RULES = { root: 'Wardrobe', reserved: ['images'], maxDepth: 8, noun: 'wardrobe' }` (`:58-63`); the four error classes become subclasses; every helper a one-line delegate; `folderSegments`/`parentFolder`/`folderName`/`joinFolder`/`isSameFolder`/`isUnderFolder`/`rebaseFolder`/`folderCrumb` RE-EXPORTED from `item-folders`.
- `vault-overlay/parsers.ts:27-28, 259` `isWardrobeItemDocumentPath` = `isItemDocumentPath(WARDROBE_FOLDER_RULES, p, '.md', isWardrobeInstructionsFileName)` (the `parseScenarioFilesSorted` hunk in the same diff is `1d0a31132`'s — the scenario-sort survey).
- `vault-overlay/wardrobe-writes.ts`: `WardrobeMountScope = VaultFolderMountScope`, `WardrobeMount = VaultFolderMount`, `runSerialized` → `runSerializedOnKey(chainKey(WARDROBE_FOLDER_RULES, mp), fn)` (key `Wardrobe:<mp>` — unchanged bytes), `resolveWardrobeMount` → `resolveVaultFolderMount`, the five folder fns → `WARDROBE_TREE` delegates (`:291-360`).
- `vault-overlay/vault-projection.ts:38-44, 80` the NEW `ext?: string` option (default `.md`) on `ProjectArrayOptions`, used only in `findManyByMountPointsInFolder(…, opts?.ext ?? '.md', …)`.
- `lib/mount-index/character-vault.ts:33-34` `export { INVENTORY_FOLDER as CHARACTER_INVENTORY_FOLDER } from '@/lib/inventory/folders'` (v5 HAS the constant — P4.D322).
- `lib/file-storage/wardrobe-image-bridge.ts`: `INVENTORY_IMAGES_FOLDER = 'Inventory/images'` (`:54`); `wardrobeItemImageFolder(itemId, imagesFolder = WARDROBE_IMAGES_FOLDER)` (`:59-61`), `wardrobeItemImagePath(itemId, leafName, imagesFolder)` (`:64-70`), `WriteWardrobeItemImageInput.imagesFolder?` (`:99-100`) used at `:132`, `deleteWardrobeItemImageLink(mp, itemId, leafName, imagesFolder = WARDROBE_IMAGES_FOLDER)` (`:193-201`).
- `lib/wardrobe/item-images.ts`: `deleteImageLink(mp, itemId, leaf, imagesFolder?)` (`:326-330`); `removeImageFile(repos, itemId, file, imagesFolder?)` now EXPORTED (`:332-342`); `cleanupItemImages(…, imagesFolder?)` (`:386-411`); `carryItemImages(args.imagesFolder?)` threaded into `writeWardrobeItemImage` (`:454-482`); `commitMovedImages(…, imagesFolder?)` → `dropSourceImageLinks(itemId, toDrop, imagesFolder)` (`:541-572`).
- `lib/wardrobe/outfit-change-effects.ts:85-104`: `refreshAvatarForOutfit` RENAMED `refreshAvatarForAppearance` (same body) with `export const refreshAvatarForOutfit = refreshAvatarForAppearance` (`:104`). Callers pass `'inventory-change'` as `sourceContext`.
- `lib/database/repositories/doc-mount-documents.repository.ts:222-264` NEW **`findManyInFolderContaining(folder, extension, needles)`**: `withRawDb([], …)` — SQL (verbatim):
  `SELECT d.id, d.fileId, d.content, d.contentSha256, d.plainTextLength, d.createdAt, d.updatedAt, l.id AS linkId, l.mountPointId, l.relativePath, l.fileName, l.folderId, l.lastModified, l.linkGroupId, f.fileType FROM doc_mount_file_links l JOIN doc_mount_documents d ON d.fileId = l.fileId JOIN doc_mount_files f ON f.id = l.fileId WHERE LOWER(l.relativePath) LIKE ? AND (instr(d.content, ?) > 0 OR instr(d.content, ?) > 0 …)` with `` `${folder}/`.toLowerCase() + '%' `` then the needles; JS post-filter `startsWith(prefixLower) && endsWith(extensionLower)`; empty needle list → `[]` with NO query; error text `Error finding documents in a folder by content` `{folder, extension, needleCount}`.

### 1.5 The model layer: location, pool, resolve, dependents, equipped set, errors, svg

**`lib/inventory/location.ts` (280, NEW).** Mirrors `lib/wardrobe/location.ts`
(whose interface at the pin, `:59-105`, is: `scope, id, mountPointId,
characterId, origin, readItems(includeArchived?), findItem, create, update(id,
patch), updateMany(patches, opts), delete(id): boolean, listFolders,
resolveFolder, createFolder, renameFolder, deleteFolder, writableMountPointId`).
Differences in `InventoryLocation` (`:55-86`): `update(id, patch, remove?)`
(a third `remove: readonly string[]` for cleared overrides);
`delete(ids: readonly string[], followOn?: (remaining) => InventoryRecordPatch[]): Promise<string[]>`
(many ids + follow-on patches, returns the ids deleted — the wardrobe's is one id → boolean);
`origin: InventoryOrigin`. Everything else is name-for-name.
- `:53` `GENERAL_INVENTORY_ORIGIN = { scope: 'general', id: null, name: 'Quilltap General' }`.
- `:89-91` `locationKey(loc)` = `${scope}:${mountPointId}`.
- `:94-104` `ensureInventoryFolder(mp)` → `ensureFolderPath(mp, 'Inventory')`; WARN `[InventoryLocation] Failed to ensure Inventory folder` `{mountPointId, context:'inventory', error}` (never throws).
- `:107-113` `cleanPatch` drops `id / createdAt / updatedAt` from every patch (identity is the writer's).
- `:119-182` `inventoryLocationForMount(mount, id, origin)`: `writableMount()` re-resolves a character's mount at WRITE time through `resolveInventoryMount(characterId)` (`Character ${id} has no linked vault` when null) — reads use the mount as given; `readItems(includeArchived=false)` filters `!archivedAt`; `findItem` reads archived included; `update` = `updateMany([{id, patch, remove}])[0] ?? null`; `resolveFolder('' | whitespace)` → `''` without a read.
- `:194-212` `resolveInventoryLocation(scope, id, repos, userId, {ensure})` → DEBUG `[InventoryLocation] Resolved inventory location` `{scope, id: id ?? null, ensure, found, mountPointId: location?.mountPointId ?? null, op:'resolve', context:'inventory'}` (one line per call that RETURNS).
- `:214-262` `resolve`: `general` → `getGeneralMountPointId()` (null → null; `ensure` → `ensureInventoryFolder`); `!id` → null; `character` → `characters.findByIdRaw(id)`, `userId` gate, `characterDocumentMountPointId` required, origin `{scope:'character', id, name: character.name}` — NO tombstone check on resolve (writes refuse through `writableMount`); `project` / `group` → `findById`, `name = owner.name || ('Project' | 'Group')`, `ensure` → `ensureOwnerOfficialStore(scope, owner.id, name)` + `ensureInventoryFolder`, else `owner.officialMountPointId ?? null` (null → null); origin `{scope, id: owner.id, name: owner.name}` (the RAW name, not the fallback).
- `:268-280` `groupLocationsForCharacter(characterId)` over `resolveGroupMountsForCharacter` (dynamic import of `tiered-mount-pool`), one location per mount, origin `{scope:'group', id: group.id, name: group.name}`.

**`lib/inventory/pool.ts` (351, NEW; +25 in `102a23648`, +48 in `7263105c6`).**
- `:38-72` `InventoryTierMounts {groupMountPointIds; projectMountPointIds}`, `UnreadInventoryMount {mountPointId; error}`, `InventoryPool { characterId; tiers; consultedMounts; unreadMounts; byId; own; reachable(); get; owns; resolve; resolved(); resolveId }`.
- `:76-80` `shadow(layers)` — a `Map` set in order, LATER WINS (insertion order of the Map = first-seen order; a re-set keeps the original position — V8 Map semantics; v5 `IndexMap` matches).
- `:86-137` `buildInventoryPool(characterId, tiers, {own, group, project, general}, externalParents, unreadMounts, consultedMounts)`: `weakestFirst = [general, project, group, own]`; `reachable()` = shadow of each layer FILTERED `!archivedAt` (archived dropped per tier BEFORE shadowing — an archived personal copy never hides a shared one), cached; `lookup(id) = byId.get(id) ?? externalParents.get(id)`; `resolve` WARNs ONCE per record id `[InventoryPool] Instance whose archetype exists nowhere; resolved to its own fields` `{characterId, itemId, parentId}`; `resolved()` = `reachable().map(resolve)`; `resolveId` over `byId` (archived included).
- `:144-167` `readTier(mountPointIds, originOf, unread, consulted)`: per mount `consulted.push`, `readMountInventory({mountPointId, scope:'project', characterId:null})` (the scope is a dummy for reads), tag `{...record, origin, mountPointId}`; failure → `unread.push({mountPointId, error})` + WARN `[InventoryPool] Failed to read an Inventory tier; skipping it` `{mountPointId, error}`.
- `:170-192` `fetchExternalParents(layers)`: parent ids not in any layer → `findRecordsAnywhere(missing)`, tagged origin `{scope:'project', id:null, name:'Elsewhere'}`; DEBUG `[InventoryPool] Fetched archetypes outside the pool` `{wanted, found}`.
- `:205-220` `projectNameOf(repos, projectId)` — `'Project'` when no id / no repo / no row / empty name; a thrown read → WARN `[InventoryPool] Project name lookup failed` `{projectId, error}` → `'Project'` (102a23648).
- `:226-296` **`loadInventoryPool(repos, {characterId, chatId?, projectId?, ownRecords?})`**: `projectId ??= chats.findById(chatId)?.projectId` (a throw → WARN `[InventoryPool] Project lookup for chat failed` `{chatId, error}`, continue with null); `characters.findByIdRaw`; `Promise.all([resolveGroupMountsForCharacter, resolveProjectMountPointIds(projectId), getGeneralMountPointId(), projectNameOf])`; origins — own `{scope:'character', id, name: character?.name ?? ''}`, group `{scope:'group', id: group.id, name: group.name}` per mount, project `{scope:'project', id: projectId, name: projectName}`, general; `consulted` seeded with the own mount when `ownRecords` were handed in; `Promise.all` of the four tier reads — the project tier EXCLUDES the own mount id (`mp !== ownMountPointId`), own tier = `ownRecords` tagged or `readTier([own])` or `[]`; DEBUG `[InventoryPool] Loaded inventory pool` `{characterId, chatId: chatId ?? null, projectId, ownCount, groupCount, projectCount, generalCount, reachable: pool.byId.size, unread, op:'pool'}` (`reachable` is the ARCHIVED-INCLUDED count).
- `:304-332` `loadPoolForLocation(repos, location, {chatId?, projectId?})`: a character location → `loadInventoryPool`; a shared tier → `own = location.readItems(true)` tagged with `location.origin`, `general` read UNLESS `location.scope === 'general'`, `characterId: null`, `tiers` both `[]`, `consulted = [location.mountPointId, (general)]`.
- `:335-346` `loadCastInventoryPools(repos, chat, characterIds)` — `Promise.all`, a `Map` in cast order.
- `:349-351` `kinOf(name, pool)` — resolved, `!isArchetype && kind !== 'kit' && isKin`.

**`lib/inventory/resolve.ts` (157, NEW; pure).**
- `:33-48` `withInventoryDefaults(record)`: `{kind:'item', equippedByDefault:false, maxEquipped:1, canCopy:false, isArchetype:false, itemIds:[], metadata:{}, ...stripUndefined(rest), name: rest.name ?? fileStem ?? 'Unnamed object', parentId: rest.parentId ?? null, folder: rest.folder ?? ''}` — `fileStem` removed; KEY ORDER of the result object = defaults first, then the record's own keys in their order, then `name`/`parentId`/`folder` re-set (a re-set keeps position; a NEW key appends — a serialised resolved item's key order is V8 insertion order; the wire is `successResponse`'s JSON so the order IS observable on the routes).
- `:55-58` `overriddenFieldsOf(record)` = inheritable fields whose value `!== undefined` (an explicit `null` COUNTS as an override).
- `:64-112` `resolveItem(record, lookup)`: no parent → `{...withInventoryDefaults(record), ...(origin && {origin})}`; parent missing or `!parent.isArchetype` → own fields + `kind:'item', isArchetype:false, itemIds:[], overriddenFields, parentMissing:true, parentName:null`; else `inherited[field] = own[field] !== undefined ? own[field] : base[field]` for the 11 fields, `metadata = {...base.metadata, ...record.metadata}` (shallow, instance wins), then `{...inherited, id, kind:'item', isArchetype:false, parentId, itemIds:[], folder: record.folder ?? '', archivedAt: record.archivedAt ?? null, createdAt, updatedAt, overriddenFields, parentMissing:false, parentName: base.title || base.name, origin?}`.
- `:115-119` `inventoryDisplayName` → `Excalibur (sword)` when `title.trim()` differs from `name.trim()` case-insensitively, else `title || name`; `:122-124` `inventoryLabel` = `title?.trim() || name`; `:127-129` `isKin` = trimmed lower names equal; `:132-134` `isEquippable` = `!isArchetype && kind !== 'kit' && !archivedAt`.
- `:140-157` `kitState(kit, equipped, resolvable)`: members that are missing / kits / archived are SKIPPED from the total; an archetype member counts carried when some equipped item's `parentId` is it; `total === 0 || carried === 0` → `'none'`, `carried === total` → `'all'`, else `'some'`.

**`lib/inventory/dependents.ts` (203, NEW).**
- `:44-67` `readAllInventoryRecords()` — every mount (`docMountPoints.findAll()`), ONE `findManyByMountPointsInFolder(ids, 'Inventory', '.json', {recursive:true})`, `parseInventoryItemFile(doc, {quiet:true})`; DEBUG `[InventoryDependents] Read every Inventory record` `{mountCount, recordCount, op:'read-all'}`.
- `:74-100` `findRecordsAnywhere(ids)` — `findManyInFolderContaining('Inventory', '.json', ids)`, path-filtered, **sorted `a.relativePath.localeCompare(b.relativePath)`**, first match per wanted id wins; DEBUG `[InventoryDependents] Found records by id across the instance` `{wanted, candidates, found, op:'find-anywhere'}`.
- `:103-121` `describeMount(mp, generalMp)`: General → `{scope:'general', ownerId:null, ownerName:'Quilltap General', ownerArchived:false}`; `characters.findByDocumentMountPointIdRaw(mp)` → `{scope:'character', ownerId, ownerName: name, ownerArchived: !!archivedAt}`; `projects.findAll().find(officialMountPointId === mp)`; `groups.findAll().find(…)`; else `{scope:'unknown', ownerId:null, ownerName: mount?.name ?? null, ownerArchived:false}`.
- `:124-127` `mountScopeOf(mp)` → scope or null for `unknown`.
- `:135-146` `isReachableFromKitTier(kitMp, target)`: same mount or `general` → true; kit owner not a character → false; `project` → true; `group` → the character's group locations include the target mount; else false.
- `:152-177` `findKitsStrandedByMove(itemId, target)` — candidates via `findManyInFolderContaining(…, [itemId])`, non-archived kits naming it that can't reach `target`; DEBUG `[InventoryDependents] Checked kits a move would strand` `{itemId, targetMountPointId, targetScope, candidates, stranded, op:'kits-stranded'}`; rows `{id, name: kit.title || kit.name || null, ownerName, mountPointId}`.
- `:180-203` `findDependents(archetypeId)` → `readAllInventoryRecords()` filtered `parentId === archetypeId`, each tagged by `describeMount` (memoised per mount); DEBUG `[InventoryDependents] Found dependents` `{itemId, count, archivedHolders, op:'dependents'}`.

**`lib/inventory/equipped-set.ts` (55, NEW).** `equippedIdsOf(chat, characterId)` = `normalizeEquippedInventory(chat.equippedInventory)[characterId] ?? []`; `hasEquippedEntry` = `characterId in normalize(...)` (an EMPTY array counts); `readEquippedSet(chat, characterId, pool)` keeps ids whose `pool.resolveId(id)` is `isEquippable`, DEBUG per dropped id `[InventoryEquippedSet] Dropped an id that no longer resolves to a carriable object` `{chatId, characterId, itemId, reason: 'missing'|'archived'|'archetype'|'kit'}` (reason precedence: missing → archived → archetype → kit). Nothing written.

**`lib/inventory/errors.ts` (221; +20 `102a23648`, +62 `7263105c6`).** `InventoryRefusal(code, message, status: 400|409 = 409, details = {})` (`name: 'InventoryRefusal'`). Codes and messages VERBATIM:
- `OFF_ROSTER` (`InventoryOffRosterError(itemNames)`, 409, details `{itemNames}`): `${names.map(n => `"${n}"`).join(', ')} ${n === 1 ? 'is' : 'are'} on the project's shelf, and you are not on this project's character roster. Ask the user to add you to the roster in the project's Characters card.`
- `ARCHETYPE_IN_USE` (`dependents, holders`): `${n} ${n === 1 ? 'object derives' : 'objects derive'} from it`; details `{dependents, holders: [{scope, ownerId, ownerName}]}`.
- `ARCHIVED_DEPENDENTS` (`characters`): `Instances live in archived characters' vaults, which cannot be changed: ${characters.map(c => c.name ?? c.id ?? 'unknown').join(', ')}`.
- `NOT_COPYABLE` (`itemIds`): `Cannot be copied: ${ids.join(', ')}`.
- `CAP_EXCEEDED` (`itemId, itemName, cap`): `"${itemName}" can be carried at most ${cap} at a time`.
- `INVALID_PARENT` (message, details) — messages at the sites: `Create an instance with instantiate`; `The parent must be an archetype this tier can reach`; `An archived archetype cannot be instantiated`; `An instance cannot be an archetype`; `A kit cannot be an archetype`; `An archetype cannot be a kit`; `Only an instance inherits; this object has no archetype`; `This object has no archetype to detach from`; `"${title || name}" is not an archetype; equip it instead`; `The parent must be an archetype this character can reach`.
- `KIT_NESTING` (`memberIds`): `A kit cannot contain another kit: ${ids.join(', ')}`.
- `KIT_STRANDED` (`itemId, kits`): `Moving this would strand it from ${n} kit${n === 1 ? '' : 's'} that name it: ${kits.map(k => k.name ?? k.id).join(', ')}`.
- `NOT_EQUIPPABLE` (`itemId, reason: 'archetype'|'archived'|'kit'|'missing'`): `Object ${itemId} cannot be carried (${reason})`.
- `INVALID_SHAPE` (`InventoryShapeError`, **400**): `issues.join('; ')`; `Kit members not reachable from this tier: ${missing.join(', ')}` (`{missing}`); `An instance is an item, never a kit`; `An object with that id already exists at the destination` (`{itemId}`); `Source and destination are the same` (`InventoryTransferSameLocationError`, `transfers.ts:46-51`).
- `NOT_A_PARTICIPANT` (`equip.ts:91-96`, **400**): `Character ${characterId} is not a participant of this chat`.
- `TITLE_SHADOW` (`InventoryTitleShadowError(title, collisions)`, 409): lines joined by `\n` — `title rejected: "${title}"`, then per collision `${i === 0 ? 'collides with' : 'and with'} KIND NAME "${kindName}" (reason: kind-name shadow)` OR `${lead} a gate key on tool "${toolName}" (reason: live gate shadow)` + `  that gate's "${ref}" already lands on ${landsOn.map(n => `"${n}"`).join(', ')}`, then `choose a title that is not a kind name and not a gate key.` / `choose a title that is not a kind name.` / `choose a title that is not a gate key.`; details `{title, collisions}`.
- `TITLE_CHECK_UNAVAILABLE` (`title, unread`): `title rejected: "${title}"`, per unread `the title check could not read ${u.pool} (${u.reason})`, then `a title is not accepted unchecked; repair what could not be read, then try again.`; details `{title, unread}`.

**`lib/inventory/svg.ts` (223, NEW; pure).** `InventorySvgError` (`An SVG glyph must start with an <svg> element`; `An SVG glyph may be at most 16 KB` — `${INVENTORY_SVG_MAX_BYTES / 1024} KB`). `ALLOWED_ELEMENTS` = `svg g path circle ellipse line polyline polygon rect title desc defs symbol use lineargradient radialgradient stop clippath mask pattern text tspan marker` (lower-case compare; output keeps the INPUT casing of the tag name, `:170`). `keepAttribute` (`:43-53`): `on*` dropped; `compact = value.replace(/[\s\u0000-\u001f]+/g,'').toLowerCase()` containing `javascript:` / `data:` / `vbscript:` dropped; `href`/`xlink:href` kept only if `value.trim().startsWith('#')`; `style` dropped if compact has `url(` / `expression(` / `@import`; every `url(…)` in compact must match `/^url\(['"]?#/`. `escapeAttribute` = `&` not followed by `[a-z0-9#]+;` → `&amp;`, `"` → `&quot;`, `<` → `&lt;`. `ATTRIBUTE_RE = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g`, attribute names must match `/^[a-zA-Z_:][-a-zA-Z0-9_:.]*$/`; rebuilt as `<${name} a="v" b="v">` / `<name …/>`. Tokeniser (`:95-179`): text escaped (`&`→`&amp;` with the same entity guard, `<`→`&lt;`, `>`→`&gt;`); `<!--…-->`, `<![CDATA[…]]>`, `<?…?>` dropped (unterminated → to end); `<!…>` with bracket depth (DOCTYPE internal subset) dropped; `findTagEnd` skips quoted values; a close tag pops back to the matching open (`lastIndexOfName`: a `null` entry also matches when the closing name is NOT an allowed element — a dropped element closes by its own name); `/`-ending raw tag = self-closing; disallowed non-self-closing elements push `null` and increment `dropping` (contents dropped); the root must be `svg` (else throw); leftover opens closed in order. `inventorySvgDataUri(svg)` = `data:image/svg+xml;utf8,${encodeURIComponent(svg)}` (client use).

### 1.6 The chat-spine chokepoints: equip, effects, arrival defaults, prompt section, image phrase, Pascal's line

**`lib/inventory/equip.ts` (534; 348 @`4fbdad186`, +153 `e1872c94f`, +37 `102a23648`).**
- `:62-67` `EquipChat {id; projectId?; equippedInventory?; participants?: {type?, characterId?, status?}[]}`; `:69` `SkipReason = 'missing'|'archived'|'is_kit'`; `:71-86` `EquipResult {equipped; created; skipped: {id, kitId, reason}[]; alreadyEquipped: string[]}`, `UnequipResult {unequipped; notEquipped}`.
- `:98-106` `assertParticipant` — seated = some participant with `characterId === id && status !== 'removed'` (NB: `type` is NOT checked here — the route's `castOf` checks `type === 'CHARACTER'`); INFO `[InventoryEquip] Refused` `{code:'NOT_A_PARTICIPANT', chatId, characterId}`.
- `:113-133` `assertShelfReach(chat, characterId, items, ctx)` (102a23648): only for `isCharacterGesture(ctx.provenance)`; `fromShelf = items.filter(isOnClosedShelf(item, NOTHING_CARRIED))` (= every item with `origin.scope === 'project'`); empty or `projectShelfOpenTo(chat.projectId, characterId)` → return; else INFO `[InventoryEquip] Refused` `{code:'OFF_ROSTER', chatId, characterId, projectId: chat.projectId ?? null, itemIds, provenance}` and throw `InventoryOffRosterError(items.map(i => i.title || i.name))`.
- `:138-144` `freshChat` (re-read inside the lock, fall back to the stale one), `poolFor`.
- `:173-175` `withChatInventoryLock(chatId, fn)` = `runSerialized(`inventory-equip:${chatId}`, fn)` — the SAME module-level chain map as the folder writes (`item-folder-ops.ts:86`), a different key family.
- `:187-210` `dropCarriedEverywhere` (e1872c94f; the tools lane's caller, the keystone's home — see §5).
- `:213-239` `equipItemsHoldingLock` (no lock) / `equipItems` (takes the lock) — both `assertParticipant` FIRST (outside the lock).
- `:241-382` **`equipUnserialized`** — the plan: `current = readEquippedSet(fresh, characterId, pool)`; `carriedParents` = their `parentId`s; `planExisting` (already carried → `alreadyEquipped`; de-dup planned); `spareInstances(archetypeId)` = own, non-archived, uncarried, unplanned instances of it, resolved; for each unique id: `pool.resolveId(id) ?? known.get(id)` — missing / archived / archetype → lenient: DEBUG `[InventoryEquip] Arrival skipped an object that cannot be carried` `{chatId, characterId, itemId, reason}` + continue; else INFO `[InventoryEquip] Refused` `{code:'NOT_EQUIPPABLE', …, reason}` + throw; a non-kit → `planExisting`; a kit → each member: missing → `skipped 'missing'`, archived → `'archived'`, kit → `'is_kit'`, archetype → carried already (`carriedParents.has`) → `alreadyEquipped.push(member.id)`, else once per archetype a spare instance (`planExisting(spare)`) or `{kind:'instantiate', archetype: member, kitFolder: pool.owns(kit.id) ? kit.folder : null}`; non-archetype member → `planExisting`. Then `assertShelfReach` over the planned subjects. Then the CAP over the whole plan in order: `counted = [...current]`, `kin = counted.filter(isKin(other, subject)).length`, `kin + 1 > subject.maxEquipped` → lenient: WARN `[InventoryEquip] Arrival dropped an object over its cap` `{chatId, characterId, itemId, cap}` + continue; else INFO `[InventoryEquip] Refused` `{code:'CAP_EXCEEDED', chatId, characterId, itemId, cap}` + throw `InventoryCapExceededError(id, title || name, maxEquipped)` — NOTHING written (instantiations included); accepted subjects join `counted`. Nothing accepted → DEBUG `[InventoryEquip] Nothing new to carry` `{chatId, characterId, alreadyEquipped, skipped}` and return (NO write, NO realtime, NO notify). Writes: if any `instantiate` → `resolveInventoryLocation('character', characterId, repos, ctx.userId)` (`Character ${id} has no vault to hold a kit's instances` when null) → `instantiate(repos, vault, archetype.id, {folder: kitFolder ?? undefined, pool})` each, in plan order, pushed to `created` AND `equipped`; THEN the existing ones pushed to `equipped` (so `equipped` order = created first, then existing — NOT plan order); `setEquippedInventory(chat.id, characterId, [...current ids, ...equipped ids])`; `publishRealtime('chats', chat.id)`; DEBUG `[InventoryEquip] Equipped` `{op:'equip', chatId, characterId, equipped, created, skipped, alreadyEquipped}`; `notifyInventoryChanged(repos, chat, characterId, {equipped, unequipped: []}, ctx)`.
- `:385-434` **`unequipItems`** (under the lock): for each unique id — a kit → every carried item whose `id` or `parentId` is a member joins `removing`; a carried id → `removing`; else `notEquipped.push(id)` (a kit NEVER lands in `notEquipped`); `unequipped = current.filter(removing)` (current order); write only when non-empty (`setEquippedInventory(fresh.id, characterId, current.filter(!removing).map(id))` + `publishRealtime('chats')`); DEBUG `[InventoryEquip] Unequipped` `{op:'unequip', chatId, characterId, unequipped, notEquipped}` ALWAYS; `notifyInventoryChanged(…, {equipped: [], unequipped}, ctx)` ALWAYS (it early-returns on an empty change).
- `:451-534` `pickUpItems` (e1872c94f — the tools lane's gesture; home here): `INVALID_PARENT` arms (`:469-478`), `assertShelfReach`, the cap over `archetypes` when `equip`, `resolveInventoryLocation('character', …)` (`Character ${id} has no vault to pick an object up into`), `instantiate(…, {pool, overrides: {title}?})` each at the ROOT, `publishRealtime('inventory')`, then (when `equip && created.length`) `setEquippedInventory([...current, ...created])` + `publishRealtime('chats')`; DEBUG `[InventoryEquip] Picked up` `{op:'pick-up', chatId, characterId, archetypes, created, equipped}`; `notifyInventoryChanged(…, {equipped: equip ? created : [], unequipped: [], pickedUp: created}, ctx)`.

**`lib/inventory/equip-effects.ts` (246; 111 @`4fbdad186`, +133 `e1872c94f`, +18 `102a23648`).**
- `:38` `InventoryChangeProvenance = 'operator'|'character'|'pascal'`; `:41-46` `InventoryChangeContext extends Pick<WardrobeTurnContainers,'pendingInventoryAnnouncements'> { userId; provenance; announce }`; `:49-56` `InventoryChange {equipped; unequipped; pickedUp?; given?: {item, toName}}`.
- `:59-62` `looksPlural(name)`: last whitespace-separated word matches `/s$/i` and NOT `/(ss|us|is)$/i` ("lockpicks", "gloves" → plural; "compass", "abacus", "iris" → not).
- `:65-68` `indefiniteArticle(name)`: `/^(uni|use|usu|uti|eu|one|once)/i` → `'a'`; `/^[aeiou]/i` → `'an'`; else `'a'`.
- `:70-76` `withArticle(item, 'a'|'the')`: a trimmed title → the title verbatim; `'the'` → `the ${name}`; plural → `some ${name}`; else `${a|an} ${name}` (name trimmed).
- `:78-82` `joinList`: 1 → as is; 2 → `A and B`; ≥3 → `A, B, and C` (Oxford comma).
- `:90-108` **`describeInventoryChange(characterName, change)`**: `given` → `*${name} hands ${withArticle(item,'the')} to ${toName}.*`; clauses in order: `sets down ${joinList(unequipped 'the')}`, `takes up ${joinList(equipped minus pickedUp ids, 'a')}`, `picks up ${joinList(pickedUp, 'a')}`; none → null; else `*${name} ${clauses.join(' and ')}.*`.
- `:114-167` **`notifyInventoryChanged(repos, chat, characterId, change, ctx)`**: early return when nothing changed and no give; `pending && ctx.announce && !change.given` → `recordPendingInventoryChange` + DEBUG `[InventoryEffects] Inventory change joined the turn` `{chatId, characterId, provenance}` and return (no refresh now); `pending && change.given` → `strikeFromPendingInventoryChange(pending, `${chatId}:${characterId}`, item.id)`; then try: `ctx.announce` → `characters.findById(characterId)`, `describeInventoryChange(character?.name ?? 'Someone', change)` → `postPascalInventoryChange({chatId, content})` when non-null; ALWAYS `refreshAvatarForAppearance(repos, {userId, chatId, characterId}, 'inventory-change')`; DEBUG `[InventoryEffects] Inventory change effects fired` `{chatId, characterId, announced, provenance, equipped: labels, unequipped: labels}`; catch → WARN `[InventoryEffects] Inventory change effects failed` `{chatId, characterId, error}`. Never throws.
- `:174-198` `recordPendingInventoryChange` — key `${chatId}:${characterId}`; entry `{userId, chatId, characterId, equipped:[], unequipped:[], pickedUp:[], provenances:[]}`; provenance appended once in first-seen order; an equipped item undoes a pending unequip (splice) else `pushUnique`; an unequipped item undoes a pending equip (splice) AND `removeById(pickedUp)`, else `pushUnique(unequipped)`; `pickedUp` pushed unique.
- `:206-216` `strikeFromPendingInventoryChange` removes the id from all three lists.
- `:223-246` `flushPendingInventoryAnnouncements(context, repos?)`: no map / empty → return; snapshot entries, `pending.clear()` (idempotent), `repos ?? getRepositories()`; per entry `notifyInventoryChanged(repos, {id: chatId}, characterId, {equipped, unequipped, pickedUp}, {userId, provenance: provenances[0] ?? 'character', announce: true})` (NO `pendingInventoryAnnouncements` in that ctx, so it posts); DEBUG `[InventoryEffects] Drained the turn's Inventory announcements` `{characters: n, provenances: [{characterId, provenances}]}`.
- `lib/wardrobe/turn-containers.ts:18-30, 39-40, 44-48, 57` (e1872c94f): `PendingInventoryChange {userId; chatId; characterId; equipped; unequipped; pickedUp; provenances}`; `WardrobeTurnContainers.pendingInventoryAnnouncements?: Map<string, PendingInventoryChange>`; `newWardrobeTurnContainers()` adds `pendingInventoryAnnouncements: new Map()`; `turnContainersOf` threads it. **Shared contract with the tools lane** (§5).
- `lib/services/chat-message/orchestrator.service.ts` (`102a23648` hunk at `processMessage`, old `:1725-1730`): the `flushPendingInventoryAnnouncements(toolContext)` call MOVED from "beside the wardrobe flush" (before the save) into a `try { … every save branch … } finally { await flushPendingInventoryAnnouncements(toolContext) }` (the new `:1810` `finally`) — Pascal's netted line posts AFTER the saved turn; the wardrobe flush stays BEFORE. Order-observable on the tier-3 orchestrator corpus (message row order). Tools-lane territory (the containers exist only with the tools); recorded here as the observable order.

**`lib/inventory/apply-defaults.ts` (196, NEW).**
- `:32` `InventoryArrivalSource = 'chat-start'|'participant-added'|'merge'`; `:34-39` options `{source; userId; sourceChatId?}`.
- `:42-48` `defaultIdsFor` = `loadInventoryPool(repos, {characterId, chatId: chat.id, projectId: chat.projectId ?? null}).resolved().filter(equippedByDefault && !isArchetype && !archivedAt).map(id)` (kits INCLUDED — equip expands them; `resolved()` is already non-archived).
- `:51-60` `equipDefaults` → `equipItemsHoldingLock(repos, chat, characterId, ids, {userId, provenance:'operator', announce:false}, {lenient:true})`; `null` when no ids.
- `:63-79` **`applyInventoryDefaults(repos, chatId, characterIds, opts)`** — try/catch whole → WARN `[InventoryArrival] Failed to apply arrival defaults` `{chatId, source, error}`.
- `:81-109`: `chats.findById(chatId)` null → DEBUG `[InventoryArrival] No such chat; nothing to apply` `{chatId, source}`; `sourceChat` read when `sourceChatId`; per UNIQUE character (`Set` order) `withChatInventoryLock(chat.id, () => arriveUnderLock(…))`, a throw → WARN `[InventoryArrival] Failed to apply arrival defaults for a character` `{chatId, characterId, source, error}` and continue.
- `:111-159` **`arriveUnderLock`**: `current = chats.findById(chatId) ?? chat` (re-read inside the lock); `hasEquippedEntry(current, characterId)` → DEBUG `[InventoryArrival] Character already has an entry; defaults not revisited` `{chatId, characterId, source}` and return; `sourceChat && hasEquippedEntry(sourceChat, characterId)` → `setEquippedInventory(chatId, characterId, equippedIdsOf(sourceChat, characterId))` + DEBUG `[InventoryArrival] Copied the source chat's carried objects` `{op:'arrival', chatId, sourceChatId, characterId, source, equipped: ids}` and return (ids copied VERBATIM, unresolved ones included — the reader drops them later); else `equipDefaults(repos, current, …)`; `!result || result.equipped.length === 0` → **`setEquippedInventory(chatId, characterId, [])`** (the empty-entry stamp — so a later re-seat is not revisited); DEBUG `[InventoryArrival] Applied arrival defaults` `{op:'arrival', chatId, characterId, source, equipped: ids, created: ids, skipped}`.
- `:166-196` `resetInventoryToDefaults(repos, chat, characterId, userId)` under the lock: `setEquippedInventory(chat.id, characterId, null)` (DELETE the entry), re-read, `equipDefaults`, the `[]` stamp when nothing equipped, re-read, DEBUG `[InventoryArrival] Reset to defaults` `{op:'reset-defaults', chatId, characterId, equipped}`; returns `equippedIdsOf(after)` (the SORTED, de-duped stored list). NB: `assertParticipant` fires inside `equipItemsHoldingLock` — but `equipDefaults` returns null BEFORE it for a character with no defaults, so a non-participant with no defaults is stamped `[]` rather than refused; the route checks `castOf` first (`inventory.ts:94-96`).
- **Call sites:** `app/api/v1/chats/route.ts:1441-1449` — in `handleCreate`, AFTER the outfit `try/catch` (`:1385-1439`) and BEFORE `compileAllIdentityStacks` (`:1451-1461`), with every `CHARACTER` participant's id, `{source:'chat-start', userId: user.id, sourceChatId: validatedData.continuationFromChatId ?? null}`; `app/api/v1/chats/[id]/actions/participants.ts:240-242` — the LAST statement of `applyOutfitForAddedParticipant(…)` (after its outfit try/catch), `{source:'participant-added', userId}` for the one character (both the add-participant `:371` and the reactivation `:452` callers reach it — the reactivation ONLY when an `outfitSelection` was sent, since that is the only path that calls `applyOutfitForAddedParticipant` there); `lib/chat/apply-chat-merge.ts:265-267` — per merged character, right after its outfit selection, `{source:'merge', userId, sourceChatId}` (the merged chat).

**`lib/inventory/prompt-section.ts` (79, NEW).** `INVENTORY_SECTION_HEADER = '## Carried and equipped'` (`:36`). `renderInventorySection(items)` (`:39-46`): keep `kind !== 'kit' && description?.trim()`, **sort `inventoryLabel(a).localeCompare(inventoryLabel(b), undefined, {sensitivity:'base'})`** (STABLE — V8's `Array.prototype.sort` is stable; ties keep equipped-set order), lines `- ${inventoryDisplayName(item)}: ${description.trim()}`, joined `\n` under the header; empty → null. `buildInventorySection({chat, characterId, repos})` (`:55-79`): `equippedIdsOf(chat, characterId).length === 0` → null with NO pool read; else `loadInventoryPool` + `readEquippedSet`; DEBUG `[InventorySection] Built the carried-objects section` `{chatId, characterId, present, op:'prompt-section'}`; a throw → WARN `[InventorySection] Failed to build the carried-objects section; turn continues` `{chatId, characterId, error}` → null.

**`lib/chat/context-manager.ts`** (+74 in the span; the inventory hunks are `4fbdad186` + `102a23648`, the `offerYield` / `turnYieldInstruction` hunks at `:550-555, 2204-2215, 2268` are `2b5e8ee24`'s — the interrupts lane):
- `:157` `import { buildInventorySection } from '@/lib/inventory/prompt-section'`.
- `:476-484` the `isContinueMode` doc comment: `Continue mode: any turn built without a new user message — the Salon's Continue, a nudge, every chained speaker after the first, and every autonomous-room turn. Never a model finishing its own sentence. The Aurora Core whisper skips these turns by design; the other trailing sections (recall, mail, progressions, inventory, inform) are built on them as on any turn.` (v5 carries this without "inventory" since P4.D322 — add the word).
- `:2170-2183` the ungated `progressionsLLMContext` (PORTED, P4.D322).
- `:2185-2193` **`const inventoryLLMContext = (await buildInventorySection({ chat, characterId: character.id, repos: getRepositories() })) ?? ''`** — UNGATED (bug 253; the first-shipped `4fbdad186` shape was `!isContinueMode ? … : ''` — port the pin's).
- `:2253-2257` the trailing-order comment: `what it is carrying (progressions, then the Inventory), what the operator says is so (inform), and the turn note last`.
- `:2260-2268` the ONE list: `[coreWhisperLLMContext, llmRecallText, suparnaMailLLMContext, progressionsLLMContext, inventoryLLMContext, informBlock, turnSkipInstruction, turnYieldInstruction].filter(Boolean)` — **inventory sits between progressions and inform**.

**`lib/services/carina/carina.service.ts:67, 592-598, 643`:** `inventorySection = await buildInventorySection({ chat, characterId: answerer.id, repos })`; `questionWithProgressions = [question, progressionsSection, inventorySection].filter(Boolean).join('\n\n---\n\n')` (was a two-part ternary — SAME bytes when no inventory); `:643` the `false, // canManageInventory — no inventory_* tools here` positional in `buildTools` (e1872c94f — the tools lane owns `buildTools`'s new argument; the keystone's Carina hunk is the section only).

**`lib/chat/initial-greeting.ts:51-56, 145, 160-176`:** NEW `GreetingRequest.trailingContextSections?: string[]`; `greetingInstruction = 'The chat is beginning now. Greet the user immediately in-character and invite them to engage.'`; `trailing = sections.filter(s => s.trim().length > 0)`; when non-empty DEBUG `[Greeting Generation] Appending trailing context sections to the greeting turn` `{chatId, characterId, sectionCount}` and the user message = `${greetingInstruction}\n\n---\n\n${trailing.join('\n\n---\n\n')}`; the system prompt UNCHANGED.

**`app/api/v1/chats/route.ts`** (`autoGenerateFirstMessage`): `:771-784` the `chatRow = await repos.chats.findById(chatId)` read MOVED UP (it was at old `:782-786`, after `baseParams`) and `openerInventory = chatRow ? await buildInventorySection({chat: chatRow, characterId: context.character.id, repos}) : null`; `:786-788` `baseParams` gains `...(openerInventory ? { trailingContextSections: [openerInventory] } : {})`; `:877` the SAME spread on the `generateGreetingMessage` call. Since `applyInventoryDefaults` ran at `:1441` (before the opening sequence), the opener's defaults ARE carried when the greeting is built.

**`lib/inventory/image-description.ts` (74, NEW).** `renderCarryingPhrase(items)` (`:27-33`): keep `kind !== 'kit' && imageDescription?.trim()`, sort by `inventoryLabel` `localeCompare(…, undefined, {sensitivity:'base'})`, map `imageDescription.trim().replace(/[.!?]+$/, '')`, `carrying ${phrases.join(', ')}` or null. `describeEquippedInventoryForImages(chat, characterId, pool)` = render(readEquippedSet). `loadCarryingPhrase(repos, chat, characterId)` (`:48-74`): `equippedIdsOf` empty → null (no read); DEBUG `[InventoryImages] Built the carrying phrase` `{chatId, characterId, present, op:'image-phrase'}`; throw → WARN `[InventoryImages] Failed to build the carrying phrase; picture goes without it` → null.

**`lib/image-gen/appearance-resolution.ts`** (`4fbdad186`'s hunk; the `406dbdd5a` moderated-painter hunk in the same file is the smalls survey's):
- `:62-68` `ResolvedCharacterAppearance.carryingDescription?: string`.
- `:243-257` `resolveCharacterAppearances(...)` now = `resolveAppearancesWithoutInventory(...)` (the old body, renamed `:288`) then `if (!chatId || appearances.length === 0) return result; return {...result, appearances: await withCarrying(result.appearances, chatId)}`.
- `:264-286` `withCarrying`: dynamic imports; `chats.findById(chatId)` null → as were; per appearance `loadCarryingPhrase(repos, chat, appearance.characterId)` → `{...appearance, carryingDescription}` when non-null; catch → WARN `[AppearanceResolution] Could not add carried objects to appearances` `{context:'image-gen.appearance-resolution', chatId, error}`.
- `:458-461` the classifier text: `[physicalDescription, clothingDescription, carryingDescription].filter(Boolean).join(' ')` per character, joined ` | ` — **was `${phys} ${cloth}`**: with an EMPTY clothing (or physical) the old form kept a trailing/leading space the new form drops — a byte change on the moderation request even with no inventory (the ledger's PORT hunk (1)).
- `:510-513, 538-549` the sanitizer input `sanitizableAppearanceText(a)` = `` `${phys}. ${cloth}`.trim() `` then `` `${base}. ${carrying}`.trim() `` when carrying; `textChanged` compares against the same; a changed text sets `physicalDescription = sanitized.appearanceText, clothingDescription: '', carryingDescription: undefined, wasSanitized: true`.
- `:556-568` `sanitizableAppearanceText` (pure); `:574-583` **`sceneDescriptionParts(resolved, genderPrefix = '')`** (pure, exported): `[genderPrefix + physical]`, `Wearing: ${clothing}` when non-empty, then the carrying phrase with its FIRST CHAR upper-cased (`carrying.charAt(0).toUpperCase() + carrying.slice(1)` — a JS `charAt` = one UTF-16 unit).
- `lib/background-jobs/handlers/story-background.ts:26, 475` — `descParts = sceneDescriptionParts(resolved, genderPrefix)` replaces the inline two-part build (identical bytes when no carrying).
- `lib/image-gen/prompt-expansion.ts:324-338` (`buildExpansionContext`): the `clothing` array is built when `clothingDescription || carryingDescription`; the outfit entry as before (`Current outfit (from story)` / `Current outfit`), then `{ name: 'Carried', usageContext: null, description: carryingDescription }` when present.
- `lib/image-gen/avatar-prompt.ts:49-55, 220-246`: `BuildPromptOptions.carrying?: string|null`; `carryingText` = trimmed, first char upper-cased, trailing `[.!?]+` stripped, then `.` appended; `carryingBlock` = `${carryingText}\n\n` when there IS an outfit block, else `${carryingText} ` (one space); the prompt = `${intro} ${physBlock}${outfitBlock}${carryingBlock}${outro}` / `${intro}${outfitBlock}${carryingBlock}${outro}`; `hasAppearance` UNCHANGED (physical || outfit — carrying alone does NOT make an appearance).
- `lib/background-jobs/handlers/character-avatar.ts:35, 158-165`: `carrying = await loadCarryingPhrase(repos, chat, payload.characterId)` BEFORE `buildCharacterAvatarPrompt(…, {equippedSlots, projectMountPointIds, characterAesthetic, carrying})`.

**`lib/services/pascal/writer.ts:33-39, 184-228`:** `PASCAL_INVENTORY_CHANGE_KIND = 'inventory-change'`; `postPascalInventoryChange({chatId, content})`: `chats.findById` null → DEBUG `Pascal inventory line dropped — no such chat` `{context, chatId}` → null; message `{type:'message', id: randomUUID(), role:'ASSISTANT', content, opaqueContent: content, attachments: [], createdAt: now, participantId: null, systemSender: 'pascal', systemKind: 'inventory-change', targetParticipantIds: null}` — **NO `pascalMeta`**; `chats.addMessage`; INFO `[Pascal] Inventory change posted` `{context, chatId, messageId}`; catch → ERROR `[Pascal] Failed to post inventory change` `{context, chatId, error}` → null.

**`lib/inventory/roster-gate.ts` (54, NEW, `102a23648`):** `isCharacterGesture(p)` = `p !== 'operator'`; `projectShelfOpenTo(projectId, characterId)` = no project → true; `projectRosterAdmits(projectId, characterId)`; a throw → WARN `[InventoryRoster] Roster lookup failed; the project shelf stays closed` `{projectId, characterId, error}` → false; `isOnClosedShelf(item, carriedIds)` = `item.origin?.scope === 'project' && !carriedIds.has(item.id)`. `lib/projects/roster-access.ts:4-14` is COMMENT-ONLY (±8; `projectRosterAdmits` and the policy unchanged — v5's `project_roster_access.rs:1-12` header carries the old wording).

**`lib/inventory/context.ts` (78; e1872c94f + 102a23648)** — the tools lane's `loadInventoryContext` (DEBUG `[InventoryContext] Loaded` `{characterId, chatId, reachable, equipped, projectShelf: 'none'|'open'|'closed by roster'}`); recorded here because `resolvedPoolOf(pool)` (`:40-47` — reachable + the holder's OWN archived records) is the shape the title lint's `resolvedWith` (`title-lint.ts:242-248`) mirrors. Home: tools lane (§5).

### 1.7 Mutations, pictures, transfers, the title lint

**`lib/inventory/item-mutations.ts` (592; 552 @`4fbdad186`, +52 `7263105c6`).** Every refusal: `refuse(error, meta)` → INFO `[InventoryMutations] Refused` `{code, message, ...meta, context:'inventory'}` then throw (`:82-85`); every success `logOp(op, location, meta)` → DEBUG `[InventoryMutations] ${op}` `{op, scope, mountPointId, ...meta, context}` (`:87-95`).
- `:103-122` `assertKitMembers(pool, itemIds, meta, allowMissing)`: a member `pool.get(id)` missing (and not excused) → `missing`; `kind === 'kit'` → `kits`; KITS refuse FIRST (`KIT_NESTING`), then missing (`INVALID_SHAPE` `Kit members not reachable from this tier: …` `{missing}`).
- `:124-127` `assertShape` → `INVALID_SHAPE` `issues.join('; ')` `{issues}`.
- `:142-177` **`createInventoryItem(repos, location, body, {preserve?, pool?})`**: meta `{scope, mountPointId, op:'create'}`; `assertShape(fields)`; `parentId` → `INVALID_PARENT` `Create an instance with instantiate`; `itemIds` de-duped; a kit loads `loadPoolForLocation` (no context) unless given and `assertKitMembers`; record = `{...withInventoryDefaults({...fields, id: preserve?.id ?? randomUUID(), createdAt/updatedAt: preserve's ?? now, folder: ''}), itemIds, svg: sanitizeInventorySvg(fields.svg), archivedAt: preserve?.archivedAt ?? null, imageFileId: preserve?.imageFileId ?? null, folder: folder ? await location.resolveFolder(folder) : ''}` — NB `svg` is written even when null/undefined → the serialiser skips `undefined` but writes `null` (sanitize returns null for empty → `"svg": null` IS in the file; likewise `archivedAt: null`, `imageFileId: null`, `parentId: null`, `title`/`description` only if the body sent them); lint `{door:'create', before:null, after: record}` with `{pool}` UNLESS `preserve` (a transfer is a move); `location.create`; `writeTitleLintRows` after; `logOp('Created object', {itemId, kind, isArchetype, folder})`.
- `:195-234` **`instantiate(repos, location, archetypeId, {folder?, overrides?, pool?})`**: `pool.get(archetypeId)` not an archetype → `INVALID_PARENT` `The parent must be an archetype this tier can reach` `{archetypeId}`; archived → `An archived archetype cannot be instantiated`; overrides filtered to inheritable keys with `!== undefined`; `svg` in overrides sanitised; record `{id: randomUUID(), parentId, ...overrides, createdAt: now, updatedAt: now, folder: resolved | '', fileStem: archetype.title || archetype.name || undefined}` (KEY ORDER: id, parentId, overrides…, timestamps — the serialiser re-orders to `INVENTORY_FILE_KEYS` anyway); lint only when `overrides.title` is a non-blank string; returns `{record: stored, item: resolveItem(stored, id => id === archetypeId ? archetype : pool.get(id))}`; `logOp('Instantiated archetype', {itemId, archetypeId, overrides: keys})`.
- `:247-305` **`updateInventoryItem(repos, location, current, body, context)`**: `patch` = defined fields; an INSTANCE (`current.parentId`): `patch.isArchetype` truthy → `INVALID_PARENT` `An instance cannot be an archetype`; `kind === 'kit'` or non-empty `itemIds` → `INVALID_SHAPE` `An instance is an item, never a kit`; then `kind`/`isArchetype`/`itemIds` DELETED from the patch (silently); a NON-instance: `nextKind = patch.kind ?? current.kind ?? 'item'`; `isArchetype: true` on a kit → `A kit cannot be an archetype`; `isArchetype: false` on an archetype with dependents → `ARCHETYPE_IN_USE`; `kind: 'kit'` on an archetype not also un-archetyping → `An archetype cannot be a kit`; `assertShape({...current, ...patch})`; a kit with `itemIds` given → de-dup + `assertKitMembers(loadPoolForLocation(repos, location, context), …)`; `svg` sanitised when given; `folder` given and `!isSameFolder(folder, current.folder)` → `patch.folder = resolveFolder`; `archived` given → `archivedPatch(current.archivedAt, archived, now)` (`lib/wardrobe/archived-patch.ts` — v5 `wardrobe.rs:1454`) → `patch.archivedAt`; lint `{door:'update', before: current, after: {...current, ...patch}}` with `{context}`; `location.update(current.id, patch)`; rows written only when `updated`; `logOp('Updated object', {itemId, fields: keys, found})`.
- `:308-323` `clearOverrides(location, current, fields)`: not an instance → `Only an instance inherits; this object has no archetype`; lint with `after` = current minus the fields (door `update`, repos = `getRepositories()`); `location.update(id, {}, fields)`; `logOp('Cleared overrides', {itemId, fields})`.
- `:326-339` `setInventoryItemArchived(location, current, archived)` → `archivedPatch`; no change → `{item: current, changed: false}` with NO write/log; lint door `'archive'`; `logOp('Archive state changed', {itemId, archivedAt})`. (NB: the ROUTE never calls this — `handleUpdate` routes `archived` through `updateInventoryItem`; the function is for the tools lane's `inventory_archive`.)
- `:346-357` `bakedFields(record, resolved)` = `{parentId: null, kind:'item', isArchetype:false, itemIds: [], <10 inheritable fields from resolved except imageFileId>, imageFileId: record.imageFileId ?? null}`.
- `:360-373` `detach(repos, location, current, pool?)`: not an instance → `This object has no archetype to detach from`; `location.update(id, bakedFields(current, pool.resolve(current)))`; `logOp('Detached instance', {itemId, parentId, parentMissing})`. NO lint.
- `:389-432` `detachAll(archetypeId)`: `findDependents`; any `ownerArchived` → `ARCHIVED_DEPENDENTS` (unique by ownerId); none → 0; the archetype read via `findRecordsAnywhere`; grouped by mount; a character mount re-resolved through `resolveVaultFolderMount(ownerId)` (a tombstone THROWS `CharacterArchivedError` here — unreachable after the check), a shared/unknown mount `{mountPointId, scope: unknown ? 'project' : scope, characterId: null}`; `updateInventoryManyInMount(mount, patches)` per mount; DEBUG `[InventoryMutations] Baked instances in one mount` `{op:'detach-all', mountPointId, scope, itemId, count}`.
- `:446-477` **`deleteInventoryItem(repos, location, current, {force})`**: `await location.writableMountPointId()` FIRST (the tombstone refuses before anything); an archetype with dependents → `ARCHETYPE_IN_USE` unless `force` → `detachAll` first; lint door `'delete'` (`after: null`); `location.delete([id], remaining => kits naming it WITH MORE THAN ONE member → `{id, patch: {itemIds: minus it}}`)` — a kit whose ONLY member it is keeps naming it; `deleted.length > 0` → rows + `cleanupInventoryItemImages(repos, id, '[InventoryMutations]', meta)`; `logOp('Deleted object', {itemId, deleted, forced, detached})`; returns `{deleted, detached}`.
- `:484-487` `assertCopyable(items, meta)` → `NOT_COPYABLE` with the blocked ids.
- `:490-502` `copyRecord(source, folder, title?)` = `{...source, id: randomUUID(), folder, title (when `!== undefined`), archivedAt: null, imageFileId: null, createdAt: now, updatedAt: now}` (a copy keeps `fileStem` — harmless, never serialised).
- `:505-520` `duplicateInventoryItem(repos, location, current)`: `assertCopyable([pool.resolve(current)], {itemId, op:'duplicate'})`; `titles = readItems(true).map(title || name || '')`; `nextCopyTitle(resolved.title || resolved.name, titles)` (`lib/wardrobe/next-copy-title.ts` — v5 `wardrobe.rs:1536`); lint door create; `logOp('Duplicated object', {itemId, copyId})`.
- `:523-533` `moveInventoryItemsToFolder(location, itemIds, folder)` → `resolveFolder`, `updateMany(unique ids → {folder}, {requireAll: true})`; `logOp('Moved objects to folder', {folder, requested, moved})`.
- `:536-560` `copyInventoryItemsToFolder(repos, location, itemIds, folder)`: missing ids → `InventoryItemNotInLocationError(missing)` (a 400 at the route, NOT an `InventoryRefusal`); `assertCopyable(sources resolved, {op:'copy-items', folder})` — ANY non-copyable refuses the whole batch; per source `copyRecord(source, target)` (title KEPT), lint create, `location.create` ONE AT A TIME (N projections); `logOp('Copied objects to folder', {folder, count})`.
- `:567-592` folders: `createInventoryFolder` (`resolveFolder` → empty → `InventoryFolderPathError('A folder needs a name')`; `logOp('Created folder', {requested, folder})`); `renameInventoryFolder` (`from = path.trim().replace(/^\/+|\/+$/g,'')`, `normalizeInventoryFolderName(name)`; `logOp('Renamed folder', {from, to})`); `deleteInventoryFolder` (`logOp('Deleted folder', {folder, parent, movedItems, movedFolders})`, returns `{movedItems, movedFolders}` — `parent` dropped from the response).

**`lib/inventory/item-images.ts` (237, NEW).** `LOG_CONTEXT = 'inventory.item-images'`; re-exports `INVENTORY_IMAGES_FOLDER` and `ForeignWardrobeImageError as ForeignInventoryImageError`; `InventoryImageSummary = WardrobeItemImageSummary & {inherited?}`; `InventoryItemHome {location; record; item}`. `listInventoryItemImages` = `listWardrobeItemImages(repos, itemId)` (the SAME `files.findByLinkedTo` read — pictures of either kind are told apart by `linkedTo` only). `listInventoryImagesWithFallthrough(repos, item)` (`:72-86`): own pictures, else when `parentId` the ARCHETYPE's pictures each `{...summary, inherited: true}` + DEBUG `[InventoryImages] Instance shows its archetype's pictures` `{context, itemId, parentId, count}`. `assertInventoryImageChoice` = `assertItemImageChoice`. `SOURCE_BY_KIND = {generated:'GENERATED', uploaded:'UPLOADED', imported:'IMPORTED'}`. **`addInventoryItemImage(repos, home, input)`** (`:116-157`): `writableMountPointId()`; `writeWardrobeItemImage({mountPointId, itemId: record.id, kind, content, contentType, description: `Inventory image for “${title || name}”` (curly quotes U+201C/U+201D), imagesFolder: 'Inventory/images'})`; `createGeneratedFileRow(repos, {userId, sha256, originalFilename: leafName, stored, width, height, linkedTo: [id], tags: [id], source, generation: {prompt, model, revisedPrompt}})`; `location.update(id, {imageFileId: file.id})`; INFO `[InventoryImages] Added inventory object image` `{context, scope, itemId, fileId, kind, sizeBytes}`. `setCurrentInventoryItemImage` (`:160-170`): choice check, tombstone, update, DEBUG `[InventoryImages] Set current inventory object image` `{context, itemId, fileId}`. `deleteInventoryItemImage` (`:173-190`): not one of its own → `ForeignWardrobeImageError`; tombstone; `removeImageFile(repos, id, target, 'Inventory/images')`; the next current = when the deleted was current OR the current no longer exists → first remaining by the list order (newest first) else null, written through `location.update`; INFO `[InventoryImages] Deleted inventory object image` `{context, itemId, fileId, current}`. `cleanupInventoryItemImages` / `carryInventoryItemImages` / `commitMovedInventoryImages` delegate with `imagesFolder: 'Inventory/images'`. `resolveInventoryItemHome(repos, userId, scope, containerId, itemId)` (`:224-237`): `resolveInventoryLocation(scope, containerId, repos, userId)` (NO ensure), `findItem` (archived included), `loadPoolForLocation`, `{location, record, item: pool.resolve(record)}`.

**`lib/inventory/item-image-generation.ts` (160, NEW).** `LOG_CONTEXT = 'inventory.item-image-generation'`. `NoInventoryImageProfileError`: `No image profile is configured. Set one in Settings → Images before generating pictures of objects.` (`:37`). `InventoryImageGenerationError(message, trail, refused)`. `inventoryImageSubject(item)` = `imageDescription?.trim() || description?.trim() || title?.trim() || name` with trailing `[.!?]+` stripped. **`buildInventoryItemImagePrompt`** = `Still-life product photograph of ${subject}. A single object, centred, shown whole on a plain neutral ground, even soft light, no person and no hands. The object is the subject.` (`:60`). `generateInventoryItemImage(repos, {userId, home, imageProfileId?})` (`:72-160`): tombstone FIRST; `resolveWardrobeImageProfile(userId, repos, imageProfileId)` — no profile / no `apiKeyId` / no key → `NoInventoryImageProfileError`; `projectMount` only for a `project` location; `prompt = withArtDirection(build…, await resolveAesthetic({kind:'lantern', projectOfficialMountPointId: projectMount ?? undefined}))`; DEBUG `[InventoryItemImage] Generating inventory object image` `{context, itemId, scope, profileId}`; `conciergePolicy = resolveConciergeSettings(chatSettings.findByUserId(userId) ?? null, null)`; `makeLoggedImageAttempt({userId, logType: 'INVENTORY_ITEM_IMAGE', prompt, primaryProfileId, chatId: null, characterId: location.characterId, params: {overrides: {n: 1, style: 'natural'}, orientation: 'square', logContext: {context, itemId}}})`; `generateImageWithConciergeFailover({profile, apiKey}, attempt, {userId, chatId: null, chat: null, purpose: 'dialog', conciergePolicy, primaryVia: 'primary'})`; a throw → `trail = getConciergeTrail(error)`, `refused = trail?.some(outcome === 'refused')`, WARN `[InventoryItemImage] Inventory object image generation failed` `{context, itemId, error, refused}`, throw `InventoryImageGenerationError(message, trail, refused)`; `decodeProviderImage(failover.result, 'inventory')` null → `InventoryImageGenerationError('The image provider returned no picture', trail.length ? trail : null, false)`; `addInventoryItemImage(…, {kind:'generated', content, contentType, width, height, generationPrompt: prompt, generationModel: failover.profile.modelName, generationRevisedPrompt: decoded.revisedPrompt})`; INFO `[InventoryItemImage] Inventory object image generated` `{context, itemId, fileId, profileId: failover.profile.id, rerouted}`; returns `{fileId, prompt, profile: {id, name}, rerouted, trail: trail.length > 0 ? trail : null}`. Compare the wardrobe twin (`wardrobe_item_image_generation.rs:264`): the Inventory has NO component leaves, NO `chatId`, `purpose 'dialog'`, orientation `square`, `style 'natural'`.

**`lib/inventory/transfers.ts` (141; extracted from the route by `e1872c94f` — the route is the keystone's, `inventory_give` the tools lane's consumer).** `transferInventoryItem(repos, {action, source, destination, record, userId})` (`:54-141`): same `locationKey` → `InventoryTransferSameLocationError`; `copy` → `assertCopyable([sourcePool.resolve(record)], {op:'transfer-copy', itemId})`; `move` → `source.writableMountPointId()` (an archived source refuses BEFORE anything lands); a kit with members → `destinationPool = loadPoolForLocation(repos, destination)`, members unseen there are excused when `findRecordsAnywhere` finds their mount and `isReachableFromKitTier(destination.mountPointId, {mountPointId, scope: mountScopeOf(mp)})` → `assertKitMembers(destinationPool, itemIds, meta, contextual)`; a MOVE of a non-kit → `findKitsStrandedByMove(record.id, {mountPointId: destination.mountPointId, scope: destination.scope})` minus itself → INFO `[InventoryTransfers] Move refused: it would strand kits` `{op, itemId, kits, context}` + `KIT_STRANDED`; `planned = {...record, id: copy ? randomUUID() : record.id, folder: normalizeInventoryFolderPathOrRoot(record.folder), archivedAt: copy ? null : record.archivedAt ?? null, createdAt/updatedAt: copy ? now : record's}`; an id already at the destination → `INVALID_SHAPE` `An object with that id already exists at the destination`; `carryInventoryItemImages(repos, {mode: action, sourceItemId, destinationItemId: planned.id, destinationMountPointId: await destination.writableMountPointId(), userId})` BEFORE the object lands; `planned.imageFileId = record.imageFileId ? fileIdMap.get(it) ?? null : record.imageFileId`; `destination.create(planned)` (NO lint — `preserve` semantics: the create path here is `location.create` directly, not `createInventoryItem`); a move → `source.delete([record.id])` (`[]` → throw `Failed to remove the object from its source after the move`) then `commitMovedInventoryImages(repos, record.id, pendingMove)`; INFO `[InventoryTransfers] Inventory object transferred` `{userId, action, itemId, resultItemId, from: source.scope, to: destination.scope, folder, context}`; returns `{stored, item: destinationPool.resolve({...stored, origin, mountPointId})}` (a fresh pool load).

**`lib/inventory/title-lint.ts` (411, NEW, `7263105c6`).**
- `:89-98` `normalizeTitle(v)` = `(v ?? '').trim().toLowerCase().normalize('NFKD').replace(/\p{M}+/gu, '').replace(/\s+/g, ' ').trim().toLowerCase()` — NFKD + strip marks (`\p{M}`), collapse whitespace; punctuation kept.
- `:101-108` `TitleLintDoor = 'create'|'update'|'archive'|'delete'|'document-write'|'document-delete'|'document-move'`; `:110-130` `TitleLintRequest {door; before; after}`, `TitleLintRow` (five reasons: `gate key naming {title, tools}`, `gate dark {gateKey, tools}`, `gate redirected {gateKey, tools, newOwners}`, `gate unverifiable {gateKey, tools, unread}`, `gate key moved {gateKey, tools}`), `TitleLintVerdict {event: {entering, leaving}; rows}`; `:132-139` options `{pool?, context?, census?}`.
- `:149-155` `takeCensus` → `collectToolGateKeys()` (lazy import); `:157-165` `mountName(mp)` via `docMountPoints.findById` (throw → null); `:165` `storeLabel(id, name)` = `store "${name}"` | `store ${id}`; `:168-171` `describeItem(item)` = `${title || name} (${origin.name})` when an origin name, else the label.
- `:188-226` **`read(repos, location, opts)`**: pool = `opts.pool ?? loadPoolForLocation(repos, location, opts.context ?? {})` — a throw → `unread.push({pool: `the ${scope} inventory "${origin.name || mountPointId}"`, reason})`; every `pool.unreadMounts` → `{pool: `the inventory in ${storeLabel(mp, name)}`, reason: error}`; every `consultedMounts` not failed: `mountName === null` → `{pool: `the inventory in store ${mp}`, reason: 'no such store; an empty read from it is not an answer'}` else `pools.push(storeLabel)`; census — each `census.unread` → `{pool: `${definitionPath} in store "${mountName}"`, reason}`; a thrown census → `{pool: "the custom tools' Tools/ folders", reason}`.
- `:229-239` `sideOf(record, pool)` → `{id, title: resolved.title?.trim() ?? '', name: resolved.name ?? '', reachable: !resolved.archivedAt}` (resolved through the pool, or `resolveItem(record, () => undefined)` without one).
- `:242-248` `resolvedWith(pool, itemId, replacement)` → `{items: pool.resolved() with the item replaced by `{...item, title: replacement.title || null, name}` when `replacement?.reachable` else REMOVED, owns}`.
- `:251-260` `usesOf(census, key)` groups uses whose `normalizeTitle(ref) === key` by the ref as written (Map insertion order); `uniqueTools` = sorted unique tool names (default JS sort).
- `:268-343` **`lintInventoryTitle`**: `beforeKey = before?.reachable ? normalizeTitle(before.title) : ''`, same `afterKey`; `nameChanged` = both sides and normalised names differ; `entering = !!afterKey && (afterKey !== beforeKey || nameChanged)`; `leaving = !!beforeKey && beforeKey !== afterKey`; DEBUG `[InventoryTitleLint] Census` `{door, itemId, scope, mountPointId, context:'inventory', entering, leaving, pools, reachableObjects: pool?.byId.size ?? null, toolStores: census?.storesConsulted ?? [], gateKeys: census?.keys.length ?? null, unread}` on EVERY call; entering: `unread.length > 0 || !pool || !census` → INFO `[InventoryTitleLint] Title refused: protected set incomplete` `{…meta, title, unread}` + `TITLE_CHECK_UNAVAILABLE`; kind names = `Map(normalizeTitle(name) → name)` seeded with the object's own new name, then every `pool.byId` record except itself (resolved), first spelling wins; `afterKey` found → `kind-name shadow`; gate keys: for each `usesOf(census, afterKey)` ref, `resolveInventoryRef(ref, resolvedWith(pool, itemId, null), new Set())` — no hit → `naming.push(...uses)`, hits → one `live gate shadow` per unique tool `{toolName, ref, landsOn: hits.map(describeItem)}`; collisions → INFO `[InventoryTitleLint] Title rejected` `{…meta, title, collisions}` + `TITLE_SHADOW`; naming → row `gate key naming {title, tools}`; leaving → `leavingRows`.
- `:346-378` `leavingRows`: no census → `[{gate unverifiable, gateKey: before.title, tools: [], unread: names}]`; per `usesOf(census, normalizeTitle(before.title))` ref: no pool → unverifiable; `hits = resolveInventoryRef(ref, resolvedWith(pool, itemId, after), new Set())`, `others = hits minus itemId` → `gate redirected {newOwners: describeItem}`; some hit (itself) → `gate key moved`; `unread.length > 0` → unverifiable; else `gate dark`; finally `census.unread.length > 0 && rows.length === 0` → one unverifiable row with `tools: []`.
- `:380-411` `writeTitleLintRows(verdict, itemId, location)`: per row INFO `[InventoryTitleLint] ${message}` where message = `Title doubles as address and gate key` (naming) / `A gate key was left behind; it still lands on this object by name` (moved) / `A gate key was left behind` (the three alerts); meta `{...row, reason: 'gate redirected: <owners joined ", ">' | 'gate unverifiable: <unread joined "; ">' | row.reason, itemId, scope, mountPointId, alert: reason ∈ {dark, redirected, unverifiable}, context:'inventory'}`.

**`lib/inventory/title-lint-documents.ts` (192, NEW).** `InventoryDocumentChange` = `{kind:'write', mountPointId, relativePath, content}` | `delete` | `{kind:'move', fromPath, toPath}` | `move-folder`. `locationForMount(mp)` (`:49-70`): a character vault (`findByDocumentMountPointIdRaw`) → character location; General → general; any other mount → a `project`-scoped location with origin `{scope:'project', id: null, name: mount.name}` (a GROUP store lints as `project` — its pool = own folder + General, which is the same); unknown mount → null. `priorContent` = `docMountDocuments.findByMountPointAndPath(mp, path)?.content ?? ''` or null. `lintInventoryDocumentChange(change)` (`:105-192`): `inventoryWritesAreLinted()` → no-op; a `Tools/*.tool.json` path (not a folder move, no Inventory path) → census only, DEBUG `[InventoryTitleLint] Census` `{kind, mountPointId, paths, context, door:'document-write', entering:false, leaving:false, toolStores, gateKeys, unread}` (a thrown census → `unread: [{error: String(e)}]`); no location → `TITLE_CHECK_UNAVAILABLE(paths[0], [{pool: `store ${mp}`, reason: 'no such store; an empty read from it is not an answer'}])`; a prior read that throws → `TITLE_CHECK_UNAVAILABLE(labelOf(path, incoming), [{pool: `${path} in store "${name}"` | `${path} in store ${mp}`, reason: `its prior state could not be read: ${msg}`}])`; `move-folder` only when exactly one side `isTitleLintFolder` — lists `findManyByMountPointsInFolder([mp], fromPath, '.json', {recursive:true})` (a throw → unavailable `{pool: `the folder ${fromPath}`, reason: `its contents could not be listed: ${msg}`}`), each file `to = toPath + rest`; per file: `prior`, `next = to === null ? null : content ?? prior`, `before = parse(from, prior)`, `after = parse(to, next)`, `pairsOf` (different ids in one file → `[{before, null}, {null, after}]`), `lintInventoryTitle(getRepositories(), location, {door, ...pair})`, rows kept when non-empty; DEBUG `[InventoryTitleLint] Document change linted` `{…meta, files, rows}`; returns the row writer.

**`lib/inventory/lint-scope.ts` (55).** `AsyncLocalStorage<true>` under `processSingleton('quilltap.inventory.title-lint-scope')`; `withInventoryWritesLinted(fn)` / `inventoryWritesAreLinted()`; `isToolDefinitionPath(p)` = `/^tools\/[^/]+\.tool\.json$/i` on the path with leading slashes stripped; `isTitleLintFolder(p)` = lower-cased, trimmed of slashes, `=== 'inventory' || startsWith('inventory/')`; `isTitleLintPath(p)` = tool definition OR (lint folder AND `/\.json$/i`).

**`lib/mount-index/database-store.ts:30-31, 44-52, 156, 190, 211-215, 282, 306, 487, 573`:** `lintInventoryChange(change)` — `inventoryWritesAreLinted()` → no-op; relevance (`move-folder` → either side `isTitleLintFolder`; else the paths `some(isTitleLintPath)`); lazy import of `lintInventoryDocumentChange`. Hooks: `writeDatabaseDocument` lints AFTER the native-text / size checks and BEFORE `ensureFolderPath` (`:156`), writes rows after `emitDocumentWritten` (`:190`); `deleteDatabaseDocument` lints after the link lookup, before `deleteWithGC`, rows after `emitDocumentDeleted`; `moveDatabaseDocument` lints before the destination folder ensure, rows after `emitDocumentMoved`; `moveDatabaseFolder` lints after the conflict checks (`:487`), rows after the per-doc `emitDocumentMoved` loop (`:573`).

**`lib/pascal/tool-gate-keys.ts` (93).** `collectToolGateKeys()` over `listAllCustomTools()` (the tools lane's NEW `entries {definition, mountName, definitionPath}`, `unread`, `errors[].stage`, `storesConsulted`): for each definition, each of `availableWhen` / `withheldWhen`, each `gateGroups(gate)` group (an any-of walk — `a53024d3c`), each subject `inventory` / `inventoryCount`, each `Object.keys(group[subject] ?? {})` → `{ref, toolName: definition.name, clause, subject, mountName, definitionPath}`; `unread` = library `unread` + `errors` with `stage === 'read' || 'json'`; `storesConsulted.length === 0` → `unread.push({mountName: '(no enabled stores)', definitionPath: 'Tools/', reason: 'the store listing came back empty'})`; DEBUG `[ToolGateKeys] Census taken` `{context:'custom-tools', stores, definitions, keys, unread}`. **Depends on the tools/Pascal lane's `listAllCustomTools` reshape** (`lib/pascal/custom-tools.ts` +59 in `7263105c6`: `loadToolsFromMount` returns `unread` + `CustomToolLoadError.stage`, `listAllCustomTools` returns `unread` + `storesConsulted`) and on `gateGroups` (`a53024d3c`).

### 1.8 The routes

**`lib/inventory/routes/inventory-route-factory.ts` (441, NEW).** Config `{scope, paramsToId, logTag, logIdKey}`; `LABELS = {character:'Character', group:'Group', project:'Project', general:'Quilltap General'}`; `projectContextOf(req)` reads `?projectId` / `?chatId`; `serialize(pool, location, record)` = `pool.resolve({...record, origin: location.origin, mountPointId})`.
- `makeResolver` (`:97-111`): `resolveInventoryLocation(scope, id, repos, user.id, {ensure})`; null → `general` → `serverError('Quilltap General store is not provisioned yet')` (500), else `notFound(LABELS[scope])` (404 `${Label} not found`).
- Collection `probe` (`:122-132`, `ensure:false`): found → location; general → null; a project/group that EXISTS but has no store → null (an empty answer); else 404.
- `GET` (`:305-307`): `withActionDispatch({folders, archetypes}, handleList)` — an unknown action → 400 (the middleware's); **`handleList`** (`:134-159`): no store → `{mountPointId: null, items: []}`; `includeArchived = readIncludeArchived(req)`; `shared = scope === 'character' && ?shared=true` → `loadInventoryPool(repos, {characterId, ...projectContextOf})`, `items = includeArchived ? byId.values().map(resolve) : resolved()`; else `loadPoolForLocation` + `readItems(includeArchived).map(serialize)`; DEBUG `${logTag} Listed inventory` `{[logIdKey]: location.id, mountPointId, count, shared, includeArchived, context}`; `{mountPointId, items}`. `handleGetFolders` → `{folders: []}` / `{folders}` + DEBUG `${logTag} Listed inventory folders` `{[logIdKey], count}`. `handleGetArchetypes` → `loadPoolForLocation(repos, found, projectContextOf(req)).resolved().filter(isArchetype)` → `{items}` + DEBUG `${logTag} Listed reachable archetypes`.
- `POST` (`:309-322`): `withActionDispatch({instantiate, 'create-folder', 'rename-folder', 'delete-folder', 'move-items', 'copy-items', duplicate}, handleCreate)`. **Parse BEFORE resolve** everywhere (an invalid body never provisions). `handleCreate` (`ensure: true`): `createInventorySchema.parse` (a ZodError → the middleware's 400); a kit → `loadPoolForLocation(…, projectContextOf(req))`; INFO `${logTag} Created inventory object` `{[logIdKey], userId, mountPointId, itemId, kind}`; `created({item: serialize(pool ?? loadPoolForLocation, …)})` (201). `handleInstantiate` (`ensure: true`): pool with `{projectId: body.projectId ?? null, chatId: body.chatId ?? null}`; INFO `${logTag} Instantiated archetype` `{[logIdKey], userId, itemId, archetypeId}`; `created({item: {...item, origin: location.origin}})`. `create-folder` (`ensure: true`) → `{folder}`; `rename-folder` (no ensure) → `{folder}`; `delete-folder` (no ensure) → `{movedItems, movedFolders}`; `move-items` / `copy-items` (no ensure) → INFO `${logTag} Moved|Copied inventory objects to folder` `{[logIdKey], userId, folder: body.folder, count}` + `{items: serialized}`; `duplicate` (no ensure): body `{itemId}` not a non-empty string → 404 `Inventory object not found`; `findItem` null → 404; INFO `${logTag} Duplicated inventory object` `{[logIdKey], itemId, copyId}`; 201 `{item}`.
- Item handlers (`:331-441`): `findRecord` = resolve (no ensure) + `findItem` → 404 `Inventory object not found`; `respondWith` → 404 or `{item: serialize}`. `GET` `withActionDispatch({dependents}, handleGetOne)`; `?action=dependents` → `{dependents: isArchetype ? findDependents(id) : []}` + DEBUG `${logTag} Listed archetype dependents` `{[logIdKey], itemId, count}`. `PUT` `withActionDispatch({'clear-override', detach}, handleUpdate)`: `handleUpdate` — `findRecord` THEN `updateInventorySchema.parse`; `body.imageFileId !== undefined` → `assertInventoryImageChoice` (a foreign id → 400 `imageFileId must name one of this object's own pictures` through `inventoryErrorResponse`); `updateInventoryItem(…, projectContextOf(req))`; INFO `${logTag} Updated inventory object` `{[logIdKey], userId, itemId}`; `clear-override` → `clearInventoryOverrideSchema.parse` → `clearOverrides`; `detach` → INFO `${logTag} Detached instance` `{[logIdKey], itemId}`. `DELETE`: `?force=true`; `deleteInventoryItem`; `!deleted` → 404; INFO `${logTag} Deleted inventory object` `{[logIdKey], userId, itemId, force, detached}`; `{success: true, detached}`.
- The eight thin route files: `app/api/v1/inventory/route.ts` (`general`, `paramsToId: () => null`, `logTag '[Inventory General v1]'`, `logIdKey 'generalId'`; `noParams` wrapper), `inventory/[itemId]/route.ts` (same config), `characters/[id]/inventory/{route,[itemId]/route}.ts` (`'[Characters v1]'`, `characterId`), `projects/[id]/inventory/…` (`'[Projects v1]'`, `projectId`), `groups/[id]/inventory/…` (`'[Groups v1]'`, `groupId`).

**`lib/inventory/routes/responses.ts` (41).** `inventoryErrorResponse(error)`: `InventoryRefusal` → `errorResponse(message, status, {code, ...details})`; `InventorySvgError` → `badRequest(message, {code: 'INVALID_SVG'})`; `InventoryFolderPathError` / `InventoryItemNotInLocationError` → `badRequest(message)`; `ForeignInventoryImageError` → `badRequest("imageFileId must name one of this object's own pictures")`; `InventoryFolderNotFoundError` → `notFound('Inventory folder')`; `InventoryFolderConflictError` → `conflict(message)`; `CharacterArchivedError` → INFO `[Inventory routes] Refused a write to an archived character` `{context:'inventory'}` + `conflict("An archived character's inventory cannot be changed")`; anything else RETHROWN (→ the middleware's 500).

**`app/api/v1/inventory/transfers/route.ts` (35).** `POST` only: `inventoryTransferSchema.parse`; `source = resolveInventoryLocation(from.scope, from.id, repos, user.id)` (no ensure) + `findItem` — either null → 404 `Inventory object not found`; `destination = resolveInventoryLocation(to.scope, to.id, …, {ensure: true})` null → 400 `Invalid destination`; `{item}` 200; refusals through `inventoryErrorResponse`.

**`app/api/v1/inventory/[itemId]/images/route.ts` (230).** `LOG_TAG = '[Inventory Images v1]'`; `containerQuerySchema = z.object({scope: z.enum(['character','project','group','general']), id: z.string().min(1).optional()}).refine(scope === 'general' || !!id, {message: 'id is required for this scope'})` (a bad scope → the Zod enum message joined by `; `); `generateBodySchema = {imageProfileId: z.string().min(1).nullable().optional()}`; `fileIdBodySchema = {fileId: z.string().min(1, 'fileId is required')}`; `uploadKindSchema = z.enum(['uploaded','imported']).default('uploaded')`. `findHome` → 400 (issues joined `; `) / `resolveInventoryItemHome` null → 404 `Inventory object not found`. `GET` `withActionDispatch({'save-targets'}, handleList)`: list → `images = listInventoryImagesWithFallthrough`, `current = item.imageFileId when among the listed ids else null`, DEBUG `${LOG_TAG} Listed object images` `{itemId, count, current, context}`, `{current, images}`; `save-targets` → `{albums: listAllPhotoAlbumOptions(repos)}`. `POST` `withActionDispatch({generate, upload, 'set-current', 'delete-image', 'save-to-store'})` (NO default → an absent/unknown action is the middleware's 400). `generate` (`:83-124`): `req.text()`; blank → `{}`; non-JSON → DEBUG `${LOG_TAG} Generate body is not valid JSON` `{itemId}` + 400 `Request body is not valid JSON`; `generateBodySchema.safeParse` failure → 400 (issues `; `); `trackActivity('image', () => generateInventoryItemImage(…))`; `files.findById(result.fileId)`; 201 `{image: file ? toWardrobeImageSummary(file) : {fileId}, current: fileId, prompt, profile, rerouted, trail}`; `NoInventoryImageProfileError` → 400 (its message); `InventoryImageGenerationError` → `errorResponse(refused ? 'The image provider declined to draw this object' : `Image generation failed: ${message}`, refused ? 422 : 502, {trail, refused})`; else `inventoryErrorResponse`. `upload` (`:126-159`): `formData()` failure → 400 `Expected a multipart upload`; no `File` → 400 `No file provided`; `validateImageFile` throw → 400 (its message or `Invalid image`); `kind` invalid → 400 `kind must be "uploaded" or "imported"`; `convertToWebP(buffer, type, name)`; `addInventoryItemImage(…, {kind, content, contentType, width ?? null, height ?? null})`; INFO `${LOG_TAG} Uploaded object image` `{itemId, fileId}`; 201 `{image: summary, current: id}`. `set-current` / `delete-image`: `fileIdBodySchema.parse` INSIDE the try (a ZodError reaches `inventoryErrorResponse` and is RETHROWN → the middleware's 400); `{current}`. `save-to-store` (`:189-216`): `SaveImageRequestSchema.safeParse(await req.json().catch(() => ({})))` → 400 (issues); the picture must be among `listInventoryImagesWithFallthrough` → 400 `That picture does not belong to this object`; an archived character's vault as the target → 409 `That store belongs to an archived character and cannot be written to`; `saveImageToAlbum({mountPointId, fileId, caption: caption ?? (title || name), tags: tags ?? [], attribution: {name: user.name ?? 'Quilltap', id: user.id ?? null, role: 'user'}})`; INFO `${LOG_TAG} Saved object picture to a store` `{itemId, fileId, mountPointId}`; `savedImageResponse` / `SaveImageToAlbumError` → `saveImageErrorResponse`. (Compare the wardrobe's `wardrobe_item_images.rs`: NO `link-image` action for the Inventory.)

**`app/api/v1/chats/[id]/actions/inventory.ts` (100) + the dispatch hunks.** `OPERATOR(userId) = {userId, provenance: 'operator', announce: true}`; `castOf(chat)` = unique `characterId`s of participants with `type === 'CHARACTER' && characterId && status !== 'removed'`. `GET ?action=inventory` (`handlers/get.ts:14, 45, 77`; `route.ts:15`): chat null → 404 `Chat not found`; `loadCastInventoryPools`; per cast member `{equipped: readEquippedSet, kits: {[kitId]: kitState(kit, equipped, id => pool.resolveId(id))} for every reachable kit}`; DEBUG `[Chats v1] Read the cast inventory` `{chatId, characters, carried: total equipped}`; `{inventory, projectId: chat.projectId ?? null}`. `POST ?action=inventory-equip|inventory-unequip` (`post.ts:40-41, 113-114`): `chatInventoryEquipSchema.parse` BEFORE the chat read; 404; `equipItems` / `unequipItems` with `OPERATOR`; INFO `[Chats v1] Inventory ${mode}` `{chatId, characterId, ids}`; `successResponse(result)` (the `EquipResult` / `UnequipResult` object AS the data). `POST ?action=inventory-reset-defaults` (`post.ts:115`): `chatInventoryResetSchema.parse`; 404; not in `castOf` → `inventoryErrorResponse(new InventoryNotAParticipantError(id))` (400); `resetInventoryToDefaults`; INFO `[Chats v1] Inventory reset to defaults` `{chatId, characterId, count}`; `{equipped}`. `actions/index.ts:33` exports the three. Registration order in `post.ts`'s map: `… 'equip', 'wardrobe-rollback', 'inventory-equip', 'inventory-unequip', 'inventory-reset-defaults', 'toggle-avatar-generation' …`; in `get.ts`: `… 'wardrobe-history', 'inventory', 'photo-albums' …`.

### 1.9 The carriers

- **`lib/backup/restore/uuid-remap.ts`** (+88): `:62` imports `isInventoryItemDocumentPath`; `:187-210` `inventoryDocuments(data)` — every `docMountFileLinks` link whose path passes the predicate, content from `docMountDocuments` by `fileId` (a string), `JSON.parse` to a non-array object with a non-empty string `id` (NOT UUID-checked here); `:220-222` `inventoryItemIds(data)`; `:225-235` `InventoryImagePointerFix {mountPointId (remapped); sourceMountPointId; itemId; imageFileId (remapped)}`; `:243-258` `planInventoryImagePointerFixes(original, remapper)` — a pointer must be a string naming a file in `original.files`; `:303-306` **`remapLinkId = id => inventoryIds.has(id) ? id : wardrobeItemIdRemap.get(id) ?? remapper.remap(id)`** — Inventory object ids travel VERBATIM in `files.linkedTo` (and tags, through the same remap of array fields); `:441-449` the bug-252 comment grows "and the carried Inventory".
- **`lib/chat/character-keyed-fields.ts`** (+4): `:18-19` doc; `:40` `'equippedInventory'` appended to `CHARACTER_KEYED_CHAT_FIELDS` (now FIVE: `equippedOutfit, characterAvatars, courierCheckpoints, pendingOutfitNotifications, equippedInventory`); `:122` `rekey('equippedInventory', ids => Array.isArray(ids) ? ids.filter(typeof string) : [])` — the KEY follows the character remap (an unknown character's entry DROPPED, as every keyed field), the VALUES are kept verbatim (non-array → `[]`). Consumers: the new-account restore (`remapBackupData`, `uuid-remap.ts:441-449`) and the `.qtap` import reconcile (`lib/import/quilltap-import/reconcile.ts:423` — the `patch` arms there write `equippedOutfit` / `characterAvatars` / `courierCheckpoints` / the rest → `pendingOutfitNotifications`; at the pin the import's `_` arm would route `equippedInventory` to `pendingOutfitNotifications` UNLESS it has its own arm — **measure at port time**: `reconcile.ts:423-440`).
- **`lib/backup/restore/restore.ts`** (+41): `:46-54` imports; `:113-121` `inventoryImagePointerFixes = planInventoryImagePointerFixes(data, remapper)` planned beside the wardrobe's (new-account only, against the ORIGINAL data, the same remapper); `:979-1008` **step `22f-quater`** after `22f-ter` (the wardrobe's pointer repair) and before `22g`: group fixes by `mountPointId`, skipping those whose `mountPointId` OR `sourceMountPointId` is a tombstoned vault; per mount `updateInventoryManyInMount({mountPointId, scope:'project', characterId:null}, fixes → {id: itemId, patch: {imageFileId}})` (no `requireAll` — a missing object is skipped); a throw → warning `Failed to repoint Inventory pictures in a restored store (${mountPointId}): ${msg}` + WARN `Failed to repoint Inventory pictures after restore` `{mountPointId, count, error}`; when any were planned INFO `Repointed Inventory pictures after new-account restore` `{planned, fixed}`. (`RESTORE_TS_MESSAGES` census both sides — memory note `restore-census-whitelist`.)
- **`lib/characters/archive-service.ts`** — PORTED (P4.D322); nothing remains.
- **`.qtap` export / import** — no explicit hunk names `equippedInventory` (`git grep` over `lib/export lib/import` at the pin finds only the keyed-fields remap in `reconcile.ts`); the chat row is spread whole on export (so the column travels) and `chats.create` on import writes whatever the row carries — v5's `qtap_export/schema-key-order.json` MUST gain `equippedInventory` after `equippedOutfit` (`:116`) and `canManageInventory` after `canCreateOutfits` (`:36`) or the exported key order diverges.
- **`lib/realtime/topic-map.ts`** (+11) and **`lib/query/keys.ts`** (+33) — client query keys (`queryKeys.inventory.lists / folderLists / equipped(id)`), the `chats` topic adding `inventory.equipped(id)`, the NEW `'inventory'` topic case — the SPA survey's (`apps/web/src/app/core/realtime-topic-map.ts`); the server-side `REALTIME_TOPICS` + `'inventory'` is `e1872c94f`'s (`realtime/types.rs:35` — SEVEN topics today; the eighth is the tools lane's).
- **`lib/workspace/{tab-meta,types}.ts`** — `inventory: { title: 'The Inventory', icon: 'inventory' }`, `TabKind 'inventory'` (payload `{characterId?; projectId?}`) — names only; the SPA survey owns.

### 1.10 Tests v4 shipped (the oracle's seed corpus), by file with case counts

| v4 test (at the pin) | cases | what it pins |
|---|---|---|
| `__tests__/unit/lib/inventory/inventory-files.test.ts` | 12 | path predicate; `parseInventoryItemFile` whole/sparse/skips/unknown keys/timestamp fallback; `isInventoryItemDocument`; `serializeInventoryRecord` round trip + sparse; shape issues; create/update bodies |
| `…/inventory/resolve.test.ts` | 7 | `resolveItem` standalone/instance/picture fall-through/explicit null/missing parent; `inventoryDisplayName`; `kitState` |
| `…/inventory/svg.test.ts` | 8 | plain drawing; empty; drops script/foreignObject/style; handlers/hrefs/javascript; `url(`; comments/PI/DOCTYPE; not-an-svg + size cap; closes opens |
| `…/inventory/item-mutations.test.ts` | 8 | duplicate-id files left alone; archetype edit reaches instances + detach; delete in use + force bakes; archived holder refuses; kit nesting + member scrub; canCopy; `(copy)` title; a note survives |
| `…/inventory/equip.test.ts` | 13 | the per-chat cap; archetype refused; kit expansion (one minted, three carried, one line); atomic cap breach; arrival (defaults, continuation copy, returning seat, reset under lock, lenient cap); Pascal's line (articles, plural, "a" before consonant); pending changes (provenances, netted pick-up) |
| `…/inventory/routes.test.ts` | 12 | unknown action 400; list/instantiate/dependents/ARCHETYPE_IN_USE/force; INVALID_PARENT/KIT_NESTING/INVALID_SHAPE; clear-override + detach; transfers move/copy; kits whole (KIT_STRANDED ×2, INVALID_SHAPE, character kit with project member ×2); chat actions (equip/read/CAP_EXCEEDED/reset; NOT_A_PARTICIPANT) |
| `…/inventory/delivery.test.ts` | 2 | the trailing section's lines/sort/kit/undescribed; sword→model, image-only→pictures, set down removes both |
| `…/inventory/image-description.test.ts` (`d88c1ce39`) | 3 | `renderCarryingPhrase` trims; only drawn; never a kit |
| `…/inventory/title-lint.test.ts` | 23 | normalisation ×2; the crafted collision ×5; passes ×4; leaving ×2; fail closed ×4; guard ×1; archive/delete/restore/unverifiable ×4; census line ×1 |
| `…/inventory/title-lint-documents.test.ts` | 8 | path claims; shadow refused; gate dark on rewrite; delete/move out; description-only; unreadable prior; tool file census; mutations pass through |
| `…/mount-index/database-store-title-lint.test.ts` | 4 | write lint before/rows after; refusal leaves store untouched; delete/move; never loads for other paths |
| `…/pascal/tool-gate-keys.test.ts` | 5 | both clauses/subjects; file gone on open; failed listing/unparseable; no enabled store; any-of groups + aggregate keys not counted |
| `…/migrations/add-chat-equipped-inventory.test.ts` | 3 | registered/labelled/4.11.0/follows the outfit column; adds nullable then nothing; idempotent |
| `…/lib/backup/inventory-backup.test.ts` | 6 | archetype/instance/kit ids kept; chat carried set re-keyed; picture link kept; pointer planned against the remapped vault; nothing for an unknown file; empty chat as was |
| `…/app/api/v1/inventory/images-route.test.ts` | 2 | malformed JSON 400; empty body = default profile |
| `__tests__/unit/context-management.test.ts` | +2 (`:1453`, `:1572`) | byte-identical when nothing described; carried on a continue-mode turn |
| `__tests__/unit/chat-initial-greeting.test.ts` | +(within 16) | the user-turn `---` join of `trailingContextSections` |
| `…/lib/image-gen/appearance-carrying.test.ts` | 3 | image tool `Carried` entry; avatar carries / nothing when none |
| `…/image-gen/appearance-resolution.test.ts` | +1 (`:659`) | classifies + sanitizes the carrying with the rest |
| `…/lib/chat/character-keyed-fields.test.ts` | +1 (`:33`) | re-keys the carried Inventory, ids kept |
| `…/lib/database/repositories/base-repository-patch-only.test.ts` | (`:104`) | `patchOnlyFields` declares the four |
| `…/lib/characters/archive-service.test.ts` | (`:631`) | keeps `Inventory` (PORTED) |
| `…/realtime/topic-map.test.ts` | +1 (`:46`) | the `chats` topic refreshes the cast inventory (SPA) |
| `…/doc-mount-documents-find-in-folder-containing.integration.test.ts` | 0 (file empty at the pin) | — |
| `__tests__/helpers/inventory-fake-store.ts` (343) | — | an IN-MEMORY fake of the repositories / database-store / folder-paths / instance-settings / tiered-mount-pool / tool-gate-keys modules; `GENERAL_MP = 'mp-general'`, `PROJECT_MP = 'mp-project'`, project `{id:'project-1', name:'The Quest'}`; `offRoster`, `gateKeys`, `gateKeysUnread`, `missingMounts` knobs |

**Every unit test above runs on the FAKE store**, so none seeds the differential
corpus directly: the oracle cases must plant the same shapes through v4's REAL
`writeDatabaseDocument` / repositories on a fixture pair (the P4.D322
`extend-character-archive-inventory.ts` idiom), as the wardrobe keystone did
with `build-wardrobe-keystone-fixture.ts`.

---

## 2. The v5 intersection map (v5 `main` @ `5a94a9d1e`)

Verdict key: **HAS** (identical), **PRE-DRIFT** (what differs), **LACKS**.

### 2.1 Schema, repositories, reads

| v4 surface | v5 home (file:line) | verdict / what the lane edits |
|---|---|---|
| `chats.equippedInventory` column | `services/provisioning/fresh_schema.json:11` (the `chats` DDL — `"equippedOutfit" TEXT,\n  "characterAvatars" TEXT`, no inventory); `db/chats_read.rs:98` (explicit SELECT list; marshal `:368` `equippedOutfit` = `row.get(64)`, later additions appended at 99-102 `:418-428`); `db/chats.rs:485` `ChatCreate.equipped_outfit: Option<Value>` (`#[serde(rename_all="camelCase")]`, `#[serde(default)]`), `:689` `ChatUpdate.equipped_outfit: Option<Option<Value>>`, `:988` the INSERT column list, `:1271` the `set_col!` | **LACKS.** Edit: re-dump `fresh_schema.json` (§3); `chats_read.rs` APPEND `equippedInventory` to the SELECT list and marshal at the NEXT index (103) with `put_opt_json`; `chats.rs` `ChatCreate.equipped_inventory: Option<Value>` + INSERT + `ChatUpdate.equipped_inventory: Option<Option<Value>>` + `set_col!`; a NEW `db/chats_inventory.rs` (the `chats_outfits.rs:152` twin) with `set_equipped_inventory(chat_id, character_id, ids: Option<&[String]>) -> Result<bool>` (RMW through `chats_read::find_by_id`, dedup + sort, `ChatUpdate{equipped_inventory: Some(Some(state))}`, the DEBUG `Equipped inventory write`), `normalize_equipped_inventory(raw) -> Map<String, Vec<String>>`, `find_chat_ids_carrying_inventory_id` (`LIKE '%id%'` + the JS filter), `remove_equipped_inventory_id` |
| `patchOnlyFields` (`chats.repository.ts:566`) | v5 has no `_update` whole-row writer (`ChatUpdate` is a patch struct), so "patch-only" is structural; the restore's `chats.create` (`restore/orchestrator.rs:786`, `ChatCreate` deserialised from the bag) and the import's `patch` arms (`quilltap_import/reconcile.rs:486-489`) are the two writers that carry the column whole | **PRE-DRIFT** (no concept needed) — the lane adds the `"equippedInventory" => patch.equipped_inventory = Some(Some(value))` arm at `reconcile.rs:486` (today the `_` arm would route it to `pending_outfit_notifications` — the same v4 hazard, §8 Q5) |
| `characters.canManageInventory` column | `fresh_schema.json:6` (`"canCreateOutfits" INTEGER,\n  "canChooseOutfit" INTEGER DEFAULT 0`); `db/characters_read.rs:66-71` `SLIM_COLUMNS` (`canCreateOutfits` marshal `:196` `row.get(19)`); `db/characters.rs:296` INSERT list + `:408` `set_bool!(patch.can_create_outfits …)`; `db/vault_character_update.rs:437` the vault-vs-column routing list; `api/characters.rs:2225` the PUT key allow-list; `services/quilltap_import/characters.rs:110-111`; `services/backup/restore/orchestrator.rs:3361` `can_create_outfits: ob(ch, "canCreateOutfits")`; `services/cascade_delete.rs:522`, `services/speaker_names.rs:143-144`, `services/mount_index/document_text_search.rs:509`, `enclave/step.rs:1618` (test DDL twins) | **LACKS.** Edit each listed site: APPEND the column to `SLIM_COLUMNS` (marshal at the end), `CharacterCreate` / `CharacterPatch` `can_manage_inventory: Option<bool>` + INSERT + `set_bool!`, the vault-update column list (`:437`), the PUT allow-list (`:2225`), the import struct, the restore bag read; `schema-key-order.json:36` gains `canManageInventory` after `canCreateOutfits` and `:116` gains `equippedInventory` after `equippedOutfit`; the `generators/qtap-export.schema.json` copy re-vendored. The TOOL GATE on the flag (`inventoryToolFlags`, `buildTools`'s 8th positional) is the tools lane's |
| `SELECT *` readers of `chats` / `characters` | `db/chats.rs:1709` (moderation ledger by `column_names()` — tolerant); `db/store_name_reconcile.rs:687-712` `read_vault_owner_rows` — `SELECT * FROM "characters"`, hydrated BY NAME with `boolean = CHARACTER_BOOLEAN_COLUMNS.contains(name) || name.starts_with("is")` | **PRE-DRIFT:** the extra `canManageInventory` cell is tolerated (hydrated as a NUMBER `0/1/NULL` unless added to `CHARACTER_BOOLEAN_COLUMNS` — v4 reads a Zod boolean; the ledger's "measure first" item: ADD the name there and pin it in `store_name_reconcile`'s test) |
| `LLMLogTypeEnum` + `IMAGE_SPEND_LOG_TYPES` | `services/llm_logging.rs:370-371` `IMAGE_SPEND_LOG_TYPES = &[IMAGE_GENERATION, WARDROBE_ITEM_IMAGE]`; `:397` `log_type::WARDROBE_ITEM_IMAGE`; `almanack/phase6_wire_records.rs:159` (the bug-196 roll-up); `image_gen/image_attempt.rs:22, 89` | **LACKS** `INVENTORY_ITEM_IMAGE` — add the const + the spend list entry (the Almanack's per-profile roll-up moves for a fixture carrying such a row) |
| `docMountDocuments.findManyInFolderContaining` | `db/doc_mount_documents.rs:458-520` has `find_many_by_mount_points_in_folder(_opts)` family; NO content-contains query | **LACKS** — a NEW `find_many_in_folder_containing(folder, ext, needles) -> Vec<DocMountDocumentWithLink>` with v4's exact SQL (`instr(d.content, ?) > 0` ORed; `LOWER(l.relativePath) LIKE ?`) and the JS post-filter; the raw-SQL class (§8 Q4) |
| `projectArrayIntoVaultFolder` `ext` option | `db/vault_wardrobe_write.rs:56-61` `ProjectArrayOptions {preserve_file_names, recursive, owns_file}`; `:124` the `.md` literal | **PRE-DRIFT** — add `ext: &'a str` (default `".md"` in `FLAT`) and use it in the listing; every existing caller unchanged |
| The item-folder lift (`item-folders.ts`, `item-folder-ops.ts`) | `wardrobe_folders.rs:23-346` (the rules inline: `WARDROBE_FOLDER_MAX_DEPTH`, `WARDROBE_RESERVED_FOLDER_SEGMENTS`, `WardrobeFolderPathError(String)`, `not_found_message` / `conflict_default_message` / `not_in_location_message`, the 15 helpers); `db/vault_wardrobe_public.rs:234-300` (`resolve_wardrobe_mount` — the tombstone `DbError::CharacterArchived`; `WardrobeMount`), `:567-700` (`folder_under_wardrobe`, `find_stored_folder`, `list_folders_in_mount`, `list_folder_paths_in_mount`), `:708-1000` (`create_folder_in_mount`, `rename_folder_in_mount`, `delete_folder_in_mount`) | **PRE-DRIFT (the pre-lift copies).** The lane introduces `item_folders.rs` (an `ItemFolderRules {root, reserved, max_depth, noun}` + the generic fns + `capitalise`) and `db/item_folder_ops.rs` (an `ItemFolderTree<T>`-style trait with `ext`, `context`, `is_item_document`, `has_stable_id`, `read`, `reproject`, `refile`, and the three error constructors), then re-points the wardrobe's fns at them as one-line delegates — exactly v4's refactor, string-preserving; `wardrobe_folders_equivalence` + `wardrobe_folder_writes_tier2_equivalence` + `vault_wardrobe_public_equivalence` are the regression net (they must stay green BYTE-for-byte at the pin). v5 has NO `runSerialized` chain map — v4's per-folder serialisation is the writer task's single-writer ordering in v5 (§8 Q3) |
| `CharacterArchivedError` | `db/mod.rs:231` `DbError::CharacterArchived {character_id}`; `db/wardrobe_location.rs:66` `WardrobeLocationError::CharacterArchived`; the web edge's 409 `An archived character's wardrobe cannot be changed` (`api/wardrobe_route_factory.rs:265`) | **HAS** the error; the Inventory needs its own 409 sentence (`An archived character's inventory cannot be changed`) + the INFO `[Inventory routes] Refused a write to an archived character` |
| `sanitizeFileName`, `archivedPatch`, `nextCopyTitle`, `patchSchemaOf` | `vault_overlay.rs:667` `sanitize_file_name`; `wardrobe.rs:1454` `archived_patch`; `:1536` `next_copy_title`; `patchSchemaOf` has NO generic twin (v5 ports each patch schema by hand — `api/text_replacements.rs:352-363` is the idiom) | **HAS** the three helpers (reuse); the Inventory's `updateInventorySchema` / `InventoryItemFileSchema` / `instantiate.overrides` are hand-written three-state parsers (`Option<Option<T>>` where v4's `.nullable().optional()`) |
| `ensureOwnerOfficialStore`, `ensureFolderPath`, `getGeneralMountPointId`, `resolveGroupMountsForCharacter`, `resolveProjectMountPointIds`, `projectRosterAdmits`, `publishRealtime` | `db/ensure_official_store.rs:200`; `db/doc_mount_file_links.rs:1563` (a repository method); `db/instance_settings.rs:135`; `db/tiered_mount_pool.rs:316`; `tools/wardrobe_shared.rs:43, 65`; `project_roster_access.rs:61`; `realtime/bus.rs:188 publish_realtime(topic: RealtimeTopic, id: Option<&str>)` | **HAS** — reuse by name; `project_roster_access.rs:1-12`'s header gains the inventory sentence (comment-only, `roster-access.ts:4-14`) |

### 2.2 The services and the chat spine

| v4 surface | v5 home | verdict |
|---|---|---|
| `lib/wardrobe/location.ts` → `inventory/location.ts` | `db/wardrobe_location.rs:157-645` (`WardrobeLocation {scope, id, mount_point_id, character_id, origin, mount}`, `location_key`, `location_for_mount`, `resolve_wardrobe_location(main, mount, scope, id, user_id, opts)`, `group_locations_for_character`; the write-time re-resolve `writable_mount`) | **LACKS** — new `db/inventory_location.rs` mirroring it with `InventoryRecord` ops: `read_items(include_archived)`, `find_item`, `create`, `update(id, patch, remove)`, `update_many(patches, require_all)`, `delete(ids, follow_on)`, the five folder ops, `writable_mount_point_id`; `GENERAL_INVENTORY_ORIGIN`; the DEBUG `[InventoryLocation] Resolved inventory location` |
| `inventory/pool.ts` (four tiers, shadowing, external parents) | NO wardrobe twin of `loadInventoryPool` (the wardrobe pool is per-tool: `tools/wardrobe_tool_table.rs:20 load_tool_pool`; `db/archetype_wardrobe.rs:145-216` reads shared tiers with `WardrobeOrigin`/`WardrobeOriginScope`; `db/wardrobe_read.rs:109 read_shared_tiers`) | **LACKS** — new `inventory/pool.rs` (`InventoryPool`, `build_inventory_pool`, `load_inventory_pool`, `load_pool_for_location`, `load_cast_inventory_pools`, `kin_of`); reuse `WardrobeOriginScope`-like enum as `InventoryOriginScope` (or share one) |
| `inventory/resolve.ts`, `equipped-set.ts`, `dependents.ts`, `errors.ts`, `svg.ts`, `folders.ts` | none | **LACKS** — new `inventory/{resolve,equipped_set,dependents,errors,svg,folders}.rs` (pure where v4 is pure; `svg.rs` needs `regex` twins of the five patterns — v5's `jsregex` family) |
| `inventory/item-mutations.ts`, `item-images.ts`, `item-image-generation.ts`, `transfers.ts` | the wardrobe twins: `services/wardrobe_item_mutations.rs` (`create_item`, `update_item`, `set_item_archived`, `move_items_to_folder`, `PreservedIdentity`, `CreateItemOptions`), `services/wardrobe_item_images/{service,carry,primitives}.rs` (`add_wardrobe_item_image`, `set_current…`, `delete…`, `link…`, `carry_item_images(CarryArgs)`, `commit_moved_images`, `drop_source_image_links`, `assert_item_image_choice`, `cleanup_item_images(ItemImageCleanupMeta)`), `services/wardrobe_item_image_generation.rs:264`, `services/wardrobe_transfers.rs:448` | **LACKS** the Inventory modules; **PRE-DRIFT** on the images primitives: `wardrobe_image_paths.rs:16-26` and `wardrobe_image_bridge.rs:175 write_wardrobe_item_image` / `:273 delete_wardrobe_item_image_link` hard-code `Wardrobe/images` — they grow an `images_folder: &str` parameter (default the wardrobe's) exactly as v4's `imagesFolder`; `carry.rs:114 carry_item_images`, `:230 commit_moved_images`, `:276 drop_source_image_links` and `primitives.rs:154 cleanup_item_images` thread it |
| `inventory/equip.ts`, `equip-effects.ts`, `apply-defaults.ts`, `roster-gate.ts` | none; the analogs: `services/wardrobe_wear_commit.rs` (the ONE equip chokepoint), `services/outfit_change_effects.rs:114 refresh_avatar_for_outfit(db, user_id, chat_id, character_id, source_context: &'static str)`, `services/outfit_selections.rs` (`apply_outfit_selections`, `EquipSource::ChatStart`), `services/pascal_writer.rs:187 post_pascal_result` | **LACKS** — new `inventory/{equip,equip_effects,apply_defaults,roster_gate}.rs`; `refresh_avatar_for_outfit` gets the v4 alias twin `refresh_avatar_for_appearance` (rename + keep the old name); `pascal_writer.rs` gains `post_pascal_inventory_change(db, chat_id, content)` (`PASCAL_INVENTORY_CHANGE_KIND`) |
| `prompt-section.ts` → `build_context` | `services/build_context.rs:2924` `progressions_llm_context`, `:3062-3072` the ONE trailing list `[core_whisper, llm_recall_text, suparna_mail, progressions, inform_block, turn_skip_instruction]`, `:3104-3109` the `\n\n---\n\n` join; `BuildContextInput.is_continue_mode` `:565` | **LACKS** — new `inventory/prompt_section.rs` (`INVENTORY_SECTION_HEADER`, `render_inventory_section`, `build_inventory_section(main, mount, chat, character_id)`); `build_context.rs` inserts `inventory_llm_context` between progressions and inform (and the interrupts lane's `turn_yield_instruction` goes LAST — a Shared contract on the same list) |
| Carina | `services/carina_query.rs:468-494` (`progressions_section` then `question_with_progressions = format!("{}\n\n---\n\n{progressions_section}", question)` when non-empty) | **PRE-DRIFT** — becomes the three-part filter/join |
| the greeting | `services/initial_greeting.rs:175-180` the literal user message; `GreetingRequest` (no trailing field); `services/chat_create.rs:2781 base_request`, `:3158` the second `GreetingRequest {` | **LACKS** `trailing_context_sections: Vec<String>` + the `---` join + the DEBUG `[Greeting Generation] Appending trailing context sections to the greeting turn`; `chat_create.rs`'s greeting phase builds the section from the chat row |
| arrival defaults' three call sites | `services/chat_create.rs:1631-1648` (`apply_outfit_selections` … `compile_all_identity_stacks`); `api/chat_cast.rs:349-360` (`apply_outfit_for_added_participant` then `refresh_avatar_for_arriving_character`) and `:473-485` (the reactivation, gated on `outfit_selection.is_some()`); `services/chat_merge.rs:496-499` (`apply_outfit_selection_sync` per character) | **LACKS** — `apply_inventory_defaults(db, chat_id, &ids, {source, user_id, source_chat_id})` inserted at each (chat_create: after the outfit block, before step 10; chat_cast: as the LAST statement of `apply_outfit_for_added_participant` `services/chat_participants.rs:1321` so both callers reach it; chat_merge: after the outfit line) |
| `appearance-resolution.ts` | `services/appearance_resolution.rs:86-99` `ResolvedCharacterAppearance` (no carrying), `:290` `resolve_character_appearances`, `:484-486` the classify join `format!("{} {}", phys, cloth)`, `:554-556` / `:601-604` the sanitizer `"{}. {}"` | **PRE-DRIFT** — add `carrying_description: Option<String>`, the `with_carrying` tail (needs `chat_id` + a chat read), the `[phys, cloth, carry].filter(non-empty).join(" ")` classify text (BYTE CHANGE with an empty clothing — `appearance_sanitize_gate_tier3_equivalence` re-pins), `sanitizable_appearance_text`, `scene_description_parts` |
| `story-background.ts`, `prompt-expansion.ts`, `avatar-prompt.ts`, `character-avatar.ts` | `services/story_background_job.rs:615-618` (inline `Wearing:`); `tools/generate_image.rs:1069-1080` (the `ExpansionClothing` entry); `services/avatar_prompt.rs:29-41 AvatarPromptOptions {equipped_slots, project_mount_point_ids, character_aesthetic}`, `:185-198` the block join; `services/character_avatar_job.rs:298-306` | **PRE-DRIFT** — each gains the carrying arm (`scene_description_parts`; a second `ExpansionClothing {name: "Carried", …}`; `AvatarPromptOptions.carrying: Option<String>` + `carrying_block`; the job loads `load_carrying_phrase` before the prompt) |
| `pascal/writer.ts` | `services/pascal_writer.rs:187` | **LACKS** the inventory writer (above) |
| `roster-access.ts` header | `project_roster_access.rs:1-12` | **PRE-DRIFT** (comment) |
| `title-lint*.ts`, `lint-scope.ts`, `tool-gate-keys.ts`, `database-store.ts` hooks | nothing (`title_lint` absent); `db/database_store.rs:191 write_database_document`, `:280 delete_database_document`, `:297 move_database_document`, `:504 delete_database_folder`, `:537 move_database_folder`; `pascal/roster.rs:416 load_tools_from_mount`, `:563 list_all_custom_tools(mount) -> CustomToolLibrary` | **LACKS** — new `inventory/{title_lint,title_lint_documents,lint_scope}.rs` + `pascal/tool_gate_keys.rs`; the four store hooks; `CustomToolLibrary` must grow `unread` / `errors[].stage` / `stores_consulted` (tools lane) |

### 2.3 The transports

| v4 | v5 | verdict |
|---|---|---|
| the route factory (`createInventoryCollectionHandlers` / `createInventoryItemHandlers`) | `api/wardrobe_route_factory.rs` (`WardrobeRouteConfig {scope, log_tag}` + `CHARACTER_/GENERAL_/PROJECT_/GROUP_WARDROBE` consts `:95-115`, `write_error_response :255`, `list_character_items :692`, `list_store_items :724`, `folders_list :899`, `folder_create/rename/delete :993-1056`, `items_move/copy :1093-1115`, `create :1229`, `item_get :1323`, `item_update :1392`, `item_delete :1496`, `config_for :1579`, `FolderAction :1602`, `folder_action :1614`); dispatched from `api/characters.rs:841, 2681-2756`, `api/projects.rs` (`project_wardrobe_list`, engine `:3722`), `api/groups.rs`; `Request::{Character,Group,Project}Wardrobe{List,InstructionsGet,InstructionsSet,Create,Get,Update,Delete}` (`api/types.rs:675-722, 961-999, 1564-1602`) and the General family `Request::Wardrobe{List,Create,ItemGet,Update,Delete,TransferDestinations,TransferApply,PreviewAvatar,AnalyzeImage,ItemWearHistory,ItemImagesList,ItemImageGenerate…}` (`:2708-2800`); the web edges `quilltap-web/src/wardrobe_routes.rs` (`/api/v1/wardrobe`, `/transfers`, `/{itemId}`), `wardrobe_images_routes.rs` (`/api/v1/wardrobe/{itemId}/images`), `characters_routes.rs:843-902` (`/api/v1/characters/{id}/wardrobe` GET/POST), the tier item routes through `POST /api/dispatch` (`lib.rs:327`) | **LACKS** — new `api/inventory_route_factory.rs` (`InventoryRouteConfig {scope, log_tag, log_id_key}`), new `Request` variants per tier (`CharacterInventoryList {character_id, include_archived, shared, project_id, chat_id}`, `…Folders`, `…Archetypes {project_id, chat_id}`, `…Create {item}`, `…Instantiate {body}`, `…FolderCreate/Rename/Delete`, `…ItemsMove/Copy`, `…Duplicate {item_id}`, `…ItemGet`, `…ItemDependents`, `…ItemUpdate {item, project_id, chat_id}`, `…ClearOverride {fields}`, `…Detach`, `…ItemDelete {force}` for Character/Group/Project + the General `Inventory*` family), `InventoryTransferApply {body}`, `InventoryItemImages{List,Generate,Upload,SetCurrent,Delete,SaveTargets,SaveToStore} {scope, container_id, item_id, …}`, `ChatInventoryGet {chat_id}`, `ChatInventoryEquip/Unequip {chat_id, body}`, `ChatInventoryResetDefaults {chat_id, body}`; web edges `inventory_routes.rs` (`/api/v1/inventory`, `/transfers`, `/{itemId}`), `inventory_images_routes.rs`, `/api/v1/characters/{id}/inventory(/{itemId})` + projects/groups (REST or dispatch — mirror the wardrobe's split: the orchestrator decides whether the tier item routes get REST edges this time; the SPA survey's client calls `inventoryCollectionUrl` REST paths `container.ts:28-64`, so REST edges for ALL FOUR tiers are needed unless the SPA re-routes through dispatch); `wardrobe_routes.rs:629-700` `CHAT_GET_ACTIONS` gains `"inventory"` after `"wardrobe-history"`, `CHAT_POST_ACTIONS` gains the three after `"wardrobe-rollback"`; every new routes file gets its `COLLAPSE_CENSUS` row (`quilltap-harness/tests/web_edge_body_parse_guard.rs:107`) and `dispatch_wrong_type_census.rs` rows per variant |
| `inventoryErrorResponse` | `api/wardrobe_route_factory.rs:255 write_error_response(WardrobeLocationError)` | **LACKS** — `inventory_error_response(InventoryError) -> Response` with the `{code, ...details}` bag (`Response::error(kind, message)` + details — the web edge maps `Conflict` → 409, `BadRequest` → 400; the `code` rides `details`) |

### 2.4 Already ported (P4.D322) — what remains

- `lib/characters/archive-service.ts`: `KEPT_FOLDERS` / the prefix rule / `CHARACTER_INVENTORY_FOLDER` — **v5 HAS** (`services/character_archive/service.rs:282-295`); the extend case `harness/oracle/fixtures/extend-character-archive-inventory.ts` plants a populated object, a picture link, an empty subfolder and the bare folder. **Nothing remains** of that file; the keystone's `inventory/folders.rs` should EXPORT `INVENTORY_FOLDER = "Inventory"` and `service.rs:286` re-point its private const at it (one source of truth).
- `lib/chat/context-manager.ts` bug 254 — **v5 HAS** the unconditional progressions (`build_context.rs:2924`). Remains: the inventory section + the doc-comment word.

### 2.5 Modules the lane must NOT touch (siblings')

`tools/definitions/*`, `tools/executor.rs`, `services/tool_build.rs` (the 8th positional), `services/tools_inventory.rs`, `services/orchestrator.rs`'s tool loop and the inventory FLUSH placement, `pascal/*` (except the NEW `tool_gate_keys.rs` which the lint lane adds), `realtime/types.rs`'s eighth topic, `almanack/phase3_ledgers.rs`'s `May manage their inventory` line, `doc_edit/uri_producers.rs` / `services/librarian_notifications.rs` / `tools/self_inventory.rs` (bug 256 — smalls), `services/appearance_resolution.rs`'s `406dbdd5a` moderated-painter hunk (smalls), the interrupts' `turn_yield_instruction` slot, `apps/web/**` (SPA lane), `help/` + `docs/v4/` (vendor lane).

---

## 3. Schema / migrations / boot ensures

### 3.1 The two shapes, measured

| | generateDDL-rendered (a FRESH instance; what the re-dump carries) | the migration-appended shape (an EXISTING instance) |
|---|---|---|
| `chats.equippedInventory` | `"equippedInventory" TEXT` at cid 66, between `"equippedOutfit" TEXT` and `"characterAvatars" TEXT` (the `ChatMetadataSchema` key order, `chat.types.ts:1047`; `JsonSchema.nullable().optional()` → `TEXT`, NO DEFAULT) | `addColumnIfMissing('chats', 'equippedInventory', 'TEXT DEFAULT NULL')` — APPENDED after the last column (`turnSkippingEnabled` / `showThinking` …), `sqlite_master` text ends `, "equippedInventory" TEXT DEFAULT NULL)` |
| `characters.canManageInventory` | `"canManageInventory" INTEGER DEFAULT NULL`? — NO: generateDDL renders a `z.boolean().nullable().optional()` as `INTEGER` with NO default (compare `"canCreateOutfits" INTEGER` at `fresh_schema.json:6`), between `canCreateOutfits` and `canChooseOutfit` | `addColumnIfMissing('characters', 'canManageInventory', 'INTEGER DEFAULT NULL')` — appended, text ends `, "canManageInventory" INTEGER DEFAULT NULL)` |

So BOTH columns have the `TEXT` / `INTEGER` vs `… DEFAULT NULL` two-shape
split (the P4.D249 `chat_informs.permanent` precedent, NOT the P4.D304
byte-equal one — the ledger row `:402` guessed `INTEGER DEFAULT NULL` for the
rendered shape; §8 D1). A `DEFAULT NULL` is semantically a no-default, so a row
written by an app that does not bind the column reads NULL either way; both
ensures compare against v4's REAL runner in `table_info` + `sqlite_master`
(the `chat_informs_permanent_ensure` idiom).

### 3.2 `migrations_state` ids, order, dependsOn

`add-chat-equipped-inventory-v1` (`dependsOn add-equipped-outfit-field-v1`),
then `add-character-inventory-flag-v1` (`dependsOn
add-character-wardrobe-flags-v1`), registered at `migrations/scripts/
index.ts:924-926` AFTER `seedChatWardrobeHistoryMigration` and BEFORE
`addTerminalSessionGeometryMigration` (`:928`). v4's runner records a
`shouldRun` false as "skipped and NOT recorded", so neither is stamped on a
fresh instance; on a migrated instance v4 stamps each once. **UNSTAMPED in v5
(R-B of P4.D304)** — a re-ADD is a no-op on either app, no ping-pong.

### 3.3 The lines (NO-PORT, the deferred runner's)

INFO `Added equippedInventory column to chats` / `Added canManageInventory
column to characters` (inside the `if (added)`; `context:
migration.<id>`); ERROR `Failed to add … column to …`; `database-utils`'
DEBUG `Adding column`; the two prettify labels. v5 logs ONE INFO per column it
added (`target: "quilltap::boot"`, `migrationId`, `columnAdded = true`) with a
v5-worded message, silent on a no-op boot — the `chat_settings_4_11_repair.rs:
107-125` idiom.

### 3.4 How v5 does this today (the exact pattern to copy)

- `crates/quilltap-core/src/db/chat_settings_4_11_repair.rs` — `ensure_column`
  = `table_exists` gate + `schema_probe::add_column_if_missing(main, table,
  column, decl)`; `run_chat_settings_4_11_ensures(main)` runs both in v4's
  registration order, logging per added column; three unit tests (old table →
  both appended in order with the exact `sqlite_master` tail; a pre-existing
  row reads the defaults; half-migrated + idempotent).
- `crates/quilltap-host/src/host.rs:2170-2181` — the chain call inside
  `seed_built_ins`, AFTER the P4.D288 span chain and the wardrobe-history
  ensures (`:2183-2229`), BEFORE the P4.160 index-family backfill
  (`:2230-2257`). **The Inventory ensures slot in at `:2229` (after P4.D315's
  `// === end P4.D315 ===`, before `// === P4.160`)** — v4's registration order.
- `crates/quilltap-harness/tests/restore_vintage_state.rs:162-182` — the
  vintage mirror-boot list calls `run_chat_settings_4_11_ensures` then
  `run_chat_wardrobe_history_ensures`; the new chain joins after the latter
  (memory note `vintage-restore-mirror-boot-list`).
- `crates/quilltap-harness/tests/zero_migrations_rerun_equivalence.rs:118-139`
  — `run_chain` runs the same ensures; the id lists (`:118` the two 4.11 ids,
  `:127-129` the history ids) grow by the two Inventory ids (both EXPECTED
  unstamped by v5 and re-run by v4 ONLY on the migrated base — the P4.D304
  arm at `:276`).
- `crates/quilltap-harness/tests/provisioning_equivalence.rs:288-330` — (1d)
  compares the committed `fresh_schema.json` with v4's LIVE generateDDL dump
  (`QT_FRESH_SCHEMA_LIVE`, required) — FIRES at the regen until the re-dump
  lands; (1b)/(1c) `assert_table_columns` / `assert_index_families` (`:1003,
  1089`) carry the `BOOT_CREATED_HISTORY_TABLE` carve — no new carve needed
  here (both columns are rendered).
- `crates/quilltap-harness/tests/chat_settings_4_11_columns_ensure_equivalence.rs`
  — the per-ensure differential vs v4's REAL narrowed `MigrationRunner` (memory
  note `oracle-runner-and-skip-traps`); the oracle case
  `harness/oracle/cases/chat-settings-wardrobe-image-settings-ensure.ts` /
  `chat-wardrobe-history-ensure.ts` are the shapes to copy for an
  `inventory-columns-ensure.ts` (five modes: fresh / pre-column / v4-migrated /
  v5-ensured-then-v4 / half-migrated).
- `migration_indexes.json` — UNMOVED (neither migration makes an index).
- The repair module registry `crates/quilltap-core/src/db/mod.rs:125-174` (`pub
  mod …_repair`) gains `inventory_columns_repair`.

### 3.5 What ONE re-dump of `fresh_schema.json` from v4's generateDDL at the pin carries

`harness/oracle/provision/dump-fresh-schema.ts:85-140` drives EVERY repository
in `getRepositories()` (`count()` / `findAll()` / `findById`) so each
materialises its tables, then dumps the three partitions' `sqlite_master`
(`{main, mountIndex, llmLogs}`). At `322a5488e` the container has
`logEntries: new LogEntriesRepository()` (`lib/database/repositories/
index.ts:130, 195`), whose `count()` runs its lazy `ensureTable` + the
`onTableEnsured` indexes — so the dump WILL carry `log_entries` + its three
indexes in the `llmLogs` block (the ledger's "VERIFY at re-dump" item: the
dumper's `count()` IS the lazy trigger), AND `terminal_sessions."cols"` /
`"rows"` (bug 255, FIFTEENTH move), AND the two Inventory columns. The
committed file is ONE artifact; the orchestrator must decide (§8 Q1) whether
the keystone's re-dump is the round's one re-dump (the Log and bug-255 lanes
then consume it, with their boot ensures / `log_entries` ruling landing in the
SAME round) or whether the keystone commits a dump with those blocks hand-
pruned (never done before — D23 says "re-dump, never by hand"). Recommended:
ONE re-dump, taken by the keystone lane at `322a5488e`, with `provisioning_
equivalence` (1d) + (1b) green only once every consuming lane's ensure exists
— i.e. the Log's `log_entries` boot ensure and bug 255's two-column ensure are
ALSO this round's (the ledger's §1 already plans round (2) that way).

### 3.6 Reads that the new columns affect (the "named columns" rule)

`chats_read.rs:98` and `characters_read.rs:66-71` name their columns, so a
v5 at `5a94a9d1e` opening a v4-migrated instance is unaffected by the EXTRA
columns, and a v5 after this round opening a PRE-round instance is healed by
the boot ensure before the first read (the ensure runs in `seed_built_ins`
inside `Db::write`, before the pumps). The committed web fixture pairs
(`crates/quilltap-web/tests/fixtures/*-main.db`, 111 files) boot through the
host, so the ensure ADDs the columns on each test's scratch copy — NO widening
owed (unlike P4.D265's `migrate-memories-fixture-columns.ts --module` pass,
which was for a stamped data rewrite). The harness tier-2 pairs are rebuilt
from v4 at the pin and carry the columns.

---

## 4. The differential plan

Per unit: tier, the existing family / case it extends, the fixture pair, what is NEW.

| Unit (v5 module) | Tier | Extends / closest recipe | Fixture | NEW oracle case / notes |
|---|---|---|---|---|
| `inventory/folders.rs` + `item_folders.rs` (the lift) | 1 exact | `wardrobe_folders_equivalence.rs` (`cases/wardrobe-folders.ts`, `QT_ORACLE_WARDROBE_FOLDERS`) — the SAME probe matrix run with BOTH rules | none | `cases/item-folders.ts`: every helper × both rules (noun strings, depth cap, reserved segment, `resolveItemFolder` ancestor spelling, `isItemDocumentPath` with/without `skipFileName`); the wardrobe family must stay green unchanged |
| `inventory/svg.rs` | 1 exact | `cases/*.ts` pure families (e.g. `outfit-hash.ts`) | none | `cases/inventory-svg.ts`: the eight unit shapes + a fuzz list (nested quotes, `<svg/>`, stray closes, DOCTYPE with `[`, CDATA unterminated, mixed-case tags, `xlink:href="#a"`, `style="fill:url(#g)"`, `style="background:url(x)"`, `onClick`, a 16 KiB + 1 glyph, an unknown element closing an allowed one) — bytes exact |
| `inventory/resolve.rs` (+ `equipped_set`, `inventory_display_name`, `kit_state`, `with_inventory_defaults` KEY ORDER) | 1 exact | pure family | none | `cases/inventory-resolve.ts`: `resolveItem` over sparse / explicit-null / missing-parent / non-archetype-parent records, serialised with `JSON.stringify` to pin KEY ORDER; `kitState` matrix; `normalizeEquippedInventory` over string / object / array / junk |
| `inventory_readers.rs` (`parse_inventory_item_file`, `inventory_file_name_for`, the duplicate scan, `serialize_inventory_record`) | 2 structural (+ captured WARN lines) | `vault_wardrobe_item_file_equivalence.rs` (`cases/vault-wardrobe-item-file.ts`) and `vault_wardrobe_read_equivalence.rs` (`build-vault-wardrobe-read-fixture.ts`) | a NEW `build-inventory-keystone-fixture.ts` (the `build-wardrobe-keystone-fixture.ts` twin): four tiers (vault, group store, project store, General), a standalone, an archetype in General, an instance in the vault (sparse, one explicit `null`), a kit naming an archetype + two objects, an archived object, a `.json` note that is not an object, a duplicate-id file at a hand-made path, a file with reserved keys, a `Inventory/images/<id>/x.webp` link, a hand-written file with no timestamps, mixed-case folders | `cases/inventory-readers-tier2.ts`: v4's REAL `readInventoryRecords` + `readDuplicateInventoryPaths` per mount; compare records (minted ids remapped), `duplicatePaths`, and the captured `Two Inventory files claim one id…` / `Inventory file …; skipped` WARNs by text |
| `inventory_writes.rs` (create / updateMany / delete+followOn / the projection with `ext '.json'`, `ownsFile`, `preserveFileNames`) + `db/item_folder_ops.rs` | 2 structural DB diff | `wardrobe_folder_writes_tier2_equivalence.rs` (`cases/wardrobe-folder-writes-tier2.ts` over the keystone pair) + `vault_wardrobe_write_equivalence.rs` | the keystone pair | `cases/inventory-writes-tier2.ts`: every write op then a dump of `doc_mount_file_links` / `doc_mount_documents` / `doc_mount_folders` (content bytes compared — `serializeInventoryRecord` order + `\n`); the note survives; the duplicate left alone; create-/rename-/delete-folder incl. the three conflict sentences; the stamping-before-rename arm (a file without a stable id — which for `.json` objects cannot happen since an object has an id by definition; plant a `hasStableId=false` note to prove the arm is unreachable both sides) |
| `inventory_location.rs` | 2 structural | `wardrobe_location_tier2_equivalence.rs` (`cases/wardrobe-location-tier2.ts`) — copy its probe matrix | the keystone pair | `cases/inventory-location-tier2.ts`: four scopes × ensure, the archived character (reads; every write `CharacterArchivedError`), General, a project with NO `Inventory/` (ensure creates exactly the folder; the read probe returns `[]` without writing), NULL `officialMountPointId`, missing/empty ids, `groupLocationsForCharacter`, `locationKey`, the DEBUG line |
| `inventory/pool.rs` + `dependents.rs` | 2 structural (reads) + captured lines | NEW family; the wardrobe tool pool (`wardrobe_tools_equivalence`) is the nearest | the keystone pair + a second character with its own instance of the General archetype and a kit naming a project object | `cases/inventory-pool-tier2.ts`: `loadInventoryPool` with `chatId` / `projectId` / neither, shadowing (same id in two tiers, an ARCHIVED personal copy over a shared one), `fetchExternalParents` (an instance whose archetype lives in a store the pool can't see — plant a second project), `unreadMounts` (a planted VIRTUAL-column throw on `doc_mount_documents` — memory note `throwing-plant-must-fail-both-reads`), `consultedMounts`, `loadPoolForLocation` for every scope, `findDependents` / `findKitsStrandedByMove` / `isReachableFromKitTier` / `mountScopeOf`, the `Elsewhere` origin, the WARN/DEBUG lines; `findManyInFolderContaining` compared as raw rows |
| `inventory/item_mutations.rs` | 2 structural | `wardrobe_item_mutations` is covered by `wardrobe_routes_equivalence` (`cases/wardrobe-routes.test.ts`, jest-style over `build-wardrobe-routes-fixture.ts`) | a NEW `build-inventory-routes-fixture.ts` + pair (`QT_FIXTURE_INV_MAIN/MOUNT`) | `cases/inventory-mutations-tier2.ts`: create (every shape issue, kit members, svg sanitised, the `null` fields written), instantiate (folder, overrides incl. an inherited title → NO lint), update on instance / archetype / kit (every refusal arm, the silent kind/isArchetype/itemIds drop on an instance, `archived` through `archivedPatch`), clearOverrides, setArchived idempotent, detach / detachAll (incl. the ARCHIVED_DEPENDENTS refusal via a tombstoned second character), delete (force bakes; the kit-member scrub rule: >1 members only; pictures cleaned), duplicate (`(copy)` titles), move/copy to folder (NOT_COPYABLE batch, missing ids 400), folders; compare the file bytes + the INFO `Refused` / DEBUG op lines |
| `inventory/item_images.rs` + the bridge `images_folder` param | 2 structural | `wardrobe_item_images_tier2_equivalence.rs` (`cases/wardrobe-item-images.test.ts`, `build-wardrobe-item-images-fixture.ts`) | the routes pair + pictures planted through v4's REAL `addInventoryItemImage` | `cases/inventory-item-images.test.ts`: add / set-current / delete (the next-current rule) / cleanup / carry+commit on move / fall-through to the archetype's pictures (`inherited`); the `Inventory image for “…”` description bytes; the wardrobe family UNCHANGED |
| `inventory/item_image_generation.rs` | 3 mocked-LLM | `wardrobe_item_image_generation_tier3_equivalence.rs` (`cases/wardrobe-item-image-generation.test.ts`) — canned provider + the Concierge failover | the images pair + an image profile | `cases/inventory-item-image-generation.test.ts`: the prompt bytes (`Still-life product photograph of …` through `withArtDirection` with a lantern aesthetic), the subject precedence (imageDescription → description → title → name, trailing punctuation), no profile / no key 400 sentence, refused → 422 + trail, provider failure → 502, `INVENTORY_ITEM_IMAGE` llm_logs row (+ the Almanack roll-up moves) |
| `inventory/transfers.rs` | 2 structural | `wardrobe_transfers_tier2_equivalence.rs` (`cases/wardrobe-transfers.test.ts`, `build-wardrobe-transfers-fixture.ts`) — its normaliser (ts placeholders, first-seen id map, sha recompute) is REUSED verbatim | a NEW transfers pair | `cases/inventory-transfers.test.ts`: move keeps id (every chat's set + kits still name it), copy mints + NOT_COPYABLE, same-location 400, kits whole (KIT_STRANDED ×2, INVALID_SHAPE, the contextual excuse), an archived source refuses a move BEFORE anything lands, pictures travel, the id-collision 400, the INFO line |
| `inventory/equip.rs` + `equip_effects.rs` + `apply_defaults.rs` + `roster_gate.rs` + `db/chats_inventory.rs` | 2 structural + 3 (Pascal's line is text, no LLM) | `chats_outfits_tier2_equivalence.rs` (`cases/chats-outfits-tier2.ts`) for the column writer; `outfit_change_effects_tier3_equivalence.rs` (`cases/outfit-change-effects-tier3.ts`) for the effects + avatar refresh | the routes pair + a chat with two seated characters (one off a project roster) | `cases/inventory-equip-tier2.ts`: the 13 `equip.test.ts` shapes on a REAL DB — cap per chat, archetype refused, kit expansion (one minted, three carried; `equipped` ORDER = created first), atomic cap breach (nothing written), unequip (kit → members + instances; `notEquipped`), arrival (defaults; continuation copies VERBATIM incl. a dangling id; returning seat not revisited; reset under lock; lenient cap WARN), the `[]` stamp, `OFF_ROSTER` both ways (character provenance refused, operator admitted; what they carry stays), `setEquippedInventory`'s dedup+sort, `removeEquippedInventoryId`, `findChatIdsCarryingInventoryId`; compare `chats.equippedInventory` bytes, the `chat_messages` Pascal row (`*Vivienne takes up Excalibur and a signet ring.*`, `systemKind inventory-change`, NO `pascalMeta`), the avatar-generation job row (`caller_context 'inventory-change'`), the realtime publishes (`chats` with id; `inventory` on pick-up), and every INFO/DEBUG/WARN line |
| `inventory/prompt_section.rs` in `build_context` | 3 mocked-LLM | `build_context_tier3_equivalence.rs` (`cases/build-context-tier3.test.ts`, `build-context-tier3-fixture.ts`, `QT_FIXTURE_BC_MAIN/MOUNT`) — ops in `build-context-tier3.json` (`compression_applied`, …, `progressions_*` already there) | the BC pair + a plant: an `Inventory/` object with a description in the character's vault + `chats.equippedInventory = {"<charId>": ["<id>"]}` | NEW ops `inventory_section_turn`, `inventory_section_continue_mode` (bug 253), `inventory_nothing_described_byte_identical`, `inventory_kit_skipped`, `inventory_sort_base_sensitivity` (titles `éclair` / `Eclair` / `eclair`); compare the composed trailing tail bytes (`…progressions\n\n---\n\n## Carried and equipped\n- …\n\n---\n\n<inform>`) |
| Carina + the greeting + `chat_create`'s opener | 3 | `carina_query_tier3_equivalence.rs` (`cases/carina-query-tier3.test.ts`), `initial_greeting_equivalence.rs` (`cases/initial-greeting.test.ts`), `chat_create_capstone_equivalence.rs` (`cases/chat-create-capstone.test.ts`) | their pairs + the same plant | ops: Carina's three-part join; the greeting user turn with one and two sections (`---` joins); chat create with a default-flagged object → the opener's `trailingContextSections` AND the `{"<charId>": [...]}` cell AND the `[InventoryArrival] Applied arrival defaults` DEBUG |
| the arrival hooks' DB state | 2 | `chat_create_capstone_equivalence` (`QT_ORACLE_CC`), `chat_cast_routes_equivalence` (`QT_ORACLE_CHAT_CAST`, `build-chat-cast-fixture.ts`), the merge family (`chat_merge` — grep `apply_chat_merge` cases) | existing pairs | **RUNTIME BREAK:** at the regen v4 writes `equippedInventory = {"<charId>": []}` on every created chat / added seat / merged character; the v5 side must write the same cell or these families go RED. Extend each with one "a flagged object exists" row |
| the image phrase (appearance / avatar / story / image tool) | 3 | `appearance_sanitize_gate_tier3_equivalence.rs` (`QT_ORACLE_APPEARANCE_GATE`), `avatar_job_tier3_equivalence.rs` (`QT_FIXTURE_AVATAR_*`, `build-avatar-job-fixture.ts`), `story_background_job_tier3_equivalence.rs` (`build-story-background-job-fixture.ts`), the `generate_image` tool family | their pairs + a carried object with an `imageDescription` | ops: the classify text with EMPTY clothing (the byte change — RE-PIN), the sanitizer text `phys. cloth. carrying`, `sceneDescriptionParts` capitalisation, the avatar prompt's `carryingBlock` both branches, the `Carried` expansion entry; `[AppearanceResolution] Could not add carried objects…` on a planted throw |
| `pascal_writer.rs` `post_pascal_inventory_change` | 2 | `pascal_writers_equivalence.rs` (`cases/pascal-writers.ts`) | — | the row bytes + the INFO/DEBUG/ERROR lines |
| the carriers | 2 | `backup_uuid_remap_equivalence.rs` (`cases/backup-uuid-remap.test.ts`), `system_restore_state.rs` (`cases/system-restore.test.ts` — the `RESTORE_TS_MESSAGES` census both sides), `chat_keyed_fields_equivalence.rs` (`cases/character-keyed-fields.ts`), `qtap` export/import families (`system_export_equivalence`, `quilltap_import_*`) | the restore archive builders (`build-restore-archives*.test.ts`) grown with an Inventory tier + a chat carrying one | ops: `remapLinkId` keeps object ids; `planInventoryImagePointerFixes` incl. an unknown file; `22f-quater` incl. a tombstoned vault skip and a planted write failure (`Failed to repoint Inventory pictures in a restored store (…)`); `equippedInventory` re-keyed (unknown character dropped; non-array → `[]`); the `.qtap` export key order (`schema-key-order.json`) + the import's keyed-field arm |
| the title lint + documents + the store hooks + `tool_gate_keys` | 2 structural + captured lines (the "census line" IS the evidence) | NEW family; the gate-key census needs the tools lane's `listAllCustomTools` shape; plants: `Tools/*.tool.json` with `availableWhen.inventory` keys (an any-of array too), a listed-but-missing tool file, an unparseable one, a schema-invalid one | the routes pair + the tool plants | `cases/inventory-title-lint-tier2.ts`: the 23 + 8 + 4 + 5 unit shapes on a REAL DB (the fake store's `gateKeys` → real tool files; `missingMounts` → a mount row deleted after the pool read; `gateKeysUnread` → the planted file states); compare the refusal MESSAGE bytes (`title rejected: …\n…`), the `[InventoryTitleLint] Census` DEBUG (`pools`, `toolStores`, `gateKeys`, `unread` arrays), the INFO rows (`alert`), and that a refused `writeDatabaseDocument` leaves `doc_mount_*` untouched |
| `inventory_route_factory` + transfers + images routes + chat actions (web) | 2 route family (status + body bytes + DB after) | `wardrobe_routes_equivalence` (`cases/wardrobe-routes.test.ts`, jest-style, `build-wardrobe-routes-fixture.ts`), `group_wardrobe_routes_equivalence`, `wardrobe_item_images_routes_equivalence`, `chat_cast_routes_equivalence` for the chat `?action=`s | the routes pair | `cases/inventory-routes.test.ts`: the 12 `routes.test.ts` shapes + every `?action=` × four tiers × the probe outcomes (no store → `{mountPointId: null, items: []}` / `{folders: []}` / `{items: []}`; an unknown project → 404 `Project not found`; General unprovisioned → 500), `?shared=true` with `?projectId`/`?chatId`, `?includeArchived`, every refusal's `{code, ...details}` body + status, the Zod 400s (parse-before-resolve: an invalid body on an unprovisioned project store must NOT create the store — assert the `doc_mount_points` count), the item routes' 404s, `?force=true`, the images route's seven actions (incl. `generate` with a blank / non-JSON / wrong-typed body; `upload` multipart — `curl -T` gotcha per the dogfood memory note; `save-to-store` to an archived vault 409), the chat actions' bodies (`successResponse(result)` = the raw `EquipResult`), the log lines by `logTag` / `logIdKey` |
| the two boot ensures | 2 vs v4's REAL narrowed `MigrationRunner` | `chat_settings_4_11_columns_ensure_equivalence.rs` + `cases/chat-settings-wardrobe-image-settings-ensure.ts` (the five-mode idiom) | a fresh pair + a pre-column pair | `cases/inventory-columns-ensure.ts`; plus `provisioning_equivalence` (1d) green on the re-dump; `zero_migrations_rerun_equivalence` + `restore_vintage_state` lists grown |

**Runtime breaks the orders must own (at the pin):**
1. `harness/oracle/cases/build-context-tier3.test.ts` and every chat-spine case
   that drives v4's `buildContext` / `runCarinaQuery` / `generateGreetingMessage`
   / `handleCreate`: `buildInventorySection` → `equippedIdsOf` on the chat row —
   `[]` on every existing fixture → NO byte move; BUT `handleCreate` /
   `applyOutfitForAddedParticipant` / `applyChatMerge` now WRITE
   `chats.equippedInventory` (`{"<charId>": []}`) — `chat_create_capstone`,
   `chat_cast_routes`, `chats_participants_tier2` and the merge families' DB
   diffs go RED (a NEW cell on v4's side) until v5 writes it.
2. The `appearance_sanitize_gate` classify-text join: `${phys} ${cloth}` →
   `[…].filter(Boolean).join(' ')` — a fixture character with EMPTY clothing now
   sends a different moderation request body; the pinned request bytes move
   (ledger PORT hunk (1)). RE-PIN with the port.
3. `cases/character-keyed-fields.ts` (`chat_keyed_fields_equivalence`): v4's
   `CHARACTER_KEYED_CHAT_FIELDS` is now FIVE; a case that asserts the list or
   remaps a chat object carrying `equippedInventory` moves.
4. The restore census: v4's `restore.ts` gains THREE message templates
   (`Failed to repoint Inventory pictures in a restored store (…): …`, the
   two logger texts) — `RESTORE_TS_MESSAGES` must grow on both sides or
   `system_restore_state`'s census guard fails (memory note
   `restore-census-whitelist`).
5. `wardrobe_folders_equivalence` / `wardrobe_folder_writes_tier2` /
   `vault_wardrobe_public_equivalence`: v4's wardrobe fns are now delegates over
   `item-folders.ts` — the oracle cases import the same names (`normalizeWardrobeFolderPath`
   etc., all still exported from `lib/wardrobe/folders.ts`) → NO import death;
   `folderSegments` / `joinFolder` / … are re-exports → still importable.
6. IMPORT-DEAD check (memory note `setupphase-import-dead-census-and-reaim-lane`):
   `refreshAvatarForOutfit` is KEPT as an alias (`outfit-change-effects.ts:104`)
   — no death; `removeImageFile` became EXPORTED (an addition); `wardrobe-writes.ts`
   dropped its local `WardrobeMountScope` definition in favour of a type alias
   (types only). No v5 oracle builder imports a deleted symbol in this slice.
7. `__tests__/unit/lib/database/repositories/doc-mount-documents-find-in-folder-containing.integration.test.ts`
   is EMPTY at the pin (0 lines) — nothing to seed from; write the raw-row case
   fresh.

---

## 5. Proposed lane cut (disjoint ownership) + Shared contracts

The server side of the Inventory programme splits into FOUR lanes; the SPA, the
tools/Pascal, the smalls (bug 256 / 406dbdd5a / 1d0a31132), the Log, bug 255
and the vendor lanes are siblings.

### Lane K — the KEYSTONE (storage, model, columns, the lift) — stage 0, everything else waits on it

| v4 → v5 (NEW unless noted) |
|---|
| `lib/schemas/inventory.types.ts` → `inventory/types.rs` (`InventoryItem`, `InventoryItemFile` (sparse, `Option<Option<T>>` three-state), `InventoryRecord`, `InventoryOrigin`, `ResolvedInventoryItem`, `INVENTORY_FILE_KEYS`, `INVENTORY_INHERITABLE_FIELDS`, `inventory_shape_issues`, the request-body parsers with v4's Zod messages) |
| `lib/inventory/folders.ts` + `lib/mount-index/item-folders.ts` → `inventory/folders.rs` + `item_folders.rs`; the wardrobe delegation (`wardrobe_folders.rs` EDIT, string-preserving) |
| `vault-overlay/item-folder-ops.ts` → `db/item_folder_ops.rs`; `db/vault_wardrobe_public.rs:666-1000` EDIT (the folder ops become delegates); `db/vault_wardrobe_write.rs` EDIT (`ProjectArrayOptions.ext`) |
| `vault-overlay/inventory-readers.ts` / `inventory-writes.ts` → `db/inventory_readers.rs` / `db/inventory_writes.rs` |
| `lib/inventory/{location,pool,resolve,dependents,equipped-set,errors,svg}.ts` → `inventory/{location,pool,resolve,dependents,equipped_set,errors,svg}.rs` |
| `doc-mount-documents.repository.ts` `findManyInFolderContaining` → `db/doc_mount_documents.rs` EDIT |
| `chats.repository.ts` (+138) → `db/chats_inventory.rs` NEW + `db/chats.rs` / `db/chats_read.rs` EDIT; `character.types.ts` + the characters column sites (§2.1) EDIT; `llm-log.types.ts` → `services/llm_logging.rs` EDIT |
| the two boot ensures → `db/inventory_columns_repair.rs` NEW; `host.rs:2229` EDIT; `restore_vintage_state.rs` / `zero_migrations_rerun_equivalence.rs` EDIT; `fresh_schema.json` RE-DUMP (§3.5); `schema-key-order.json` + `generators/qtap-export.schema.json` EDIT |
| `wardrobe-image-bridge.ts` / `wardrobe/item-images.ts` `imagesFolder` → `services/wardrobe_image_paths.rs`, `wardrobe_image_bridge.rs`, `wardrobe_item_images/{carry,primitives}.rs` EDIT (the parameter) |
| `lib/inventory/item-images.ts`, `item-mutations.ts` (WITHOUT the lint calls — lane L adds them), `item-image-generation.ts`, `transfers.ts` → `inventory/{item_images,item_mutations,item_image_generation,transfers}.rs` |
| `lib/characters/archive-service.ts` — PORTED; `service.rs:286` re-pointed at `inventory::folders::INVENTORY_FOLDER` (EDIT) |
| `project_roster_access.rs:1-12` header (comment) |
| bumps: core (every unit), harness frozen (`0.0.1110` — only if the harness crate's src moves) |

### Lane E — EQUIP + the chat spine + the image phrase + the carriers — stage 1 (after K)

| v4 → v5 |
|---|
| `lib/inventory/equip.ts` WHOLE at the pin (incl. `pickUpItems`, `dropCarriedEverywhere`, `withChatInventoryLock`), `equip-effects.ts` WHOLE (incl. the pending map + flush), `apply-defaults.ts`, `roster-gate.ts` → `inventory/{equip,equip_effects,apply_defaults,roster_gate}.rs` |
| `lib/wardrobe/turn-containers.ts` `PendingInventoryChange` + the container slot → `services/wardrobe_turn_containers.rs` (or wherever v5 keeps `WardrobeTurnContainers` — grep `pending_wardrobe_announcements`) EDIT — **Shared contract S1** |
| `outfit-change-effects.ts` `refreshAvatarForAppearance` → `services/outfit_change_effects.rs` EDIT (rename + alias) |
| `pascal/writer.ts` `postPascalInventoryChange` → `services/pascal_writer.rs` EDIT |
| `prompt-section.ts` → `inventory/prompt_section.rs`; `context-manager.ts` → `services/build_context.rs` EDIT (the list slot + doc word); `carina.service.ts` → `services/carina_query.rs` EDIT; `initial-greeting.ts` + `chats/route.ts` opener → `services/initial_greeting.rs` + `services/chat_create.rs` EDIT |
| the three arrival hooks → `services/chat_create.rs`, `services/chat_participants.rs:1321`, `services/chat_merge.rs` EDIT |
| `image-description.ts` → `inventory/image_description.rs`; `appearance-resolution.ts` (the 4fbdad186 hunk ONLY), `avatar-prompt.ts`, `prompt-expansion.ts` (`tools/generate_image.rs`), `story-background.ts`, `character-avatar.ts` → EDITs |
| `uuid-remap.ts`, `restore.ts` 22f-quater, `character-keyed-fields.ts`, the import's keyed arm → `services/backup/uuid_remap.rs`, `restore/orchestrator.rs`, `chat/character_keyed_fields.rs`, `quilltap_import/reconcile.rs` EDIT |
| bumps: core |

### Lane R — the ROUTES (web + api + Request variants) — stage 1 (after K; the chat actions after E)

| v4 → v5 |
|---|
| `lib/inventory/routes/inventory-route-factory.ts` + `responses.ts` + the eight thin files → `api/inventory_route_factory.rs` NEW, `api/inventory.rs` (General), the tier handlers in `api/{characters,groups,projects}.rs` EDIT, `api/types.rs` variants, `api/engine.rs` arms |
| `app/api/v1/inventory/transfers/route.ts` → `api/inventory.rs` `inventory_transfer_apply` |
| `app/api/v1/inventory/[itemId]/images/route.ts` → `api/inventory_item_images.rs` NEW (the `wardrobe_item_images.rs` twin, no `link-image`) |
| `app/api/v1/chats/[id]/actions/inventory.ts` + the get/post dispatch → `api/chat_inventory.rs` NEW; `quilltap-web/src/wardrobe_routes.rs:629-700` EDIT (the action lists) |
| web: `quilltap-web/src/inventory_routes.rs`, `inventory_images_routes.rs` NEW; `lib.rs` routes; `characters_routes.rs` / `groups_routes.rs` / `projects_routes.rs` EDIT (REST edges for the tier collections + items — the SPA's `inventoryCollectionUrl` paths); `web_edge_body_parse_guard.rs` + `dispatch_wrong_type_census.rs` rows |
| CLI: none (v4's `npx quilltap` has no inventory command at the pin — verified: no `packages/quilltap` hunk names inventory) |
| bumps: core, web |

### Lane L — the TITLE LINT — stage 2 (after K AND the tools/Pascal lane's `listAllCustomTools` reshape + `refs.ts`)

| v4 → v5 |
|---|
| `title-lint.ts`, `title-lint-documents.ts`, `lint-scope.ts` → `inventory/{title_lint,title_lint_documents,lint_scope}.rs` |
| `lib/pascal/tool-gate-keys.ts` → `pascal/tool_gate_keys.rs` (consumes the tools lane's `CustomToolLibrary {entries, unread, errors[].stage, stores_consulted}` and `gate_groups`) |
| the lint calls in `item-mutations.ts` (create / instantiate / update / clearOverrides / setArchived / delete / duplicate / copy-items) → `inventory/item_mutations.rs` EDIT (lane K leaves `// lint seam` call sites as a `TitleLint` trait object defaulting to a no-op? — NO stubs: lane K ships WITHOUT lint; lane L inserts the calls — the K/L order is strict) |
| `pool.ts` `consultedMounts` / `unreadMounts` — lane K ships them (they are data, not lint) |
| `inventory-writes.ts` `withInventoryWritesLinted` + `database-store.ts`'s four hooks → `db/inventory_writes.rs`, `db/database_store.rs` EDIT (§6 T9 for the scope mechanism) |
| `errors.ts` `TITLE_SHADOW` / `TITLE_CHECK_UNAVAILABLE` — lane K ships the enum arms (so the error type is closed); lane L ships the constructors' message assembly? — simpler: lane K ships `errors.rs` WHOLE at the pin (the two classes are pure string assembly) |
| bumps: core |

### Shared contracts (name-level)

- **S1 `PendingInventoryChange` / `WardrobeTurnContainers.pending_inventory_announcements: Option<HashMap<String, PendingInventoryChange>>`** (`chat_id:character_id` keys), `new_wardrobe_turn_containers()` creates it, `turn_containers_of` threads it — lane E defines; the tools lane calls `notify_inventory_changed` / `record_pending_inventory_change` / `flush_pending_inventory_announcements(context, db)`; the orchestrator's `finally` placement (after every save branch, after the wardrobe flush) is the tools lane's.
- **S2 `InventoryChangeContext {user_id: String, provenance: InventoryChangeProvenance, announce: bool, pending: Option<&mut HashMap<…>>}`** and `InventoryChangeProvenance {Operator, Character, Pascal}` — lane E; consumed by the tools + Pascal lanes.
- **S3 `equip_items(db, chat: &EquipChat, character_id, ids, ctx, opts: EquipOptions {lenient, known: &[ResolvedInventoryItem]}) -> Result<EquipResult, InventoryError>`, `unequip_items`, `pick_up_items(…, PickUpOptions {title, equip}) -> PickUpResult`, `drop_carried_everywhere(db, character_id, item_id, also_chat_ids) -> usize`, `with_chat_inventory_lock`** — lane E; the tools lane's nine handlers call them.
- **S4 `InventoryPool` + `load_inventory_pool(main, mount, LoadInventoryPoolOptions {character_id, chat_id, project_id, own_records})`, `InventoryPool::{reachable, resolved, resolve, resolve_id, owns, by_id, own, consulted_mounts, unread_mounts}`** — lane K; consumed by E, R, L and the tools lane's `load_inventory_context` / `resolved_pool_of`.
- **S5 `InventoryError` (the `InventoryRefusal` enum with `code()`, `status()`, `details() -> Map`, `Display` = v4's message)** + `inventory_error_response(e) -> Response` — K defines the enum (all 13 codes), R the response mapper; the tools lane renders `Inventory Error: …` from `Display`.
- **S6 the `Request` variant names** (§2.3) — R defines; the SPA lane's `core-contract.ts` mirrors; the tools lane does not use them.
- **S7 `refresh_avatar_for_appearance(db, user_id, chat_id, character_id, source_context: &'static str)`** — E renames; the wardrobe callers keep `refresh_avatar_for_outfit` (alias).
- **S8 `build_inventory_section(main, mount, chat: &Value, character_id) -> Option<String>`** and the trailing-list slot — E; the interrupts lane appends `turn_yield_instruction` AFTER `turn_skip_instruction` on the same `Vec` (`build_context.rs:3062`): both lanes touch the one list — unify by ORDER: `[…, progressions, inventory, inform, turn_skip, turn_yield]`.
- **S9 `CustomToolLibrary {entries: Vec<{definition, mount_name, definition_path}>, unread: Vec<{mount_name, definition_path, reason}>, errors: Vec<CustomToolLoadError {…, stage: Read|Json|Schema}>, stores_consulted: Vec<String>}`** and `gate_groups(gate) -> Vec<Group>` — the tools/Pascal lane defines; lane L consumes.
- **S10 `resolve_inventory_ref(ref, &ResolvedInventoryPool, equipped: &HashSet<String>, opts) -> Vec<InventoryRefMatch>`** (`refs.ts`) and `ResolvedInventoryPool {items, owns}` — the tools lane defines; lane L consumes (`title-lint.ts:76`).
- **S11 the realtime topic `RealtimeTopic::Inventory` (`"inventory"`)** — the tools lane adds the eighth topic (`realtime_topics_equivalence`); lane E's `pick_up_items` publishes it → lane E depends on it (or E adds the topic and the tools lane's `realtime_topics` pin moves with E — ONE owner: assign to E since `pickUpItems` lives in E's `equip.rs`; ledger row `:402` had it under e1872c94f).
- **S12 the characters PUT key `canManageInventory`, the import/restore field, the export schema property** — lane K (column plumbing); the tools lane's `inventory_tool_flags` reads the column through `characters_read`.

### Version bumps

K: `quilltap-core`; E: `quilltap-core`; R: `quilltap-core` + `quilltap-web`; L: `quilltap-core`. `quilltap-harness` stays frozen at `0.0.1110` unless a harness `src/` normaliser moves (the transfers normaliser is reused as is). `quilltap-host` bumps with K (the ensure chain). CLI unchanged (no v4 CLI hunk).

---

## 6. Traps

- **T1 — `withInventoryDefaults` KEY ORDER is observable.** The resolved
  object reaches the wire through `successResponse` / `created` as-is; V8
  object spread + re-set keeps the FIRST insertion position of a re-set key
  (`kind`, `equippedByDefault` … come first from the defaults, then the
  record's keys in FILE order, then `name` / `parentId` / `folder` keep their
  earlier positions or append). `resolveItem` for an instance builds a
  different order (`inherited` fields in `INVENTORY_INHERITABLE_FIELDS` order
  first, then `id, kind, isArchetype, parentId, itemIds, folder, archivedAt,
  createdAt, updatedAt, overriddenFields, parentMissing, parentName, origin`).
  Use `serde_json::Map` (preserve_order) and compare BYTES
  (`serde_json::to_string`), never `==` (memory note `serde-json-map-eq-ignores-order`).
- **T2 — `setEquippedInventory` sorts with the DEFAULT JS sort** (UTF-16
  code-unit order on lowercase-hex UUIDs = byte order — `Vec<String>::sort()`
  matches for ASCII) and de-dups with `Set` (first-seen). `removeEquippedInventoryId`
  does NOT re-sort. The stored JSON cell is `JSON.stringify` of the map in
  insertion order (a character's entry keeps its original position; a re-set
  keeps position; `delete` then re-add APPENDS — `resetInventoryToDefaults`
  moves the character's entry to the END of the map).
- **T3 — the positional marshal (§Headline 6):** append the new columns at the
  END of `chats_read.rs`'s SELECT list and `characters_read.rs`'s
  `SLIM_COLUMNS`, with the matching `row.get(103)` / `row.get(28+)` indices —
  NOT at DDL position.
- **T4 — `localeCompare` ×4:** `scanInventoryFolder` and `findRecordsAnywhere`
  sort PATHS with `localeCompare(b)` (default sensitivity — `collation::
  locale_compare`); `renderInventorySection` / `renderCarryingPhrase` and
  `listFoldersIn` sort with `{sensitivity: 'base'}` (`locale_compare_base`).
  V8's sort is stable; ties keep input order (the equipped set's stored order).
- **T5 — `JSON.parse` fidelity:** `parseInventoryItemFile` and
  `normalizeEquippedInventory` run V8's `JSON.parse` on operator-written files
  — the v5 twin is `jsjson` (P4.154's V8-parser port); a duplicate key keeps
  the LAST value, `__proto__` is a plain key.
- **T6 — `new Date(value)` in `toIso`:** a hand-written `createdAt` like
  `2026-10-20` or `Oct 20 2026` parses through V8's `Date.parse` →
  `js_date_parse_ms` (memory note `v4-safequery-writes-and-v5-date-helpers`);
  a parse to NaN falls back to the document's timestamps, then the epoch.
- **T7 — the ensure's two shapes:** `TEXT` (rendered) vs `TEXT DEFAULT NULL`
  (migrated) and `INTEGER` vs `INTEGER DEFAULT NULL` — BOTH legal
  `sqlite_master` texts (the P4.D249 precedent; the ledger row `:402` says the
  rendered shape is `INTEGER DEFAULT NULL` — it is NOT; §8 D1).
- **T8 — the `equippedInventory` cell is written at EVERY chat create / seat
  add / merge** (`{"<charId>": []}`) even when no Inventory exists — the
  committed capstone / cast / merge corpora change at the regen; and the cell's
  PRESENCE is what `hasEquippedEntry` keys on (an empty array = "arrived
  already"). A v5 that forgets the `[]` stamp re-applies defaults on every
  re-seat.
- **T9 — `AsyncLocalStorage` has no Rust twin.** `withInventoryWritesLinted` /
  `inventoryWritesAreLinted()` mark the mutation layer's projection so the
  document-store hooks skip it. In v5 every Inventory write goes through
  `db/inventory_writes.rs` on the writer task: port the SCOPE as an explicit
  `linted: bool` (or a `WriteProvenance::InventoryMutation`) argument on
  `write_database_document` / `delete…` / `move…` / `move_database_folder`,
  defaulting to "lint" for every other caller — behaviour-equivalent, and the
  `database-store-title-lint.test.ts` fourth case ("never loads the lint for
  other paths or the mutations' own writes") becomes a call-count assertion.
  NOT a refusal.
- **T10 — `runSerialized` keys.** v4 serialises per folder
  (`Inventory:<mp>`) and per chat (`inventory-equip:<chatId>`) through ONE
  module map; in v5 the writer task serialises every write, but the READ-plan-
  WRITE spans (`equipUnserialized` reads the pool, plans, THEN writes) must
  stay atomic across awaits — run the whole equip under one `Db::write`
  closure (the `set_equipped_outfit` RMW idiom) or the race the lock prevents
  (an equip landing mid-arrival being rewound, `equip.test.ts:172`) returns.
  The `Promise.all` of the four tier reads is order-free.
- **T11 — `assertParticipant` ignores `type`** (`equip.ts:98-106`) while the
  route's `castOf` requires `type === 'CHARACTER'`: a USER-type participant
  with a `characterId` is seated for `equipItems` but excluded from the GET and
  from `reset-defaults`. Port both as written.
- **T12 — `pool.resolved()` excludes archived; `byId` includes them;
  `resolveId` reads `byId`.** So `equipItems` on an ARCHIVED direct id finds it
  and refuses `NOT_EQUIPPABLE (archived)`; `readEquippedSet` drops it;
  `kinOf` / `defaultIdsFor` never see it. `assertKitMembers` uses `pool.get`
  (archived members ALLOWED in a kit — they are skipped at equip time).
- **T13 — the classify-text byte change** (`appearance-resolution.ts:458-461`)
  moves the moderation request for any character with an EMPTY clothing or
  physical description, Inventory or not — the P4.D178 / 406dbdd5a pins
  (smalls lane) and this hunk touch the same function: coordinate the re-pin.
- **T14 — `describeInventoryChange` article rules** are regex on the RAW name
  (`/^(uni|use|usu|uti|eu|one|once)/i` then `/^[aeiou]/i`; plural =
  `/s$/i && !/(ss|us|is)$/i` on the LAST whitespace word); a titled object is
  the trimmed title with NO article; the Oxford-comma `joinList`. The line is
  wrapped `*…*` and ends `.` — `*Vivienne sets down the lantern and takes up
  a signet ring and picks up some lockpicks.*` (three clauses joined by ` and `,
  no comma).
- **T15 — `instantiate` writes `fileStem`** so the file is named after the
  archetype until the instance has a title; the projection names it
  `title || name || fileStem || object-<id8>` — for an instance with NO
  overrides the file is `<archetype title or name>.json` (collision-suffixed by
  the projection: ` (2)` …). A kit's instantiation puts the instance in the
  KIT's folder when the kit is the character's own (`kitFolder`), else the
  root.
- **T16 — the `svg` null write:** `createInventoryItem` sets `svg:
  sanitizeInventorySvg(fields.svg)` → `null` for an absent/blank glyph, so
  every created file carries `"svg": null` (and `"archivedAt": null`,
  `"imageFileId": null`, `"parentId": null`); `instantiate` writes NO `svg`
  key unless overridden. Pin the file bytes.
- **T17 — the `fetchExternalParents` origin is a lie by design** (`{scope:
  'project', id: null, name: 'Elsewhere'}`) — `isOnClosedShelf` treats such a
  parent as a project-shelf object (origin.scope === 'project'), but it is only
  ever a LOOKUP target (never in `resolved()`), so the gate never sees it.
- **T18 — `findManyInFolderContaining` is a SUBSTRING hit** (`instr`) — a
  UUID inside `metadata` or a kit's `itemIds` matches; callers re-parse and
  match `record.id` exactly. The LIKE prefix is `inventory/%` on
  `LOWER(relativePath)`; v5's `doc_mount_documents` partition is the mount
  index DB (raw SQL there, memory note `production-sql-capture`).
- **T19 — `handleGetInventory` iterates `pool.resolved()` for kits** (non-
  archived, archetypes excluded by `kind === 'kit'`), keyed by kit id, in
  shadowed Map order; `inventory[characterId]` in cast order.
- **T20 — the greeting's `trailing.length > 0` filter** drops whitespace-only
  sections; `openerInventory` is built from the chat row read BEFORE
  `baseParams` (the read moved up — any oracle that counts `chats.findById`
  calls in the greeting phase moves by zero, same count, earlier).
- **T21 — `update` on an instance deletes `kind` / `isArchetype` / `itemIds`
  from the patch SILENTLY** after the two refusals; an explicit `isArchetype:
  false` on an instance is accepted and dropped.
- **T22 — the character tier's `?shared=true` with `includeArchived`** returns
  `Array.from(pool.byId.values()).map(resolve)` — shadowed, archived INCLUDED,
  in `general → project → group → own` insertion order (Map order of first
  insertion, with re-sets keeping the general-tier position!). An object in BOTH
  General and the vault lists at General's position with the VAULT's record.
  Pin the array order.

---

## 7. Tiering suggestion

**Tier 1 — must land (the keystone cannot be unified without them):**
- The two columns + the re-dump + the boot ensure chain + the three harness
  homes (`provisioning_equivalence` 1d green; `zero_migrations_rerun`;
  `restore_vintage_state`).
- The storage layer whole: types, readers, writes with `ext: '.json'`, the
  item-folder lift (string-preserving; the wardrobe families green), location,
  pool, resolve, dependents, equipped set, errors, svg, mutations (without
  lint), item images (the `images_folder` parameter), transfers,
  `findManyInFolderContaining`, `chats_inventory.rs`'s four fns,
  `INVENTORY_ITEM_IMAGE`.
- The chat spine: `equip` / `unequip` / arrival defaults (the `[]` stamp) /
  reset / `OFF_ROSTER`, Pascal's line, the avatar refresh alias, the trailing
  section (ungated) in `build_context` + Carina + the greeting's user turn +
  the opener, the image phrase through the five pipelines, the carriers
  (`remapLinkId`, `22f-quater`, the keyed field, the key order, the import
  arm).
- The routes: the four-tier factory + transfers + images + the three chat
  actions, every refusal's `{code, …details}` body and status, the
  `COLLAPSE_CENSUS` / dispatch-census rows.

**Tier 2 — should land (same round, trailing lane L):** the title lint whole
(it is a REFUSAL gate on every titled write; without it v5 accepts titles v4
refuses — a model-visible divergence on `inventory_create` / the editor, but
only once the tools lane's gate keys exist; the lint's `document-write` door
also fires on Scriptorium edits of `Inventory/*.json` — operator-visible).
Lands after the tools lane's `CustomToolLibrary` reshape and `refs.rs`.

**Tier 3 — explicit typed refusals:** none proposed. The one candidate (the
`AsyncLocalStorage` lint scope) ports as T9's explicit argument. `pickUpItems`
/ `dropCarriedEverywhere` are Tier 1 in lane E even though only the tools lane
calls them (a later tools-lane landing then needs no E change).

---

## 8. Open questions / rulings owed / disagreements with the ledger

**Rulings owed:**
- **Q1 — the re-dump's extra blocks.** Taken at `322a5488e`, the dump carries
  `log_entries` (+3 indexes, `llmLogs`) and `terminal_sessions.cols/rows`
  besides the two Inventory columns (§3.5). Ruling: ONE re-dump consumed by
  three lanes (keystone / Log / bug 255) in ONE round — or a keystone-only
  dump with the other blocks pruned by hand (never done; D23 forbids hand
  edits). Recommended: one round, one dump; the Log lane's `log_entries`
  ruling (lazy first-use vs boot ensure vs `fresh_schema.json` row) is then a
  PREREQUISITE of the keystone's green (1d).
- **Q2 — ownership of the `canManageInventory` plumbing** (the characters PUT
  key `put.ts:77`, the export schema property, the import/restore field): the
  ledger row `:402` lists them under `e1872c94f` (tools); this survey assigns
  them to lane K as column plumbing (every read/write site of the column in one
  lane). Confirm.
- **Q3 — `runSerialized` → the writer task.** v4's per-folder and per-chat
  promise chains have no v5 twin; T10 proposes whole-op `Db::write` closures.
  Confirm that an equip's pool READ may run inside the write closure (it reads
  the mount index + main — the P4.D279 wear-commit precedent did the same).
- **Q4 — `findManyInFolderContaining`'s home:** a `DocMountDocumentsRepository`
  method with v4's raw SQL (the P4.D288 `instr` precedent?) or an
  Inventory-module query. Recommended: the repository (one SQL twin,
  `doc_mount_documents_tier2` row).
- **Q5 — the import's keyed arm at the pin:** `lib/import/quilltap-import/
  reconcile.ts:423-440` routes unknown keyed fields to
  `pendingOutfitNotifications` (`_` arm) — MEASURE whether v4 at the pin gave
  `equippedInventory` its own arm (the diff of `reconcile.ts` in the span is
  EMPTY — `git diff --stat` lists no `lib/import` file — so v4's `.qtap` import
  writes a re-keyed `equippedInventory` INTO `pendingOutfitNotifications`?).
  If so this is a v4 defect to FILE (bug) and a RULED divergence (fix v5, both
  ways pinned).
- **Q6 — the tier item REST edges** (`/api/v1/{characters,groups,projects}/{id}/inventory(/{itemId})`):
  the wardrobe gives the character tier REST and the group/project tiers
  dispatch-only; the SPA's `inventoryCollectionUrl` uses REST for all four.
  Decide with the SPA survey (REST for all four is the v4 shape).

**Disagreements with the ledger rows:**
- **D1** — row `:402` says generateDDL renders `"canManageInventory" INTEGER
  DEFAULT NULL`; it renders `"canManageInventory" INTEGER` (no default — see
  `"canCreateOutfits" INTEGER` at `fresh_schema.json:6`, the same Zod shape).
  Two shapes, as `:401` already says for `chats`.
- **D2** — row `:401` says `d88c1ce39` "fold into the Inventory SPA lane" —
  agreed, and it has NO server hunk at all (only the editor hint, help, a
  test).
- **D3** — row `:401` lists the realtime `inventory` topic under `e1872c94f`
  (`:402`); `equip.ts:507` (`pickUpItems`, e1872c94f) is the publisher and
  lives in lane E — assign the topic to E (S11).
- **D4** — row `:403`'s "arrival defaults are ungated holds only if arrival
  uses `operator` provenance" — MEASURED: `apply-defaults.ts:59` passes
  `provenance: 'operator'` and `assertShelfReach` returns at once for it
  (`equip.ts:119`) — ungated, confirmed.
- **D5** — row `:401`'s "`.qtap` export spreads it, import creates it": no
  `lib/export` / `lib/import` hunk exists in the span; the export carries the
  column only because the chat row is spread whole, and the import's keyed
  remap is the pre-existing `reconcile.ts:423` call (Q5).
- **D6** — row `:407` says the lint "touches `pascal/roster.rs:416
  load_tools_from_mount` (the ENOENT skip at `:508` — the `unread` seam)" — the
  seam is the tools/Pascal lane's `listAllCustomTools` reshape (S9); lane L
  consumes it, it does not edit `roster.rs`.

**Open questions for the drafters:**
- The web transport's status for an `InventoryRefusal` with `status 400`
  (`INVALID_SHAPE`, `NOT_A_PARTICIPANT`): `Response::error(ErrorKind::BadRequest,
  msg)` carries no `details` today? — check `api/types.rs`'s error bag
  (`details` exists: `wardrobe_routes.rs` reads `details.refused`); the
  `{code, ...details}` spread must survive to the HTTP body byte-for-byte
  (`{"success": false, "error": "...", "details": {"code": "...", ...}}`).
- Whether `inventory/svg.rs` needs ICU for `toLowerCase` (ASCII tag names
  only — `str::to_ascii_lowercase` differs from JS on non-ASCII attribute
  names, which the name regex `[a-zA-Z_:][-a-zA-Z0-9_:.]*` already rejects;
  but `keepAttribute`'s `compact.toLowerCase()` runs on VALUES — use
  `to_lowercase` (byte-identical to JS per Phase 1)).
