# Survey — the wardrobe / Inventory item family, the tier walk under it, and Pascal presets

Read-only planning survey, **2026-11-04**. v4 = `~/source/quilltap-server`, read
ONLY through `git show 624d0d25c:<path>` / `git diff 236c04e35 624d0d25c --
<path>` / `git show <sha>` (the checkout's working tree carries unrelated
optimizer edits and was never read). **The oracle baseline is `236c04e35`; the
round PINS at `624d0d25c`** (`4.13.0-dev.8`). v5 = `main` at **`ff1651177`**
(core 0.0.1427, host 0.0.223, web 0.0.256, cli 0.0.38, harness 0.0.1110, SPA
0.5.971). Every v4 line number below is AT THE PIN; every v5 line number is at
`ff1651177`. Nothing is taken from commit prose — every claim is from a shipped
hunk; where a ledger row disagrees with a hunk this survey says so (§0.3).

## 0. Header

**Commits in scope** (topological order on v4 `main`)

| sha | date | subject | class (from the hunks) |
|---|---|---|---|
| `fd165fce0` | 10-30 | pascal: custom-tool presets follow the tool walk | PORT-NEW (server `?action=presets` + preset pool) + SPA |
| `b2b783b47` | 10-31 | item-browser: shared folder browser for the Wardrobe and the Inventory | SPA only (refactor + the container catalogue) |
| `a0ed5ecad` | 10-31 | item-browser: shared server folder layer | PORT (a route REMOVED; folder log lines move; transfer refusals typed) — NOT "no behaviour change" |
| `134961572` | 11-02 | Remove dead Inventory folder wrappers (#124) | NO-PORT as a commit; its v5 twins are LIVE and retire with `a0ed5ecad` |
| `0c47f4539` | 11-02 | Refactor: settle tier walk once in reach.ts (#141) | PORT — #135 / #136 fixed + a tier-walk refactor that is NOT behaviour-equal (§1.4) |
| `600c2906f` | 11-02 | Refactor 4.12 spec 6: the browser family, finished (#143) | PORT (server: probe/no-provision, `?action=duplicate`, `wardrobe` topic, item-route preamble, images factory) + SPA |
| `27fb3ec77` | 11-02 | salon: avatar opens full size; carried items under the title | SPA only |

Other v4 commits that touch the same v4 files in the span (NOT this area,
named for collisions): `db6190b44` (`lib/realtime/job-topics.ts` +1 —
`ESTABLISHED_FACT_MEMORIES`), `aec626eff` (`lib/realtime/topic-map.ts` —
`mountPoints` scoped by id; `lib/mount-index/database-store.ts`),
`4efea65e5` (`lib/wardrobe/apply-outfit-selections.ts`, `slot-ops.ts` — Green
Room spread), `c34986421` / `5efe53ea7` (`components/item-browser/*` —
`DestinationPickerDialog` rename, `FolderBrowserShell`, `ItemActionMenu`,
`lib/folder-browser/document-actions.ts`), `29ba4d3f8` / `94f041764`
(wardrobe / inventory row + dialog CSS / WCAG).

### 0.1 Headline — ten things the planner must not miss

1. **The tier walk is a KEYSTONE, not a wardrobe detail.** `0c47f4539` makes
   `TierResolveOptions.roster` (`'gated' | 'open'`) and `.enabled`
   (`'require' | 'ignore'`) REQUIRED (`tiered-mount-pool.ts:73-101`), moves
   the project-roster gate INSIDE the walk (`:376-388`), makes the project tier
   **official store first, then every link** (`resolveProjectMountPointIds`,
   `:186-207`), adds `dropDisabledMounts` (`:445-475`) and `includeEmpty`
   (`:266`, `:288`), and adds a NEW module `lib/mount-index/reach.ts` (468
   lines: `resolveOwnerStore`, `groupOriginMap`, `castGroupMounts`,
   `resolveCastPool`, `orderMounts`, `bucketMounts`, `classifyMount`). Every v5
   caller of `resolve_tiered_mount_pool` (`build_context.rs:1356`,
   `tools/search.rs:711`, `doc_edit/path_resolver.rs:493`,
   `pascal/roster.rs:1020`) and every hand-rolled walk
   (`wardrobe_instructions.rs:76-125`, `pascal/roster.rs:568-592`,
   `wearable_pool.rs:424`, `inventory/pool.rs:242`,
   `services/core_whisper.rs:223`, `tools/self_inventory.rs:879` / `:1943`,
   `inventory/dependents.rs:216-241`, `inventory/title_lint_documents.rs:144-176`)
   moves. It must land FIRST (stage 0) and it is shared with the scenarios
   (#136), context-builder (knowledge injector) and scriptorium-search areas.
2. **"Behaviour equal" is false — the commit body itself lists the moves.**
   Inside a tier the EARLIER mount now wins: the wardrobe / Inventory pools read
   each tier REVERSED (`wardrobe/pool.ts:103`, `inventory/pool.ts:156`, the
   group-tier list `wardrobe-route-factory.ts:237`); custom tools and dressing
   instructions stop sorting mount ids lexicographically (`orderMounts`,
   `reach.ts:362-383`); the wardrobe transfer probe walks groups FORWARD and
   now probes every linked project store (`transfers.ts:173`, `:194-212`);
   `self_inventory` credits a store shared by two groups to the first only;
   the title lint classifies a group's / project's official store as THAT
   owner (`title-lint-documents.ts:57-68`). §1.4 has the full list.
3. **#135 (wardrobe GETs provision a store) is LIVE in v5.**
   `api/wardrobe_route_factory.rs:28-35` ("ported as written"), `:636` and
   `:845` resolve with `ensure: scope != Character`, so `list_store_items`
   (`:759`) and `instructions_get_store` (`:887`) run on the WRITER and mint a
   `Project Files:` / `Group Files:` store plus `Wardrobe/` on a READ. At the
   pin both are read probes answering `{ mountPointId: null, wardrobeItems: [] }`
   / `{ instructions: null }` (`item-folder-routes.ts:275-291`,
   `wardrobe-route-factory.ts:138-152`, `:298-321`). The P4.166 degraded-General
   arm (`:784-818`) is re-shaped by the move off the writer.
4. **`GET /api/v1/wardrobe/transfers` is REMOVED and v5's SPA still calls it.**
   v5 serves it at `quilltap-web/src/lib.rs:486-488` →
   `wardrobe_routes.rs:460` → `CoreRequest::WardrobeTransferDestinations`
   (`api/types.rs:2919`, `api/engine.rs:5652`) →
   `api/wardrobe.rs:125-178` (+ the degraded arm `:159-178`). The SPA reads it
   at `wardrobe/wardrobe-transfer-dialog.ts:263` (contract
   `core/core-contract.ts:8768-8770`). v4 moved its dialog onto the shared
   container catalogue (`lib/hooks/use-container-catalogue.ts`, b2b783b47 — the
   characters / projects / groups lists) BEFORE removing the route. Removal is
   a server+SPA pair (R-B).
5. **A NEW wardrobe `POST ?action=duplicate`** (`wardrobe-route-factory.ts:258-296`,
   schema `wardrobe.types.ts:251`) builds the copy SERVER-side through the
   client-safe `draftFromItem` / `draftToPayload` (`lib/wardrobe/item-draft.ts`)
   and `createWardrobeSchema.parse` — v5 has those only in the SPA
   (`apps/web/src/app/wardrobe/item-draft.ts`). The title is chosen against
   the TIER's titles (archived included), not the merged list on display (the
   v4 SPA's old client-side rule) — v5's `next_copy_title` (`wardrobe.rs:1537`)
   is reusable.
6. **A NEW `wardrobe` realtime topic, published from EVERY wardrobe location
   write** (`wardrobe/location.ts:122-138`, with a new DEBUG
   `[WardrobeLocation] Location write {op, mountPointId, scope, published,
   context}` on every write), in `REALTIME_TOPICS` between `inventory` and
   `storytellerRounds` (`realtime.types.ts:64`), a `wardrobe.*` /
   `Wardrobe/`-path arm in `job-topics.ts:170-184`, `:232-235`, and the SPA
   topic map (`topic-map.ts:40-41`, `:150-156`). v5's ten-topic array
   (`realtime/types.rs:45-56`, test `topics_are_v4s_ten_in_order` `:119`)
   becomes eleven; the Inventory's publish (`inventory/location.rs:130`) is
   the precedent. The new DEBUG reddens EVERY captured wardrobe-write log
   corpus.
7. **One item-route preamble for both kinds** (`lib/mount-index/item-folder-routes.ts`,
   328 lines) and **one folder layer** (`item-folder-location.ts`, 155): the
   folder DEBUGs become `[ItemFolders] Created folder` / `Renamed folder` /
   `Deleted folder` / `Moved items to folder` `{kind, scope, mountPointId, …,
   context}` (from `[FolderMutations] … wardrobe folder`,
   `[ItemMutations] Moved wardrobe items to folder`, `[InventoryMutations] …`);
   the Inventory's folder actions gain INFO lines it never had; the Inventory's
   item 404s become per-tier (`Group inventory object` / `Project inventory
   object` / `General inventory object`, `inventory-route-factory.ts:79-84`);
   the archived 409's INFO becomes `[Item routes] Refused a ${noun} write to an
   archived character` (`item-folder-routes.ts:200` — **"a inventory"**, v4's
   grammar); NEW DEBUGs `No ${noun} location`, `General not provisioned;
   nothing to read`, `No store yet; nothing to read`.
8. **One item-images route factory** (`lib/mount-index/item-images-route.ts`,
   377 lines) under both `wardrobe/[itemId]/images` and
   `inventory/[itemId]/images`. Wire bodies hold; log TEXT and KEYS move
   (`Listed wardrobe item images` → `Listed item images`, `Listed object
   images` → same, `scope` joins every meta, the Inventory gains set-current /
   delete-image / refusal INFOs it never logged). Only logs — but every
   images-route capture reddens.
9. **IMPORT-DEAD at the pin is wider than the ledger says, and one listed
   case is NOT dead.** Dead: `cases/item-mutations-tier2.ts:145-147`
   (`folder-mutations`), `cases/inventory-keystone-tier2.ts:266-272`
   (`createInventoryFolder` / `rename…` / `delete…`, dies at first use),
   `cases/item-folders.ts:68` (`normalizeInventoryFolderName`, a static import
   — dies at load), AND three GET-transfers callers the ledger misses
   (`cases/wardrobe-transfers.test.ts:203-212`,
   `cases/wardrobe-routes.test.ts:331-336`,
   `cases/degraded-mount-routes.test.ts:641`, `:651`). NOT dead:
   `cases/inventory-types.ts` (imports only `renameInventoryFolderSchema` et al.,
   still at `lib/schemas/inventory.types.ts:258-263`).
10. **Pascal presets need a server-side Rust twin v5 said it would never
    need.** `apps/web/src/app/pascal/tool-presets.ts:18-25` records "this module
    has NO Rust twin and needs none"; at the pin `lib/pascal/tool-preset-pool.ts:25`
    imports `parsePresetFromPath` (`tool-presets.ts:47`) server-side. The new
    `GET ?action=presets` (`custom-tools/route.ts:380-433`, dispatch `:583`)
    walks `resolveTieredMountPool(…, { roster: 'open', enabled: 'require' })`
    in `orderedMounts` order (now `orderMounts` — keystone-dependent), dedups by
    preset name (nearer shadows farther, `shadows[]`), sorts by `localeCompare`,
    and reports `stores` + `unread`. `?action=` on the GET gains `presets`,
    so the unknown-action envelope's `availableActions` becomes
    `["interventions","presets"]` (v5 `custom_tools_routes.rs:58-63`, test
    `quilltap-web/tests/query_param_semantics_equivalence.rs`).

### 0.2 Rulings owed (full list in §10)

R-A the tier-walk keystone's shape and ownership (shared with three other
areas); R-B `GET /wardrobe/transfers` removal (server + SPA pair, same round);
R-C port v4's two shared factories as shared Rust modules (item-route
preamble + images route) vs. byte-matching the two kinds separately; R-D the
duplicate action's server-side draft twin; R-E v4's "Refused a inventory
write" grammar (port verbatim; a v4 issue is optional); R-F the
`wardrobe_location` owner read moving to RAW (`findByIdRaw`) for project /
group.

### 0.3 Ledger corrections (each verified against the pin's bytes)

- **`0c47f4539`** row says "behaviour claimed equal — measure". Measured: NOT
  equal — eleven behaviour moves (§1.4 list (a)–(k)), four of them on wardrobe /
  Inventory / Pascal surfaces. It also says #135 / #136 live in this commit's
  routes; #136 is the SCENARIO routes (`groups/scenarios/route.ts`,
  `scenario-item-route-factory.ts`) — a scenarios-area lane, not this area's.
- **`600c2906f`** row: the collection GET's DEBUG `No store yet; no wardrobe
  items` that `0c47f4539` introduced is GONE at the pin (replaced by the
  preamble's `No store yet; nothing to read {[logIdKey], scope, context}`), and
  `No store yet; no wardrobe folders` likewise. Port the pin, not the
  intermediate.
- **`a0ed5ecad`** row ("no behaviour change — verify"): FALSE on four counts —
  (1) the two "Failed to remove … from source after move" refusals were a 500
  with THAT message; they are now thrown and answer
  `500 Failed to transfer wardrobe item` with an ERROR `[WardrobeTransfers v1]
  Failed to transfer item` (`transfers/route.ts:40-41`;
  `transfers.ts:377`, `:383`); (2) the read-back ERROR and the success INFO
  lose ` v1` (`[WardrobeTransfers] …`, `transfers.ts:405`, `:417`); (3) the
  folder DEBUGs re-text (headline 7); (4) the Inventory folder actions gain
  INFO lines. Its v5 refs are also wrong: the GET is `wardrobe_routes.rs:460`
  (router `lib.rs:486-488`), the variant `api/types.rs:2919` (not `:2788`).
- **`134961572`** row ("v5 intersection: none"): v5 HAS live twins —
  `inventory/item_mutations.rs:1374-1438` (`create_inventory_folder` /
  `rename_inventory_folder` / `delete_inventory_folder`, route-called at
  `api/inventory_route_factory.rs:840`, `:863`, `:886`) and
  `inventory/folders.rs:91` (`normalize_inventory_folder_name`, harness
  `inventory_folders_equivalence.rs:25`, `:279`). They are live because v5 has
  not adopted `a0ed5ecad`'s shared layer; they retire WITH that port. No v5
  twin of `AnnouncementRouteSender` (grep empty).
- **The ledger's IMPORT-DEAD list** (§1, the "measured by grep" bullet): add
  the three GET-transfers callers (headline 9); strike `cases/inventory-types.ts`.
- **`fd165fce0`** row: correct as far as it goes; add that `0c47f4539` then
  changes the same pool to `{ roster: 'open', enabled: 'require' }` and moves
  the disabled check from the loop (`!mount.enabled`) into the pool — equal on
  output, plus one DEBUG `Disabled stores left out of the pool {dropped}`.

## 1. Per commit, the shipped hunks

### 1.1 `fd165fce0` — Pascal presets on the tool walk (14 files; server 4)

- **`app/api/v1/chats/[id]/custom-tools/route.ts`** — `handlePresets`
  (`:380-433`): `tool` must match `IDENTIFIER_PATTERN` else
  `400 A valid custom tool name is required` (`:388-389`); `404 Chat`; the
  perspective is chosen exactly as POST chooses (`asCharacterId` match, else
  `preferOperator`); none → `400 No character participant with id <id> is in
  this chat` / `400 This chat has no character whose presets could be read`
  (`:408-413`); DEBUG `Custom-tool presets listed for the operator {context,
  chatId, tool, characterId, presets, stores}` (`:423`); 200 body =
  `{ presets: [{tier, mountPointId, mountName, name, path, shadows:[…]}],
  stores: [{tier, mountPointId, mountName}], unread: [{…, reason}] }` (spread
  order: `{ ...store, name, path, shadows }`). GET dispatch
  `{ interventions, presets }` (`:583`).
- **`lib/pascal/tool-preset-pool.ts`** (NEW, 134 at the pin): §0.1 item 10.
  WARN `Preset listing failed for store {context, mountPointId, mountName,
  error}` (`:98`); DEBUG `Tool presets resolved {context, toolName,
  characterId, stores, presets, shadowed, unread}` (`:123`). `mount` missing →
  skipped silently (`:89`).
- **`lib/pascal/custom-tools.ts`** — `listToolsFolderFiles` exported
  (`:234-262`): database = prefix list filtered to `isRootOfToolsFolder`
  (`:218`); disk = every `isFile()` entry; ENOENT/ENOTDIR → `[]`. Definition
  loading filters `.tool.json` after (`:363-365`) — **equal** to v5's
  `is_root_tool_file` + `list_tool_files_from_disk` (`roster.rs:290-297`,
  `:933-955`), which must be split the same way so presets share it.
  `orderedMounts` exported (`:442`).
- **`lib/query/keys.ts`** — `customTools.presets(chatId, asCharacterId,
  toolName)` (client).
- **v5 surface:** `pascal/roster.rs` (split the listing; export
  `ordered_mounts`), NEW `pascal/tool_preset_pool.rs`, NEW
  `pascal/tool_presets.rs` (`parse_preset_from_path`, `PRESET_NAME_PATTERN`,
  `PRESET_FILE_SUFFIX` — tier-1 vs v4 `tool-presets.ts`), `api/custom_tools.rs`
  (a `chat_custom_tool_presets` beside `chat_custom_tools_list` `:371`; reuse
  `load_perspectives` `:87`, `prefer_operator` `:273`), `api/types.rs` + `engine.rs`
  (a `ChatCustomToolPresets { chat_id, tool, as_character_id }` variant),
  `quilltap-web/src/custom_tools_routes.rs:52-77` (the action list + arm).
  Ported originally by P4.D43 (`c988fbd2` run presets) and the run route by
  P4.6ay / P4.D345.

### 1.2 `b2b783b47` — the shared SPA folder browser (32 files; client only)

No server hunk. NEW `components/item-browser/{FolderBrowserShell,
ItemActionMenu, ItemFolderField, ContainerSelect, useFolderBrowserState}`,
`lib/folder-browser/view.ts` (the shared view model, moved out of
`lib/wardrobe/folder-view.ts` −132), `lib/hooks/use-container-catalogue.ts`
(characters / projects / groups lists, `localeCompare`-sorted, `Untitled
project` / `Untitled group` fallbacks — the GET-transfers replacement). §6 maps
it.

### 1.3 `a0ed5ecad` — the shared server folder layer (21 files; server 12)

- **`lib/mount-index/item-folder-location.ts`** (NEW): `ItemFolderLocation`
  (`:33-60`), `ItemFolderKind {rules, pathError}` (`:71-74`), `locationKey`
  (`:77`), `createItemFolder` / `renameItemFolder` / `deleteItemFolder` /
  `moveItemsToFolderIn` (`:96-155`) with ONE DEBUG shape `[ItemFolders] <op>
  {kind, scope, mountPointId, …meta, context}` (`:85-93`). Note the meta GAINS
  `itemCount` on the Inventory's create / rename (the old Inventory logged
  none) and LOSES the Inventory's `op` key.
- **`lib/mount-index/item-folder-routes.ts`** (+108 here, 328 at the pin after
  `600c2906f`): `itemFolderActionHandlers` (`:85-139`): parse → resolve
  (create ensures; rename / delete do not) → mutate → INFO `${logTag}
  {Created|Renamed|Deleted} ${noun} folder {[logIdKey], userId, mountPointId,
  …meta, context}` → 200; refusal → the kind's mapping.
- **`lib/wardrobe/transfers.ts`** (NEW, 439 at the pin): the route body moved
  out; `WardrobeTransferSourceNotFoundError` (`:58-63`) /
  `WardrobeTransferRefusedError` (`:66-71`); the route (`transfers/route.ts`,
  40 lines) maps them (`notFound('Wardrobe item')`, `badRequest(message)`),
  Zod → `400 issues.join('; ')`, archived → `409 An archived character's
  wardrobe cannot be changed`, else ERROR + 500. **GET REMOVED.**
- **`lib/wardrobe/folder-mutations.ts`** — shrunk here, DELETED by `600c2906f`.
- **`lib/schemas/wardrobe.types.ts`** — `createWardrobeFolderSchema` +
  `deleteWardrobeFolderSchema` → ONE `wardrobeFolderPathSchema` (same rules).
- **v5 surface:** `services/wardrobe_folder_mutations.rs` (`[FolderMutations]`
  `:46`, `:73`, `:97` — DELETE into the shared module),
  `services/wardrobe_item_mutations.rs:620` (`[ItemMutations] Moved wardrobe
  items to folder`), `inventory/item_mutations.rs:1270-1438` (the Inventory's
  move + folder wrappers, `log_op!` `:1294`, `:1385`, `:1404`, `:1428`),
  `services/wardrobe_transfers.rs` (`:674`, `:681` the two 500 texts; `:748-770`
  payload helpers), `api/wardrobe.rs:125-178` (GET), `:420-530` (POST),
  `:474`, `:493` (the ` v1` tags). Ported by P4.D281 (folder actions, transfers),
  P4.D324 / P4.D325 (Inventory routes), P4.D309 (copy-to-folder).

### 1.4 `0c47f4539` — the tier walk (54 files; prod 31)

The hunks, then the measured behaviour moves.

- **`lib/mount-index/reach.ts`** (NEW, 468): quoted in full at the pin.
  `resolveOwnerStore(scope, id, repos, userId, { ensure?, archived, context })`
  (`:138-185`) reads characters / projects / groups **RAW**
  (`findByIdRaw`); `ensure` provisions via `ensureOwnerOfficialStore` and heals
  the folder with WARN `[Reach] Failed to ensure ${folder} folder
  {mountPointId, folder, context, error}` (`:119-124`), swallowed or rethrown
  per `onFolderFailure`; `archived: 'refuse'` → DEBUG `[Reach] Archived
  character refused {characterId, context}`. `orderMounts` (`:362-383`) is
  tier order + pool order + a `seen` dedup. `bucketMounts` (`:399-422`) is the
  knowledge-search tiering. `classifyMount` (`:446-468`): General first, then
  character by vault (raw), then project / group by OFFICIAL store (raw).
- **`tiered-mount-pool.ts`** — headline 1. New DEBUGs `Character off project
  roster — project tier withheld {projectId, characterId}` (`:381-385`),
  `Disabled stores left out of the pool {dropped}`; WARNs `Project official
  store lookup failed {projectId, error}`, `Mount lookup failed; store left out
  of the pool {mountPointId, error}`.
- **Wardrobe:** `location.ts:213-244` (resolve over `resolveOwnerStore`,
  `archived: 'allow'`, the folder heal `swallow`); `groupLocationsForCharacter`
  `:247-252` (strongest first); `pool.ts:97-104` (reversed `readTier`);
  `wardrobe-instructions.ts:57-66` (`orderMounts`, no sort);
  `transfers.ts:173`, `:194-212` (forward group probe; `projectLocations`);
  `wardrobe-route-factory.ts` GETs → `ensure: false` (#135; superseded by the
  `600c2906f` probe).
- **Inventory:** `location.ts:199-229` (same), `:232-237`; `pool.ts:156`
  (reversed); `dependents.ts:118` + `title-lint-documents.ts:57` (`classifyMount`).
- **Pascal:** `custom-tools.ts:442` (`orderedMounts = orderMounts`), `:513-525`
  (`{ roster: 'open', enabled: 'ignore' }`); `tool-preset-pool.ts:72-80`
  (`{ roster: 'open', enabled: 'require' }`, `:89` drops its own enabled check).
- **Other areas (named, not owned here):** `knowledge-injector.ts:136-144`
  (`bucketMounts`, its own `dedupeTierTriple` call REMOVED — relies on a
  deduped input), `context-manager.ts` (`{ roster: 'open', enabled: 'ignore' }`),
  `search-scriptorium-handler.ts` (`bucketMounts`, roster `'gated'` inside the
  walk — gains the DEBUG it never logged), `doc-edit/path-resolver.ts` (its
  own DEBUG `Path resolver: character off project roster — project tier
  withheld` → the pool's text), `scenario-builder/mount-pool.ts`,
  `scenarios/location.ts`, `scenario-item-route-factory.ts`,
  `groups/scenarios` routes (#136), `chats/[id]/actions/{documents,group-stores}.ts`,
  `core-whisper.ts:262-265`, `self-inventory/builders.ts:160`, `:962`.

**Measured behaviour moves (the "equal" claim, falsified):**

- (a) Project tier = official store FIRST, then links, deduped — even when the
  official store is not linked (a restore / a hand-edited row). Order moves
  for every consumer; membership moves only for the unlinked case.
- (b) Wardrobe pool: a same-id item in two group stores (or two project
  stores) now resolves to the EARLIER mount (was the later).
- (c) Inventory pool: the same, and `consulted` is pushed in REVERSED order
  (`pool.ts:157`) — re-measure whether `consulted` reaches a wire or log.
- (d) Group-tier listing (`?scope=group`): earlier group's item wins.
- (e) Custom tools: same-tier shadowing by pool order, not mount-id sort.
- (f) Dressing instructions: probe order by pool order, not mount-id sort —
  `cases/wardrobe-instructions-cascade.ts` rows
  `group_tier_deduped_and_sorted` / `project_tier_deduped_and_sorted` go red.
- (g) Wardrobe transfer source probe: groups forward (was reversed); linked
  project stores now probed (was the official store only).
- (h) `self_inventory` groups: a shared store credited to the first group
  only; group names from the RAW row (was hydrated `findById`); its DEBUG
  `self_inventory: skipping group with unavailable store` GONE; the project's
  stores through `resolveProjectMountPointIds` (official first).
- (i) Core whisper: WARN `[CoreWhisper] Failed to resolve group Core mounts`
  GONE (failures now inside `resolveGroupMountsForCharacter`).
- (j) Title lint: a group's / project's official store becomes a scope
  `group` / `project` location WITH the owner's id and name (was a nameless
  `project` location, `id: null`).
- (k) Wardrobe / Inventory location: project / group owner read RAW (was
  hydrated `findById` — Zod parse + overlay); the folder-heal WARN re-texted
  (`[SharedWardrobe]` / `[InventoryLocation]` → `[Reach]`, + `folder` key).

### 1.5 `134961572` — dead Inventory wrappers (4 files)

Removes `createInventoryFolder` / `renameInventoryFolder` /
`deleteInventoryFolder` (`item-mutations.ts`), `normalizeInventoryFolderName`
(`folders.ts`), `AnnouncementRouteSender` (`announcer/writer.ts`). Dead in v4
because `a0ed5ecad` routed the folder actions through the shared layer. v5:
§0.3.

### 1.6 `600c2906f` — the browser family (79 files; server 22)

- **`lib/mount-index/item-folder-routes.ts`** (+236): `OWNER_LABELS`
  (`:164-169`), `itemErrorResponse` / `withItemErrors` (`:192-223`),
  `itemRoutePreamble` (`:247-328`): `resolveLocation` (`:260-268`: DEBUG
  `${logTag} No ${noun} location {scope, ensure, context}`, General → `500
  Quilltap General store is not provisioned yet`, else `404 <owner>`),
  `probe` (`:275-291`), `getFolders` (`:293-306`: DEBUG `Listed ${noun}
  folders {[logIdKey], mountPointId, count, context}` — the Inventory GAINS
  `mountPointId`).
- **`lib/wardrobe/routes/wardrobe-route-factory.ts`** — the preamble; the GET
  list (`:298-321`) and instructions GET (`:138-152`) are probes;
  `handleDuplicate` (`:258-296`: `duplicateWardrobeItemSchema.parse`
  (`itemId: z.string().min(1, 'itemId is required')`), resolve WITHOUT ensure,
  `404 <item label>`, `nextCopyTitle(current.title, readItems(true).titles)`,
  `createWardrobeSchema.parse(draftToPayload({...draftFromItem(current), title}))`,
  DEBUG `Duplicating wardrobe item {[logIdKey], itemId, title, folder,
  context}`, `componentLookupFor` when the copy has components, `createItem`,
  INFO `Duplicated wardrobe item {[logIdKey], userId, mountPointId, itemId,
  copyId, title, context}`, `201 { wardrobeItem }`); POST dispatch order
  `instructions, create-folder, rename-folder, delete-folder, move-items,
  copy-items, duplicate` (`:324-330`).
- **`lib/wardrobe/location.ts`** — the `write` wrapper + topic (headline 6).
  `updateMany` publishes only when `updated.length > 0`; `delete` only when it
  deleted.
- **`lib/inventory/routes/{inventory-route-factory,responses}.ts`** —
  headline 7; `INVENTORY_ITEM_ERRORS` (`responses.ts`), the archived INFO
  re-texted from `[Inventory routes] Refused a write to an archived character`.
- **`lib/mount-index/item-images-route.ts`** (NEW) + both route adapters
  (`wardrobe/[itemId]/images/route.ts:58-118`, `inventory/…:41-63`) — headline 8.
  Messages per kind: wardrobe `invalidJson: 'Invalid JSON body'`, inventory
  `'Request body is not valid JSON'`; the wardrobe adapter keeps `link-image`
  and its own 409 text (`This character is archived; their wardrobe cannot be
  changed`).
- **Realtime:** `realtime.types.ts:64`, `job-topics.ts:144`, `:170-184`,
  `:232-235`, `topic-map.ts:40-41`, `:150-156`.
- **`lib/mount-index/path-utils.ts`, `database-store.ts`,
  `mount-points/[id]/folders/route.ts`** — `normaliseRelativePath(…,
  {allowRoot})` + `isSafeRelativePath`: the mount-index area's (the
  database-store normaliser now throws `FileOpError('INVALID_PATH')` where it
  threw a plain `Error` — a status-code question for that lane).
- **SPA:** `ItemControlDialogShell`, `ItemFolderPickerDialog` (a NEW one —
  `c34986421` renamed the old to `DestinationPickerDialog`), `ItemImageViewer`,
  `ItemTransferFooter`, `useItemOverlays`, `ItemDialogProvider`,
  `use-item-folders`, `refresh-unless-realtime`, `lib/inventory/request.ts`.

### 1.7 `27fb3ec77` — avatar full size; carried items (SPA only)

`MessageDesktopAvatar` opens the Salon image viewer; equipped Inventory items
under the title (`InventoryGlyph` `fallback: 'abbreviation'`);
`useChatCarriedItems` shares `useChatInventory`'s cache; `getMessageAvatar`
returns `characterId`. No server hunk.

## 2. The v5 surface map

| v4 at the pin | v5 at `ff1651177` | ported by |
|---|---|---|
| `tiered-mount-pool.ts` | `db/tiered_mount_pool.rs` (888) — `TierResolveOptions` `:88-96`, `resolve_tiered_mount_pool` `:376` | P4.D256, P4.149 |
| `reach.ts` (NEW) | none — NEW module (`db/reach.rs` proposed) | — |
| `resolveProjectMountPointIds` | `tools/wardrobe_shared.rs:43-53` (links only) | P4.149 |
| `wardrobe/location.ts` | `db/wardrobe_location.rs` (`resolve` `:402-533`, WARN `:542-553`, groups `:557-584`, test `:592-617`) | P4.D279 |
| `wardrobe/pool.ts` | `wearable_pool.rs` (`:231-235`, `:424`) | P4.D279 |
| `wardrobe/wardrobe-instructions.ts` | `wardrobe_instructions.rs:76-125` | P4.D119 |
| `wardrobe/transfers.ts` + route | `services/wardrobe_transfers.rs` (`:385` group probe, `:674`, `:681`), `api/wardrobe.rs:125-530`, web `wardrobe_routes.rs:460-480`, `lib.rs:486-488` | P4.D112, P4.D281 |
| `wardrobe-route-factory.ts` | `api/wardrobe_route_factory.rs` (2173) | P4.D281 |
| `folder-mutations.ts` (deleted) | `services/wardrobe_folder_mutations.rs` | P4.D279 |
| `item-folder-location.ts` / `item-folder-routes.ts` (NEW) | none — NEW shared modules | — |
| `inventory/location.ts` | `inventory/location.rs` (`ensure_inventory_folder` `:96-108`, `resolve` `:369-470`, owner `find_by_id` `:443`, `:451`) | P4.D324 |
| `inventory/pool.ts` | `inventory/pool.rs:242-275` | P4.D324 |
| `inventory/dependents.ts` | `inventory/dependents.rs:216-241` (`find_all` ×2) | P4.D324 |
| `inventory/title-lint-documents.ts` | `inventory/title_lint_documents.rs:144-190` | P4.D340 |
| `inventory/item-mutations.ts` | `inventory/item_mutations.rs:1270-1438` | P4.D324 |
| `inventory-route-factory.ts` + `responses.ts` | `api/inventory_route_factory.rs` (labels `:993`, `:1095`, `:1108`, `:1354`; archived INFO `:227`; folders `:588`, `:827-890`) | P4.D325 |
| item images routes | `api/wardrobe_item_images.rs` (923), `api/inventory_item_images.rs` (574), web `wardrobe_images_routes.rs`, `inventory_images_routes.rs` | P4.D263, P4.D275, P4.D326 |
| realtime topics | `realtime/types.rs:15-56`, `job_topics.rs:154-250`, SPA `core/realtime.types.ts:61`, `core/realtime-topic-map.ts:119-170` | P4.D328 (Inventory precedent) |
| `pascal/custom-tools.ts` | `pascal/roster.rs` (`:290`, `:568-592`, `:933-955`, `:1010-1040`) | P4.6ay, P4.D328, P4.D332 |
| `tool-preset-pool.ts` / `?action=presets` | none — NEW | — |
| `core-whisper.ts` | `services/core_whisper.rs:223` | (core whisper order) |
| `self-inventory/builders.ts` | `tools/self_inventory.rs:879-930`, `:1943` | P4.D350 |

**NO-COUNTERPART:** `AnnouncementRouteSender`; v4's jest suites; the
`refactor-4-12-*.md` specs (docs mirror only).

## 3. Schema / D23 shapes

None. No migration, no table, no column in any of the seven commits.

## 4. Log lines (v4 text + keys + level → v5 home)

| level | v4 text at the pin | keys | replaces | v5 home |
|---|---|---|---|---|
| WARN | `[Reach] Failed to ensure ${folder} folder` | mountPointId, folder, context, error | `[SharedWardrobe] Failed to ensure Wardrobe folder` / `[InventoryLocation] Failed to ensure Inventory folder` | `db/wardrobe_location.rs:542-553`, `inventory/location.rs:96-108` |
| DEBUG | `[Reach] Archived character refused` | characterId, context | NEW (scenarios only) | reach |
| DEBUG | `Character off project roster — project tier withheld` | projectId, characterId | path-resolver's own text | tiered pool |
| DEBUG | `Disabled stores left out of the pool` | dropped | NEW | tiered pool |
| WARN | `Project official store lookup failed` / `Mount lookup failed; store left out of the pool` | projectId,error / mountPointId,error | NEW | tiered pool |
| DEBUG | `[WardrobeLocation] Location write` | op, mountPointId, scope, published, context | NEW | wardrobe location |
| DEBUG | `[ItemFolders] Created folder` / `Renamed folder` / `Deleted folder` / `Moved items to folder` | kind, scope, mountPointId, …, context | `[FolderMutations] … wardrobe folder`, `[ItemMutations] Moved wardrobe items to folder`, `[InventoryMutations] …` | NEW shared module |
| INFO | `${logTag} {Created,Renamed,Deleted} inventory folder` | [logIdKey], userId, mountPointId, …, context | NEW for the Inventory (wardrobe text unchanged) | route preamble |
| DEBUG | `${logTag} No ${noun} location` | scope, ensure, context | NEW | preamble |
| DEBUG | `${logTag} General not provisioned; nothing to read` / `No store yet; nothing to read` | context / [logIdKey], scope, context | `No store yet; no wardrobe folders` | preamble |
| DEBUG | `${logTag} Listed inventory folders` | [logIdKey], **mountPointId**, count, context | (no mountPointId) | preamble |
| INFO | `[Item routes] Refused a ${noun} write to an archived character` | context | `[Inventory routes] Refused a write…` (wardrobe: NEW) | preamble |
| DEBUG / INFO | `${logTag} Duplicating wardrobe item` / `Duplicated wardrobe item` | §1.6 | NEW | wardrobe factory |
| ERROR / INFO | `[WardrobeTransfers] Transferred outfit did not read back…` / `[WardrobeTransfers] Wardrobe item transferred` | unchanged keys | the ` v1` tags | `api/wardrobe.rs:474`, `:493` |
| DEBUG/INFO | images factory: `Item not in the named container`, `Listed item images`, `Generate body is not valid JSON`, `Generated item image`, `Generation failed`, `Uploaded item image`, `Set current item image`, `Deleted item image`, `Listed save targets` (+context), `Refused to save…`, `Saved item picture to a store`, `Save to store rejected` | `{itemId, scope, …, context}` | per-kind texts | both images modules |
| DEBUG | `Custom-tool presets listed for the operator`; `Tool presets resolved`; WARN `Preset listing failed for store` | §1.1 | NEW | Pascal |

## 5. Harness / oracle impact

**IMPORT-DEAD at the pin (stage-0 re-aims):** `cases/item-mutations-tier2.ts`
(re-aim `createWardrobeFolder` et al. at `createItemFolder(WARDROBE_FOLDER_KIND,
…)`), `cases/inventory-keystone-tier2.ts:266-272` (→ `createItemFolder(
INVENTORY_FOLDER_KIND, …)`), `cases/item-folders.ts:68` (→
`normalizeItemFolderName(INVENTORY_FOLDER_RULES, input, pathError)` — the
`inv_normalize_name` rows keep their ids), and the three GET-transfers rows
(`wardrobe-transfers.test.ts` `__destinations`, `wardrobe-routes.test.ts`
`transferDestinations`, `degraded-mount-routes.test.ts`
`transfers_destinations` / `_vaultless`) — RETIRE with the route, and their
Rust sides (`wardrobe_transfers_tier2_equivalence.rs`,
`quilltap-web/tests/wardrobe_routes_equivalence.rs`,
`degraded_mount_routes_equivalence.rs`) drop the rows by name.

**Families that redden at regen (v4 drift, not v5 bugs):**

- keystone: `tiered_mount_pool_equivalence` (`cases/tiered-mount-pool.ts` —
  its matrix passes `opts: {}`, still runnable; the `basic` row's project tier
  gains the official store first), `knowledge_injector_equivalence`,
  `scenario_builder_mount_pool_equivalence` (other areas),
  `core_whisper_equivalence`, `self_inventory_equivalence`;
- wardrobe: `wardrobe_location_tier2_equivalence` (WARN text),
  `wardrobe_folder_writes_tier2_equivalence`, `item_mutations_tier2_equivalence`,
  `wardrobe_instructions_tier2_equivalence` + `wardrobe-instructions-cascade.ts`
  (the two `_sorted` rows), `wardrobe_instructions_routes_equivalence`,
  `group_wardrobe_routes_equivalence`, `quilltap-web/tests/{wardrobe_routes,
  wardrobe_folders_routes, wardrobe_instructions_routes, characters_wardrobe_route,
  wardrobe_copy_items_route}`, `wardrobe_transfers_tier2_equivalence`,
  `wardrobe_tools_equivalence` / `pascal_wardrobe_effects_equivalence` (the new
  location-write DEBUG wherever logs are captured),
  `wardrobe_item_images_tier2_equivalence`,
  `quilltap-web/tests/{wardrobe_images_routes,wardrobe_item_images_routes}_equivalence`;
- Inventory: `inventory_keystone_tier2_equivalence`, `inventory_folders_equivalence`,
  `quilltap-web/tests/inventory_routes_equivalence` (labels, folder INFOs),
  `quilltap-web/tests/inventory_item_images_routes_equivalence`,
  `inventory_title_lint_equivalence` (classify), `inventory_tools_equivalence`
  (pool order where a fixture has ≥2 group stores);
- Pascal: `pascal_roster_equivalence` / `cases/pascal-custom-tools-discovery.test.ts`
  (mocks ONLY `resolveTieredMountPool` from `tiered-mount-pool`; `reach.ts`
  loads its other names as `undefined` — `orderMounts` survives; verify at
  regen), `pascal_custom_tools_route_equivalence`,
  `quilltap-web/tests/query_param_semantics_equivalence` (`availableActions`);
- realtime: `realtime_topics_equivalence` (`cases/realtime-topics.ts` — eleven
  topics; also `aec626eff`'s scoped `mountPoints` and `db6190b44`'s job arm),
  `realtime_publish_sites_guard`, `quilltap-web/tests/realtime_hint_wire`;
- degraded: `degraded_mount_routes_equivalence` (General list no longer on the
  writer).

**Closest recipes for NEW differentials:** `reach.ts` → the real-DB tsx case
`cases/tiered-mount-pool.ts` + `fixtures/build-tiered-mount-pool-fixture.ts`
(add rows for `roster: 'gated'`, `enabled: 'require'`, an UNLINKED official
store, two groups sharing a store, `classifyMount`); the preset pool + route →
`cases/pascal-custom-tools-route.test.ts` (jest route corpus) and
`pascal-custom-tools-discovery.test.ts` (mocked pool); `parse_preset_from_path`
→ a tier-1 pure case beside `cases/pascal-trigger-phrases.ts`; the
duplicate action → `cases/wardrobe-routes.test.ts` (and the Inventory's
duplicate rows in `inventory-routes.test.ts` for the 400 shapes); the folder
layer → the re-aimed `item-mutations-tier2.ts`; `draftFromItem` /
`draftToPayload` → tier-1 like `next_copy_title_equivalence.rs`; the topic →
`cases/realtime-topics.ts`.

## 6. SPA

| v4 at the pin | v5 at `ff1651177` |
|---|---|
| `components/item-browser/FolderBrowserShell.tsx`, `useFolderBrowserState.ts`, `ItemActionMenu.tsx`, `ItemFolderField.tsx`, `ContainerSelect.tsx`, `ItemControlDialogShell.tsx`, `ItemImageViewer.tsx`, `ItemTransferFooter.tsx`, `useItemOverlays.ts` | NEW in `apps/web/src/app/item-browser/` (today: `item-folder-bar.ts`, `item-folder-picker-dialog.ts`, `item-folder-row.ts`, `item-select-bar.ts`, `item-tier-row.ts`) |
| `ItemFolderPickerDialog.tsx` (new body) | `item-browser/item-folder-picker-dialog.ts`; retire `wardrobe/wardrobe-folder-picker-dialog.ts`, `inventory/inventory-folder-picker-dialog.ts` |
| `lib/folder-browser/view.ts` | `wardrobe/folder-view.ts` → shared |
| `lib/hooks/use-container-catalogue.ts` | NEW (replaces `wardrobeTransferDestinations`, `wardrobe-transfer-dialog.ts:263`) |
| `lib/hooks/use-item-folders.ts`, `use-wardrobe-folders.ts` | `wardrobe/wardrobe-folders.api.ts`, `inventory/inventory-folders-data.ts` |
| `components/providers/{item,wardrobe,inventory}-dialog-provider.tsx` | `wardrobe/wardrobe-dialog.service.ts`, `inventory/inventory-dialog.service.ts` |
| `WardrobeBrowser`, `wardrobe-control-dialog`, `WardrobeTransferDialog`, `wardrobe-item-row`, `useWardrobeItemActions` (duplicate → `?action=duplicate`), `WardrobeFolderField`, `WardrobeItemImageSection`, `wardrobe-item-thumbnail` | `wardrobe/wardrobe-browser.ts` (828), `wardrobe-control-dialog.ts` (1386), `wardrobe-transfer-dialog.ts`, `wardrobe-item-row.ts`, `wardrobe-item-actions.ts`, `item-editor/wardrobe-folder-field.ts`, `item-editor/wardrobe-item-image-section.ts`, `wardrobe-item-thumbnail.ts`, `wardrobe-image-viewer.ts`, `wardrobe-item-overlays.ts`, `wardrobe.api.ts` |
| `InventoryBrowser`, `inventory-control-dialog`, `InventoryTransferDialog`, `inventory-item-row`, `useInventoryItemActions`, `inventory-item-editor`, `inventory-image-viewer` (deleted), `lib/inventory/request.ts`, `use-inventory.ts` | `inventory/inventory-browser.ts`, `inventory-control-dialog.ts`, `inventory-transfer-dialog.ts`, `inventory-item-row.ts`, `inventory-item-actions.ts`, `inventory-item-editor.ts`, `inventory-image-viewer.ts`, `inventory.api.ts`, `inventory-refusal.ts` |
| `lib/query/refresh-unless-realtime.ts`, topic map `wardrobe` | `core/realtime-topic-map.ts`, `core/realtime.types.ts:61`, the `onTopic('wardrobe', …)` subscriptions |
| `CustomToolRunDialog.tsx` (+175: store-labelled presets, shadows note, "Save into" select, unread note, loads from the preset's own store) | `chat/custom-tool-presets.ts` (`:117-270` today walks `mountFilesList` on the vault), `chat/custom-tools.api.ts:28-38` (query key), `chat/custom-tools-popup.ts:188`, `pascal/tool-presets.ts:18-25` (the "no Rust twin" note retires) |
| `27fb3ec77`: `MessageDesktopAvatar`, `Avatar.onImageClick`, `inventory-glyph` `fallback`, `useChatCarriedItems` | `chat/message-row.ts:97-130`, `:528`, `ui/avatar.ts`, `inventory/inventory-glyph.ts`, `inventory/chat-inventory-data.ts` |
| `components/scenarios/ScenarioRow.tsx` adopts `ItemActionMenu` | scenarios SPA (other area) |

## 7. The lane split, by FILE OWNERSHIP

- **K — tier-walk keystone (stage 0; shared with scenarios / context /
  scriptorium surveys).** Owns `db/tiered_mount_pool.rs`, NEW `db/reach.rs`,
  `tools/wardrobe_shared.rs` (`resolve_project_mount_point_ids` re-homed or
  delegating), and ONE fenced hunk each at the four callers
  (`services/build_context.rs:1356`, `tools/search.rs:696-715`,
  `doc_edit/path_resolver.rs:480-500` — its own roster gate removed,
  `pascal/roster.rs:1020-1030`). Oracle: `cases/tiered-mount-pool.ts` + a NEW
  `cases/reach.ts`. **Collision:** `build_context.rs` with the context-builder
  lane; `search.rs` / `path_resolver.rs` with the scriptorium / doc-edit area.
- **W1 — item routes + folder layer + #135 + duplicate + topic (server,
  after K).** Owns `db/wardrobe_location.rs`, `inventory/location.rs`, NEW
  `item_folder_location.rs`, NEW `api/item_folder_routes.rs`,
  `api/wardrobe_route_factory.rs`, `api/inventory_route_factory.rs`,
  `services/wardrobe_folder_mutations.rs` (delete), `services/wardrobe_item_mutations.rs`
  (move hunk), `inventory/item_mutations.rs` (folder wrappers out),
  `inventory/folders.rs` (`normalize_inventory_folder_name` out), NEW
  `wardrobe/item_draft.rs`, `realtime/types.rs`, `realtime/job_topics.rs`
  (wardrobe arm), web `wardrobe_routes.rs` (collection arms), `inventory_routes.rs`;
  `api/types.rs` + `engine.rs` (the duplicate verb — fenced hunks). Oracle
  re-aims: `item-mutations-tier2.ts`, `inventory-keystone-tier2.ts`,
  `item-folders.ts`. **Collision:** `realtime/types.rs` / `job_topics.rs`
  with the Deus-ex (`ESTABLISHED_FACT_MEMORIES`) and document-store
  (`mountPoints` scoped) lanes.
- **W2 — item images factory (server, independent).** Owns NEW
  `api/item_images_route.rs`, `api/wardrobe_item_images.rs`,
  `api/inventory_item_images.rs`, web `wardrobe_images_routes.rs`,
  `inventory_images_routes.rs`.
- **W3 — transfers + walk consumers (server, after K).** Owns
  `services/wardrobe_transfers.rs`, `api/wardrobe.rs` (GET removed, typed
  refusals, tags), web `lib.rs:486-488` (GET un-routed), `api/types.rs` +
  `engine.rs` (the `WardrobeTransferDestinations` variant REMOVED — fenced),
  `wearable_pool.rs`, `inventory/pool.rs`, `wardrobe_instructions.rs`,
  `inventory/dependents.rs`, `inventory/title_lint_documents.rs`,
  `tools/self_inventory.rs` (the two hunks), `services/core_whisper.rs`.
  **Collision:** `tools/self_inventory.rs` with any character-kind lane that
  touches `self_inventory`; `wardrobe_location.rs::group_locations_for_character`
  is W1's (W3 consumes it).
- **P — Pascal presets server (after K).** Owns NEW `pascal/tool_preset_pool.rs`,
  NEW `pascal/tool_presets.rs`, `pascal/roster.rs` (listing split,
  `ordered_mounts` → `order_mounts`, minus K's one hunk), `api/custom_tools.rs`,
  web `custom_tools_routes.rs`, `api/types.rs` + `engine.rs` (fenced).
- **S1 — item-browser SPA.** Owns every SPA row of §6 except the Pascal and
  `27fb3ec77` rows, plus `core/core-contract.ts` (fenced hunks: remove
  `wardrobeTransferDestinations`, add the duplicate verb), `core/realtime*.ts`.
  **Collisions:** `item-browser/*` with the scriptorium folder-browser SPA
  lane (`c34986421` / `5efe53ea7`: `DestinationPickerDialog`,
  `FolderBrowserShell`, `ItemActionMenu`, `lib/folder-browser/document-actions`)
  — one owner for `item-browser/*` or a stacking rule; `realtime-topic-map.ts`
  with the document-store SPA (`aec626eff`); the row / dialog files with the
  WCAG / opaque-dropdown lane (`29ba4d3f8`, `94f041764`); ScenarioRow with the
  scenarios SPA.
- **S2 — Pascal presets + Salon riders SPA (after P's contract).** Owns
  `chat/custom-tool-presets.ts`, `chat/custom-tools.api.ts`,
  `chat/custom-tools-popup.ts`, `pascal/tool-presets.ts` (note), `chat/message-row.ts`
  (fenced: `27fb3ec77` only — collides with the Salon SSE / `?msg=` lanes),
  `ui/avatar.ts`, `inventory/inventory-glyph.ts`, `inventory/chat-inventory-data.ts`,
  and a `core-contract.ts` hunk (the presets verb).
- **Help / docs** (`help/{wardrobe,inventory,custom-tools,chats}.md`,
  `docs/v4/` API.md, folder-browser.md, refactor specs) → the round's vendor lane.

**Stacking:** K → {W1 ∥ W3 ∥ P}; W2 free; S1 after W1 + W3 contracts; S2
after P. `api/types.rs` / `engine.rs` are touched by W1, W3, P — fenced named
hunks, unifier merges.

## 8. Tiering

Tier-1: `parse_preset_from_path`, `order_mounts`, `bucket_mounts`,
`group_origin_map`, `draft_from_item` / `draft_to_payload`. Tier-2 (real DB):
`reach` (owner store, classify, cast pool), the tiered pool, the folder layer,
the location writes + topic, the pools' within-tier order, transfers, the
title lint, self_inventory / core whisper. Route corpora (jest): the wardrobe /
Inventory collection + item + images routes, custom-tools `?action=presets`.
No tier-3 (no model call).

## 9. Fixtures

`build-tiered-mount-pool-fixture.ts` needs: two groups SHARING a store (to
pin first-claim), a project whose official store is NOT linked, a disabled
store in each tier, an off-roster character. The wardrobe / Inventory route
fixtures need a project and a group with NO store (the #135 empty answer — and
a row asserting the store table is UNTOUCHED after the GET). The Pascal route
fixture needs presets at two tiers with the same name (`shadows`), a preset
with an illegal name (filtered), and an unreadable store (`unread`).

## 10. Rulings owed to the human

- **R-A — the tier-walk keystone.** Port `reach.ts` whole as `db/reach.rs` and
  make `roster` / `enabled` required enum fields with NO `Default` on
  `TierResolveOptions` (v4's "every caller states which it means" — the
  compiler enforces it). Recommended: yes, as a stage-0 lane owned by ONE
  surveyor's plan, consumed by this area, scenarios (#136), the context builder
  and the scriptorium search.
- **R-B — `GET /api/v1/wardrobe/transfers`.** Recommended: REMOVE in this round
  (route, `WardrobeTransferDestinations`, the degraded arm, the three oracle
  rows), with S1 moving the dialog onto a container catalogue the same round.
  Alternative (keep as a v5-only route) is a divergence with no user value.
- **R-C — shared factories.** Recommended: port v4's item-route preamble and
  images factory as ONE Rust module each (SRP / DRY, and the log texts are
  stated once as in v4). Alternative: keep per-kind modules and match bytes —
  cheaper now, two copies to drift later.
- **R-D — duplicate's draft twin.** Recommended: a server-side Rust
  `wardrobe/item_draft.rs` (tier-1 vs v4's client-safe `item-draft.ts`), the SPA
  keeping its own for the editor. Note the copy can 400 (Zod) on an item that
  fails today's create schema — port that faithfully.
- **R-E — `Refused a inventory write`.** Recommended: port verbatim (v4-faithful);
  a v4 GitHub issue is optional — draft only with the human's go-ahead.
- **R-F — raw owner reads.** `resolveOwnerStore` reads projects / groups RAW
  where v5's `wardrobe_location.rs:469-489` / `inventory/location.rs:443-451`
  read hydrated. Recommended: follow v4 (raw), and measure one corrupt-row
  case each way in the reach fixture before the lane claims equality.
- **Deliberate divergence to confirm:** none proposed.

## 11. Dogfood rows (on a Friday copy, after the round)

Open a store-less project's and group's wardrobe (list + Dressing
instructions) and confirm NO `Project Files:` / `Group Files:` store appears;
Duplicate a wardrobe item in a group store (title `(copy)` against that tier);
transfer an item found in a LINKED project store; two browser windows on one
wardrobe, edit in one, the other refreshes (the `wardrobe` topic); a Pascal
run dialog with presets in the vault AND a group store of the same name (the
shadows note, "Save into" the group store, reload); the Salon avatar opens full
size and carried items show under the name.
