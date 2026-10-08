# Survey — v4 `3ee3b1342` "Wardrobe wear ledger (#81)": the SERVER CORE

Read-only survey, 2026-10-08, for the `/setupphase` round that absorbs v4's
wardrobe programme. Scope: schema + migrations, the repository, every equip
path through the chokepoint, the read API + tools, lifecycle (cascade / item
delete / transfers), and the v5 counterparts. **Out of scope** (other
surveys): the export/import/backup/restore carriers (`lib/backup/*`,
`lib/export/*`, `lib/import/*`, `uuid-remap.ts`, `qtap-export.schema.json`)
and the SPA (`components/wardrobe/*`, `lib/query/keys.ts`, the client half of
`staged-live-outfits.ts`).

Sources: v4 `~/source/quilltap-server` at HEAD **`f5e953a3f`** (read with
`git show`; never checked out). The commit is `3ee3b1342` (2026-10-07,
106 files, +6610/−199). **HEAD is SEVEN commits past it**, and three of them
touched this survey's files: `7c8572869` (#82 item images — `item-route-steps.ts`,
the item routes, `wardrobe.types.ts`, `wardrobe-handler-shared.ts`),
`b3f937076` (tool pictures — `wardrobe-{list,read}-handler.ts`,
`wardrobe-{list,read}-tool.ts`), `039f7017c` (bug 179 — the child proxy).
Line numbers below are **HEAD** unless marked `@3ee3`; where the later commits
changed the same lines it is called out. v5 = `quilltap-v5` main `4895d1200`.

Everything below is from the HUNKS and the post-commit files, not the commit
prose.

---

## 1. v4 at HEAD — schema + migrations (file:line facts)

### 1.1 The DDL — `lib/database/backends/sqlite/wardrobe-wear-stats-ddl.ts` (73 lines, unchanged since `3ee3`)

- `:16` `WARDROBE_WEAR_STATS_TABLE = 'wardrobe_wear_stats'`.
- `:18-34` `WARDROBE_WEAR_STATS_DDL` — three statements, verbatim:

```sql
CREATE TABLE IF NOT EXISTS "wardrobe_wear_stats" (
    "id" TEXT PRIMARY KEY,
    "itemId" TEXT NOT NULL,
    "wearerCharacterId" TEXT,
    "wearCount" INTEGER NOT NULL DEFAULT 0,
    "firstWornAt" TEXT NOT NULL,
    "lastWornAt" TEXT NOT NULL,
    "lastWornChatId" TEXT,
    "createdAt" TEXT NOT NULL,
    "updatedAt" TEXT NOT NULL
  )
CREATE UNIQUE INDEX IF NOT EXISTS "idx_wardrobe_wear_stats_item_wearer"
    ON "wardrobe_wear_stats" ("itemId", COALESCE("wearerCharacterId", ''))
CREATE INDEX IF NOT EXISTS "idx_wardrobe_wear_stats_wearer"
    ON "wardrobe_wear_stats" ("wearerCharacterId")
```

  (Whitespace: the CREATE TABLE's columns are indented 4 spaces, the closing
  `)` 2; the index bodies are on a continuation line indented 4. SQLite stores
  the text as written minus `IF NOT EXISTS`, so `sqlite_master.sql` comparands
  see exactly this.)
- `:7-11` the why of `COALESCE(...,'')`: NULLs are distinct in a plain UNIQUE
  index; folding to `''` admits ONE unattributed row per item, which is what
  makes `ON CONFLICT ("itemId", COALESCE("wearerCharacterId", ''))` legal.
- `:42-52` `WARDROBE_WEAR_INCREMENT_SQL` — the one atomic upsert:
  `INSERT … VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?) ON CONFLICT (…COALESCE…) DO UPDATE SET
  "wearCount" = "wearCount" + 1, "firstWornAt" = MIN("firstWornAt", excluded."firstWornAt"),
  "lastWornChatId" = CASE WHEN excluded."lastWornAt" >= "lastWornAt" THEN excluded."lastWornChatId" ELSE "lastWornChatId" END,
  "lastWornAt" = MAX("lastWornAt", excluded."lastWornAt"), "updatedAt" = excluded."updatedAt"`.
  Note the ORDER of the SET clauses: `lastWornChatId` is decided BEFORE
  `lastWornAt` moves (SQLite evaluates `excluded`/old values consistently, so
  order is cosmetic, but the byte text matters for a capture).
- `:55-73` `wardrobeWearIncrementParams({id,itemId,wearerCharacterId,at,chatId,now})`
  → `[id, itemId, wearerCharacterId, at, at, chatId, now, now]` (firstWornAt
  = lastWornAt = `at`; createdAt = updatedAt = `now`).

### 1.2 Migration `migrations/scripts/add-wardrobe-wear-stats-table-v1.ts` (77 lines)

- `:27` id `add-wardrobe-wear-stats-table-v1`; `:32` description
  `Create wardrobe_wear_stats table: who has worn which wardrobe item, how often, and when`;
  `:33` `introducedInVersion: '4.10.0'`; `:34` `dependsOn: ['sqlite-initial-schema-v1']`.
- `:36-39` `shouldRun`: `isSQLiteBackend() && !sqliteTableExists('wardrobe_wear_stats')`.
- `:41-59` `run`: `db.exec` each of the three DDL statements; INFO
  `Created wardrobe_wear_stats table` `{context: 'migration.add-wardrobe-wear-stats-table-v1'}`;
  result `itemsAffected: 1`, message `Created wardrobe_wear_stats table`.
- `:60-75` catch: ERROR `Failed to create wardrobe_wear_stats table` `{context, error}`;
  result `success: false`, `itemsAffected: 0`, same message with `Failed to`.
- Ledger row: written by the runner (`migrations/index.ts`, `recordCompletedMigration`
  via `migrations/state.ts:114`) into `migrations_state (id, completedAt, quilltapVersion, itemsAffected, message)`.

### 1.3 Seed `migrations/scripts/seed-wardrobe-wear-stats-v1.ts` (157 lines) — the algorithm EXACTLY

- `:36` id `seed-wardrobe-wear-stats-v1`; `:73` description
  `Seed the wardrobe wear ledger from every chat's current equipped outfits`;
  `:74` `4.10.0`; `:75` `dependsOn: ['add-wardrobe-wear-stats-table-v1']`.
- `:77-82` `shouldRun`: SQLite backend AND `wardrobe_wear_stats` exists AND
  `chats` exists AND `chats` has BOTH columns `equippedOutfit` and `updatedAt`.
  **No ledger check of its own** — once-only is the runner's `isMigrationCompleted`
  gate (`migrations/index.ts:144-147`, checked BEFORE `shouldRun` at `:164`).
  The header says so: `:15` "Idempotent in effect only if run once — it is, by
  the migration ledger."
- `:49-69` `wearsFromEquippedOutfit(rawOutfit)` (pure, exported):
  `null`/empty → `[]`; `JSON.parse` throws → `[]`; not a plain object (null,
  non-object, array) → `[]`; for each `[characterId, slots]` of
  `Object.entries(parsed)` in insertion order: skip `''` characterId; for each
  id of `allEquippedItemIds(normalizeEquippedSlots(slots))` push
  `{characterId, itemId}` (skip `''`). `allEquippedItemIds`
  (`lib/schemas/wardrobe.types.ts:291-293`) is
  `Array.from(new Set(WARDROBE_SLOT_TYPES.flatMap(s => slots[s] ?? [])))` —
  so the SAME id in two slots of one chat is ONE wear, slot order top → bottom
  → footwear → accessories → hair, first-seen. `normalizeEquippedSlots`
  (`:309-316`) is `EquippedSlotsSchema.safeParse(raw ?? {})`, else per-slot
  `Array.isArray ? filter(typeof string) : []` — so a legacy row lacking slots
  (`{top:['x']}`) reads fine (test `:137-143`).
- `:88-93` reads ALL chats up front: `SELECT "id", "updatedAt", "equippedOutfit"
  FROM "chats" WHERE "equippedOutfit" IS NOT NULL` — **no ORDER BY** (rowid
  scan order). `:94` prepares `WARDROBE_WEAR_INCREMENT_SQL`; `:95` ONE `now`.
- `:103-123` ONE synchronous `db.transaction`: per chat `scanned++`; wears =
  `wearsFromEquippedOutfit`; `chatsWithOutfits++` when any; per wear
  `increment.run(…params({id: randomUUID(), itemId, wearerCharacterId:
  characterId, at: chat.updatedAt, chatId: chat.id, now}))`, `wears++`;
  `reportProgress(scanned, chats.length, 'chats')` after EVERY chat (inside the
  transaction — `:101-102` "the loading screen cannot update inside it").
- Date used: **the chat's `updatedAt`** for both first and last worn; `now` for
  createdAt/updatedAt. A second chat wearing the same item → `wearCount + 1`,
  `lastWornAt = MAX`, `lastWornChatId` moves when `>=` (ties go to the LATER
  processed chat — rowid order).
- Composites: `:11` "Whole composite ids left in legacy rows are credited as
  themselves"; ids that no longer resolve produce orphan rows, never pruned (`:11-13`).
- Dedupe: only within one (chat × character) via the Set; across chats it is
  the upsert. No dedupe across characters (two characters wearing the coat in
  one chat = two rows).
- `:125-130` INFO `Seeded the wardrobe wear ledger from current outfits`
  `{context: 'migration.seed-wardrobe-wear-stats-v1', chatsScanned, chatsWithOutfits, wearsCredited}`;
  `:132-139` result `itemsAffected: wears`, message
  `` `Credited ${wears} wear(s) across ${chatsWithOutfits} chat(s)` ``.
- `:140-155` catch: ERROR `Failed to seed the wardrobe wear ledger` `{context, error}`;
  `success:false`, `itemsAffected:0`, message `Failed to seed the wardrobe wear ledger`.
- Pretty labels (`lib/startup/prettify.ts:154-155`): `Ruling a ledger for who
  has worn what` / `Taking stock of what the cast is wearing this minute`.

### 1.4 Registration

- `migrations/scripts/index.ts`: imports `:417-419` (comment
  `// Wardrobe wear ledger: wardrobe_wear_stats table, then a one-time seed from current outfits`);
  the runner array `:849-851` (AFTER `impersonationVoiceModeMigration`);
  the trailing re-export `:1259-1261`. (The commit's 4th squashed step fixed
  them being only in the re-export block — the registry guard caught it.)
- `lib/database/repositories/index.ts`: export `:61`, import `:106`, container
  field `wardrobeWear: WardrobeWearRepository` `:156`; `createRepositories`
  hoists `const chatsRepo = new ChatsRepository()` `:175`, `chats: chatsRepo`
  `:180`, and `wardrobeWear: new WardrobeWearRepository(chatsRepo)` `:219-220`
  (comment: "The wear ledger's chokepoint writes equipped slots through this chats repo").

### 1.5 **Does the repository (generateDDL) shape differ from the hand DDL? YES — materially.**

- Registration as a repository means `AbstractBaseRepository.getCollection`
  (`base.repository.ts:109-121`) calls `ensureCollection(name, schema)` on first
  access → `backend.ts:762-790` → `generateDDL(name, schema)`
  (`schema-translator.ts:433-449`) over `WardrobeWearStatsRowSchema`
  (`lib/schemas/wardrobe-wear.types.ts:24-37`). Generated text (derived from
  the translator's rules, not measured — the lane must dump it):
  - `"id" TEXT PRIMARY KEY NOT NULL` (UUIDSchema).
  - `"itemId" TEXT NOT NULL`; `"wearerCharacterId" TEXT` (nullable → no NOT NULL).
  - **`"wearCount" REAL NOT NULL`** — `mapToSQLiteType` (`:316-334`) emits
    `INTEGER` only when BOTH `min` and `max` are integers; `.int().nonnegative()`
    sets `min = 0` and no `max` → **`REAL`**. `generateColumnConstraints`
    (`:343-358`) adds NOT NULL (no default) — **no `DEFAULT 0`**. (The same rule
    put `background_jobs.attempts REAL DEFAULT 0` in `fresh_schema.json`.)
  - `"firstWornAt" TEXT NOT NULL`, `"lastWornAt" TEXT NOT NULL`,
    `"lastWornChatId" TEXT`, `"createdAt" TEXT NOT NULL`, `"updatedAt" TEXT NOT NULL`.
  - Indexes: generateDDL's rules (`:265-278`) add `idx_<table>_userId` (none —
    no `userId`) and **`idx_wardrobe_wear_stats_createdAt ON ("createdAt" DESC)`**;
    it emits **NEITHER** the UNIQUE COALESCE index NOR the wearer index.
- On a REAL v4 boot the generateDDL shape never materializes: PHASE 1 runs
  migrations first (`instrumentation.ts:420-437`), so the hand DDL creates the
  table; the repository's `CREATE TABLE IF NOT EXISTS` is then a no-op and only
  its `createdAt` index is added. A migrated v4 instance therefore has:
  hand table text (`INTEGER NOT NULL DEFAULT 0`) + THREE indexes.
- **But v5's `fresh_schema.json` dumper (`harness/oracle/provision/dump-fresh-schema.ts:101-125`)
  drives EVERY repository via `count()` (a base-repo method the wear repo
  inherits, `base.repository.ts:456`) with NO migrations** — so the SIXTH D23
  re-dump WILL carry `wardrobe_wear_stats` in the **generateDDL shape** (REAL,
  no default, `createdAt` index only). `dump-migration-indexes.ts` (migrations
  first, then repos) will carry the two migration indexes (names not in
  `fresh_schema.json`). A v5-provisioned instance thus gets: Zod table text +
  `createdAt` index + (after the `migration_indexes.json` re-dump) the UNIQUE
  COALESCE + wearer indexes. `provisioning_equivalence` (1b) compares column
  **NAMES only** (`crates/quilltap-harness/tests/provisioning_equivalence.rs:887-915`)
  — the REAL-vs-INTEGER + missing-DEFAULT asymmetry passes silently; (1a) keeps
  v5 on the generateDDL text by the standing R-A.
- Consequences for v5 (see §6): the UNIQUE index is the `ON CONFLICT` target —
  without it SQLite refuses the increment statement outright; and a REAL
  affinity cell reads back `1.0`, so every v5 read of `wearCount` must be
  type-tolerant (v4 normalizes with `Number()` at `wardrobe-wear.repository.ts:165-167`).

### 1.6 `__tests__/unit/migrations/wardrobe-wear-stats.test.ts` (144 lines) — v4's own expectations

- `:72` the `chats` stand-in is just `(id, updatedAt, equippedOutfit)`.
- `:80-91` table created once (`shouldRun` true → false), and two
  `wearerCharacterId = NULL` rows for `'coat'` throw `/UNIQUE/`.
- `:93-135` four chats: chat-1 (2026-01-01, A: top coat+shirt, bottom slacks;
  B: top coat), chat-2 (2026-03-01, A: top coat AND accessories coat), chat-3
  `null`, chat-4 `'not json'` → `itemsAffected` **5**, **4 rows**; coat×A =
  `{wearCount 2, first 2026-01-01, last 2026-03-01, lastWornChatId 'chat-2'}`;
  coat×B count 1; `reportProgress` last called `(3, 3, 'chats')` (only
  non-NULL outfits are scanned — 3 of the 4 chats).
- `:137-143` `wearsFromEquippedOutfit({A:{top:['x']}})` → one wear; `null` → `[]`; `'[]'` → `[]`.

---

## 2. v4 at HEAD — `lib/database/repositories/wardrobe-wear.repository.ts` (479 lines, unchanged since `3ee3`)

### 2.1 Types

- `:61-67` `EquipSource = 'ui' | 'tool' | 'chat-start' | 'participant-added' | 'merge' | 'take-off'`
  — comments: `ui` = dialog set_all / wear / replace / add_to_slot; `tool` =
  `wardrobe_wear`, `wardrobe_create equip_now`; `merge` NEVER counts;
  `take-off` = removeFromSlot ("nothing can be newly worn, kept for the log line").
- `:69-78` `CommitEquippedOutfitInput {chatId, characterId, nextSlots, wornBundles?: {id, leafIds[]}[], source, at?}`.
- `:80-85` `CommitEquippedOutfitResult {slots, newlyWornLeafIds, creditedBundleIds, changed}`.
- `:87-92` `WardrobeWearIncrement {itemId, wearerCharacterId, chatId, at}`.
- `:95-98` `EquippedOutfitStore { getEquippedOutfitForCharacter, setEquippedOutfit }` — the chats slice.

### 2.2 The pure diff — `diffEquippedOutfit(prior, next, wornBundles=[])` `:104-130` (exported)

- `before = Set(prior ? allEquippedItemIds(prior) : [])`; `after = allEquippedItemIds(next)`;
  `newlyWorn = after.filter(!before.has)` (slot order, first-seen, deduped).
- Bundles: iterate `wornBundles` in order; skip a bundle id already credited;
  **skip a bundle whose own id is in `newlyWornSet`** (`:117-119` — a legacy
  whole-composite id stored in the slots is already counted by the leaf diff);
  credit when `bundle.leafIds.some(id in newlyWornSet)`.
- `changed = !sameSlots(prior, next)` — `sameSlots` `:132-144`: `prior == null`
  → `next` has zero ids; else union of keys, per key `?? []`, length + positional equality.

### 2.3 `summarize(rows)` `:147-161` and `normalizeRow` `:165-167`

- Skips `wearCount <= 0`; sums counts; `firstWornAt` = min by string `<`;
  `lastWornAt` = max by **strict `>`** (ties keep the FIRST row seen — SELECT
  order, unordered → rowid) and carries that row's `lastWornChatId`.
- `normalizeRow`: `wearCount: Number(row.wearCount)` (better-sqlite3 may hand a bigint).

### 2.4 The class — `WardrobeWearRepository extends AbstractBaseRepository<WardrobeWearStatsRow>`

- `:174-176` ctor `(private readonly chats?: EquippedOutfitStore)` → `super(TABLE, RowSchema)`.
- `:182-195` generic `create`/`update`/`delete` → `_create`/`_update`/`_delete` ("prefer the ledger methods").
- **`commitEquippedOutfit(input)` `:223-262`** (the chokepoint):
  1. `:225-227` no chats store → `throw new Error('WardrobeWearRepository.commitEquippedOutfit needs a chats store')`.
  2. `:229` `prior = await chats.getEquippedOutfitForCharacter(chatId, characterId)`.
  3. `:230` `written = await chats.setEquippedOutfit(chatId, characterId, nextSlots)` —
     **the slots are written even when equal to prior** (`:205-207`).
  4. `:231-234` `!written` → `log.warn('Equipped outfit write failed; no wears credited', {chatId, characterId, source})`
     then **`throw new Error(`Failed to save the equipped outfit for character ${characterId} in chat ${chatId}`)`**.
  5. `:237-241` diff over **`nextSlots`** (not `written`) with `input.wornBundles ?? []`.
  6. `:243` `credit = source === 'merge' ? [] : [...newlyWornLeafIds, ...creditedBundleIds]`.
  7. `:244-249` when non-empty: `at = input.at ?? new Date().toISOString()`;
     `incrementWears(credit.map(itemId => ({itemId, wearerCharacterId: characterId, chatId, at})))`.
  8. `:251-259` `log.debug('Committed equipped outfit', {chatId, characterId, source, newlyWorn, creditedBundles, credited, changed})`.
  9. `:261` returns `{slots: written, newlyWornLeafIds, creditedBundleIds, changed}`.
  - `:214-221` recorded NON-atomicity: prior read + slot write are NOT inside the
    credit transaction; two simultaneous equips race on the slots "the same way
    they always have"; the ledger cost is "at most a credit for a garment the
    other request then took off". Logger: `logger.child({ module: 'wardrobe-wear' })` `:58`.
- **`incrementWears(entries)` `:272-290`**: empty → return; ONE `now`; `runAtomically`:
  prepare `WARDROBE_WEAR_INCREMENT_SQL`, `statement.run(...params({id: randomUUID(), …, now}))` per entry.
- **`runAtomically(work)` `:299-305`**: `getRawDatabase()` else
  `throw new Error('Wardrobe wear ledger: the main database is not initialized')`;
  `db.transaction(() => work(db))()` — better-sqlite3 nests as a **SAVEPOINT**
  when the job applier's `BEGIN IMMEDIATE` is open (`:292-298`; test `:122-127`).
- **`deleteByItemIds(itemIds)` `:308-318`**: `Set(filter(Boolean))`; chunked by
  `SQLITE_VARIABLE_CHUNK_SIZE`; `DELETE FROM "wardrobe_wear_stats" WHERE "itemId" IN (?,…)`
  via `rawQuery` (NOT atomic across chunks); `log.debug('Dropped wear ledger rows for deleted items', {itemCount})`.
  "A composite's deletion leaves its components' rows alone" (`:307`).
- **`foldWearerIntoUnattributed(characterId)` `:327-363`**: ONE `now`; ONE
  `runAtomically`: `SELECT * … WHERE "wearerCharacterId" = ?` → none → `0`;
  upsert each as `(randomUUID(), itemId, NULL, wearCount, firstWornAt, lastWornAt, lastWornChatId, now, now)`
  with `ON CONFLICT … DO UPDATE SET "wearCount" = "wearCount" + excluded."wearCount",
  MIN first, CASE >= chat, MAX last, "updatedAt" = excluded."updatedAt"` (`:338-348`);
  then `DELETE … WHERE "wearerCharacterId" = ?` (`:353`); returns row count.
  `:357-362` when `> 0`: `log.info('Folded a departed wearer into the unattributed wear ledger', {characterId, rowCount})`.
  Why a fold not `SET NULL`: the unique index admits one unattributed row (`:320-325`).
- **`upsertRows(rows)` `:371-398`** (import/restore carrier — other survey, but
  the SQL is core): per row `rawQuery(INSERT … ON CONFLICT … DO UPDATE SET
  wearCount/firstWornAt/lastWornAt/lastWornChatId/updatedAt = excluded.*)` —
  REPLACE semantics, no increment, NOT atomic across rows; callers must pre-merge
  duplicate keys (`:365-370`); `log.debug('Upserted wear ledger rows', {rowCount})`.
- **`findSummaries(itemIds)` `:408-430`**: dedupe; map pre-filled with
  `neverWornSummary()` for EVERY id; `safeQuery(…, 'Error reading wear summaries', {itemCount}, result)`
  — the fallback IS the pre-filled map, so a missing table or broken read = all never-worn.
- **`findHistory(itemId)` `:433-454`**: rows with `wearCount > 0`; wearers
  mapped `{characterId, wearCount, firstWornAt, lastWornAt, lastWornChatId}` and
  sorted `lastWornAt` DESC by string compare (V8 stable sort → SELECT order on ties);
  `{...summarize(rows), wearers}`; fallback `{...neverWornSummary(), wearers: []}`
  under `'Error reading wear history'`.
- **`findRowsForItems(itemIds)` `:457-469`** chunked `SELECT *`; **`findRowsForWearer(characterId)` `:472-478`**.

### 2.5 `__tests__/unit/lib/database/repositories/wardrobe-wear.repository.test.ts` (343 lines) — expectations

Real better-sqlite3 `:memory:` with the hand DDL (`:71-73`), `rawQuery` and
`getRawDatabase` mocked onto it (`:74-79`), a fake `EquippedOutfitStore` with
`writes` counter (`:48-64`).
- `:90-99` insert at 1 then increment moves last forward (count 2, first 01-01, last 02-01, chat 2).
- `:101-110` an EARLIER wear landing late never rewinds `lastWornAt`/chat.
- `:112-120` a batch with one bad row (`itemId null`) rejects and leaves 0 rows (whole or nothing).
- `:122-127` nests inside an open `BEGIN IMMEDIATE`; `ROLLBACK` erases it.
- `:129-136` one row per wearer; `:138-149` two NULL-wearer increments → one row count 2.
- `:151-190` fold sums counts, min first / max last + its chat; `:192-200` fold rolls back whole when the final DELETE fails (wearer rows intact, count 1).
- `:202-215` `findSummaries` totals across wearers; unknown id → the zero summary.
- `:217-226` `findHistory` wearers most recent first; never → `[]`.
- `:228-235` `deleteByItemIds` drops only those items.
- `commitEquippedOutfit` `:237-330`: credits a newly worn leaf only (`:241-248`);
  a removal credits nothing, re-wearing later is a second wear (`:250-256`);
  two characters each earn a wear of a shared garment (`:258-262`); a bundle
  credited ONCE when ≥1 leaf transitioned (`:264-276`, `creditedBundleIds == ['suit']`);
  a bundle whose leaves were all on earns nothing and `changed === false` (`:278-289`);
  `'merge'` writes slots, credits nothing (`:292-298`); equal slots → `changed false`
  but `writes + 1` (`:301-308`); **a `null` slot write throws `/Failed to save the equipped outfit/` and 0 rows** (`:310-316`);
  replaying two buffered ops credits each against the TRUE prior state (`:318-329`).
- `diffEquippedOutfit` `:333-342`: a legacy whole-composite id is a plain id; an empty write over nothing is `changed false`.

---

## 3. v4 at HEAD — every equip path through the chokepoint

### 3.1 `lib/wardrobe/apply-outfit-selections.ts` (616 lines)

- `:24` imports `dissolveBundleToLeaves, dissolveBundlesInSlotsWithCredit`; `:25` `EquipSource`; `:49` `buildDefaultOutfitWithCredit`.
- `:156-163` `OutfitSelectionContext.source?: Extract<EquipSource, 'chat-start' | 'participant-added' | 'merge'>`.
- `:167-170` `ResolvedOutfit {slots, wornBundles}`.
- `:178-192` `manualWornBundles(wornBundleIds, pool)`: `new Set(ids)`; unknown id
  → dropped; `dissolveBundleToLeaves(bundle, byId)` null (not a bundle / no
  wearable parts) → dropped; else `{id, leafIds}`.
- `resolveSelection` `:364-573`: `default` → `buildDefaultOutfitWithCredit(pool)` (`:370-371`);
  `manual` → `wornBundles = selection.wornBundleIds?.length ? manualWornBundles(…, pool) : []`,
  slots `?? makeEmptyEquippedSlots()` (`:373-380`); `none` → `{empty, []}` (`:382-383`);
  `previous_chat` → copied slots carry `wornBundles: []` ("the bundle was credited
  in the source chat", `:392-393`), fallback default-with-credit (`:408`);
  `llm_choose` → `dissolveBundlesInSlotsWithCredit(result.result.slots, lookup)` (`:520-523`), fallback `:559`.
- Commit loop `:582-615`: `source = context?.source ?? 'chat-start'`; per selection
  `repos.wardrobeWear.commitEquippedOutfit({chatId, characterId, nextSlots, wornBundles, source})`;
  catch → `logger.error('[applyOutfitSelections] Failed to persist equipped outfit', {chatId, characterId, error})`.
- Callers: `app/api/v1/chats/route.ts:1370` `source: 'chat-start'`;
  `app/api/v1/chats/[id]/actions/participants.ts:222` `source: 'participant-added'`;
  `lib/chat/apply-chat-merge.ts:252-253` `source: 'merge'` ("A merge changes nobody's clothes").
- Jest `apply-outfit-selections.ledger.test.ts` `:109-198`: default credits the
  `isDefault` bundle it dissolved as `'chat-start'` (`:110-121`, slots top `['shirt']`);
  source passthrough (`:123-130`); merge still goes through the chokepoint
  marked `'merge'` with `wornBundles: []` (`:132-141`); previous_chat claims no
  bundle (`:143-152`); manual credits quick-picked bundles expanded server-side
  `[{id:'suit', leafIds:['shirt','slacks']}]` (`:154-168`); manual without ids claims
  nothing (`:170-179`); llm_choose credits the picked bundle, slots dissolved
  `top ['shirt'], bottom ['slacks']` (`:181-196`).

### 3.2 `lib/wardrobe/outfit-displacement.ts` (411 lines)

- `:67-81` `DisplacementRepos { chats: {getEquippedOutfitForCharacter}; wardrobeWear: {commitEquippedOutfit}; wardrobe? }` — the chats `setEquippedOutfit` slice is GONE from the primitives' contract.
- `:104` `PutOnSource = Extract<EquipSource, 'ui' | 'tool'>`; `:107` `WornBundle = {id, leafIds}`.
- `:114-124` `wornBundlesFor(item, itemsById, onlySlot?)`: `dissolveBundleToLeaves` null → `[]`;
  with `onlySlot`, only leaves whose `slots` include it; none → `[]`; else `[{id: item.id, leafIds}]`.
- `:126-138` `commit(…)`: `await wardrobeWear.commitEquippedOutfit({chatId, characterId, nextSlots, wornBundles, source})`;
  **returns `nextSlots`** (not the repo echo — "From the job child the result is synthetic").
- `:210-224` `equipItem(repos, chatId, characterId, item, opts?, source: PutOnSource = 'ui')`;
  `:231-245` `replaceItem(… source = 'ui')`; `:280-302` `addToSlot(… slot, item, opts?, source = 'ui')`
  → `wornBundlesFor(item, itemsById, slot)`; `:308-324` `removeFromSlot` → `commit(…, 'take-off', [])`.
- Pure variants unchanged (`wearItemIntoSlots :155-178`, `replaceItemIntoSlots :185-200`, `addItemToSlot :253-274`, `computeDisplacedSlots :346-411`).

### 3.3 `lib/wardrobe/dissolve-bundles.ts` (233 lines)

- `:170-175` `dissolveBundlesInSlots` now `= dissolveBundlesInSlotsWithCredit(...).slots`.
- `:182-233` `dissolveBundlesInSlotsWithCredit(currentSlots, itemsById)` → `{slots, wornBundles}`;
  `wornBundles = Array.from(dissolved, ([id, leaves]) => ({id, leafIds}))` — the
  `Map` insertion order (slot order, first-seen). Everything else byte-identical.

### 3.4 `lib/wardrobe/default-outfit.ts` (51 lines)

- `:31-33` `buildDefaultOutfit(items) = buildDefaultOutfitWithCredit(items).slots`;
  `:40-51` `…WithCredit` builds the sorted default slots then returns
  `dissolveBundlesInSlotsWithCredit(next, new Map(items))`.

### 3.5 `lib/wardrobe/staged-live-outfits.ts` (182 lines) — CLIENT staging; listed for completeness

`:38-41` `StagedGesture {mutate, wornBundleIds}`; `:48-53` `wornBundleIdsFor(item)`
(the item's own id iff it has components); `:56-65` `appendWornBundleIds` (dedupe,
first-seen); `:73-87` `rebaseStagedGestures`; `:93-104` `buildSetAllEquipBody`
omits `wornBundleIds` when empty; `:157-182` `classifyStagedOutfits(staged,
baselines, wornBundleIdsByChar={})` attaches `wornBundleIds` only when non-empty.
Consumed by the dialog (SPA survey).

### 3.6 `lib/wardrobe/item-route-steps.ts` — `cleanupEquippedRefs` `:72-99`

- Signature now `repos: { chats: {removeEquippedItemFromAllChats}; wardrobeWear: {deleteByItemIds} }`
  (every caller moved from `repos.chats` to `repos`).
- Step 1 unchanged (`:81-88`, WARN `` `${logTag} Cleanup of equipped references had issues, proceeding with delete` ``).
- Step 2 `:90-98`: `deleteByItemIds([itemId])`; DEBUG
  `` `${logTag} Dropped wear-ledger rows for deleted item` `` `{...meta, itemId}`;
  catch WARN **`` `${logTag} Cleanup of wear-ledger rows had issues, proceeding with delete` ``**
  `{...meta, ledgerError}`. `:63-70` "a composite's deletion drops only its own
  ledger rows, never its components'".
- (HEAD also carries `7c8572869`'s `imageChoiceError` `:33-48` and the
  `cleanupItemImages` re-export `:27` — out of scope.)

### 3.7 `app/api/v1/chats/[id]/actions/outfit.ts` (397 lines) — `set_all` + `wornBundleIds`

- `:57` `equipBodySchema.wornBundleIds: z.array(z.string().min(1)).optional()` (`:50-56` "A claim, not a fact").
- `:220-243` `resolveWornBundles(ctx, characterId, wornBundleIds, tiers)`:
  `Set` dedupe; `repos.wardrobe.findByIdsForCharacter(characterId, ids, tiers).filter(isBundle)`;
  per bundle `loadBundleLookup(repos, characterId, bundle.componentItemIds, tiers)` → `wornBundlesFor(bundle, lookup)`;
  when `result.length !== ids.length` DEBUG `[Chats v1] Some claimed worn bundles were not credited` `{characterId, claimed, resolved, context: 'wardrobe'}`.
- `:269-297` `set_all`: id validation unchanged (`:273-285`, 400 `` `Wardrobe item ${id} not available to this character` ``);
  `wornBundles = await resolveWornBundles(…, wornBundleIds ?? [], tiers)`;
  `commitEquippedOutfit({chatId, characterId, nextSlots: bodySlots!, wornBundles, source: 'ui'})`;
  `updatedSlots = bodySlots!`; INFO `[Chats v1] Equipped outfit replaced (set_all)`
  `{chatId, characterId, wornBundleCount, context: 'wardrobe'}` (the `wornBundleCount` key is NEW).
- **The failure shape MOVED**: pre-`3ee3`, `setEquippedOutfit → null` fell to
  `if (!updatedSlots) return serverError('Failed to update equipped slot')` (`:367-369`,
  still present but now unreachable for `set_all`); post-`3ee3` the chokepoint
  THROWS → the outer catch `:390-396` → **500 `Failed to equip wardrobe slot`**,
  and the avatar trigger + announcement are skipped. Pinned by `outfit.test.ts:294-304`
  (`commitEquippedOutfit` rejects → 500, `enqueueWardrobeOutfitAnnouncement` not called).
- `wear`/`equip`/`replace`/`add_to_slot` → the primitives with the default `'ui'`
  (`:305`, `:316`, `:332`); `remove_from_slot`/`clear_slot` → `removeFromSlot` (→ `'take-off'`).
- Jest `outfit.test.ts` adds (+95): set_all with `wornBundleIds: [SUIT, SHIRT, 'not-reachable']`
  → `source 'ui'`, `wornBundles [{SUIT, [SHIRT, SLACKS]}]`; without → `[]`;
  wearing a bundle claims it with its leaves and `nextSlots {top:[SHIRT], bottom:[SLACKS]}`;
  `clear_slot` → `{source: 'take-off', wornBundles: []}` (`:306-313`).

### 3.8 Tool handlers + definitions (HEAD lines; `b3f937076` drift noted)

- `lib/tools/handlers/wardrobe-wear-handler.ts:108, :112, :117` pass `'tool'` to
  `addToSlot` / `replaceItem` / `equipItem`. `wardrobe-create-handler.ts` hunk
  `@3ee3 :271` `equipItem(repos, chatId, targetCharacterId, newItem, tiers, 'tool')`.
- **`wardrobe-list-handler.ts`**: `:20-21` imports `formatRelativeDays`, `neverWornSummary`;
  `:118-119` ONE `repos.wardrobeWear.findSummaries(filteredItems.map(id))`
  (over the FILTERED list, after `include_equipped`/type/appropriateness filters);
  `:124` `wear = summaries.get(id) ?? neverWornSummary()`; `:145-146` result fields
  `wear_count: wear.wearCount`, `last_worn_at: wear.lastWornAt`; `:174` debug gains `neverWornCount`.
  `:204-212` **`formatWardrobeListWearNote(item, nowMs = Date.now())`**:
  `!wear_count || !last_worn_at` → `' · never worn'`; unparseable date → `' · never worn'`;
  else `` ` · last worn ${formatRelativeDays(lastMs, nowMs)}` ``.
  `:219-252` `formatWardrobeListResults(output, nowMs = Date.now())` — the line is
  `` `  ${typeTags} ${title}${equippedTag}${sharedTag}${appropriatenessTag}${compositeTag}${cueTag}${description}${wearTag}` ``
  at `3ee3`; HEAD `:249` appends `${pictureTag}` AFTER `${wearTag}` (`b3f937076`).
- **`wardrobe-read-handler.ts`**: `:67` `wear = await buildWardrobeReadWear(repos, characterId, item.id)`;
  `:87` `wear` on the output (so **`wardrobe_update`'s echo carries it too** — `:9-10` shared builder).
  `:97-127` `buildWardrobeReadWear`: `findHistory(itemId)` + `resolveWearers(history.wearers, repos)`
  (no avatars); DEBUG `Wardrobe read resolved wear history` `{context: 'wardrobe-read-handler', characterId, itemId, wearCount, wearerCount}`;
  wearers → `{character_id, name: resolved[i]?.name ?? '', is_you: id !== null && id === characterId, departed: kind !== 'character', wear_count, first_worn_at, last_worn_at}`.
  `:130-134` `wearerPhrase`: `is_you` → `'you'`; `departed` → `'someone no longer in the household'`; else name.
  `:137-141` `timesPhrase`: `once` / `twice` / `` `${n} times` ``.
  `:144-153` `wearDate(iso)`: `toLocaleDateString('en-GB', {day:'numeric', month:'short', year:'numeric', timeZone:'UTC'})` → `14 Mar 2026`; NaN → the raw iso.
  `:155-158` `relativeWearDate` → `formatRelativeDays` or raw iso. `:160-163` `joinPhrases` (`a, b and c`).
  `:170-188` **`formatWardrobeWearParagraph(wear, nowMs)`**: `!wear || wear_count===0 || wearers.length===0 || !last_worn_at` → `'Never worn.'`;
  `[latest, ...others] = wearers`; `last = `${relative(last_worn_at)} by ${phrase(latest)}``;
  head = count 1 ? `` `Worn once, ${last}.` `` : `` `Worn ${n} times, first ${wearDate(first_worn_at ?? last_worn_at)}, last ${last}.` ``;
  others → `` `${head} Also worn by ${join(others.map(w => `${phrase(w)} (${times(w.wear_count)})`))}.` ``.
  `:267-289` `formatWardrobeReadResults(output, nowMs)`: `:286` `` `  wear: ${paragraph}` `` is the LAST line (after `  equipped: …`). HEAD `:279` has `b3f937076`'s `  picture: …` line.
- **`wardrobe-list-tool.ts`**: `:81-84` `wear_count: number` ("0 = never worn"), `last_worn_at: string | null`.
  Description `@3ee3` (exact post-commit text, one string):
  `Retrieve wardrobe items available to the current character — from their own wardrobe plus shared items in the project and Quilltap General. Supports optional filtering by item type and appropriateness context. Each item includes its equipped status (which slot[s] it occupies, if any), a composite flag indicating whether it bundles other items, and an is_own flag (shared archetypes can be worn but not edited), and when it was last worn (or that it never has been). Use wardrobe_wear to put on / layer items, wardrobe_take_off to remove them, and wardrobe_read for the full detail (including the Portrait Cue) of one item. Archived items are excluded from results.`
  (Note v4's own grammar: "…an is_own flag (…), and when it was last worn…" — the
  stray comma is in the bytes.) HEAD `:117-128` reads `…flag (shared archetypes can be worn but not edited), when it was last worn (or that it never has been), and, when the item has a picture, its image_file_id — pass that to describe_image to see what the item looks like. Use wardrobe_wear…` (`b3f937076`).
- **`wardrobe-read-tool.ts`**: `:69-70` `wear?: WardrobeReadWearResult` ("absent on failure");
  `:77-89` `WardrobeReadWearerResult {character_id: string|null, name, is_you, departed, wear_count, first_worn_at: string, last_worn_at: string}`;
  `:94-99` `WardrobeReadWearResult {wear_count, first_worn_at: string|null, last_worn_at: string|null, wearers[]}`.
  Description `@3ee3`: `Read the full detail of ONE wardrobe item by id (preferred) or title. Returns everything wardrobe_list omits: the Portrait Cue (image_prompt), default-outfit membership, composite/replace behaviour, the full component list, archived status, whether you own it, the slots it is currently equipped in, and its wear history (how often it has been worn, when, and by whom). Items from your own wardrobe, the project, and Quilltap General all resolve.`
  HEAD `:108-116`: `…equipped in, its wear history (how often it has been worn, when, and by whom), and its picture's image_file_id when it has one (pass that to describe_image to see what it looks like). Items from…` (`b3f937076`).
- Jest `wardrobe-wear-readout.test.ts` (210 lines) — the exact lines:
  `:109` `findSummaries` called ONCE with `['item-1','item-2']`; `:110-113` items `[item-1, 4, THREE_DAYS_AGO]`, `[item-2, 0, null]`;
  `:116` `'  [top] Greatcoat · last worn 3 days ago'`; `:117` `'  [footwear] Spats · never worn'`;
  `:132-133` read lines end `'  equipped: no'` then `'  wear: Never worn.'`;
  `:148-150` `'  wear: Worn 4 times, first 14 Mar 2026, last 3 days ago by you. Also worn by Marguerite (once).'`;
  `:165-167` `'  wear: Worn 3 times, first 14 Mar 2026, last 2 weeks ago by Marguerite. Also worn by you (twice).'`;
  `:184-192` wearers `[[GONE, true, false], [MARGUERITE, false, false], [null, true, false]]` and
  `'  wear: Worn 5 times, first 14 Mar 2026, last 3 days ago by someone no longer in the household. Also worn by Marguerite (once) and someone no longer in the household (twice).'`;
  `:195-208` single wear → `'Worn once, 3 days ago by you.'`.

### 3.9 `lib/wardrobe/wear-display.ts` (156 lines) — CLIENT-SAFE; consumed only by `components/wardrobe/*`

`:27-29` `wearOf`, `:32-34` `isNeverWorn`; `:48-58` `formatWearLine` →
`Never worn` / `Worn once · last …` / `Worn 4× · last …` (count only when no date);
`:65-69` `formatWornWhen`; `:76-83` `WardrobeListSort` + labels; `:115-144` `sortWardrobeItems`
(`localeCompare` secondary, never-worn last under the two wear sorts); `:150-156` filter.
No server import (`git grep` → components only). Belongs to the SPA survey.

### 3.10 `lib/wardrobe/wear-history.ts` (180 lines) — the server read helpers

- `:37` `DEPARTED_WEARER_LABEL = 'a departed character'`; `:40` `UNATTRIBUTED_WEARER_LABEL = 'unattributed'`.
- `:50-65` `attachWear(items, repos)`: empty → `[]`; ONE `findSummaries(ids)`;
  DEBUG `Attached wear summaries to wardrobe read` `{itemCount, wornCount, context: 'wardrobe'}`;
  `{...item, wear: summaries.get(id) ?? neverWornSummary()}` — a read-time ANNOTATION,
  never a `WardrobeItemSchema` field, never accepted on write (`:7-11`).
- `:85-131` `resolveWearers(wearers, repos, {avatars?})`: null id → `{null, 'unattributed', null, 'unattributed'}`;
  `characters.findByIdRaw(id)` (RAW — "a broken vault costs a label, not a 500");
  throw → WARN `Could not read wearer; labelling as departed` `{characterId, error, context}`;
  no `name` → `{id, 'a departed character', null, 'departed'}`; avatars via
  `enrichWithDefaultImage(character.defaultImageId, repos)?.filepath` with WARN `Could not resolve wearer avatar`.
- `:147-180` `buildWearHistoryPayload(itemId, repos)` → `{history, wearers: [{characterId, name, avatarUrl}], lastWornChat: {id, title} | null}`;
  `chats.findById(lastWornChatId)` (missing/throw → null, WARN `Could not read last-worn chat` `{itemId, chatId, error, context}`);
  DEBUG `Built wear history` `{itemId, wearCount, wearerCount, lastWornChatResolved, context}`.

### 3.11 The read routes

- `app/api/v1/characters/[id]/wardrobe/route.ts`: `:24` import; `:107-110` the
  group-tier branch `attachWear(await findArchetypesInMountsAttributed(groups, includeArchived), repos)`;
  `:121-127` the own-vault branch `attachWear(withOrigin(findByCharacterId(id, includeArchived), {scope:'character', id, name}), repos)`.
- `app/api/v1/wardrobe/route.ts:65-68` General GET `attachWear(withOrigin(archetypeItems, GENERAL_WARDROBE_ORIGIN), repos)`.
- `lib/mount-index/mount-wardrobe-route-factory.ts`: `:41` import; `:240-243`
  collection GET `attachWear(withOrigin(readWardrobe(mp, includeArchived), origin), repos)`;
  `:292-295` the POST create's echo list ALSO `attachWear`ed; `:327-342` `findItem`
  (404 `ownerLabel` / `` `${ownerLabel} wardrobe item` ``); `:350-365` `handleGetWearHistory`
  → `buildWearHistoryPayload` + DEBUG `` `${logTag} Read ${owner} wardrobe item wear history` ``
  `{[logIdKey]: id, itemId, wearCount, context: 'wardrobe'}` → `successResponse(payload)`;
  `:368-379` item GET = `withActionDispatch({'wear-history': …}, default)`.
  DELETE `:426` `cleanupEquippedRefs(repos, …)`.
- `app/api/v1/characters/[id]/wardrobe/[itemId]/route.ts`: `:27-34` character +
  item 404s run BEFORE `:36-52` `dispatchAction(req, {'wear-history': …}, default)`;
  DEBUG `[Wardrobe v1] Read wardrobe item wear history` `{characterId, itemId, wearCount}`;
  DELETE `:117` `cleanupEquippedRefs(repos, itemId, '[Wardrobe v1]', {characterId: id, itemId})`.
- `app/api/v1/wardrobe/[itemId]/route.ts`: `:30-46` same shape, DEBUG
  `[Wardrobe Archetypes v1] Read archetype item wear history` `{itemId, wearCount}`;
  DELETE `:99` `cleanupEquippedRefs(repos, itemId, '[Wardrobe Archetypes v1]', {itemId})`.
- Jest `wear-ledger-routes.test.ts` (219 lines): `:137-150` character collection
  carries `wear` beside `origin` from ONE `findSummaries(['item-1','item-2'])`;
  `:152-160` General too; `:173-183` character item `?action=wear-history` body
  `EXPECTED` with `findByIdRaw(GONE)` and `chats.findById(CHAT_ID)` called;
  `:185-195` General item — deleted chat → `lastWornChat: null`;
  **`:197-206` 404 BEFORE reading the ledger when the item is not in the tier**;
  `:209-217` no action → the item itself.
- `__tests__/unit/app/api/v1/groups/[id]/wardrobe/route.test.ts` (+10) and
  `characters/[id]/wardrobe/[itemId]/route.test.ts` (+5): the mocks gain `wardrobeWear`.

### 3.12 Lifecycle

- **Cascade** `lib/cascade-delete.ts:404-413`: after plugin data, BEFORE
  `repos.characters.delete` (`:416`): try `foldWearerIntoUnattributed(characterId)`
  → DEBUG `[CascadeDelete] Folded wear-ledger rows into unattributed` `{context: {characterId}}`;
  catch ERROR `` `Failed to fold wear-ledger rows for character ${characterId}` `` `{context: {characterId}}, err`.
  Jest `cascade-delete.test.ts` (+66): order `['fold', 'delete-character']`; a
  rejected fold (`'ledger busy'`) still deletes the character, `success: true`.
- **Transfers** `app/api/v1/wardrobe/transfers/route.ts:13-18` — a DOC COMMENT
  only, no code: a move keeps the id so the tally follows (the source-side
  delete "goes straight to the store, not through `cleanupEquippedRefs`"); a copy
  mints a fresh id → empty ledger. Jest `transfers/route.test.ts` (+71) pins both.
- **Item delete**: the four routes (§3.6, §3.11) via `cleanupEquippedRefs(repos, …)`.
- **Chat delete**: untouched — `lastWornChatId` dangles by design
  (`wardrobe-wear.types.ts:33` "a deleted chat leaves a dangling id the readers
  treat as 'a chat since deleted'").

### 3.13 Types, parsers, format-time, the child proxy

- `lib/schemas/wardrobe-wear.types.ts` (77 lines): `:24-37` `WardrobeWearStatsRowSchema`
  (`id: UUIDSchema`, `itemId: z.string().min(1)`, `wearerCharacterId: z.string().nullable()`,
  `wearCount: z.number().int().nonnegative()`, `firstWornAt/lastWornAt: TimestampSchema`,
  `lastWornChatId: z.string().nullable()`, `createdAt/updatedAt`); `:46-51`
  `WardrobeWearSummarySchema {wearCount, firstWornAt|null, lastWornAt|null, lastWornChatId: UUID|null}`;
  `:56-63` `WardrobeWearerSchema` (`characterId: UUID|null`, `wearCount: positive`);
  `:68-70` `WardrobeWearHistorySchema = Summary.extend({wearers})`; `:75-77`
  `neverWornSummary() = {wearCount: 0, firstWornAt: null, lastWornAt: null, lastWornChatId: null}`.
- `lib/schemas/wardrobe.types.ts:258` `OutfitSelectionSchema.wornBundleIds: z.array(z.string().min(1)).optional()`
  (`:253-257` "'manual' only … Validated and expanded server-side"). HEAD also has `7c8572869`'s `imageFileId` lines — out of scope.
- `lib/database/repositories/vault-overlay/parsers.ts`: `:238-246`
  `resolveWardrobeItemId(frontmatterId, mountPointId, relativePath)` = the old
  inline rule (`/^[0-9a-f-]{36}$/i` → id, else `stableUuidFromString(`wardrobe-item:${mp}:${path}`)`),
  now called at `:346`; `:254-261` `isWardrobeItemDocumentPath(relativePath)` (a
  `.md` DIRECTLY under `Wardrobe/`, case-insensitive, not `instructions.md`);
  `:267-277` `wardrobeItemIdForDocument({mountPointId, relativePath, content})`
  (frontmatter `id` alone). The latter two exist for the `.qtap` carriers (other survey).
- `lib/format-time.ts:184-196` **`formatRelativeDays(ts, nowMs = Date.now())`** —
  `daysOld = max(0, (nowMs - ts)/86400000)`; `<1 today`, `<2 yesterday`,
  `<7 "${floor} days ago"`, `<14 last week`, `<30 "${floor(/7)} weeks ago"`,
  `<60 last month`, `<365 "${floor(/30)} months ago"`, else `"${years} year${s} ago"`.
- `lib/memory/memory-weighting.ts:12` imports it; `:164-167` `formatRelativeAge(memory, now)`
  = `formatRelativeDays(eventReferenceTimeMs(occurredAt, referenceTimeMs(memory)), now.getTime())`.
  **Byte-identical behaviour confirmed from the hunk**: the removed body had the
  same eight branches, the same `Math.max(0, …)`, the same `Math.floor`s, and
  `${Math.floor(daysOld/365)} year${Math.floor(daysOld/365) > 1 ? 's' : ''} ago`
  is exactly the new `years` variable form. No threshold moved.
- `lib/background-jobs/child/child-repositories-proxy.ts:243-254` (`@3ee3 :195-207`):
  `METHOD_OVERRIDES['wardrobeWear.commitEquippedOutfit'] = 'write'` ("Buffered whole
  so the parent replays it on its RW connection, where the prior state is true")
  and `['wardrobeWear.foldWearerIntoUnattributed'] = 'write'` ("Not reached from a
  job today"). HEAD additionally carries `039f7017c`'s bug-179 read-your-writes
  overlay (`:181-202` `recordBufferedOutfitWrite` / `bufferedOutfitFor`, `:481-484`
  the `chats.getEquippedOutfitForCharacter` intercept). **v5 has no child
  process** — its job runner is in-process and every write goes through the
  single writer, so the chokepoint's diff is always against the true state and
  neither the buffering nor bug 179 has an analog (the P4.159 survey already
  drafted bug 179 as a v4 filing). The fence test
  (`equip-chokepoint-fence.test.ts:18-23`) sanctions exactly two files
  (`chats.repository.ts` the definition, `wardrobe-wear.repository.ts` the
  chokepoint) and asserts the proxy override (`:47-53`).

### 3.14 Docs/help touched (vendoring list, not behaviour)

`help/wardrobe.md` (+25, "## The Ledger" + the two tool bullets), `help/project-wardrobe.md`
(+2), `docs/developer/API.md` (+28), `docs/developer/DDL.md` (+59),
`docs/developer/BACKGROUND_JOBS_CHILD.md` (+2), `docs/developer/bugs.md` (bug 179
filed), `docs/developer/features/complete/wardrobe-wear-ledger.md` (the design of
record, moved), `public/schemas/qtap-export.schema.json` (+25, carriers survey).
v5 mirrors: `help/wardrobe.md` (repo root, embedded — `help_tree_embed_guard`),
`docs/v4/developer/{API,DDL,BACKGROUND_JOBS_CHILD}.md`.

---

## 4. What the commit did NOT do

- **Did not change the slot writer.** `chats.repository.ts` `getEquippedOutfit :772`,
  `getEquippedOutfitForCharacter :797`, `setEquippedOutfit :815-838` (still
  `existing ?? {}` → `state[characterId] = slots` → `update` → returns `slots`,
  `safeQuery` fallback `null`), `removeEquippedItemFromAllChats :844` — untouched.
  Slot BYTES are unchanged on every path.
- **Did not make the chokepoint atomic with the slot write** (`:214-221`); only the
  credits are one transaction.
- **Did not register the Zod schema anywhere for DDL beyond the repository
  constructor** — no `SQLITE_TABLES` entry in `sqlite-initial-schema.ts`; the
  table on every real v4 instance comes from the MIGRATION.
- **Did not give the seed its own once-only gate**; it relies on the runner's
  `migrations_state` check. `shouldRun` is TRUE on any instance with the table
  and the two chat columns.
- **Did not add an ORDER BY to the seed's chat scan** (rowid order decides
  `lastWornChatId` on equal `updatedAt`).
- No FK from the ledger to chats or characters (items are files; `:26,:33`);
  no pruning of orphan item ids; chat delete, item ARCHIVE, item UPDATE/rename
  and `wardrobe_take_off` leave the ledger alone (take-off commits `'take-off'`,
  which can credit nothing).
- **Transfers: no code**, only the header comment; `deleteFromSource` still
  bypasses `cleanupEquippedRefs`.
- Did not add `wear_count`/`last_worn_at` to `wardrobe_wear`, `wardrobe_take_off`,
  `wardrobe_create` or `wardrobe_archive` outputs; `wardrobe_update`'s echo gains
  `wear` ONLY by sharing `buildWardrobeReadOutput` (verify whether
  `formatWardrobeUpdateResults` prints it — not read here).
- Did not put `wear` on `WardrobeItemSchema`; it is a response annotation only.
- Did not touch `applyOutfitForAddedParticipant`'s fallback logic beyond `source`.
- Did not fix bug 179 (filed; fixed by `039f7017c` in the child proxy).
- Did not touch `removeEquippedItemFromAllChats`' per-slot `.filter` or its count.
- Did not change `allEquippedItemIds` / `normalizeEquippedSlots` (both pre-existing,
  `wardrobe.types.ts:291-316`).

---

## 5. v5 main (file:line facts)

### 5.1 The slot writer and its FIVE caller sites (the future chokepoint's clients)

- `crates/quilltap-core/src/db/chats_outfits.rs`: `get_equipped_outfit :123`,
  `get_equipped_outfit_for_character :133`, **`set_equipped_outfit :152-185`**
  (`Result<bool, DbError>`; `Ok(false)` when the chat row is missing — "v4's
  `update` is a no-op"; normalizes the OTHER characters' bags via
  `normalize_equipped_outfit_state`, stores the caller's `slots` raw; `ChatUpdate
  {equipped_outfit}` through `ChatsRepository::update`), `remove_equipped_item_from_all_chats :190`.
- `grep -rn set_equipped_outfit crates/` (non-harness) → exactly five writers:
  1. `tools/wardrobe_shared.rs:413-425` `persist(main, chat_id, character_id, &Slots)` →
     `:421` — used by `equip_item :318-343`, `replace_item :345-369`, `add_to_slot :371-397`,
     `remove_from_slot :399-411` (the displacement primitives; module comment `:196-200`
     "v4's `result ?? next`"). Callers of those: `api/chat_outfits.rs:44` (wear/replace/add/remove/clear arms),
     `tools/wardrobe_wear.rs:296/:310/:324` (`apply_op :244`), `tools/wardrobe_create.rs:354-357`
     (`equip_now`), `tools/wardrobe_take_off.rs:174/:294`.
  2. `api/chat_outfits.rs:564` — the `set_all` arm (`:520-573`).
  3. `services/chat_participants.rs:1457` — `apply_outfit_for_added_participant :1323-…`
     (its own five-mode re-implementation; `previous_chat` degenerates to `default`, `:1309`).
  4. `services/outfit_selections.rs:652` — `apply_outfit_selections :611-663` (phase 2 commit loop,
     ERROR `[applyOutfitSelections] Failed to persist equipped outfit`).
  5. `services/outfit_selections.rs:763` — `apply_outfit_selection_sync :751-765`
     (called by `services/chat_merge.rs:466` inside the writer, `:457-467`).
- Readers (unchanged by the port, listed so the chokepoint survey is complete):
  `tools/wardrobe_archive.rs:129`, `wardrobe_create.rs:382`, `wardrobe_list.rs:175`,
  `wardrobe_read.rs:131`, `wardrobe_shared.rs:191` (`load_current_wardrobe_state :185-193`),
  `api/chat_cast.rs:179`, `services/story_background_job.rs:1373`,
  `services/chat_create.rs:2307`, `services/outfit_selections.rs:733` (previous_chat),
  `services/build_context.rs:1329`, `services/character_avatar_job.rs:280`.

### 5.2 `services/outfit_selections.rs` (the v4 `apply-outfit-selections.ts` port, P4.D39)

- `OutfitSelection :99-112` (`character_id`, `mode`, `slots: Option<Slots>` — **no `worn_bundle_ids`**).
- `OutfitContext :114-128` (`user_id`, `project_mount_point_ids`, `scenario_text`,
  `cheap_settings`, `source_chat_id` — **no `source`**).
- `slots_to_value :136`; **`default_outfit_from_pool :157-184`** (sorted defaults →
  `dissolve_bundles_in_slots` — returns `Slots` only, no credit); `lookup_from :187`;
  `resolve_default_outfit :204-216`.
- `apply_outfit_selections :611-663`; `resolve_selection :671`; `resolve_non_llm_selection :715-748`
  (`manual :726` passes `selection.slots` through; `previous_chat :728-745`);
  `apply_outfit_selection_sync :751-765`; `resolve_llm_choose :1167` → `dissolve_bundles_in_slots :1225`;
  `run_llm_choose_via_db :1106` → `:1157`.
- Call sites: `services/chat_create.rs:1526-1532` builds `OutfitContext` (no source) and `:1537` calls;
  `services/chat_merge.rs:458-466`.

### 5.3 The pure wardrobe layer

- `dissolve_bundles.rs`: `WearableNode :41`, `DissolvedLeaf :75`, `dissolve_bundle_to_leaves :115`,
  `lay_leaves_into_slots :179`, **`dissolve_bundles_in_slots :228-…`** — it ALREADY builds the
  insertion-ordered `dissolved: Vec<(String, Vec<DissolvedLeaf>)>` (`:231`) that v4's
  `wornBundles` is derived from; only the return type lacks it.
- `wardrobe.rs`: `sort_for_default_outfit :381`, `wear_item_into_slots :679`,
  `replace_item_into_slots :703`, `add_item_to_slot :723`.
- `memory_weighting.rs:222-245` `format_relative_age(m: &MemoryInputs, now_ms: f64)` —
  the ladder is INLINE over `days_old`; there is no `format_relative_days(ts_ms, now_ms)`
  primitive to share with the tools. Phase-1 pinned (`QT_ORACLE_WEIGHTING`, `memory-weighting.ts` case).
- `vault_overlay.rs:1077-…` `parse_wardrobe_item_file`; the id rule inline at `:1110-1115`
  (`is_wardrobe_id_shaped` → else `stable_uuid_from_string("wardrobe-item:{mp}:{path}")`);
  `stable_uuid_from_string :54`. No `is_wardrobe_item_document_path` / `wardrobe_item_id_for_document`.

### 5.4 Tools

- `tools/wardrobe_list.rs`: `WardrobeListItemResult :23-40` — **no `wear_count`/`last_worn_at`**
  (and no `image_file_id` either — `b3f937076` is also pending); `run :153` (equipped read `:175`);
  `format :274-343` — the line at `:332-342` is `type_tags, title, equipped_tag, shared_tag,
  appropriateness_tag, composite_tag, cue_tag, description` (v4 `3ee3` appends `wearTag`; HEAD then `pictureTag`).
- `tools/wardrobe_read.rs`: `WardrobeReadToolOutput :28-47` (no `wear`); `build_read_failure :76`;
  `build_read_output :100-160` (equipped `:131-133`); `format :259-315` — `  equipped: …` is the
  LAST line (`:305-313`); v4 appends `  wear: …` after it.
- `tools/wardrobe_wear.rs` `apply_op :244-…` → `add_to_slot :296`, `replace_item :310`, `equip_item :324`;
  `tools/wardrobe_create.rs:354-357` `equip_now`; `tools/wardrobe_take_off.rs:174,:294`;
  `tools/wardrobe_shared.rs` (`find_equipped_slots :161`, `equip_item…remove_from_slot :318-411`).
- **Tool descriptions live in `tools/definitions/data.rs`** (GENERATED — header `:1-8`:
  "byte-exact … `JSON.stringify({ name, description, parameters })` from v4 …
  Regenerate via `harness/oracle/tools/gen-tool-catalog.mjs`"); `wardrobe_list :276-278`,
  `wardrobe_read :281-283` carry the PRE-`3ee3` descriptions. Pinned by
  `crates/quilltap-harness/tests/tool_definitions_equivalence.rs`
  (oracle cases `harness/oracle/cases/tool-definitions.ts`, `tool-definitions-canonical.ts`).

### 5.5 API arms

- `api/chat_outfits.rs`: `EquipBody :275-284` (`character_id, mode, slot, item_id, slots` —
  **no `worn_bundle_ids`**; `parse_equip_body :373-480` strips unknown keys, so a client's
  `wornBundleIds` is silently dropped today); `set_all :520-573` → `set_equipped_outfit :564`;
  `.is_err()` → **`internal("Failed to update equipped slot")`** `:566-568` (v4 post-`3ee3`:
  `Failed to equip wardrobe slot`, §3.7); `wear`/`equip`/`replace :574-621` → `Err` →
  `internal("Failed to update equipped slot")` `:616-620` (a path v4's pre-`3ee3` safeQuery
  never reached; post-`3ee3` it throws → `Failed to equip wardrobe slot`); `add_to_slot :622…`;
  then the avatar trigger + announcement legs.
- `api/wardrobe.rs`: `wardrobe_list :228-240` (`Response::Wardrobe({wardrobeItems})` — the
  `attachWear` seam); `wardrobe_item_get :302`; `wardrobe_delete :413-445`
  (`:431` `let _ = remove_equipped_item_from_all_chats` — warn-and-proceed shape).
- `api/characters.rs`: `character_wardrobe_list :846`; `character_wardrobe_get :2751`;
  `character_wardrobe_delete :2891-2925` — **`:2917` uses `?`** (a `DbError` FAILS the
  delete) where v4's `cleanupEquippedRefs` warns and proceeds; pre-existing, and the
  new ledger drop must be warn-and-proceed on all four routes.
- `api/groups.rs`: `group_wardrobe_list :1212`, `_get :1302`, `_delete :1400-1424` (`:1408 let _ =`).
- `api/projects.rs`: `project_wardrobe_list :1487`, `_get :1614`, `_delete :1720-1745` (`:1729 let _ =`).
- No `wear`/`wear-history`/`wardrobe_wear_stats` anywhere in `crates/quilltap-core/src`,
  `crates/quilltap-web/src`, `apps/web/src` (grep, excluding the `wardrobe_wear` TOOL).
- Web transport: `crates/quilltap-web/src/wardrobe_routes.rs:190-198` `wardrobe_item_get`
  → `CoreRequest::WardrobeItemGet { item_id }` — **no `?action` dispatch on the item GET**
  (the collection GET `:111-122` dispatches `["instructions"]` via `crate::query::dispatch_action`);
  router `lib.rs:492`. The character/group/project item GETs will need the same `wear-history` arm.
- `services/cascade_delete.rs`: `execute_cascade_delete :307-388`; plugin data `:378-382`;
  `CharactersRepository::new(main).delete :385` — the fold slot is between them.
- `services/wardrobe_transfers.rs`: `:24-25` move keeps id, copy mints; `delete_from_source :689-716`
  calls the store writers directly (never `remove_equipped_item_from_all_chats`) — already
  v4's shape, so only a tier-2 row is owed (rows survive a move; a copy starts empty).

### 5.6 Provisioning artifacts + the boot-ensure PATTERN

- `services/provisioning/mod.rs`: `FRESH_SCHEMA_JSON :170` (`include_str!("fresh_schema.json")`),
  `MIGRATION_INDEXES_JSON :230`, `migration_index_family() :246`, `provision_fresh_instance :335-380`
  (`exec_ddl(main, &schema.main, &migration_indexes.main) :353`, then mount-index `:364`, llm-logs `:375`).
- `fresh_schema.json` format: `{"main": ["CREATE TABLE …", …, "CREATE INDEX …"], "mountIndex": […], "llmLogs": […]}`
  — one statement per string, tables sorted by name then indexes sorted by name, verbatim
  `sqlite_master.sql` (no `IF NOT EXISTS`). `migration_indexes.json` (74 lines) adds a
  `source` block `{dumper, v4Commit, v4Version, note: "Generated - never hand-edit (D23)…"}`
  then the same per-partition arrays. The sixth re-dump + the index re-dump are the D23
  moves for this table (the ledger §1 already says so).
- `seed_built_ins` — `crates/quilltap-host/src/host.rs:1248` (called `:563`); each ensure is
  a `// === P4.x (v4 `sha`, migration `id`) === … // === end ===` block; the LAST step is
  `db::migration_index_family_repair` (`migration_index_family_repair.rs:75` "runs LAST").
- **The table-CREATE precedent**: `db/chat_informs.rs:104-117` `CHAT_INFORMS_TABLE_DDL`
  (`CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` in ONE `execute_batch`,
  generateDDL shape) behind `ensure_chat_informs_table :127-129`, then the column ensure
  `db/chat_informs_permanent_repair.rs:61-76` (table gate → column gate → ALTER; module
  doc `:1-47` = v4 commit + migration id + `dependsOn`, the two-shape measurement, the
  NO-PORT list). **The multi-shape ensure precedent**:
  `db/chat_settings_impersonation_voice_mode_repair.rs` (doc `:1-80`: shapes measured,
  v4's predicate verbatim, the one-transaction recorded divergence, NO-PORT of the
  migration's INFO/DEBUG/ERROR/ledger/pretty label; exports `VoiceModeEnsureOutcome`).
- **The ledger-stamping precedent (the cross-app handshake)**: `db/migrations_ledger.rs:30`
  `ensure_migrations_tables` (v4's `migrations_state` + `migrations_metadata` DDL); six heals
  stamp v4's row with `recordCompletedMigration`'s shape (`avatar_rolls_collapse_heal.rs:701-730`
  `stamp(...)`), and SKIP when v4 already wrote the row
  (`thinking_prefill_retire_heal.rs:335-357` test `a_v4_written_ledger_row_makes_the_heal_skip`
  → `RetireOutcome::AlreadyCompleted`). This is the pattern the SEED ensure must follow — see §7.2.

---

## 6. Harness families + recipes + the ensure pattern

### 6.1 Families touching outfits / wardrobe / the wardrobe tools (`grep -l -iE 'outfit|wardrobe' tests/*.rs`, 72 files)

Directly moved by this commit (regen at the new pin, fixtures must gain the table — §7.4):
- `chats_outfits_tier2_equivalence.rs` — the four slot ops incl. `setEquippedOutfit` /
  `removeEquippedItemFromAllChats` (case `harness/oracle/cases/chats-outfits-tier2.ts:3-8, :91-94`).
  Recipe `:32-38`: `QT_FIXTURE_OUT=/tmp/qt-choutfit-fixture.db $N/npx tsx …/fixtures/build-chats-outfits-fixture.ts`;
  `QT_FIXTURE_CHOUTFIT=… npx tsx …/cases/chats-outfits-tier2.ts > /tmp/oracle-choutfit.ndjson`;
  run `QT_ORACLE_CHOUTFIT` + `QT_FIXTURE_CHOUTFIT`. Fixture spec `fixtures/chats-outfits-tier2.json`.
- `wardrobe_tools_equivalence.rs` — all seven tools' Output + `format*` text (case
  `cases/wardrobe-tools.ts:98-140`). Recipe `:26-32`: `QT_FIXTURE_WT_MAIN`/`QT_FIXTURE_WT_MOUNT`
  via `fixtures/build-wardrobe-tools-fixture.ts` (spec `fixtures/wardrobe-tools.json`);
  `node --import tsx …/cases/wardrobe-tools.ts > /tmp/oracle-wardrobe-tools.ndjson`; run `QT_ORACLE_WT`.
  **Every `wardrobe_list` line gains ` · never worn` and every `wardrobe_read` gains `  wear: Never worn.`
  the moment the pin moves** (no ledger rows in the fixture) — plus the equip ops will THROW
  on a fixture lacking the table (§7.4).
- `wardrobe_tools_avatar_trigger_equivalence.rs` (`QT_WT_AVATAR_SPEC`, `QT_FIXTURE_WTA_*`, `QT_ORACLE_WTA`, `:37-45`).
- `chat_cast_routes_equivalence.rs` — add-participant → `set_equipped_outfit` (jest case
  `cases/chat-cast-routes.test.ts`, `QT_ORACLE_CHAT_CAST`, fixture `fixtures/chat-cast.json`, `:67-69, :149`).
- `chat_create_capstone_equivalence.rs` — chat-start outfits (`QT_FIXTURE_CC_MAIN/MOUNT/LLM`,
  `build-chat-create-capstone.ts`, jest `chat-create-capstone.test.ts`, `QT_ORACLE_CC`, `:50-61`).
- `outfit_llm_choose_tier3_equivalence.rs` (`QT_ORACLE_LLM_CHOOSE`, jest `chat-dialogs-llm-choose-tier3`, fixture `chat-dialogs-web.json`, `:19-21, :138`).
- `dissolve_bundles_equivalence.rs` (`QT_ORACLE_DISSOLVE`, case `cases/dissolve-bundles.ts`, `:14-17`) — add `…WithCredit` rows.
- `tool_definitions_equivalence.rs` — the two descriptions move.
- `characters_reads_equivalence.rs`, `wardrobe_public_read_equivalence.rs` (`QT_FIXTURE_WPR_*`, `QT_ORACLE_WPR`, spec `wardrobe-public-read-tier2.json`, `:23-29`),
  `group_wardrobe_routes_equivalence.rs` (jest `group-wardrobe.test.ts` over the transfers fixture, `:24-32`),
  `projects_routes_equivalence.rs` — the collection GETs gain the `wear` annotation.
- `wardrobe_transfers_tier2_equivalence.rs` (jest `wardrobe-transfers.test.ts`, `QT_FIXTURE_WTR_*`, `:47-55`) — the move/copy ledger rows.
- `characters_mutations_equivalence.rs` (cascade delete; `grep -i cascade`) — the fold.
- `vault_wardrobe_item_file_equivalence.rs` (`QT_ORACLE_VAULT_WARDROBE_ITEM_FILE`, `:15-18`) — `resolveWardrobeItemId` is a no-op extraction; add `isWardrobeItemDocumentPath`/`wardrobeItemIdForDocument` rows if the core exposes them.
- `provisioning_equivalence.rs` (1a/1b/1c/1d; recipe `:30-50`, needs `QT_FRESH_SCHEMA_LIVE` from the PIN) and
  `migration_index_backfill_equivalence.rs` (`QT_ORACLE_PROVISION`, `QT_ORACLE_INDEX_BACKFILL`, `QT_V5_BACKFILLED_OUT`, `:34-46`) — move with the two re-dumps.
- Unaffected but adjacent: `wardrobe_tier2_equivalence` (`QT_FIXTURE_WARDROBE`, `QT_ORACLE_WARDROBE`, spec `wardrobe-tier2.json`),
  `outfit_hash_equivalence` (`QT_ORACLE_OUTFIT_HASH`), `generated_wardrobe_items_equivalence`,
  `vault_wardrobe_{read,write,emit,public}_equivalence`, `vault_legacy_wardrobe_equivalence`,
  `vault_component_leaves_equivalence`, `wardrobe_instructions_{routes,tier2}_equivalence`,
  `outfit_instructions_wiring_guard`, `archived_wearer_read_guard`, `tiered_mount_pool_equivalence`,
  `almanack_tier2_equivalence`, `avatar_job_tier3_equivalence`, `image_generation_tier3_equivalence`,
  `photo_tools_equivalence`, `post_office_*`, `pascal_*`, `search_tools_equivalence`, `state_sql_tools_equivalence`,
  `system_{backup,restore,import,delete_data}_*` + `backup_uuid_remap_equivalence` + `qtap_import_equivalence` +
  `restore_vintage_state` (the carriers — other survey), `memory_weighting` (tier-1, must NOT move).

### 6.2 The tier-2 real-DB ENSURE differential pattern (P4.D251 / P4.D249)

- **`chat_settings_voice_mode_ensure_equivalence.rs`** (header `:1-45`): both sides start from
  the SAME base file per mode, built by the oracle case at the pin from v4's own generateDDL
  with the moved line removed; the oracle runs v4's REAL migration modules through
  **`harness/oracle/lib/v4-migrations.ts`** on a copy; the Rust test runs the ensure on its
  own copy. Three byte-exact comparands per mode: `PRAGMA table_info` (name/type/notnull/
  dflt/pk, order included), the table's `sqlite_master.sql`, every row's relevant cells;
  v4's migration report recorded but asserted only for having RUN (no vacuous mode).
  Recipe: `QT_FIXTURE_OUT_DIR=/tmp/qt-voice-mode-ensure $N/npx tsx $V5W/harness/oracle/cases/chat-settings-voice-mode-ensure.ts > /tmp/oracle-voice-mode-ensure.ndjson`;
  run `QT_ORACLE_VOICE_MODE_ENSURE` + `QT_FIXTURE_VOICE_MODE_ENSURE_DIR`. Pepper from `fixtures/chat-settings-tier2.json` (`:56-66`).
- **`chat_informs_permanent_ensure_equivalence.rs`** (`:1-40`): FOUR shapes — baseline / NO TABLE
  (v4 `not needed`, neither side creates) / already migrated / generateDDL-current — with
  `+RAN` asserted per mode; case `cases/chat-informs-permanent-ensure.ts`, env
  `QT_ORACLE_INFORM_ENSURE` + `QT_FIXTURE_INFORM_ENSURE_DIR`.
- For the wear ledger the natural modes: (a) NO table, chats with outfits (the Friday
  shape) → v4 runs BOTH migrations; (b) table present (generateDDL shape, as a v5-fresh
  instance has it), no ledger rows, chats with outfits → v4's seed RUNS (`shouldRun` true);
  (c) table + both `migrations_state` rows → nothing; (d) the seed corpus: legacy
  whole-composite ids, the same id in two slots, `'not json'`, `'[]'`, `null`, an `''`
  character key, two chats sharing an item with equal/unequal `updatedAt` (rowid tie-break).
  Comparands: `table_info` + `sqlite_master.sql` for the table AND both indexes + every
  row minus `id`/`createdAt`/`updatedAt` (minted), plus the `migrations_state` rows (minus
  `completedAt`/`quilltapVersion`).

---

## 7. Premise corrections / surprises

1. **"Registered as a repository, so generateDDL creates it" is true — and it creates a
   DIFFERENT table than the migration.** Zod → `"wearCount" REAL NOT NULL` (no `DEFAULT 0`),
   plus an auto `idx_wardrobe_wear_stats_createdAt`, and NONE of the two hand indexes
   (§1.5). A real v4 boot never sees that shape (migrations run first), but the SIXTH D23
   re-dump will carry it, so a v5-FRESH instance gets REAL affinity while every migrated
   instance has INTEGER. `provisioning_equivalence` (1b) compares column NAMES only — this
   asymmetry is invisible to the tripwire. v5's reads of `wearCount` must be type-tolerant
   (`1.0` vs `1`); and the UNIQUE COALESCE index is the `ON CONFLICT` target, so until the
   `migration_indexes.json` re-dump lands in the same round, the increment SQL on a
   v5-fresh instance is a hard SQLite error ("ON CONFLICT clause does not match any PRIMARY
   KEY or UNIQUE constraint"). The boot ensure must create BOTH indexes itself (the
   index-family repair runs LAST, after any seed would already have run). **A ruling is
   owed**: keep the generateDDL text per R-A (and read tolerantly), or add this table to
   the `idx_doc_mount_folders_mp_path`-style carve-out so a fresh v5 instance gets the
   migration's INTEGER text.
2. **Double-seed hazard on a shared instance (the §R.13 class).** v4's runner checks
   `migrations_state` BEFORE `shouldRun` (`migrations/index.ts:144-147, :164`); the seed's
   `shouldRun` is TRUE whenever the table and the two chat columns exist. If v5 creates and
   seeds the table on Friday WITHOUT stamping `add-wardrobe-wear-stats-table-v1` AND
   `seed-wardrobe-wear-stats-v1`, v4's next boot re-seeds — every current outfit credited
   twice. The ensure must stamp both rows with the heals' `recordCompletedMigration` shape
   and honour a v4-written row (skip), exactly as `thinking_prefill_retire_heal.rs` does.
   The mirror case: a v5-fresh instance (table present, unseeded, no ledger row) that v4
   boots → v4 seeds it; v5's ensure must then see v4's row and not seed again.
3. **The `set_all` failure message moves** from v5's pinned `Failed to update equipped
   slot` (`chat_outfits.rs:566-568`) to `Failed to equip wardrobe slot` (the throw reaches
   the route's catch), with the avatar trigger and announcement skipped; the primitives'
   `Err` arms (`:616-620`) move the same way.
4. **HEAD ≠ the commit on the tool surfaces**: `b3f937076` appended `pictureTag` after
   `wearTag`, added `image_file_id`, and rewrote both descriptions again; `7c8572869` changed
   `item-route-steps.ts` and the item routes. A lane pinned at `3ee3b1342` must port the
   `3ee3` bytes (§3.8 quotes them) and leave the picture clauses to the #82 lane — or the
   round must land #81 and #82 in order.
5. **`characters.rs:2917` fails the delete on a scrub error (`?`)** where v4 and the other
   three v5 routes warn-and-proceed; the new `deleteByItemIds` step must be
   warn-and-proceed on all four, and that `?` is a pre-existing divergence to fold in.
6. **No child process in v5** → the `METHOD_OVERRIDES` buffering, the synthetic return,
   bug 179 and `039f7017c`'s overlay have no analog. The fence test's INTENT does: a
   census guard over `set_equipped_outfit(` with ONE sanctioned caller (the existing
   `*_sites_guard` / `*_census` families are the shape).
7. **The credit lists already exist in v5**: `dissolve_bundles_in_slots` builds the
   insertion-ordered `dissolved` Vec (`:231`) v4's `wornBundles` is made from, and
   `default_outfit_from_pool` composes over it — both are return-type widenings, not new logic.
8. **`formatRelativeAge` → `formatRelativeDays` is byte-identical** (same eight branches,
   floors and pluralization); the Phase-1 `memory_weighting` pin must NOT move. v5 needs a
   shared `format_relative_days(ts_ms, now_ms)` extracted from `memory_weighting.rs:222-245`
   for the two tool formatters (which take an injectable `nowMs` in v4 — the tier-2 case
   must pin the clock).
9. **Transfers need no code** (v4's hunk is a comment; v5's `delete_from_source` already
   bypasses the scrub) — one tier-2 row proves the rows survive a move.
10. **`wear-display.ts` is client-only** (imported by `components/wardrobe/*` only) despite
    living under `lib/wardrobe/` — SPA survey.
11. **Ordering subtleties the port must copy**: the seed scans `chats` in rowid order with
    `>=` tie-break for `lastWornChatId`; `summarize` uses strict `>` (FIRST row wins ties,
    SELECT order); `findHistory` sorts by string `lastWornAt` DESC with a STABLE sort;
    `allEquippedItemIds` is slot-order first-seen (so `newlyWornLeafIds` order is deterministic).
12. **Every existing tier-2 fixture lacks the table.** At the new pin, v4's
    `findSummaries` degrades gracefully (safeQuery → never-worn) but `commitEquippedOutfit`
    → `incrementWears` → `db.prepare(INCREMENT_SQL)` THROWS "no such table" the moment a
    newly-worn leaf exists, so `wardrobe-tools`, `chats-outfits` (if it ever calls the
    chokepoint), `chat-cast`, `chat-create-capstone` and the transfers fixtures must create
    the table **with the migration's DDL** (`WARDROBE_WEAR_STATS_DDL` or
    `addWardrobeWearStatsTableMigration.run()`), NOT via the repository (which would give
    the index-less REAL shape and the `ON CONFLICT` error).
13. **`wardrobe_update`'s echo gains `wear`** through the shared `buildWardrobeReadOutput`
    — a model-visible change the commit prose never mentions; whether
    `formatWardrobeUpdateResults` prints it was not read here (verify in the lane).
14. The transport-side `?action=wear-history` is three NEW item-GET dispatch arms in v5
    (character / General / group+project), where today only the collection GET dispatches
    (`wardrobe_routes.rs:111-122`); v4's 404-before-ledger order is pinned
    (`wear-ledger-routes.test.ts:197-206`).
