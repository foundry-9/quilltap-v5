# Survey — the `64743ee13` memory-refactor + scenarios round's SCHEMA KEYSTONE: the twelve migrations, the EIGHTH + NINTH D23 re-dumps (one dump), the boot-ensure chain, the repositories that change shape, the vector-mirror retirement, the job-type move, the wire registrations, and the IMPORT-DEAD census

Read-only planning survey, **2026-10-13**. v4 pin **`64743ee13`** (`4.10.0-dev.175`,
read ONLY through `git show 64743ee13:<path>` / `git diff 4a38222e6 64743ee13`);
oracle baseline **`4a38222e6`** (what v5 has ported; pre-shape line numbers marked
`(pre)`); v5 `main` at **`a7c6f0d03`**. Every v4 line number below is AT THE PIN
unless marked `(pre)`. Nothing was built, run or written in either repo except
this file. Census scripts (read-only, scratch only) are described in §5.4.

> ⚠ **The v4 checkout is DIRTY at survey time** (the brief says clean):
> `git -C ~/source/quilltap-server status --short` lists nine modified files —
> `app/api/v1/chats/[id]/actions/{impersonation-voice-preview,inform,rebuild-summary,story-background}.ts`,
> `app/api/v1/chats/[id]/handlers/get.ts`, `app/api/v1/connection-profiles/[id]/route.ts`,
> `app/api/v1/memories/route.ts`, `app/api/v1/plugins/[name]/route.ts`,
> `lib/mount-index/scenario-item-route-factory.ts`. HEAD is still `64743ee13`.
> Every fact here is from the committed tree; **every regen this round needs a
> pinned worktree** (`/tmp/qt-v4-pin-…` with its own `npm ci --offline`), never
> the live checkout — `app/api/v1/memories/route.ts` is one of the dirty files.

---

## 0. The span for this slice, and the ledger corrections

Twelve migrations land in `4a38222e6..64743ee13` (the `migrations/` stat:
15 files, +1,482 / −3): `545e299bb` `add-chat-scenario-source-v1`;
`c2795733f` FIVE (`add-episode-extraction-watermark-v1`,
`add-memories-superseded-by-index-v1`, `add-memory-superseded-at-v1`,
`recompute-reinforced-importance-v1`, `rezone-legacy-memory-occurred-at-v1`);
`e0ac7493a` `move-memory-maintenance-settings-v1`; `af48f6cc2`
`retire-memory-vector-entries-v1` (+ `normalize-vector-storage.ts` no longer
re-creates the mirror); `ac6348c5f` `add-memory-extraction-cursors-v1` +
`drop-inter-character-memory-jobs-v1`; `4ae0c4ca6`
`add-tfidf-text-shape-version-v1`; `104e986c7`
`drop-housekeeping-consolidation-jobs-v1`; `b045dd986` touches
`add-memory-tiers-v1` (a doc-path COMMENT only). The schema files that move
generateDDL: `lib/schemas/chat.types.ts` (+2 scenario columns, −the OTHER
watermark), `memory.types.ts` (+`supersededAt`),
`embedding-job.types.ts` (+`textShapeVersion`), `settings.types.ts`
(`ChatSettingsSchema` −2 columns; the maintenance / consolidation / limits
schemas), NEW `memory-extraction-cursor.types.ts`, DELETED
`vector-indices.types.ts` (+ its repository). `job.types.ts` moves the job
enum (no DDL). The repositories that change shape: `memories.repository.ts`
(1921 lines; +848/−… — the bare `delete` refuses, six SQL-delta verbs, the
index read), NEW `memory-extraction-cursors.repository.ts` (151),
`chat-settings.repository.ts` (−2 seed keys), `chats.repository.ts`
(patch-only list), `background-jobs.repository.ts` (+`findActiveByType`),
`embedding-profiles` (+`findInstanceDefault`), DELETED
`vector-indices.repository.ts` (−381). NEW `lib/memory/retire.ts` (569),
`policy-protection.ts`, `settings-legacy.ts` (91), `index-store/*`.

**Ledger corrections (§1 / §3 rows), measured from the hunks:**

1. **generateDDL renders a `.nullable().optional()` column as BARE `TEXT`, not
   `TEXT DEFAULT NULL`.** The `545e299bb` row says v4's generateDDL renders
   `"scenarioSource" TEXT DEFAULT NULL, "scenarioNotes" TEXT DEFAULT NULL`. The
   committed dump shows the translator's real output for the same Zod shape:
   `"consolidatedAt" TEXT`, `"supersededById" TEXT`,
   `"otherExtractionWatermarkMessageId" TEXT` (bare). Predicted re-dump:
   `"scenarioSource" TEXT,` / `"scenarioNotes" TEXT,`; `TEXT DEFAULT NULL` is
   the MIGRATION's appended shape. Same for `"supersededAt" TEXT`.
2. **`textShapeVersion` is predicted `REAL DEFAULT 0` in generateDDL, NOT
   `INTEGER DEFAULT 0`.** `schema-translator.ts:322-328` maps a number to
   `INTEGER` only when BOTH an integer `min` AND an integer `max` exist;
   `z.number().int().nonnegative()` carries a min only (cf. `"reinforcementCount"
   REAL DEFAULT 1` from `z.number().int().min(1).default(1)`, and
   `"sidebarWidth" INTEGER` from `.min(256).max(512)`). The migration appends
   `INTEGER DEFAULT 0`. **The two shapes differ in type AFFINITY, not just
   position** — v4 writes `1` into a REAL column and gets `1.0` back; a v5 read
   with `rusqlite` `get::<i64>` on a REAL cell fails (`InvalidColumnType`).
   Confirm by re-dump; read the cell type-tolerantly either way (the
   `wearCount` precedent).
3. **The cursor table's generateDDL dump also brings an index the ledger does
   not name**: `CREATE INDEX "idx_memory_extraction_cursors_createdAt" ON
   "memory_extraction_cursors" ("createdAt" DESC)` (every base repository's
   `createdAt` index; no `userId` column → no `userId` index). Predicted.
4. **Every REAL v4 instance carries the cursor table in the MIGRATION shape**
   (`UNIQUE ("chatId","pass")` + `FOREIGN KEY … ON DELETE CASCADE`), fresh ones
   included: a real v4 first boot runs `MigrationRunner` in PHASE 1 before any
   repository is touched (`instrumentation.ts:420`, the P4.153 finding), so the
   migration's `CREATE TABLE IF NOT EXISTS` wins and the repository's
   `ensureCollection` is a no-op. Only a generateDDL-only build (the dumper, the
   harness builders) has the bare shape. v4's main connection runs
   `foreign_keys = ON` (`lib/database/backends/sqlite/client.ts:120`), so the
   FK is LIVE: a chat delete cascades its cursors; a cursor insert for an
   absent chat FAILS. See §9 O-2.
5. **`add-memory-tiers-v1` at the pin STILL adds
   `chats.otherExtractionWatermarkMessageId`** (`:44`, `:83`, `:147-150` —
   `b045dd986` changed only a comment). On v4 the cursor migration drops it
   again the same boot. So the "retire P4.D265's chat-column arm" ruling makes
   v5's tiers ensure DIVERGE from v4's tiers migration ALONE but CONVERGE with
   v4's chain (tiers → episode watermark → cursors). The tiers differential
   (`memory_tiers_ensure_equivalence`) must compare chains, not the one
   migration (§5.2).
6. **The real dead-import set is narrower and different from §1's "Regen"
   list.** `memory-calendar.ts`, `memory-calendar-refresh`, `memory-edit.ts`,
   `fold-window.ts`, `extraction-route.ts` never existed at the baseline
   (born in `c2795733f`, gone again by Phase A–D), so no v5 harness file
   imports them; `reinforced-importance.ts` IS deleted but no harness file
   imports it (`memory-name-helpers.ts` takes `calculateReinforcedImportance`
   from `memory-gate`). Measured: **28 deleted modules, 79 import references
   in 60 harness files**, plus 3 removed named exports from modules that
   survive, plus 12 builders calling `repos.vectorIndices.*` (runtime-dead) —
   the full table is §5.4.
7. **The chat-wipe INFO at the pin** is `log.info('Chat wipe spared manual,
   cold and digest rows', { chatId, spared, sparedBy })` on the child logger
   `module: 'memory:retire'` (`retire.ts:547-551`) — not the `124c8aca3`
   shape `{ chatId, spared, manual, cold, digests }` with a `[Memory]` prefix.
   Port the pin's.
8. **`migration_indexes.json`: 63 → 62 statements (main 53 → 52)** — the
   ledger's arithmetic holds; the inserted line sorts after
   `idx_memories_reinforcedImportance` (`r` < `s`). And
   **`provisioning_equivalence`'s `TABLE_COLUMN_ASYMMETRY` row
   `chat_settings / v4 / memoryExtractionConcurrency` trips BY DESIGN** —
   `move-memory-maintenance-settings-v1` drops that column on a real v4 first
   boot too (§5.2).
9. The "three retention knobs GONE" of `104e986c7` are NOT in any settings
   schema at the pin (`MemoryConsolidationSettingsSchema` only GAINS
   `tfidfClusterThreshold`; `coldRetentionDays` stays) — they are request-level
   options of the retired housekeep action. Not this slice; the maintenance
   lane should verify.

---

## 1. v4 AT THE PIN

### 1.1 The runner (unchanged in the span) — what a port must reproduce

`migrations/index.ts` (pin): `sortMigrationsByDependency` (`:32-64`) is a DFS
topological sort that visits each migration's `dependsOn` first, then pushes it
— so the run order is the registry's array order except where a dependency sits
LATER in the array. `MigrationRunner.runMigrations` (`:142-249`):

1. ledger FIRST — `isMigrationCompleted(state, id)` → skipped (`:145-147`);
2. a dependency deferred this boot → this one deferred too (WARN `Migration
   deferred to the next boot: a dependency did not complete`);
3. `shouldRun()` false → skipped AND NOT RECORDED (re-asked next boot); a throw
   from `shouldRun` → ERROR `Error checking if migration should run`, skipped;
4. INFO `Running migration` {migrationId, description}; `run()`;
5. success → `recordCompletedMigration` (the `migrations_state` row) + INFO
   `Migration completed successfully` {migrationId, itemsAffected, durationMs};
   failure → ERROR `Migration failed`; **not `resumable` → the boot STOPS**
   (`:200-217`). None of the twelve is `resumable`.

The runner's lines, the `migrations_state` row and the pretty labels are the
standing NO-PORT class (P4.D63/D73/D79/D249/D251/D255/D265) — v5 logs ONE
own boot INFO per ensure that did anything.

**Registration (`migrations/scripts/index.ts`, imports `:424-443`, array
`:879-903`, re-export `:1317-1340`)** — appended after `addMemoryTiersMigration`
(`:879`) in this order, which IS the run order (every `dependsOn` is earlier):

| # | array line | export | id | `dependsOn` |
|---|---|---|---|---|
| 1 | 881 | `addMemorySupersededAtMigration` | `add-memory-superseded-at-v1` | `add-memory-tiers-v1` |
| 2 | 883 | `addChatScenarioSourceMigration` | `add-chat-scenario-source-v1` | `add-chat-scenario-text-field-v1` |
| 3 | 885 | `addEpisodeExtractionWatermarkMigration` | `add-episode-extraction-watermark-v1` | `add-memory-tiers-v1` |
| 4 | 887 | `recomputeReinforcedImportanceMigration` | `recompute-reinforced-importance-v1` | `add-memory-gate-fields-v1` |
| 5 | 889 | `rezoneLegacyMemoryOccurredAtMigration` | `rezone-legacy-memory-occurred-at-v1` | `add-episodic-memory-fields-v1` |
| 6 | 891 | `addMemoriesSupersededByIndexMigration` | `add-memories-superseded-by-index-v1` | `add-memory-tiers-v1` |
| 7 | 893 | `moveMemoryMaintenanceSettingsMigration` | `move-memory-maintenance-settings-v1` | `create-instance-settings-table-v1`, `migrate-extraction-knobs-to-instance-settings-v1`, `add-auto-housekeeping-settings-field-v1` |
| 8 | 895 | `retireMemoryVectorEntriesMigration` | `retire-memory-vector-entries-v1` | `sqlite-initial-schema-v1`, `normalize-vector-storage-v1`, `fix-text-embeddings-after-update-v1`, `normalize-embeddings-unit-vectors-v1`, `apply-embedding-profile-truncation-v1`, `quantize-embeddings-v1` |
| 9 | 897 | `addMemoryExtractionCursorsMigration` | `add-memory-extraction-cursors-v1` | `add-memory-tiers-v1`, `add-episode-extraction-watermark-v1` |
| 10 | 899 | `dropInterCharacterMemoryJobsMigration` | `drop-inter-character-memory-jobs-v1` | `sqlite-initial-schema-v1` |
| 11 | 901 | `addTfidfTextShapeVersionMigration` | `add-tfidf-text-shape-version-v1` | `sqlite-initial-schema-v1`, `create-embedding-tables-v1` |
| 12 | 903 | `dropHousekeepingConsolidationJobsMigration` | `drop-housekeeping-consolidation-jobs-v1` | `sqlite-initial-schema-v1` |

All twelve: `introducedInVersion: '4.10.0'`, not `resumable`.

**`lib/startup/prettify.ts` `PRETTY_LABELS` (`:160-171`, NO-PORT, record in
module docs only):** `add-memory-superseded-at-v1` "Chalking a date on every
bottle laid down in the cellar…" (`:160`); `recompute-reinforced-importance-v1`
"Re-weighing every remembrance on an honest scale…" (`:161`);
`rezone-legacy-memory-occurred-at-v1` "Resetting the older diary dates to the
household clock…" (`:162`); `move-memory-maintenance-settings-v1` "Moving the
Commonplace Book’s tidying orders to the household ledger…" (`:163`, U+2019);
`add-memories-superseded-by-index-v1` "Cross-indexing the cellar, so every
bottle may be traced to its vintage…" (`:164`); `retire-memory-vector-entries-v1`
"Retiring the duplicate ledger of recollections, now that each one keeps its
own…" (`:165`); `add-memory-extraction-cursors-v1` "Gathering every chronicle’s
bookmarks into one drawer…" (`:166`); `drop-inter-character-memory-jobs-v1`
"Dismissing a clerk who had nothing left to file…" (`:167`);
`add-tfidf-text-shape-version-v1` "Noting which edition of the word-ledger each
index was drawn from…" (`:168`); `drop-housekeeping-consolidation-jobs-v1`
"Retiring two charwomen in favour of one capable housekeeper…" (`:169`);
`add-chat-scenario-source-v1` "Noting where each chat's scene was found" (`:170`,
no ellipsis); `add-episode-extraction-watermark-v1` "Slipping a bookmark into
each chronicle, lest old chapters be retold…" (`:171`).

**Classification for v5's boot-ensure design:**

| id | class | v4 once-only mechanism | v5 shape (proposed) |
|---|---|---|---|
| `add-memory-superseded-at-v1` | column + backfill, **convergent** (re-run stamps only rows still NULL) | `shouldRun` = column missing OR superseded rows unstamped | unstamped, table-gated ensure |
| `add-chat-scenario-source-v1` | two ADD COLUMNs | `shouldRun` = either column missing | unstamped ensure |
| `add-episode-extraction-watermark-v1` | column + backfill (column later dropped) | `shouldRun` = column missing — **true again on every instance once the cursor migration dropped the column; only the ledger stops it** | fold into the cursor ensure (§3 Δ9, §9 O-3) |
| `recompute-reinforced-importance-v1` | **DATA REWRITE** — pure function of (importance, count), so a re-run is a no-op unless a writer used the old formula | `shouldRun` = the three columns exist → **always true**; ledger only | ledger-gated, STAMPED (ruling) |
| `rezone-legacy-memory-occurred-at-v1` | **DATA REWRITE, NOT idempotent** (a re-run would move every NEW UTC-midnight stamp — e.g. "last night" = 18:00 local in a UTC−6 zone — again) | `shouldRun` = `occurredAt` exists → **always true**; ledger only | ledger-gated, STAMPED (ruling) |
| `add-memories-superseded-by-index-v1` | index | `shouldRun` = index missing | covered by the P4.160 family backfill once `migration_indexes.json` carries it (§3 Δ6) |
| `move-memory-maintenance-settings-v1` | copy + key delete + 3 DROP COLUMNs, convergent | `shouldRun` = a dead column OR the dead key present | unstamped ensure, atomic (P4.D251 precedent) |
| `retire-memory-vector-entries-v1` | verify-and-copy + 2 DROP TABLEs, **destructive**, convergent | `shouldRun` = either table exists | unstamped ensure (RULED: mirror) |
| `add-memory-extraction-cursors-v1` | CREATE + copy + 2 DROP COLUMNs, **destructive**, convergent | `shouldRun` = table missing OR a legacy column present | unstamped ensure (RULED), replaces P4.D265's arm |
| `drop-inter-character-memory-jobs-v1` | row DELETE, convergent | `shouldRun` = rows of the type exist | unstamped ensure |
| `add-tfidf-text-shape-version-v1` | ADD COLUMN | `shouldRun` = column missing | unstamped ensure, two shapes |
| `drop-housekeeping-consolidation-jobs-v1` | row DELETE, convergent | `shouldRun` = rows of either type exist | unstamped ensure |

### 1.2 `add-memory-superseded-at-v1` (130 lines)

`COLUMN = 'supersededAt'`, `LOG_CONTEXT = 'migration.add-memory-superseded-at-v1'`.
`assessWork()` (`:41-61`): `{needsColumn, unstampedRows}` — not SQLite / no
`memories` → zeros; `needsColumn = !sqliteColumnExists('memories','supersededAt')`;
no `supersededById` column → return; count `SELECT COUNT(*) AS n FROM memories
WHERE supersededById IS NOT NULL` (column missing) or `… AND supersededAt IS
NULL` (present). `shouldRun` (`:69-73`) = `needsColumn || unstampedRows > 0`.
`run()` (`:75-129`): `addColumnIfMissing('memories','supersededAt','TEXT DEFAULT
NULL')` → INFO `` `Added memories.supersededAt column` `` {context}; then, when
`memories.supersededById` exists:

```sql
UPDATE memories
                SET supersededAt = updatedAt
              WHERE supersededById IS NOT NULL AND supersededAt IS NULL
```

`changes > 0` → INFO `Backfilled supersededAt from updatedAt for superseded
memories` {context, rows}. Message `` `Memory supersession clock in place
(${itemsAffected} changes)` ``. ERROR `Memory supersededAt migration failed`
{context, error}; failure message `` `Memory supersededAt migration failed: ${msg}` ``.

### 1.3 `add-chat-scenario-source-v1` (100 lines)

`shouldRun` (`:37-50`): SQLite AND `chats` exists AND (`scenarioSource` OR
`scenarioNotes` missing). `run()`: `addColumnIfMissing('chats',
'scenarioSource','TEXT DEFAULT NULL')`, then `'scenarioNotes'` same DDL; no
backfill. INFO `Added the scenario source columns to chats` {context:
`migration.add-chat-scenario-source`, columnsAdded, durationMs}; message
`` `Added ${n} scenario source column(s) to chats table` ``; ERROR `Failed to add
the scenario source columns` {context, error}.

### 1.4 `add-episode-extraction-watermark-v1` (124 lines)

`COLUMN = 'episodeExtractionWatermarkMessageId'`. Exported pure
`lastSummaryAnchor(raw: unknown): string | null` (`:35-45`): non-string or `''`
→ null; `JSON.parse` failure → null; not an array or empty → null; the LAST
element if it is a non-empty string, else null. `shouldRun` (`:54-58`): `chats`
exists AND the column missing. `run()`: add `TEXT DEFAULT NULL` → INFO
`` `Added chats.episodeExtractionWatermarkMessageId column` ``; when
`chats.summaryAnchorMessageIds` exists:

```sql
SELECT id, summaryAnchorMessageIds FROM chats
              WHERE episodeExtractionWatermarkMessageId IS NULL
                AND summaryAnchorMessageIds IS NOT NULL
                AND summaryAnchorMessageIds NOT IN ('', '[]')
```

per row `UPDATE chats SET episodeExtractionWatermarkMessageId = ? WHERE id = ?`
when `lastSummaryAnchor` is non-null (no transaction; `reportProgress` per row);
INFO `Backfilled episode watermarks from summary anchors` {context, candidates,
backfilled}. Message `` `Episode watermark in place (${backfilled} chats
backfilled)` ``; ERROR `Episode-watermark migration failed`. NOTE `'[ ]'`
passes the SQL filter and then parses to `[]` → null (skipped).

### 1.5 `recompute-reinforced-importance-v1` (140 lines) — DATA REWRITE

Imports `calculateReinforcedImportance` from
`@/lib/memory/scoring/reinforced-importance` (29 lines):

```ts
export const REINFORCED_IMPORTANCE_STEP = 0.05
export function reinforcedImportance(importance: number, count: number): number {
  const n = Number.isFinite(count) ? Math.max(1, count) : 1
  return Math.min(1.0, importance + Math.log2(n) * REINFORCED_IMPORTANCE_STEP)
}
export const calculateReinforcedImportance = reinforcedImportance
```

`BATCH_SIZE = 500`, `EPSILON = 1e-9`. `shouldRun` (`:49-57`): `memories` exists
AND `reinforcedImportance`, `reinforcementCount`, `importance` all exist —
**true on every boot; only the ledger makes it once-only.** `run()`: reads
ALL rows up front `SELECT id, importance, reinforcementCount, reinforcedImportance
FROM memories` (no ORDER BY — rowid scan order); INFO `Recomputing reinforced
importance` {context, total}; per batch of 500: `value =
calculateReinforcedImportance(row.importance ?? 0.5, row.reinforcementCount ?? 1)`;
write when stored is NULL OR `|stored − value| > 1e-9`; each batch's writes in one
`db.transaction`; DEBUG `Recomputed reinforced-importance batch` {context,
scanned, total, rewritten}; INFO `Recomputed reinforced importance` {context,
total, rewritten}. Message `` `Recomputed reinforced importance on ${n} of
${total} memories` ``; ERROR `Reinforced-importance recompute failed`.
**v5 today computes `log2(count + 1)`** (`memory_gate.rs:45-47`, used by
`db/memories.rs:784`, `consolidation_plan.rs`, `memory_merge.rs`,
`services/memory_dedup.rs`, `services/memory_gate.rs`) — every v5-written row
sits up to +0.05 high; the recompute fixes the stock, the formula port fixes the
flow. Both must land in the same round.

### 1.6 `rezone-legacy-memory-occurred-at-v1` (182 lines) — DATA REWRITE, NOT idempotent

Imports `getMemoryTimeZone`, `isValidTimeZone`, `zonedDayBoundsIso` from
`@/lib/memory/time/calendar` (209 lines at the pin). `BATCH_SIZE = 500`,
`DAY_MS = 86_400_000`.

- `resolveInstanceTimeZone(db)` (`:68-77`): when `chat_settings` and its
  `timezone` column exist, `SELECT timezone FROM chat_settings WHERE timezone IS
  NOT NULL` (rowid order) and take the FIRST that `isValidTimeZone` accepts;
  then `getMemoryTimeZone(configured)` = valid argument → (cached instance zone,
  empty at boot) → valid `process.env.QUILLTAP_TIMEZONE` → `systemTimeZone()`
  (`Intl…resolvedOptions().timeZone`, else `'UTC'`). **Note:** a real v4
  `chat_settings` has NO `timezone` column (`provisioning_equivalence`'s
  `TABLE_COLUMN_ASYMMETRY`: "a generateDDL column no v4 migration adds"), so on
  real v4 data the zone is env → system. v5-fresh instances have the column.
- `rezonedValue(occurredAt, tz)` (`:80-86`): `ms = Date.parse(occurredAt)`; not
  finite or `ms % DAY_MS !== 0` → null; `date = new
  Date(ms).toISOString().slice(0,10)`; `local = zonedDayBoundsIso(date,
  tz)?.from`; return `local` only when `local !== occurredAt` (**a STRING
  compare** — in a zero-offset zone a stored `…T00:00:00Z` without millis is
  still rewritten to `…T00:00:00.000Z`; the header's "no-op" is only true for
  the `.000Z` spelling).
- `zonedDayBoundsIso(date, tz)` (`calendar.ts:191-207`): `YYYY-MM-DD` regex +
  `Date.UTC` round-trip check; `from = zonedTimeToUtcMs(y, m, d, 0, tz)`
  rendered `toISOString()`. `zonedTimeToUtcMs` (`:146-159`) is a TWO-PASS
  offset guess: `guess = Date.UTC(y,m,d,h)`; `first = offsetMs(guess)`;
  `candidate = guess − first`; `second = offsetMs(candidate)`; return
  `second === first ? candidate : guess − second`. `offsetMs` reads wall-clock
  parts through `Intl.DateTimeFormat('en-US', {hourCycle:'h23', …})`. **Its
  DST-gap answer is this algorithm's, not jiff's `compatible` disambiguation —
  a midnight that does not exist locally (e.g. `America/Santiago`,
  `America/Havana`, historical `Asia/Beirut`) needs a tier-1 row.**
- `shouldRun` (`:95-99`): `memories` exists AND `occurredAt` exists — **always
  true; ledger only.** `run()`: `SELECT id, occurredAt FROM memories WHERE
  occurredAt IS NOT NULL AND occurredAt LIKE '%T00:00:00%'` (all up front);
  INFO `Re-stamping legacy UTC-midnight memory dates` {context, timeZone,
  candidates}; per batch of 500 → `UPDATE memories SET occurredAt = ? WHERE id =
  ?` in one transaction; DEBUG `Re-stamped memory-date batch` {context, scanned,
  total, rewritten}; INFO `Re-stamped legacy UTC-midnight memory dates`
  {context, timeZone, candidates, rewritten}. Message `` `Re-stamped ${n} legacy
  memory date(s) at local midnight in ${timeZone}` ``; ERROR `Legacy memory-date
  re-stamp failed`. No re-embed (the header's reasoning). `updatedAt` is NOT
  touched.

### 1.7 `add-memories-superseded-by-index-v1` (81 lines)

`INDEX_NAME = 'idx_memories_supersededById'`. `shouldRun`: `memories` exists AND
`supersededById` exists AND the index is absent (`SELECT name FROM sqlite_master
WHERE type='index' AND name=?`). `run()`:

```sql
CREATE INDEX IF NOT EXISTS "idx_memories_supersededById" ON "memories" ("supersededById") WHERE "supersededById" IS NOT NULL
```

then re-checks existence (throws `Index was not created`); INFO
`` `Added idx_memories_supersededById index` ``; message `` `Created
idx_memories_supersededById on memories(supersededById) WHERE supersededById IS
NOT NULL` ``; ERROR `` `Failed to add idx_memories_supersededById index` ``.
SQLite stores it in `sqlite_master` WITHOUT `IF NOT EXISTS`.

### 1.8 `move-memory-maintenance-settings-v1` (151 lines)

Constants: `SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff'`,
`MAINTENANCE_KEY = 'memoryMaintenance'`, `DEAD_INSTANCE_KEY =
'memoryExtractionConcurrency'`, `DEAD_COLUMNS = ['autoHousekeepingSettings',
'memoryExtractionLimits', 'memoryExtractionConcurrency']`. Re-exports
`legacyHousekeepingToMaintenance` from `@/lib/memory/settings-legacy`.

`shouldRun` (`:64-71`): both `instance_settings` and `chat_settings` exist; any
DEAD column present → true; else `SELECT 1 FROM "instance_settings" WHERE "key" = ?`
for the dead key. `run()` (`:73-150`, context
`migration.move-memory-maintenance-settings`):

1. when `autoHousekeepingSettings` exists: `SELECT "autoHousekeepingSettings" AS
   value FROM "chat_settings" WHERE "userId" = ?` (single user) `??` `SELECT …
   ORDER BY "updatedAt" DESC LIMIT 1`; `maintenance =
   legacyHousekeepingToMaintenance(row.value)`; non-null →
   `INSERT INTO "instance_settings" ("key", "value") VALUES (?, ?) ON
   CONFLICT("key") DO NOTHING` with `JSON.stringify(maintenance)` (**no
   `phases` key** — `{enabled, perCharacterCap, perCharacterCapOverrides}`);
   DEBUG `Copied housekeeping settings into instance_settings.memoryMaintenance`
   {context, copied, enabled, perCharacterCap, overrides: <count>};
2. `DELETE FROM "instance_settings" WHERE "key" = ?` (dead key);
3. for each DEAD column still present: `ALTER TABLE "chat_settings" DROP COLUMN
   "<col>"` (no transaction around any of it);
4. INFO `Moved memory maintenance settings and dropped the dead memory knobs`
   {context, copied, droppedColumns: [...], droppedInstanceKey: bool,
   durationMs}. Message `` `Moved memory maintenance settings${copied ? '' : '
   (nothing to copy)'}; dropped ${n} column(s)` ``; ERROR `Failed to move memory
   maintenance settings` {context, error}, failure `itemsAffected: 0`.

`lib/memory/settings-legacy.ts` (91 lines):
- `legacyHousekeepingToMaintenance(raw)` (`:29-61`): a string is `JSON.parse`d
  (failure → null); not a non-array object → null; `clampCap(n)` = `typeof n ===
  'number' ? n : Number(n)`, non-finite → null, else `min(100000, max(100,
  floor(num)))`; overrides kept per entry when clampable (entry order of the
  source object); returns `{ enabled: v.enabled === true, perCharacterCap:
  clampCap(v.perCharacterCap) ?? 2000, perCharacterCapOverrides }`
  (`DEFAULT_CAP = MemoryMaintenanceSettingsSchema.parse({}).perCharacterCap`).
- `withMemoryMaintenanceFromLegacy(instanceSettings, chatSettings)` (`:77-91`,
  the RESTORE twin): drop dead keys (`memoryExtractionConcurrency`); a row with
  key `memoryMaintenance` present → return; else candidates = chat-settings rows
  that HAVE an `autoHousekeepingSettings` property; source = the single user's
  else the FIRST (not latest!); append `{ key: 'memoryMaintenance', value:
  JSON.stringify(maintenance) }`.

### 1.9 `retire-memory-vector-entries-v1` (171 lines) — RULED: mirror

`PAGE_SIZE = 500`. `mirrorBlob(value)` (`:43-51`): null/undefined → null;
`Buffer` → itself if non-empty else null (**copied byte-for-byte, never
re-quantized**); string → `parseLegacyEmbeddingText` (`float32-conversion.ts:316`:
JSON array → as-is; JSON object → `Object.values`; else undefined) → non-empty
→ `float32ToBlob` (**the QUANTIZED int8 format**, `EMBEDDING_STORAGE_DTYPE`,
`:265-267`); anything else null.

`shouldRun` (`:67-70`): SQLite AND (`vector_entries` OR `vector_indices`
exists). `run()` (`:72-170`), context `migration.retire-memory-vector-entries-v1`:

1. when BOTH `vector_entries` and `memories` exist: `total = SELECT COUNT(*) AS n
   FROM vector_entries`; INFO `Verifying memory vectors against their rows
   before retiring the mirror` {context, mirrorRows}; page `SELECT rowid AS rid,
   id, embedding FROM vector_entries WHERE rowid > ? ORDER BY rowid LIMIT ?`,
   EACH PAGE one transaction: per row `SELECT embedding IS NOT NULL AS has FROM
   memories WHERE id = ?` → no row: `orphaned++`; `has` (an EMPTY blob counts as
   has): `alreadyOnRow++`; `mirrorBlob` null: `unusable++`; else `copied +=
   UPDATE memories SET embedding = ? WHERE id = ? AND embedding IS NULL` changes;
   cursor = the page's last rowid;
2. for `vector_entries` then `vector_indices`, when present: `DROP TABLE IF
   EXISTS "<t>"`, `dropped++` (outside any transaction);
3. INFO `Retired the memory vector mirror` {context, copied, alreadyOnRow,
   orphaned, unusable, tablesDropped}. `itemsAffected = copied + dropped`;
   message `` `Memory vectors now live on their rows only (${copied} copied from
   the mirror, ${orphaned} orphan mirror rows discarded, ${dropped} tables
   dropped)` ``. ERROR `Retiring the memory vector mirror failed` {context,
   error, copied}; failure `itemsAffected: copied`.

`normalize-vector-storage.ts` `shouldRun` (`:43-48`): when `vector_entries` is
absent it now answers `sqliteTableExists('vector_indices')` (was `true`) — "both
missing means retire… has dropped the mirror for good: never recreate it". v5
has no port of that migration (verify: no `normalize_vector_storage` in
`crates/`), so nothing to change — but no v5 code may ever CREATE either table
again (§3 Δ12: the harness `CREATE TABLE vector_*` plants).

`lib/database/manager.ts:116-119` drops `registerBlobColumns('vector_entries',
…)`. `lib/startup/reconcile-embedding-dimensions.ts` drops the two vector arms
and the `vectorEntriesDeleted` / `vectorIndexMetaFixed` result fields and their
INFO fields; `lib/startup/repair-text-embeddings.ts` drops the `vector_entries`
table + `vectorEntriesRepaired`; `lib/embedding/reapply-profile.ts`
`MAIN_DB_TABLES` drops `vector_entries` and calls `memoryIndex.reloadAll()`
(WARN text `[ReapplyProfile] Failed to drop the memory index caches`).

### 1.10 `add-memory-extraction-cursors-v1` (124 lines) — RULED: port, retire P4.D265's arm

Exported `LEGACY_WATERMARK_COLUMNS = [{column:'otherExtractionWatermarkMessageId',
pass:'other-fold'}, {column:'episodeExtractionWatermarkMessageId',
pass:'episode-fold'}]`. `shouldRun` (`:44-49`): `chats` exists; cursor table
missing → true; else any legacy column present. `run()` (context
`migration.add-memory-extraction-cursors-v1`):

```sql
CREATE TABLE IF NOT EXISTS "memory_extraction_cursors" (
          "id" TEXT PRIMARY KEY,
          "chatId" TEXT NOT NULL,
          "pass" TEXT NOT NULL,
          "messageId" TEXT NOT NULL,
          "createdAt" TEXT NOT NULL,
          "updatedAt" TEXT NOT NULL,
          UNIQUE ("chatId", "pass"),
          FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE
        )
```

(the text is stored in `sqlite_master` with its own newlines/indentation; note
`"id" TEXT PRIMARY KEY` — no `NOT NULL`, unlike generateDDL's). INFO
`memory_extraction_cursors table in place` {context}. `present` = legacy
columns that exist; ONE `now = new Date().toISOString()` for every insert; per
present column (in the LEGACY order — other first): `SELECT "id", "<col>" AS
"messageId" FROM "chats" WHERE "<col>" IS NOT NULL AND "<col>" <> ''` → in ONE
transaction per column: `INSERT INTO "memory_extraction_cursors" ("id", "chatId",
"pass", "messageId", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?) ON
CONFLICT("id") DO NOTHING` with id `` `${chatId}:${pass}` ``; INFO `Copied
watermark column into cursors` {context, column, pass, rows: <rows read, not
inserted>}. Then per present column `ALTER TABLE "chats" DROP COLUMN "<col>"` +
INFO `` `Dropped chats.${column}` ``. Message `` `Extraction cursors in place
(${backfilled} carried over from ${present.length} watermark column(s))` ``;
ERROR `Extraction-cursor migration failed`. **With the FK live (`foreign_keys =
ON`) the copy can only name existing chats (it reads them from `chats`), so
no FK failure in the migration itself.**

### 1.11 `drop-inter-character-memory-jobs-v1` (66) and `drop-housekeeping-consolidation-jobs-v1` (67)

Both: `background_jobs` exists; `shouldRun` = `SELECT COUNT(*) AS n FROM
"background_jobs" WHERE "type" = 'INTER_CHARACTER_MEMORY'` (resp. `"type" IN
('MEMORY_HOUSEKEEPING', 'MEMORY_CONSOLIDATION')`) `> 0`; `run()` = the matching
`DELETE` (ANY status — history rows included). INFO `Deleted retired
INTER_CHARACTER_MEMORY jobs` {context, deleted} / `Deleted retired housekeeping
and consolidation jobs` {context, deleted}; messages `` `Deleted ${n} retired
INTER_CHARACTER_MEMORY job(s)` `` / `` `Deleted ${n} retired
housekeeping/consolidation job(s)` ``; ERRORs `Failed to delete
INTER_CHARACTER_MEMORY jobs` / `Failed to delete housekeeping and consolidation
jobs`. Context `migration.<id>`.

### 1.12 `add-tfidf-text-shape-version-v1` (73)

`shouldRun`: `tfidf_vocabularies` exists AND `textShapeVersion` missing.
`addColumnIfMissing('tfidf_vocabularies','textShapeVersion','INTEGER DEFAULT
0')`; INFO `Added textShapeVersion column to tfidf_vocabularies` {context,
durationMs}; message the same sentence; ERROR `Failed to add textShapeVersion
column`. No backfill: 0 = "fitted with anchor lines, refit once".

### 1.13 The D23 re-dump at the pin — PREDICTED statement by statement

(`harness/oracle/provision/dump-fresh-schema.ts` from a pinned worktree;
**predicted from the schemas + the translator, confirm by the real dump — D23,
never by hand**.) Committed `fresh_schema.json`: main 85 / mountIndex 31 /
llmLogs 3. Predicted pin: **main 83** / 31 / 3.

| committed line | statement | change |
|---|---|---|
| 10 | `CREATE TABLE "chat_settings"` | LOSES `"autoHousekeepingSettings" TEXT DEFAULT '{"enabled":false,…}'` and `"memoryExtractionLimits" TEXT DEFAULT '{"enabled":false,"maxPerHour":20,…}'` (between `memoryCascadePreferences` and `autonomousRoomSettings`) |
| 11 | `CREATE TABLE "chats"` | GAINS `"scenarioSource" TEXT,` + `"scenarioNotes" TEXT,` right after `"scenarioText" TEXT,` (bare TEXT — §0 #1); LOSES `"otherExtractionWatermarkMessageId" TEXT,` (after `timelineMode`). No `episodeExtractionWatermarkMessageId` (never in the pin's schema) |
| 24 | `CREATE TABLE "memories"` | GAINS `"supersededAt" TEXT,` between `"supersededById" TEXT,` and `"consolidatedFrom" TEXT DEFAULT '[]',` (`memory.types.ts:139-145`) |
| new (sorted after `memories`, before `plugin_configs`) | `CREATE TABLE "memory_extraction_cursors"` | NEW, generateDDL shape: `"id" TEXT PRIMARY KEY NOT NULL, "chatId" TEXT NOT NULL, "pass" TEXT NOT NULL, "messageId" TEXT NOT NULL, "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL` (no UNIQUE, no FK, no CHECK for the enum) |
| 33 | `CREATE TABLE "tfidf_vocabularies"` | GAINS `"textShapeVersion" REAL DEFAULT 0,` between `"fittedAt" TEXT NOT NULL,` and `"createdAt" TEXT NOT NULL,` (§0 #2 — predicted REAL) |
| 35, 36 | `vector_entries`, `vector_indices` | DELETED |
| new (sorted after `idx_memories_createdAt`) | `CREATE INDEX "idx_memory_extraction_cursors_createdAt" ON "memory_extraction_cursors" ("createdAt" DESC)` | NEW |
| 85, 86 | `idx_vector_entries_createdAt`, `idx_vector_indices_createdAt` | DELETED |

Dumper behaviour at the pin: `getRepositories()` no longer has `vectorIndices`
→ its `secondary` trigger (`dump-fresh-schema.ts:149`) records
`vectorIndices(secondary:absent)` in `skipped` (harmless; drop the row);
`memoryExtractionCursors` has the base `count()` → driven by the main loop.
`chat_settings_seed.json` (captured by the same run, `QT_SEED_OUT`) LOSES the
two column names (`:13-14`) and their values (`:50-51`) —
`ChatSettingsRepository`'s seed (`chat-settings.repository.ts:165-180 (pre)`)
dropped both objects.

### 1.14 `migration_indexes.json` at the pin — PREDICTED

Committed: `source.v4Commit 01a83539d`, main 53 / mountIndex 5 / llmLogs 5 = 63.
A real v4 first boot at the pin creates the vector tables in
`sqlite-initial-schema-v1` and DROPS them in `retire-…` (the shouldRun sees
them), so their indexes vanish with them; `memories.supersededById` exists
(tiers) so the partial index is made. Predicted **main 52**:

- INSERT after line 47 (`idx_memories_reinforcedImportance`):
  `"CREATE INDEX \"idx_memories_supersededById\" ON \"memories\" (\"supersededById\") WHERE \"supersededById\" IS NOT NULL"`;
- DELETE lines 58-59 (`idx_vector_entries_characterId`,
  `idx_vector_indices_characterId`);
- the cursor table's `UNIQUE` makes `sqlite_autoindex_memory_extraction_cursors_1`
  (`sql IS NULL` — the dumper's `indexRows` must keep ignoring NULL-sql rows;
  verify in `migrations-first.ts`).

Run with `QT_REAL_BOOT_MASTER` (REQUIRED, P4.160 R-F) from a real `tsx
server.ts` first boot at the pin.

### 1.15 Schemas at the pin (the Rust row structs follow these)

- `memory.types.ts:139-147`: `supersededById: UUIDSchema.nullable().optional()`
  (`:139`); **`supersededAt: TimestampSchema.nullable().optional()` (`:145`)**;
  `consolidatedFrom: z.array(UUIDSchema).default([])`; `consolidatedAt`. The
  `reinforcedImportance` comment now reads `importance + log2(count)*0.05`.
- `chat.types.ts`: `ScenarioSourceSchema` (`:795`) `{ mountPointId: UUIDSchema,
  path: z.string().min(1).max(500), bodySha256: z.string().length(64) }`;
  `ScenarioRefSchema` `{ mountPointId, path }`; `scenarioSource:
  ScenarioSourceSchema.nullable().optional()` (`:926`, Base `:1319`),
  `scenarioNotes: z.string().nullable().optional()` (`:929`, Base `:1322`);
  `otherExtractionWatermarkMessageId` GONE from both (`:1114` comment: "The
  fold-grain extraction passes' read positions live in
  `memory_extraction_cursors`").
- NEW `memory-extraction-cursor.types.ts` (48 lines):
  `ExtractionCursorPassEnum = z.enum(['other-fold', 'episode-fold', 'turn'])`
  (`'turn'` from `64743ee13`, bug 245); `FoldCursorPass = Exclude<…,'turn'>`;
  `EXTRACTION_CURSOR_PASSES`; `MemoryExtractionCursorSchema = { id:
  z.string().min(1), chatId: UUIDSchema, pass: ExtractionCursorPassEnum,
  messageId: z.string().min(1), createdAt, updatedAt }`;
  `extractionCursorId(chatId, pass) = `${chatId}:${pass}``;
  `ExtractionCursorMap = Partial<Record<pass, string>>`.
- `embedding-job.types.ts:56`: `textShapeVersion:
  z.number().int().nonnegative().default(0)` between `fittedAt` and `createdAt`.
- `settings.types.ts`: `MEMORY_MAINTENANCE_CAP_MIN = 100`,
  `MEMORY_MAINTENANCE_CAP_MAX = 100_000` (`:135-136`);
  `MemoryMaintenanceCapSchema = z.number().int().min(100).max(100000)`;
  `MemoryMaintenanceCapOverridesSchema = z.record(z.string(), CapSchema)`;
  `MemoryMaintenancePhaseTogglesSchema` (`:153`) `{ foldNearDuplicates:
  boolean.default(true), demote: boolean.default(true), retire:
  boolean.default(true) }`; `MemoryMaintenanceSettingsSchema` (`:170`) `{
  enabled: boolean.default(false), perCharacterCap: CapSchema.default(2000),
  perCharacterCapOverrides: OverridesSchema.default({}), phases:
  TogglesSchema.prefault({}) }` (a stored value without `phases` parses with
  every phase ON); the `AutoHousekeeping*` schemas are DELETED;
  `MemoryConsolidationSettingsSchema` (`:252`) GAINS `tfidfClusterThreshold:
  z.number().min(0).max(1).default(0.30)` after `clusterThreshold`;
  `MemoryExtractionLimitsSchema` (`:315`) `maxPerHour:
  z.number().int().positive().max(10_000).default(20)`; `ChatSettingsSchema`
  (`:731`) LOSES `autoHousekeepingSettings` and `memoryExtractionLimits`.
- NEW `lib/schemas/patch-schema.ts` `patchSchemaOf` (bug 244 — strips
  `.default()` / `.prefault()` then `.optional()`); used by every `*-config`
  write. Cross-lane (settings); the keystone's settings setters must accept a
  PATCH that names only some keys (§7 C1-5).
- `job.types.ts:20-43`: `BackgroundJobTypeEnum` = `MEMORY_EXTRACTION,
  CONTEXT_SUMMARY, TITLE_UPDATE, LLM_LOG_CLEANUP, EMBEDDING_GENERATE,
  EMBEDDING_REFIT, EMBEDDING_REINDEX_ALL, EMBEDDING_REAPPLY_PROFILE,
  STORY_BACKGROUND_GENERATION, CHAT_DANGER_CLASSIFICATION, SCENE_STATE_TRACKING,
  CHARACTER_AVATAR_GENERATION, WARDROBE_ITEM_IMAGE_GENERATION,
  CONVERSATION_RENDER, MEMORY_MAINTENANCE, MEMORY_REGENERATE_CHAT,
  MEMORY_REGENERATE_ALL, WARDROBE_OUTFIT_ANNOUNCEMENT, AUTONOMOUS_ROOM_TURN,
  AUTONOMOUS_ROOM_SCHEDULE_TICK, CARINA_MEMORY_EXTRACTION,
  CHARACTER_HEADSHOULDERS_BACKFILL, REGENERATE_CONVERSATION_SUMMARIES` — 23
  (was 25): `INTER_CHARACTER_MEMORY` gone; `MEMORY_HOUSEKEEPING` +
  `MEMORY_CONSOLIDATION` → ONE `MEMORY_MAINTENANCE` in the housekeeping slot.
  `handlers/index.ts` registers `MEMORY_MAINTENANCE: handleMemoryMaintenance`
  (`:55`).
- Payloads (`lib/background-jobs/queue-service.ts`, 1603 lines):
  `MemoryExtractionPayload` (`:55-110`) = `{ chatId, turnOpenerMessageId:
  string|null, extractionAnchorMessageId?: string|null, extractAfterMessageId?:
  string|null (bug 245), connectionProfileId, pass?: 'turn'|'other-fold'|'episode-fold',
  cursor?: string|null, ignoreIdle?: boolean, backfill?: boolean,
  foldOtherCatchup?/foldOtherIgnoreIdle?/foldEpisodeCatchup?: boolean
  (@deprecated) }`; `memoryExtractionPassOf(p)` (`:116-121`) = `p.pass` ||
  (`foldEpisodeCatchup` → `'episode-fold'`) || (`foldOtherCatchup` →
  `'other-fold'`) || `'turn'` (JS truthiness). `MemoryMaintenancePayload`
  (`:329-345`) = `{ characterId, phases?: MaintenancePhase[], reason:
  'scheduled'|'watermark'|'manual', exemptPhases?: MaintenancePhase[],
  settings?: { consolidation?: Partial<MemoryConsolidationSettings>,
  maxClustersPerRun?: number, cap?: number } }`; `MAINTENANCE_PHASES =
  ['fold-near-duplicates','consolidate','demote','retire','mirror']`
  (`lib/memory/maintenance/types.ts:15`); `withImpliedPhases` adds `mirror`
  with `consolidate`.

### 1.16 Repositories at the pin

**`memories.repository.ts` (1921 lines) — the verb inventory.** Removed since
`(pre)`: `bulkDelete` (→ `bulkDeleteUnsafe`), `countHotByCharacterId`,
`deleteByChatId`, `deleteBySourceMessageId`, `deleteForCharacter`,
`findColdIdsByCharacterId`, `findExpiredColdIds`, `replaceInMemories`. New:
`addRelatedMemoryIds` / `removeRelatedMemoryIds` (`:676`, `:688`, via private
`updateRelatedMemoryIdsAtomically` `:696-765`), `removeRelatedMemoryIdsEverywhere`
(`:767`), `appendReinforcementFootnotes` (`:820`), `bulkDeleteUnsafe` (`:882`),
`countCapBreakdown` (`:1005`), `countCorpusSince` (`:980`), `findDanglingLinks`
(`:1290`), `findDigestSubjectsByIds` (`:1432`), `findDigestsContainingMembers`
(`:1234`), `findIdsSupersededByAny` (`:1349`), `findIndexRowsByCharacterId`
(`:1060`), `removeConsolidatedMembers` (`:1387`). Changed:
`incrementReinforcement` (`:622`), `updateTierBulk` (`:1148`), `delete` (`:868`).
Key exact facts (the rest are the sibling lanes' — retire, maintenance, repair,
admission — consumers, but the VERBS are keystone):

- `delete(id)` (`:868-871`) — **refuses**: `logger.error('Refused a bare memory
  delete; memories are deleted through retireMemories', { memoryId: id })` then
  `throw new Error('MemoriesRepository.delete is refused: delete memories
  through retireMemories (lib/memory/retire.ts)')` (NOT inside `safeQuery`).
- `bulkDeleteUnsafe(characterId, ids)` (`:882-905`): `deleteMany({characterId,
  id: {$in: chunk}})` per `SQLITE_VARIABLE_CHUNK_SIZE` chunk, summed; RETHROW
  `Error bulk deleting memories for character` {characterId, count}. "a test
  greps for any other caller" (only `retire.ts`).
- `incrementReinforcement(characterId, memoryId, at, by = 1)` (`:622-670`):
  `by` not a positive integer → `throw new Error(`Memory reinforcement: delta
  must be a positive integer (got ${by})`)` BEFORE `safeQuery`; transaction:
  `SELECT "importance", "reinforcementCount", "lastReinforcedAt" FROM "memories"
  WHERE "id" = ? AND "characterId" = ?`; `count = (count ?? 1) + by`;
  `reinforcedImportance = calculateReinforcedImportance(importance ?? 0.5,
  count)`; `lastReinforcedAt = stored && stored > at ? stored : at` (string
  compare, forward-only — bug 226); UPDATE the four columns (+`updatedAt` =
  `getCurrentTimestamp()` minted BEFORE the transaction); returns
  `{reinforcementCount, reinforcedImportance, lastReinforcedAt}`; WARN `Memory
  not found for reinforcement` {memoryId, characterId}; DEBUG `Memory
  reinforcement counted` {memoryId, characterId, by, reinforcementCount,
  reinforcedImportance, lastReinforcedAt}; RETHROW `Error reinforcing memory`.
- `updateTierBulk(characterId, ids, tier, extra = {})` (`:1148-1191`): ONE `now
  = new Date().toISOString()`; `supersession = extra.supersededById !==
  undefined ? { supersededAt: extra.supersededById ? now : null } : {}`; per
  chunk `updateMany({characterId, id: {$in}}, {tier, ...extra, ...supersession,
  updatedAt: now})`; DEBUG `Moved memories between tiers` {characterId, tier,
  requested, changed, supersededById: extra.supersededById ?? undefined};
  FALLBACK 0 `Error moving memories between tiers`. "Called only by
  `memoryIndex.setTier`".
- `findIndexRowsByCharacterId(characterId)` (`:1060-1093`): `SELECT id,
  embedding, tier FROM memories WHERE characterId = ? AND embedding IS NOT
  NULL`; `decodeStoredEmbedding` (`:69-84`: Float32Array / Buffer →
  `blobToFloat32` / Uint8Array / number[] / JSON-array text; else null); empty
  or undecodable → skipped and counted; tier `'cold'` iff the cell is
  `'cold'`, else `'hot'` (NULL → hot); WARN `Skipped memory embeddings that
  could not be decoded` {characterId, undecodable}; DEBUG `Read memory index
  rows` {characterId, rows}; FALLBACK `[]` `Error reading memory index rows`.
- `removeRelatedMemoryIdsEverywhere(ids)` (`:767-818`): chunk size
  `max(1, floor((SQLITE_VARIABLE_CHUNK_SIZE − 1) / 2))`; per chunk the
  `json_group_array … WHERE value NOT IN (…)` UPDATE guarded by `json_valid` +
  `EXISTS (… value IN (…))`; DEBUG `Scrubbed deleted ids from related links`
  {ids, changed}; FALLBACK 0 `Error scrubbing related memory links`.
- `countCapBreakdown(characterId)` (`:1005-1031`) → `{hotTotal, demotable
  (hot AUTO), protected = hotTotal − demotable, cold}`; FALLBACK zeros `Error
  counting memories against the housekeeping cap`.
- `findDanglingLinks(characterIds?)` (`:1290-1345`) — **RETHROW** ("a scan that
  failed must not read as a clean graph"), `Error scanning for dangling memory
  links`.
- `HOT_TIER_SQL = `COALESCE(tier, 'hot') = 'hot'`` (`:51`).

**NEW `memory-extraction-cursors.repository.ts` (151 lines)** —
`AbstractBaseRepository<MemoryExtractionCursor>` over
`'memory_extraction_cursors'`. "Reads throw on failure rather than answering
'no cursor'" (`:9-11`):

| verb | line | SQL / behaviour | safeQuery mode + message |
|---|---|---|---|
| `create(data, opts)` | 36 | `_create` with id `extractionCursorId(chatId, pass)` unless given | (base) |
| `update` / `delete` | 43 / 47 | base `_update` / `_delete` | (base) |
| `findByChatId(chatId)` | 56 | `findByFilter({chatId})` → map pass→messageId (row order; later row wins) | RETHROW `Error finding extraction cursors for chat` {chatId} |
| `findCursor(chatId, pass)` | 68 | `_findById('<chatId>:<pass>')?.messageId ?? null` | RETHROW `Error finding extraction cursor` {chatId, pass} |
| `findAllByChat()` | 77 | `_findAll()` → `Map<chatId, map>` (insertion order) | RETHROW `Error listing extraction cursors` {} |
| `setCursor(chatId, pass, messageId)` | 102 | `INSERT INTO "memory_extraction_cursors" ("id", "chatId", "pass", "messageId", "createdAt", "updatedAt") VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT("id") DO UPDATE SET "messageId" = excluded."messageId", "updatedAt" = excluded."updatedAt"` (ONE `now` for both stamps); DEBUG `[ExtractionCursors] Cursor set` {chatId, pass, messageId} | RETHROW `Error setting extraction cursor` {chatId, pass} |
| `clearCursor(chatId, pass)` | 123 | `deleteMany({id})`; DEBUG `[ExtractionCursors] Cursor cleared` {chatId, pass, removed} | FALLBACK `false`, `Error clearing extraction cursor` |
| `clearForChat(chatId)` | 137 | `deleteMany({chatId})` | FALLBACK 0, `Error clearing extraction cursors for chat` |

`ON CONFLICT("id")` targets the PRIMARY KEY, so the upsert works on BOTH table
shapes. Callers: `lib/memory/extraction/cursors.ts:34` (`liveCursorStore` — read
/ advance / clear, DEBUG `[ExtractionCursors] Advanced` {chatId, pass,
messageId}), `extraction/catchup.ts:155`, `extraction/cursor-bundle.ts:58,80,112`
(`BUNDLE_CURSOR_FIELDS = { 'other-fold': 'otherExtractionWatermarkMessageId',
'episode-fold': 'episodeExtractionWatermarkMessageId', turn:
'turnExtractionWatermarkMessageId' }`), `scheduled-maintenance.ts:273`,
`handlers/memory-regenerate-chat.ts:59` (`clearForChat`). **No chat-delete path
clears cursors explicitly** — v4 relies on the FK cascade (§0 #4).

**`chat-settings.repository.ts`**: the getOrCreate seed (`:165-180 (pre)`) loses
`autoHousekeepingSettings` / `memoryExtractionLimits`. **`chats.repository.ts`
`patchOnlyFields` (`:560-562`)** = `[...CONCIERGE_MODE_FIELDS]` (the watermark
entry gone). **`background-jobs.repository.ts`** + `findActiveByType(type)`
(`:103-116`, `{type, status: {$in: ['PENDING','PROCESSING']}}` sorted
`createdAt` ASC, FALLBACK `[]` `Error finding active background jobs by type`).
**`tfidf-vocabulary.repository.ts`** unchanged (the column rides the schema;
`handlers/embedding-refit.ts:135` writes `textShapeVersion:
LEXICAL_TEXT_SHAPE_VERSION`). **`instance-settings/index.ts`**: DELETED
`getMemoryExtractionConcurrency` / `setMemoryExtractionConcurrency` and the
`memoryExtractionConcurrency` key constant; NEW `KEY_MEMORY_MAINTENANCE =
'memoryMaintenance'` (`:46`), `getMemoryMaintenanceSettings()` (`:266-272`,
`readJsonSetting` — WARN `` `[InstanceSettings] memoryMaintenance failed to
parse — using defaults` `` {error}, defaults = `Schema.parse({})`),
`setMemoryMaintenanceSettings(value: Partial)` (`:274-282`, shallow `{...current,
...value}` then `writeJsonSetting` = `schema.parse` + `JSON.stringify`); every
default now `Schema.parse({})`. `lib/memory/settings.ts`
`resolveMemoryCap(settings, characterId)` = override → `perCharacterCap` → 2000;
`resolveMemoryMaintenance()`; `resolveMemorySettings(userId?)` (DEBUG
`[MemorySettings] Resolved memory settings` {userId, maintenanceEnabled,
perCharacterCap, capOverrides, consolidationEnabled, otherPass}).

### 1.17 The vector store / index store and the ONE deletion path (C1-level)

- `lib/embedding/vector-store.ts` (494): `CharacterVectorStore.load()`
  (`:115-153`) = `memories.findIndexRowsByCharacterId` → entries tagged with the
  row tier; DEBUG `Loaded vector index from memory rows` {context:
  'CharacterVectorStore.load', characterId, entries, cold, dimensions}; a throw
  → ERROR `Error loading vector index from memory rows` and an EMPTY store.
  `save()` / `saveMeta` are GONE; dimensions = the DOMINANT stored length
  (`getDimensions` `:192-202`, ties → first length inserted); `search(q, limit,
  filter?)` (`:221-247`) returns `[]` + WARN `Query vector dimension mismatch —
  search embedding profile likely differs from index` {context, characterId,
  storedDimensions, queryDimensions} only when NO entry has the query's length,
  else scores only same-length entries. `VectorStoreManager` (`:403-480`)
  caches loaded stores.
- `lib/memory/index-store/memory-index.ts` (259): `memoryIndex` = the ONLY
  writer of `memories.embedding` / `memories.tier`: `upsert(characterId,
  [{id, embedding, tier?, alongside?}])` (the embedding + `alongside` fields in
  ONE row update), `indexInserted`, `setTier(characterId, ids, tier, extra?)`
  (→ `updateTierBulk`), `remove(characterId, ids)` (cache eviction only),
  `reload` / `reloadAll`; a cache failure drops the cache (`withLoadedStore`).
  `index-store/embedding-text.ts` (`embeddingTextFor`,
  `LEXICAL_TEXT_SHAPE_VERSION`, `lexicalEmbeddingTextFor`),
  `index-store/backfill.ts` (`generateMissingEmbeddings`).
- `lib/memory/policy-protection.ts`: `policyProtectionOf({source, tier})` →
  `'manual'` (MANUAL) | `'digest'` (CONSOLIDATED) | `'cold'` (tier cold) | null;
  `partitionByPolicyProtection(rows)` → `{actionable, spared, sparedBy:{manual,
  digest, cold}}`.
- `lib/memory/retire.ts` (569): `RetireReason` (`:49-71`) = `'item-delete' |
  'chat-wipe' | 'message-cascade' | 'message-reattribution' | 'swipe-regenerate'
  | 'character-delete' | 'character-archive' | 'restore-replace' | 'retention' |
  'near-duplicate-fold' | 'graph-repair'`; `RetireOptions { reason, keep?:
  'manual' }`; `RetireReport { requested, retired, retiredIds, spared, sparedBy,
  digestsRetiredWithLastMember, vectorsRemoved, neighboursTouched,
  digestLinksScrubbed, charactersAffected }`; `retireMemories(ids, opts)`
  (`:384-448`): dedupe → `planRetirement` (resolve rows; `keep:'manual'` spares
  MANUAL / cold / a digest with a LIVING member; unless `reason === 'retention'`
  cascade up to `MAX_DIGEST_CASCADE_PASSES = 3` passes retiring digests left with
  no living member) → `removeRelatedMemoryIdsEverywhere(scrubIds)` (with `keep`,
  only doomed ids; else requested ∪ doomed) → `scrubDigestLinks`
  (`removeConsolidatedMembers` on surviving digests;
  `memoryIndex.setTier(…,'cold',{supersededById:null})` on orphaned members) →
  per holder `bulkDeleteUnsafe` + `memoryIndex.remove` → WARN `Retirement
  rewrote an unusually large neighbour set` (≥ `NEIGHBOUR_WARN = 200`) + DEBUG
  `Retired memories` {reason, keep, requested, retired, spared, sparedBy,
  digestsRetiredWithLastMember, vectorsRemoved, neighboursTouched,
  digestLinksScrubbed, charactersAffected, durationMs} → `announceRetirement`
  (parent only: `invalidateFrozenArchive` per holder, `publishRealtime('memories')`,
  Commonplace mirror refresh for removed digests' buckets; DEBUG `Refreshed
  Commonplace mirror after retiring a digest`, WARN `Commonplace mirror refresh
  after retiring a digest failed`). `retireChatMemories(chatId)` (`:543-555`) =
  `findByChatId` → `retireMemories(ids, {reason:'chat-wipe', keep:'manual'})` +
  INFO `Chat wipe spared manual, cold and digest rows` {chatId, spared, sparedBy}
  (when spared > 0) + DEBUG `Wiped a chat's extracted memories` {chatId, retired,
  spared}. `previewChatWipe(chatId)` (`:562-569`). `scrubDanglingMemoryLinks`
  (`:480`, repair). Logger child `module: 'memory:retire'` on every line.

### 1.18 The memories wire at the pin (`app/api/v1/memories/route.ts`, committed)

GET actions (`:200-211`, `availableActions` in this order): `maintenance-status`,
`embeddings`, `maintenance-config`, `extraction-limits-config`, `recall-config`,
`consolidation-config`, `extraction-mode-config`, `backfill-embeddings`,
`character-memory-counts`, `regenerate-all`. POST (`:257-270`): `search`,
`maintain`, `repair`, `embeddings`, `maintenance-config`,
`extraction-limits-config`, `recall-config`, `consolidation-config`,
`extraction-mode-config`, `backfill-embeddings`, `regenerate-all`,
`anchor-gate-probe`. PUT: `embeddings`. Retired (`housekeep` GET+POST,
`housekeep-sweep`, `consolidate`, `housekeeping-config` GET+POST,
`extraction-concurrency` GET+POST) now answer `dispatchAction`'s unknown arm
(`lib/api/middleware/actions.ts:82-98`): WARN `Unknown action requested`
{action, availableActions, method, path} + **400 `{ error: 'Unknown action:
<action>', availableActions: [<that verb's list>] }`**. Schemas the keystone's
wire variants carry RAW (R-H): `maintainSchema` (`:129-141`) `{characterId?:
z.uuid(), phases?: z.array(z.enum(MAINTENANCE_PHASES)).min(1), dryRun?:
boolean, settings?: {consolidation?: patchSchemaOf(Consolidation),
maxClustersPerRun?: int 1..1000, cap?: CapSchema}}`; `repairSchema`
(`:144-148`) `{characterId?: z.uuid(), dryRun?: boolean}`;
`maintenanceConfigSchema` (`:162-167`) `{enabled?, phases?:
patchSchemaOf(Toggles), perCharacterCap?: CapSchema, perCharacterCapOverrides?:
OverridesSchema}` (unknown keys stripped); `maintenance-status` takes
`?characterId=`. `/system/tools`: `memory-dedup-preview` (GET) and
`memory-dedup` (POST) GONE → the same unknown-action 400 with the tools route's
lists; `INTER_CHARACTER_MEMORY` gone from `estimateTokensForJob` and
`getJobTypeName` (no `MEMORY_MAINTENANCE` label — the raw type renders).

### 1.19 Topics / activity / dispatcher

`lib/realtime/job-topics.ts:78-91`: `MEMORY_EXTRACTION | CARINA_MEMORY_EXTRACTION
| MEMORY_REGENERATE_CHAT` → `[{topic:'memories', id: chatId}]`;
`MEMORY_MAINTENANCE` → `[{topic:'memories'},{topic:'mountPoints'}]` (NO dryRun
clause — a dry run never enqueues). `activity-kinds.ts:40-46`:
`MEMORY_MAINTENANCE: 'memory'` in the Mem block (after `MEMORY_REGENERATE_ALL`).
`job-dispatcher.ts:663` `JOBS_INVALIDATING_FROZEN_ARCHIVE = new
Set(['MEMORY_MAINTENANCE'])`; `commonplace-mirror.ts:49` `MIRRORING_JOB_TYPES`
the same. `handleRegenerateAll`'s wipe set (`route.ts:1283-1289`) =
`['MEMORY_REGENERATE_ALL','MEMORY_REGENERATE_CHAT','MEMORY_EXTRACTION']`.

---

## 2. What v4 DELETED in the span that v5 still carries (keystone-relevant)

| v4 deleted | v5 twin (file:line on `a7c6f0d03`) |
|---|---|
| `vector-indices.repository.ts` (−381) + `vector-indices.types.ts` (−104) | `crates/quilltap-core/src/db/vector_indices.rs` (347 lines, whole); `db/mod.rs:150` (`pub mod vector_indices`), `:566-568` (`Writers::vector_indices()`) |
| `CharacterVectorStore.save` / `saveMeta` / the mirror load | `db/vector_store.rs:124-176` (`load` reads `vector_indices` + `vector_entries` + `find_cold_ids_by_character_id`), `:392-…` (`flush`), `:35` import of the repo |
| `memories.findColdIdsByCharacterId`, `findExpiredColdIds`, `countHotByCharacterId` | `db/memories_read.rs:1020`, `:1097`, `:992` (callers: `vector_store.rs:129`, `housekeeping.rs:490,614`, `memory_gate.rs:1497`) |
| `memories.bulkDelete`, `deleteByChatId`, `deleteBySourceMessageId`, `deleteForCharacter`, `replaceInMemories`; bare `delete` now REFUSES | `db/memories.rs:534` (`delete`), `:598` (`delete_for_character`), `:639` (`bulk_delete`), `:928` (`delete_by_chat_id`), `:936` (`delete_by_source_message_id`), `:948` (`replace_in_memories`, caller `services/search_replace.rs:300`) |
| `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch[Report]` (memory-gate) | `db/memories.rs:1052` (`delete_with_unlink`), `:1141` (`delete_many_with_unlink`) + `CountSource` (`:139`) |
| `memory-service.ts` `deleteMemoryWithVector` / `deleteMemoriesBy{SourceMessage,SourceMessages,ChatId}WithVectors` | `services/memory_service.rs:106`, `:150`, `:166`, `:189` |
| `AutoHousekeeping*` schemas; `chat_settings.autoHousekeepingSettings` / `memoryExtractionLimits` | `db/chat_settings.rs:229-253` (structs), `:838-839` (row fields), `:1052-1055` + `:1118-1119` (INSERT), `:1408-1409` (tolerant SELECT list), `:1496-1500` (POSITIONAL reads `r.get(12)` / `(13)`), `:1730-1760` (`find_auto_housekeeping_settings_by_user_id`), `:1762-1800` (`SchedulerUserSettings` + `find_all_scheduler_settings` SELECTing the column) |
| `get/setMemoryExtractionConcurrency` + the instance key | `db/instance_settings.rs:527-558`; wire `api/memories.rs:1477-1500`, `api/engine.rs:3889-3897`, `api/types.rs:1724-1729` |
| `chats.otherExtractionWatermarkMessageId` (schema + patch-only) | `db/chats_read.rs:96` (ALL_COLUMNS), `:322` (POSITIONAL `row.get(73)`, every later read indexed after it); `db/chats.rs:414-417` (`ChatCreate` field), `:886` + `:999` (INSERT), `:1711-1750` (`set_other_extraction_watermark`); `db/memory_tiers_repair.rs:104-106`, `:148`, `:177-179`, `:192`, `:229-233` (the arm the ruling RETIRES) |
| `INTER_CHARACTER_MEMORY`, `MEMORY_HOUSEKEEPING`, `MEMORY_CONSOLIDATION` job types | `services/job_runner.rs:142-170` (`KNOWN_JOB_TYPES`), `:824`, `:963-971`, `:1192`; `api/system_data.rs:159-161` (estimate), `:182` (name), `:636-660` (`JOB_TYPES`, "25 values at `01a83539d`"); `realtime/job_topics.rs:97-125`; `services/activity_kinds.rs:83-88`; `services/frozen_archive.rs:90`; `api/memories.rs:1585-1592` (regenerate-all wipe set); `services/queue_service.rs:156-172`, `:186`, `:259-295`; `services/consolidation_triggers.rs:99`, `:267`; `quilltap-host/src/host.rs:2440`, `memory_consolidation.rs:114`, `spine.rs:2933-2960`, `:3919-3920`, `:3993` |
| `/system/tools` `memory-dedup[-preview]` | `quilltap-web/src/system_data_routes.rs:79`, `:95` (the action lists → `availableActions` bytes), `:373-385`, `:558-571`; `api/types.rs:1279-1296` (`MemoryDedupPreview` / `MemoryDedupRun`); `api/engine.rs:3194-3218`; `api/memory_maintenance.rs:40` |
| `memories` route `housekeep*` / `consolidate` / `housekeeping-config` / `extraction-concurrency` | `api/types.rs:1689-1729`, `:2876-2880`; `api/engine.rs:3834-3897`, `:5614`; `api/memories.rs:1190`, `:1299`; `api/memory_consolidation.rs` |
| reconcile-dimensions vector arms + result fields | `services/embedding_dimension_reconcile.rs:11-14`, `:134-137`, `:317-337`, `:376`, `:387`; `quilltap-host/src/host.rs:1229`; `services/backup/restore/orchestrator.rs:2143`, `:2163`; `restore/mod.rs:110` |
| reapply-profile's `vector_entries` table | `services/embedding_reapply_profile.rs:7`, `:54` |
| delete-all's vector wipe | `services/delete_all.rs:205-206` |
| `reinforced-importance.ts` formula `log2(count+1)` | `memory_gate.rs:42-47` |

---

## 3. Behaviour deltas the keystone must hit (numbered)

**Schema / D23**

- **Δ1 — the D23 re-dump (EIGHTH + NINTH in one).** Re-dump
  `crates/quilltap-core/src/services/provisioning/fresh_schema.json` +
  `chat_settings_seed.json` from the pin (§1.13); never by hand. Update the
  provisioning doc paragraph (`services/provisioning/mod.rs:193-205` style) and
  its tests (`:785-800` pin `otherExtractionWatermarkMessageId` after
  `timelineMode` — that assertion inverts).
- **Δ2 — `migration_indexes.json` re-dump** (§1.14) with the real-boot cross-check.
- **Δ3 — positional marshals re-indexed.** `db/chats_read.rs` (`ALL_COLUMNS`
  `:79-96` + `marshal_row` `:257-…`): insert `scenarioSource` (JSON object →
  `put_opt_json`, its stored text parsed) and `scenarioNotes` (string) at
  34/35, remove index 73 → every read from `totalPromptTokens` to
  `otherExtractionWatermarkMessageId` shifts +2 and everything after shifts
  +1 net; the `alignment_census` (`:550`) re-pins against the new dump.
  `db/memories_read.rs` `COLS` (`:79-84`) + `marshal_row` (`:240-280`): insert
  `supersededAt` at 25 (`put_nullable_str`), shift 25..28 → 26..29;
  `alignment_census` (`:1483`). `db/chat_settings.rs:1488-1510`: remove reads
  12/13, shift every later index −2.
- **Δ4 — `memories` write side.** `MemCreate` (`db/memories.rs:67-110`) +
  `superseded_at: Option<String>`; `INSERT` (`:362`) names it;
  `parse_create_memory` (`:172-260`) reads `supersededAt`; `MemUpdate`
  (`:282-318`) + the column; `update_tier_bulk` (`:829-896`) stamps
  `supersededAt = now` when `extra.superseded_by_id` is `Some(Some(_))`, NULL
  when `Some(None)`, untouched when `None` — ONE `now` shared with `updatedAt`
  (v4 `:1158-1167`); the `api/zod_issues.rs:1159-1162` MemorySchema twin gains
  `nullable_timestamp_issues(row, "supersededAt", …)` between `supersededById`
  and `consolidatedFrom`.
- **Δ5 — `chats` columns.** `ChatCreate` (`db/chats.rs:414-417`) loses the
  watermark, gains `scenario_source: Option<Value>` / `scenario_notes:
  Option<String>` (or the scenarios lane adds them — §7 C1-9); INSERT
  (`:886`, `:999`); delete `set_other_extraction_watermark` (`:1711-1750`) and
  `other_extraction_watermark_writers_census.rs`.
- **Δ6 — the partial index on pre-round instances needs NO ensure of its own.**
  P4.160's `ensure_migration_index_family` (`host.rs:2122-2149`) replays every
  `migration_indexes.json` statement by name with `IF NOT EXISTS` spliced; once
  Δ2 lands it creates `idx_memories_supersededById` on any instance with
  `memories.supersededById` (tiers ensure runs first). Its absent-table / UNIQUE
  arms do not apply (plain partial index). Verify the splice keeps the `WHERE`
  tail byte-exact (it splices only after `INDEX `).
- **Δ7 — `tfidf_vocabularies.textShapeVersion`.** Boot ensure appends
  `INTEGER DEFAULT 0` when missing; `db/tfidf_vocabulary.rs` `TvReadRow`
  (`:68`), `find_by_profile_id` SELECT (`:85`), `TvCreate` (`:109`), INSERT
  (`:169-171`), `TvUpdate` (`:138`), `upsert_by_profile_id` (`:265`) carry it;
  the read accepts INTEGER and REAL (§0 #2) and maps NULL/absent → 0.

**Boot ensures (the chain, in v4's run order, inside `seed_built_ins`
`quilltap-host/src/host.rs:1272`, after P4.D265's block `:2099-2121` and BEFORE
P4.160's backfill `:2122`)**

- **Δ8 — P4.D265's tiers ensure loses step 4** (`memory_tiers_repair.rs:104-106`,
  `:126`, `:140`, `:148`, `:157`, `:177-179`, `:192`, `:229-233`; outcome field
  `chat_column_added` + the host INFO field `chatColumnAdded` `host.rs:2116`);
  module doc rewritten (the ruling; P4.D251-supersedes-P4.D179 precedent);
  `test_support::ensure_memory_tiers_columns` (`test_support.rs:192-205`) doc
  follows. Its tests at `:387`, `:414`, `:502` (chat-column expectations) and
  `:233-247` (`only_chats_lacking_the_watermark…`) change.
- **Δ9 — the extraction-cursor ensure** (`add-episode-extraction-watermark-v1`
  + `add-memory-extraction-cursors-v1` as ONE pass, RULED): (1) table absent →
  `CREATE TABLE` in the **migration's** text (§9 O-2 on fresh instances); (2)
  `other-fold` cursors from `otherExtractionWatermarkMessageId` when the column
  exists (non-NULL, non-empty, `ON CONFLICT("id") DO NOTHING`, id
  `<chatId>:other-fold`, ONE `now`); (3) `episode-fold` cursors: from
  `episodeExtractionWatermarkMessageId` when that column exists, and — the
  episode migration's BACKFILL — from `lastSummaryAnchor(summaryAnchorMessageIds)`
  for chats without one, gated per §9 O-3; (4) `ALTER TABLE "chats" DROP COLUMN`
  each legacy column present. Atomic in v5 (the P4.D251 recorded-divergence
  precedent: v4 has one transaction per column copy and none around the
  DROPs). FATAL on failure. Effects-differential against v4's REAL runner over
  the episode + cursor pair (§5.2).
- **Δ10 — the vector-mirror retirement ensure** (RULED mirror): exactly §1.9 —
  paged by rowid, 500 per page, per-page transaction, BLOB copied as-is, TEXT
  through `embedding_blob::parse_legacy_embedding_text` →
  `embedding_blob::float32_to_blob` (quantized — `embedding_blob.rs:318`), the
  four counters, then DROP each table present. FATAL on failure (not
  resumable). MUST run before `reconcile_embedding_dimensions_at_boot`
  (`host.rs:621`, after `seed_built_ins` `:587` — already true) so the reconcile
  never deletes mirror rows the copy would have kept.
- **Δ11 — the settings move ensure**: §1.8 — copy `legacy_housekeeping_to_maintenance`
  of the single user's (else latest-`updatedAt`) cell with `ON CONFLICT DO
  NOTHING`, DELETE the dead key, DROP each dead column present (atomic, the
  P4.D251 precedent). The stored JSON is `{"enabled":…,"perCharacterCap":…,
  "perCharacterCapOverrides":{…}}` — NO `phases` (byte-pin it).
- **Δ12 — the two job-row DELETE ensures** + **`add-memory-superseded-at-v1`**
  (column `TEXT DEFAULT NULL` + the `UPDATE … SET supersededAt = updatedAt`
  backfill) + **`add-chat-scenario-source-v1`** (two `TEXT DEFAULT NULL`
  columns) + **`add-tfidf-text-shape-version-v1`** — unstamped, table-gated,
  idempotent, FATAL on failure, v4's lines NO-PORT, v5's own ONE INFO each when
  not a no-op.
- **Δ13 — the two DATA rewrites, ledger-gated + STAMPED** (RULED; the
  `generated_image_placeholder_heal.rs:149-153` / `wardrobe_wear_stats_repair.rs`
  `stamp` shape, `migrations_ledger::ensure_migrations_tables` first):
  `recompute-reinforced-importance-v1` (§1.5 — skip when EITHER app recorded
  the id; else run over every row and stamp `itemsAffected = rewritten`,
  message v4's sentence; a fresh / empty instance still stamps, as v4's real
  first boot does — v4's `shouldRun` is true whenever the columns exist);
  `rezone-legacy-memory-occurred-at-v1` (§1.6, same gate; zone = first valid
  `chat_settings.timezone` when that column exists → `QUILLTAP_TIMEZONE` →
  the host system zone; `Date.parse` semantics via `js_date_parse_ms`, never the
  strict ISO parser — memory note; the STRING compare; v4's two-pass
  `zonedTimeToUtcMs`). Both rewrite in 500-row batches, one transaction each.
- **Δ14 — the restore mirror-boot list** (`crates/quilltap-harness/tests/restore_vintage_state.rs:140-146`
  holds P4.D265's): add a `boot-align` block per new column / table ensure
  that a restore INSERT binds (`supersededAt`, `scenarioSource` /
  `scenarioNotes`, the cursor table, `textShapeVersion`) AND the column-DROP
  ensures (a restore of a pre-Phase-A archive binds `chat_settings` columns
  that no longer exist — the carriers lane's translation, but the vintage
  instance must be ALIGNED first). Memory note `vintage-restore-mirror-boot-list`.
- **Δ15 — `STRUCTURAL_TABLES` unchanged.** v4's verifiable set (11 dedicated-db
  repositories + `help_doc_chunks`, `db/table_shape.rs:77`) does not include
  the cursor repository (`AbstractBaseRepository`) nor the vector tables;
  `table_shape_equivalence` stays green. No `/health` change.

**Repositories**

- **Δ16 — NEW cursor repository** (§1.16 table; C1-1).
- **Δ17 — `memories` verbs** (§1.16): add `bulk_delete_unsafe`,
  `add_related_memory_ids` / `remove_related_memory_ids` (savepoint
  read-modify-write; ids deduped; self + present skipped; unchanged list → no
  write; DEBUG `Memory links updated` {memoryId, characterId, op, requested,
  changed, size}; WARN `Memory not found for link update` {memoryId,
  characterId, op}), `remove_related_memory_ids_everywhere`,
  `append_reinforcement_footnotes` (live-content `appendCappedFootnotes` — the
  pure fn lives at `lib/memory/reinforcement-footnotes.ts:20-41`
  `MAX_REINFORCEMENT_FOOTNOTES` / `countReinforcementFootnotes` /
  `appendCappedFootnotes`; v5 has a twin near `memory_gate.rs:49`), `count_cap_breakdown`,
  `count_corpus_since`, `find_index_rows_by_character_id`,
  `find_digests_containing_members`, `find_dangling_links` (RETHROW),
  `find_ids_superseded_by_any`, `remove_consolidated_members`,
  `find_digest_subjects_by_ids`; GROW `increment_reinforcement` (`at`, `by`, the
  positive-integer throw, forward-only `lastReinforcedAt`, the new DEBUG
  fields, `Reinforcement.last_reinforced_at`); the formula moves to
  `log2(max(1, count))` (C1-4). `delete` REFUSES (ERROR line + `Err`) only once
  every caller is on `retire_memories` (§9 O-4 — ordering inside the round).
  Remove `find_cold_ids_by_character_id`, `find_expired_cold_ids`,
  `count_hot_by_character_id`, `bulk_delete`, `delete_by_chat_id`,
  `delete_by_source_message_id`, `delete_for_character`, `replace_in_memories`,
  `delete_with_unlink`, `delete_many_with_unlink`, `CountSource` when their last
  caller moves (sibling lanes).
- **Δ18 — the vector store re-pointed** (`db/vector_store.rs`): `load` reads
  `find_index_rows_by_character_id` (never fails: v4's fallback `[]` + ERROR,
  and v4's outer catch → ERROR `Error loading vector index from memory rows` +
  empty), DEBUG `Loaded vector index from memory rows` {context, characterId,
  entries, cold (= rows tagged cold, NOT the cold-id set), dimensions};
  dimensions = dominant length; search's mismatch arm per §1.17; `flush` and
  the `VectorIndicesRepository` import DELETED. Every `store.flush(&main.vector_indices())`
  caller (§4.3) drops its flush (the row already carries the embedding) or
  routes the write through the index-store's `upsert` (C1-3).
- **Δ19 — `chat_settings`**: drop the two columns from the struct, INSERT,
  tolerant SELECT list, positional read, and the `ChatSettingsCreate`/update
  paths; delete `find_auto_housekeeping_settings_by_user_id` and the
  `auto_housekeeping_settings` half of `SchedulerUserSettings` (callers move to
  `get_memory_maintenance_settings`: `api/memories.rs:1195`, `:1309`,
  `services/memory_gate.rs:1477`, `quilltap-host/src/spine.rs:2947`,
  `services/queue_service.rs:1446-1520`).
  `chat_settings_column_sites_guard.rs` drops the two rows.
- **Δ20 — `instance_settings`**: delete `get/set_memory_extraction_concurrency`
  (`:527-558`); NEW `memoryMaintenance` get/set/parse (C1-5); grow
  `MemoryConsolidationSettings` (`:690`) by `tfidf_cluster_threshold` (0.30,
  after `cluster_threshold`, both in `to_json` key order and the parse);
  `parse_memory_extraction_limits` (`:584`) adds `maxPerHour ≤ 10000`
  (`too_big` issue). (The last two may be assigned to the TF-IDF / settings
  lanes — they are one file.)
- **Δ21 — job types**: `KNOWN_JOB_TYPES` / `JOB_TYPES` to v4's 23 in v4's
  order (`MEMORY_MAINTENANCE` in the housekeeping slot); `job_topics.rs`
  `MEMORY_MAINTENANCE` → `[memories, mountPoints]` collection hints (no dryRun
  arm), the INTER arm removed; `activity_kinds.rs` Mem block;
  `frozen_archive.rs:90` → `["MEMORY_MAINTENANCE"]`; `api/system_data.rs`
  estimate + name rows removed; `api/memories.rs:1585-1592` wipe set → three
  types. The handler REGISTRATION for `MEMORY_MAINTENANCE` is the maintenance
  lane's (until then the "recognized but not yet available" fallback answers
  — the P4.D265 `MEMORY_CONSOLIDATION` precedent, `job_runner.rs:159-161`).
- **Δ22 — `MEMORY_EXTRACTION` payload decode** (`services/memory_extraction_job.rs:52-112`):
  `pass` (`memoryExtractionPassOf` incl. the deprecated flags, JS-truthy),
  `cursor`, `ignore_idle`, `backfill`, `extract_after_message_id`; the struct
  is C1 so the extraction lane and the enqueuers (`queue_service.rs:446-540`
  dedupe key → `(chatId, opener, anchor, pass, cursor)`) share it.

**Wire**

- **Δ23 — new `Request` variants with refusal arms** (C1-8): `MemoryMaintain {
  options: Value }`, `MemoryMaintenanceStatus { character_id: Option<String> }`,
  `MemoryRepair { options: Value }`, `MemoryMaintenanceConfigGet`,
  `MemoryMaintenanceConfigSet { config: Value }`. Retired:
  `MemoryHousekeepPreview`, `MemoryHousekeep`, `MemoryHousekeepSweep`,
  `MemoryHousekeepingConfigGet/Set`, `MemoryExtractionConcurrencyGet/Set`,
  `MemoryConsolidate`, `MemoryDedupPreview`, `MemoryDedupRun` (§9 O-6 on
  delete-vs-refuse). `quilltap-web/src/system_data_routes.rs:70-97` action
  lists drop `memory-dedup-preview` / `memory-dedup` (the `availableActions`
  bytes of every tools 400 change).

---

## 4. v5 on `main` today (`a7c6f0d03`) — per file in this slice

### 4.1 Schema artifacts + provisioning

- `crates/quilltap-core/src/services/provisioning/fresh_schema.json` (126
  lines; main 85 / 31 / 3 — line 10 `chat_settings`, 11 `chats`, 24 `memories`,
  33 `tfidf_vocabularies`, 35-36 the vector tables, 85-86 their indexes) —
  re-dump (Δ1).
- `…/migration_indexes.json` (77 lines; `source.v4Commit 01a83539d…`; main 53
  — lines 42-47 the `memories` family, 58-59 the vector `characterId` pair) —
  re-dump (Δ2).
- `…/chat_settings_seed.json` (75 lines; `:13-14`, `:50-51` the two dead keys).
- `…/provisioning/mod.rs` (908 lines): `:193-205` the D23 history paragraph
  (append re-dump #8/#9), `:211` / `:238` / `:289` the `include_str!`s,
  `:325-340` `migration_main_index(name)`, `:422`
  `provision_fresh_instance`, tests `:785-800` (the watermark position
  assertion inverts; add `supersededAt`, the scenario pair, the cursor table).

### 4.2 Boot ensures + host

- `crates/quilltap-core/src/db/memory_tiers_repair.rs` (569) — step 4 retires
  (Δ8); lines listed in §2.
- `crates/quilltap-core/src/db/migrations_ledger.rs` —
  `ensure_migrations_tables` (the stamp precondition, reused by Δ13).
- Precedent modules to copy: `chat_informs_permanent_repair.rs` (two shapes,
  P4.D249), `chat_settings_impersonation_voice_mode_repair.rs` (supersede +
  DROP COLUMN + ONE transaction, P4.D251), `wardrobe_wear_stats_repair.rs`
  (stamping ensure, `stamp` `:316-337`, `already_completed` `:339-347`),
  `generated_image_placeholder_heal.rs` (ledger-gated heal `:142-325`),
  `migration_index_family_repair.rs` (Δ6).
- `crates/quilltap-host/src/host.rs`: `start` (`:245`) → PHASE 0.75 optimize
  (`:555-563`) → PHASE 2 backups (`:576`) → `seed_built_ins` (`:587`) →
  structural verify (`:604`) → dimension reconcile (`:621`). Inside
  `seed_built_ins` (`:1272-2160`): P4.D255 wardrobe (`:2063-2098`) → P4.D265
  tiers (`:2099-2121`) → **new ensures here, in §1.1's order** → P4.160 index
  backfill (`:2122-2149`). `reconcile_embedding_dimensions_at_boot`
  (`:1196-1240`; `:1229` logs `vector_entries_deleted`). NOTE: v5 takes the
  startup backup BEFORE the ensures (v4 PHASE 2 runs after PHASE 1 migrations)
  — a pre-existing order difference that here works in v5's favour (the
  pre-DROP state is backed up).
- `crates/quilltap-core/src/test_support.rs:192-205`
  (`ensure_memory_tiers_columns`) — after Δ8 it no longer adds the chat column;
  add a sibling for the cursor table if fixtures need it.

### 4.3 Vector mirror — every v5 caller (production code)

- `db/vector_indices.rs` (347, whole module: `VectorIndexMeta` `:74`,
  `VectorEntryInput` `:86`, `VectorIndicesRepository` `:94-340`,
  `VectorEntryRow` `:342`) and `db/mod.rs:150`, `:566-568`.
- `db/vector_store.rs` (601): `:35` import; `load` `:124-176`; `flush`
  `:392-…`.
- `services/memory_gate.rs:443` (load), `:823-825` + `:1036-1042` (load +
  `flush(&main.vector_indices())`); tests `:1589-1860`, `:2141-2340` plant the
  tables.
- `services/memory_service.rs:137-139`, `:260-267` (flush in the delete
  paths), `:715`, `:1747`, `:1846` (load), `:1838` + `:1884-1890`
  (`rebuild_vector_index` — `VectorIndicesRepository::delete…` + re-add); tests
  `:1898-2149`.
- `services/housekeeping.rs:547-551` (load + flush); tests `:631-1071`.
- `services/memory_dedup.rs:459-463` (load + flush); tests `:696-699`.
- `services/consolidation.rs:91`, `:921`, `:933`, `:1019` (load, stage, flush).
- `services/anchor_gate_probe.rs:63`, `:299`, `:407` (load); test `:606`.
- `generators/optimizer.rs:41`, `:1641` (load).
- `services/embedding_generate_job.rs:375-392` (`VectorIndicesRepository::add_entry`).
- `services/embedding_reindex_job.rs:86` (doc), `:298` (repo).
- `services/cascade_delete.rs:23`, `:376` (`delete_by_character_id` — the
  half-completed-delete hazard).
- `services/character_archive/service.rs:74`, `:1275`.
- `services/embedding_dimension_reconcile.rs:11-14`, `:134-137`, `:317-337`,
  `:376`, `:387` (table-guarded; tests `:607-1025`).
- `services/embedding_reapply_profile.rs:7`, `:54`.
- `services/delete_all.rs:205-206`.
- backup: `services/backup/collect.rs:119`, `:521`, `:649-672`, `:734`, `:843`,
  `:887`, `:971-990`; `staging.rs:118`; `manifest.rs:81`; `uuid_remap.rs:359`,
  `:496`; restore `archive.rs:186`, `:233`; `restore/mod.rs:75`, `:110`;
  `restore/preview.rs:59`; `restore/orchestrator.rs:1766-1800` (step 22j),
  `:2143`, `:2163`, `:4348`, `:4440`, `:4603`.
- `api/memories.rs:2412-2426` (a test dropping `vector_entries`).
- `realtime/publish_sites.rs:810-813` (test DDL); `db/memories.rs:828` (doc);
  `db/help_docs.rs:72` (doc); `lib.rs:115` (doc); `host.rs:1229`.
- quilltap-web tests: `characters_action_route.rs` (2 refs).

### 4.4 Deletion chokepoints (become `retire_memories` callers — sibling lanes; listed so the keystone's C1 covers them)

`api/memories.rs:644-690` (item DELETE → `delete_with_unlink`), `:706`
(chat wipe → `delete_memories_by_chat_id_with_vectors`); `services/chat_admin.rs:68`,
`:493`, `:607`; `services/message_reattribute.rs:21`, `:99`;
`services/regenerate_swipe.rs:72`, `:1098`; `api/salon.rs:1186`;
`services/cascade_delete.rs:372`; `services/memory_dedup.rs:442`;
`services/delete_all.rs:356`; `services/housekeeping.rs:538`;
`services/character_archive/service.rs:1249`; `realtime/publish_sites.rs:884-958`
(tests).

### 4.5 Settings, chats, jobs, wire

- `db/chat_settings.rs` — Δ19 lines; `db/instance_settings.rs` — Δ20 lines
  (`read_json_setting` / `merge_patch` / `write_setting` are the helpers to
  reuse; `get_memory_consolidation_settings` `:1130` /
  `set_memory_consolidation_settings` `:1145` the shape to copy).
- `db/chats.rs` (2208) / `db/chats_read.rs` (932) — Δ3, Δ5.
- `db/memories.rs` (2151) / `db/memories_read.rs` (1604) — Δ3, Δ4, Δ17.
- `db/tfidf_vocabulary.rs` (342) — Δ7.
- `services/job_runner.rs`, `api/system_data.rs`, `realtime/job_topics.rs`,
  `services/activity_kinds.rs`, `services/frozen_archive.rs`,
  `services/memory_extraction_job.rs` — Δ21, Δ22.
- `api/types.rs` (6102): memories variants `:1613-1776`, P4.D265's fence
  `:2850-2880`, `Response::Memory` `:4562`, `Response::MemoryMaintenance`
  `:4551`; `api/engine.rs` (8560) arms `:3194-3218`, `:3834-3897`, `:5598-5620`;
  `api/memories.rs` `not_available` (`:71`); P4.D265's single refusal helper in
  `engine.rs` (R-G) — reuse it.
- `quilltap-web/tests/dispatch_wrong_type_census.rs` — rows for every new typed
  field (`character_id` on `MemoryMaintenanceStatus`).

---

## 5. Harness

### 5.1 Families this keystone owns (tier, fate at the pin)

| family (Rust test) | oracle case / builder | tier | at the pin |
|---|---|---|---|
| `provisioning_equivalence` | `provision/build-provision-oracle.ts`, `dump-fresh-schema.ts`, `migrations-first.ts` | 2 | RED by design until Δ1/Δ2; `TABLE_COLUMN_ASYMMETRY` row `chat_settings/v4/memoryExtractionConcurrency` (`:819-830`, the column at `:826`) trips (the column is dropped on a real first boot — narrow the row to the two remaining columns); the cursor table's two shapes are a NEW asymmetry class (same column set, different constraints — §9 O-2) |
| `migration_index_backfill_equivalence` | `cases/migration-index-backfill.ts` | 2 | RED until Δ2 (one name gained, two lost) |
| `memory_tiers_ensure_equivalence` | `cases/memory-tiers-ensure.ts` | 2 | RED: v4's narrowed runner still adds the chat column; re-point to the CHAIN (tiers → episode → cursors) per §0 #5; the "only `chats` behind" mode inverts |
| `chat_informs_permanent_ensure_equivalence` / `chat_settings_voice_mode_ensure_equivalence` / `wardrobe_wear_stats_ensure_equivalence` / `chat_settings_wardrobe_image_settings_ensure_equivalence` | their `*-ensure.ts` cases | 2 | the TEMPLATES for the new ensure families (unchanged; the chat-settings ones may redden only on seed-row column sets) |
| `vector_indices_tier2_equivalence` | `cases/vector-indices.ts` + `fixtures/build-vector-indices-fixture.ts` | 2 | **IMPORT-DEAD** → retire whole with `db/vector_indices.rs` |
| `vector_store_tiers_equivalence` | `cases/vector-store-tiers.ts` | 2 | RUNTIME-DEAD: `builder.save()` (`:113`) no longer exists; rewrite over rows (plant embeddings on `memories`, then `load` + search) |
| `memories_tier2_equivalence` | `cases/memories-tier2.ts` | 2 | IMPORT-DEAD (`markMemoriesAccessed` from `memory-service`, `:173`) + RED (new verbs, `supersededAt`) |
| `memory_delete_tier2_equivalence` | `cases/memory-delete-tier2.ts` | 2 | IMPORT-DEAD (`deleteMemoryWithUnlink`, `deleteMemoriesWithUnlinkBatch` from `memory-gate`) → re-point at `retireMemories` |
| `memory_cascade_tier2_equivalence` | `cases/memory-cascade-tier2.ts` + builder | 2 | IMPORT-DEAD (four `memory-service` deleters) |
| `memory_name_helpers_equivalence` | `cases/memory-name-helpers.ts` | 1 | IMPORT-DEAD (`calculateReinforcedImportance` from `memory-gate`) → `@/lib/memory/scoring/reinforced-importance`; rows move (count 1 = importance) |
| `memory_weighting_equivalence` / `ranking_blend_equivalence` | `cases/memory-weighting.ts`, `ranking-blend.ts` | 1 | IMPORT-DEAD (Phase-1 oracle; the scoring lane re-points to `lib/memory/scoring/*` — `calculateProtectionScore` `scoring/protection.ts:112`, `DEFAULT_PROTECTION_CONFIG` `:76`, `computeRankingBlend` / `defaultMinCosineForProvider` `scoring/relevance.ts:24,38`, `formatRelativeAge` `lib/format-time.ts:163`; `calculateEffectiveWeight` / `DEFAULT_WEIGHTING_CONFIG` have NO replacement) |
| `chats_tier2_equivalence` | `cases/chats-tier2.ts` | 2 | RED (row key set: scenario pair in, watermark out) |
| `chat_settings_tier2_equivalence` + `chat_settings_column_sites_guard` | `cases/chat-settings-tier2.ts` | 2 / guard | RED (two keys out) |
| `other_extraction_watermark_writers_census` | — | guard | retire (column gone) |
| `memory_settings_schemas_equivalence` | `cases/memory-settings-schemas.ts` | 1 | RED + GROW: `tfidfClusterThreshold`, the `memoryMaintenance` schema rows (cap bounds 100/100000, `.int()`, override records, `phases` prefault), `maxPerHour ≤ 10000` |
| `instance_settings_json_warns_equivalence` | `cases/instance-settings-json-warns.test.ts` | 2 | GROW: the `memoryMaintenance` parse-failure WARN |
| `tfidf_vocabulary_tier2_equivalence` / `tfidf_vocabulary_upsert_tier2_equivalence` | `cases/tfidf-vocabulary*-tier2.ts` | 2 | RED (column; REAL vs INTEGER) |
| `background_jobs_tier2_equivalence` / `system_jobs_collection_equivalence` / `system_jobs_routes_equivalence` | `cases/background-jobs.ts`, `system-jobs-*.test.ts` | 2 | RED (job enum, `findActiveByType`) |
| `activity_tables_equivalence` / `realtime_topics_equivalence` | `cases/activity-tables.ts`, `realtime-topics.ts` | 1 | RED (Mem block; the maintenance topic arm) |
| `table_shape_equivalence` | `cases/table-shape.ts` | 1 | GREEN (no structural change) — verify the census still prints 11 |
| `restore_vintage_state` | (vintage fixture) | 2 | RED until Δ14 |
| `system_body_guards_equivalence` | `cases/system-body-guards.test.ts` | 2 | IMPORT-DEAD (`@/lib/tools/memory-dedup` mocked) |

### 5.2 New / grown corpus rows (this keystone)

- **One ensure family per migration class**, modelled on
  `memory_tiers_ensure_equivalence.rs` (header recipe `:18-28`) — v4's REAL
  `MigrationRunner` narrowed to the migration(s) (`fixtures/apply-memory-tiers-migration.ts`
  mechanics; `lib/v4-migrations.ts` `runV4Migrations`) vs the v5 ensure, over
  the SAME base files, comparing EFFECTS (`PRAGMA table_info` incl. dflt text,
  `sqlite_master.sql`, the touched rows, `migrations_state` rows where the
  ensure stamps), modes: pre-round (the committed vintage), fresh-generateDDL,
  already-migrated + ledger, half-migrated (both columns present), tables
  absent. Required rows:
  - cursors: other column only (a v5 pre-round instance); both columns; empty
    / `''` / NULL values; a chat with anchors `'[]'`, `'[ ]'`, malformed JSON,
    a non-string last element; a pre-existing cursor row (DO NOTHING); the
    FK-live delete-a-chat after (migration shape cascades);
  - vector retire: BLOB already on row; row has NULL embedding + BLOB mirror;
    an EMPTY row blob (`has` true); a JSON-array TEXT mirror; a
    `{"0":…}` object TEXT mirror; unparseable TEXT; an orphan mirror row; a
    zero-length mirror BLOB; `vector_indices` only; neither; > 500 rows (two
    pages);
  - settings move: single-user row; no single-user row (latest `updatedAt`
    wins); NULL cell; malformed JSON; string caps; caps out of bounds both ways;
    a pre-existing `memoryMaintenance` key; `memoryExtractionConcurrency`
    column present and absent; the dead key present with no dead columns;
  - recompute: count NULL / 0 / 1 / 2 / 1e6, importance NULL, stored NULL,
    a value within 1e-9 (not rewritten), the ledger row present (skip) /
    absent (run + stamp), a 0-row instance (stamps);
  - rezone: zone sources (valid / invalid `chat_settings.timezone`, column
    absent, `QUILLTAP_TIMEZONE`), `…T00:00:00Z` vs `…T00:00:00.000Z` in UTC,
    a non-midnight `T00:00:00` string with an offset suffix, a DST-gap midnight
    zone, a negative (pre-1970) date, the ledger gate; **pin `TZ` for the run
    (the second-zone oracle needs tsx, not jest — memory note)**;
  - superseded-at: column missing; present with unstamped superseded rows;
    nothing to do;
  - scenario-source / tfidf / two job-DELETE ensures: one row each plus the
    no-op.
- `reinforced_importance` tier-1 (count ∈ {−1, 0, 0.5, 1, 2, 3, NaN, ∞},
  importance at the cap).
- the cursor repository tier-2 (`cases/memory-extraction-cursors-tier2.ts`,
  NEW): every verb, both table shapes, the RETHROW vs FALLBACK lines.
- the memories repository tier-2 grows (each new verb; `update_tier_bulk`'s
  `supersededAt` three-state; `increment_reinforcement` `by`/`at`).
- the vector store over rows (`vector_store_tiers` rewrite): dominant
  dimensions with a mixed corpus, the mismatch WARN only when NO entry matches.

Closest recipes to copy: `memory_tiers_ensure_equivalence.rs` (+
`cases/memory-tiers-ensure.ts`), `chat_settings_voice_mode_ensure_equivalence.rs`
(DROP COLUMN), `wardrobe_wear_stats_ensure_equivalence.rs` (stamping),
`migration_index_backfill_equivalence.rs`, `provisioning_equivalence.rs`
(header recipe `:31-67`).

### 5.3 The committed fixture pairs — the in-place NARROW + WIDEN

107 `.db` files under `crates/quilltap-web/tests/fixtures/` (the `*-main.db` set
P4.D265 widened — its list, `p4.d265-…md:43`, is the starting census; 20
`.meta.json` sidecars) + `migration-vintage/{quilltap,quilltap-mount-index,quilltap-llm-logs}.db`.
P4.D265 widened 43 pairs through `harness/oracle/fixtures/migrate-memories-fixture-columns.ts
--module migrations/scripts/add-memory-tiers-v1.ts#addMemoryTiersMigration` from
the pin (header `:1-40`; `lib/v4-migrations.ts` mechanics: keyed open,
journal-mode restore, `-wal`/`-shm` cleanup, a pair rewritten with no logical
change `git restore`d). This round's chain, in v4's order, through the REAL
modules from the `64743ee13` worktree: `add-memory-superseded-at-v1`,
`add-chat-scenario-source-v1`, `add-episode-extraction-watermark-v1`,
`add-memories-superseded-by-index-v1`, `move-memory-maintenance-settings-v1`,
`retire-memory-vector-entries-v1`, `add-memory-extraction-cursors-v1`,
`drop-inter-character-memory-jobs-v1`, `add-tfidf-text-shape-version-v1`,
`drop-housekeeping-consolidation-jobs-v1` — **and decide** whether the two
data rewrites run on the pairs (§9 O-5: they rewrite memory rows other
families' oracle bodies include, and the rezone's output depends on the zone
of the machine running the widen). The pairs have no `migrations_state`, so a
host-booting test (quilltap-web) would run the ledger-gated rewrites at boot
unless the widen stamps them.

**`fixtures/apply-memory-tiers-migration.ts` (C1 §13 of P4.D265) becomes a
hazard at the pin**: on a pin-built generateDDL fixture the tier columns
already exist, but v4's tiers migration still ADDS `chats.otherExtractionWatermarkMessageId`
(§0 #5) — every builder / plant calling it re-introduces the dropped column.
Ten callers: `cases/anchor-gate-probe-tier3.test.ts`, `system-import.test.ts`,
`system-import-execute.test.ts`, `memory-dedup.test.ts`,
`memory-tiers-ensure.ts`, `system-export.test.ts`, `memories-routes.test.ts`,
`recall-replay.test.ts`, `frozen-archive-tier2.test.ts` (+ the helper). Chain
the cursor migration after it, or retire it where the pin's generateDDL
already carries the columns.

### 5.4 The IMPORT-DEAD census (critical)

**Method (read-only):** `git ls-tree -r --name-only` at `4a38222e6` and
`64743ee13`; a Python scan of every `.ts/.tsx/.js/.mjs` under
`harness/oracle/` (856 files; 820 with v4 references; 5,056 references) for
static `import … from '@/…'`, side-effect imports, `const {…} = await
import('@/…')` / `require`, `jest.mock` / `doMock` / `requireActual` specifiers,
and `'lib/…'` / `'migrations/…'` path literals passed to `join(v4Root, …)`;
each specifier resolved (`''`, `.ts`, `.tsx`, `.js`, `.json`, `/index.ts`) at
both trees; for modules that survive but CHANGED in the span, each destructured
/ named import and each `mod.member` use of a bound module checked against the
pin's exports (`export` decls, `export {…}`, `export * from` followed).
**Result: 28 deleted modules, 79 references, 60 files; 3 removed exports from
surviving modules; no dead `migrations/` path literal.**

**(a) Deleted modules** (file → dead import → v4 replacement at the pin):

| harness file | dead import (names) | replacement at `64743ee13` |
|---|---|---|
| `cases/consolidation-triggers-tier2.test.ts` | `@/lib/background-jobs/handlers/memory-consolidation` (`handleMemoryConsolidation`); `@/lib/background-jobs/scheduled-housekeeping` (`runScheduledHousekeeping`); `@/lib/memory/consolidation-triggers` (whole) | `handlers/memory-maintenance.ts` `handleMemoryMaintenance`; `lib/memory/maintenance/scheduler.ts` `runScheduledMemoryMaintenance` / `scheduleMemoryMaintenance`; `maintenance/triggers.ts` `maybeEnqueueMaintenance[AfterCommit]` + `maintenance/ledger.ts` `shouldEnqueue` |
| `cases/fold-other-pass-tier3.test.ts` | `@/lib/background-jobs/maintenance/fold-other-catchup` (`enqueueFoldOtherCatchups`); `@/lib/memory/fold-other-pass` (`findFoldOtherCatchupCandidates`, `runFoldOtherPass`) | `lib/memory/extraction/catchup.ts` (`findFoldCatchupCandidates`, `runFoldCatchup`); `extraction/index.ts` `otherFoldPass`, `runFoldPass` |
| `cases/fold-episode-tier3.test.ts` | `@/lib/memory/fold-episode-pass` (`runFoldEpisodePass`) | `extraction/index.ts` `episodeFoldPass`, `runFoldPass` |
| `cases/memory-injector.ts` | `@/lib/chat/context/memory-injector` (`formatCurrentSceneState`, `SceneStateEmissionEntry`, `formatSummaryForContext`, `formatDigestProvenance`, `formatMemorySubjectPrefix`, `MemorySubjectContext`, `formatDynamicMemoryHead`, `formatFrozenMemoryArchive`, `formatInterCharacterMemoriesForContext`, `formatMemoriesForContext`, `formatMemoryMetadataTag`); `@/lib/memory/memory-service` (`SemanticSearchResult`) | scene state: `lib/chat/context/scene-state.ts:20,63,141`; `lib/memory/recall/render.ts` (`formatMemorySubjectPrefix` `:85`, `formatDigestProvenance` `:96`, `MemorySubjectContext` `:73`); the four `format*ForContext` / head / archive / metadata-tag renderers have NO same-name export (Phase E's `recall/render` + `recall/deliver` — the recall lane maps them); `SemanticSearchResult` → `recall/search.ts` `RecallSearchResult` |
| `cases/memory-pools.ts` | `@/lib/chat/context/memory-injector` (whole) | `lib/memory/recall/*` (recall lane) |
| 19 files (§5.4 c) | `@/lib/database/repositories/vector-indices.repository` (`VectorIndicesRepository`) | NONE — `memories.findIndexRowsByCharacterId`; plant vectors on `memories.embedding` |
| `fixtures/build-{consolidation,context-summary-service,fold-episode,fold-other-pass}-fixture.ts` | `@/lib/schemas/vector-indices.types` (`VectorEntryRowSchema`, `VectorIndexMetaSchema`) | NONE |
| `cases/memory-search-extraction.test.ts` | `@/lib/memory/cheap-llm-tasks/memory-tasks` (`extractMemorySearchKeywords`) | `lib/memory/cheap-llm-tasks/index.ts:37` re-export / `lib/memory/recall/query.ts:191` |
| `cases/memory-tasks-tier1.test.ts` | same module (`extractSelfMemoriesFromTurn`, `extractOtherMemoriesFromTurn`, `extractOtherMemoriesFromFold`) | `lib/memory/extraction/` passes (`selfTurnPass`, `otherTurnPass`, `otherFoldPass`) + `prompts/*` (`buildSelfTurnMessages`, `buildOtherTurnMessages`, `buildOtherFoldMessages`) |
| `cases/context-feeders-leaves.ts` | `@/lib/memory/conversation-summary-search` (`renderRelevantConversationsBlock`) | `lib/memory/recall/conversations.ts:248` (renamed, R096) |
| `cases/vault-conv-search.ts` | same (`searchVaultConversationSummaries`) | `recall/conversations.ts:89` |
| `cases/day-references.ts` | `@/lib/memory/day-references` (`resolveDayReference`) | `lib/memory/time/clock.ts:245` |
| `cases/episodic.ts` | (module survives) `@/lib/memory/episodic` — REMOVED exports `buildMemoryEmbeddingText`, `resolveWhenPhrase` | `lib/memory/index-store/embedding-text.ts` `embeddingTextFor` / `lexicalEmbeddingTextFor`; `lib/memory/time/clock.ts:210` `resolveWhenPhrase` |
| `cases/build-context-tier3.test.ts` | `@/lib/memory/frozen-archive-cache` (`resetFrozenArchiveCacheForTests`) | `lib/memory/recall/archive.ts:162` (R090) |
| `cases/frozen-archive-tier2.test.ts`, `cases/regenerate-swipe-tier3.test.ts`, `cases/salon-swipe-generate.test.ts` | `@/lib/memory/frozen-archive-cache` (mocked / whole) | `recall/archive.ts` |
| `cases/memory-housekeeping-tier2.ts` | `@/lib/memory/housekeeping` (`needsHousekeeping`, `runHousekeeping`) | `lib/memory/maintenance/run.ts` `runMaintenance` (+ `demote.ts`, `ledger.ts`) — NO `needsHousekeeping` twin |
| `cases/memory-watermark-tier3.test.ts` | `@/lib/memory/housekeeping-outcome-cache` (`recordHousekeepingOutcome`); `@/lib/memory/memory-service` (`createMemoryWithGate`) | `maintenance/ledger.ts` `recordMaintenanceOutcome`; `lib/memory/admission/admit.ts` `admitCandidate` / `admitRow` |
| `cases/archive-reencrypt-tier2.test.ts`, `cases/character-archive-tier2.test.ts` | `@/lib/memory/memory-gate` (mocked / whole) | `lib/memory/retire.ts` (`retireMemories`, reason `character-archive` / `character-delete`) |
| `cases/extract-novel-details.ts` | `@/lib/memory/memory-gate` (`extractNovelDetails`) | `lib/memory/admission/novel-details.ts:41` (re-exported `admission/index.ts:29`), byte-identical |
| `cases/memory-delete-tier2.ts` | `@/lib/memory/memory-gate` (`deleteMemoryWithUnlink`, `deleteMemoriesWithUnlinkBatch`) | `retire.ts` `retireMemories` |
| `cases/memory-merge-plan.ts` | `@/lib/memory/memory-gate` (`MAX_REINFORCEMENT_FOOTNOTES`, `appendCappedFootnotes`, `countReinforcementFootnotes`); `@/lib/memory/memory-merge` (`planMemoryMerge`) | `lib/memory/reinforcement-footnotes.ts:20,25,41`; `admission/fold.ts` `foldInto` (+ `apply.ts` `applyFold`) |
| `cases/memory-name-helpers.ts` | `@/lib/memory/memory-gate` (`calculateReinforcedImportance`) | `lib/memory/scoring/reinforced-importance.ts:29` (also `lib/memory/index.ts:189`) |
| `cases/memory-gate-tier3.test.ts` | `@/lib/memory/memory-gate` (whole); `@/lib/memory/memory-service` (`createMemoryWithGate`) | `lib/memory/admission/*` (`decideAdmission`, `admitCandidate`) |
| `cases/memory-processor-tier3.test.ts` | `@/lib/memory/memory-processor` (`processTurnForMemory`) | `lib/memory/extraction/turn.ts` `runTurnExtraction` / `runTurnPasses` |
| `cases/orchestrator-tier3.test.ts`, `cases/regenerate-swipe-tier3.test.ts`, `cases/salon-swipe-generate.test.ts` | `@/lib/memory/memory-recap` (mocked) | `lib/memory/recall/recap.ts` (R074; `generateMemoryRecap` via `recall/index.ts`) |
| `cases/recent-conversations-block.ts` | `@/lib/memory/memory-recap` (`buildRecentConversationsBlock`) | `recall/recap.ts:93` |
| `cases/memories-tier2.ts` | `@/lib/memory/memory-service` (`markMemoriesAccessed`) | `lib/memory/recall/deliver.ts` `stampAccessed` (delivery-only stamping) |
| `cases/memory-cascade-tier2.ts` | `@/lib/memory/memory-service` (`deleteMemoriesByChatIdWithVectors`, `deleteMemoriesBySourceMessageWithVectors`, `deleteMemoriesBySourceMessagesWithVectors`, `deleteMemoryWithVector`) | `retire.ts` `retireChatMemories` / `retireMemories` (reasons `chat-wipe`, `message-cascade`, `item-delete`) |
| `cases/semantic-search-tuning-tier3.test.ts` | `@/lib/memory/memory-service` (`reserveBackgroundSlots`, `searchMemoriesSemantic`); `@/lib/memory/recall-tuning` (`RecallTuningInputSchema`, `resolveRecallTuning`) | `lib/memory/scoring/rank.ts:82` `reserveBackgroundSlots`; `recall/search.ts` `searchRecall` / `searchMemories`; `recall/tuning.ts:131` |
| `fixtures/build-episodic-recall-fixture.ts`, `fixtures/build-memories-web-fixture.ts` | `@/lib/memory/memory-service` (`generateMissingEmbeddings`) | `lib/memory/index-store/backfill.ts:29` (re-exported `index-store/index.ts:19`) |
| `cases/memory-weighting.ts` | `@/lib/memory/memory-weighting` (`DEFAULT_PROTECTION_CONFIG`, `DEFAULT_WEIGHTING_CONFIG`, `calculateEffectiveWeight`, `calculateProtectionScore`, `formatRelativeAge`) | `scoring/protection.ts:76,112`; `lib/format-time.ts:163`; `DEFAULT_WEIGHTING_CONFIG` / `calculateEffectiveWeight` NONE (deleted, Phase A) |
| `cases/ranking-blend.ts` | same module (`computeRankingBlend`, `defaultMinCosineForProvider`, `formatRelativeAge`) | `scoring/relevance.ts:24,38`; `format-time.ts:163` |
| `cases/recall-history.ts` | `@/lib/memory/recall-history` (6 names) | `lib/memory/recall/history.ts` (R098) |
| `cases/recall-tags.ts` | `@/lib/memory/recall-tags` (18 names); `@/lib/memory/recall-tuning` (`RecallTuningInputSchema`, `resolveRecallTuning`) | `recall/tags.ts` (R095; `parseTargetingTags` `:341`); `recall/tuning.ts` |
| `cases/recall-tuning.ts` | `@/lib/memory/recall-tuning` (7 names) | `recall/tuning.ts` (R095; `selectSpecificAnchors` `:164`) |
| `cases/scenario-resolvers.ts` | `@/lib/mount-index/general-scenarios` (`resolveGeneralScenarioBody`), `@/lib/mount-index/project-scenarios` (`resolveProjectScenarioBody`) | `lib/scenarios/location.ts` `resolveScenarioLocation` (scenarios lane) |
| `fixtures/build-chat-scenario-fixture.ts` | `general-/group-/project-scenarios` (`ensure*ScenariosFolder`) | `resolveScenarioLocation(scope, id, repos, userId, { ensure: true })` |
| `fixtures/build-groups-projects-fixture.ts` | same + `setGroupScenarioDefault`, `setProjectScenarioDefault` | the location's write API (scenarios lane) |
| `cases/memory-dedup.test.ts` | `@/lib/tools/memory-dedup` (`deduplicateAllMemories`) | NONE (tool deleted) — `lib/memory/maintenance/fold-near-duplicates.ts` is the LLM-free successor |
| `cases/system-body-guards.test.ts` | `@/lib/tools/memory-dedup` (mocked) | remove the mock (route actions gone) |

**(b) Removed exports of surviving modules:** `cases/consolidation-triggers-tier2.test.ts`
← `@/lib/background-jobs/queue-service` `enqueueMemoryConsolidation` (→
`enqueueMemoryMaintenance` / `…Many` / `…ForUser`, `queue-service.ts:739`,
`:753`, `:772`) and `@/lib/memory/consolidation`
`runConsolidation` (→ `consolidate`, and `maintenance/consolidate.ts`
`runConsolidatePhase`; also used via `jest.requireActual(...).runConsolidation`);
`cases/consolidation-service-tier3.test.ts` ← `@/lib/memory/consolidation`
`runConsolidation`; `cases/episodic.ts` ← `@/lib/memory/episodic`
`buildMemoryEmbeddingText`, `resolveWhenPhrase` (row above).

**(c) The 19 `VectorIndicesRepository` importers:** `cases/vector-indices.ts`;
`fixtures/build-carina-memory-extraction-fixture.ts`, `build-carina-query-fixture.ts`,
`build-character-generators-fixture.ts`, `build-consolidation-fixture.ts`,
`build-context-tier3-fixture.ts`, `build-embedding-generate-fixture.ts`,
`build-embedding-remainder-fixture.ts`, `build-ep-mgmt-fixture.ts`,
`build-first-message-context-fixture.ts`, `build-fold-episode-fixture.ts`,
`build-memory-cascade-fixture.ts`, `build-memory-gate-fixture.ts`,
`build-memory-housekeeping-fixture.ts`, `build-memory-pipeline-jobs-fixture.ts`,
`build-memory-processor-fixture.ts`, `build-memory-watermark-fixture.ts`,
`build-qtap-import-fixture.ts`, `build-vector-indices-fixture.ts`.

**(d) Runtime-dead (no import; `repos.vectorIndices` is `undefined` at the
pin → `TypeError`):** `fixtures/build-almanack-fixture.ts:1153-1154`,
`build-character-archive-fixture.ts:384-385`,
`build-character-generators-fixture.ts:458-459`, `build-characters-fixture.ts:383-384`,
`build-enclave-step-fixture.ts:314-315`, `build-headshoulders-fixture.ts:269`,
`build-long-chat-fixture.ts:356-357`, `build-orchestrator-fixture.ts:618-619`,
`build-regenerate-swipe-fixture.ts:269,296`, `build-salon-fixture.ts:525-526`,
`build-search-tools-fixture.ts:439`, `build-system-data-fixture.ts:777-778`.
SAFE (`?.`): `provision/migrations-first.ts:103`, `cases/system-restore.test.ts:1071`,
`fixtures/build-restore-archive-bug117.test.ts:215`,
`build-restore-archives-dedupe.test.ts:201`; `provision/dump-fresh-schema.ts:149`
(caught, logged as skipped). Plus `cases/vector-store-tiers.ts:113`
(`builder.save()` gone).

**(e) Shape-RED (string references, not dead):** 24 files name
`autoHousekeepingSettings` / `memoryExtractionLimits` /
`otherExtractionWatermarkMessageId` / `INTER_CHARACTER_MEMORY` /
`MEMORY_HOUSEKEEPING` / `MEMORY_CONSOLIDATION` /
`get|setMemoryExtractionConcurrency` (e.g. `cases/chats-tier2.ts`,
`memories-config.test.ts`, `system-jobs-collection.test.ts`,
`backup-uuid-remap.test.ts`, `system-import-execute.test.ts`,
`fixtures/build-memory-watermark-fixture.ts`,
`migrate-memories-fixture-columns.ts`).

Families whose oracle feeds from the dead cases (for the order's RED list):
`archive_reencrypt_tier2`, `build_context_tier3`, `character_archive_tier2`,
`context_feeders_leaves`, `day_references`, `extract_novel_details`,
`fold_other_pass_tier3`, `frozen_archive_tier2`, `memories_tier2`,
`memory_cascade_tier2`, `memory_delete_tier2`, `memory_gate_tier3`,
`memory_housekeeping_tier2`, `memory_injector`, `memory_merge_plan`,
`memory_name_helpers`, `memory_pools`, `memory_processor_tier3`,
`distill_search_extraction`, `memory_tasks`, `memory_watermark_tier3`,
`memory_weighting`, `orchestrator_tier3` (+ brahma / help-chat variants),
`ranking_blend`, `recall_history`, `recall_tags`, `recall_tuning`,
`recent_conversations_block`, `regenerate_swipe_tier3`,
`messages_swipe_sse_route` (web), `scenario_resolvers`,
`semantic_search_tuning_tier3`, `system_body_guards`, `vault_conv_search`,
`vector_indices_tier2`; and through the dead builders:
`carina_memory_extraction_tier3`, `carina_query_tier3`,
`commonplace_digest_vault_bridge_tier2`, `consolidation_service_tier3`,
`consolidation_triggers_tier2`, `context_summary_service_tier3`,
`data_retention_web_routes`, `embedding_generate_jobs`, `embedding_remainder`,
`anchor_gate_probe_tier3`, `precompute`, `recall_replay`,
`first_message_context`, `fold_episode_tier3`, `memory_pipeline_jobs_tier3`,
`chat_scenario_routes`, `qtap_import`. (A family over a COMMITTED pair stays
runnable until its builder is re-run — RED only on a rebuild.)

---

## 6. v4 jest tests at the pin covering this slice

| path | ~cases |
|---|---|
| `__tests__/unit/lib/database/migration/add-episode-extraction-watermark-v1.integration.test.ts` | 4 |
| `…/migration/add-memory-extraction-cursors-v1.integration.test.ts` | 5 |
| `…/migration/add-memory-superseded-at-v1.test.ts` | 4 |
| `…/migration/move-memory-maintenance-settings-v1.test.ts` | 8 |
| `…/migration/recompute-reinforced-importance-v1.test.ts` | 3 |
| `…/migration/retire-memory-vector-entries-v1.test.ts` | 5 |
| `…/migration/rezone-legacy-memory-occurred-at-v1.test.ts` | 8 |
| `__tests__/unit/lib/database/repositories/memories-bulk-delete.test.ts` | 3 |
| `…/repositories/memories-cap-breakdown.test.ts` | 2 |
| `…/repositories/memories-link-deltas.test.ts` | 8 |
| `…/repositories/memories-recap-bands.test.ts` | 3 |
| `…/repositories/memories-find-by-about.test.ts` | 4 |
| `__tests__/unit/lib/embedding/vector-store.test.ts` | 36 |
| `__tests__/unit/lib/embedding/vector-store-process-singleton.test.ts` | 2 |
| `__tests__/unit/lib/memory/memory-index-and-retire.test.ts` | 26 |
| `__tests__/unit/lib/memory/retire-memories-unlink.test.ts` | 11 |
| `__tests__/unit/lib/memory/policy-protection.test.ts` | 11 |
| `__tests__/unit/lib/memory/index-store-backfill.test.ts` | 1 |
| `lib/schemas/__tests__/patch-schema.test.ts` | 3 |
| `__tests__/unit/background-jobs/child-repositories-proxy.test.ts` | (M — the cursor verbs' `find`/`set`/`clear` prefixes) |

No v4 tests exist for `add-chat-scenario-source-v1`,
`add-memories-superseded-by-index-v1`, `drop-*-jobs-v1`,
`add-tfidf-text-shape-version-v1`, the cursor repository itself, or
`settings-legacy.ts` (the migration test covers `legacyHousekeepingToMaintenance`).

---

## 7. Cross-lane meeting points and PROPOSED C1 (shared Rust spellings)

The keystone PROVIDES everything below in ONE commit the stacked lanes branch
from (the P4.D265 / P4.D279 shape). NEEDS from nobody. Lanes that consume it:
admission / reinforcement (formula, `increment_reinforcement`, link deltas,
footnotes), extraction (cursors, payload), recall (index rows, tier),
maintenance (job type, settings, cap breakdown, dangling links, retire),
carriers (cursor bundle fields, `supersededAt`, settings legacy translation),
scenarios (the two chat columns), TF-IDF (`textShapeVersion`), SPA (the wire).

**C1-1 — the cursor repository** (`crates/quilltap-core/src/db/memory_extraction_cursors.rs`,
NEW; `Writers::memory_extraction_cursors()` in `db/mod.rs` beside
`chat_informs()` `:365`):

```rust
pub const TABLE: &str = "memory_extraction_cursors";
/// v4's MIGRATION text (the shape every real v4 instance carries) — §9 O-2.
pub const MIGRATION_TABLE_DDL: &str = /* §1.10 verbatim */;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtractionCursorPass { OtherFold, EpisodeFold, Turn }
impl ExtractionCursorPass {
    pub const ALL: [Self; 3];                 // v4 enum order
    pub fn as_str(self) -> &'static str;      // "other-fold" | "episode-fold" | "turn"
    pub fn parse(s: &str) -> Option<Self>;
    pub fn bundle_field(self) -> &'static str; // BUNDLE_CURSOR_FIELDS
    pub fn is_fold(self) -> bool;              // FoldCursorPass
}
pub fn extraction_cursor_id(chat_id: &str, pass: ExtractionCursorPass) -> String; // "<chatId>:<pass>"
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractionCursorMap { pub other_fold: Option<String>, pub episode_fold: Option<String>, pub turn: Option<String> }
pub struct MemoryExtractionCursorsRepository<'c> { conn: &'c Connection }
impl MemoryExtractionCursorsRepository<'_> {
    pub fn find_by_chat_id(&self, chat_id: &str) -> Result<ExtractionCursorMap, DbError>;          // RETHROW
    pub fn find_cursor(&self, chat_id: &str, pass: ExtractionCursorPass) -> Result<Option<String>, DbError>; // RETHROW
    pub fn find_all_by_chat(&self) -> Result<Vec<(String, ExtractionCursorMap)>, DbError>;        // RETHROW, rowid order
    pub fn set_cursor(&self, chat_id: &str, pass: ExtractionCursorPass, message_id: &str) -> Result<(), DbError>; // RETHROW
    pub fn clear_cursor(&self, chat_id: &str, pass: ExtractionCursorPass) -> Result<bool, DbError>; // FALLBACK false
    pub fn clear_for_chat(&self, chat_id: &str) -> Result<i64, DbError>;                          // FALLBACK 0
}
```

The `db::fallback` homes carry v4's six safeQuery messages (§1.16 table). The
cursor `pass` read from a row is `ExtractionCursorPass::parse`; an unknown
string (a future v4 pass) is skipped with a WARN (v4's Zod would drop the row in
`findByFilter`).

**C1-2 — the boot ensures** (one module per v4 class, `crates/quilltap-core/src/db/`):
`memory_superseded_at_repair::ensure_memory_superseded_at(main) -> Result<SupersededAtOutcome, DbError>`;
`chats_scenario_source_repair::ensure_chats_scenario_source_columns(main)`;
`memory_extraction_cursors_repair::ensure_memory_extraction_cursors(main, now_iso) -> Result<CursorEnsureOutcome { table_created, other_fold_copied, episode_fold_copied, episode_backfilled, columns_dropped: Vec<&'static str> }, DbError>`;
`memory_maintenance_settings_move::ensure_memory_maintenance_settings_moved(main) -> Result<SettingsMoveOutcome { copied, dropped_columns, dropped_instance_key }, DbError>`;
`memory_vector_mirror_retire::retire_memory_vector_mirror(main) -> Result<VectorRetireOutcome { copied, already_on_row, orphaned, unusable, tables_dropped }, DbError>`;
`retired_job_rows_purge::{purge_inter_character_memory_jobs, purge_housekeeping_consolidation_jobs}(main) -> Result<usize, DbError>`;
`tfidf_text_shape_repair::ensure_tfidf_text_shape_version_column(main)`;
`memory_rewrite_heals::{recompute_reinforced_importance, rezone_legacy_memory_occurred_at}(main, now_iso, zone) -> Result<LedgeredHealOutcome { AlreadyCompleted | Ran { rewritten, total } }, DbError>`
(each exposes `pub const MIGRATION_ID`). Host wiring in `seed_built_ins` in
§1.1 order, each FATAL (`?`), v5's own one INFO (`target: "quilltap::boot"`,
`migrationId = …`) when not a no-op.

**C1-3 — the vector store over rows** (`db/vector_store.rs`, keep the name):
`CharacterVectorStore::load(conn, character_id) -> Self` (infallible, v4's
fallbacks); `dimensions()` = dominant; `search` / `search_filtered` with the
length-skip; DELETE `flush`; `memories_read::find_index_rows_by_character_id(conn,
character_id) -> Vec<IndexRow { id: String, embedding: Vec<f32>, tier: Tier }>`.
A thin `services/memory_index.rs` (`upsert(ws, character_id, &[IndexUpsert])`,
`set_tier(ws, character_id, ids, tier, TierExtra)`, `remove(…) -> usize` (0 —
v5 keeps no cache; every search loads)) is the ONE writer of
`memories.embedding` / `.tier` if the round adopts v4's invariant (§9 O-4).

**C1-4 — the reinforced-importance formula**:
`crates/quilltap-core/src/memory_scoring/reinforced_importance.rs` (NEW pure
leaf): `pub const REINFORCED_IMPORTANCE_STEP: f64 = 0.05; pub fn
reinforced_importance(importance: f64, count: f64) -> f64` (`n = if
count.is_finite() { count.max(1.0) } else { 1.0 }`); `memory_gate::calculate_reinforced_importance`
becomes a re-export until the admission lane deletes `memory_gate.rs`.

**C1-5 — `MemoryMaintenanceSettings`** (`db/instance_settings.rs`):

```rust
pub const KEY_MEMORY_MAINTENANCE: &str = "memoryMaintenance";
pub const MEMORY_MAINTENANCE_CAP_MIN: i64 = 100;
pub const MEMORY_MAINTENANCE_CAP_MAX: i64 = 100_000;
pub const MEMORY_MAINTENANCE_DEFAULT_CAP: i64 = 2000;
#[derive(Clone, Debug, PartialEq)] pub struct MemoryMaintenancePhaseToggles { pub fold_near_duplicates: bool, pub demote: bool, pub retire: bool } // Default all true
#[derive(Clone, Debug, PartialEq)] pub struct MemoryMaintenanceSettings {
    pub enabled: bool, pub per_character_cap: i64,
    pub per_character_cap_overrides: serde_json::Map<String, Value>, // preserve_order; values i64
    pub phases: MemoryMaintenancePhaseToggles,
}
impl MemoryMaintenanceSettings { pub fn to_json(&self) -> Value; pub fn cap_for(&self, character_id: &str) -> i64; }
pub fn parse_memory_maintenance_settings(v: &Value) -> Result<MemoryMaintenanceSettings, Vec<ZodIssue>>;
pub fn get_memory_maintenance_settings(main: &Connection) -> MemoryMaintenanceSettings; // WARN on bad stored value
pub fn set_memory_maintenance_settings(main: &Connection, patch: &Value) -> Result<MemoryMaintenanceSettings, DbError>; // shallow merge
pub fn legacy_housekeeping_to_maintenance(raw: &Value) -> Option<Value>;          // {enabled,perCharacterCap,perCharacterCapOverrides}
pub fn with_memory_maintenance_from_legacy(instance_rows: &[(String, String)], chat_settings_rows: &[Value]) -> Vec<(String, String)>;
```

`to_json` key order `enabled, perCharacterCap, perCharacterCapOverrides,
phases{foldNearDuplicates, demote, retire}`; `memory_settings_schemas_equivalence`
pins the parse; the legacy translator is shared by Δ11 and the restore.

**C1-6 — job types** (`services/job_runner.rs`): `KNOWN_JOB_TYPES` /
`api/system_data.rs::JOB_TYPES` = v4's 23; add `pub const
MEMORY_MAINTENANCE: &str = "MEMORY_MAINTENANCE";` and a `MemoryMaintenancePayload
{ character_id, phases: Option<Vec<MaintenancePhase>>, reason: MaintenanceReason,
exempt_phases: Option<Vec<MaintenancePhase>>, settings: Option<Value> }` with
`MaintenancePhase { FoldNearDuplicates, Consolidate, Demote, Retire, Mirror }`
(`as_str` kebab, v4 order) and `MaintenanceReason { Scheduled, Watermark, Manual }`
in `services/queue_service.rs` (the maintenance lane fills enqueue / dedupe /
handler). `MemoryExtractionPayload` grows (Δ22) with `pub enum
MemoryExtractionPass { Turn, OtherFold, EpisodeFold }` and `fn pass_of(&Value) ->
MemoryExtractionPass`.

**C1-7 — memories row + repository verbs** (`db/memories.rs` /
`memories_read.rs`): `MemCreate.superseded_at`, `MemUpdate` field, the
`IndexRow` read, `CapBreakdown { hot_total, demotable, protected, cold }`,
`DanglingMemoryLinks { related: Vec<DanglingLink>, consolidated_from:
Vec<(DanglingLink, i64)>, superseded_by: Vec<(DanglingLink, Tier)> }`,
`Reinforcement { reinforcement_count, reinforced_importance, last_reinforced_at }`,
`increment_reinforcement(&self, character_id, memory_id, at, by: i64)`,
`add_related_memory_ids` / `remove_related_memory_ids(&self, character_id,
memory_id, ids) -> Result<Option<Vec<String>>, DbError>`,
`remove_related_memory_ids_everywhere(&self, ids) -> Result<usize, DbError>`,
`append_reinforcement_footnotes(&self, character_id, memory_id, details) ->
Result<Option<(String, Vec<String>)>, DbError>`, `bulk_delete_unsafe(&self,
character_id, ids) -> Result<i64, DbError>`, the `find_*` reads of §1.16;
`policy_protection.rs` (`PolicyProtection { Manual, Digest, Cold }`,
`policy_protection_of(source, tier)`, `partition_by_policy_protection`); and
**the retire signatures** (`services/memory_retire.rs`: `RetireReason` (11, kebab
`as_str`), `RetireOptions { reason, keep_manual: bool }`, `RetireReport`
(§1.17 fields + `SparedBy`), `retire_memories(ws, ids, &opts) ->
Result<RetireReport, DbError>` (writer-side), `announce_retirement(db, &report,
…)` (effects after commit), `retire_chat_memories`, `preview_chat_wipe`) —
whether the keystone also lands the BODY is §9 O-1.

**C1-8 — the wire** (`api/types.rs`, ONE fence `// === <keystone>: the
64743ee13 round's wire ===`; responses reuse `Response::Memory(Value)`;
arms answer P4.D265's R-G refusal naming the owner):

```rust
/// v4 POST `/memories?action=maintain` (`maintainSchema`, route.ts:129-141) — RAW (R-H).
MemoryMaintain { options: serde_json::Value },
/// v4 GET `?action=maintenance-status[&characterId=]`.
#[serde(rename_all = "camelCase")] MemoryMaintenanceStatus { #[serde(default)] character_id: Option<String> },
/// v4 POST `?action=repair` (`repairSchema`, :144-148) — RAW.
MemoryRepair { options: serde_json::Value },
/// v4 GET `?action=maintenance-config` — `{success, settings}`.
MemoryMaintenanceConfigGet,
/// v4 POST `?action=maintenance-config` (`maintenanceConfigSchema`, :162-167) — RAW.
MemoryMaintenanceConfigSet { config: serde_json::Value },
```

`MemoryMaintenanceConfigGet/Set` can be FILLED by the keystone (they need only
C1-5) instead of refused. Retire: `MemoryHousekeepPreview`, `MemoryHousekeep`,
`MemoryHousekeepSweep`, `MemoryHousekeepingConfigGet`,
`MemoryHousekeepingConfigSet`, `MemoryExtractionConcurrencyGet`,
`MemoryExtractionConcurrencySet`, `MemoryConsolidate`, `MemoryDedupPreview`,
`MemoryDedupRun` (+ `Response::MemoryMaintenance` if nothing else uses it —
`conversation_summaries_regenerate` does, `api/memory_maintenance.rs:72`, so it
stays). Census: `dispatch_wrong_type_census.rs` rows for
`MemoryMaintenanceStatus.character_id`.

**C1-9 — chats columns**: `ChatCreate` / `ChatUpdate` gain `scenario_source:
Option<Value>` (stored `JSON.stringify` key order `mountPointId, path,
bodySha256`) and `scenario_notes: Option<String>`; `chats_read` marshal
(Δ3). Whether the keystone lands only the plumbing and the scenarios lane the
semantics is a planner split (the column ensure is keystone either way).

---

## 8. SPA / help / docs / CLI touch points (this slice only)

- SPA `apps/web/src/app/core/core-contract.ts:6012-6060` (`MemoryHousekeepOptions`,
  `memoryHousekeep*`, `memoryHousekeepingConfig*` request types), `:6343-6347`
  (union), the extraction-concurrency and dedup request types, and
  `MemoryGenerateEmbeddings.batchSize` (`af48f6cc2` stripped it); new request
  types for C1-8. Consumers `apps/web/src/app/memory/housekeeping-dialog.ts`,
  `memory.api.ts:21` — the SPA lane's; the keystone must not leave the SPA
  sending a variant that no longer deserialises without the SPA lane in the
  same round (§9 O-6).
- `docs/v4/`: `DDL.md` (vector tables retired; cursor table; `supersededAt`;
  the scenario pair; `textShapeVersion`; the chat-settings drops) — the docs
  lane re-vendors.
- CLI (`crates/quilltap-cli`): no `memories` verb family in v5 (`db_cmd.rs`
  only); `quilltap db` table listings that name `vector_*` (grep before
  landing).
- `help/`: none in this slice.

---

## 9. Open points for the order (observations / questions — not rulings)

- **O-1 — keystone size.** This slice is larger than P4.D265's: two re-dumps,
  ~11 ensures (two ledger-gated, three destructive), a new repository, ~14
  memories verbs + a removal sweep, the vector store re-point with ~25
  production call sites, the settings move, the job-type move, the wire, and
  ~60 harness files to re-point / retire. Consider **KEYSTONE-A** (D23 + all
  ensures + cursor repo + settings + job types + wire + row structs + the
  vector store LOAD over rows, ending in a green gate) and **KEYSTONE-B** (the
  index-store writer + `retire_memories` + migrating every §4.4 deletion caller
  + `delete` refusing + `db/vector_indices.rs` deleted). Until B lands, A must
  keep the old deletion paths compiling WITHOUT the vector tables (their
  `flush` / `delete_by_character_id` calls must be removed in A, or the
  half-completed character delete hazard ships).
- **O-2 — the cursor table shape on a FRESH v5 instance.** Every real v4
  instance has the migration shape (UNIQUE + live FK cascade); the generateDDL
  dump has the bare shape. With the bare shape, a chat delete on a v5-fresh
  instance leaves orphan cursor rows that a later chat reusing the id… (ids are
  UUIDs, so harmless) — but it is a write-semantics divergence of the
  `idx_doc_mount_folders_mp_path` class (the 2026-10-06 ruling). Options:
  (a) keep R-A (generateDDL text) and record the asymmetry; (b) provision the
  migration text for this one table. Also: with the FK live, any v5 writer
  that inserts a cursor BEFORE its chat row exists (restore / `.qtap` import
  ordering — carriers lane) fails `FOREIGN KEY constraint failed` on real
  instances. Needs a ruling.
- **O-3 — the episode-fold backfill's once-only gate.** v4's
  `add-episode-extraction-watermark-v1` re-qualifies (`shouldRun` true) on
  EVERY instance once the cursor migration dropped its column; only v4's
  ledger stops it. A v5 ensure that ran its backfill unledgered would re-insert
  (DO NOTHING) every boot — harmless in effect but an `ALTER ADD` + `DROP` pair
  if ported literally. Options: (a) composite ensure whose episode-backfill arm
  is gated on the ledger row `add-episode-extraction-watermark-v1` and STAMPS
  it; (b) gate on "the cursor table was absent at boot start" (no stamp; a v4
  boot later would run its own convergent backfill + cursor copy); (c) port
  literally with a stamp. Effects-identical for the first boot; differs in what
  a later v4 boot does. Also confirm whether v5's pre-round fold EPISODE pass
  kept any position (v5 has no episode column — so v4's backfill from the last
  summary anchor is exactly what a pre-round v5 instance should get).
- **O-4 — `MemoriesRepository::delete` refusing.** v4 throws; v5 has ~14
  production callers of the delete family (§4.4). The refusal can only land
  with (or after) the retire migration. Decide whether v5 mirrors the THROW
  (ERROR line + `Err`) or simply deletes the verb (a compile-time refusal); v4's
  line is reachable only from a bug, so the compile-time route is the
  "pinned by construction" alternative — but then the ERROR text is unported.
- **O-5 — the data rewrites on committed fixtures.** Widening the ~49 pairs
  through `recompute-…` and `rezone-…` changes `reinforcedImportance` /
  `occurredAt` cells other families' bodies include (recall, recap bands,
  anchors), and the rezone result depends on the widening machine's zone. Either
  (a) widen with both rewrites under a pinned `TZ`/`QUILLTAP_TIMEZONE` and
  stamp the ledger in the pair (so host-booting tests see no-ops), or (b) skip
  them for fixtures and stamp the ledger rows anyway (fixture = "already
  migrated"), recording the residual. A host-booting quilltap-web test over an
  unstamped pair WILL run both rewrites at boot.
- **O-6 — retired wire variants: delete or refuse?** Deleting the Request
  variants makes an old SPA build's request fail deserialisation (`Invalid
  request:`), not v4's `Unknown action: housekeep` 400 with
  `availableActions`. The HTTP edge (`quilltap-web`) could synthesise v4's
  unknown-action body for the HTTP route forms; `/api/dispatch` typed requests
  have no v4 analog. Suggest: delete the variants in the keystone only if the
  SPA lane lands in the same round (it must, for the Maintenance card).
- **O-7 — `textShapeVersion` affinity.** Confirm the generateDDL type by the
  real dump (predicted REAL). If REAL, every v5 reader must take
  INTEGER-or-REAL; the TF-IDF lane's refit-policy compare (`< 1`) must use the
  numeric value, not `get::<i64>`.
- **O-8 — rezone fidelity.** v4's zone source on real data is env → system
  (no `chat_settings.timezone` column on real v4). v5's "system zone" must be
  the same IANA name Node's `Intl` resolves (`host_zone::system_display_zone`
  — verify it is not the display-zone override). The DST-gap answer is v4's
  two-pass algorithm; the round needs a tier-1 family over
  `zonedTimeToUtcMs` / `zonedDayBoundsIso` (the time lane may already own the
  calendar port — coordinate so the keystone's rezone calls the SAME function).
- **O-9 — the startup backup runs before the destructive ensures** in v5 (v4:
  after migrations). Keep (safer: a pre-DROP physical backup exists) and record,
  or align? No v4-visible output differs except the backup's content.
- **O-10 — v5 builds OLDER than this round must not open an instance this
  round migrated** (the reverse of the §1 hazard): the dropped columns / tables
  break their reads exactly as the ledger describes for the forward direction.
  The dogfood copy discipline (a copy per build vintage) should be restated in
  the round record.
- **O-11 — `memoryMaintenance` cap overrides key order.** v4 iterates
  `Object.entries` of the parsed override object; integer-like keys would sort
  first in V8 (character ids are UUIDs, so never) — use `preserve_order` and
  record the assumption, or add a row with a numeric-string key.
- **O-12 — loud-refusal deferrals that fit this slice:** none needed for the
  schema; `MemoryMaintain` / `MemoryRepair` / `MemoryMaintenanceStatus` refuse
  until their lanes fill them (R-G). The `MEMORY_MAINTENANCE` handler answers
  the runner's "recognized but not yet available" fallback until the
  maintenance lane registers it — meaning a v4-written queued
  `MEMORY_MAINTENANCE` row on a shared instance fails on v5 in that window
  (shared-instance note for the round record).
