# Survey — the MEMORIES API surface, the memory-touching chat / message / system routes, the CLI, and the shared instance lock across `4a38222e6` → `64743ee13`

Read-only planning survey, 2026-10-13. v4 = `~/source/quilltap-server`, read
ONLY through `git show 64743ee13:<path>` (the PIN, `4.10.0-dev.175`, v4 `main`
HEAD, clean) and `git diff 4a38222e6 64743ee13 -- <paths>` / `git show <sha> --
<paths>` (the span). v5 = `~/source/quilltap-v5` `main` at `a7c6f0d03`. **Every
v4 line number below is AT THE PIN** unless marked `(pre)` (= `4a38222e6`, the
current oracle baseline — the shape v5 ported). Nothing here is taken from
commit prose (drift ledger §5.3); every claim is read off a shipped hunk. The
drift ledger's §1 (lines 17–553) and the §3 rows for this slice's shas
(`drift-ledger.md:595` `124c8aca3`, `:600` `c2795733f`, `:601` `e0ac7493a`,
`:602` `af48f6cc2`, `:603` `8af764514`, `:604` `ac6348c5f`, `:605` `93c5af6ad`,
`:606` `584d28164`, `:607` `4ae0c4ca6`, `:608` `104e986c7`, `:609`
`b045dd986`, `:610` `48790fb73`, `:611` `9ec4fa2e9`, `:612` `64743ee13`) were
read first and are built on, not restated; where a hunk disagrees with them it
says so (§9 C-1…C-3).

**Scope (this survey):** `app/api/v1/memories/route.ts` + `[id]/route.ts`
(every action, schema, status, body, log line); the memory-touching hunks of
`app/api/v1/chats/[id]/actions/{memories,bulk,recall-replay}.ts`,
`app/api/v1/messages/[id]/route.ts`,
`app/api/v1/chats/[id]/messages/[messageId]/route.ts`,
`app/api/v1/system/tools/route.ts` (the retired `memory-dedup` pair and the
`INTER_CHARACTER_MEMORY` estimate/name rows); the settings READ/WRITE surfaces
those routes call (`lib/instance-settings`, `lib/schemas/settings.types.ts`,
`lib/schemas/patch-schema.ts`); v5's `api/memories.rs`,
`api/memory_consolidation.rs`, `api/memory_maintenance.rs`, the `Request` /
`Response` variants and engine arms, `quilltap-web/src/system_data_routes.rs`,
the dispatch / web censuses; the CLI (`packages/quilltap/**` vs
`crates/quilltap-cli`); the shared `quilltap.lock` (`lib/database/backends/
sqlite/instance-lock.ts` + `packages/quilltap/lib/lock-helpers.js` vs
`crates/quilltap-host/src/lock.rs`).

**Out of scope (named siblings own the INTERNALS; this survey names the seam
it calls):** the maintenance job's phases, ledger internals, scheduler and
triggers (`lib/memory/maintenance/*`); admission (`lib/memory/admission/*`,
incl. `admitCandidate` / `updateMemory` / the anchor-gate probe's decisions);
the retirement chokepoint (`lib/memory/retire.ts`) and the index store
(`lib/memory/index-store/*`); recall (`lib/memory/recall/*`, incl. the
recall-replay result's new keys); extraction (`lib/memory/extraction/*`,
`listExtractionTurns`, the dry-run NDJSON producer); the schema keystone (the
eleven migrations, `fresh_schema.json`, the `chat_settings` column drops, the
`memory_extraction_cursors` table, the repository functions listed in §7);
the SPA cards; `help/` + `docs/v4/` re-vendoring.

---

## 0. The span for this slice, in one paragraph

Ten of the nineteen commits touch `app/api/v1/memories/route.ts`
(`124c8aca3`, `c2795733f`, `e0ac7493a`, `af48f6cc2`, `8af764514`,
`ac6348c5f`, `93c5af6ad`, `584d28164`, `104e986c7`, `b045dd986`; 1,397 (pre) →
1,388 lines) and four touch `[id]/route.ts` (`124c8aca3`, `c2795733f`,
`af48f6cc2`, `93c5af6ad`; 195 lines at the pin). The surface that results:
**five POST + three GET actions RETIRED** (`housekeep` GET+POST,
`housekeep-sweep`, `housekeeping-config` GET+POST, `consolidate`,
`extraction-concurrency` GET+POST — the concurrency pair by `e0ac7493a`, the
rest by `b045dd986` after `104e986c7` had briefly kept them as aliases), plus
the `system/tools` `memory-dedup-preview` / `memory-dedup` pair (`104e986c7`);
**four NEW** (`POST ?action=maintain`, `GET ?action=maintenance-status`,
`POST ?action=repair`, GET/POST `?action=maintenance-config` — the config
pair now instance-wide in `instance_settings.memoryMaintenance`, with
`phases` toggles from `b045dd986`); the manual create rebuilt on admission
(MANUAL forced and `skipGate`/`source` stripped — bug 218, `c2795733f`;
`projectId` / `kind` / `occurredAt` / `entities` accepted; `{memory, action}`
with the lowercase admission kind and 201-vs-200 — `8af764514`; trim + the
by-reason 400/502 — bug 236, `93c5af6ad`); the item GET stops stamping
`lastAccessedAt` (bug 219, `c2795733f`); the item PUT runs through the
`updateMemory` chokepoint (re-embed on text change, 502 / 400
`invalid-related`, `c2795733f`); the item DELETE and the chat wipe through
`retireMemories` / `retireChatMemories` (bugs 200/201 — `124c8aca3`, then
`af48f6cc2`; the wipe answers `sparedCount`); every settings POST validated
through `patchSchemaOf` (bug 244, `b045dd986`); `character-memory-counts`
gains the cap breakdown (bug 210, `c2795733f`); POST `embeddings` loses
`batchSize` and PUT `embeddings`'s `failed` changes meaning (`af48f6cc2`);
search defaults `minImportance` to `TOOL_MIN_IMPORTANCE` (`e0ac7493a`) and
moves onto `lib/memory/recall/search` (`584d28164`). The CLI gains the
lock-recorded port (`resolveServerPort`) on `anchor-probe` / `recall-replay`
/ `memory-diff` / `memories` (bug 239, `93c5af6ad`), the `Port:` line in `db
--lock-status`, the re-labelled anchor-probe tables (`8af764514`) and its
`note` (`4ae0c4ca6`), recall-replay's ring / fallback / trace printing
(`584d28164`), new completion templates and `db memories --tier`
(`b045dd986`), and the `memories maintain` / `validate --repair` verbs
(`b045dd986`, `48790fb73`, `9ec4fa2e9`). The server lock gains an optional
`port` (bug 239).

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `app/api/v1/memories/route.ts` (1,388 lines)

Imports that are new at the pin (each a sibling seam, §7): `searchMemories`
(`lib/memory/recall/search`), `generateMissingEmbeddings` /
`rebuildVectorIndex` / `enqueueMissingEmbeddings`
(`lib/memory/index-store/backfill`), `admitCandidate` / `admissionInserted`
(`lib/memory/admission`), `retireChatMemories` (`lib/memory/retire`),
`repairMemoryGraph` (`lib/memory/repair`), `runMaintenance` /
`getMaintenanceLedgerStatus` / `MAINTENANCE_PHASES` / `withImpliedPhases` /
`MaintenanceReport` (`lib/memory/maintenance`), `filterImportance` /
`TOOL_MIN_IMPORTANCE` (`lib/memory/scoring`), `MemoryCandidateSchema` /
`MEMORY_ENTITY_CAP` (`lib/memory/candidate`), the three
`MemoryMaintenance*Schema`s, `patchSchemaOf`, `MemoryKindEnum`,
`enqueueMemoryMaintenance` / `enqueueMemoryMaintenanceForUser`,
`get/setMemoryMaintenanceSettings`, `errorResponse`.

#### 1.1.1 Dispatch maps (`dispatchAction`, `lib/api/middleware/actions.ts:153-183`)

- **GET** (`:199-214`), thunk order = v4's `availableActions` order:
  `maintenance-status`, `embeddings`, `maintenance-config`,
  `extraction-limits-config`, `recall-config`, `consolidation-config`,
  `extraction-mode-config`, `backfill-embeddings`, `character-memory-counts`,
  `regenerate-all`; fallback = the list (`handleListMemories`).
  (pre `:181-197`: `housekeep`, `embeddings`, `housekeeping-config`,
  `extraction-limits-config`, `recall-config`, `consolidation-config`,
  `extraction-mode-config`, `backfill-embeddings`, `character-memory-counts`,
  `extraction-concurrency`, `regenerate-all`.)
- **POST** (`:254-273`): `search`, `maintain`, `repair`, `embeddings`,
  `maintenance-config`, `extraction-limits-config`, `recall-config`,
  `consolidation-config`, `extraction-mode-config`, `backfill-embeddings`,
  `regenerate-all`, `anchor-gate-probe`; fallback = create.
  (pre `:237-258`: `search`, `housekeep`, `housekeep-sweep`, `embeddings`,
  `housekeeping-config`, `extraction-limits-config`, `recall-config`,
  `consolidation-config`, `extraction-mode-config`, `consolidate`,
  `backfill-embeddings`, `regenerate-all`, `extraction-concurrency`,
  `anchor-gate-probe`.)
- **PUT** (`:279-281`): `embeddings` only, no fallback (`Action parameter
  required` without one). **DELETE** (`:287-295`): `?chatId=` → wipe, else 400
  `Query parameter required: chatId`.
- **A retired action at the pin** answers `dispatchAction`'s unknown arm:
  400 `{"error":"Unknown action: <x>","availableActions":[…the map above…]}`
  with WARN (logger child `{module:'api-action-middleware'}`) `Unknown action
  requested` `{action, availableActions, method, path}` (`actions.ts:83-99`).
  v4's own test pins it: `memories-consolidation-actions.test.ts:187-194`
  ("the retired alias actions are gone" — `consolidate`, `housekeep`,
  `housekeep-sweep`, `housekeeping-config` → 400).

#### 1.1.2 Zod request schemas (verbatim where exactness matters)

```ts
// :94-116  createMemorySchema (source + skipGate GONE — stripped if sent)
characterId: z.uuid('Character ID is required'),
content: z.string().trim().min(1, 'Memory content is required'),   // bug 236
summary: z.string().trim().min(1, 'Memory summary is required'),
keywords: z.array(z.string()).prefault([]),
tags: z.array(z.uuid()).prefault([]),
importance: z.number().min(0).max(1).prefault(0.5),
aboutCharacterId: z.uuid().nullable().optional(),
chatId: z.uuid().nullable().optional(),
projectId: z.uuid().nullable().optional(),          // NEW (c2795733f)
sourceMessageId: z.uuid().nullable().optional(),
kind: MemoryKindEnum.optional(),                     // ['semantic','episodic']
occurredAt: z.iso.datetime({ offset: true }).nullable().optional(),
entities: z.array(z.string().min(1)).max(MEMORY_ENTITY_CAP).optional(), // 12
// :118-125 searchMemorySchema — UNCHANGED from (pre) :90-97
// :129-141 maintainSchema
characterId: z.uuid().optional(),
phases: z.array(z.enum(MAINTENANCE_PHASES)).min(1).optional(),
dryRun: z.boolean().optional(),
settings: z.object({
  consolidation: patchSchemaOf(MemoryConsolidationSettingsSchema).optional(),
  maxClustersPerRun: z.number().int().min(1).max(1000).optional(),
  cap: MemoryMaintenanceCapSchema.optional(),          // int 100..100000
}).optional(),
// :144-148 repairSchema
characterId: z.uuid().optional(), dryRun: z.boolean().optional(),
// :150-152 generateEmbeddingsSchema — batchSize GONE (pre :110-113 had
//          z.number().min(1).max(50).prefault(10))
characterId: z.uuid('Character ID is required'),
// :154-159 rebuildIndexSchema — UNCHANGED
// :163-168 maintenanceConfigSchema
enabled: z.boolean().optional(),
phases: patchSchemaOf(MemoryMaintenancePhaseTogglesSchema).optional(),
perCharacterCap: MemoryMaintenanceCapSchema.optional(),
perCharacterCapOverrides: MemoryMaintenanceCapOverridesSchema.optional(),
// :173-179 the four settings patch schemas (bug 244)
extractionLimitsConfigSchema = patchSchemaOf(MemoryExtractionLimitsSchema)
recallConfigSchema           = patchSchemaOf(MemoryRecallSettingsSchema)
consolidationConfigSchema    = patchSchemaOf(MemoryConsolidationSettingsSchema)
extractionModeConfigSchema   = patchSchemaOf(MemoryExtractionModeSettingsSchema)
// :182-187 backfillStartSchema — UNCHANGED (characterId uuid opt, batchSize int 1..2000 prefault 500)
// :685-688 anchorGateProbeSchema — UNCHANGED
```

Retired with their actions: `housekeepingOptionsSchema` (pre `:99-108`),
`housekeepingConfigSchema` (pre `:122-128`), `consolidateSchema` (pre
`:153-158`), `extractionConcurrencySchema` (pre `:167-169`), and the
`CONSOLIDATE_DRY_RUN_*` constants (pre `:149-151`, renamed `MAINTAIN_DRY_RUN_*`
`:732-734` with the same values 10 / 40 / 120_000).

#### 1.1.3 Every handler at the pin — status, body (key order), log lines

| Action | Handler (pin lines) | Success body (keys in order) | Refusals (status, `error` VERBATIM) | Lines (level, message, context keys) |
|---|---|---|---|---|
| GET list `?characterId=` | `listMemoriesByCharacter` `:301-417` | paginated (`limit` present): `{memories, count, totalCount}` (each memory `...row, tagDetails`); legacy: same, `totalCount = count` | 400 `Query parameter required: characterId, chatId, or messageId`; 404 `Character not found`; catch → 500 `Failed to list memories` | ERROR `[Memories API] Error listing memories` `{}` + error (`:245`) |
| GET `?chatId=` | `countMemoriesByChat` `:419-436` | `{chatId, memoryCount}` | 404 `Chat not found` | — |
| GET `?messageId=` | `getMemoriesByMessage` `:438-504` | `{memoryCount, isSwipeGroup, swipeCount, memories:[{id, summary, characterId, importance}]}` | 404 `Message not found` | — |
| POST create (no action) | `handleCreateMemory` `:506-638` | **201 when inserted, 200 otherwise**: `{memory, action}` (`action` ∈ `insert` / `insert-linked` / `reinforce` / `absorb` / `promote-and-reinforce`) | 400 `Validation error` + `details` (`validationError`, `:511-514` and the candidate-schema gap `:576-584`); 404 `Character not found` / `Project not found` / `Chat not found` (in that order, `:526-549`); 400 `Memory content and summary must not be blank.` (`reject:empty`, `:604-606`); **502** `The memory was not saved because its text could not be embedded. Check the configured embedding profile.` (`reject:embedding-failed`, `:607-614`); 500 `The memory was not saved.` (`:615`) | DEBUG `[Memories API] Ignoring client-supplied source/skipGate on manual create` `{characterId, source, skipGate}` (only when the body has `skipGate`, or `source` ≠ `'MANUAL'`, `:517-523`); DEBUG `[Memories API] Manual memory refused by the candidate schema` `{characterId, issues}` (`:579`); DEBUG `[Memories API] Manual memory not admitted` `{characterId, reason}` (`:600`); DEBUG `[Memories API] Manual memory create resolved` `{characterId, memoryId, action, inserted, projectId}` (`:619-625`); WARN `[Memories API] Failed to schedule refit` `{error}` (fire-and-forget seam) |
| POST `?action=search` | `handleSearch` `:640-683` | `{memories:[…row, score, usedEmbedding, tagDetails], count, query, usedEmbedding}` | `.parse` throw → middleware 400 `Validation error` + details; 404 `Character not found` | — (the pre `searchMemoriesSemantic` comment "Access times are bumped inside" is GONE; `:654-655` "it stamps nothing (only `deliver` writes lastAccessedAt)") |
| POST `?action=anchor-gate-probe` | `:696-729` | `successResponse(result)` (body = the probe result) | 400 Validation error; 404 `Character not found`; 500 `Anchor gate probe failed` | DEBUG `[Memories API] Running anchor gate probe` `{characterId, limit}`; ERROR `[Memories API] Anchor gate probe failed` `{characterId, error}` — UNCHANGED from (pre) |
| **POST `?action=maintain`** (NEW) | `handleMaintain` `:736-817` | dry run, one char: `{success:true, dryRun:true, report}`; dry run, user-wide: `{success:true, dryRun:true, reports:[…]}`; live: `{success:true, dryRun:false, jobId: jobIds[0] ?? null, jobIds}` | 400 Validation error (`safeParse`); 404 `Character not found`; 400 `A dry run that includes consolidation names one character (characterId)` (`:758-761`); 500 `Maintenance dry run failed` (`:798`); 500 `Failed to enqueue maintenance` (`:815`) | DEBUG `[Memories API] Maintenance dry run (in-process)` `{characterId: id ?? 'all', characters, phases: phases ?? 'all', maxClustersPerRun}` (`:769-774`); ERROR `[Memories API] Maintenance dry run failed` `{characterId}` + error (`:797`); INFO `[Memories API] Maintenance enqueued` `{userId, characterId: id ?? 'all', phases: phases ?? 'all', jobs}` (`:811`); ERROR `[Memories API] Failed to enqueue maintenance` `{characterId}` + error (`:814`) |
| **GET `?action=maintenance-status`** (NEW) | `handleMaintenanceStatus` `:819-833` | `{success:true, ledger:[LedgerStatus…]}` | 404 `Character not found` (only when `characterId` is truthy) | DEBUG `[Memories API] Maintenance status read` `{characterId: id ?? 'all', entries}` |
| **POST `?action=repair`** (NEW) | `handleRepair` `:835-858` | `{success:true, report}` | 400 Validation error; 404 `Character not found`; 500 `Memory graph repair failed` | ERROR `[Memories API] Memory graph repair failed` `{characterId}` + error (`:855`); the service's INFO `Memory graph repair` (module `memory:repair`) `{dryRun, characterId: id ?? 'all', found, repaired: report.repaired ?? null}` |
| **GET `?action=maintenance-config`** (NEW) | `:864-870` | `{success:true, settings}` (`{enabled, perCharacterCap, perCharacterCapOverrides, phases:{foldNearDuplicates, demote, retire}}`) | — (a corrupt stored row → defaults with WARN `[InstanceSettings] memoryMaintenance failed to parse — using defaults` `{error}`, `instance-settings/index.ts:144`) | — |
| **POST `?action=maintenance-config`** (NEW) | `:872-899` | `{success:true, settings}` (the validated, stored object) | 400 Validation error | INFO `[Memories API] Memory maintenance settings updated` `{userId, enabled, perCharacterCap, capOverrides: Object.keys(overrides).length, phases}` |
| GET/POST `?action=extraction-limits-config` | `:901-935` | `{success, settings}` / `{success, settings: merged}` (`enabled, maxPerHour, softStartFraction, softFloor`) | 400 Validation error | INFO `[Memories API] Extraction rate limits updated (instance-wide)` `{enabled, maxPerHour}` — UNCHANGED |
| GET/POST `?action=recall-config` | `:937-972` | `{success, settings}` (`scopePolicy, expandRelated, perTurnConversationSummaries`) | 400 Validation error | INFO `[Memories API] Recall settings updated (instance-wide)` `{scopePolicy, expandRelated, perTurnConversationSummaries}` — UNCHANGED |
| GET/POST `?action=consolidation-config` | `:974-996` | `{success, settings}` (re-read after write) | 400 Validation error | INFO `[Memories API] Consolidation settings updated (instance-wide)` `{enabled, fields: Object.keys(patch)}` — `fields` NOW lists only the SENT keys (bug 244) |
| GET/POST `?action=extraction-mode-config` | `:998-1019` | `{success, settings}` | 400 Validation error | INFO `[Memories API] Extraction-mode settings updated (instance-wide)` `{otherPass, fields}` — same `fields` change |
| GET `?action=character-memory-counts` | `:1022-1052` | `{success:true, characters:[{id, name, memoryCount, hotTotal, demotable, protected, cold}]}` sorted `b.demotable - a.demotable \|\| b.memoryCount - a.memoryCount` (stable) | — | DEBUG `[Memories API] Character memory counts computed` `{userId, characters}` (NEW) |
| GET `?action=backfill-embeddings` | `:1054-1080` | `{success, progress:{remaining, inFlight}}` | — | UNCHANGED |
| POST `?action=backfill-embeddings` | `:1082-1133` | `{success, enqueued, remaining, message}` (the two `message` templates and the zero-case sentence UNCHANGED) | 400 Validation error; 404; 400 `No default embedding profile is configured. Set one in the Commonplace Book tab before running the backfill.` | INFO `[Memories API] Embedding backfill batch enqueued` `{userId, characterId: id ?? '(all)', enqueued, remaining}`; the per-row WARN `[Memories API] Failed to enqueue backfill embedding job` (pre) is GONE — now one batch insert that skips rows already holding a job (`index-store/backfill.ts:115-150`, DEBUG `[Memory backfill] Embedding jobs enqueued`) |
| POST `?action=embeddings` | `:1135-1181` | `{message:'All memories already have embeddings', processed:0, failed:0, skipped:0, total}` or `{message:'Embedding generation complete', processed, failed, skipped, total, remaining}` | `.parse` 400; 404; 400 `No embedding profile configured. Please set up an embedding profile in settings.` | service DEBUG `[Memory] Backfilled missing embeddings` `{characterId, processed, failed, skipped}` (NEW, `backfill.ts:72`); WARN `[Memory] Failed to generate embedding for memory <id>` unchanged |
| GET `?action=embeddings&characterId=` | `:1183-1216` | `{total, withEmbeddings, withoutEmbeddings, percentComplete, embeddingProfileConfigured, embeddingProfileName}` | 400 `characterId query parameter required for embedding status`; 404 | UNCHANGED |
| PUT `?action=embeddings` | `:1218-1238` | `{message:'Vector index rebuilt successfully', indexed, failed}` — **`failed` is now `countWithoutEmbedding(characterId)`** (`backfill.ts:85-101`), (pre) it was per-vector add failures | `.parse` 400; 404 | service DEBUG `[Memory] Rebuilt vector index from memory rows` `{characterId, indexed, coldIndexed, withoutEmbedding}` |
| DELETE `?chatId=` | `handleDeleteByChatId` `:1240-1269` | `{success:true, chatId, deletedCount, sparedCount}` | 404 `Chat not found` | DEBUG `[Memories API] Wiped a chat's extracted memories` `{chatId, deleted, spared}` (`:1261`; (pre) `Deleted every memory for a chat` `{chatId, deleted}`); the service's INFO `Chat wipe spared manual, cold and digest rows` `{chatId, spared, sparedBy}` (only when spared > 0) and DEBUG `Wiped a chat's extracted memories` `{chatId, retired, spared}` (module `memory:retire`, `retire.ts:543-555`) |
| POST `?action=regenerate-all` | `:1271-1361` | UNCHANGED shape | UNCHANGED | the clear list loses `INTER_CHARACTER_MEMORY` (`:1282-1289`, `ac6348c5f`) |
| GET `?action=regenerate-all` | `:1363-1388` | UNCHANGED | — | — |

**Create in detail** (`:506-638`, the order is load-bearing):
`await req.json()` → `createMemorySchema.safeParse` → 400 → the
source/skipGate DEBUG → character 404 → `projectId` given ⇒ `projects.findById`
404 `Project` → `chatId` given ⇒ `chats.findById` 404 `Chat`, and when no
explicit project, `projectId = chat.projectId ?? null` →
`MemoryCandidateSchema.safeParse({content, summary, keywords, targeting:null,
importance, kind: kind ?? 'semantic', when:null, entities: entities ?? [],
tags, subject:{holderId: characterId, aboutId: aboutCharacterId ?? null},
provenance:{source:'MANUAL', witnessedContext:'manual', chatId: chatId ?? null,
projectId, sourceMessageId: sourceMessageId ?? null, anchorTimeMs: Date.now(),
pass:'manual'}})` → `admitCandidate(candidate, {occurredAt: occurredAt ??
null, narrativeTime:null}, {userId})` → `outcome.memory` null ⇒ the reason
arms (`reason = action.kind === 'reject' ? action.reason : 'unknown'`) →
`inserted = admissionInserted(outcome)` (`insert` / `insert-linked`,
`admission/outcome.ts:11-13`) → `scheduleRefit` (seam) → `{memory, action:
outcome.action.kind}` at 201/200. Trim is a Zod TRANSFORM: `'  x  '` is
stored as `'x'`.

**Maintain in detail** (`:736-817`): `req.json().catch(() => ({}))` →
`safeParse` → `phases = parsed.phases ? withImpliedPhases(parsed.phases) :
undefined` (`maintenance/types.ts:24-28`: `consolidate` adds `mirror`; result
in `MAINTENANCE_PHASES` order `['fold-near-duplicates','consolidate','demote',
'retire','mirror']`) → character 404 when named → **dry run**: `mayCallModel
= !phases || phases.includes('consolidate')`; no characterId + mayCallModel ⇒
the 400; `maxClustersPerRun = Math.min(settings?.maxClustersPerRun ?? 10,
40)`; targets = `[{id}]` or `characters.findByUserId(user.id).filter(c =>
!c.archivedAt)`; for each, sequentially, `runMaintenance({characterId,
reason:'manual', dryRun:true, ...(phases ? {phases} : {}), settings:
{...settings, maxClustersPerRun}}, {userId, timeBudgetMs: 120_000})`; a throw
anywhere ⇒ the 500 (no partial `reports`). **Live**: `payload = {reason:
'manual', ...(phases ? {phases} : {}), ...(settings ? {settings} : {})}` (the
live `maxClustersPerRun` is NOT clamped) → `[await
enqueueMemoryMaintenance(userId, {...payload, characterId})]` or
`enqueueMemoryMaintenanceForUser(userId, payload)` (`queue-service.ts:739-787`
— one deduped job per live, non-archived character).

**maintenance-status in detail** (`:819-833`): `characterId =
searchParams.get('characterId')` (FIRST occurrence); the 404 only when truthy;
`getMaintenanceLedgerStatus(characterId ?? undefined)` — note `?characterId=`
(empty) skips the 404 AND reads as "all" inside the ledger (`ledger.ts:261`
`characterId ? [characterId] : all`), and the DEBUG then logs `characterId:
''` (`'' ?? 'all'` is `''`). `LedgerStatus` (`ledger.ts:251-256`): `{characterId,
lastRun:{<phase>:{at: ISO, reason}}, lastSummary: MaintenanceSummary|null,
lastDemotion:{at: ISO, demoted, excess, effective}|null}`; `effective =
!lastDemotionIneffective(entry, Math.min(now, at))` = `demoted >=
max(10, floor(excess × 0.01))` (`:171-176`); all-characters order = the
in-memory `Map`'s insertion order; the ledger seeds once per process from
`backgroundJobs.findRecentByType('MEMORY_MAINTENANCE', 100)` (`:100-131`).

**repair in detail** (`lib/memory/repair.ts`, 98 lines): `found` from
`memories.findDanglingLinks(scope)` (`memories.repository.ts:1290-1342`, NO
fallback — a failed scan throws → the route's 500) and
`countWithoutEmbedding(characterId)` (instance-wide when unscoped — NOT
user-scoped); a live run adds `repaired = {...scrubDanglingMemoryLinks(links)
(relatedRowsTouched, digestsTouched, digestsRetired, membersCut,
charactersAffected), embeddingJobsQueued, embeddingSkippedReason?:
'no-embedding-profile'}` (`REPAIR_EMBEDDING_BATCH = 2000`). Report key order:
`dryRun, characterId (null when unscoped), found:{related:{links, rows},
consolidatedFrom:{links, digests}, supersededBy, missingEmbeddings},
repaired?`. **`dryRun` defaults to `false`: a bare POST `{}` is a LIVE,
instance-wide repair** (v4's own test pins it,
`memories-consolidation-actions.test.ts:330-339`).

**maintenance-config POST in detail** (`:872-899`): `safeParse` (unknown keys
— e.g. the retired `autoMergeSimilarThreshold` / `mergeSimilar` — are
STRIPPED, not refused) → `{phases, ...rest}`; when `phases` present, read
current and merge `{...current.phases, ...phases}`; `setMemoryMaintenanceSettings({...rest,
...mergedPhases})` (`instance-settings/index.ts:274-282`: `{...current,
...value}` then `writeJsonSetting` = full-schema `.parse` + `JSON.stringify`,
echoing the validated object). `perCharacterCapOverrides` REPLACES the whole
map (no per-key merge). Schemas (`settings.types.ts:131-187`):
`MEMORY_MAINTENANCE_CAP_MIN = 100`, `MAX = 100_000`; cap = `z.number().int()
.min(100).max(100000)`; overrides = `z.record(z.string(), cap)`; toggles =
`{foldNearDuplicates, demote, retire}` each `z.boolean().default(true)`;
settings = `{enabled: false, perCharacterCap: 2000, perCharacterCapOverrides:
{}, phases: toggles.prefault({})}`. The (pre) housekeeping config's override
values were `int().positive()` and its cap `int().min(100).max(100000)` — the
pin's override bound is NARROWER (100..100000).

#### 1.1.4 `lib/schemas/patch-schema.ts` (NEW, 42 lines, `b045dd986`)

`patchSchemaOf(schema)`: for each shape key, unwrap every `.default()` /
`.prefault()` (`withoutDefault`, `:18-28`) then `.optional()`; unknown keys
stripped; bounds / enums / nullability kept. Consequence for the four settings
POSTs: an omitted key stays omitted (the (pre) `.partial()` refilled every
default — the clobber v5 ported as `PARTIAL_DEFAULTS_CLOBBER`).

Settings schema deltas the routes inherit (`settings.types.ts`, `diff`):
`MemoryConsolidationSettingsSchema` gains `tfidfClusterThreshold:
z.number().min(0).max(1).default(0.30)` between `clusterThreshold` and
`minClusterSize` (`4ae0c4ca6`); `MemoryExtractionLimitsSchema.maxPerHour`
gains `.max(10_000)` (the route's (pre) hand schema was `int().min(1)
.max(10000)` — same accepted set for numbers, different issue text);
`AutoHousekeeping*Schema` → `MemoryMaintenance*Schema`; `ChatSettingsSchema`
LOSES `autoHousekeepingSettings` + `memoryExtractionLimits` (so every chat
settings READ body loses both keys — `chat-settings.repository.ts` defaults
`:165-179` (pre) deleted).

### 1.2 `app/api/v1/memories/[id]/route.ts` (195 lines)

- `updateMemorySchema` (`:19-29`): `content` / `summary`
  `z.string().trim().min(1).optional()` (bug 236), `keywords`, `tags`
  (`uuid[]`), `importance` 0..1, `aboutCharacterId` / `chatId` uuid
  nullable optional, `relatedMemoryIds` uuid[] optional.
- **GET** (`:34-74`): 404 `Memory not found` (absent row, or its character
  absent); `{memory: {...row, tagDetails}}`; **no access stamp** (bug 219);
  DEBUG `[Memories API v1] Fetched memory (access time not stamped)`
  `{memoryId, characterId}`; catch → 500 `Failed to fetch memory` + ERROR
  `[Memories API v1] Error fetching memory`.
- **PUT** (`:79-142`): 404s first, then `updateMemorySchema.parse(body)`
  (THROW → middleware 400 `Validation error` + details), then
  `updateMemory(characterId, memoryId, data, {userId})`
  (`admission/update-memory.ts:77-…`): `!ok` → `embedding-failed` ⇒ WARN
  `[Memories API v1] Memory edit refused: embedding failed` `{memoryId,
  characterId, error: result.message}` + **502** `The memory was not saved
  because its new text could not be embedded. ${result.message}` (and
  `result.message` = `Could not embed the edited memory: <provider error>`);
  `invalid-related` ⇒ 400 `relatedMemoryIds must name existing memories of the
  same character: <ids joined ', '>`; `not-found` ⇒ 404. Success: DEBUG
  `[Memories API v1] Memory updated` `{memoryId, characterId, fields:
  Object.keys(validatedData)}` (Zod output = schema key order, present keys
  only) → `scheduleRefit` seam → `{memory}`. The chokepoint re-embeds ONLY
  when `content`/`summary` changes value; an importance edit recomputes
  `reinforcedImportance` from `reinforcementCount ?? 1`; a
  `relatedMemoryIds` edit dedupes, drops self, validates every id, and links
  both sides. With no embedding profile a TEXT edit is a 502, an
  importance-only edit succeeds.
- **DELETE** (`:147-195`): 404s, then `retireMemories([id], {reason:
  'item-delete'})`; `retired === 0` ⇒ 404 `Memory not found`; seams
  (`handleEntityDeletion`, `scheduleRefit`); INFO `[Memories API v1] Memory
  deleted` `{memoryId, characterId}`; `{success:true}`; catch → 500 `Failed to
  delete memory`.

### 1.3 Memory-touching chat / message / system routes

- `app/api/v1/chats/[id]/actions/memories.ts` (`ac6348c5f`, `e0ac7493a`):
  `resolveExtractionProfileId(chat, ctx)` — seat profile first
  (`pickExtractionConnectionProfileId`), then `cheapLLMSettings.
  defaultCheapProfileId`, then `USER_DEFINED`'s `userDefinedProfileId`, each
  owned by the user. **queue-memories** 400 sentence is now `No connection
  profile available for memory extraction. Give a seat a profile or set a
  cheap LLM profile in settings.` with WARN `[Chats v1] No connection profile
  for memory extraction` `{userId, chatId}` ((pre) `No valid cheap LLM
  configured. Please set a cheap LLM profile in settings.` + WARN `[Chats v1]
  No valid cheap LLM configured` `{userId}`); turns from
  `listExtractionTurns(messages, chat.chatType)`; the non-autonomous empty
  sentence is now `No messages found in this chat — nothing to extract
  memories from.` ((pre) `No user messages found …`; the autonomous one is
  unchanged). **extract-memories-dry-run** streams the new NDJSON (`start` with
  `concurrency`; `candidate` lines BEFORE their `turn`; `fold` events
  `{type:'fold', pass, slices, skippedReason, candidatesAdded, debugLogs}`;
  candidates carry `extractionPass` + `decision`), 400 `ExtractionContextError
  .message` from `buildExtractionContext`, INFO `[Chats v1] Streaming dry-run
  memory extraction` `{chatId, concurrency, connectionProfileId, projectId}`.
  Internals = the extraction sibling; v5 has no dry-run route (listed in
  `wardrobe_routes.rs:627`'s chat action names only).
- `chats/[id]/actions/bulk.ts` (`af48f6cc2`): ONE `retireMemories(ids,
  {reason:'message-reattribution'})`, `memoriesDeleted = report.retired`; the
  error line is now `[Chats v1] Failed to delete memories during bulk
  re-attribution` `{memoryCount, error}` ((pre) per memory `… memory during
  bulk re-attribution` `{memoryId, error}`).
- `messages/[id]/route.ts` (`af48f6cc2`): the DELETE cascade collects ids per
  source message and calls `retireMemories(ids, {reason:'message-cascade'})`;
  INFO `[Messages API v1] Cascade deleted memories with message` `{messageId,
  memoriesDeleted, vectorsRemoved}` (values from the report). Reattribute:
  ONE retire, error line `[Messages API v1] Failed to delete memories during
  re-attribution` `{messageId, memoryCount, error}`.
- `chats/[id]/messages/[messageId]/route.ts` (`ac6348c5f`): the Courier
  resolve drops `conciergePolicy` / `isDangerousChat` from `MemoryChatSettings`
  (extraction sibling).
- `chats/[id]/actions/recall-replay.ts` (`584d28164`): import paths only —
  **the request wire is UNCHANGED**; the response gains keys (recall sibling).
- `system/tools/route.ts` (`104e986c7`, `ac6348c5f`): GET
  `memory-dedup-preview` and POST `memory-dedup` DELETED from both maps (and
  `deduplicateAllMemories` with `lib/tools/memory-dedup.ts`); a call answers
  400 `Unknown action`. `estimateTokensForJob` + `getJobTypeName` lose the
  `INTER_CHARACTER_MEMORY` rows (+400 tokens; `'Character Memory'`). No
  `MEMORY_MAINTENANCE` row is added to either table (falls to `default`).

### 1.4 The shared instance lock

**`lib/database/backends/sqlite/instance-lock.ts`** (891 lines; `93c5af6ad`
the only span commit): `LockFileContent.port?: number` between
`processArgv0` and `history` (`:63-73`); `resolveListeningPort()` (`:328-333`)
= `PORT` unset or `''` → `3000`; else `Number(PORT)` if integer 1..65535,
else `undefined`. `buildLockContent()` (`:351-367`) spreads `...(port !==
undefined ? {port} : {})` BEFORE `history` — key order `pid, hostname,
startedAt, lastHeartbeat, environment, processTitle, processArgv0, port?,
history`; DEBUG `Building instance lock content` `{pid, port}` (module
`database:instance-lock`). `applyListeningPort(content)` (`:339-346`) sets or
`delete`s `content.port` + DEBUG `Recorded listening port on instance lock`
`{port}`; called in `claimStaleLock` (`:552-583`) and `overrideInstanceLock`
(`:~845-870`) — both start from `{...existing}`, so when the existing lock
HAD `port` the key keeps its position, and when it did NOT (an older v4, a
CLI `db --write` claim, every v5-written lock today) the assignment
**APPENDS `port` AFTER `history`**. The re-entrant (same PID) path does not
touch `port`. The heartbeat rewrites the parsed object, so `port` survives.
Every other key the existing lock carried survives a claim/override too
(`{...existing}`).

**`packages/quilltap/lib/lock-helpers.js`** (423 lines, NEW in the span —
moved/extended): `getLockStatus(dataDir)` (absent / corrupt / active / stale /
suspect, `:60-111`); `resolveServerPort({port, instance, dataDir})`
(`:135-175`): explicit `--port` wins (`source:'flag'`); neither instance nor
dataDir ⇒ `3000` (`'default'`); else `resolveDataDirAndPassphrase({instance,
dataDir})` (throws `Specify either --instance or --data-dir, not both.`, and
an unknown instance) → `getLockStatus` → `lockPort = Number.isInteger(port) &&
1..65535`; `active` + port ⇒ `{port, source:'lock', note: \`Using port
${port} from the ${label} instance lock\`}` (`label = instanceName ||
dataDir`); otherwise 3000 with `hint = baseHint + '\nNo port found for the
${label} instance: ${why}; fell back to 3000.'` where `why` ∈ `its lock records
no port (server predates port recording)` / `it has no instance lock, so no
server appears to be running` / `its lock is stale (${reason})` / `its lock
looks suspect (${reason})` / `its lock file is unreadable`; `baseHint =
'Start the server (npm run dev) or pass --port to match a non-default port.'`.
The CLI write-lock (`acquireWriteLock`, `:306-365`) still writes NO `port`.

**`bin/quilltap.js`** `db --lock-status` (`:592-598`): after `Started:`,
`if (Number.isInteger(lock.port)) console.log(\`  Port:         ${lock.port}\`)`
(any integer, unbounded).

### 1.5 The CLI at the pin (`packages/quilltap/**`)

| File | Span commits | What moved |
|---|---|---|
| `lib/anchor-probe-command.js` | `8af764514`, `93c5af6ad`, `4ae0c4ca6` | help text (admission wording, `-d/--data-dir`, `-i/--instance`, `--passphrase` "Accepted and ignored", `--port` default from the lock); parse adds the three flags, `port: undefined`; `resolveServerPort` + DIM note to stderr + `target.hint` on connect failure; table labels `would fold` / `near-dup absorbed` / `Fold only without anchors`; row marker from `anchoredAction` / `anchorFreeAction` (`reinforce` / `absorb` / `promote-and-reinforce` fold) and the footer `= would fold into an older row only without the anchor line (reinforce ≥ X)`; prints `result.note` in YELLOW when set |
| `lib/recall-replay-command.js` | `584d28164`, `93c5af6ad` | help (stage-trace paragraph + the three flags + `--port` text); parse adds the flags; port resolution as above; a `Ring` line (`ringSource === 'debug-log'` vs the current-ring wording), a `TEXT FALLBACK (what the new path returns with no usable vector)` path when `fallbackPath` is an array, and `STAGE TRACE` lines (`printTrace`) |
| `lib/memory-diff-command.js` | `ac6348c5f`, `93c5af6ad` | port resolution; `fold` event rendering |
| `lib/memories-commands.js` | `104e986c7`, `b045dd986`, `9ec4fa2e9` | `consolidate` → `maintain`; `validate --repair [--write]` (POST `repair`); `status` asks the server's `maintenance-status`; `last demotion: within the cap…` line when `excess === 0` (`9ec4fa2e9`) |
| `lib/memories-maintain-command.js` (NEW) / `memories-consolidate-command.js` (DELETED) | `b045dd986`, `48790fb73` | the `maintain` verb (POST `maintain`, `--phases`, `--cap`, `--max`, `--threshold`, `--dry-run`) and its report renderer |
| `lib/memories-server.js` (NEW) | `b045dd986` | `postMemoriesAction` — the one POST helper; an `Unknown action` 400 gets `(is the server older than this CLI? '<action>' needs a matching Quilltap server)` |
| `lib/db-commands.js` | `b045dd986` | `db memories --tier hot\|cold` (`COALESCE(tier,'hot')`, `[tier]` heading, a `tier` column), new usage line, `--tier must be hot or cold (got '…')` |
| `lib/graph-integrity.js` | `b045dd986` | the `validate` checks behind `memories validate` |
| `bin/quilltap.js` | `93c5af6ad`, `b045dd986` | the `Port:` line; the `db --help` `memories --character` flags block (four lines) |
| `lib/completion/{bash,fish,zsh}.template` | `93c5af6ad`, `b045dd986` | `--tier` on `db`; memories `maintain` (not `consolidate`), `--phases` / `--cap` / `--repair` / `--write`, `-p`; `--instance --data-dir --passphrase` on `recall-replay` + `anchor-probe` (bash `rr_flags` / `ap_flags`) |

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 deleted | v5 twin (file:line on `a7c6f0d03`) |
|---|---|
| GET `?action=housekeep` (`handleHousekeepPreview`, (pre) `:~665-714`) | `Request::MemoryHousekeepPreview` `types.rs:1689-1700`; engine `engine.rs:3834-3853`; `memories.rs:963-977` (`hk_details`), `:985-1037` |
| POST `?action=housekeep` (`handleHousekeep`) + `housekeepingOptionsSchema` | `Request::MemoryHousekeep` `types.rs:1701-1704`; `engine.rs:3854-3857`; `memories.rs:1048-1078` (`validate_housekeep_options`), `:1083-1102`, `:1106-1164` |
| POST `?action=housekeep-sweep` | `Request::MemoryHousekeepSweep` `types.rs:1705`; `engine.rs:3858-3861`; `memories.rs:1168-1173` (calls `queue_service::enqueue_memory_housekeeping`) |
| GET/POST `?action=housekeeping-config` | `Request::MemoryHousekeepingConfig{Get,Set}` `types.rs:1707-1712`; `engine.rs:3862-3872`; `memories.rs:1180-1203`, `:1228-1267` (`validate_housekeeping_config`), `:1302-1355`; reads `chat_settings::find_auto_housekeeping_settings_by_user_id` (`db/chat_settings.rs:1732-1760`) |
| GET/POST `?action=extraction-concurrency` (+ `get/setMemoryExtractionConcurrency`, `setMemoryExtractionConcurrencyOverride`) | `Request::MemoryExtractionConcurrency{Get,Set}` `types.rs:1725-1731`; `engine.rs:3889-3902`; `memories.rs:1477-1504`; `db/instance_settings.rs:527` (`KEY_MEMORY_EXTRACTION_CONCURRENCY`), `:534-558` |
| POST `?action=consolidate` (`handleConsolidate`, `consolidateSchema`) | `Request::MemoryConsolidate` `types.rs:2873-2880`; `engine.rs:5614-5617`; `memory_consolidation.rs:41-45` (consts), `:137-142`, `:148-188`, `:195-289` |
| `system/tools` `memory-dedup-preview` / `memory-dedup` (+ `lib/tools/memory-dedup.ts`) | `Request::MemoryDedup{Preview,Run}` `types.rs:1279-1297`; `engine.rs:3190-3220`; `memory_maintenance.rs:31-67` (`memory_dedup`); web `system_data_routes.rs:79` + `:95` (map entries), `:373-386`, `:558-572`; service `services/memory_dedup.rs` (whether the fold-near-duplicates phase reuses it is the maintenance sibling's call) |
| `generateEmbeddingsSchema.batchSize` | `Request::MemoryGenerateEmbeddings.batch_size` `types.rs:1758-1764`; `engine.rs:3944-3958`; `memories.rs:1843-1920` (the 1..50 check `:1856-1860`, passed to the service `:1900-1907`) |
| `INTER_CHARACTER_MEMORY` in regenerate-all's clear list | `memories.rs:1589` |
| `INTER_CHARACTER_MEMORY` estimate + name rows (`system/tools`) | `api/system_data.rs:159-161`, `:182` (the `JOB_TYPES` row `:639` is the job-type sibling's) |
| Item GET access stamp (bug 219) | `memories.rs:396-407` (the `update_access_time` write) |
| `.partial()` settings parses (bug 244) | `memory_consolidation.rs:74-98`, `:111-134` (the faithful clobber, R-A); the table `PARTIAL_DEFAULTS_CLOBBER` in `memory_consolidation_routes_equivalence.rs:14-21`, `:355-400` |
| `chat_settings.autoHousekeepingSettings` / `memoryExtractionLimits` (schema keystone) | `db/chat_settings.rs` (the 18 sites: `:1053-1055`, `:1118-1119`, `:1408-1409`, `:1496-1500`, `:1732-1790`, test DDLs) — keystone; `almanack/phase3_ledgers.rs:779-786` reads the housekeeping knobs (Almanack sibling) |

---

## 3. The behaviour deltas a port must hit (numbered)

Each: v4 site (pin) → v5 site (`a7c6f0d03`).

1. **Retire the five POST + three GET memories actions** (`housekeep` ×2,
   `housekeep-sweep`, `housekeeping-config` ×2, `consolidate`,
   `extraction-concurrency` ×2). v4 `route.ts:199-214`, `:254-273` → v5 the §2
   rows. The answer a retired verb gives on v5's wire is a C1 decision (§7.3,
   §9 O-1).
2. **Retire the `system/tools` memory-dedup pair.** v4 `system/tools/route.ts`
   (the two map entries gone) → v5 `system_data_routes.rs:79`, `:95` (remove
   from `TOOLS_GET_ACTIONS` / `TOOLS_POST_ACTIONS` — the lists are v4's
   `availableActions`, so a retired name must LEAVE them), `:373-386`,
   `:558-572`; `memory_maintenance.rs:31-67`; engine `:3190-3220`. After
   removal `?action=memory-dedup` answers v4's `Unknown action: memory-dedup`
   envelope via `dispatch_required_action` (the REST edge exists, so this one
   is byte-comparable). `dispatch_memory_maintenance` (`:229-245`) stays for
   the summaries regen.
3. **NEW `maintain`** (`route.ts:736-817`) → new `api/memory_maintenance.rs`
   handler (or a new `api/memories_maintenance.rs`), new variant + engine arm
   (§7.3). Hit: the `withImpliedPhases` expansion, the dry-run 400 for
   consolidation-without-character, the 10/40 clamp on DRY runs only, the
   unclamped live `settings` passthrough, the archived-character filter on a
   user-wide dry run, the three body shapes, the four lines.
4. **NEW `maintenance-status`** (`:819-833`) incl. the empty-`characterId`
   quirk and the seeded in-memory ledger semantics (the ledger itself is the
   maintenance sibling's).
5. **NEW `repair`** (`:835-858`) incl. `dryRun` default `false` (a bare POST
   writes), the scoped vs instance-wide `missingEmbeddings`, no fallback on
   the scan, the report key order.
6. **NEW `maintenance-config` GET/POST** (`:864-899`) over a NEW instance
   setting `memoryMaintenance` (`instance-settings/index.ts:266-282`): the
   phases-merge, the overrides-replace, unknown keys stripped, the 100..100000
   override bound, the INFO's `capOverrides` COUNT.
7. **Manual create** (`:94-116`, `:506-638`) → v5 `memories.rs:461-558`:
   (a) drop `source` / `skipGate` from the bag (`CreateBag` `:461-482`,
   `default_source` `:486-488`) — the row is always MANUAL; (b) accept
   `projectId` / `kind` / `occurredAt` (ISO with offset, nullable) /
   `entities` (≤12, each non-empty); (c) the 404 ORDER character → project →
   chat, and `projectId` derived from the chat; (d) TRIM `content`/`summary`
   (stored trimmed; blank → 400 Validation error); (e) refusal
   envelope = `Validation error` + `details` (v5 today: `bad_request("Invalid
   memory body: <serde>")` `:500-503` — a divergence the port closes); (f)
   admission via the sibling (`admitCandidate`, MANUAL provenance
   `witnessedContext:'manual'`, `pass:'manual'`, `targeting:null`,
   `when:null`); (g) the four outcome arms: 201/200 `{memory, action}`, 400
   `Memory content and summary must not be blank.`, **502** embedding (§9
   O-3), 500 `The memory was not saved.`; (h) the four DEBUG lines; (i) the
   v5 sentence `Failed to generate embedding for memory — no row was created.
   Check the configured embedding profile.` (`:533-537`) is RETIRED.
8. **Item GET stops stamping** (bug 219, `[id]/route.ts:59-66`) → v5
   `memories.rs:396-407` delete the write (the engine arm can then stay
   `ready_db`); add the DEBUG `[Memories API v1] Fetched memory (access time
   not stamped)`; the doc comment `:372-377` and `types.rs:1642` ("access-time
   bump") change.
9. **Item PUT through `updateMemory`** (`[id]/route.ts:79-142`) → v5
   `memories.rs:568-655`: re-embed on text change (engine arm `engine.rs:3809`
   moves from `ready_db` to `ready_db_and_memory_embedding` — OPTIONAL seam:
   an importance-only edit must succeed without a profile, a text edit with
   none is the 502); trim; `.parse`-throw envelope (v5 today `bad_request
   ("Invalid memory update body: <serde>")` `:600-603`); 400
   `relatedMemoryIds must name existing memories of the same character: …`;
   **502** `The memory was not saved because its new text could not be
   embedded. Could not embed the edited memory: <msg>`; WARN + DEBUG lines;
   `types.rs:1666` "(NO re-embed)" doc changes.
10. **Item DELETE through `retireMemories({reason:'item-delete'})`**
    (`[id]/route.ts:161-166`) → v5 `memories.rs:664-689`
    (`delete_with_unlink`): `retired === 0` ⇒ 404; the INFO `[Memories API v1]
    Memory deleted` `{memoryId, characterId}` (v5 logs nothing today — a
    pre-existing gap the port closes).
11. **Chat wipe spares manual / cold / digest rows** (bugs 200/201,
    `route.ts:1240-1269`) → v5 `memories.rs:697-732`: call the sibling's
    `retire_chat_memories`; body gains `sparedCount`; the DEBUG is renamed
    and gains `spared` (v5 `:723-727` logs `Deleted every memory for a chat`
    `{chatId, deleted}`, pinned by the module test at `:2272-2298`). **This is
    the LIVE-DATA HAZARD (b) — no v5 chat wipe until this lands.**
12. **`character-memory-counts`** (`:1022-1052`) → v5 `memories.rs:418-451`:
    four new keys from `countCapBreakdown` (`hotTotal`, `demotable`,
    `protected`, `cold`; `memoryCount = hotTotal + cold`), the two-key sort,
    the DEBUG line.
13. **List `minImportance` reads `reinforcedImportance ?? importance`** —
    legacy path `route.ts:373-375` (`filterImportance`) → v5 `memories.rs:192-195`
    (reads `importance`); paginated path is the repository's
    (`memories.repository.ts` `reinforcedImportance: {$gte}`) → v5
    `memories_read::find_by_character_id_paginated` (keystone/sibling).
    `tier` handling is UNCHANGED (already ported by P4.D269).
14. **Search `minImportance` default `TOOL_MIN_IMPORTANCE` (0)** and the
    recall-pipeline search (`route.ts:656-664`) → v5 `memories.rs:891-954`
    (`min_importance: bag.min_importance` → `Some(0.0)` when absent; the
    reinforced read is the recall sibling's). "Stamps nothing" is ALREADY v5
    (`memory_service.rs:539`, P4.D267) — no delta there (ledger §1 lists it;
    correct for v4, already absorbed in v5).
15. **POST `embeddings` drops `batchSize`** (`route.ts:150-152`, `:1140`) →
    v5 `memories.rs:1843-1920` (drop the 1..50 refusal `:1856-1860` — v4 now
    STRIPS the key, so `batchSize: 999` is accepted) + `types.rs:1758-1764`
    field + census row (§5.4).
16. **PUT `embeddings`'s `failed`** = rows without an embedding
    (`backfill.ts:85-101`) → v5 `memories.rs:1925-1945` +
    `memory_service::rebuild_vector_index` (index sibling).
17. **POST `backfill-embeddings`** batch-enqueues and SKIPS rows that already
    hold an active job (`backfill.ts:115-150`) → v5 `memories.rs:1750-1831`
    (per-row loop `:1795-1816`): `enqueued` changes on a re-run; the
    no-profile check stays AFTER the empty check (unchanged order).
18. **Settings POSTs through `patchSchemaOf`** (bug 244, `route.ts:173-179`,
    `:979-1019`) → v5 `memory_consolidation.rs:74-98`, `:111-134` (parse the
    `*SettingsInput` patch — `instance_settings.rs:768-800` / `:858-…` already
    exist — and write `{...current, ...patch}`); the INFO `fields` = SENT keys
    only; `tfidfClusterThreshold` in the consolidation settings (keystone/TF-
    IDF sibling owns the struct). `recall-config` / `extraction-limits-config`
    keep their `?? current` merges (no behaviour change; v5 `:1382-1433`,
    `:1446-1474` stand).
19. **regenerate-all** clears three job types, not four (`route.ts:1282-1289`)
    → v5 `memories.rs:1589` (delete the `INTER_CHARACTER_MEMORY` entry).
20. **queue-memories** (`chats/[id]/actions/memories.ts:66-113`) → v5
    `memories.rs:1969-2087`: the seat-first profile resolution, the two new
    400 sentences + the new WARN, turns from the extraction sibling's
    `list_extraction_turns` (replacing `:2026-2063`'s two hand loops).
21. **Re-attribution + message cascade through `retireMemories`** → v5
    `services/message_reattribute.rs:104` and `services/chat_admin.rs:615`
    (error lines renamed + re-keyed), `api/salon.rs:1186`
    (`delete_memories_by_source_messages_with_vectors` → retire,
    `vectorsRemoved` from the report). The chokepoint is the deletion
    sibling's; the LINE bytes are this slice's.
22. **`system/tools` tasks-queue tables** lose `INTER_CHARACTER_MEMORY` → v5
    `system_data.rs:159-161`, `:182`.
23. **Instance lock `port`** (`instance-lock.ts:63-73`, `:328-367`,
    `:552-583`, `:~845-870`) → v5 `quilltap-host/src/lock.rs:100-110`
    (`LockFileContent`), `:292-305` (`build_lock_content`), `:504-530`
    (`claim_stale_lock`), the override path, and the boot wiring that knows
    the port (`quilltap-web/src/main.rs:129-160` `args.port` →
    `production_host_config` → `HostAssembler::pre_open`
    `quilltap-host/src/host.rs:520-541`). Hit: (a) the key sits between
    `processArgv0` and `history` on a fresh write; (b) a claim/override
    REPLACES or DELETES it — and v4 APPENDS it after `history` when the
    claimed lock had none (§9 O-7); (c) **parse it LENIENTLY** — today a v4
    lock with any `port` reads fine only because the struct does not know the
    key (serde ignores unknown fields; ledger row `:605` says so); once v5
    types it, a `port` that is a string, a float or > 65535 must NOT make
    `read_lock_file` (`lock.rs:329-332`) return `None` — measured on the code:
    `acquire_instance_lock` would then `create_new`, hit `AlreadyExists`,
    re-read `None` and FAIL BOOT with `Instance lock file at … was transiently
    created by another process but could not be read. Retry acquisition.`
    (`lock.rs:540-570`), and a running v5's `heartbeat_tick` (`:662-669`)
    would read `None` as "file disappeared" and SHUT DOWN; (d) the two DEBUG
    lines.
24. **CLI `db --lock-status` `Port:` line** (`bin/quilltap.js:595-597`) → v5
    `quilltap-cli/src/db_cmd.rs:709-712` (after `Started:`), reading the Map
    (`lock_str`/`lock_num` `:445-452`): print iff the JSON number is an
    integer, rendered the JS way (`nodefmt::js_num_string`).
25. **CLI `anchor-probe` / `recall-replay` port resolution + flags**
    (`lock-helpers.js:135-175`, the two commands) → v5
    `quilltap-cli/src/anchor_probe_cmd.rs:41-95` (`Flags`, `parse_flags`,
    `port: 3000` default), `:260-300` (the connect-failure sentence + fixed
    `Start the server…` line `:300`), and `recall_replay_cmd.rs:64-160`,
    `:364-433` (`:433`); a NEW shared `resolve_server_port` in
    `quilltap-cli/src/resolve.rs` (beside `resolve_data_dir_and_passphrase`
    `:71-…`, whose both-flags sentence `:78` is v4's) and a lock read via
    `quilltap_host::lock::classify_lock_status_probed`
    (`lock.rs:814-…`, the `suspect` probe included).
26. **CLI anchor-probe rendering** (`anchor-probe-command.js:127-218`) → v5
    `anchor_probe_cmd.rs:186-252` (`print_rows`: the fold predicate over
    `anchoredAction`/`anchorFreeAction`, the new footer), `:370-400` (the
    three relabelled lines `:378`, `:385`, `:398`), plus the `note` line; the
    module doc `:1-5` ("Memory Gate").
27. **CLI recall-replay rendering** (`recall-replay-command.js:205-219`,
    `:364-380`) → v5 `recall_replay_cmd.rs:196-272` (`print_path`), the run
    tail: the `Ring` line, the `TEXT FALLBACK …` path, `print_trace` (NEW).
28. **CLI help + completion bytes**: `src/help/anchor_probe_help.txt`,
    `recall_replay_help.txt`, `db_help.txt` (lines 25-26 → the pin's four-line
    flags block), and `src/help/completion/{bash,fish,zsh}.template` re-captured
    at the pin (today md5-identical to (pre) — measured with `diff -q`).

---

## 4. v5 on `main` today — per file

- **`crates/quilltap-core/src/api/memories.rs`** (2,450 lines). Survives with
  edits: `memory_list` `:140-227` (delta 13), `memory_count_by_chat`
  `:281-294`, `memory_by_message` `:304-368` (unchanged), `memory_get`
  `:378-410` (delta 8), `memory_character_counts` `:418-451` (delta 12),
  `CreateBag` + `memory_create` `:461-558` (delta 7, a near-rewrite),
  `UpdateBag` + `memory_update` `:568-655` (delta 9, rewrite onto the
  sibling's `update_memory`), `memory_delete` `:664-689` (delta 10),
  `memory_delete_by_chat` `:697-732` (delta 11), the search family
  `:739-954` (delta 14), configs `:1363-1474` (`validate_extraction_limits_config`
  `:1271-1297` and the shared `zod_number` / `zod_safe_int` `:1207-1215`
  STAY — the housekeeping validator was their other user), regenerate/backfill
  `:1511-1831` (deltas 17, 19), `memory_generate_embeddings` `:1843-1920`
  (delta 15), `memory_rebuild_index` `:1925-1945` (delta 16),
  `chat_queue_memories` `:1969-2087` (delta 20), the anchor probe
  `:2102-2194` (route unchanged; the probe body is the admission sibling's).
  **Dies:** `:963-1173` (housekeeping), `:1180-1203`, `:1228-1267`,
  `:1302-1355` (housekeeping config), `:1477-1504` (concurrency); the imports
  `:35-44` (`housekeeping`, `memory_gate::create_memory_with_gate`,
  `delete_memories_by_chat_id_with_vectors`, `enqueue_memory_housekeeping`)
  follow. Module tests `:2212-2450` re-pin the wipe line.
- **`crates/quilltap-core/src/api/memory_consolidation.rs`** (289 lines):
  `:1-98` / `:101-134` stay (configs; bug 244 rewrites the two setters and the
  module doc `:7-17`); `:41-45`, `:137-289` (the consolidate action) DIE.
- **`crates/quilltap-core/src/api/memory_maintenance.rs`** (96 lines):
  `memory_dedup` `:31-67` DIES; `conversation_summaries_regenerate`
  `:68-96` stays (the module name now collides with the feature — §9 O-6).
- **`crates/quilltap-core/src/api/types.rs`** (6,102 lines): the retired
  variants (§2); `MemoryCreate` / `MemoryUpdate` / `MemoryGet` /
  `MemoryDeleteByChat` / `MemoryCharacterCounts` doc comments
  (`:1642-1682`, `:1658-1660`); `MemoryGenerateEmbeddings` (`:1758-1764`);
  `Response::Memory` doc (`:4555-4562`) — the housekeeping envelopes
  sentence; `Response::MemoryMaintenance` doc (`:4548-4551`) — dedup gone.
- **`crates/quilltap-core/src/api/engine.rs`** (8,560 lines): arms
  `:3190-3220` (dedup, die), `:3762-3968` (memories family; die
  `:3834-3872`, `:3889-3902`; edit `:3787-3790`, `:3809-3812`, `:3944-3958`),
  `:5614-5617` (consolidate, dies); new arms beside `:5598-5613`.
- **`crates/quilltap-core/src/db/instance_settings.rs`** (1,894 lines): the
  concurrency key + fns `:527-558` die (keystone — `move-memory-maintenance-
  settings-v1` deletes the row); NEW `memoryMaintenance` key + get/set +
  `MemoryMaintenanceSettings` (keystone or this lane — §7.1).
- **`crates/quilltap-web/src/system_data_routes.rs`**: §3.2 lines.
- **`crates/quilltap-host/src/lock.rs`** (1,716 lines): `LockFileContent`
  `:100-110`, `build_lock_content` `:292-305`, `read_lock_file` `:329-332`,
  `write_lock_file` `:336-…`, `claim_stale_lock` `:504-530`,
  `acquire_instance_lock` `:537-…` (re-entrant arm `:582-596` keeps `port`),
  the override fn, `heartbeat_tick` `:662-…` (round-trips the struct — keeps
  `port` once typed). CLI write-lock `:858-1010` writes NO port (v4 parity).
- **`crates/quilltap-cli/src/`**: `main.rs` (`SUBCOMMANDS` unchanged; `memories`
  / `memory-diff` stay `not_yet_available`), `anchor_probe_cmd.rs` (438),
  `recall_replay_cmd.rs` (627), `db_cmd.rs` (lock-status `:680-760`; the `db
  memories` verb is recognized-but-loud — `DB_VERBS` `:26-39`, not ported),
  `resolve.rs` (314), `help/*.txt`, `help/completion/*.template`.

---

## 5. Harness

### 5.1 Families that move (tier; RED vs IMPORT-DEAD at the pin)

| Family (Rust test + oracle case) | Tier | At the pin |
|---|---|---|
| `memories_routes_equivalence.rs` (1,524) + `cases/memories-routes.test.ts` (509) + `cases/memories-config.test.ts` (704), fixture `crates/quilltap-web/tests/fixtures/memories-{main,mount}.db` built by `fixtures/build-memories-web-fixture.ts`, widened by `fixtures/apply-memory-tiers-migration.ts` | 2 (+ live TF-IDF provider) | **RED**, not import-dead (the oracle imports only the route modules + `@/lib/{database,repositories,embedding/embedding-service,embedding/vector-store,instance-settings,startup}` — all present at the pin). Rows: `get` (no stamp — dump `lastAccessedAt`), `create_insert` (+`action:'insert'`, 201), `create_near_duplicate` (sends `source:'AUTO'` — now STRIPPED; MANUAL never folds into an AUTO row ⇒ likely `insert-linked` at 201 — re-measure), `update` (summary edit ⇒ re-embed via a provider; `reinforcedImportance` recomputed), `delete` (retire chokepoint), `delete_by_chat` (+`sparedCount`, new line), `character_counts` (+4 keys, new sort), `search_min_importance` (reinforced read), `backfill_start` (batch dedupe), `regenerate_all` (3 types); the Rust housekeeping block `:841-912` and config rows `:914-~1030`, `:~1215-1230` (concurrency) DIE (the oracle cases `housekeep_*` ×4 `memories-config.test.ts:330-388`, `housekeeping_config_*` ×6 `:389-475`, `extraction_concurrency_*` ×2 `:654-664` now answer 400 `Unknown action`). The fixture's `chat_settings` columns + `vector_*` tables NARROW in place through v4's real migrations (keystone). |
| `memory_consolidation_routes_equivalence.rs` (442) + `cases/memory-consolidation-routes.test.ts` (210) | 2/3 | **RED**: the `consolidate` cases → 400 `Unknown action`; `PARTIAL_DEFAULTS_CLOBBER` (`:14-21`, `:355-400`) TRIPS BY DESIGN on all three rows (ledger §1 `b045dd986`); `fields` lists sent keys; consolidation settings gain `tfidfClusterThreshold`. The oracle's `CAPTURED` set (`:63-70`) names four retired lines. |
| `memory_dedup_equivalence.rs` (390) + `cases/memory-dedup.test.ts` (302) | 2 | **IMPORT-DEAD**: imports `@/lib/tools/memory-dedup` (`deduplicateAllMemories`) and mocks `@/lib/memory/memory-gate` (`:100`) — both deleted. Retire (or re-aim at the fold phase — maintenance sibling). |
| `system_body_guards_equivalence.rs` (quilltap-web tests) | 2 | **RED**: its `memory-dedup` rows (threshold coercion) — v4 answers `Unknown action` now. |
| `settings_routes_equivalence.rs` + `cases/settings-routes.test.ts`; `chat_settings_tier2_equivalence` | 2 | **RED** (keystone-owned): the chat-settings READ body loses `autoHousekeepingSettings` + `memoryExtractionLimits`. |
| `recall_replay_equivalence.rs` + `cases/recall-replay.test.ts` | 2/3 | wire UNCHANGED; result keys move (recall sibling). |
| `anchor_gate_probe_tier3_equivalence.rs` + `cases/anchor-gate-probe-tier3.test.ts` | 3 | result gains `anchoredAction` / `anchorFreeAction` / `lexicalProfile` / `note` (admission + TF-IDF siblings); route unchanged. |
| `memory_settings_schemas_equivalence.rs` + `cases/memory-settings-schemas.ts` | 1 | **RED**: `tfidfClusterThreshold`; `maxPerHour.max(10_000)`; NEW rows for `MemoryMaintenanceSettingsSchema` + the patch parse (bug 244) belong here. |
| `instance_settings_json_warns_equivalence.rs` | 2 | gains the `memoryMaintenance failed to parse` WARN row. |
| `quilltap-cli/tests/cli_differential.rs` (Tier R, 5,426) | R | **RED**: `db help` (`:2173`), the `anchor-probe` help/table cases (`:5151-5290`), the `recall-replay` help/table cases (`:4652-…`), `completion bash/zsh/fish` (`:4430-4435`), the lock-status cases (`:2549-2940`) unaffected unless a plant carries `port`. |
| `quilltap-cli/tests/completion_behavior.rs` | unit | **RED**: `bash_completes_the_p4d269_template_lines` (`:573-…`) asserts anchor-probe offers EXACTLY `--help --json --limit --port` — the pin adds `--instance --data-dir --passphrase`; `fish_and_zsh_carry_the_p4d269_template_lines` (`:635-…`) stays GREEN (the pin's fish template still carries `Probe anchor-line effect on Memory Gate`, `fish.template:64`; bash `vf_anchor_probe` is untouched by the span). |
| `quilltap-web/tests/dispatch_wrong_type_census.rs` | census | **RED** (count + rows) — §5.4. |
| `quilltap-harness/tests/web_edge_body_parse_guard.rs` | census | **RED**: `COLLAPSE_CENSUS` `system_data_routes.rs` 13 → 12 (the `threshold` `and_then(Value::as_f64)` site `:570` leaves; the adjudication text `:107-…` names it). |
| `quilltap-host` lock unit tests (`lock.rs` `#[cfg(test)]`) + `tests/host_lock_ordering.rs` | unit | grow the four port rows (v4 `instance-lock.test.ts` `listening port` describe). |

### 5.2 Corpus rows to ADD (proposed)

- **memories-routes / memories-config** (tier 2): `create_manual_trims`
  (`'  x  '` stored `'x'`), `create_blank_after_trim` (400 Validation error +
  details), `create_project_derived_from_chat`, `create_project_explicit_wins`,
  `create_missing_project_404`, `create_missing_chat_404`,
  `create_kind_occurred_entities`, `create_source_skipgate_stripped` (+ the
  DEBUG), `create_embedding_failed_502` (a refusing provider; no row written —
  `check_tables`), `create_reinforce_200` (a MANUAL twin of a MANUAL row),
  `get_does_not_stamp`, `update_text_reembeds`, `update_importance_only_no_profile`
  (200), `update_text_no_profile_502`, `update_invalid_related_400`,
  `update_blank_after_trim_400`, `delete_by_chat_spares_manual_cold_digest`
  (plant one of each), `character_counts_breakdown` (hot AUTO / hot MANUAL /
  cold / NULL-tier rows), `embeddings_batchsize_ignored`,
  `rebuild_index_failed_counts_unembedded`, `backfill_start_skips_queued`,
  `retired_actions_unknown` (all eight retired verbs → v4's 400 body; the v5
  side per the §9 O-1 ruling), `maintenance_config_get_default`,
  `maintenance_config_phases_merge`, `maintenance_config_overrides_replace`,
  `maintenance_config_strips_retired_merge_knobs`,
  `maintenance_config_override_below_100_400`, `maintenance_config_corrupt_row_warn`,
  `repair_dry_run_scoped`, `repair_bare_post_is_live` (plant a dangling
  `relatedMemoryIds`, a digest missing ONE member, a digest missing ALL, an
  orphan `supersededById`, an unembedded row; dump the rows + the queued
  `EMBEDDING_GENERATE` jobs), `repair_no_profile_skipped_reason`,
  `repair_unknown_character_404`, `repair_bad_body_400`,
  `maintenance_status_unknown_404`, `maintenance_status_empty_param_lists_all`,
  `maintain_live_one` / `maintain_live_all` (dump the `MEMORY_MAINTENANCE`
  rows, mask ids), `maintain_dry_llm_free_user_wide` (archived character
  skipped), `maintain_dry_consolidate_without_character_400`,
  `maintain_unknown_phase_400`, `maintain_implied_mirror` (payload
  `phases:['consolidate','mirror']`).
- **`maintain` dry runs that include `consolidate`** are tier 3 (mocked LLM —
  the `memory_consolidation_routes` R-E runner precedent); LLM-free dry runs
  are tier 2.
- **settings patch rows** (bug 244) replacing `PARTIAL_DEFAULTS_CLOBBER`: a
  tuned stored value survives a one-key POST, BOTH sides; `fields` = the sent
  key.
- **system/tools**: `memory_dedup_preview_unknown_action`,
  `memory_dedup_unknown_action` (byte-compared — the REST edge exists).
- **CLI Tier R**: `db --lock-status` with `port: 3005`, with `port: "3005"`
  (no line), with `port: 3000.5` (no line); `anchor-probe <id> --instance X`
  / `--data-dir D` resolving a live lock's port (note line on stderr), a lock
  without port (hint tail on a dead port), `--instance` + `--data-dir`
  (`Error: Specify either --instance or --data-dir, not both.` exit 1),
  `--passphrase p` swallowed; the same set for `recall-replay`; the new
  anchor-probe table with `anchoredAction` / `note`; the recall-replay ring /
  fallback / trace tail.

### 5.3 Closest recipes to copy

- Route families: `memory_consolidation_routes_equivalence.rs:22-40` (the
  `/tmp` mirror `cp -R $V5W/harness/oracle/*`, `QT_FIXTURE_MEM_MAIN/MOUNT`,
  jest `--roots`, the `CAPTURED` line-capture idiom `cases/memory-consolidation-
  routes.test.ts:63-70`, `:132-142`), and `memories-routes.test.ts:14-26`.
  **Regen from a pinned worktree at `64743ee13` with its own `npm ci
  --offline`** (ledger §1 "Regen"; the sweep driver needs
  `/opt/homebrew/bin/python3.13` and the spaceless `--v5w
  ~/source/quilltap-v5` — memory notes).
- The v4 job host claims `MEMORY_MAINTENANCE` jobs mid-case — hold it off
  with `__quilltapJobHost.shuttingDown` (memory note `v4-oracle-job-host-and-
  log-capture.md`); the ledger is a `processSingleton` — `resetMaintenance
  LedgerForTests()` between cases.
- Tier R: `cli_differential.rs:316-400` (the two-dialect wire stub). **For the
  lock-port cases the lock's `pid` must be ALIVE and node-shaped on BOTH
  sides** (`lock-helpers.js:20-47` and v5 `verify_pid_is_quilltap`
  `lock.rs:764-…` both match `/node|electron|quilltap|next-server/`); the stub
  runs in the test process (name `cli_differential-<hash>` — fails the
  probe ⇒ `suspect` ⇒ no port). Plant the lock with the PID of a spawned
  `$QT_NODE -e 'setInterval(()=>{},1e9)'` and keep the in-process stub on
  the planted port.
- Lock unit rows: v4 `__tests__/unit/lib/database/instance-lock.test.ts`
  `describe('listening port')` (four cases, `+43` lines in the span).

### 5.4 Dispatch census arithmetic (`dispatch_wrong_type_census.rs`)

`CENSUS` rows that LEAVE: `MemoryDedupPreview.threshold` (`:920-925`),
`MemoryDedupRun.threshold` (`:926-935`), `MemoryHousekeepPreview.{max_memories,
max_age_months, min_importance, merge_similar}` (`:1089-1117`),
`MemoryExtractionConcurrencySet.concurrency` (`:1118-1127`),
`MemoryGenerateEmbeddings.batch_size` (`:1138-1144`) — **−8 rows**.
`EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE` (`:2738`, 479): −1
(`MemoryHousekeepPreview.character_id`), +1 per new typed `*_id` field — with
the §7.3 spellings (`MemoryMaintenanceStatus.character_id` typed; the three
body-carrying verbs raw) it lands back at **479**. Measure red first (the
house rule: "Measured by running the test").

---

## 6. v4 jest tests at the pin covering the slice

| Test | Cases | Covers |
|---|---|---|
| `__tests__/unit/api/memories-consolidation-actions.test.ts` | 21 | `maintain` (dry/live/one/all/unknown phase/implied mirror/bug 223 threshold carry), the retired aliases → 400, settings round-trips + bug 244, list `tier`/`source`, `character-memory-counts` (bug 210), `maintenance-status`, `repair` |
| `__tests__/unit/api/memories-manual-create.test.ts` | 13 | bug 218 (MANUAL forced, skipGate ignored), project derivation / explicit / 404s, `kind`/`occurredAt`/`entities`, 201 for `insert-linked`, the 502 + the `empty` 400 (bug 236), trim, malformed body; item GET no stamp (bug 219) |
| `__tests__/unit/api/memories-edit-embedding-failure.test.ts` | 1 | PUT 502 + no refit |
| `__tests__/unit/api/memories-dry-run-project.test.ts` | 6 | the chat dry-run route's project (extraction sibling) |
| `__tests__/unit/lib/memory/memory-graph-repair.test.ts` | 6 | `repairMemoryGraph` |
| `__tests__/unit/lib/memory/maintenance/ledger-and-triggers.test.ts` | 21 | the ledger (sibling; the status shape) |
| `lib/schemas/__tests__/patch-schema.test.ts` | — | `patchSchemaOf` |
| `__tests__/unit/lib/database/instance-lock.test.ts` | 51 (4 new) | `listening port` |
| `packages/quilltap/lib/__tests__/server-port-resolution.test.js` | 9 (NEW) | `resolveServerPort`, `--passphrase` swallowing, `port` undefined unless given |
| `packages/quilltap/lib/__tests__/memories-maintain-command.test.js` | 18 (NEW) | the `maintain` verb (unported in v5) |
| `__tests__/unit/app/api/v1/messages/[id]/route.test.ts` | (−5 lines) | cascade through retire |

---

## 7. Cross-lane meeting points

### 7.1 NEEDS from the schema keystone / siblings (proposed Rust spellings)

| Need | v4 | Proposed v5 |
|---|---|---|
| `memoryMaintenance` instance setting | `get/setMemoryMaintenanceSettings` (`instance-settings/index.ts:266-282`), schema `settings.types.ts:131-187` | `db::instance_settings::{MemoryMaintenanceSettings, MemoryMaintenancePhaseToggles, MemoryMaintenanceSettingsPatch, get_memory_maintenance_settings(&Connection) -> MemoryMaintenanceSettings, set_memory_maintenance_settings(&Connection, &Value) -> Result<MemoryMaintenanceSettings, DbError>, parse_memory_maintenance_config(&Value) -> Result<MemoryMaintenanceSettingsPatch, Vec<ZodIssue>>}`, `KEY_MEMORY_MAINTENANCE = "memoryMaintenance"` — keystone (the migration writes it) |
| patch parses (bug 244) | `patchSchemaOf` | `db::instance_settings::{parse_memory_consolidation_patch, parse_memory_extraction_mode_patch}` returning the existing `*SettingsInput` structs (`instance_settings.rs:768`, `:858`) |
| cap breakdown | `memories.countCapBreakdown` (`memories.repository.ts:1005-1028`; `HOT_TIER_SQL = COALESCE(tier,'hot') = 'hot'`; demotable = hot AND `source = 'AUTO'`; safeQuery fallback zeros + `Error counting memories against the housekeeping cap`) | `db::memories_read::count_cap_breakdown(&Connection, &str) -> Result<CapBreakdown { hot_total, demotable, protected, cold }, DbError>` |
| dangling scan | `memories.findDanglingLinks` (`:1290-1342`, NO fallback, DEBUG `Scanned for dangling memory links`) | `db::memories_read::find_dangling_links(&Connection, Option<&[String]>) -> Result<DanglingMemoryLinks, DbError>` |
| unembedded rows | `findIdsWithoutEmbedding`, `countWithoutEmbedding` | existing `memories_read::{find_ids_without_embedding, count_without_embedding}` (`db/memories_read.rs:1149`, `:1137`) — confirm the `memories.embedding`-only read after the vector retirement |
| recent jobs by type | `backgroundJobs.findRecentByType(type, 100)` | `db::background_jobs::BackgroundJobsRepository::find_recent_by_type(&self, &str, usize)` (maintenance sibling consumes) |
| admission | `admitCandidate`, `admissionInserted`, `MemoryCandidateSchema`, `updateMemory` | admission sibling: `services::memory_admission::{admit_candidate(db, provider, &MemoryCandidate, AdmitTiming{occurred_at, narrative_time}, &AdmitOptions{user_id}) -> Result<AdmissionOutcome, DbError>, AdmissionActionKind::{Insert, InsertLinked, Reinforce, Absorb, PromoteAndReinforce, Reject}, admission_inserted(&AdmissionOutcome) -> bool, RejectReason::{Empty, EmbeddingFailed}}` with `AdmissionActionKind::as_str()` = the lowercase wire string; `services::memory_admission::update_memory(db, provider: Option<&P>, character_id, memory_id, &MemoryEditPatch, user_id) -> Result<UpdateMemoryResult, DbError>` with `UpdateMemoryResult::{Ok(Value), NotFound, InvalidRelated(Vec<String>), EmbeddingFailed(String)}` |
| retirement | `retireMemories`, `retireChatMemories`, `scrubDanglingMemoryLinks` | deletion sibling: `services::memory_retire::{retire_memories(db, &[String], RetireOptions{reason, keep_manual}) -> Result<RetireReport, DbError>, retire_chat_memories(db, &str) -> Result<RetireReport, DbError>, scrub_dangling_memory_links(db, &DanglingMemoryLinks) -> Result<DanglingScrubReport, DbError>}`, `RetireReason::{ItemDelete, ChatWipe, MessageCascade, MessageReattribution, …, GraphRepair}` |
| repair | `repairMemoryGraph` | `services::memory_repair::repair_memory_graph(db, user_id, Option<&str>, dry_run) -> Result<MemoryGraphRepairReport, DbError>` with `to_json()` in v4 key order — **this slice may own it** (98 lines, its only caller is the route; §9 O-5) |
| maintenance | `runMaintenance`, `withImpliedPhases`, `MAINTENANCE_PHASES`, `getMaintenanceLedgerStatus`, `enqueueMemoryMaintenance{,ForUser}` | maintenance sibling: `services::memory_maintenance::{MaintenancePhase (serde kebab), MAINTENANCE_PHASES, with_implied_phases(&[MaintenancePhase]) -> Vec<MaintenancePhase>, MaintenanceRequest, MaintenanceRunSettings, MaintenanceReport::to_json, maintenance_runner() -> Option<Arc<dyn MaintenanceRunner>>}` (the process-global host-installed runner, the `consolidation_runner()` R-E precedent `memory_consolidation.rs:236-241`), `services::memory_maintenance_ledger::get_maintenance_ledger_status(db, Option<&str>) -> Result<Vec<Value>, DbError>`, `services::queue_service::{enqueue_memory_maintenance(db, user_id, &MemoryMaintenancePayload) -> Result<String, DbError>, enqueue_memory_maintenance_for_user(db, user_id, &MemoryMaintenancePayload) -> Result<Vec<String>, DbError>}` |
| backfill batch | `enqueueMissingEmbeddings` → `enqueueEmbeddingGenerateMany` | index sibling: `services::memory_index_backfill::enqueue_missing_embeddings(db, user_id, Option<&str>, limit) -> Result<EnqueueMissing::{Ok{enqueued, remaining}, NoProfile{remaining}}, DbError>` (used by BOTH `backfill-embeddings` and repair) |
| extraction turns / profile | `listExtractionTurns`, `pickExtractionConnectionProfileId` | extraction sibling |

### 7.2 PROVIDES to siblings

- The settings READ for `maintenance-config` is the same object the
  maintenance job reads (`resolveMemorySettings`, `lib/memory/settings.ts`) —
  one getter, owned once.
- The SPA consumes every wire spelling in §7.3 (SPA sibling).
- The CLI's `resolve_server_port` (§7.4) is reusable by a later `memories`
  / `memory-diff` port.
- The lock `port` (§7.5) is read by v4's CLI against a v5 server.

### 7.3 Proposed C1 wire (`api/types.rs` `Request` / `Response`)

**New `Request` variants** (beside the P4.D265 block `:2849-2880`). Bodies
ride RAW so the handlers run ported Zod (the P4.D279 "one flattened body"
precedent; keeps the census unmoved):

```rust
/// v4 `POST /api/v1/memories?action=maintain` (`route.ts:736-817`,
/// `maintainSchema` :129-141) — `{success, dryRun:true, report}` |
/// `{success, dryRun:true, reports}` | `{success, dryRun:false, jobId, jobIds}`.
MemoryMaintain { body: serde_json::Value },
/// v4 `GET ?action=maintenance-status[&characterId=]` (`:819-833`) —
/// `{success, ledger}`. `character_id` is v4's FIRST query occurrence, kept
/// verbatim (`Some("")` = present-empty, which v4 reads as "all").
#[serde(rename_all = "camelCase")]
MemoryMaintenanceStatus { #[serde(default)] character_id: Option<String> },
/// v4 `POST ?action=repair` (`:835-858`, `repairSchema` :144-148) —
/// `{success, report}`.
MemoryRepair { body: serde_json::Value },
/// v4 `GET ?action=maintenance-config` (`:864-870`) — `{success, settings}`.
MemoryMaintenanceConfigGet,
/// v4 `POST ?action=maintenance-config` (`:872-899`) — `{success, settings}`.
MemoryMaintenanceConfigSet { config: serde_json::Value },
```

**Changed:** `MemoryGenerateEmbeddings { character_id }` (drop `batch_size`;
serde ignores an unknown key, as Zod strips it); `MemoryCreate` /
`MemoryUpdate` / `MemoryGet` / `MemoryDeleteByChat` / `MemoryCharacterCounts`
keep their shapes (bodies change only inside `Response::Memory`).

**Responses:** reuse `Response::Memory(Value)` for all five new verbs (the
family carrier, `types.rs:4562`; its doc gains the maintenance envelopes);
`Response::MemoryMaintenance` keeps only the summaries-regen bodies (rename
optional, §9 O-6). Success status stays 200 on the dispatch edge (the
`ChatPostOffice` / `ChatAdmin` 201 precedent, `types.rs:4689-4693`,
`:4711-4713`) — the create's 201-vs-200 is carried by `action` in the BODY.

**Retired** (delete): `MemoryHousekeepPreview`, `MemoryHousekeep`,
`MemoryHousekeepSweep`, `MemoryHousekeepingConfigGet`,
`MemoryHousekeepingConfigSet`, `MemoryExtractionConcurrencyGet`,
`MemoryExtractionConcurrencySet`, `MemoryConsolidate`, `MemoryDedupPreview`,
`MemoryDedupRun`. **What a retired verb answers** — the precedent, measured:
v5's memories family has NO REST edge (dispatch only), and `dispatch.rs:69-78`
answers an undecodable body `400 {"kind":"badRequest","message":"Invalid
request: unknown variant \`memoryHousekeep\`, expected one of …"}`; the
P4.6ae order (`p4.6ae-files-family-server.md:50`: "they 400 as unknown
variants until ported") and the SPA's realtime/e2e probes
(`status-log.md:18950`, `:29705` — "unknown variant" treated as not-ready)
rely on exactly that arm. No prior round deleted a variant because v4
RETIRED its action (the P4.D279 deletions were repository shims; P4.D251
kept a retired body KEY as ignored, not a verb). The REST-edged retirement
(`system/tools` memory-dedup) answers v4's own `Unknown action` envelope
through `query::dispatch_required_action` once the names leave the two
action lists. §9 O-1 asks the ruling.

**Engine arms** (`engine.rs`): `MemoryMaintain` → `ready_db()` for the live
path, the dry run through the installed runner (refusal when absent, AFTER
the 400/404 arms — R-E); `MemoryMaintenanceStatus` → `ready_db()`;
`MemoryRepair` → `ready_db()` (the embedding jobs are queued, not run);
`MemoryMaintenanceConfig{Get,Set}` → `ready_db()`; `MemoryUpdate` →
`ready_db_and_memory_embedding()` (`:6758`); `MemoryCreate` stays
`ready_memory_embedding()` (`:6771` — but see §9 O-4).

### 7.4 CLI

`quilltap-cli/src/resolve.rs`:
`pub struct ServerTarget { pub port: i64, pub source: PortSource, pub note: Option<String>, pub hint: String }`,
`pub enum PortSource { Flag, Lock, Default }`,
`pub fn resolve_server_port(port: Option<i64>, instance: &str, data_dir: &str) -> Result<ServerTarget, String>`
(errors are v4's `Error: <message>` + exit 1 at the caller). Both
`anchor_probe_cmd::Flags` and `recall_replay_cmd::Flags` gain `data_dir`,
`instance`, `port: Option<i64>`, and swallow `--passphrase <v>`.

### 7.5 Lock

`LockFileContent` gains, between `process_argv0` and `history`:
`#[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "lenient_port")] pub port: Option<u16>`
where `lenient_port` maps any non-integer / out-of-range value to `None`
(never a parse error — delta 23c). `build_lock_content(port: Option<u16>)`;
`claim_stale_lock` / override set or clear it; a `HostConfig.listening_port:
Option<u16>` threaded from `quilltap-web`'s `args.port`; the Tauri host passes
`None` (§9 O-8).

---

## 8. SPA / help / docs / CLI touch points

- **SPA** (sibling-owned; the wire it calls): `apps/web/src/app/memory/
  memory.api.ts:111` (`memoryHousekeepPreview`), `:120` (`memoryHousekeep`),
  `:149`/`:158` (housekeeping config), `:173` (`memoryHousekeepSweep`),
  `:260` (`memoryConsolidate`); `core/core-contract.ts:6040-6060`,
  `:6124-6141` (incl. the concurrency pair), `:6294-6320`, `:8728-8740`
  (dedup); `core/core-client.ts` (dedup); `memory/housekeeping-dialog.ts`;
  `screens/settings/memory/{memory-housekeeping-card,memory-consolidation-card,
  memory-consolidation-report-dialog,memory-dedup-card,memory-tab}.ts`;
  `e2e/settings-memory-flow.spec.ts`. The create caller (`memory.api.ts:80`)
  gains `action`; v4's Maintenance card / dialog / repair button are the SPA
  sibling's.
- **help/** (`b045dd986`): `memory-maintenance.md` NEW,
  `memory-housekeeping.md` + `memory-consolidation.md` DELETED,
  `cli-memories.md` and the memory pages edited — the vendor sibling.
- **docs/v4/developer/API.md** (+210/−112 in the span) and **CLI.md** (+27) —
  the memories action table and the CLI flags; re-vendor with the round.
- **CLI help texts / completion templates** — delta 28 (Tier R pins them).

---

## 9. Open points for the order

**Corrections to the ledger / brief (hunks win):**

- **C-1** — "search `includeCold` stamping nothing" is NOT a v5 delta: v5's
  API search already stamps nothing (`memory_service.rs:539`, P4.D267, and
  `memories.rs:885-890`'s comment). The pin's live search delta is only the
  `TOOL_MIN_IMPORTANCE` default + the recall-pipeline internals (sibling).
- **C-2** — "list `tier`" is not new in the span: `tierParam` reads
  identically at (pre) `:302-303` and the pin `:317-318`; v5 honours it
  (P4.D269). The list's live delta is `filterImportance` (delta 13).
- **C-3** — ledger row `:605` says v5 "reads a v4 lock with `port` fine but a
  v5 rewrite drops the key". True today; but the obvious fix (a typed
  `port: Option<u16>`) turns any malformed `port` into a WHOLE-LOCK parse
  failure — which every acquire path treats as "no lock". The order must
  require a lenient read (delta 23c).
- **C-4** — the `housekeep` GET preview is retired too (the brief's list names
  the POST verbs): eight dispatch names leave in all (§0).

**Rulings / decisions needed:**

- **O-1 (C1)** — the answer for a RETIRED dispatch verb: (A) delete the
  variants — serde's `Invalid request: unknown variant …` 400 (the standing
  dispatch arm; no new code; differs from v4's `Unknown action: <x>` +
  `availableActions` body); or (B) keep ten unit-ish variants as typed
  refusals answering v4's exact envelope with the memories GET/POST
  `availableActions` lists (more code, byte-faithful, and every future
  retirement inherits the pattern). Recommendation for the planner: (A) for
  the dispatch-only memories verbs, recorded as a divergence row; the
  `system/tools` pair gets v4's envelope for free.
- **O-2** — the validation envelope for the NEW verbs: `Response::
  validation_error(zod_issue_details(...))` WITH `details` (the
  `memory_search` precedent, `memories.rs:897-900`) or the memories config
  "house class" message-only `Validation error` (R-K, `memory_consolidation.rs:
  18-19`). v4's CLI (`memories-server.js:71-77`) PRINTS `details`; a v5
  server answering without them changes what `quilltap memories maintain`
  shows on a bad `--phases`. Recommend `details` for `maintain` / `repair` /
  `maintenance-config` (ported issues), and decide whether the create / PUT
  rewrites adopt them too (closing v5's `Invalid memory body:` sentence).
- **O-3 (C1)** — the two **502**s (create, PUT). The dispatch boundary has no
  502 `ErrorKind` (`dispatch.rs:19-36`); the house carry is `ErrorKind::
  Internal` + `details` with the web edge restoring 502 — but the memories
  family HAS no web edge. Options: (a) Internal + `details:{status:502}` →
  HTTP 500 on `/api/dispatch`, SPA reads `details`; (b) a new
  `ErrorKind::BadGateway` → 502 (touches `dispatch.rs:status_for`, the SPA
  error mapper, every harness `status_of`). A C1 ruling either way;
  recorded divergence under (a).
- **O-4** — `MemoryCreate`'s engine arm uses the REFUSING
  `ready_memory_embedding()`: with no embedding profile v5 refuses before
  validation/404, where v4 validates, 404s, then admits and answers the
  **502**. Moving it to the optional seam makes the order v4's. Same
  question for `memory_search`.
- **O-5** — who owns `repairMemoryGraph`: this lane (its only caller is the
  route) or the deletion sibling (it calls `scrubDanglingMemoryLinks` +
  `retireMemories` with `graph-repair`). Splitting means a HANDOFF on the
  scrub fn.
- **O-6** — `api/memory_maintenance.rs` today holds dedup + summaries regen;
  after dedup dies the name collides with v4's "memory maintenance" feature.
  Rename to `api/conversation_summaries.rs` and put the new verbs in
  `api/memory_maintenance.rs`? (`Response::MemoryMaintenance` likewise.)
- **O-7** — the lock key ORDER on a claim of a port-less lock (v4 appends
  `port` after `history`, `{...existing}` + assignment). A serde struct cannot
  express "after history only when it was absent". Carry it (a
  `port_after_history: bool` set from the parsed input, or a raw
  `serde_json::Map` passthrough that also keeps unknown keys — v4's
  `{...existing}` keeps them, v5 drops them today) vs rule it a cosmetic
  divergence (JSON readers are order-blind). Needs a ruling; the CLI
  differential's lock-status cases print fields by name, so Tier R cannot
  see it.
- **O-8** — which port v5 records: `quilltap-web --port` (the bound value; v4
  reads `PORT`, default 3000 — v5 has no `PORT` env read) vs also honouring
  `PORT`; and the Tauri shell (no HTTP listener) → omit `port` (v4's
  "undefined when not a valid port" arm) — confirm.
- **O-9** — the v4 CLI `memories` family (`maintain`, `validate --repair`,
  `status` + "within the cap", `ls --tier`), `memory-diff`, and the `db
  memories` verb (incl. `--tier`) are UNPORTED in v5 (`main.rs:258`
  `not_yet_available`; `db_cmd.rs:26-39`; the R-J named deferral
  `completion_behavior.rs:563-570`). Recommend they stay out (no PORT-NEW
  this round): only the help / completion BYTES and the two ported tools'
  flags move. A v4 CLI pointed at a v5 server's `/api/v1/memories?action=…`
  gets v5's 404 (no memories REST family) — unchanged, recorded.
- **O-10** — `maintain`'s dry run with `consolidate` needs the host's
  cheap-LLM executor: install a `maintenance_runner()` the R-E way, and
  should an engine without it answer the loud typed refusal AFTER the 400 /
  404 arms (the `memory_consolidate` precedent `memory_consolidation.rs:
  236-241`) — yes by precedent; confirm the sentence.
- **O-11 (tier-3 loud refusal candidates)** — none required at the route
  layer if the siblings land; if the maintenance sibling slips, the five new
  verbs should land as typed `not_available("<action>")` refusals
  (`memories.rs:71-76`) rather than be absent, so the SPA's new card renders
  an honest failure.
- **O-12** — `?characterId=` (empty) on `maintenance-status`: carry v4's
  "lists all, logs `characterId: ''`" quirk (the wire keeps `Some("")`), or
  record a divergence. Recommend carry (one line).
- **O-13 (size)** — this slice is ~1 lane: the memories route rewrite
  (deltas 1-22) is one lane if the admission / retire / maintenance / index
  seams exist as stubs-with-HANDOFF at KEYSTONE; the lock + CLI (deltas
  23-28) are an independent, small, Tier-R-gated lane that touches no
  memory code and can run in parallel from `main`. Recommend splitting
  them.
