# Survey — the memory programme's DATA-LAYER KEYSTONE (`3f7320138` F4, `d58548051` schema + repository, `70f9b495c` increment) + `ed8b15b50`'s mount-point NOCASE index

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` main,
HEAD `01a83539d` (`4.10.0-dev.143`, tree clean, read with `git show` only);
v5 = main at `96cfdaaf7`. Every fact below is from the hunks or the HEAD files;
line numbers are HEAD unless marked. Scope: the schema move, the
`add-memory-tiers-v1` re-home, the memories repository (every new / changed
verb), the chats watermark column, the two instance-settings keys, the job
type, the vector store's tier metadata, the pure modules — and what a KEYSTONE
lane must land so the stacked memory lanes (gate/housekeeping; recall/context;
extraction + consolidation; carriers) never edit its files. Out of scope:
consumers (gate F1/F3, frozen archive, recall tuning, housekeeping demotion,
consolidation engine, fold-other pass, API/SPA/CLI, carriers' remaps).

**Measured, not guessed** (scratch under
`$TMPDIR/setupphase-survey-memory-schema-keystone/`, nothing written in either
repo; `git -C ~/source/quilltap-server status` clean after every run):
- `dump-fresh-schema.ts` at HEAD → `fresh-head.json` (85 / 31 / 3 statements).
- `dump-migration-indexes.ts` at HEAD with `QT_FRESH_SCHEMA=fresh-head.json`
  and an EMPTY stand-in real-boot master → `migidx-head.json.rejected` (the 63
  "findings" are the empty master's artefact; the BODY is the measurement).
- A scratch `measure-mf.ts` over `migrations-first.ts` (v4's REAL
  `MigrationRunner` on an empty data dir, then the repository pass) → the
  migration-built `memories` / `chats` `PRAGMA table_info`, their indexes, and
  the `migrations_state` rows.

---

## 1. The SEVENTH D23 re-dump — exactly what moves

### 1.1 `fresh_schema.json` (committed at `f5e953a3f`) vs a HEAD dump

`diff` of the two files: **exactly three statements**, every other statement
byte-equal and in the same order.

| Line (committed file) | Partition | Change | Commit |
|---|---|---|---|
| 11 | main | `CREATE TABLE "chats"` gains `  "otherExtractionWatermarkMessageId" TEXT,` **between `"timelineMode" TEXT,` and `"budgetMaxTurns" REAL,`** (schema order; bare `TEXT`, no default) | `d58548051` |
| 24 | main | `CREATE TABLE "memories"` gains four lines **between `"reinforcedImportance" INTEGER DEFAULT 0.5,` and `"createdAt" TEXT NOT NULL,`**: `"tier" TEXT DEFAULT 'hot',` / `"supersededById" TEXT,` / `"consolidatedFrom" TEXT DEFAULT '[]',` / `"consolidatedAt" TEXT,` | `d58548051` |
| after 116 | mountIndex | NEW `CREATE UNIQUE INDEX "idx_doc_mount_points_name_nocase" ON "doc_mount_points" ("name" COLLATE NOCASE)` (sorted after `idx_doc_mount_points_createdAt`); mountIndex 30 → 31 | `ed8b15b50` |

So the fresh-schema dump DOES pick up the NOCASE index (the ledger's
"NOT measured" is now answered): v4 creates it in
`DocMountPointsRepository`'s table-init hook
(`lib/database/repositories/doc-mount-points.repository.ts:85`,
`ensureMountPointNameUniqueIndex(db)`), which the dumper's repository
first-access pass triggers. It is NOT a migration index (§1.2).

### 1.2 `migration_indexes.json` (committed at `f5e953a3f`, 52 / 5 / 5) vs a HEAD runner dump

Body diff: the `source` stamp (`v4Commit`/`v4Version`) and **one** line,
inserted after committed line 42 (`idx_memories_characterId`; `_` 0x5F sorts
after `I` 0x49):

```
    "CREATE INDEX \"idx_memories_character_tier\" ON \"memories\" (\"characterId\", \"tier\")",
```

main 52 → 53; mountIndex / llmLogs unchanged. The NOCASE index is "shared with
fresh_schema" (the dumper leaves those out). Dumper stderr at HEAD:
`migrations: run=126 skipped=88 deferred=[] failed=[]`; main UNIQUE set
unchanged (`idx_chat_documents_unique, idx_connection_profiles_userId_name,
idx_folders_userId_projectId_path, idx_wardrobe_wear_stats_item_wearer`).
**A lane must still run the REAL-boot cross-check** (`QT_REAL_BOOT_MASTER` from
a `tsx server.ts` first boot at the pin — recipe in
`surveys/2026-10-06-p4.153-v4-first-boot-indexes.md`, "The recipe"); the
dumper writes `<out>.rejected` on any finding.

### 1.3 The two column shapes (generateDDL vs the migration) — measured

| Column | generateDDL (fresh dump; `PRAGMA` dflt) | migration `add-memory-tiers-v1` (migrations-first build `PRAGMA`) |
|---|---|---|
| `memories.tier` | `TEXT DEFAULT 'hot'`, schema slot | `TEXT DEFAULT 'hot'`, appended (cid 25) |
| `memories.supersededById` | `TEXT` (no default) | `TEXT DEFAULT NULL` (dflt `NULL`), appended (cid 26) |
| `memories.consolidatedFrom` | `TEXT DEFAULT '[]'` | `TEXT DEFAULT '[]'`, appended (cid 27) |
| `memories.consolidatedAt` | `TEXT` | `TEXT DEFAULT NULL`, appended (cid 28) |
| `chats.otherExtractionWatermarkMessageId` | `TEXT`, after `timelineMode` | `TEXT DEFAULT NULL`, appended LAST (cid 104 of 105 on a real first boot — after `conciergeModeReason`) |

`notnull` 0 everywhere. The only semantic difference is cosmetic (`DEFAULT
NULL` vs none); defaults that matter (`'hot'`, `'[]'`) agree. Real first boot
(measured): `add-memory-tiers-v1` row `itemsAffected 6`, message `Memory tiers
in place (6 changes)` (4 columns + index + chat column; no heal rows).

---

## 2. v4 at HEAD — file:line facts

### 2.1 Schemas (`d58548051`)

- `lib/schemas/memory.types.ts:29` `MemorySourceEnum = z.enum(['AUTO','MANUAL','CONSOLIDATED'])`;
  `:67` `MemoryTierEnum = z.enum(['hot','cold'])` (+ `MemoryTier` type, exported
  from `lib/schemas/types.ts`).
- `MemorySchema` after `:134 reinforcedImportance … .default(0.5)`:
  `:137 tier: MemoryTierEnum.default('hot')`,
  `:139 supersededById: UUIDSchema.nullable().optional()`,
  `:141 consolidatedFrom: z.array(UUIDSchema).default([])`,
  `:143 consolidatedAt: TimestampSchema.nullable().optional()`, then
  `createdAt`, `updatedAt`. **Parsed key order** = that order.
- `lib/schemas/chat.types.ts:1085` (`ChatMetadataSchema`) and `:1424`
  (`ChatMetadataBaseSchema`): `otherExtractionWatermarkMessageId:
  z.string().nullable().optional()` — right after `timelineMode`.
- `lib/schemas/job.types.ts:37` `'MEMORY_CONSOLIDATION'` between
  `MEMORY_HOUSEKEEPING` and `MEMORY_REGENERATE_CHAT` (enum now 24 values).
- `lib/schemas/settings.types.ts:219-235` `MemoryConsolidationSettingsSchema`
  (key order = wire order): `enabled` bool `.default(false)`;
  `connectionProfileId` `z.string().nullable().default(null)`;
  `clusterThreshold` `z.number().min(0).max(1).default(0.72)`;
  `minClusterSize` `z.number().int().min(2).default(3)`;
  `maxClusterSize` `z.number().int().min(2).default(30)`;
  `matureAfterDays` `z.number().min(0).default(7)`;
  `maxClustersPerRun` `z.number().int().positive().default(40)`;
  `watermark` `z.number().int().positive().default(150)`;
  `coldRetentionDays` `z.number().int().positive().nullable().default(null)`.
- `:256-263` `MemoryExtractionModeSettingsSchema`: `otherPass`
  `z.enum(['turn','fold','hybrid']).default('hybrid')`; `perTurnOtherFloor`
  `z.number().min(0).max(1).default(0.75)`; `foldCandidatesPerSubject`
  `z.number().int().positive().default(3)`.

### 2.2 `migrations/scripts/add-memory-tiers-v1.ts` (177 lines, `d58548051`)

- `:32` id `add-memory-tiers-v1`; `:33` context `migration.add-memory-tiers-v1`;
  `:34` `INDEX_NAME = 'idx_memories_character_tier'`; `:91-92` description
  `Add tier/supersededById/consolidatedFrom/consolidatedAt to memories (+ (characterId, tier) index) and chats.otherExtractionWatermarkMessageId`;
  `:93` `introducedInVersion '4.10.0'`; `:94` `dependsOn:
  ['add-episodic-memory-fields-v1']`. Registered LAST
  (`migrations/scripts/index.ts:423`, `:859`, `:1273`), after
  `addWardrobeImageSettingsFieldMigration`. Pretty label
  (`lib/startup/prettify.ts:159`): `Fitting the Commonplace Book with a cellar…`.
- `:37-42` `MEMORY_COLUMNS` in add order: `tier TEXT DEFAULT 'hot'`,
  `supersededById TEXT DEFAULT NULL`, `consolidatedFrom TEXT DEFAULT '[]'`,
  `consolidatedAt TEXT DEFAULT NULL`. `:44` chat column name.
- `:53-87` `assessWork()`: per missing memories column; `needsIndex` =
  `SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?` absent;
  when `tier` exists, `nullTierRows = SELECT COUNT(*) AS n FROM memories WHERE
  tier IS NULL OR consolidatedFrom IS NULL`; `needsChatWatermark` when `chats`
  exists without the column. Table-gated (`sqliteTableExists`).
- `:96-105` `shouldRun` = any of the four. (v4's runner checks the
  `migrations_state` ledger FIRST — a completed row never re-runs, even if NULL
  rows later appear.)
- `:107-176` `run`, in order: (1) each missing memories column via
  `addColumnIfMissing` (`migrations/lib/database-utils.ts:347-360`: DEBUG
  `Adding column {context:'migrations.database-utils', table, column}` then
  ``ALTER TABLE "memories" ADD COLUMN "<col>" <ddl>``) → INFO
  ``Added memories.<col> column`` `{context}`; (2) the index:
  ``CREATE INDEX IF NOT EXISTS "idx_memories_character_tier" ON "memories" ("characterId", "tier")``
  → INFO `Created idx_memories_character_tier` `{context}`; (3) the heal
  (whenever `memories.tier` exists):
  `UPDATE memories SET tier = COALESCE(tier, 'hot'), consolidatedFrom = COALESCE(consolidatedFrom, '[]') WHERE tier IS NULL OR consolidatedFrom IS NULL`
  → when `changes > 0` INFO `Stamped NULL-tier memories hot` `{context, rows}`;
  (4) ``ALTER TABLE "chats" ADD COLUMN "otherExtractionWatermarkMessageId" TEXT DEFAULT NULL``
  → INFO `Added chats.otherExtractionWatermarkMessageId column` `{context}`.
  Result message `Memory tiers in place (${itemsAffected} changes)`;
  catch → ERROR `Memory-tiers migration failed` `{context, error}`, message
  `Memory-tiers migration failed: <msg>`. No transaction wraps the steps.
- `:73-74` comment ("A fresh database whose table came from the Zod DDL has no
  column default") is **false** — measured §1.3: generateDDL gives `tier` /
  `consolidatedFrom` their defaults. NULLs arise only from writes that bind
  NULL explicitly.

### 2.3 `lib/database/repositories/memories.repository.ts` at HEAD

- `:25-26` `MemoryCreateInput` — `tier` / `consolidatedFrom` optional on create.
- `:33` `HOT_TIER_SQL = "COALESCE(tier, 'hot') = 'hot'"` (NULL reads hot).
- `:185-240` `findByCharacterIdPaginated` gains `tier?: MemoryTier`
  (`:195-213` — `filter.tier = tier`, an EQUALITY → `"tier" = ?`; a NULL-tier
  row is in neither tier's listing); `source` widened to `CONSOLIDATED`.
- `:307` `findBySource(…, 'AUTO'|'MANUAL'|'CONSOLIDATED')` — type only.
- **`:361-389` `findMostImportant` (F4 `3f7320138`, hot predicate `d58548051`)**:
  `limit <= 0 → []` (no query); raw
  ```sql
  SELECT * FROM memories
            WHERE characterId = ?
              AND COALESCE(tier, 'hot') = 'hot'
            ORDER BY reinforcedImportance DESC,
                     COALESCE(lastReinforcedAt, createdAt) DESC,
                     id ASC
            LIMIT ?
  ```
  → `hydrateRawRows(rows, 'most-important fetch')`; DEBUG `Fetched most
  important memories {characterId, limit, returned}`; fallback `[]` with ERROR
  `Error finding most important memories {collection, characterId, limit, error}`.
- `:393-420` `hydrateRawRows(rows, context)` (factored out by `3f7320138`;
  `consolidatedFrom` added to the hydrated list by `d58548051`): JSON-parse
  string cells of `keywords, tags, relatedMemoryIds, entities, consolidatedFrom`
  (parse failure → `[]`), `MemorySchema.safeParse`, failures DROPPED with WARN
  ``Memory failed validation in ${context}`` `{memoryId, error}`. **Raw rows
  keep SQL NULL as `null`** (unlike `findByFilter`, whose `hydrateRow` maps
  NULL → `undefined`, `lib/database/backends/sqlite/backend.ts:468`), so a row
  with `tier IS NULL` or `consolidatedFrom IS NULL` passes the SQL predicate and
  then FAILS `MemoryTierEnum.default('hot')` / `z.array().default([])` (zod
  defaults replace `undefined` only) → dropped with the WARN. (Predicted from
  zod semantics — measure, §6 Q2.)
- `:430-475` `findRecentByImportanceTier` — the three `findByFilter` calls gain
  `tier: 'hot'` (`:442`, `:455`, `:470`) — an EQUALITY, **not** `HOT_TIER_SQL`.
- `:490-500` `create(data: MemoryCreateInput)` → `_create({...data, tier:
  data.tier ?? 'hot', consolidatedFrom: data.consolidatedFrom ?? []})` —
  the prefill runs BEFORE `MemorySchema` validation, so `null` tier /
  consolidatedFrom on create (import, restore) become `'hot'` / `[]`.
- **`:571-606` `incrementReinforcement(characterId, memoryId, at)` (`70f9b495c`)**:
  `safeQuery` WITHOUT fallback (rethrows; ERROR `Error reinforcing memory
  {collection, characterId, memoryId, error}`). One synchronous
  `db.transaction`: `SELECT "importance", "reinforcementCount" FROM "memories"
  WHERE "id" = ? AND "characterId" = ?`; none → `null` + WARN `Memory not found
  for reinforcement {memoryId, characterId}`; else `reinforcementCount =
  (row.reinforcementCount ?? 1) + 1`, `reinforcedImportance =
  calculateReinforcedImportance(row.importance ?? 0.5, count)`,
  `UPDATE "memories" SET "reinforcementCount" = ?, "reinforcedImportance" = ?,
  "lastReinforcedAt" = ?, "updatedAt" = ? WHERE "id" = ?` (`updatedAt` =
  `getCurrentTimestamp()`, `lastReinforcedAt` = `at`); DEBUG `Memory
  reinforcement counted {memoryId, characterId, reinforcementCount,
  reinforcedImportance}`; returns `{reinforcementCount, reinforcedImportance}`.
  The job-child `increment*` buffer arm is NO-PORT.
- `:758-772` `countHotByCharacterId`: `SELECT COUNT(*) AS n FROM memories WHERE
  characterId = ? AND COALESCE(tier, 'hot') = 'hot'`; fallback `0`, ERROR
  `Error counting hot memories for character`.
- `:777-793` `countUnconsideredHot`: `… WHERE characterId = ? AND <HOT> AND
  consolidatedAt IS NULL AND source != 'CONSOLIDATED'`; fallback `0`, ERROR
  `Error counting unconsidered hot memories`.
- `:801-816` `findColdIdsByCharacterId`: `SELECT id FROM memories WHERE
  characterId = ? AND tier = 'cold'`; fallback `[]`, ERROR `Error finding cold
  memory ids`.
- `:824-858` `findHotDigests(characterId, subject: string|'self'|'any', limit)`:
  `limit <= 0 → []`; `subject 'self'` → `AND aboutCharacterId = ?` bound to
  `characterId`; `'any'` → no clause; else `AND aboutCharacterId = ?` bound to
  `subject`. SQL: `SELECT * FROM memories WHERE characterId = ? AND source =
  'CONSOLIDATED' AND <HOT> <subjectSql> ORDER BY reinforcedImportance DESC,
  COALESCE(lastReinforcedAt, createdAt) DESC, id ASC LIMIT ?` →
  `hydrateRawRows(rows,'hot digest fetch')`; fallback `[]`, ERROR `Error finding
  hot digests {characterId, subject, limit}`.
- `:874-905` `updateTierBulk(characterId, ids, tier, extra =
  {supersededById?, consolidatedAt?})`: empty → 0; per
  `chunkArray(ids, SQLITE_VARIABLE_CHUNK_SIZE)` → base `updateMany({characterId,
  id: {$in: chunk}}, {tier, ...extra, updatedAt: now})` — and base
  `updateMany` (`base.repository.ts:526-540`) OVERWRITES `updatedAt` with its own
  `getCurrentTimestamp()`; sums `modifiedCount`; DEBUG `Moved memories between
  tiers {characterId, tier, requested, changed, supersededById}` (key omitted
  when `undefined`; `null` → omitted too via `?? undefined`); fallback `0`,
  ERROR `Error moving memories between tiers {characterId, tier, count}`.
  Callers: housekeeping demote `(…, 'cold')` (`lib/memory/housekeeping.ts:378`),
  consolidation `(…, 'cold', {supersededById, consolidatedAt})`
  (`consolidation.ts:614`), the gate's promote `(…, [id], 'hot',
  {supersededById: null})` (`memory-gate.ts:226`).
- `:911-929` `markConsidered(characterId, ids, when)`: chunked `updateMany`
  `{consolidatedAt: when}` (+ base's `updatedAt` = now — markConsidered DOES
  bump `updatedAt`); fallback `0`, ERROR `Error marking memories considered`.
- `:934-940` `findSupersededBy(digestId)` — `findByFilter({supersededById})`;
  **no caller anywhere in v4** (dead).
- `:947-968` `findExpiredColdIds(characterId, olderThan)`: `SELECT id FROM
  memories WHERE characterId = ? AND tier = 'cold' AND supersededById IS NOT
  NULL AND source != 'MANUAL' AND updatedAt < ?`; fallback `[]`, ERROR `Error
  finding expired cold memories`. (Deletion itself reuses the existing
  `deleteMemoriesWithUnlinkBatch` — no new delete SQL.)
- `:1071-1110` `findByCharacterAboutCharacters` — the CTE's WHERE gains
  `AND COALESCE(tier, 'hot') = 'hot'` (ORDER BY still `importance DESC,
  COALESCE(lastReinforcedAt, createdAt) DESC` — F4 did NOT touch it); rows via
  `hydrateRawRows(rows, 'partition fetch')`.
- `lib/repositories/user-scoped.ts:339` `findBySource` type widened only.
- `lib/database/repositories/chats.repository.ts:560-564` `patchOnlyFields()`
  = `[...CONCIERGE_MODE_FIELDS, 'otherExtractionWatermarkMessageId']`. Writers:
  `lib/memory/fold-other-pass.ts:444` `chats.update(chatId, {watermark: id})`;
  `lib/background-jobs/handlers/memory-regenerate-chat.ts:144` `… null`.
  `ChatsRepository.update` (`:292-301`) PRESERVES `updatedAt` unless named.

### 2.4 `lib/memory/reinforced-importance.ts` (14 lines, `70f9b495c`) —
`calculateReinforcedImportance(base, count) = Math.min(1.0, base +
Math.log2(count + 1) * 0.05)`, byte-identical to the old gate copy (re-exported
from `memory-gate.ts:30`). Importers: the repository and the gate.

### 2.5 `lib/embedding/vector-store.ts` (`d58548051`)

- `:34` `VectorMetadata.tier?: 'hot'|'cold'` (absent reads hot; not persisted).
- `:81` interface `setTier(ids, tier)`; `:89` `isHotVector(m) = m.tier !== 'cold'`.
- `load()` `:117` reads `loadColdIds()` and stamps each entry `tier: coldIds.has(id)
  ? 'cold' : 'hot'`; `:133` DEBUG `Loaded vector index {context:
  'CharacterVectorStore.load', characterId, entries, cold}`.
- `:493-503` `loadColdIds` → `memories.findColdIdsByCharacterId`; its catch WARN
  `Could not read cold memory ids; treating every vector as hot` is
  **unreachable** (the repository verb has a `[]` fallback and logs its own
  ERROR instead) — except under a strict scope, where the rethrow lands here.
- `:506-521` `setTier`: re-stamps entries whose tier differs; DEBUG `Re-stamped
  vector tiers {context: 'CharacterVectorStore.setTier', characterId, tier,
  requested, changed}`.
- Filters are applied INSIDE `searchLinear` / `searchHeap` before scoring
  (`:384`, `:448`). Hot-only callers: `memory-service.ts:955-958` (unless
  `includeCold`), `character-optimizer.service.ts:738`; both-tier callers: the
  gate, the `search` tool (`includeCold: true`, `search-scriptorium-handler.ts:278`),
  the Memories API search (`route.ts:560`).

### 2.6 Instance settings (`lib/instance-settings/index.ts`, `d58548051`)

`:43-44` keys `memoryConsolidation`, `memoryExtractionMode`; `:96-99` defaults =
`Schema.parse({})`; `:245-261` / `:267-283` getters via `readJsonSetting`
(`:140-155` — unwritten → defaults silently; parse/schema failure → WARN
`` `[InstanceSettings] ${key} failed to parse — using defaults` `` `{error}`),
setters MERGE `{...current, ...value}` then `writeJsonSetting` (`:161-165`,
`schema.parse` → throws ZodError; `JSON.stringify(validated)` in schema key
order). Neither key is in `NON_PORTABLE_INSTANCE_SETTING_KEYS` (`:75`) — both
travel with export/backup (carriers' concern).

### 2.7 Job type (`d58548051`) — `handlers/index.ts:59`
`MEMORY_CONSOLIDATION: handleMemoryConsolidation` (after HOUSEKEEPING);
`activity-kinds.ts:45` `'memory'` (after HOUSEKEEPING) + the memory title
`Memory work (extraction, regeneration, consolidation, housekeeping)` (SPA-side
in v5); `realtime/job-topics.ts` arm: `payload?.dryRun === true → []`, else
`[{topic:'memories'},{topic:'mountPoints'}]`. `ipc-types.ts`'s
`writeCommonplaceDigestsToVault` host-RPC and `job-dispatcher.ts:772`'s
child-buffered `'memories.updateTierBulk'` are NO-PORT plumbing.

### 2.8 The pure modules — where they belong

| Module | Lines | Depends on | Consumer | Recommendation |
|---|---|---|---|---|
| `lib/memory/reinforced-importance.ts` | 14 | — | repository + gate | Already ported (`crates/quilltap-core/src/memory_gate.rs:45` `calculate_reinforced_importance`, tier-1 Phase 1); the keystone's `increment_reinforcement` calls it — nothing to move. |
| `lib/memory/memory-merge.ts` (`planMemoryMerge` pure, `applyMemoryMerge`) | 176 | `appendCappedFootnotes`, `extractNovelDetails`, `patchMemory`, `reembedMemory` from `memory-gate.ts` (F3) | `lib/tools/memory-dedup.ts:15` ONLY (housekeeping's use retired by `d58548051`) | **Gate/dedup lane**, after F3's footnote cap; not keystone. |
| `lib/memory/recall-tuning.ts` | 207 | `recall-tags.ts` constants, zod | `memory-service.ts:35`, `recall-replay.ts:36` | **Recall lane**; not keystone. |

---

## 3. v5 today — file:line counterparts

- **Artifacts:** `crates/quilltap-core/src/services/provisioning/fresh_schema.json`
  (`f5e953a3f` dump), `migration_indexes.json` (`source.v4Commit f5e953a3f…`,
  52/5/5); `provisioning/mod.rs:192` / `:265` `include_str!`, `:281`
  `migration_index_family()`, `:292` `fresh_main_index(name)`, `:628`
  `assert_eq!(all.len(), 62)` (→ 63). Dumpers:
  `harness/oracle/provision/dump-fresh-schema.ts`, `dump-migration-indexes.ts`
  (+ `migrations-first.ts`).
- **Boot-ensure precedents** (all main-partition, re-homed migrations):
  `db/chat_informs_permanent_repair.rs` (P4.D249 — ALTER with the migration's
  DDL verbatim, no backfill, no ledger read or stamp, v4's migration lines
  NO-PORT), `db/chat_settings_impersonation_voice_mode_repair.rs` (P4.D251 —
  ADD + translate UPDATE + DROP), `db/wardrobe_wear_stats_repair.rs` (P4.D255 —
  the first STAMPING ensure since P4.D63, justified only because the seed has
  no once-only gate; ledger DDL via `db/migrations_ledger.rs`
  `ensure_migrations_tables`; reads its index text from `fresh_schema.json` via
  `fresh_main_index`, "never spelled here"), `db/migration_index_family_repair.rs`
  (P4.160 — backfills every `migration_indexes.json` statement absent on an
  existing instance, `IF NOT EXISTS` spliced; skips a statement on an absent
  table; LAST in `seed_built_ins`). Wiring: `crates/quilltap-host/src/host.rs`
  `seed_built_ins` — P4.D255 block `:2011-2045`, then P4.160 `:2046-2073`.
  **Consequence:** once `idx_memories_character_tier` is in the artifact, P4.160
  creates it on every existing v5 instance by itself — but its `CREATE INDEX`
  on `("characterId","tier")` FAILS (non-fatal ERROR `Failed to backfill a
  migration-created index`) unless the tier ensure added `tier` first. Place the
  new ensure in the P4.D255 region (v4 registration order), BEFORE P4.160.
- **Fixture healers:** `crates/quilltap-core/src/test_support.rs:148-188`
  (`ensure_p4d171_columns`, `ensure_p4d182_columns`, `ensure_p4d225_columns`,
  `ensure_wear_ledger_on`); 46 test files call them on committed vintage
  fixtures (`crates/quilltap-web/tests/fixtures/*.db`, 106 files + 3 under
  `migration-vintage/`).
- **Memory reads** `db/memories_read.rs`: `:59` `COLS` (25 columns, schema
  order), `:120-234` `marshal_row(row, keep_nulls)` positional `row.get(0..=24)`
  (no alignment census — chats_read has one), `:237` `query_memories`, `:401`
  `find_by_character_id_paginated` (`PaginateOptions` `:375` — no `tier`),
  `:505` `find_by_source`, `:531` `find_most_important` (still `ORDER BY
  importance DESC LIMIT`, normal omit-NULL path, no `limit<=0` guard, no hot
  predicate), `:547` `find_recent_by_importance_tier` (no tier filter), `:602`
  `find_by_character_about_characters` (verbatim CTE, `keep_nulls = true`, no
  hot predicate, no validation drop). Callers: `services/frozen_archive.rs:117`,
  `services/memory_recap/mod.rs:668`, `services/build_context.rs:2849`,
  `api/memories.rs:169`.
- **Memory writes** `db/memories.rs`: `:67` `MemCreate` (23 fields), `:117`
  `parse_create_memory` (P4.161 — the ONE `MemorySchema` parse for `.qtap`
  import `services/quilltap_import/memories.rs:149` and restore
  `services/backup/restore/orchestrator.rs:944`), `:211` `MemUpdate`
  (`#[derive(Default)]`), `:266-319` `create_row` INSERT (25 columns), `:340-428`
  `update_row`, `:462` `update_for_character`, `:578` `update_access_time_bulk`
  (unchunked — the shape `update_tier_bulk` would follow, chunked). No
  `increment_reinforcement`, no tier verbs. `MemCreate {` literals:
  `services/memory_gate.rs:587` (PRODUCTION, `create_memory_direct_with_embedding`),
  test literals `services/memory_gate.rs:1178`, `services/memory_service.rs:1523`,
  `services/housekeeping.rs:620`, `realtime/publish_sites.rs:839`,
  `db/memories.rs:1074`, `crates/quilltap-harness/tests/memories_tier2_equivalence.rs:192`.
- **Zod twin** `api/zod_issues.rs:904` `MEMORY_SOURCE = ["AUTO","MANUAL"]` (the
  live cause of the 2026-10-09 walk's 365 refused digests), `:1062-1121`
  `zod_memory_issues` (no tier keys). Recorded by
  `harness/oracle/cases/repository-zod-messages.ts` (`:61` imports v4's REAL
  `MemorySchema`; `:201` memory rows).
- **Gate:** `services/memory_gate.rs:657` `reinforce_memory` — `existing_count
  + 1.0` from the snapshot, one absolute patch (bug 182's shape, ledger-correct);
  `SkipNearDuplicate` writes nothing.
- **Chats** `db/chats_read.rs:78` `ALL_COLUMNS` (101 columns; `timelineMode` at
  index 72, `marshal_row` `:315`), alignment census `:540-760` pinned to
  `fresh_schema.json` — **the re-dump REDDENS it until the column is read**;
  `db/chats.rs:850` INSERT (does not need the column — NULL default),
  `:1035` `update` (`ChatUpdate` `:514`, sets only named columns),
  `:1644` `set_concierge_mode` (the ONE-writer + census precedent). 26 test
  files hand-roll a `chats` DDL (5 carry `timelineMode TEXT`).
- **Instance settings** `db/instance_settings.rs`: `:95` `read_json_setting`
  (v4's WARN), `:121` `write_setting`, `:151-248` the `memoryRecall` struct /
  `to_json` / parse twin / get / set — the template. No consolidation / mode keys.
- **Job tables:** `services/job_runner.rs:141` `KNOWN_JOB_TYPES`,
  `api/system_data.rs:636` `JOB_TYPES` ("23 values"), `services/activity_kinds.rs:80`
  `JOB_TYPE_ACTIVITY`, `realtime/job_topics.rs:111` (HOUSEKEEPING arm).
  `MEMORY_CONSOLIDATION` absent everywhere.
- **Vector store** `db/vector_store.rs`: `:43` `VectorEntry {id, embedding}` (no
  metadata), `:80` `load(conn, character_id)` (per-call load — every caller
  loads fresh: `memory_gate.rs:434/637/838`, `memory_dedup.rs:413`,
  `memory_service.rs:121/239/501/1330/1429`, `housekeeping.rs:336/513`,
  `generators/optimizer.rs:1641`), `:148` `search(query, limit)` (linear, no
  filter). No tier, no `set_tier`, no filter.
- **Hand-rolled `memories` DDL in tests** (break when `COLS` grows):
  `quilltap-host/tests/host_boot.rs:50`, `realtime/publish_sites.rs:802`,
  `services/memory_gate.rs:1152`, `services/memory_service.rs:1490`,
  `services/embedding_reapply_profile.rs:454`,
  `services/embedding_dimension_reconcile.rs:614`, `services/job_runner.rs:1088`,
  `services/housekeeping.rs:587`, `db/memories.rs:1055`, `db/memories_read.rs:1012,1090`.

---

## 4. What the ledger row / commit prose got wrong or left open

1. **`findRecentByImportanceTier` does NOT use `COALESCE(tier,'hot')='hot'`** —
   it filters `tier: 'hot'` (equality; NULL-tier rows excluded). The ledger
   lists it with the COALESCE pair. The COALESCE predicate is in
   `findMostImportant`, `findByCharacterAboutCharacters`, `findHotDigests`,
   `countHotByCharacterId`, `countUnconsideredHot` (the last three unlisted).
2. **"four INFO lines + `Stamped NULL-tier memories hot`"** — up to SEVEN INFO
   lines (four `Added memories.<col> column`, `Created
   idx_memories_character_tier`, the heal line with `rows`, `Added
   chats.otherExtractionWatermarkMessageId column`) plus the database-utils
   DEBUG per ALTER and the ERROR on failure.
3. **The two shapes** — now measured (§1.3): they differ only by `DEFAULT
   NULL` on `supersededById` / `consolidatedAt` / the chat column, and by
   position. The migration header's "Zod DDL has no column default" is false.
4. **The NOCASE index** — measured: it IS in a fresh-schema dump (mountIndex
   30 → 31) and is NOT in the migration-index dump. It therefore rides the SAME
   `fresh_schema.json` re-dump as the memory columns if that re-dump is taken at
   HEAD (§7 decision D1).
5. `findSupersededBy` (new) has no caller; the vector store's WARN is
   unreachable outside a strict scope; `markConsidered` bumps `updatedAt` (via
   base `updateMany`) though it does not name it — which feeds
   `findExpiredColdIds`' `updatedAt < ?` window.
6. `70f9b495c` row is accurate (increment shape, WARN/DEBUG, `?? 1`, `?? 0.5`);
   add: no fallback (rethrow + ERROR `Error reinforcing memory`).

---

## 5. Differential plan

**Existing families the keystone owns and must regenerate at the pin**
(predicted RED at HEAD): `provisioning_equivalence` (D23 tripwire (1d) by
design; (1c) index set gains `idx_memories_character_tier` + the NOCASE index),
`memories_read_equivalence` (four new keys on every memory; F4 ordering;
hot predicates), `memories_tier2_equivalence` (create writes `tier`/
`consolidatedFrom`), `repository_zod_messages_equivalence` (MemorySchema
bounds grow), `chats_read_equivalence` / `chats_tier2_equivalence` (the
watermark column), `activity_tables_equivalence`, `realtime_topics_equivalence`
(job topics), `system_jobs_routes_equivalence` (24-value enum),
`instance_settings_json_warns_equivalence`, `migration_index_backfill_equivalence`
(artifact grows). **Families that redden at HEAD but belong to consumer lanes**
(the keystone must not chase them): `memory_gate_tier3`, `build_context_tier3`,
`memory_housekeeping_tier2`, `memory_processor_tier3`, the recap / frozen
archive / precompute / recall-replay families — they move with the
repository semantics the keystone lands AND with their own commits.

**New cases the keystone adds:**
- `harness/oracle/cases/memory-tiers-ensure.ts` + `memory_tiers_ensure_equivalence.rs`
  — the P4.D249/P4.D251 multi-mode template over v4's REAL
  `add-memory-tiers-v1` via `harness/oracle/lib/v4-migrations.ts`
  `runV4Migrations`: (a) BASELINE (the `f5e953a3f` `memories`/`chats` — derived
  by deleting exactly the moved lines from v4's own generateDDL, as
  `chat-informs-permanent-ensure.ts` does), seeded rows; (b) no `memories`, no
  `chats`; (c) already migrated; (d) GENERATEDDL-CURRENT with rows whose `tier`
  / `consolidatedFrom` were planted NULL and NO index → v4 RUNS (index + heal,
  `rows`); (e) columns present, index absent; (f) memories migrated, `chats`
  lacking the watermark. Compare `table_info`, `sqlite_master.sql`, index SQL,
  every row's `(id, tier, consolidatedFrom)`, and — if ported — the line set.
- `memories-read.ts` grows: F4 tie-break rows (equal `reinforcedImportance`,
  `lastReinforcedAt` NULL vs set, id tiebreak), `limit 0`, cold rows excluded,
  a NULL-tier row (the raw-path drop), `findHotDigests` × {self, any,
  characterId, limit 0}, the two counts, `findColdIdsByCharacterId`,
  `findExpiredColdIds` (MANUAL excluded, non-superseded excluded, window edge),
  `findRecentByImportanceTier` with a NULL-tier row (excluded) vs
  `findMostImportant` (included-then-dropped), paginated `tier` filter.
- `memories-tier2.ts` grows: `create` with/without tier, `CONSOLIDATED` +
  `consolidatedFrom`, `incrementReinforcement` (hit; miss → null; wrong
  character → null; `reinforcementCount` NULL → 2), `updateTierBulk` (cold with
  extra; hot with `supersededById: null`; foreign-character ids untouched;
  >`SQLITE_VARIABLE_CHUNK_SIZE` ids), `markConsidered`. Normalize `updatedAt`.
- `repository-zod-messages.ts` grows MemorySchema rows: `tier` out-of-enum /
  `null` (refused by raw schema, accepted after the create prefill),
  `supersededById` non-uuid, `consolidatedFrom` non-array / non-uuid element,
  `consolidatedAt` bad timestamp, `source: 'CONSOLIDATED'` accepted.
- Settings: a tier-1 parse case over v4's REAL `MemoryConsolidationSettingsSchema`
  / `MemoryExtractionModeSettingsSchema` (defaults, each bound, `null`
  `coldRetentionDays`, `0` watermark refused, `1.5` minClusterSize refused) +
  the WARN rows in `instance_settings_json_warns`.
- Vector store: a unit/tier-1 row that `load` stamps cold ids and a filtered
  search skips them before top-K (v4's `searchLinear` filter position).

**v4 jest tests to mirror:**
`__tests__/unit/lib/database/migration/add-memory-tiers-v1.integration.test.ts`
(4 cases: dated/dependsOn, adds columns+index stamping existing rows hot, heals
NULL tiers, idempotent); `memories-find-most-important.test.ts` (3 cases:
ranking + tiebreak, `[]` without querying for limit ≤ 0, hydrates JSON
arrays); the repository half of `memory-recall-housekeeping-fixes.test.ts`
(`70f9b495c`'s increment cases).

---

## 6. Open questions / measurements a lane must make before coding

- **Q1 (D1, planning): which pin the re-dump is taken at.** A HEAD dump carries
  the NOCASE UNIQUE index into `fresh_schema.json`, so every v5-provisioned
  instance and every test that replays the mount-index partition from the
  artifact gains a UNIQUE `("name" COLLATE NOCASE)` on `doc_mount_points` —
  BEFORE bug 186's repository refusal / reconcile land. v5 already uniquifies
  vault names (`db/character_vault.rs:324`
  `next_unique_mount_point_name`, `db/mount_index_case_repair.rs` repairs
  without an index), so the production hazard is the store-create / import /
  restore paths that do not uniquify; the test hazard is any fixture that seeds
  case-colliding store names. Measure with the workspace gate.
- **Q2:** confirm on v4 that a `tier IS NULL` row is DROPPED by
  `findMostImportant` / `findHotDigests` / `findByCharacterAboutCharacters`
  with ``Memory failed validation in <context>`` (zod 4 defaults skip `null`),
  and KEPT-as-hot by `findByFilter` paths; then decide whether v5's raw reads
  port the validation drop (they have none today).
- **Q3 (ruling):** the ensure's ledger semantics. Precedent (P4.D249/P4.D251):
  no ledger read, no stamp, migration lines NO-PORT. v4's runner reads the
  ledger first — after v4 ran it (live Friday, 2026-10-08 22:46Z), v4 never
  heals NULL rows again; an unledgered v5 ensure would. Harmless (convergent),
  but say so. Recommendation: precedent (no stamp; `shouldRun` gates only).
- **Q4:** v4's exact `buildUpdateQuery` SQL for `updateTierBulk` /
  `markConsidered` (column order of the `SET`, `IN (…)` shape) — capture via
  `sqlite3_trace_v2` if a byte comparison is wanted; otherwise compare effects.
- **Q5:** the committed-fixture blast radius — which tests read `memories` or
  `chats` off a vintage fixture without a boot (they break on `no such column:
  tier` / `otherExtractionWatermarkMessageId`); give them
  `test_support::ensure_memory_tiers_columns`. Also the 10 hand-rolled
  `memories` DDLs and the hand-rolled `chats` DDLs that carry a full column set.
- **Q6:** port `findSupersededBy` (dead in v4) or record it NO-PORT.
- **Q7:** `find_most_important` moving to the raw (keep-NULL) path changes the
  memory objects the frozen archive receives (explicit `null`s) — check no
  consumer serializes them to a wire before the recall lane takes over.

---

## 7. The KEYSTONE — proposed units and the files each owns

Every stacked memory lane branches from the keystone; the keystone edits a few
consumer files ONLY mechanically (struct literals, test DDLs) so the stacked
lanes inherit them and never touch the keystone's own files.

| Unit | Content | v5 files (OWNED) |
|---|---|---|
| **K1 D23 re-dump #7** | Re-dump `fresh_schema.json` (3 statements, §1.1) and `migration_indexes.json` (+1 main, §1.2, with the REAL-boot cross-check) from the round's pin; artifact count 62 → 63. | `crates/quilltap-core/src/services/provisioning/{fresh_schema.json,migration_indexes.json,mod.rs}` (+ a `migration_main_index(name)` sibling of `fresh_main_index`) |
| **K2 `add-memory-tiers-v1` boot ensure** | `ensure_memory_tiers(main) -> Result<MemoryTiersEnsureOutcome, DbError>`: per missing column `ALTER TABLE "memories" ADD COLUMN "<c>" <migration ddl>` in v4's order; the index from `migration_indexes.json` (`IF NOT EXISTS` spliced) when absent and the table exists; the heal UPDATE verbatim when `tier` exists; the chats ALTER `TEXT DEFAULT NULL`. Table-gated, idempotent, no stamp (Q3). Wired in `seed_built_ins` after P4.D255, before P4.160. `test_support::ensure_memory_tiers_columns`. Oracle + family §5. | `crates/quilltap-core/src/db/memory_tiers_repair.rs` (NEW), `db/mod.rs` (one `pub mod`), `crates/quilltap-host/src/host.rs` (one block), `crates/quilltap-core/src/test_support.rs`, `harness/oracle/cases/memory-tiers-ensure.ts`, `crates/quilltap-harness/tests/memory_tiers_ensure_equivalence.rs` |
| **K3 memory reads** | `COLS` + `marshal_row` grow to 29 (tier, supersededById, consolidatedFrom, consolidatedAt between reinforcedImportance and createdAt; `tier` NULL → `"hot"` and `consolidatedFrom` NULL → `[]` on the omit-NULL path, `supersededById`/`consolidatedAt` nullable-optional); a `memories_read` alignment census vs `fresh_schema.json` (chats_read's shape); `find_most_important` (F4 SQL, hot predicate, `limit<=0 → []`, keep-NULL path, DEBUG, fallback `[]`); `find_recent_by_importance_tier` (`tier = 'hot'` equality ×3); `find_by_character_about_characters` (+ hot predicate); `PaginateOptions.tier`; NEW `count_hot_by_character_id`, `count_unconsidered_hot`, `find_cold_ids_by_character_id`, `find_hot_digests(conn, character_id, DigestSubject, limit)`, `find_expired_cold_ids` — each with v4's fallback value + ERROR line. Q2's validation-drop decision lives here. | `crates/quilltap-core/src/db/memories_read.rs`, `harness/oracle/cases/memories-read.ts`, `crates/quilltap-harness/tests/memories_read_equivalence.rs`; mechanical: `api/memories.rs:169` (`tier: None`) |
| **K4 memory writes + schema twin** | `MemCreate` + `tier: String`, `superseded_by_id: Option<String>`, `consolidated_from: Vec<String>`, `consolidated_at: Option<String>`; INSERT 29 columns; `MemUpdate` + `tier: Option<String>`, `superseded_by_id: Option<Option<String>>`, `consolidated_from: Option<Vec<String>>`, `consolidated_at: Option<Option<String>>`; `parse_create_memory` applies v4's create prefill (`tier ?? 'hot'`, `consolidatedFrom ?? []`) before the twin; `MEMORY_SOURCE` += `CONSOLIDATED`, `MEMORY_TIER`, four checks appended after `reinforcedImportance` in `zod_memory_issues`; NEW `increment_reinforcement(character_id, memory_id, at) -> Result<Option<Reinforcement{count, importance}>, DbError>` (ONE savepoint, read-then-write, WARN/DEBUG, rethrow + ERROR); `update_tier_bulk(character_id, ids, tier, TierExtra{superseded_by_id: Option<Option<String>>, consolidated_at: Option<String>}) -> i64` (chunked, `updatedAt = now`, DEBUG, fallback 0); `mark_considered(character_id, ids, when) -> i64` (chunked, also bumps `updatedAt`). Every `MemCreate {` literal updated (§3). | `crates/quilltap-core/src/db/memories.rs`, `crates/quilltap-core/src/api/zod_issues.rs` (memory section), `harness/oracle/cases/memories-tier2.ts` (+ its spec JSON), `harness/oracle/cases/repository-zod-messages.ts`, `crates/quilltap-harness/tests/{memories_tier2_equivalence,repository_zod_messages_equivalence}.rs`; mechanical: `services/memory_gate.rs:587` (+ test literals in `memory_gate.rs`, `memory_service.rs`, `housekeeping.rs`, `realtime/publish_sites.rs`) and the 10 hand-rolled `memories` test DDLs |
| **K5 chats watermark** | `ALL_COLUMNS` + `marshal_row` read `otherExtractionWatermarkMessageId` (schema slot after `timelineMode`, census green); ONE writer `ChatsRepository::set_other_extraction_watermark(chat_id, Option<&str>) -> Result<bool, DbError>` (`UPDATE chats SET "otherExtractionWatermarkMessageId" = ? WHERE id = ?`, `updatedAt` untouched — v4's `update` preserves it) + a writers census (the Concierge precedent); `ChatUpdate` gains NO field. Hand-rolled full-column `chats` test DDLs updated. | `crates/quilltap-core/src/db/chats_read.rs`, `crates/quilltap-core/src/db/chats.rs` (the one method + census), `harness/oracle/cases/chats-read.ts`, `crates/quilltap-harness/tests/chats_read_equivalence.rs`; mechanical: the hand-rolled `chats` DDLs Q5 finds |
| **K6 instance settings** | `MemoryConsolidationSettings` (9 fields, `Default`, `to_json` in schema order, parse twin with every bound) + `get_/set_memory_consolidation_settings` (merge `{...current, ...patch}`); `MemoryExtractionModeSettings` (3 fields) likewise; through `read_json_setting` / `write_setting`. | `crates/quilltap-core/src/db/instance_settings.rs`, a new tier-1 oracle case + family, `instance_settings_json_warns` rows |
| **K7 job type tables** | `MEMORY_CONSOLIDATION` in `KNOWN_JOB_TYPES` (after HOUSEKEEPING), `JOB_TYPES` (24, v4 enum order), `JOB_TYPE_ACTIVITY` (Memory, after HOUSEKEEPING), the job-topics arm (`dryRun:true → []`, else memories + mountPoints collection hints). No handler (the consolidation lane registers it; until then the loud "recognized but not yet available" fallback). | `services/job_runner.rs` (the const only), `api/system_data.rs` (the const only), `services/activity_kinds.rs`, `realtime/job_topics.rs` |
| **K8 vector store tiers** | `VectorEntry` gains `cold: bool`; `load` stamps from `memories_read::find_cold_ids_by_character_id` + DEBUG `Loaded vector index`; `set_tier(&mut self, ids, Tier)` + DEBUG; `search_filtered(query, limit, pred)` filtering BEFORE scoring and `is_hot(id)`; `search` unchanged (both tiers). | `crates/quilltap-core/src/db/vector_store.rs` |

Sequencing inside the lane: K1 → K2 → K4 → K3 → K5 → {K6, K7, K8}.

---

## 8. Meeting points (the draft contract the stacked lanes consume)

- **Gate / housekeeping lane** consumes: `MemoriesRepository::increment_reinforcement`,
  `update_tier_bulk` (demote `Tier::Cold`; promote `Tier::Hot` +
  `superseded_by_id: Some(None)`), `memories_read::count_hot_by_character_id`,
  `find_expired_cold_ids`, `CharacterVectorStore::set_tier`,
  `get_memory_consolidation_settings().cold_retention_days`,
  `calculate_reinforced_importance` (exists). Owns `memory-merge.ts`'s port,
  `services/memory_gate.rs` (beyond K4's literal), `services/housekeeping.rs`,
  `services/memory_dedup.rs`.
- **Recall / context lane** consumes: `find_most_important` (already F4 + hot),
  `find_hot_digests` + `DigestSubject::{SelfSubject, Any, Character(id)}`,
  `find_by_character_about_characters` (hot), `find_recent_by_importance_tier`
  (hot), `search_filtered` + `is_hot`, the memory JSON's `tier` /
  `supersededById` / `consolidatedFrom` keys (`formatDigestProvenance` reads
  `consolidatedFrom.len()`), `MEMORY_SOURCE` incl. `CONSOLIDATED`. Owns
  `recall-tuning.ts`'s port, frozen archive, build_context, recap,
  memory_service search (`includeCold`).
- **Extraction + consolidation lane** consumes: `get_memory_extraction_mode_settings`,
  `get_/set_memory_consolidation_settings`, `ChatsRepository::set_other_extraction_watermark`
  + the chat JSON key, `MemCreate{source:"CONSOLIDATED", tier:"hot",
  consolidated_from}`, `update_tier_bulk(…Cold, TierExtra{superseded_by_id:
  Some(Some(digest)), consolidated_at: Some(now)})`, `mark_considered`,
  `count_unconsidered_hot`, `count_hot_by_character_id`, `MemUpdate`'s four
  setters (digest revisions), the `MEMORY_CONSOLIDATION` job-type rows (it
  registers the handler).
- **Carriers lane** consumes: the grown `parse_create_memory` (+ prefill) and
  `zod_memory_issues`, `MemCreate`'s four fields (restore/import write them;
  the remaps of `supersededById` / `consolidatedFrom` / `relatedMemoryIds` /
  the chat watermark are theirs), the memory JSON's key order for `.qtap`
  export, the two settings keys travelling as portable instance settings.
- **Bug-186 / mount-index lane** (`ed8b15b50`): consumes K1's re-dump (if D1 =
  HEAD) — it must NOT re-dump `fresh_schema.json` itself; it owns the
  existing-instance ensure (`ensureMountPointNameUniqueIndex` → v5
  `db/mount_index_case_repair.rs`: collision repair, `nocaseUniqueIndexIsValid`,
  `DROP INDEX IF EXISTS` + `CREATE UNIQUE INDEX`, INFO `Mount-index: created the
  unique document-store name index`) and the repository refusal. If D1 = HEAD
  the two lanes must agree that K1's gate is green with the UNIQUE index
  present on fresh-schema-built test databases.
