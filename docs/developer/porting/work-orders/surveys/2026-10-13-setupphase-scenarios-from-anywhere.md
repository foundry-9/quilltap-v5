# Survey — SCENARIOS FROM ANYWHERE (`545e299bb`): the location, the browser, source tracking — server, SPA, help

Read-only planning survey, **2026-10-13**. v4 = `~/source/quilltap-server`
read ONLY through `git show 64743ee13:<path>` (**the PIN**), `git diff
4a38222e6 64743ee13 -- <paths>` / `git show 545e299bb -- <paths>` (the span)
and `git show 4a38222e6:<path>` (the PRE shape v5 ported — line numbers marked
`(pre)`). **Every other v4 line number is AT THE PIN `64743ee13`.** Baseline
(what v5 has ported): **`4a38222e6`**. v5 = `main` at **`a7c6f0d03`**. Nothing
here is taken from commit prose (ledger §5.3); every claim is from shipped
hunks. The drift ledger's §1 (hazard (a), Schema / D23) and its §3 row for
`545e299bb` (`drift-ledger.md:594`) were read first and are built on, not
restated — **§0.1 lists the four places this survey CORRECTS that row.**

Scope (this slice): `lib/scenarios/**`, `lib/mount-index/scenario*` +
`group-knowledge.ts`, the three DELETED tier modules, every scenario route
(`app/api/v1/scenarios/**`, `projects|groups/[id]/scenarios/**`,
`groups/scenarios`), chat create's scenario arm, `chats/[id]?action=scenario` +
`?action=scenario-source`, `chats.scenarioSource` / `scenarioNotes`
behaviour + the `add-chat-scenario-source-v1` migration (behaviour and its
backfill — the DDL itself is the schema keystone survey's), the startup heals
+ project/group create hooks, the carriers (`.qtap` reconcile, new-account
restore remap, export/backup via the row), the SPA (`components/scenario/**`,
New Chat, the in-chat control), `help/` + `docs/v4/`. **Out of scope:** the
memory programme (it shares `fresh_schema.json`, the `chats` row, the `.qtap`
chat carrier, `remap_chat`, `help/chats.md` and the export schema — §7
names every meeting point), bug 228's reminted message ids (`8050958be`, its
own row; it shares `quilltap_import/reconcile.rs` + `entities.rs::create_chat`
— §7.4).

---

## 0. The span for this slice, in one paragraph

ONE commit carries the whole slice: **`545e299bb`** (2026-10-12, `#86`, a
squashed PR of four commits — the feature; the control's review fixes; "heal
folders on item writes, honest heal counts, one row read per character ref";
"save keeps the chat's scene; character location is read-only"). It replaces
the three per-tier scenario modules (`general-` / `project-` /
`group-scenarios.ts`) and `resolveStoreScenarioTier` with ONE chokepoint,
`lib/scenarios/location.ts` (a location = one owner's `Scenarios/` folder:
a character vault [READ-ONLY], a project's / group's OFFICIAL store, or
Quilltap General; a `ScenarioRef { mountPointId, path }` names any scenario
anywhere); adds an instance-wide catalogue (`GET /scenarios?action=everywhere`,
excerpts only) and a by-ref read (`?action=read`); makes the chat remember
the RECIPE of its scene (`scenarioText` + NEW `scenarioSource {mountPointId,
path, bodySha256}` + NEW `scenarioNotes`, written together by exactly two
callers), with a new `?action=scenario-source` GET and a `reload` flag that
distinguishes "Reload from source" from an ordinary save (which now KEEPS the
chat's stored preset when its own source file was revised); carries the
source through `.qtap` import (reconcile remap) and new-account restore
(`UuidRemapper`); rewires the startup heals (General / projects / groups) and
the project/group create hooks through the location (a folder failure now
RETHROWS; counts split `provisioned` / `healed` / `failed`); and on the SPA
side adds the `ScenarioBrowserDialog`, a "Browse all scenarios…" entry +
"Elsewhere" optgroup in `ScenarioSelect` (which now ALWAYS renders), a
provenance line / Reload / notes box in the in-chat control, and ONE
`scenarioSelection` in New Chat's form state seeded character > project >
general (fixing "shows the project default, submits the character's"). The
help tree gains `help/scenario-browser.md`. **No later commit in
`545e299bb..64743ee13` touches the slice's behaviour** — measured:
`git diff 545e299bb 64743ee13` over every slice path moves only
`app/api/v1/chats/route.ts:32` (an import path, `memory-recap` →
`memory/recall/recap`, memory Phase E) and `lib/schemas/chat.types.ts`
(memory Phase D deletes `otherExtractionWatermarkMessageId` from both chat
schemas + two comment paths — the memory slice's). `help/chats.md` moves again
in `93c5af6ad` (memory). `migrations/scripts/index.ts` at the pin registers
`addMemorySupersededAtMigration` (`c2795733f`) BETWEEN `addMemoryTiersMigration`
and `addChatScenarioSourceMigration` (`index.ts:880-883`).

### 0.1 Corrections to the ledger's §3 row (`drift-ledger.md:594`)

1. **"a character location update carries `defaultScenarioId` across a
   retitle and refuses a retitle onto an existing file"** — true of the
   squash's SECOND commit only; the FOURTH ("character location is read-only")
   deleted those writes. At the pin `characterLocation` (`location.ts:324-357`)
   has NO `create`/`update`/`rename`/`delete`/`setDefault`; writes exist only
   on `StoreScenarioLocation` (`:125-132`). Port nothing for a character write.
2. **"INFO `[Chats v1] Scenario changed` gains `source`"** — `source` already
   existed (pre `actions/scenario.ts:134-142`). It gains `textChanged`
   (second key), the `'ref'` arm (first in the cascade), and
   `sourceMountPointId` (last key) — `actions/scenario.ts:193-211`.
3. **"Reddens: the general / project / group scenario routes families (the
   modules are GONE)"** — the ROUTE files survive (thin configs); the
   `scenarios-routes.test.ts` oracle imports only route files +
   `database-store` / `folder-paths` / `instance-settings`, so it RUNS at the
   pin and its bodies are predicted largely GREEN (§5.1). What is
   IMPORT-DEAD is `harness/oracle/cases/scenario-resolvers.ts` and the
   `build-groups-projects-fixture.ts` BUILDER (§5).
4. **"`quilltap-host/src/host.rs` (the startup heals)"** — v5 ports ONLY
   General's heal (`services/builtin_mounts.rs:462-480` +
   `host.rs:1927-1940`). v4's PHASE 3.4 / 3.4b project + group heals
   (`ensure-project-scenarios.ts`, `ensure-group-scenarios.ts`,
   `backfill-group-stores.ts`) are a **standing tracked deferral** since P4.4
   (`work-orders/p4.4-route-backfill-unlock-chat-creation.md:114-117`); grep
   finds no v5 twin. The "honest-count" fix lands in code v5 never ported —
   §9 OP-1.
5. (Brief) `help/chat-participants.md` is NOT touched by `545e299bb` — only by
   `124c8aca3` / `ac6348c5f` (memory). The scenario help set is `chats`,
   `general-scenarios`, `groups`, `project-scenarios` + NEW `scenario-browser`.

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `lib/schemas/chat.types.ts` (schemas; client-safe)

```ts
// :795
export const ScenarioSourceSchema = z.object({
  mountPointId: UUIDSchema,                 // UUIDSchema = z.uuid() (common.types.ts:52)
  path: z.string().min(1).max(500),         // `Scenarios/<file>.md`
  bodySha256: z.string().length(64),        // hex SHA-256 of the body as resolved (frontmatter stripped, before notes)
});
// :810
export const ScenarioRefSchema = z.object({ mountPointId: UUIDSchema, path: z.string().min(1).max(500) });
```
`ChatMetadataSchema` `:926` / `:929` and `ChatMetadataBaseSchema` `:1319` /
`:1322`: `scenarioSource: ScenarioSourceSchema.nullable().optional()`,
`scenarioNotes: z.string().nullable().optional()` — immediately after
`scenarioText` (`:919` / `:1312`). A ZodObject field persists as JSON TEXT;
generateDDL renders `"scenarioSource" TEXT DEFAULT NULL, "scenarioNotes" TEXT
DEFAULT NULL` after `"scenarioText" TEXT` (keystone's D23 re-dump). Because the
repository validates rows with the schema, a chat CREATE (incl. `.qtap`
import's `repos.chats.create`) carrying a malformed `scenarioSource` FAILS
with a ZodError (§9 OP-9).

### 1.2 `migrations/scripts/add-chat-scenario-source-v1.ts` (100 lines) — behaviour

- `id 'add-chat-scenario-source-v1'`, `introducedInVersion '4.10.0'`,
  `dependsOn ['add-chat-scenario-text-field-v1']`, description `Add
  scenarioSource and scenarioNotes to chats (where each chat's scene was
  found)`.
- `shouldRun`: false unless SQLite backend AND `chats` exists; true when
  EITHER column is missing (`:37-50`).
- `run`: `addColumnIfMissing('chats','scenarioSource','TEXT DEFAULT NULL')`,
  then `('chats','scenarioNotes','TEXT DEFAULT NULL')` (`:57-62`; each a DEBUG
  `Adding column {context:'migrations.database-utils', table, column}` from
  `database-utils.ts:353-357` then `ALTER TABLE "chats" ADD COLUMN "<c>"
  TEXT DEFAULT NULL`). **NO BACKFILL** — a pre-existing chat reads `source:
  null`, and the SPA infers the preset by body as before.
- INFO `Added the scenario source columns to chats` `{context:
  'migration.add-chat-scenario-source', columnsAdded, durationMs}`; result
  message `Added ${n} scenario source column(s) to chats table`. Catch: ERROR
  `Failed to add the scenario source columns` `{context, error}` + result
  `{success:false, message:'Failed to add the scenario source columns', error}`.
- Registered `index.ts:426` (import) / `:882` (array — after
  `addMemorySupersededAtMigration` `:880`, before
  `addEpisodeExtractionWatermarkMigration` `:884`) / `:1320` (export).
  `lib/startup/prettify.ts` label `"Noting where each chat's scene was found"`.
- On an EXISTING instance the columns are APPENDED (physical order differs
  from a fresh dump) — harmless to v5's name-keyed column lists.

### 1.3 `lib/scenarios/location.ts` (566 lines, NEW) — THE chokepoint

Exports / types:
- `ScenarioScopeSchema = z.enum(['character','project','group','general'])`
  (`:62`); `StoreScenarioScope = Exclude<ScenarioScope,'character'>` (`:100`).
- re-exports `ScenarioRefSchema`, `ScenarioRef` from chat.types (`:66`).
- `GENERAL_OWNER_NAME = 'Quilltap General'` (`:73`).
- Errors (`:76-97`): `ScenarioConflictError(filename)` → message `A scenario
  named "${filename}" already exists`; `ScenarioNotFoundError(path)` →
  `Scenario not found: ${path}`; `ScenarioFilenameError(message)`.
- `interface ScenarioLocation { scope; id: string|null; mountPointId;
  ownerName; list(opts?: {includeArchived?}): Promise<ListScenariosResult>;
  read(path): Promise<ParsedScenario|null>; idForPath(path): string|null;
  parse(doc, defaultScenarioId?) }` (`:108-122`).
  `interface StoreScenarioLocation extends ScenarioLocation { scope:
  StoreScenarioScope; create(input); update(path, patch); rename(path,
  filename); delete(path); setDefault(path|null) }` (`:125-132`).
- `characterScenarioIdForPath(mountPointId, path)` =
  `stableUuidFromString(\`scenario:${mountPointId}:${path}\`)` (`:157-159`) —
  byte-identical to the vault parser's id (`parsers.ts:672`).
- `parseScenarioForScope(scope, doc, {characterId?, defaultScenarioId?})`
  (`:166-189`): non-character → `parseScenarioDoc(doc)`; character →
  `parseScenarioFile(doc, characterId ?? '')` mapped to a `ParsedScenario`
  IN THIS KEY ORDER: `path: doc.relativePath, filename:
  doc.fileName.replace(/\.md$/i,''), name: parsed.title, description?
  (conditional spread), isDefault, rawIsDefault` (both = `!!defaultScenarioId
  && parsed.id === defaultScenarioId`), `archived: parsed.archived === true,
  body: parsed.content, lastModified, createdAt, updatedAt`.
- `normalizeScenarioPath(path)` = `resolveScenarioPath(path, 'Scenarios')`
  (`:192-196`; NB `decodeURIComponent` FIRST, then trim — §1.6).
- Private `requirePath` throws `ScenarioNotFoundError(path)` on a bad path;
  `cleanFilename(filename, emptyMessage)` = `sanitizeFileName(f).replace(/\.md$/i,'')`,
  empty → `ScenarioFilenameError(emptyMessage)`.

`storeLocation(scope, id, mountPointId, ownerName, repos)` (`:214-318`):
- `list` → `findManyByMountPointsInFolder([mp], 'Scenarios', '.md')` →
  `buildScenarioList(docs, parseScenarioDoc, opts, {scope, id, mountPointId})`.
- `read(path)` → `findByMountPointAndPath(mp, requirePath(path))` →
  `parseScenarioDoc` or null. (Archived IS returned — `read` ignores the flag.)
- `create(input)`: `cleanFilename(input.filename, 'Filename cannot be empty
  after sanitisation')` → path `Scenarios/<cleaned>.md` → existing doc →
  `ScenarioConflictError(cleaned)` → `writeDatabaseDocument(mp, path,
  buildScenarioFileContent({name, description, isDefault, archived, body}))` →
  `if (isDefault) setScenarioDefaultInFolder(mp, path, 'Scenarios')` → DEBUG
  `[ScenarioLocation] Created scenario {scope, id, mountPointId, path}` →
  `read(path)`; **null → throws `ScenarioNotFoundError(path)`** (e.g. a
  whitespace-only body that passed `min(1)` — §3 Δ12).
- `update(path, patch)`: requirePath → existing (else NotFound) → write with
  `archived: patch.archived ?? isScenarioContentArchived(existing.content)` →
  `if (patch.isDefault) setDefault` → DEBUG `[ScenarioLocation] Updated
  scenario {scope, id, mountPointId, path}` → `read`, null → NotFound.
- `rename(path, filename)`: requirePath → `cleanFilename(filename, 'newFilename
  cannot be empty after sanitisation')` → `newPath`; **only when `newPath !==
  resolved`**: source missing → NotFound; target exists → Conflict;
  `moveDatabaseDocument`; DEBUG `[ScenarioLocation] Renamed scenario {scope,
  id, mountPointId, from, to}`. Then ALWAYS `read(newPath)`, null → NotFound
  (so a same-name rename of a MISSING file is now a 404 — §3 Δ11).
- `delete(path)`: `deleteDatabaseDocument` false → NotFound; DEBUG
  `[ScenarioLocation] Deleted scenario {scope, id, mountPointId, path}`.
- `setDefault(path|null)` → `setScenarioDefaultInFolder(mp, null|requirePath(path), 'Scenarios')`.
- `idForPath()` → null; `parse(doc)` → `parseScenarioDoc(doc)`.

`characterLocation(character, mountPointId, repos)` (`:324-357`) — READ-ONLY:
`defaultScenarioId = character.defaultScenarioId ?? null` (from the row the
location was built from); `list` = same folder query, `buildScenarioList(docs,
doc => parse(doc), opts, {scope:'character', id, mountPointId})`; `read` =
`findByMountPointAndPath(mp, requirePath(path))` → parse; `idForPath(path)` =
`characterScenarioIdForPath(mp, path)`; `parse(doc, defaultId =
defaultScenarioId)`.

`resolveScenarioLocation(scope, id, repos, userId, opts={})` (`:369-401`,
overloaded: store scope → `StoreScenarioLocation|null`): runs `resolve` then
**DEBUG `[ScenarioLocation] Resolved scenario location` `{scope, id: id ??
null, ensure, found, mountPointId: location?.mountPointId ?? null, context:
'scenarios'}`** on EVERY call.

`ensureScenariosFolder(mp)` (`:408-418`): `ensureFolderPath(mp,
'Scenarios')`; catch → **WARN `[ScenarioLocation] Failed to ensure Scenarios
folder` `{mountPointId, error: message}` then RETHROW** (the "heal" fix — the
pre General helper swallowed).

`resolve(scope, id, repos, userId, ensure)` (`:420-462`):
1. `general`: `getGeneralMountPointId()`; null → null; `ensure` →
   `ensureScenariosFolder`; → `storeLocation('general', null, mp, 'Quilltap
   General', repos)`.
2. `!id` → null.
3. `character`: `usableCharacter(await repos.characters.findByIdRaw(id),
   userId)` — null when missing, `userId` mismatch, `archivedAt` truthy, or no
   `characterDocumentMountPointId` (`:150-154`); → `characterLocation`.
   (`ensure` is IGNORED for characters.)
4. project/group: `findByIdRaw(id)`; null → null; `name = owner.name ||
   ('Project'|'Group')`; `ensure` → `ensureOwnerOfficialStore(scope,
   owner.id, name)` (null → null), `mp = ensured.mountPointId`,
   `ensureScenariosFolder(mp)`; else `mp = owner.officialMountPointId ?? null`;
   no mp → null; → `storeLocation(scope, owner.id, mp, name, repos)`.

`classifyScenarioMount(mp, repos, userId)` → `{scope, id}|null` (`:465-472`)
over private `classify` (`:475-504`), ORDER: (1) `mp ===
getGeneralMountPointId()` → general; (2)
`characters.findByDocumentMountPointIdRaw(mp)` — found but other user /
archived → DEBUG `[ScenarioLocation] Mount belongs to an unavailable
character` `{mountPointId, characterId, archived}` → null; else
`{character, id, character row}`; (3)
`projects.findByOfficialMountPointIdRaw`; (4)
`groups.findByOfficialMountPointIdRaw`; else null. **A linked (non-official)
store or an arbitrary Scriptorium store classifies as nothing.**

`type ScenarioRefLookup = {status:'ok', location, scenario} | {status:
'missing', location, path} | {status:'refused', reason}` (`:507-510`).
`lookupScenarioRef(ref, repos, userId)` (`:517-565`):
1. `normalizeScenarioPath(ref.path)` not ok → WARN `[ScenarioLocation]
   Refused scenario ref: bad path` `{mountPointId, path, reason}` → refused
   with the path error as reason (BEFORE any store read).
2. `classify` null → WARN `[ScenarioLocation] Refused scenario ref: mount is
   not a scenario owner` `{mountPointId, path: normalized}` → refused `That
   store does not hold scenarios`.
3. character → `characterLocation` built from THE CLASSIFIED ROW (one row
   read per ref); else `resolveScenarioLocation(scope, id, repos, userId)`
   (a probe — no ensure; emits the Resolved DEBUG). null → WARN
   `[ScenarioLocation] Refused scenario ref: owner did not resolve` `{mountPointId,
   scope, id}` → refused `That store does not hold scenarios`.
4. `location.read(normalized.path)` null → DEBUG `[ScenarioLocation]
   Scenario ref points at a missing file` `{mountPointId, path}` → missing;
   else ok.

### 1.4 `lib/scenarios/everywhere.ts` (188 lines, NEW)

- `SCENARIO_EXCERPT_LENGTH = 240` (`:36`).
- `interface EverywhereScenario { path; scenarioId?; filename; name;
  description?; isDefault; archived; excerpt; updatedAt }`,
  `interface EverywhereSource { scope; id; name; mountPointId; scenarios;
  warnings }`.
- `scenarioExcerpt(body)` (`:70-75`): `body.replace(/\s+/g,' ').trim()`; if
  `length > 240` → `${collapsed.slice(0,240).trimEnd()}…` else collapsed.
  (JS `\s` = the Unicode whitespace class incl. NBSP / U+FEFF / U+2028;
  `slice` counts UTF-16 units and can split a surrogate pair — §9 OP-7.)
- `byName = (a,b) => a.name.localeCompare(b.name, undefined, {sensitivity:'base'})` (`:77-78`).
- `collectOwners(repos, userId)` (`:81-127`): General first (when the pointer
  exists; name `Quilltap General`), then `Promise.all([projects.findAllRaw(),
  groups.findAllRaw(), characters.findAllRaw()])`; projects then groups: rows
  with a truthy `officialMountPointId`, `name || 'Project'|'Group'`, sorted
  byName (stable); characters: `userId === userId && !archivedAt &&
  characterDocumentMountPointId`, `defaultScenarioId ?? null`, sorted byName.
  **Projects/groups are NOT user-filtered.** Owners without a store are
  skipped; nothing is provisioned.
- `listScenariosEverywhere(repos, userId, {includeArchived?})`
  (`:133-188`): mount ids deduped (`new Set`), ONE
  `findManyByMountPointsInFolder(mountIds, 'Scenarios', '.md')`, bucketed by
  `doc.mountPointId`; DEBUG `[ScenariosEverywhere] Listed scenario shelves`
  `{owners, mounts, documents, includeArchived: === true}`; per owner
  `buildScenarioList(bucket ?? [], doc => parseScenarioForScope(scope, doc,
  {characterId: id ?? undefined, defaultScenarioId}), options, {scope, id,
  mountPointId})`; row = `{path, ...(character && {scenarioId:
  characterScenarioIdForPath(mp, s.path)}), filename, name, ...(description
  !== undefined && {description}), isDefault, archived, excerpt:
  scenarioExcerpt(s.body), updatedAt}` (KEY ORDER pinned). Owners with an
  EMPTY folder are included.

### 1.5 `lib/scenarios/source-remap.ts` (48 lines, NEW, client-safe)

- `readScenarioSource(value: unknown): ScenarioSource|null` (`:21-33`): `null
  | undefined | ''` → null; a string → `JSON.parse` (throw → null); then
  `ScenarioSourceSchema.safeParse` → data or null.
- `remapScenarioSource(value, mapMount: (id) => string|null)` (`:40-48`):
  read; null → null; `mapMount(source.mountPointId)` falsy → **null (the
  source is DROPPED)**; else `{...source, mountPointId}` (key order kept).

### 1.6 `lib/mount-index/scenarios-common.ts` (350 lines; was 419 pre)

Survives with: `SCENARIOS_FOLDER = 'Scenarios'` (NEW export `:48`);
`createScenarioSchema` (`:57-65` — `filename` 1..100, `name` 1..200 opt,
`description` ≤500 opt, `isDefault` opt, `archived` opt, `body`
`min(1,'Scenario body cannot be empty')`), `updateScenarioSchema` (`:68-76`,
no filename), `renameScenarioSchema` (`:79-81`, `newFilename` 1..100);
`ParsedScenario` (`:83-103`), `ListScenariosResult`; `isScenarioContentArchived`;
`parseScenarioDoc` (`:134-177`, WARN `Scenario file body is empty; skipping`
`{mountPointId, relativePath}` on an empty body); **NEW pure
`buildScenarioList(docs, parse, options={}, logContext={})`** (`:189-238`) —
`[...docs].sort((a,b) => a.relativePath.localeCompare(b.relativePath))`, parse,
drop archived unless asked, archived never wins, first `rawIsDefault` wins, the
warning `Multiple scenarios mark themselves as default; using "${winner}".
These are also marked default but are being ignored: ${"o1", "o2"}.` and WARN
`Scenarios: multiple isDefault: true entries` **`{...logContext, winner,
offenders}`** (pre `{mountPointId, folderName, winner, offenders}`);
`setScenarioDefaultInFolder(mp, defaultPath: string|null, folder)`
(`:249-291` — NULL now CLEARS: skips step 1, demotes every `isDefault: true`;
not-found throw `Cannot set default — scenario not found: ${path}`; demote
failure WARN `Failed to demote a previously-default scenario; alphabetical
tiebreaker will compensate` `{mountPointId, folderName, relativePath, error}`);
`buildScenarioFileContent` (`:297-322`); `resolveScenarioPath` (`:328-350`:
`decodeURIComponent(scenarioPath).trim()`; empty → `scenarioPath cannot be
empty`; `..`/`//` → `Invalid scenarioPath`; prefix `Scenarios/`; append `.md`
(`/\.md$/i`); nested → `Scenarios cannot live in nested folders`).
**DELETED from it:** `listScenariosInFolder`, `readScenarioByPath`,
`resolveScenarioBody` (§2).

### 1.7 `lib/mount-index/group-knowledge.ts` (22 lines, NEW)

`GROUP_KNOWLEDGE_FOLDER = 'Knowledge'`; `ensureGroupKnowledgeFolder(mp)` →
`{folderId: await ensureFolderPath(mp, 'Knowledge')}` (moved out of the
deleted `group-scenarios.ts`; no catch — a throw propagates).

### 1.8 `lib/mount-index/scenario-item-route-factory.ts` (388 lines; was 321)

`ScenarioItemRouteConfig { scope: 'project'|'group'|'general'; ownerLabel:
'Group'|'Project'|'General'; logTag; logIdKey; afterEnsure?(mp) }`
(`:45-57`). `owner = ownerLabel.toLowerCase()`.

`scenarioErrorResponse(error)` (`:65-74`): `ScenarioNotFoundError` →
`notFound('Scenario')` (404 `Scenario not found`); `ScenarioConflictError` |
`ScenarioFilenameError` → `badRequest(error.message)`; `z.ZodError` →
`badRequest(\`Invalid request body: ${issues.map(i=>i.message).join('; ')}\`)`;
else null.

**Item handlers** `createScenarioItemHandlers<P>(config)` (`:79-285`):
- `loadLocation(ownerId, ctx, forWrite=false)` (`:91-116`): probe
  `resolveScenarioLocation(scope, ownerId, repos, user.id)`; if found AND
  `forWrite` → re-resolve with `{ensure:true}` (heals `Scenarios/`; may
  rethrow) then `afterEnsure(mp)`; found → ok. Not found: General →
  `notFound('Quilltap General mount has not been provisioned yet')` (body
  `"... yet not found"`); else `findByIdRaw(ownerId)` → missing →
  `notFound(ownerLabel)`; present → `notFound('${ownerLabel} has no official
  document store yet — restart the server or call GET /scenarios first')`.
  **A write never mints a store; a GET never heals.**
- GET (`:122-146`): `resolveScenarioPath` → 400; loadLocation(probe);
  `read` null → 404 `Scenario`; `{scenario}`. catch → ERROR `${logTag} Failed
  to read ${owner} scenario` `{[logIdKey]: id ?? null, scenarioPath}` → 500
  `Failed to read ${owner} scenario`.
- PUT (`:152-192`): path 400 → loadLocation(**forWrite**) →
  `readIncludeArchived(req)` → `updateScenarioSchema.parse(await req.json())`
  → `location.update` → `list({includeArchived})` → INFO `${logTag} Updated
  ${owner} scenario` `{[logIdKey]: id ?? null, userId, mountPointId,
  relativePath, isDefault: === true}` → `{scenarios, warnings}`. catch:
  mapped or ERROR `${logTag} Failed to update ${owner} scenario` → 500.
- POST `?action=rename` (`:198-240`): `dispatchAction(req, {rename})` (missing
  / unknown action → v4's 400s); inside: path 400 → loadLocation(forWrite) →
  `renameScenarioSchema.parse` → `location.rename` → list → INFO `${logTag}
  Renamed ${owner} scenario` `{[logIdKey], userId, mountPointId, from, to}`
  (**now also on a same-name rename**) → `{path: renamed.path, scenarios,
  warnings}`. catch: mapped or ERROR `${logTag} Failed to rename ${owner}
  scenario` → 500.
- DELETE (`:246-282`): `includeArchived` → path 400 → loadLocation(forWrite)
  → `location.delete` → list → INFO `${logTag} Deleted ${owner} scenario`
  `{[logIdKey], userId, mountPointId, relativePath}` → `{scenarios, warnings}`.
  catch: mapped (NotFound → 404) or ERROR `${logTag} Failed to delete ${owner}
  scenario` → 500.

**Collection bodies** `createScenarioCollectionBodies<P>(config)`
(`:303-388`) — NEW (General's collection is now the same code):
- `ensureLocation(ownerId, ctx, forWrite)` (`:309-334`): non-General with id →
  `findByIdRaw` missing → `notFound(ownerLabel)`; then
  `resolveScenarioLocation(..., {ensure:true})`; null: General → write:
  `badRequest('Quilltap General mount has not been provisioned yet — restart
  the server')`, read: `NextResponse.json({mountPointId: null, scenarios: [],
  warnings: []})`; non-General → `serverError('Failed to ensure ${owner}
  document store')`; then `afterEnsure(mp)`.
- `list` (`:336-352`): includeArchived → ensureLocation(read) → `{mountPointId,
  scenarios, warnings}`; catch → ERROR `${logTag} Failed to list ${owner}
  scenarios` `{[logIdKey]: id ?? null}` → 500 `Failed to list ${owner}
  scenarios`.
- `create` (`:354-385`): **`createScenarioSchema.parse(await req.json())`
  FIRST**, then ensureLocation(write) → `location.create` → INFO `${logTag}
  Created ${owner} scenario` `{[logIdKey]: id ?? null, userId, mountPointId,
  relativePath: scenario.path, isDefault: === true}` → `list({includeArchived:
  validated.archived === true})` (the d25dacc1 quirk kept) → 201
  `{mountPointId, path, scenarios, warnings}`. catch: mapped or ERROR
  `${logTag} Failed to create ${owner} scenario` `{[logIdKey]}` → 500.

### 1.9 The route files (all thin configs)

| Route | pin file (lines) | config |
|---|---|---|
| `GET/POST /api/v1/scenarios` | `scenarios/route.ts` (74) | collection, `scope 'general'`, `ownerLabel 'General'`, `logTag '[General v1]'`, `logIdKey 'ownerId'`; GET = `withCollectionActionDispatch({everywhere, read}, list)` (`:71-73`) |
| `/api/v1/scenarios/[scenarioPath]` | (30) | item, same labels |
| `/api/v1/projects/[id]/scenarios` | (33) | collection, `'Project'`, `'[Projects v1]'`, `'projectId'` |
| `/api/v1/projects/[id]/scenarios/[scenarioPath]` | (31) | item, same |
| `/api/v1/groups/[id]/scenarios` | (36) | collection, `'Group'`, `'[Groups v1]'`, `'groupId'`, `afterEnsure: ensureGroupKnowledgeFolder` |
| `/api/v1/groups/[id]/scenarios/[scenarioPath]` | (36) | item, same + `afterEnsure` |
| `GET /api/v1/groups/scenarios?characterIds=` | (99) | the participant union |

`scenarios/route.ts`:
- `handleEverywhere` (`:47-55`): `listScenariosEverywhere(repos, user.id,
  {includeArchived: readIncludeArchived(req)})` → `{sources}`; catch → ERROR
  `[General v1] Failed to list scenarios everywhere` `{}` → 500 `Failed to
  list scenarios`.
- `handleRead` (`:57-69`): query `mountPointId`, `path`; either falsy → 400
  `mountPointId and path are required`; `lookupScenarioRef` → refused → 400
  **`Invalid scenario ref: ${reason}`** (NB the spelling differs from the
  create/action refusal `Invalid scenarioRef: …`); missing → 404 `Scenario`
  (`Scenario not found`); ok → `{scenario, owner: {scope, id, name:
  ownerName, mountPointId}}`. **No try/catch** (a throw → middleware 500).
- Unknown `?action=` on GET → v4's `unknownActionResponse` 400 (pre: ignored,
  listed). Bare `?action=` likewise. (Not reachable through v5's typed
  dispatch — no port.)
- POST = `create(req, ctx, {})`.

`groups/scenarios/route.ts` (`:28-99`): `characterIds` split/trim/drop-empty;
none → `{groupScenarios: []}`; each id kept only when
`repos.characters.findById(id)` (hydrated, user-scoped) is truthy; memberships
→ distinct group ids (insertion order); per group: `resolveScenarioLocation('group',
gid, repos, user.id, {ensure:true})` (DEBUG line each), `!location?.id` → skip,
`list({includeArchived})`, empty → skip, push `{groupId: location.id,
groupName: location.ownerName, mountPointId, scenarios, warnings}`; catch →
WARN `[Groups v1] Failed to load scenarios for a group; skipping` `{groupId,
error}`; sort `a.groupName.localeCompare(b.groupName)` (default sensitivity).
**Δ vs pre:** `groupName` is now `name || 'Group'`; the folder ensure RETHROWS
into that catch (a group with a broken folder is skipped with the WARN, was
listed); NO `Knowledge/` ensure on this route (pre had none either).

`projects/route.ts:85-99` and `groups/route.ts:100-115` (create hooks): after
the INFO `[Projects v1] Project created` / `[Groups v1] Group created`, when
the row has an `officialMountPointId`: `resolveScenarioLocation(scope, id,
repos, user.id, {ensure:true})` (+ groups: `ensureGroupKnowledgeFolder(mp)`);
catch → WARN `[Projects v1] Failed to ensure project Scenarios folder on
create` `{projectId, error}` / `[Groups v1] Failed to ensure group
Scenarios/Knowledge folders on create` `{groupId, error}`. A folder failure now
emits TWO WARNs (the location's, then the route's).

### 1.10 `lib/chat/scenario-selection.ts` (232 lines; was 182)

- `ScenarioSelectionFields { scenario?; scenarioRef?: ScenarioRef|null;
  scenarioId?; projectScenarioPath?; groupScenarioPath?;
  groupScenarioGroupId?; generalScenarioPath? }` (`:50-64`).
- `ResolveScenarioSelectionOptions { repos; userId; projectId?; characterId?;
  logTag? }` (`:66-75`) — **`character` (a hydrated row) → `characterId`;
  `userId` NEW and required.**
- `ResolvedScenario { scenarioText: string|null; scenarioSource:
  ScenarioSource|null; scenarioNotes: string|null }` (`:77-81`).
- `class ScenarioRefRefusedError` message **`Invalid scenarioRef: ${reason}`**
  (`:84-89`).
- `hashScenarioBody(body)` = `createHash('sha256').update(body,'utf8').digest('hex')` (`:92-94`).
- `sameResolvedScenario(a,b)` (`:97-104`): text `===`, notes `===`, and source
  identical-by-reference OR both non-null with equal `mountPointId`, `path`,
  `bodySha256`.
- `readFromScope(scope, ownerId, rawPath, options, field)` (`:115-146`),
  logTag default `[Scenario]`:
  - non-General without owner → WARN `${logTag} ${field} provided without its
    owner; ignoring` `{[field]: rawPath}`;
  - bad path → WARN `${logTag} ${field} is not a scenario path; ignoring`
    `{[field]: rawPath, reason}` (NEW arm);
  - location null → WARN `${logTag} ${field} provided but its ${scope} has no
    scenario store` `{ownerId: ownerId ?? null, [field]: rawPath}`;
  - read null → WARN `${logTag} ${field} did not resolve to a body`
    `{ownerId: ownerId ?? null, [field]: rawPath}`.
- `readCharacterScenarioById(scenarioId, options)` (`:149-167`):
  `characterId` → `resolveScenarioLocation('character', characterId, repos,
  userId)` → `list({includeArchived: true})` → find `idForPath(s.path) ===
  scenarioId`; miss (incl. no characterId / location null) → WARN `${logTag}
  scenarioId not found on character` `{characterId: options.characterId ??
  null, scenarioId}`.
- `resolveScenarioSelection(fields, options)` (`:173-232`): precedence
  `scenarioRef` (lookup; refused → THROW `ScenarioRefRefusedError(reason)`;
  missing → WARN `${logTag} scenarioRef points at a missing file`
  `{...fields.scenarioRef}` → continue) > `scenarioId` > `projectScenarioPath`
  (owner = `projectId`) > `groupScenarioPath` (owner = `groupScenarioGroupId`)
  > `generalScenarioPath`; each guard is JS-truthy (`''` = absent). `notes =
  fields.scenario?.trim() ? trimmed : null`. No preset → `{scenarioText:
  notes, scenarioSource: null, scenarioNotes: null}` (**notes are NOT stored
  under a custom scene**). Preset → `{scenarioText: combineScenarioText(body,
  notes) ?? null, scenarioSource: {mountPointId: location.mountPointId, path:
  scenario.path, bodySha256: hashScenarioBody(body)}, scenarioNotes: notes}`
  + DEBUG `${logTag} Resolved scenario selection` `{scope, ownerId:
  location.id, mountPointId, path, hasNotes}`.

### 1.11 `lib/chat/scenario-text.ts` (42 lines)

`combineScenarioText` unchanged (`:17-25`). NEW `recoverPresetBody(scenarioText,
notes)` (`:33-42`): falsy text → null; `extra = notes?.trim()`; none → the
text; else `text.endsWith('\n\n'+extra) ? text.slice(0, -suffix.length) :
null`.

### 1.12 `POST /api/v1/chats` (`app/api/v1/chats/route.ts`)

- `createChatSchema` gains `scenarioRef: ScenarioRefSchema.optional()`
  (`:114`, between `scenarioId` `:107` and `projectScenarioPath` `:121` — the
  Zod issue order; `.optional()` NOT nullable → an explicit `null` is a 400).
- `handleCreate` (`:1180`): parse → progress `Assembling the cast…` →
  continuation / autonomous gates → `buildAllParticipants` → `primaryCharacter
  = repos.characters.findById(first)` → **`resolveScenarioSelection(fields
  incl. scenarioRef, {repos, userId: user.id, projectId, characterId:
  primaryCharacter?.id ?? null, logTag: '[Chats v1]'})`** (`:1230-1253`);
  `ScenarioRefRefusedError` → `badRequest(error.message)` (400 `Invalid
  scenarioRef: <reason>` — AFTER the Green Room's first status was emitted;
  nothing is written yet). Consumers: `buildChatContext(...,
  scenarioText ?? undefined, ...)` (`:1263`), the row `scenarioText /
  scenarioSource / scenarioNotes` (`:1341-1343`), `outfitContext.scenarioText
  ?? undefined` (`:1381`), `createInitialMessagesScenarioAndStaff(...,
  scenarioText, ...)` (`:1486`).

### 1.13 `chats/[id]` — `?action=scenario` and `?action=scenario-source`

`schemas.ts:45-58` `setScenarioSchema` key order: `scenario: z.string().nullish()`,
**`scenarioRef: ScenarioRefSchema.nullish()`**, **`reload:
z.boolean().optional()`** (NOT nullable — `reload: null` is a 400),
`scenarioId: z.uuid().nullish()`, `projectScenarioPath / groupScenarioPath:
z.string().max(500).nullish()`, `groupScenarioGroupId: z.uuid().nullish()`,
`generalScenarioPath: z.string().max(500).nullish()`.

`actions/scenario.ts` (266 lines):
- `findCharacterOwningScenario(chat, scenarioId, repos)` (`:50-72`) now
  RETURNS the character id (still walks `CHARACTER` seats with a
  `characterId`, `status !== 'removed'`, through the HYDRATED
  `repos.characters.findById` — `character.scenarios.some(s => s.id ===
  scenarioId)`; a throw → DEBUG `[Chats v1] Skipping character while
  resolving scenarioId` `{chatId, characterId, error}`).
- **`keepCurrentScene(next, chat)`** (exported, `:83-96`): null unless the
  chat has a source AND `next` has a source with the SAME `mountPointId` +
  `path` but a DIFFERENT `bodySha256`; `body = recoverPresetBody(chat.scenarioText,
  chat.scenarioNotes)`; null or `hashScenarioBody(body) !== current.bodySha256`
  → null; else `{scenarioText: combineScenarioText(body, next.scenarioNotes)
  ?? null, scenarioSource: current, scenarioNotes: next.scenarioNotes}`.
- `handleSetScenario` (`:101-218`): parse → chat 404 (v5's composite order
  puts the 404 first, unchanged) → `characterId` → resolve (refused → 400
  `Invalid scenarioRef: …`) → **`if (!reload)`**: kept → DEBUG `[Chats v1]
  Source file revised; keeping the chat's scene (no reload asked)` `{chatId,
  mountPointId, path}`, `next = kept` → `previous = {chat.scenarioText ??
  null, chat.scenarioSource ?? null, chat.scenarioNotes ?? null}` →
  `sameResolvedScenario(next, previous)` → DEBUG `[Chats v1] Scenario
  unchanged; nothing to do` `{chatId}` + `{scenarioText, changed: false,
  message: 'Scenario unchanged'}` → `repos.chats.update(chatId,
  {scenarioText, scenarioSource, scenarioNotes})` (null → 500 `Failed to update
  chat`) → **`textChanged = next.scenarioText !== previous.scenarioText`; ONLY
  when true**: `compileAllIdentityStacks` (catch → WARN `[Chats v1] Failed to
  recompile identity stacks after scenario change` `{chatId, error}`) and
  `postHostScenarioRevisionAnnouncement({chatId, scenarioText})` → INFO
  `[Chats v1] Scenario changed` `{chatId, textChanged, hadScenario,
  hasScenario, scenarioLength, source: 'ref'|'character'|'project'|'group'|
  'general'|'custom', sourceMountPointId: next.scenarioSource?.mountPointId
  ?? null}` → `{scenarioText, changed: true, message: text ? 'Scenario
  updated' : 'Scenario cleared'}`. A provenance-or-notes-only change is
  WRITTEN, answers `changed: true`, but neither recompiles nor announces.
- `handleGetScenarioSource(chatId, ctx)` (`:226-266`): chat 404 → `Chat`;
  `source = chat.scenarioSource ?? null`, `notes = chat.scenarioNotes ??
  null`; no source → `{source: null, owner: null, scenarioName: null,
  scenarioId: null, exists: false, changed: false, notes}`; else
  `lookupScenarioRef(source, ...)` (passes the whole source; its WARN lines fire
  on every GET of a refused source); `location = refused ? null :
  lookup.location`; `owner = location ? {scope, id, name: ownerName} : null`;
  `exists = status === 'ok'`; `changed = ok && hashScenarioBody(body) !==
  source.bodySha256`; DEBUG `[Chats v1] Scenario source checked` `{chatId,
  mountPointId, path, status, changed}`; body KEY ORDER `{source, owner,
  scenarioName: ok ? scenario.name : null, scenarioId:
  location?.idForPath(source.path) ?? null, exists, changed, notes}` (a
  `missing` character source still answers its `scenarioId`).
- Wiring: `actions/index.ts:61` exports `handleGetScenarioSource`;
  `handlers/get.ts:82` `'scenario-source': () => handleGetScenarioSource(chatId,
  ctx)`; route header docs.
- `chats/[id]` GET body (`handlers/get.ts:386`) still projects ONLY
  `scenarioText` — source/notes come from `?action=scenario-source`.
- `updateChatSchema` (`schemas.ts:12-35`) still exposes NONE of the three
  keys. The ONLY writers of the recipe are create and `?action=scenario`.

### 1.14 Startup heals (`lib/startup/ensure-{project,group}-scenarios.ts`, `instrumentation.ts`)

- `ensureProjectScenariosForAllProjects()` (87 lines) / `ensureGroupScenarios
  ForAllGroups()` (90): `findAllRaw()` (throw → ERROR
  `ensure-project-scenarios: failed to list projects` / `ensure-group-scenarios:
  failed to list groups` `{error}` → `{total:0, provisioned:0, healed:0,
  failed:1}`); per row `resolveScenarioLocation(scope, id, repos,
  SINGLE_USER_ID, {ensure:true})`; null → `failed++`; `!row.officialMountPointId`
  → `provisioned++`; else pointer `!==` location mp → `healed++`; groups then
  `ensureGroupKnowledgeFolder(mp)`; catch → `failed++` + WARN
  `ensure-project-scenarios: failed for project; continuing` `{projectId,
  projectName, error}` / `ensure-group-scenarios: failed for group;
  continuing` `{groupId, groupName, error}`; INFO `ensure-project-scenarios:
  complete` / `ensure-group-scenarios: complete` `{total, provisioned, healed,
  failed}` (pre `{total, healed, created, failed}` — `created` came from
  `ensureOwnerOfficialStore`'s `created` flag; a STALE-pointer recreate now
  counts `healed`, a null-pointer ADOPTION now counts `provisioned`).
- `instrumentation.ts:765-774` / `:794-804` (unchanged callers; WARN `Error
  ensuring project scenario infrastructure, continuing startup` / `Error
  ensuring group scenario infrastructure, continuing startup`
  `{context:'instrumentation.register', error}`), `:786-793`
  `backfillGroupStores` (unchanged).
- `instrumentation.ts:809-819` General: `resolveScenarioLocation('general',
  null, getRepositories(), SINGLE_USER_ID, {ensure:true})`; catch → WARN
  `Error ensuring general scenarios folder, continuing startup`
  `{context:'instrumentation.register', error}`. **A folder failure now logs
  `[ScenarioLocation] Failed to ensure Scenarios folder` THEN this WARN** (pre:
  `[GeneralScenarios] Failed to ensure Scenarios folder` and nothing else —
  the residual WARN was dead code; P4.134 ported exactly that).

### 1.15 Repositories (raw finders, NEW)

- `characters.findByDocumentMountPointIdRaw(mp)` (`characters.repository.ts:
  ~145`) = `findOneByFilter({characterDocumentMountPointId: mp})` — no overlay.
- `AbstractStoreBackedRepository.findByOfficialMountPointIdRaw(mp)`
  (`store-backed.repository.ts:~106`) = `findOneByFilter({officialMountPointId:
  mp})` — projects + groups share it.

### 1.16 Carriers

- **`.qtap` export**: whole chat row (schema-driven) — both keys exported.
  `public/schemas/qtap-export.schema.json` chats gains `scenarioSource`
  (`type ["object","null"]`, `required [mountPointId, path, bodySha256]`,
  `mountPointId {string, format uuid}`, `path {string, maxLength 500}`,
  `bodySha256 {string, pattern "^[0-9a-f]{64}$"}`, description verbatim from
  the hunk) + `scenarioNotes` (`["string","null"]`) after `scenarioText`. The
  file moves again in `c2795733f` / `ac6348c5f` / `64743ee13` (memory) —
  vendor ONCE at the pin.
- **`.qtap` import**: `importChats` spreads the bundle chat into
  `repos.chats.create` (`import-entities.ts:404-429`) — the source rides in
  verbatim and is VALIDATED by `ChatMetadataSchema`; then
  `reconcileRelationships` (`reconcile.ts:360-458`, the chats loop reads the
  CREATED row via `repos.chats.findById(newId)`): `if (chat.scenarioSource)`:
  `mapped = idMaps.mountPoints.get(src.mountPointId) ?? (await
  getRepositories().docMountPoints.findById(src.mountPointId) ?
  src.mountPointId : null)`; `remapped = remapScenarioSource(src, () =>
  mapped)`; when `!remapped || remapped.mountPointId !== src.mountPointId` →
  `updates.scenarioSource = remapped`, `hasUpdates = true`, DEBUG `Remapped
  chat scenario source` `{chatId: newId, fromMountPointId, toMountPointId:
  remapped?.mountPointId ?? null}` on the CHILD logger `{module:
  'import:quilltap-import-service'}`. Ordering: AFTER the `projectId` remap,
  before the one `repos.chats.update`.
- **New-account restore** (`lib/backup/restore/uuid-remap.ts:320-324`): after
  the chat's `remapFields`/`remapArrayFields` + `userId` spread, `if
  (chat.scenarioSource) remappedChat.scenarioSource =
  remapScenarioSource(chat.scenarioSource, id => remapper.remap(id))` —
  `remapper.remap` MINTS for an unseen id, so the source is never dropped
  (only an INVALID one becomes null). Chats are remapped AFTER characters
  (vault ids already minted) but BEFORE projects / groups / docMountPoints
  (`:298` vs `:381` / `:395` / `:547`) — a project/group/General source MINTS
  that mount's new id at the chat step, shifting every later counter id.
  Replace-mode restore preserves ids — nothing to do. (`:311-313` also carry
  memory's two NEW watermark keys at the pin — the memory slice's.)
- **Backup**: whole-table; nothing in v4.
- SillyTavern export: not represented.

### 1.17 Client lib (`lib/query/keys.ts`, `lib/realtime/topic-map.ts`)

`queryKeys.chats.scenarioSource(id) = ['chats', id, 'scenario-source']`,
`queryKeys.scenarios.everywhere(includeArchived=false) = ['scenarios',
'everywhere', {includeArchived}]`, `queryKeys.scenarios.ref(mp, path) =
['scenarios','ref', mp, path]`; topic `chats` (row-scoped) also invalidates
`chats.scenarioSource(id)` (the scenario change posts the Host announcement,
which publishes `chats`).

### 1.18 Every log line in the slice, verbatim (level)

| # | Level | Message | Metadata (key order) | Site |
|---|---|---|---|---|
| L1 | DEBUG | `[ScenarioLocation] Resolved scenario location` | scope, id, ensure, found, mountPointId, context:'scenarios' | location.ts:392 |
| L2 | WARN | `[ScenarioLocation] Failed to ensure Scenarios folder` | mountPointId, error | :412 |
| L3 | DEBUG | `[ScenarioLocation] Created scenario` | scope, id, mountPointId, path | :260 |
| L4 | DEBUG | `[ScenarioLocation] Updated scenario` | scope, id, mountPointId, path | :283 |
| L5 | DEBUG | `[ScenarioLocation] Renamed scenario` | scope, id, mountPointId, from, to | :296 |
| L6 | DEBUG | `[ScenarioLocation] Deleted scenario` | scope, id, mountPointId, path | :306 |
| L7 | DEBUG | `[ScenarioLocation] Mount belongs to an unavailable character` | mountPointId, characterId, archived | :487 |
| L8 | WARN | `[ScenarioLocation] Refused scenario ref: bad path` | mountPointId, path, reason | :524 |
| L9 | WARN | `[ScenarioLocation] Refused scenario ref: mount is not a scenario owner` | mountPointId, path | :534 |
| L10 | WARN | `[ScenarioLocation] Refused scenario ref: owner did not resolve` | mountPointId, scope, id | :548 |
| L11 | DEBUG | `[ScenarioLocation] Scenario ref points at a missing file` | mountPointId, path | :558 |
| L12 | DEBUG | `[ScenariosEverywhere] Listed scenario shelves` | owners, mounts, documents, includeArchived | everywhere.ts:149 |
| L13 | WARN | `Scenario file body is empty; skipping` | mountPointId, relativePath | scenarios-common.ts:155 |
| L14 | WARN | `Scenarios: multiple isDefault: true entries` | ...logContext(scope,id,mountPointId), winner, offenders | :230 |
| L15 | WARN | `Failed to demote a previously-default scenario; alphabetical tiebreaker will compensate` | mountPointId, folderName, relativePath, error | :281 |
| L16 | INFO | `${logTag} Updated/Renamed/Deleted/Created ${owner} scenario` | [logIdKey] (null for General), userId, mountPointId, … | factory :172/:218/:263/:364 |
| L17 | ERROR | `${logTag} Failed to read/update/rename/delete ${owner} scenario` | [logIdKey], scenarioPath | factory :138/:184/:232/:274 |
| L18 | ERROR | `${logTag} Failed to list/create ${owner} scenarios`/`scenario` | [logIdKey] | factory :345/:378 |
| L19 | ERROR | `[General v1] Failed to list scenarios everywhere` | {} | scenarios/route.ts:52 |
| L20 | WARN | `[Groups v1] Failed to load scenarios for a group; skipping` | groupId, error | groups/scenarios/route.ts:87 |
| L21 | WARN | `${logTag} ${field} provided without its owner; ignoring` | [field] | scenario-selection.ts:124 |
| L22 | WARN | `${logTag} ${field} is not a scenario path; ignoring` | [field], reason | :129 |
| L23 | WARN | `${logTag} ${field} provided but its ${scope} has no scenario store` | ownerId, [field] | :134 |
| L24 | WARN | `${logTag} ${field} did not resolve to a body` | ownerId, [field] | :142 |
| L25 | WARN | `${logTag} scenarioId not found on character` | characterId, scenarioId | :162 |
| L26 | WARN | `${logTag} scenarioRef points at a missing file` | mountPointId, path | :187 |
| L27 | DEBUG | `${logTag} Resolved scenario selection` | scope, ownerId, mountPointId, path, hasNotes | :224 |
| L28 | DEBUG | `[Chats v1] Skipping character while resolving scenarioId` | chatId, characterId, error | actions/scenario.ts:64 |
| L29 | DEBUG | `[Chats v1] Source file revised; keeping the chat's scene (no reload asked)` | chatId, mountPointId, path | :135 |
| L30 | DEBUG | `[Chats v1] Scenario unchanged; nothing to do` | chatId | :153 |
| L31 | WARN | `[Chats v1] Failed to recompile identity stacks after scenario change` | chatId, error | :181 |
| L32 | INFO | `[Chats v1] Scenario changed` | chatId, textChanged, hadScenario, hasScenario, scenarioLength, source, sourceMountPointId | :193 |
| L33 | DEBUG | `[Chats v1] Scenario source checked` | chatId, mountPointId, path, status, changed | :247 |
| L34 | INFO | `ensure-project-scenarios: complete` / `ensure-group-scenarios: complete` | total, provisioned, healed, failed | ensure-*.ts:85/:88 |
| L35 | WARN | `ensure-project-scenarios: failed for project; continuing` / `…group; continuing` | projectId, projectName, error / groupId, groupName, error | :77/:80 |
| L36 | ERROR | `ensure-project-scenarios: failed to list projects` / `…groups` | error | :52/:54 |
| L37 | WARN | `Error ensuring general scenarios folder, continuing startup` | context:'instrumentation.register', error | instrumentation.ts:815 |
| L38 | WARN | `[Projects v1] Failed to ensure project Scenarios folder on create` / `[Groups v1] Failed to ensure group Scenarios/Knowledge folders on create` | projectId/groupId, error | projects/route.ts:95, groups/route.ts:111 |
| L39 | DEBUG | `Remapped chat scenario source` (child logger) | chatId, fromMountPointId, toMountPointId | reconcile.ts:437 |
| L40 | INFO / ERROR | `Added the scenario source columns to chats` / `Failed to add the scenario source columns` | context:'migration.add-chat-scenario-source', … | migration :66 / :84 |

### 1.19 Error strings (verbatim, status)

`Invalid scenarioRef: <reason>` (400, create + `?action=scenario`); `Invalid
scenario ref: <reason>` (400, `?action=read`); reasons: `scenarioPath cannot
be empty` / `Invalid scenarioPath` / `Scenarios cannot live in nested folders`
/ `That store does not hold scenarios`; `mountPointId and path are required`
(400); `Scenario not found` (404); `Chat not found` (404); `A scenario named
"<f>" already exists` (400); `Filename cannot be empty after sanitisation` /
`newFilename cannot be empty after sanitisation` (400); `Invalid request body:
<issues;…>` (400); `Quilltap General mount has not been provisioned yet not
found` (404, item) / `Quilltap General mount has not been provisioned yet —
restart the server` (400, create); `<Owner> has no official document store yet
— restart the server or call GET /scenarios first not found` (404); `<Owner>
not found` (404); `Failed to ensure <owner> document store` (500); `Failed to
list scenarios` (500, everywhere); `Failed to <verb> <owner> scenario(s)`
(500); `Failed to update chat` (500); `Validation error` (+details) for the
create/action Zod refusals (`scenarioRef` null on create, `reload` non-boolean
/ null, a malformed ref object).

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 deleted (pre) | v5 twin (file:line on `a7c6f0d03`) | fate |
|---|---|---|
| `lib/mount-index/general-scenarios.ts` (115): `GENERAL_SCENARIOS_FOLDER`, `ensureGeneralScenariosFolder` (swallowing WARN `[GeneralScenarios] Failed to ensure Scenarios folder`), `listGeneralScenarios`, `readGeneralScenario`, `resolveGeneralScenarioBody`, `setGeneralScenarioDefault` (throw `Quilltap General mount has not been provisioned yet`) | `db/scenarios.rs:108-123` `resolve_general_scenario_body`; `api/scenarios.rs:428-435` `general_mount_id` (+ best-effort ensure); `services/builtin_mounts.rs:462-480` `ensure_general_scenarios_folder` (the swallowing WARN, P4.134) + `host.rs:1927-1940` | DIE → the location's General arm; the boot heal becomes `resolve(general, ensure)` with the RETHROW + `[ScenarioLocation]` WARN, so `host.rs:1931-1937`'s residual WARN becomes LIVE |
| `lib/mount-index/project-scenarios.ts` (91): `ensureProjectScenariosFolder`, `listProjectScenarios`, `readProjectScenario`, `resolveProjectScenarioBody`, `setProjectScenarioDefault` | `db/scenarios.rs:86-93` `resolve_project_scenario_body`; `api/projects.rs:1518-1560` `ensure_project_scenarios_store` / `load_project_scenarios_store` | DIE → location |
| `lib/mount-index/group-scenarios.ts` (104): + `ensureGroupKnowledgeFolder` (moved to `group-knowledge.ts`) | `db/scenarios.rs:95-104`; `api/groups.rs:607-650` (two ensure helpers + `GROUP_KNOWLEDGE_FOLDER`) | DIE → location + `afterEnsure` |
| `resolveStoreScenarioTier` (`scenario-selection.ts:69-113` pre) + its WARNs `${pathField} provided without ${ownerIdField}; ignoring` / `… provided but ${ownerLabel} has no officialMountPointId` | `services/scenario_selection.rs:113-204` (the three tier arms + their `tracing::warn!`s) | REWRITE (§3 Δ20-25) |
| `scenarios-common.ts` `listScenariosInFolder` (pre `:189`), `readScenarioByPath`, `resolveScenarioBody` | `db/scenarios.rs:288-362` `list_scenarios_in_folder`, `:364-381` `read_scenario_by_path`, `:54-84` `resolve_scenario_body` | `list` → pure `build_scenario_list(docs, parse, include_archived, log_ctx)`; the other two DIE |
| the pre `ScenarioItemRouteConfig` fields `scenariosFolder` / `findOwner` / `ensureFolders` / `listScenarios` / `readScenario` / `setScenarioDefault`, and the factory's no-op-rename short-circuit | `api/scenarios.rs:349-400` `rename_op` (the no-op arm `:365-373`) | REWRITE |
| `ScenarioSelect`'s `hasAnyScenarioOptions` (pre `ScenarioSelect.tsx:56-67`) | SPA `scenario/scenario-select.ts:35-55` + callers `chat/sidebar/chat-scenario-control.ts:21,266-273` + `screens/new-chat/new-chat-form.ts:26,268,561-568` | DIE (the dropdown always renders) |
| New Chat's four pointer fields (`scenarioId` / `projectScenarioPath` / `generalScenarioPath` / `groupScenarioPath` / `groupScenarioGroupId` in FORM STATE) | `screens/new-chat/new-chat.types.ts:126-151`, `new-chat.logic.ts:136-178` (`scenarioSelectionPatch`, `scenarioSelectPatch`), `:237-247` (submit chain), `new-chat.state.ts:388-420,482-484,621`, `new-chat-form.ts:553-622,741-742,807,814-815`, `character-picker-panel.ts:215,227` | REPLACE by one `scenarioSelection` + `scenarioTouched` (the WIRE still carries the four + `scenarioRef`) |
| the oracle `harness/oracle/cases/scenario-resolvers.ts` + `crates/quilltap-harness/tests/scenario_resolvers_equivalence.rs` (126) | — | IMPORT-DEAD (§5) — retire or re-aim |

---

## 3. The behaviour deltas a port must hit (numbered; v4 pin site → v5 site)

**Schema / row / ensure**
1. `chats.scenarioSource` (JSON TEXT) + `chats.scenarioNotes` (TEXT), both
   `DEFAULT NULL`, after `scenarioText` in the fresh dump — v4
   `chat.types.ts:926,929` → v5 `provisioning/fresh_schema.json:11` (KEYSTONE
   re-dump, D23).
2. `add-chat-scenario-source-v1` as an UNSTAMPED, table-gated boot ensure
   (both columns, independently, `TEXT DEFAULT NULL`, no backfill),
   differentially compared against v4's REAL migration (the
   `add-memory-tiers-v1` / P4.D249 shape) — v4 `migration :37-62` → v5 a new
   `db/chat_scenario_source_repair.rs` called from `host.rs` after the P4.D265
   block (`host.rs:2099-2121`) in v4's REGISTRATION order (after memory's
   `add-memory-superseded-at-v1` if that round lands it), and added to
   `restore_vintage_state`'s mirror-boot list (memory note) — **KEYSTONE**.
3. The row carries both keys: v5 `db/chats.rs:251` `ChatCreate` (+
   `scenario_source: Option<Value>`, `scenario_notes: Option<String>`), the
   INSERT `:856-940` (column list + params — the comment's "never tidy this
   sequence" rule), `ChatUpdate` `:519-536` (setters; `scenario_text`'s ⚠
   doc + the `chat_update_surface_cannot_reach_scenario_text` guard EXTEND to
   both new keys — the dispatch update surface must not grow them, v4
   `updateChatSchema` `schemas.ts:12-35`), `:1075` setter block;
   `db/chats_read.rs:79` `ALL_COLUMNS` + the row mapper (`:257` — every
   later index shifts by 2; JSON-parse `scenarioSource`) — **KEYSTONE** (the
   memory round edits the same three lists; §7.1).

**The location (server)**
4. ONE location module replaces the three tier modules + the per-family
   store resolvers — v4 `location.ts` → v5 NEW module (proposed
   `db/scenario_location.rs`, §7.3); `api/projects.rs:1518-1560`,
   `api/groups.rs:607-650`, `api/scenarios.rs:428-435` collapse onto it.
5. A character location is READ-ONLY and is a location at all only when the
   raw row is the caller's, not archived, and has a vault (`:150-154`,
   `:436-442`).
6. `classify` order General → character vault → project official → group
   official; linked / arbitrary stores refuse (`:475-504`). Needs two NEW raw
   finders (v5: `characters_read` by `characterDocumentMountPointId`;
   `db/projects.rs` / `db/groups.rs` by `officialMountPointId` — `LIMIT 1`, v4's
   `findOneByFilter` has no ORDER).
7. `lookupScenarioRef` → ok / missing / refused with L8-L11, reason strings
   verbatim; the character location is built from the classifying row
   (`:517-565`).
8. `ensure` semantics: a probe never provisions; `ensure:true` provisions a
   project/group store via `ensureOwnerOfficialStore` (v5
   `db/ensure_official_store.rs:200` `ensure_owner_official_store`) and heals
   `Scenarios/`, and a folder failure WARNs L2 then PROPAGATES (`:408-418`) —
   v5 today discards every folder-ensure error (`let _ =` at
   `api/projects.rs:1537-1538,1557-1558`, `api/groups.rs:624-626,646-648`,
   `api/scenarios.rs:421-423`, `api/groups.rs:319-320`).
9. L1 DEBUG on EVERY resolve (incl. each group in the union, each heal row,
   each create hook).
10. The collection's owner check is the RAW row (`findByIdRaw`) — v5
    `api/projects.rs:1528` uses the HYDRATED `repo.find_by_id` (overlay);
    `api/groups.rs:618` is already raw. Store name = `name || 'Project'|'Group'`
    (pre passed `project.name` straight through).

**Routes**
11. Item routes: GET never heals the folder (pre `loadOwnerAndStore` ensured
    on every verb — v5 `load_*_scenarios_store` `api/projects.rs:1557`,
    `api/groups.rs:646-648`, `general_mount_id` `api/scenarios.rs:432`);
    PUT/rename/DELETE heal via `ensure:true` + `afterEnsure` (group
    `Knowledge/`). A same-name rename of a MISSING file is now 404
    `Scenario` (pre 200 + list) and the INFO "Renamed" now fires on a same-name
    rename (v5 `api/scenarios.rs:365-373` short-circuits silently).
12. Writes re-read through `read`: a create/update/rename whose file parses
    to null (whitespace-only body passes `body.min(1)`) WRITES then answers
    404 `Scenario` (pre: 201/200 with a list that omits it) — v5
    `api/scenarios.rs:205-262` / `:290-334` / `:349-400`.
13. Collection POST parses the body BEFORE the owner check (pin
    `factory.ts:357-358`): an invalid body for a missing project/group is now
    400 (pre 404) — v5 `api/projects.rs:1581-1590` / `api/groups.rs:671-690`
    check the owner first.
14. Error mapping through `scenarioErrorResponse` (ZodError on DELETE/GET
    would now map too — unreachable).
15. INFO L16 / ERROR L17-L18 for ALL THREE families incl. General (`ownerId:
    null` first key; pre General logged `{userId, …}` / `{scenarioPath}`
    without it) — **v5 logs NONE of these today** (`grep tracing::
    api/scenarios.rs` is empty; the projects/groups arms likewise): port them.
16. `buildScenarioList` WARN L14 now spreads `{scope, id, mountPointId}`; v5
    `db/scenarios.rs:288-362` emits no WARN at all (and none for L13 —
    "the log is a no-op seam here", `:255`).
17. `setScenarioDefaultInFolder(null)` clears (store locations'
    `setDefault(null)`) — v5 `db/scenarios.rs:383-441` takes `&str`.
18. NEW `GET /scenarios?action=everywhere` (§1.4) and `?action=read` (§1.9)
    — v5 NEW dispatch variants (§7.2).
19. The union route: `groupName = name || 'Group'`; ensure:true per group
    with L1 + a rethrowing folder heal caught by L20 (v5
    `api/groups.rs:821-910` swallows silently and never logs L20 — a
    pre-existing gap the port should close).

**The recipe (resolver)**
20. `resolveScenarioSelection` returns `{scenarioText, scenarioSource,
    scenarioNotes}`; `scenarioRef` first; refused → typed error → 400 at both
    callers — v5 `services/scenario_selection.rs:91-217` returns
    `Option<String>`.
21. Options take `characterId` + `userId` (not a hydrated character): the
    character arm reads the vault through the location (`list({includeArchived:
    true})` + `idForPath`), so an ARCHIVED or other-user character's scenario
    NO LONGER resolves (pre: any hydrated `character.scenarios` hit) — v5
    `scenario_selection.rs:220-238` `find_scenario_content` over the
    hydrated row.
22. Every store kind resolves by `location.read` with `normalizeScenarioPath`
    (rejects `..` / `//` / nested; DECODES `%xx`) where pre's
    `resolveScenarioBody` did neither — v5 `db/scenarios.rs:54-84`.
23. The four WARN texts change (L21-L25) and two arms are NEW (L22, L26) +
    DEBUG L27; the General no-mount case now logs L23 (`generalScenarioPath
    provided but its general has no scenario store`), not "did not resolve".
24. Notes: `scenario?.trim()` → stored as `scenarioNotes` ONLY under a
    preset; a custom scene stores `scenarioNotes: null` and its text in
    `scenarioText`.
25. `hashScenarioBody` = lowercase hex SHA-256 of the UTF-8 body (the
    PARSED body — trimmed, frontmatter and, for a character file, the `#`
    heading line removed).

**Chat create**
26. `scenarioRef` in the create schema (`.optional()`, not nullable, Zod
    issue position after `scenarioId`) — v5 `services/chat_create.rs:841-864`
    (`validate_create_body`; the create path carries `details`, so nested
    issue bytes matter), `ChatCreateRequest` `:195-201` + `from_raw`
    `:284-289`.
27. The resolve call (`:1226-1250` v5) passes `characterId` + `userId`;
    `ScenarioRefRefused` → 400 `Invalid scenarioRef: <reason>`; the row writes
    all three keys (`:1455`); the other consumers keep reading only the text
    (`:1264`, `:1565`, `:1625`, `:1654`, `:1687`).

**`?action=scenario`**
28. Body gains `scenarioRef` (nullish object) and `reload` (`.optional()`
    boolean — null REJECTS) — v5 `api/types.rs:3481-3496` +
    `services/chat_scenario.rs:77-151` (raw `Value`s, P4.60 convention).
29. `keepCurrentScene` unless `reload` (§1.13) + L29.
30. The no-op compares the WHOLE recipe; a provenance/notes-only change is
    WRITTEN with `changed: true` but no recompile / no announcement —
    `verb_noop_same_general_preset` (a chat whose text equals the General
    body, no source) FLIPS to `changed: true` + a row write — v5
    `chat_scenario.rs:264-271` compares text only.
31. The write sets all three keys (v5 `:278-296` sets `scenario_text` only —
    hazard (a)).
32. L32 gains `textChanged`, `'ref'`, `sourceMountPointId` — v5 `:320-331`
    + `source_label` `:351-365`.
33. `findCharacterOwningScenario` returns the id (v5 `:154-202` returns the
    row — keep the HYDRATED walk; only the return type changes).

**`?action=scenario-source`**
34. NEW GET verb (§1.13), body key order pinned, L33, and lookup WARNs on a
    refused source — v5 NEW variant + handler (§7.2).

**Carriers**
35. `.qtap` import: ChatCreate carries the bundle's `scenarioSource` (Zod-valid
    or the chat fails — §9 OP-9) and `scenarioNotes`; reconcile remaps per
    §1.16 + L39 — v5 `services/quilltap_import/entities.rs:893-950`
    `create_chat` (typed `ChatCreate` today DROPS both keys) and
    `reconcile.rs:375-455` (after the `projectId` arm `:437-440`).
36. New-account restore: `remap_chat` remaps `scenarioSource` through the
    SAME remapper AFTER the chain + `userId` and BEFORE participants — v5
    `services/backup/uuid_remap.rs:815-870`; restore's typed decode
    `restore/orchestrator.rs:739` (`ChatCreate`) must carry both keys.
37. Backup / `.qtap` export carry both keys because they read
    `chats_read::find_all` (`services/backup/collect.rs:567`,
    `services/qtap_export/entities.rs:60`) — follows Δ3; today both DROP them
    (hazard (a), confirmed structurally).
38. `qtap-export.schema.json` — v5 vendored copy
    `crates/quilltap-core/src/generators/qtap-export.schema.json:540`.

**Startup / create hooks**
39. General boot heal → location `ensure` (L2 then L37) — v5
    `builtin_mounts.rs:462-480` + `host.rs:1927-1940`.
40. Project / group create hooks → location `ensure` (+ `Knowledge/`); a
    folder failure logs L2 then L38 — v5 `api/projects.rs:273-310`
    (P4.148 already logs the route WARN; L2 missing), `api/groups.rs:315-321`
    (logs NOTHING — `let _ =`).
41. (Deferred in v5) the PHASE 3.4 / 3.4b project + group heals with the
    `{total, provisioned, healed, failed}` counts — §9 OP-1.

**Vault order (P4.D279's OPEN item — APPLIES at the pin)**
42. v4 `read-overlay.ts:274-277` (Prompts) / `:305-308` (Scenarios) and
    `vault-readers.ts:262-265` / `:298-301` sort `relativePath.localeCompare`
    (UNCHANGED across the span — only `parsers.ts`'s comment moved); v5
    `db/vault_read_overlay.rs:381` / `:394` sort `a.relative_path.cmp(&b…)`
    (code units). NEW at the pin: the character LOCATION's `list` and the
    browser bucket also sort through `buildScenarioList`'s `localeCompare` —
    v5's `list_scenarios_in_folder` already uses `collation::locale_compare`
    (`db/scenarios.rs:304`), so `build_scenario_list` inherits it. Fix the
    two overlay sorts with `locale_compare` (a mixed-case `Scenarios/` flips
    `character.scenarios` order → the New Chat / control option order and,
    for Prompts, which prompt `normalize_prompt_defaults` promotes when none
    is marked default).

**SPA** — §8.

---

## 4. v5 on `main` today (`a7c6f0d03`) — per file

- `crates/quilltap-core/src/db/scenarios.rs` (695): `:1-130` header + the
  four `resolve_*_body` (DIE); `:132-158` `ParsedScenario` (KEEP — key order
  matches v4's); `:160-163` `ListScenariosResult`; `:168-200`
  `ScenarioWriteError` (KEEP; `DefaultNotFound` text matches); `:205-223`
  archived helpers (KEEP); `:225-279` `parse_scenario_doc` (KEEP; add L13);
  `:288-362` `list_scenarios_in_folder` → pure `build_scenario_list` (+ L14
  with log context); `:364-381` `read_scenario_by_path` (DIE → location
  `read`); `:383-441` `set_scenario_default_in_folder` (→ `Option<&str>`);
  `:443-479` `build_scenario_file_content` (KEEP); `:483-517`
  `resolve_scenario_path` (KEEP; header `:487-492` documents the deliberate
  NO-decode — §9 OP-6).
- `crates/quilltap-core/src/api/scenarios.rs` (658): `:62-186` bag
  validation (KEEP); `:188-193` `scenarios_value`; `:202-212` `list_body`;
  `:218-262` `create_op`, `:265-277` `get_op`, `:280-334` `update_op`,
  `:337-400` `rename_op`, `:403-414` `delete_op` — re-home onto a
  `StoreScenarioLocation` (Δ11-15); `:416-424` `resolve_path_or_400`,
  `ensure_scenarios_folder` (DIES — the location's ensure propagates);
  `:428-435` `general_mount_id`; `:459-565` six General handlers → the
  factory shape; `:567-658` P4.142 G2 fallback-read tests (KEEP — the
  location's reads must stay v4's fallbacks).
- `crates/quilltap-core/src/api/projects.rs` (2165): `:268-310` create hook
  (→ location ensure + L2); `:1512-1730` the six scenario handlers (thin
  configs over the shared factory).
- `crates/quilltap-core/src/api/groups.rs` (1135): `:310-321` create hook
  (→ location ensure + Knowledge + L38 WARN, today silent); `:600-814` the six
  scenario handlers; `:815-910` `group_scenarios_union` (Δ19; KEEP the
  `characters_read::find_by_id` user gate and the ICU sort).
- `crates/quilltap-core/src/services/scenario_selection.rs` (386): WHOLE
  rewrite (Δ20-25). The module header's "JS truthiness" notes stay true.
- `crates/quilltap-core/src/services/chat_scenario.rs` (410): `:77-99`
  `SetScenarioBody` (+ `scenario_ref`, `reload`), `:128-151` `validate`,
  `:154-202` `find_character_owning_scenario` (return the id), `:204-349`
  `chat_set_scenario` (Δ29-32), `:351-365` `source_label` (+ `'ref'`),
  `:367-376` `ok_body` (unchanged); + NEW `chat_get_scenario_source`.
- `crates/quilltap-core/src/scenario_text.rs` (29): + `recover_preset_body`.
- `crates/quilltap-core/src/services/chat_create.rs` (4916): `:195-201`,
  `:284-289`, `:841-864`, `:1226-1250`, `:1455` (Δ26-27).
- `crates/quilltap-core/src/db/chats.rs` (2208) / `db/chats_read.rs` (932):
  Δ3 (KEYSTONE).
- `crates/quilltap-core/src/services/quilltap_import/{entities.rs:893-950,
  reconcile.rs:375-455}`, `services/backup/uuid_remap.rs:815-870`,
  `services/backup/restore/orchestrator.rs:739`: Δ35-36.
- `crates/quilltap-core/src/services/builtin_mounts.rs:450-480` +
  `crates/quilltap-host/src/host.rs:1927-1940`: Δ39.
- `crates/quilltap-core/src/db/vault_read_overlay.rs:381,394`: Δ42.
- `crates/quilltap-core/src/api/types.rs` (6102): `:995-1040` (General family
  + union), `:3481-3496` (`ChatSetScenario`) + new variants (§7.2);
  `api/engine.rs:1637-1660`, `:2878-2930` (dispatch arms).
- `crates/quilltap-web/tests/dispatch_wrong_type_census.rs` (`:752-830` the
  scenario rows): every NEW typed field needs a row; the `note` strings cite
  v4 pre line numbers (`scenarios/route.ts:44`, `scenarios/[scenarioPath]/route.ts:106`,
  `:175,213`) that MOVE — re-cite at the pin.
- `crates/quilltap-core/src/generators/qtap-export.schema.json:540` (Δ38).
- `help/` (131 files), `docs/v4/` (§8.3).

---

## 5. Harness

### 5.1 Families + oracles in the slice, predicted at a HEAD (= pin) regen

| Family (Rust, lines) | Oracle (lines) | Tier | At the pin |
|---|---|---|---|
| `scenario_resolvers_equivalence.rs` (126) | `cases/scenario-resolvers.ts` (90) | 1/2 read | **IMPORT-DEAD** — `:58` `import('@/lib/mount-index/general-scenarios')` (`resolveGeneralScenarioBody`), `:59` `import('@/lib/mount-index/project-scenarios')` (`resolveProjectScenarioBody`). Retire, or re-aim at `lookupScenarioRef` / `resolveScenarioLocation(...).read` over the same `scenario-resolvers.json` fixture (adds the `..`/nested/`%`-decode rows pre never had). |
| `scenarios_routes_equivalence.rs` (1068, 66 cases: general_* 17, group_* 28, project_* 15, union_* 6) | `cases/scenarios-routes.test.ts` (694) | 2 | **RUNS** (imports the seven ROUTE files + `database-store` / `folder-paths` / `instance-settings` — all alive). Bodies predicted GREEN for the committed corpus (none exercises the Δ11-13 edges; the union's `groupName` only differs for an empty name). Fixture **builder** `fixtures/build-groups-projects-fixture.ts:305-308` is **IMPORT-DEAD** on a rebuild (`ensureGroupScenariosFolder` ← `group-scenarios`, `ensureProjectScenariosFolder` ← `project-scenarios`, `ensureGeneralScenariosFolder` ← `general-scenarios`; `buildScenarioFileContent` ← `scenarios-common` survives) — the committed `groups-projects-{main,mount}.db` stays usable; re-aim the builder only if rebuilt (`resolveScenarioLocation(scope, id, repos, uid, {ensure:true})`). |
| `chat_scenario_routes_equivalence.rs` (1172) | `cases/chat-scenario-routes.test.ts` (648) + `fixtures/chat-scenario-web.json` + committed `crates/quilltap-web/tests/fixtures/chat-scenario-{main,mount}.db` | 2 | **RED, by design and by fixture**: (a) the `resolver_*` cases call `resolveScenarioSelection(fields, {repos, projectId, character, logTag})` (`:391-399`) — `character` is ignored and `userId` missing at the pin, and the result is an OBJECT (`wasUndefined` always false) → every resolver row flips; re-aim with `{userId, characterId}` and emit the recipe. (b) **Every write case 500s in v4** until the committed `chat-scenario-main.db` WIDENS (v4's update writes `scenarioSource` / `scenarioNotes` → `no such column`) — widen through v4's REAL `add-chat-scenario-source-v1` (the "every committed `*-main.db` pair widens" rule, ledger Schema/D23) or rebuild (`build-chat-scenario-fixture.ts` uses `generateDDL` `:188`, so a rebuild picks the columns up). (c) `verb_noop_same_general_preset` flips to `changed: true` + a row write (Δ30). (d) the row dumps gain both keys. No log capture today — ADD it (L21-L33 are this family's lines). |
| `chat_create_capstone_equivalence.rs` (1500) | `cases/chat-create-capstone.test.ts` (490; fixtures built to `/tmp/qt-cc-*.db`) | 2/3 | RED (the created row gains both keys); ADD `scenarioRef` rows (§5.2). |
| `system_export_equivalence.rs` / `system_import_equivalence.rs` / `system_restore_equivalence.rs` / `system_backup_equivalence.rs` | `system-export.test.ts`, `system-import-execute.test.ts` (3673), `system-restore.test.ts` (1269) over `system-data-{main,mount,llmlogs}.db` | 2 | RED (every chat row / bundle gains two keys); fixture WIDENS first; ADD the reconcile rows. |
| `backup_uuid_remap_equivalence.rs` (412, tier 1, NO normalization) | `cases/backup-uuid-remap.test.ts` + `fixtures/uuid-remap-corpus.json` | 1 | GREEN unless a corpus chat carries a source — ADD rows (§5.2); the minting-order shift is exactly what its `getMapping()` comparison catches. |
| `provisioning_equivalence.rs` (1095) | live `generateDDL` (`QT_FRESH_SCHEMA_LIVE`) | 2 | RED — D23 tripwire (two columns); KEYSTONE. |
| `qtap_schema_embed_guard.rs` (141) + `public_schemas_vendor_guard.rs` | the vendored schema | guard | RED until re-vendored at the pin (shared with memory). |
| `help_tree_embed_guard.rs` (143; `VENDORED_FILE_COUNT = 131` `:92`) / `help_tree_equivalence.rs` (240) / host `host_help_docs_boot` | `help-tree-sync.test.ts` | guard / 2 | The COUNT at the pin is **131** (unchanged) but the SET differs: `+scenario-browser.md` (this slice), `+memory-maintenance.md`, `−memory-consolidation.md`, `−memory-housekeeping.md` (memory, `b045dd986`). A lane vendoring only its own page reads 132 and the memory lane 130 — the silent-auto-merge trap (`help_tree_embed_guard.rs:60-65`'s own history): vendor the WHOLE tree at the pin ONCE (§7.5). |
| `qtap_import_equivalence.rs` / `qtap-import.ts` (529) | — | 1 | Check whether it carries chat rows; the schema validator family `qtap_schema_validate_equivalence.rs` follows the schema re-vendor. |
| `scenario_builder_*` (mount pool, prompts, tier 3, request schema) | `scenario-builder-*` | — | UNTOUCHED (the Builder is unchanged at the pin). |
| `realtime_topics_equivalence.rs` (119) | `realtime-topics.ts` (job topics) | 1 | UNTOUCHED (the topic-map change is client-side). |

### 5.2 Corpus rows to ADD

- **location / lookup (NEW tier-2 family, proposed
  `scenario_location_equivalence.rs` + `cases/scenario-location.test.ts`,
  over the `groups-projects` fixture + a character with a vault + an archived
  character + a linked store)**: refused bad path (`Scenarios/../x.md`,
  nested `Scenarios/a/b.md`, empty); refused non-owner mount (a linked
  project store, a random Scriptorium store, an unknown uuid); refused
  archived character; ok for each of the four kinds; missing file; `%`-encoded
  path (`Scenarios/a%20b.md` → reads `Scenarios/a b.md`); malformed `%`
  (`Scenarios/%E0.md` — v4 THROWS `URIError`); a character ref's
  `scenarioId` = `characterScenarioIdForPath`; the character location's
  `isDefault` from the column; L1/L7-L11 captured (global capture — the
  writer-thread note).
- **everywhere** (`?action=everywhere` ± `includeArchived`): owner order
  (General, projects A–Z base, groups, characters; an empty-name project →
  `Project`; a case-only name tie — stable sort input order = v4's
  `findAllRaw` row order, so v5's raw finders must return the same order); an
  empty shelf included; an archived character omitted; a body > 240 UTF-16
  units with interior whitespace runs / NBSP / an emoji straddling unit 240
  (§9 OP-7); the multiple-default warning per bucket; `scenarioId` only on
  character rows.
- **`?action=read`**: missing params (each), refused (`Invalid scenario ref:
  …`), missing (404), ok for a character (owner `{scope:'character', id,
  name, mountPointId}`).
- **routes**: a whitespace-only body create (Δ12 → 404 after write); an
  invalid body for a missing project (Δ13 → 400); a same-name rename of a
  missing file (Δ11 → 404); a GET item against a store whose `Scenarios/`
  folder row is gone (no heal; write heals); the union with an empty group
  name; the INFO/ERROR lines L16-L18 captured.
- **resolver**: `scenarioRef` ok / missing (falls through to the next kind,
  L26) / refused (throws); an ARCHIVED character's `scenarioId` (Δ21 — no
  longer resolves); General unprovisioned (L23 text); a bad
  `projectScenarioPath` (`../x`, L22); notes under a preset vs a custom scene
  (Δ24); the hash of a heading-titled character file (Δ25).
- **`?action=scenario`**: `reload` absent / true / `null` (400) / `"yes"`
  (400); `scenarioRef: null` (accepted = absent); a kept scene (source file
  revised, no reload → text keeps the stored preset with the NEW notes,
  `scenarioSource` unchanged, L29); the same with `reload: true` (pulls the
  revision); a kept-scene attempt whose stored text no longer ends in its notes
  (falls through to the resolved recipe); a provenance-only change
  (`verb_noop_same_general_preset` re-labelled: written, `changed: true`, no
  announcement, no recompile — assert the transcript has NO new Host bubble);
  a notes-only change; L32's new keys.
- **`?action=scenario-source`**: no source; ok unchanged; ok changed; missing
  file (exists false, `scenarioId` still answered for a character); refused
  (owner null; L8/L9/L10 fire on the GET).
- **create**: `scenarioRef` ok / refused (400 after the Green Room's first
  status; nothing written) / `scenarioRef: null` (Validation error + details
  bytes) / a malformed ref (`mountPointId: "x"` → the nested uuid issue
  bytes); a character default (`characterId` = primary).
- **import / restore**: a bundle chat whose source mount came across
  (remapped + L39), one whose mount already exists here (kept, no update),
  one whose mount is absent (→ null + L39), a malformed `bodySha256` (the chat
  FAILS to import — §9 OP-9), a JSON-STRING source in a raw-row restore
  (`readScenarioSource`'s string arm); `uuid-remap-corpus.json`: a chat with a
  PROJECT-store source (mints at the chat step — counter shift), a
  character-vault source (already minted), General (minted at chats), an
  invalid source (→ `null` key written), `scenarioSource: null` (key untouched).
- **migration ensure** (keystone): both missing / one missing / both present /
  no `chats` table — byte-equal `table_info` + `sqlite_master` against v4's
  REAL migration, the P4.D249 shape.

### 5.3 Closest recipes to copy

`chat-scenario-routes.test.ts` (the action, with dumps + the transcript) for
the action / source cases; `scenarios-routes.test.ts` for the route families;
`chat-informs-remap.ts` + `backup-uuid-remap.test.ts` for the carriers; the
P4.D249 `chat_informs_permanent_repair` differential for the ensure; the
P4.D279 `wardrobe_location` family (a location chokepoint with tier-2 reads
and captured lines) for the location family.

---

## 6. v4 jest tests at the pin covering the slice (mirror these)

| Path | `it`/`test` count | Covers |
|---|---|---|
| `lib/scenarios/__tests__/location.test.ts` | 21 | resolve (null w/o id, never provisions on probe, provisions + heals on ensure, archived tombstone, other user), classify (archived vault, linked store), lookup (bad path before any read, nested, non-owner, missing ≠ refused, project by ref), character tier (one row read per ref, id = overlay id, default from column, read-only), store tier (conflict, archived preserved, clear default) |
| `lib/scenarios/__tests__/everywhere.test.ts` | 6 | owner order, excerpts, empty shelves |
| `lib/scenarios/__tests__/source-remap.test.ts` | 4 | string / object / invalid / dropped |
| `__tests__/unit/lib/mount-index/scenario-item-route-factory.test.ts` | 29 | path before store, 404 remedy w/o provisioning, GET never heals / writes heal + Knowledge, no store minted by a write, PUT/rename/DELETE arms, collection bodies |
| `__tests__/unit/lib/mount-index/scenarios-archived.test.ts` | 17 | archived semantics through the location |
| `__tests__/unit/lib/startup/ensure-scenario-heals.test.ts` | 1 (+table) | provisioned / healed / failed counts, folder failure counted |
| `__tests__/unit/lib/chat/scenario-selection.test.ts` | 20 | precedence, refs, recipe, notes, hash |
| `__tests__/unit/lib/chat/scenario-text.test.ts` | 11 | `recoverPresetBody` |
| `__tests__/unit/app/api/v1/chats/[id]/actions/scenario.test.ts` | 18 | keep/reload, recipe no-op, textChanged gating, scenario-source |
| `__tests__/unit/app/api/v1/chats/[id]/handlers/get.dispatch.test.ts` | 6 | `scenario-source` registered |
| `__tests__/unit/app/api/v1/chats/route.scenario-seed.test.ts` | 3 | create writes the recipe |
| `__tests__/unit/lib/import/quilltap-import/reconcile-scenario-source.test.ts` | 3 | the reconcile remap |
| `__tests__/unit/lib/backup/uuid-remapper.test.ts` | 59 (+29 lines) | the restore remap |
| `__tests__/unit/realtime/topic-map.test.ts` | 11 | `chats.scenarioSource` under `chats` |
| `__tests__/helpers/scenario-stores.ts` | helper | store fixtures |
| `components/scenario/__tests__/scenario-selection.test.tsx` | 22 | tokens (`store:` parse/label/`current`), payload, `ScenarioSelect` Browse/Elsewhere, browser helpers |
| `__tests__/unit/components/chat/chat-scenario-control.test.tsx` | 21 | provenance line, Reload, notes box, gone source not selectable, Custom→preset clears the box |
| `components/new-chat/__tests__/NewChatForm.test.tsx` | 30 | single selection, override note, browser |
| `components/new-chat/__tests__/useNewChat.request-body.test.tsx` | 11 (NEW) | what is sent = what is shown; seed order; continuation skip; touched pins |

---

## 7. Cross-lane meeting points

### 7.1 NEEDS from the round's schema KEYSTONE

- The D23 re-dump from the pin: `chats."scenarioSource" TEXT DEFAULT NULL,
  "scenarioNotes" TEXT DEFAULT NULL` after `"scenarioText" TEXT` — in the SAME
  dump that drops `chats."otherExtractionWatermarkMessageId"` and adds
  memory's columns (one `fresh_schema.json`).
- The boot ensure `add-chat-scenario-source-v1` (proposed
  `db::chat_scenario_source_repair::{MIGRATION_ID, ensure_chat_scenario_source_columns(main)
  -> Result<ScenarioSourceEnsureOutcome, DbError>}`, outcome `{columns_added:
  Vec<&'static str>}`, `is_noop()`), its `host.rs` call in v4 registration
  order, its `restore_vintage_state` mirror-boot entry, and the widening of
  every committed `*-main.db` through v4's real migration.
- The `chats` row: `ChatCreate { …, scenario_source: Option<Value>,
  scenario_notes: Option<String> }` (serde camelCase, after `scenario_text`),
  `ChatUpdate { scenario_source: Option<Option<Value>>, scenario_notes:
  Option<Option<String>> }` (doc'd as writable ONLY by
  `services::chat_scenario` + `chat_create`; the
  `chat_update_surface_cannot_reach_scenario_text` guard extended),
  `chats_read` reading both (`scenarioSource` JSON-parsed to an object;
  `null` when the cell is NULL). **The memory keystone edits the same
  `ALL_COLUMNS`, INSERT and mapper (dropping the watermark column) — ONE
  owner for `db/chats.rs` + `db/chats_read.rs` this round** (if the
  scenarios work is its own round, sequence it strictly before/after the
  memory keystone; never two lanes in those lists).

### 7.2 The wire contract — pin VERBATIM between the server and SPA lanes

`api/types.rs` (Rust) ⇄ `apps/web/src/app/core/core-contract.ts` (TS):

1. **NEW** `Request::ScenarioEverywhere { #[serde(default)] include_archived:
   bool }` ⇄ `{ type: 'scenarioEverywhere'; includeArchived?: boolean }` →
   `Response::Scenario(json!({ "sources": [...] }))`.
   `EverywhereSource { scope: 'general'|'project'|'group'|'character'; id:
   string|null; name: string; mountPointId: string; scenarios:
   EverywhereScenario[]; warnings: string[] }`, `EverywhereScenario { path;
   scenarioId?; filename; name; description?; isDefault; archived; excerpt;
   updatedAt }` (key order as listed). Census row: `include_archived` /
   `bool` / `V4::Query`.
2. **NEW** `Request::ScenarioReadByRef { #[serde(default)] mount_point_id:
   Option<String>, #[serde(default)] path: Option<String> }` ⇄ `{ type:
   'scenarioReadByRef'; mountPointId: string; path: string }` →
   `Response::Scenario(json!({ "scenario": ParsedScenario, "owner": { scope,
   id, name, mountPointId } }))`; absent/empty → 400 `mountPointId and path
   are required` (Option so an absent key reaches the handler, not serde).
   Census rows: `V4::Query` (a query string has no JSON type).
3. **NEW** `Request::ChatGetScenarioSource { chat_id: String }` ⇄ `{ type:
   'chatGetScenarioSource'; chatId: string }` → `Response::ChatAdmin(json!({
   source, owner, scenarioName, scenarioId, exists, changed, notes }))` —
   `ScenarioSourceInfo { source: ScenarioSource|null; owner: { scope; id:
   string|null; name } | null; scenarioName: string|null; scenarioId:
   string|null; exists: boolean; changed: boolean; notes: string|null }`
   (the `chatGetBackground` / `chatGetCost` family's naming).
4. **CHANGED** `Request::ChatSetScenario` + `#[serde(default)] scenario_ref:
   Option<Value>` and `#[serde(default)] reload: Option<Value>` (raw — P4.60;
   Zod floors in `services::chat_scenario`) ⇄ `ChatSetScenarioRequest` +
   `scenarioRef?: ScenarioRef | null; reload?: boolean`. Response unchanged
   (`ChatSetScenarioResult { scenarioText; changed; message }`).
5. **CHANGED** `chatCreate` (raw map) — `scenarioRef` read from the raw body;
   `ChatCreateRequest` (Rust) + `scenario_ref: Option<ScenarioRef>`; TS
   `ChatCreateRequest` + `scenarioRef?: ScenarioRef` (core-contract
   `:893-928`, update the precedence comment `:893-895` to put `scenarioRef`
   first).
6. Shared TS types: `ScenarioRef { mountPointId: string; path: string }`,
   `ScenarioSource { mountPointId: string; path: string; bodySha256: string }`.
7. NO change: `Scenario{List,Create,Get,Update,Rename,Delete}`,
   `{Project,Group}Scenario*`, `GroupScenariosUnion`, `CharacterScenario*` —
   same variants, same bodies (only server behaviour + logs move).

### 7.3 Proposed Rust spellings (server lane)

- `crate::db::scenario_location` — `ScenarioScope { Character, Project,
  Group, General }` (`as_str()` = v4's strings); `GENERAL_OWNER_NAME`;
  `enum ScenarioLocation { Store(StoreScenarioLocation),
  Character(CharacterScenarioLocation) }` with `scope()`, `id()`,
  `mount_point_id()`, `owner_name()`, `list(mount, include_archived)`,
  `read(mount, path)`, `id_for_path(path)`; `StoreScenarioLocation::{create,
  update, rename, delete, set_default}` over `(&Connection /*mount*/)`;
  `enum ScenarioLocationError { NotFound(String), Conflict(String),
  Filename(String), Db(DbError), Store(StoreError) }` (Display = v4's three
  messages); `resolve_scenario_location(main, mount, scope, id: Option<&str>,
  user_id, ensure: bool) -> Result<Option<ScenarioLocation>,
  ScenarioLocationError>`; `classify_scenario_mount(main, mount, mp, user_id)`;
  `enum ScenarioRefLookup { Ok { location, scenario }, Missing { location,
  path }, Refused(String) }`; `lookup_scenario_ref(main, mount, &ScenarioRef,
  user_id)`; `character_scenario_id_for_path(mp, path)` (=
  `vault_overlay::stable_uuid_from_string`, `vault_overlay.rs:54`);
  `parse_scenario_for_scope`; `normalize_scenario_path`. Writes run inside the
  writer closure (`with_both_conns`) as today.
- `crate::db::scenarios_everywhere::{SCENARIO_EXCERPT_LENGTH,
  scenario_excerpt, list_scenarios_everywhere(main, mount, user_id,
  include_archived)}` (uses `collation::locale_compare_base`).
- `crate::scenario_source::{ScenarioSource, ScenarioRef, read_scenario_source(&Value),
  remap_scenario_source(&Value, impl FnMut(&str) -> Option<String>)}` (pure;
  the restore + reconcile + chat code share it).
- `services::scenario_selection::{ResolvedScenario { scenario_text,
  scenario_source, scenario_notes }, ScenarioRefRefused(String) /* Display
  "Invalid scenarioRef: …" */, hash_scenario_body, same_resolved_scenario,
  resolve_scenario_selection(main, mount, &fields, &options) ->
  Result<ResolvedScenario, ScenarioSelectionError>}`; options gain `user_id`,
  `character_id: Option<&str>` (drop `character: Option<&Value>`); fields
  gain `scenario_ref: Option<&ScenarioRef>`.
- `scenario_text::recover_preset_body(text: Option<&str>, notes:
  Option<&str>) -> Option<String>`.
- `services::chat_scenario::{keep_current_scene, chat_get_scenario_source}`.

### 7.4 PROVIDES to / shares with siblings

- **Memory keystone / lanes:** `db/chats.rs` + `chats_read.rs` (§7.1);
  `services/backup/uuid_remap.rs::remap_chat` (memory adds
  `episodeExtractionWatermarkMessageId` + `turnExtractionWatermarkMessageId`
  to the chain list `:815-830`; this slice adds the source block after the
  chain) — ONE editor or a sequenced pair; the `.qtap` schema re-vendor
  (§1.16); `help/chats.md` (also `93c5af6ad`); `docs/v4/developer/{API,DDL}.md`
  + `CHANGELOG.md` (ten / ten / eighteen later commits — re-vendor whole at
  the pin by ONE lane).
- **Bug 228 (`8050958be`):** `quilltap_import/entities.rs::create_chat`
  (`:893-950`) and `reconcile.rs`'s chats loop — bug 228 reworks the create
  path (fresh-id remint, the phantom map); this slice adds two keys to the
  typed create and one remap block to the loop. Same lane, or 228 first.
- **SPA lane:** consumes §7.2 verbatim.

### 7.5 Help tree

Vendor `help/` WHOLE at the pin once (131 files; set change in §5.1) — by the
docs owner of whichever round lands last, or by the unifier; never a partial
per-lane vendor (the count guard cannot see a +1/−2+1 split).

---

## 8. SPA / help / docs / CLI touch points

### 8.1 SPA — v4 pin → v5 counterparts

- `components/scenario/types.ts` (244): + `STORE_SCENARIO_PREFIX = 'store:'`,
  `BROWSE_SCENARIO_VALUE = '__browse__'`, re-exported `ScenarioRef`, the
  `{kind:'store'; mountPointId; path; label}` selection; `scenarioSelectionToValue`
  `store:` arm; `scenarioValueToSelection(value, current?)` — browse → custom,
  `store:` with `colonIdx < 1` → custom, label kept from `current` when the
  token names it else `path.replace(/^Scenarios\//,'').replace(/\.md$/i,'')`;
  `scenarioSelectionToPayload` `store` → `{scenarioRef}`. (Its `ScenarioOption`
  / `toScenarioOption` / `scenarioOptionLabel` predate the span.) → v5
  `apps/web/src/app/scenario/scenario.types.ts` (187): `:98-109`, `:112-126`,
  `:133-154`, `:157-187`.
- `components/scenario/ScenarioSelect.tsx` (151): `hasAnyScenarioOptions`
  DELETED; `onBrowse?` prop; the browse entry is an ACTION (`:93-96` — calls
  `onBrowse`, emits no change; the controlled value snaps back); options:
  `Custom...` (`:103`), `Browse all scenarios…` only with `onBrowse` (`:104`),
  `<optgroup label="Elsewhere">` holding ONLY a current `store` selection
  (`:105-109`), then Project / General / groups / Character. → v5
  `scenario/scenario-select.ts` (225): `:35-55` die, template `:100-140`,
  a new `browse` output (an Angular `output<void>()`; when unbound, no entry —
  mirror "only with `onBrowse`" by an input flag, since an Angular output
  cannot be tested for a subscriber).
- **NEW** `components/scenario/ScenarioBrowserDialog.tsx` (399):
  `isInHierarchy` (`:81-94`: general always; project/group/character by id
  match), `pickToSelection` (`:100-119`: native kind when in hierarchy —
  character only with a `scenarioId` — else `store` labelled `${source.name} ›
  ${scenario.name}`), `useScenarioRefBody` (`:126-140`, `retry: false`,
  key `scenarios.ref`), `scenarioMatchesSearch` (`:143-149`, lowercased
  includes over name / description / excerpt / owner name); the dialog
  (`:151-399`): title **`Every scene on every shelf`**, `maxWidth 4xl`, search
  placeholder `Search every shelf…` (`aria-label "Search scenarios"`,
  autofocus), `Show archived` (refetches `everywhere(showArchived)`,
  `staleTime: 0`, enabled while open), error `The shelves could not be
  reached: ${message}`, loading `Dusting the shelves…`, nav (`aria-label
  "Scenario owners"`) `Every shelf` + sections General / Projects / Groups /
  Characters (collapsible `▸`/`▾`, `aria-expanded`), owner rows with count,
  greyed + title `Nothing filed here yet.` when empty, badge `in this chat`;
  results `role="listbox" aria-label="Scenarios"` (search spans every shelf
  regardless of the selected owner), rows `role="option"` with `default` /
  `archived` badges and the owner name when showing all; empty text `Nothing
  on that shelf answers to the name.` (searching) / `Nothing filed here yet.`;
  preview header `${owner} › ${name}`, the excerpt while loading, then the body
  as Markdown; ArrowUp/Down move the highlight (scroll into view), Enter =
  Use, double-click = pick; footer `Cancel` / `Use this scenario` (disabled
  w/o a highlight). → v5 NEW `scenario/scenario-browser-dialog.ts` (+ the
  three pure helpers in `scenario.types.ts` or a sibling `.logic.ts`); the
  Markdown preview through v5's existing markdown renderer.
- `components/chat/ChatScenarioControl.tsx` (514): `ScenarioSourceInfo`
  (`:61-69`); a `scenarioSource` query (`:181-186`, enabled with the section);
  `currentAsSelection` (`:221-273`) — a source with an owner that EXISTS is
  the authority (general → general; project only when `owner.id ===
  projectId`; group when `groupIds` holds it; character when it is the single
  LLM character AND `scenarioId` answered; else `store` labelled `${owner.name}
  › ${scenarioName ?? path-derived}`), `customText` = the stored notes; a GONE
  source falls through to the body-match inference; `textForSelection` (`:281-288`
  — Custom → preset CLEARS the box; preset→preset / →Custom keep it);
  preview incl. a `store` body by ref (`:312-313`); `postScenario`
  (`:352-377`) — toast `data.message || 'Scenario updated'`, drop the draft,
  AWAIT the invalidation of `scenarioSource`, then `onChatUpdated`;
  **`handleSave` (`:381-382`) ALWAYS sends `{...payload, scenario:
  customText}`**; `handleReload` (`:385-389`) `{scenarioRef: source,
  scenario: notes ?? '', reload: true}`; render (`:394-511`): the select
  (always), browser, provenance `From: <em>owner</em> › <em>name|path</em>` +
  ` — the shelf copy is gone` / ` — the shelf copy has been revised ` +
  **Reload from source** button, Show archived, the Host builder, then
  Custom → the 6-row textarea (`aria-label "Custom scenario text"`, placeholder
  `Set the scene for this conversation…`) vs preset → preview + label `Notes
  beneath the scene` + 3-row textarea (`aria-label "Notes beneath the
  scene"`, placeholder `Anything to add beneath the preset (optional)…`), the
  `Change scenario` / `Setting the scene…` button and the footnote. → v5
  `chat/sidebar/chat-scenario-control.ts` (436): `:83-160` template, `:203`
  draft, `:215-242` queries (+ source query), `:266-273` `showDropdown`
  (dies), `:282-310` `currentAsSelection` (+ source authority), `:395-403`
  setters (+ the Custom→preset clear), `:412-435` `save` (always `scenario`;
  + `invalidate` of the new chat-source key — v5's
  `['chat', id, 'scenario-source']` sits under the `chats` topic prefix
  `core/realtime-topic-map.ts:83-103`, so the realtime side needs only an
  assertion, P4.D206's idiom) + NEW `reload()`.
- `components/new-chat/NewChatForm.tsx` / `hooks/useNewChat.ts` /
  `types.ts` / `scenario-seed.ts` (NEW, 40) / `NewChatModal.tsx`:
  `NewChatFormState` drops the five pointer fields for `scenarioSelection` +
  `scenarioTouched` (`types.ts:170-181`); INITIAL `{kind:'custom'}`, false
  (`useNewChat.ts:166-167`); the seed EFFECT (`:661-688`):
  `singleLlmCharacter` (exactly one LLM seat), `characterDefaultScenarioId =
  continuationFromChatId ? null : singleLlm.defaultScenarioId ?? null`
  (UNGUARDED for archived — the d25dacc1 quirk persists), project / general
  defaults `isDefault && !archived`; `seedScenarioSelection(prev, defaults)`
  (character > project > general > custom; touched → keep), set only when the
  VALUE token changes; the load path no longer seeds any pointer
  (`:483-597` hunks), the single-LLM propagation drops `scenarioId`
  (`:703`); submit `Object.assign(requestBody,
  scenarioSelectionToPayload(state.scenarioSelection))` (`:846`) — exactly one
  pointer or none; `scenario` still only when non-empty. Form
  (`NewChatForm.tsx`): `selectedCharacterScenarioId` from the selection
  (`:194-210`), `selectedPresetBody` per kind (`:215-233`), store preview by ref
  (`:235-237`), preset block only when its content is non-empty (`:786`),
  override note ONLY when `selection.kind === 'project'` and the character
  default is listed (`:256`), every pick sets `scenarioTouched` (`:258-260`,
  `:293`, `:350`), the browser with `scenarioHierarchy {projectId:
  selectedProjectId ?? project?.id ?? null, groupIds: [...new Set(groupScenarios
  .map(s => s.groupId))], characterId: singleLlm?.character.id ?? null}`
  (`:263-271`), label `htmlFor="new-chat-scenario-select"` (`:723`), the select
  always rendered with `onBrowse` (`:740-759`). Modal: a cast change resets
  ONLY a `character` selection to custom (`NewChatModal.tsx:236-242`). → v5
  `screens/new-chat/new-chat.types.ts:126-151`, `new-chat.logic.ts:136-178`
  (die / re-shape) + `:237-247` (submit), `new-chat.state.ts:388-420,438-490,
  607-625` (seed → a `computed`/`effect` with `scenarioTouched`),
  `new-chat-form.ts:255` (`for="new-chat-scenario"` → `new-chat-scenario-select`),
  `:268-280`, `:553-622`, `:735-745`, `:780-818`, `character-picker-panel.ts:215,227`
  (clear only a character selection), NEW `scenario-seed.ts` (pure, two fns).
- `lib/query/keys.ts` / `topic-map.ts` → v5 `chatKeys`/`scenarioKeys`
  (`scenario/scenario.api.ts:34`) + NEW `fetchScenariosEverywhere`,
  `readScenarioByRef`, `fetchScenarioSource`.

**v5 spec files that move:** `scenario/scenario.types.spec.ts` (104),
`scenario/scenario-select.spec.ts` (338), `chat/sidebar/chat-scenario-control.spec.ts`
(522) + `.host.spec.ts` (313), `screens/new-chat/new-chat-form.spec.ts` (512)
+ `.host.spec.ts` (440), `new-chat.logic.spec.ts` (515),
`new-chat.state.spec.ts` (662), `character-picker-panel.spec.ts` (158),
`core/realtime-topic-map.spec.ts` / `chat-keys.spec.ts` (the new key under
the prefix). **Playwright that moves:** `e2e/salon-scenario-flow.spec.ts`
(456 — `:149-250` "opens on the scene in force, changes it, no-ops, and
clears it": the re-pick of the seeded General body is no longer a no-op
when picked as a preset (provenance adopted, `changed: true`, toast `Scenario
updated`, NO Host bubble), the dropdown always renders, the posted body now
carries `scenario` under a preset); `e2e/new-chat-flow.spec.ts` (333) only if
it asserts the scenario label/`for` or the dropdown's absence;
`e2e/scenario-builder-flow.spec.ts` (c) (`:238`) — the New Chat picker
selecting a saved General scenario (now via `scenarioTouched`). New beats:
Browse → pick from another project's shelf → "Elsewhere" + create; the
in-chat provenance + Reload (edit the shelf copy, see "revised", Reload, Host
announces). `scenarios-flow` / `scenarios-shelf-builder-flow` — the managers
(`components/scenarios/**`) are UNTOUCHED in the span.

### 8.2 Help (`help/`, v4 pin)

NEW `help/scenario-browser.md` (52 lines, `url: /salon/new`, title `# The
Scenario Browser — Every Scene on Every Shelf`); edited (by `545e299bb`):
`help/chats.md` (655 at the pin — ALSO `93c5af6ad`), `help/general-scenarios.md`
(123), `help/groups.md` (175), `help/project-scenarios.md` (106). v5 `help/`
is at 131 (`4a38222e6`). See §7.5.

### 8.3 `docs/v4/` paths

`docs/CHANGELOG.md` → `docs/v4/CHANGELOG.md`; `docs/developer/API.md` →
`docs/v4/developer/API.md`; `DDL.md` → `docs/v4/developer/DDL.md`;
`features/ROADMAP.md` → `docs/v4/developer/features/ROADMAP.md`;
`features/scenarios-from-anywhere.md` **MOVED** to `features/complete/`
(v5 still has `docs/v4/developer/features/scenarios-from-anywhere.md` — move
it); `features/complete/wardrobe-folders.md`; `features/scenario-builder.md`.
v4's root `CLAUDE.md` (the chokepoint rule) is not mirrored. API / DDL /
CHANGELOG also move in ten / ten / eighteen later memory commits — re-vendor
whole at the pin.

### 8.4 CLI

None — no v4 CLI hunk touches scenarios; v5 `quilltap-cli` has no scenario
surface.

---

## 9. Open points for the order (observations / questions — NOT rulings)

- **OP-1 — the deferred project/group startup heals.** The "honest-count"
  half of `545e299bb` (`{total, provisioned, healed, failed}`, the folder
  failure counted) lands in `ensure-{project,group}-scenarios.ts`, which v5
  has never ported (P4.4's tracked deferral). Port PHASE 3.4 / 3.4b now (the
  location's `ensure` makes them ~30 lines each; they heal stale official
  pointers on old instances), or keep deferring by name and record the counts
  as a NO-PORT? Note `backfill-group-stores.ts` precedes 3.4b in v4.
- **OP-2 — grouping with the memory round.** The scenarios D23 move and the
  memory re-dump share ONE `fresh_schema.json`, the `chats` row lists, the
  committed-fixture widening, `remap_chat`, the export schema, `help/chats.md`
  and the help tree. Two rounds = two re-dumps + two widenings of 50
  fixtures; one round = one keystone with both. The planner decides
  (ledger §1 already flags it).
- **OP-3 — split.** Recommended: **ONE server lane** (location + everywhere +
  read + the six route families + union + create hooks + General heal +
  selection + create + `?action=scenario` + `scenario-source` + import /
  restore carriers + vault sort Δ42 + harness) **+ ONE SPA lane** (types,
  select, browser, control, New Chat, specs, Playwright) **+ the keystone**
  (schema / ensure / row lists / fixture widening, shared with memory) **+
  help/docs folded into whichever lane re-vendors the whole tree at the pin.**
  The server lane is large (~2.5–3.5k Rust lines + ~8 families); if it must
  split, cut at the resolver: S-A = location + routes + everywhere/read +
  heals + vault sort; S-B = recipe (selection, create, action, source,
  carriers) consuming S-A's `lookup_scenario_ref` / `resolve_scenario_location`
  — which forces S-B to wait on S-A (a keystone-then-stage shape like
  P4.D279). The wire contract §7.2 is the SPA ⇄ server seam; pin it in BOTH
  orders' Shared-contract sections verbatim.
- **OP-4 — hazard (a), the precise shape.** Measured structurally: v5's
  reads are SAFE (explicit `ALL_COLUMNS` ignores the new columns), but (i) a
  v5 `?action=scenario` rewrites `scenarioText` and leaves a stale
  `scenarioSource`/`scenarioNotes`; v4's control then SEEDS the stale preset
  + notes as the authority (`currentAsSelection`), and a user's next "Change
  scenario" in v4 REPLACES the v5-written scene with the old preset; (ii) a v5
  backup / `.qtap` export drops both keys (via `chats_read::find_all`) and a
  v5 import / restore drops them (typed `ChatCreate`) — provenance lost, v4
  falls back to body inference. Both close with this slice; the ledger's "do
  not run a v5 build that writes scenarios against live Friday" stands until
  then.
- **OP-5 — the v5 log gaps that predate the span.** v5 logs none of L13-L18
  and never L20; the group create hook is silent where v4 WARNs. The span
  re-shapes all of them; port them whole now (they become capture-pinnable in
  the new families) or record a NO-PORT? (Recommend port — the families are
  being rebuilt anyway.)
- **OP-6 — `decodeURIComponent` on refs.** v5's `resolve_scenario_path`
  deliberately skips v4's decode (the dispatch delivers an already-decoded
  route segment — `db/scenarios.rs:487-492`). That premise is FALSE for the
  NEW ref arms: `scenarioRef.path` (JSON body) and `?action=read`'s `path`
  are decoded by v4's `normalizeScenarioPath` (`Scenarios/a%20b.md` reads `a
  b.md`; a malformed `%` THROWS `URIError` → a 500 at create / read / action,
  WARN-less). Port a JS-faithful `decodeURIComponent` for the ref arm (and
  the throw), keep the item-route arm as is, or rule a divergence?
- **OP-7 — the excerpt's code-unit cut.** `slice(0, 240)` can split a
  surrogate pair; v4 then emits a lone surrogate (JSON `\udXXX`). A Rust
  `String` cannot hold one. Options: replace with U+FFFD (ruled divergence) or
  cut before the pair (divergence). Needs a corpus row and a ruling; also pin
  JS `\s` (Unicode whitespace) in the collapse.
- **OP-8 — `findAllRaw` order for the stable `localeCompare(base)` sort.**
  Case-insensitive ties keep input order; v5's raw finders must return v4's
  `_findAll` row order (rowid?) or ties may flip. Measure on the fixture.
- **OP-9 — a malformed `scenarioSource` in a bundle / backup.** v4's
  `repos.chats.create` Zod-validates it: a `.qtap` chat with a bad source
  FAILS to import (a warning, the chat skipped); the restore path's behaviour
  depends on how restore inserts chats (check `restore.ts`); `readScenarioSource`
  only runs in reconcile/remap AFTER create. v5's import validates whole rows
  for several entities (P4.161) — extend to this key? And `chats_read`:
  what does v5 do with an unparseable stored `scenarioSource` cell (v4's row
  validation would drop/throw the row)? Decide the read arm.
- **OP-10 — the restore minting order.** A project/group/General source MINTS
  its mount's id at the chat step (chats precede projects / docMountPoints in
  `remapBackupData`). v5's `remap_backup_data` must call the remapper at the
  same point or every later counter id shifts — the tier-1 family will catch
  it, but the order should say so up front.
- **OP-11 — `?action=read` / `scenario-source` WARN noise.** A chat whose
  source owner was archived/deleted logs L9/L10 WARN on every
  `scenario-source` GET (each sidebar open + every `chats` realtime hint).
  v4-faithful; flag only so a dogfood walk does not file it.
- **OP-12 — tier-3 / loud refusals.** None needed: every surface is
  deterministic (no LLM). The Scenario Builder is untouched. The unknown
  `?action=` 400 on `GET /scenarios` has no v5 transport (typed dispatch) —
  NO-PORT by construction, record it.
- **OP-13 — the SPA `browse` affordance.** v4 shows "Browse all scenarios…"
  only when the surface passes `onBrowse`; both v4 surfaces pass it. The
  Angular port can always pass it (both surfaces) — confirm no third
  `qt-scenario-select` consumer exists that should NOT browse (grep found
  only the two).
- **OP-14 — `help/chats.md` ownership** — touched by this slice and by
  `93c5af6ad`; whole-file vendor at the pin by one owner (§7.5).
