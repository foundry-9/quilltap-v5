# Survey — the `e8e17b2d9` round's OUTFIT-HISTORY KEYSTONE: the TWELFTH D23 move (`chat_wardrobe_history`, BOOT-CREATED), its three migrations re-homed as boot ensures, the repository (`settle` / `upsertRows` / `deleteByCharacter` / the four reads), the cascade-delete hunk, `describe-entry` at its final shape, and the `GET ?action=wardrobe-history` read

Read-only planning survey, **2026-10-19**. v4 pin **`e8e17b2d9`** (v4 `main`,
2026-10-18, "Fix: restore and import keep chat outfits (bug 252)") — read ONLY
through `git show e8e17b2d9:<path>` / `git show <sha> -- <paths>` / `git diff
25b8fab6a e8e17b2d9 -- <paths>` (the v4 working tree is at `a6292f169`, past the
pin, carrying the Inventory programme — never read). Oracle baseline
**`25b8fab6a`** (what v5 has ported). v5 `main` at **`2e89d291a`**
(`/Volumes/External 1/source_overflow/quilltap-v5`; `~/source/quilltap-v5`
symlinks to it). Every v4 line number is AT THE PIN; every v5 line number is at
`2e89d291a`. Nothing was built, run or written in either repo except this file.
Port facts come from SHIPPED HUNKS, never commit messages.

**This slice:** `83387d0e5` (storage, phase 1) whole; `4ae20977f`'s schema /
migration / repository / types / prettify / `describe-entry` hunks ONLY (its
`operator-provenance.ts`, `?action=equip`, Pascal `toolName` and carrier hunks
belong to the recording and carrier surveys); `8d8618819`'s SERVER GET only
(`handleGetWardrobeHistory` + the dispatch entry — NOT the POST rollback, NOT the
client; §3 records the POST body for the sibling); and
`lib/wardrobe/history/describe-entry.ts` at its FINAL shape (`f46cf40a2`'s
summary + `4ae20977f`'s labels). It builds on the ledger's §1 "Schema / D23"
block (`drift-ledger.md:166-224`, the human's 2026-10-19 BOOT-CREATED ruling) and
its §3 rows `:328` (`83387d0e5`), `:331` (`8d8618819`), `:333` (`f46cf40a2`),
`:334` (`4ae20977f`) — where a hunk disagrees with a row, §8 says so.

Shape follows `surveys/2026-10-16-setupphase-chat-settings-d23-keystone.md`.

---

## 0. The span for this slice, in one paragraph — and the definitive DDL answer

`git diff --stat 25b8fab6a e8e17b2d9 -- lib/schemas lib/database migrations/scripts
docs/developer/DDL.md` lists exactly eleven files: the NEW
`lib/database/backends/sqlite/chat-wardrobe-history-ddl.ts` (50), the NEW
`lib/database/repositories/chat-wardrobe-history.repository.ts` (407), the NEW
`lib/schemas/chat-wardrobe-history.types.ts` (82), the three NEW migrations
`migrations/scripts/add-chat-wardrobe-history-table-v1.ts` (77),
`add-chat-wardrobe-history-tool-name-v1.ts` (75) and
`seed-chat-wardrobe-history-v1.ts` (160), their registration
(`migrations/scripts/index.ts` +13), `lib/database/repositories/index.ts` (+5 —
the `wardrobeHistory` container slot), `docs/developer/DDL.md` (+49 — the
`chat_wardrobe_history` section, `:518-545`), and two hunks that are NOT DDL:
`lib/schemas/chat.types.ts` (+16/−7 — `a05c6f755`'s `customAnnouncer` DOC COMMENT
only) and `lib/database/repositories/files.repository.ts` (+27 — `1cfc9b504`'s
`findByLinkedToAny`, a read). `lib/database/schema-translator.ts` (generateDDL)
is UNCHANGED; no `ChatsRepository` / `TaggableBaseRepository` schema moved.
**The `chat_wardrobe_history` table (with `4ae20977f`'s `actorToolName` column)
and its two UNIQUE indexes are the ONLY DDL move between `25b8fab6a` and the
pin.** (The thirteenth / fourteenth moves — `chats.equippedInventory`,
`characters.canManageInventory` — are PAST the pin and out of scope.) No
`fresh_schema.json` / `chat_settings_seed.json` / `migration_indexes.json`
re-dump is owed: the repository is a plain class (no `ensureCollection`), so
generateDDL never renders the table, `dump-fresh-schema.ts` cannot see it and
`dump-migration-indexes.ts` drops its indexes (`freshTables.has(tbl_name)`
filter). Per the ruling, v5 creates it at BOOT.

---

## 1. v4 AT THE PIN — the hunks, file by file

### 1.1 `lib/database/backends/sqlite/chat-wardrobe-history-ddl.ts` (50 lines) — the DDL single source, VERBATIM

```ts
16  export const CHAT_WARDROBE_HISTORY_TABLE = 'chat_wardrobe_history';
18  export const CHAT_WARDROBE_HISTORY_DDL: readonly string[] = [
19    `CREATE TABLE IF NOT EXISTS "chat_wardrobe_history" (
20      "id" TEXT PRIMARY KEY,
21      "chatId" TEXT NOT NULL REFERENCES "chats"("id") ON DELETE CASCADE,
22      "characterId" TEXT NOT NULL,
23      "entryIndex" INTEGER NOT NULL,
24      "slots" TEXT NOT NULL,
25      "slotsHash" TEXT NOT NULL,
26      "source" TEXT NOT NULL,
27      "actorCharacterId" TEXT,
28      "isCurrent" INTEGER NOT NULL DEFAULT 0,
29      "createdAt" TEXT NOT NULL,
30      "actorToolName" TEXT
31    )`,
32    `CREATE UNIQUE INDEX IF NOT EXISTS "idx_chat_wardrobe_history_entry"
33      ON "chat_wardrobe_history" ("chatId", "characterId", "entryIndex")`,
34    `CREATE UNIQUE INDEX IF NOT EXISTS "idx_chat_wardrobe_history_current"
35      ON "chat_wardrobe_history" ("chatId", "characterId") WHERE "isCurrent" = 1`,
36  ];
43  export const CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN = 'actorToolName';
46  export const CHAT_WARDROBE_HISTORY_INSERT_SQL = `
47    INSERT INTO "chat_wardrobe_history"
48      ("id", "chatId", "characterId", "entryIndex", "slots", "slotsHash", "source", "actorCharacterId", "actorToolName", "isCurrent", "createdAt")
49    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
50  `;
```

Whitespace is load-bearing: inside the backticks the column lines carry FOUR
leading spaces, the closing `)` TWO, the index `ON` lines FOUR. SQLite stores
the text in `sqlite_master.sql` with `IF NOT EXISTS` removed and the rest
verbatim (`CREATE TABLE "chat_wardrobe_history" (\n    "id" TEXT PRIMARY KEY,\n
…\n    "actorToolName" TEXT\n  )`), so v5's constant must reproduce the template
literal byte for byte for the ensure differential's `sqlite_master` comparand.
The header (`:7-11`) carries the why to port: the two UNIQUE indexes ARE the
invariants (contiguous numbers per timeline; at most one cursor), and `slots` is
"plain JSON TEXT like `chats.equippedOutfit` — not a compressed text column;
nothing searches it and the rows are tiny." `:38-42`: `actorToolName` "sits last
here too" because the column migration appends it to an existing table — a fresh
table and an ALTERed one agree on column ORDER.

**The pre-column vintage** (`83387d0e5`'s version of the same file, `:18-29`):
identical but without line 30 — `"createdAt" TEXT NOT NULL\n  )` — and a
10-column INSERT. A v4 instance that booted any build in `83387d0e5 … f46cf40a2`
(2026-10-16 → 2026-10-17) carries THAT table; `4ae20977f`'s ALTER then makes its
`sqlite_master.sql` `… "createdAt" TEXT NOT NULL, "actorToolName" TEXT\n  )`
(SQLite splices `, <coldef>` before the final `)`). **Two `sqlite_master` texts
are reachable, one column order.**

### 1.2 `migrations/scripts/add-chat-wardrobe-history-table-v1.ts` (77 lines)

- `MIGRATION_ID = 'add-chat-wardrobe-history-table-v1'` (`:27`); `LOG_CONTEXT =
  \`migration.${MIGRATION_ID}\`` (`:28`); `description: 'Create
  chat_wardrobe_history table: every change of clothes per character per chat,
  with a cursor'` (`:32`); `introducedInVersion: '4.11.0'` (`:33`); `dependsOn:
  ['sqlite-initial-schema-v1']` (`:34`).
- `shouldRun()` (`:36-39`): `isSQLiteBackend()` ∧ `!sqliteTableExists(
  'chat_wardrobe_history')`. False ⇒ "skipped and NOT recorded" by the runner.
- `run()` (`:41-76`): `db.exec(statement)` for each of the THREE
  `CHAT_WARDROBE_HISTORY_DDL` statements (`:45-48`) — no transaction of its own.
  INFO `Created chat_wardrobe_history table` `{ context:
  'migration.add-chat-wardrobe-history-table-v1' }` (`:50`). Result `{ id,
  success: true, itemsAffected: 1, message: 'Created chat_wardrobe_history
  table', durationMs, timestamp }` (`:52-59`) — so the runner's
  `migrations_state` row is `(id, completedAt, quilltapVersion, 1, 'Created
  chat_wardrobe_history table')`. Catch (`:60-75`): ERROR `Failed to create
  chat_wardrobe_history table` `{ context, error: errorMessage }`; result
  `success: false, itemsAffected: 0, message: 'Failed to create
  chat_wardrobe_history table', error`. Not `resumable` ⇒ a failure stops v4's
  boot.
- **At the pin the migration creates the FINAL (11-column) shape** — so on any
  instance that did not boot the `83387d0e5 … f46cf40a2` window, the column
  migration below never runs.

### 1.3 `migrations/scripts/add-chat-wardrobe-history-tool-name-v1.ts` (75 lines)

- `MIGRATION_ID = 'add-chat-wardrobe-history-tool-name-v1'` (`:28`);
  `description: 'Add actorToolName to chat_wardrobe_history: the custom tool
  behind a Pascal outfit change'` (`:33`); `introducedInVersion: '4.11.0'`
  (`:34`); `dependsOn: ['add-chat-wardrobe-history-table-v1']` (`:35`).
- `shouldRun()` (`:37-41`): SQLite ∧ `sqliteTableExists(table)` ∧
  `!sqliteColumnExists(table, 'actorToolName')` — table-gated AND column-gated.
- `run()` (`:43-74`): `added = addColumnIfMissing('chat_wardrobe_history',
  'actorToolName', 'TEXT')` (`:46`; `migrations/lib/database-utils.ts` — DEBUG
  `Adding column` `{context: 'migrations.database-utils', table, column}` then
  `ALTER TABLE "chat_wardrobe_history" ADD COLUMN "actorToolName" TEXT`). INFO
  `Added actorToolName column to chat_wardrobe_history` `{ context:
  'migration.add-chat-wardrobe-history-tool-name-v1' }` ONLY when `added`
  (`:47-49`) — no summary line. Result `message: 'Added actorToolName column to
  chat_wardrobe_history'` ALWAYS, `itemsAffected: added ? 1 : 0` (`:50-57`).
  Catch (`:58-73`): ERROR `Failed to add actorToolName column to
  chat_wardrobe_history` `{ context, error }`; failure message the same text.
  No backfill (existing rows keep NULL — the header `:4-7`: "the readers label
  such an entry plain 'Pascal'").

### 1.4 `migrations/scripts/seed-chat-wardrobe-history-v1.ts` (160 lines)

- `MIGRATION_ID = 'seed-chat-wardrobe-history-v1'` (`:35`); `description: 'Seed
  the chat wardrobe history with what every character has on now'` (`:70`);
  `introducedInVersion: '4.11.0'` (`:71`); `dependsOn:
  ['add-chat-wardrobe-history-table-v1', 'add-chat-wardrobe-history-tool-name-v1']`
  (`:73`, comment `:72` "The tool-name column first: the shared INSERT spells it").
- `shouldRun()` (`:75-80`): SQLite ∧ `sqliteTableExists('chat_wardrobe_history')`
  ∧ `sqliteTableExists('chats')` ∧ `chats` has BOTH `equippedOutfit` and
  `updatedAt` (`getSQLiteTableColumns`). **No once-gate of its own** — the runner's
  `migrations_state` is the only thing that stops a second seed (the per-pair
  `hasRows` skip makes a re-run add nothing for pairs that already have rows; the
  ledger row is still owed, see §6.1).
- `outfitsFromEquippedOutfit(rawOutfit)` (`:48-66`, exported, pure): `!rawOutfit`
  → `[]`; `JSON.parse` throws → `[]`; parsed falsy / non-object / array → `[]`;
  else `Object.entries(parsed)` (array-index keys first — the JS order), skipping
  an empty `characterId`, each value through `normalizeEquippedSlots(slots)` —
  **a `null` or malformed slot bag is NOT skipped: it normalizes to all-empty
  slots and seeds an EMPTY entry 0.**
- `run()` (`:82-159`): `SELECT "id", "updatedAt", "equippedOutfit" FROM "chats"
  WHERE "equippedOutfit" IS NOT NULL` (`:89-91`, NO `ORDER BY` — rowid scan), read
  up front; `hasRows = SELECT 1 FROM "chat_wardrobe_history" WHERE "chatId" = ?
  AND "characterId" = ? LIMIT 1` (`:92-94`); ONE `db.transaction` (`:102-126`):
  per chat, per `(characterId, slots)`: has rows → `skipped += 1`, else INSERT
  `(randomUUID(), chat.id, characterId, 0, JSON.stringify(slots),
  hashEquippedSlots(slots), 'seeded', null, null, 1, chat.updatedAt)` (`:109-121`
  — `actorCharacterId` NULL, `actorToolName` NULL, `isCurrent` 1, `createdAt` =
  the chat's `updatedAt`), `seeded += 1`; `reportProgress(i + 1, chats.length,
  'chats')` per chat (`:124`). INFO `Seeded the chat wardrobe history from current
  outfits` `{ context: 'migration.seed-chat-wardrobe-history-v1', chatsScanned:
  chats.length, entriesSeeded: seeded, pairsSkipped: skipped }` (`:128-133`, ALWAYS,
  zero included). Result `itemsAffected: seeded, message: \`Seeded ${seeded}
  outfit record(s) across ${chats.length} chat(s)\`` (`:135-142`) — **`chats.length`
  is every chat with a NON-NULL `equippedOutfit` cell** (including `'nope'` /
  `'[]'` / `'{}'` cells that seed nothing), not "chats with outfits" (contrast the
  wear seed's `chats_with_outfits`). Catch (`:143-158`): ERROR `Failed to seed the
  chat wardrobe history` `{ context, error }`; failure message the same; the
  transaction rolled back whole.

### 1.5 `migrations/scripts/index.ts` — registration at the pin

Imports `:448` `addChatWardrobeHistoryTableMigration`, `:449`
`seedChatWardrobeHistoryMigration`, `:450` `addChatWardrobeHistoryToolNameMigration`
(import order ≠ run order). The `migrations[]` array ends (`:911-920`): `:912`
`addCustomToolTriggersFieldMigration`, `:914` `addComposerSlashCommandsFieldMigration`,
`:915` comment `// Wardrobe outfit history: the table, then a one-time seed of entry
0 from current outfits`, **`:916` `addChatWardrobeHistoryTableMigration`**, `:917`
comment `// Wardrobe outfit history: actorToolName (the custom tool behind a Pascal
change)`, **`:918` `addChatWardrobeHistoryToolNameMigration`**, **`:919`
`seedChatWardrobeHistoryMigration`**, `:920` `];`. Re-export block `:1362/:1364/
:1365`. Every `dependsOn` is earlier in the array ⇒ registration order IS run order.
**v5's boot order: … P4.D288 span → P4.D304's 4.11 `chat_settings` pair → table →
tool-name → seed → the P4.160 index-family backfill.**

### 1.6 `lib/schemas/chat-wardrobe-history.types.ts` (82 lines)

- `WardrobeHistorySourceSchema = z.enum([...])` (`:24-32`), in this order with v4's
  comments: `'chat-start'` (applyOutfitSelections for a new chat),
  `'participant-added'` (… for an added / reactivated seat), `'merge'` (… from a
  merge), `'operator'` (the Wardrobe dialog `?action=equip`, or Wear again —
  actorCharacterId = the user's live character), `'character'` (a `wardrobe_*`
  tool call — actorCharacterId = caller), `'pascal'` (a custom tool's wardrobe
  effects — actorCharacterId = rolling character, actorToolName = the tool),
  `'seeded'` (the backfill migration, or a first-touch seed).
- `ChatWardrobeHistoryRowSchema` (`:37-66`), key order: `id: UUIDSchema`, `chatId:
  UUIDSchema`, `characterId: z.string().min(1)` ("No FK: characters may be archived
  tombstones"), `entryIndex: z.number().int().nonnegative()` ("0-based, contiguous
  per (chatId, characterId); never renumbered"), `slots: EquippedSlotsSchema`,
  `slotsHash: z.string().length(16)`, `source: WardrobeHistorySourceSchema`,
  `actorCharacterId: z.string().nullable()` (doc `:49-55`), `actorToolName:
  z.string().nullable().default(null)` (doc `:57-61`: "Defaults to null so bundles
  and backups from before the column still parse"), `isCurrent: z.boolean()` ("The
  cursor: exactly one row per (chatId, characterId) is current"), `createdAt:
  TimestampSchema`. This is ALSO the repository's normalized-row key order and
  hence the `.qtap` / backup row shape (the carrier siblings' concern).
- `WardrobeHistoryProvenance` (`:71-77`): `{ source: Exclude<…, 'seeded'>;
  actorCharacterId?: string; toolName?: string }` ("`pascal` only: the custom
  tool's name").
- `WardrobeHistoryIntent` (`:80-82`): `{ kind: 'append'; provenance } | { kind:
  'cursor'; index: number }` ("the last intent in a turn wins").

### 1.7 `lib/database/repositories/chat-wardrobe-history.repository.ts` (407 lines)

Header (`:1-26`) — the why to carry: `settle` "is the one runtime writer"; "It
always reads the chat's *live* slots itself — never a snapshot the caller
passes"; the job-child buffering (NO-PORT — v5 has no child, P4.D255 (5)); "Nothing
renumbers or prunes"; `upsertRows` is "for restore and import only".
`const log = logger.child({ module: 'wardrobe-history' })` (`:51`).

**Module-level:**
- `LiveOutfitReader` (`:54-56`) — `getEquippedOutfitForCharacter(chatId,
  characterId)`; the container passes its `ChatsRepository`
  (`repositories/index.ts` `wardrobeHistory: new
  ChatWardrobeHistoryRepository(chatsRepo)`). That reader (`chats.repository.ts:
  797-810`) is a FALLBACK `safeQuery` over `getEquippedOutfit` (`:772-791`,
  itself a fallback that normalizes every character's bag through
  `normalizeEquippedSlots`) — `Failed to get equipped outfit for character` /
  `Failed to get equipped outfit` + `null` on error; a missing chat, a null
  `equippedOutfit`, or an absent / falsy character key → `null`.
- `ARRIVAL_SOURCES = new Set(['chat-start', 'participant-added', 'merge'])`
  (`:64`); `EMPTY_SLOTS_HASH = hashEquippedSlots(makeEmptyEquippedSlots())` (`:66`)
  (= the `null` hash — `outfit-hash.ts` normalizes through `bySlot(slot =>
  slots?.[slot] ?? [])`).
- `parseSlots(raw)` (`:82-88`): `normalizeEquippedSlots(JSON.parse(raw))`, a throw
  → `makeEmptyEquippedSlots()`.
- `normalizeRow(row)` (`:90-105`): `{ id, chatId, characterId, entryIndex:
  Number(row.entryIndex), slots: parseSlots(row.slots), slotsHash, source:
  <safeParse ok ? data : 'seeded'>, actorCharacterId: row.actorCharacterId ?? null,
  actorToolName: row.actorToolName ?? null, isCurrent: Number(row.isCurrent) === 1,
  createdAt }` — **an unknown stored `source` READS AS `'seeded'`**; an `isCurrent`
  of `2` reads `false`; the stored `slotsHash` is passed through (never
  recomputed on read); a table without `actorToolName` reads `null`.
- `insertParams(row)` (`:108-122`): the 11 positional values in INSERT order,
  `JSON.stringify(row.slots)`, `actorCharacterId ?? null`, `actorToolName ?? null`,
  `isCurrent ? 1 : 0`.
- `buildRow(input)` (`:124-148`): `slots = cloneEquippedSlots(input.slots)` (fresh
  arrays, `top→hair` key order), `id: randomUUID()`, `slotsHash:
  hashEquippedSlots(slots)`, `actorToolName: input.actorToolName ?? null`,
  `isCurrent: true`, `createdAt: input.at`.
- `SettleOutcome` (`:151-155`): `{kind:'appended', entryIndex, seeded}` |
  `{kind:'unchanged', entryIndex}` | `{kind:'cursor-moved', entryIndex}` |
  `{kind:'skipped', reason}`.

**`class ChatWardrobeHistoryRepository`** (`:157`), `readonly dbTarget = 'main'`
(`:159`), `constructor(private readonly chats?: LiveOutfitReader)` (`:165`).

- **`settle(chatId, characterId, intent)`** (`:177-195`) — "Never throws": `intent.
  kind === 'append'` → `await settleAppend(…)`, else `settleCursor(…,
  intent.index)`; on `appended` / `cursor-moved` → `publishRealtime('chats',
  chatId)` (`:182-184`, AFTER the transaction committed; NOT on `unchanged` /
  `skipped`). Catch (`:186-194`): **WARN `Wardrobe history settle failed; the outfit
  change stands unrecorded`** `{ chatId, characterId, intent, error: <message> }`
  (module `wardrobe-history`) → `{ kind: 'skipped', reason: 'error' }`.
- **`settleAppend`** (`:197-287`):
  1. no `chats` reader → `throw new Error('ChatWardrobeHistoryRepository.settle
     needs a chats store')` (`:202-204`) → the WARN above. (v5: structurally
     unreachable — record.)
  2. `live = (await chats.getEquippedOutfitForCharacter(chatId, characterId)) ??
     makeEmptyEquippedSlots()` (`:205`) — read OUTSIDE the transaction, through the
     fallback reader; `liveHash` (`:206`); `{source, actorCharacterId, toolName} =
     intent.provenance` (`:207`); **`actorToolName = source === 'pascal' ? toolName
     ?? null : null`** (`:208`); `at = new Date().toISOString()` (`:209`, ONE for
     the call).
  3. `runAtomically` (`:213-266`): `SELECT * … WHERE "chatId" = ? AND
     "characterId" = ? ORDER BY "entryIndex" ASC` (`:214-216`) → `normalizeRow`.
     - **No rows** (`:219-253`): `arrival = ARRIVAL_SOURCES.has(source)`; if
       `arrival || liveHash === EMPTY_SLOTS_HASH` → ONE row: entry `0`, `slots:
       live`, `source: arrival ? source : 'seeded'`, `actorCharacterId: null`, NO
       `actorToolName` (→ null even for a pascal source), current → `{appended, 0,
       seeded: !arrival}`. Else (a pre-history seat dressed for the first time) →
       a SEED row entry `0`, EMPTY slots, `'seeded'`, `actorCharacterId: null`,
       `isCurrent = false` (`:248`), THEN entry `1`: `slots: live`, `source`,
       `actorCharacterId ?? null`, `actorToolName`, current → `{appended, 1, seeded:
       true}`. Both rows share `at`.
     - **Rows** (`:255-265`): `current = rows.find(isCurrent) ?? rows[last]`; equal
       hash (`current.slotsHash === liveHash` — the STORED hash) → `{unchanged,
       current.entryIndex}`; else `nextIndex = rows[last].entryIndex + 1` (max + 1,
       NOT current + 1), `UPDATE … SET "isCurrent" = 0 WHERE "chatId" = ? AND
       "characterId" = ? AND "isCurrent" = 1`, INSERT entry `nextIndex` (live,
       source, `actorCharacterId ?? null`, `actorToolName`, current) →
       `{appended, nextIndex, seeded: false}`. So a live outfit equal to an OLDER,
       non-current entry still appends.
  4. Lines (`:268-285`): `unchanged` → DEBUG `Outfit unchanged since the current
     entry; nothing recorded` `{ chatId, characterId, source, entryIndex }`; else
     DEBUG `Recorded wardrobe history entry` `{ chatId, characterId, source,
     actorCharacterId: actorCharacterId ?? null, actorToolName, outcome, slotsHash:
     liveHash }`.
- **`settleCursor(chatId, characterId, index)`** (`:289-307`, synchronous):
  `runAtomically`: `SELECT * … AND "entryIndex" = ?` (`:291-293`); none →
  `{skipped, 'missing-index'}`; else `UPDATE … "isCurrent" = 0 WHERE … AND
  "isCurrent" = 1` then `UPDATE … SET "isCurrent" = 1 WHERE "id" = ?` →
  `{cursor-moved, index}` (moving onto the ALREADY-current index still runs both
  UPDATEs and publishes). Lines: skipped → **WARN `Wardrobe history cursor target
  missing; cursor left where it was`** `{ chatId, characterId, index }` (`:302`);
  else DEBUG `Moved wardrobe history cursor` `{ chatId, characterId, index }`
  (`:304`).
- **`runAtomically(work)`** (`:313-319`): `getRawDatabase()` null → throw
  `Wardrobe history: the main database is not initialized` (v5: unreachable);
  `db.transaction(() => work(db))()` — "Nests as a savepoint inside the job
  applier's own transaction."
- **`upsertRows(rows)`** (`:330-361`) — restore / import (the carrier siblings'
  caller): per row, NOT in one transaction: `slots = normalizeEquippedSlots(row.
  slots)`; `rawQuery(INSERT_SQL + ' ON CONFLICT ("id") DO UPDATE SET "entryIndex" =
  excluded."entryIndex", "slots" = …, "slotsHash" = …, "source" = …,
  "actorCharacterId" = …, "actorToolName" = …, "isCurrent" = …, "createdAt" =
  excluded."createdAt"', insertParams({ ...row, slots, slotsHash:
  hashEquippedSlots(slots) }))` (`:335-347` — the hash RECOMPUTED; `chatId` /
  `characterId` NOT in the UPDATE set); `written += 1`. A throw (a second current
  row, a duplicate `(chat, character, entryIndex)` under a DIFFERENT id, an FK
  miss) → backend ERROR `Raw query failed` `{query, error}` (`backend.ts:839-842`,
  v5's recorded NO-PORT class — `db/fallback.rs:1522`) then **WARN `Skipped a
  wardrobe history row that would break the timeline`** `{ id, chatId,
  characterId, entryIndex, error }` (`:350-356`); after the loop DEBUG `Upserted
  wardrobe history rows` `{ requested: rows.length, written }` (`:359`); returns
  `written`.
- **`deleteByCharacter(characterId)`** (`:364-367`): `DELETE FROM … WHERE
  "characterId" = ?`; DEBUG `Dropped wardrobe history for a deleted character` `{
  characterId }` (`:366`); a failure THROWS (to cascade-delete's catch).
- **Reads** (all `rawQuery` — a failure THROWS, no fallback):
  `findEntries(chatId, characterId)` (`:374-380`) `… ORDER BY "entryIndex" ASC`;
  `findCurrent(chatId, characterId)` (`:383-389`) `… AND "isCurrent" = 1 LIMIT 1` →
  row or `null`; `findByChatId(chatId)` (`:392-398`) `… WHERE "chatId" = ? ORDER BY
  "characterId", "entryIndex" ASC`; `findAll()` (`:401-406`) `ORDER BY "chatId",
  "characterId", "entryIndex" ASC`. All map through `normalizeRow`. (`findCurrent`
  / `findByChatId` / `findAll` have no reader in this slice — the tool / carrier
  siblings call them; port all four here so the siblings share ONE module.)

### 1.8 `lib/cascade-delete.ts` — the hunk (`83387d0e5`, pin `:410-417`)

Immediately AFTER the wear-ledger fold (`:399-408`) and BEFORE `await
repos.characters.delete(characterId)` (`:419`):

```ts
410  // Drop the character's outfit timelines: a departed character's history has no reader.
411  try {
412    await repos.wardrobeHistory.deleteByCharacter(characterId)
413    logger.debug('[CascadeDelete] Dropped outfit history', { context: { characterId } })
414  } catch (err) {
415    logger.error(`Failed to drop outfit history for character ${characterId}`, { context: { characterId } }, err instanceof Error ? err : undefined)
416  }
```

A failure (a pre-history instance's `no such table`) logs and the delete
PROCEEDS. Note the repository's own DEBUG (`Dropped wardrobe history for a deleted
character`, module `wardrobe-history`) precedes the cascade's DEBUG on success —
two lines, the P4.D256 fold's two-line shape (`cascade_delete.rs:548` pins INFO +
DEBUG there).

### 1.9 `lib/startup/prettify.ts` — the labels (pin `:159-161`), verbatim

```
'add-chat-wardrobe-history-table-v1': 'Binding a ledger of every change of clothes',
'seed-chat-wardrobe-history-v1': 'Noting down what everyone has on at the moment',
'add-chat-wardrobe-history-tool-name-v1': 'Ruling a column for the contraption that did the dressing',
```

v5 has NO labels table (grep: `PRETTY_LABELS` / `prettify` appear only in repair
modules' NO-PORT headers, e.g. `chat_settings_wardrobe_image_settings_repair.rs:42`,
`wardrobe_wear_stats_repair.rs:66`) — the standing NO-PORT class; record the three
labels in the new ensure module's header verbatim.

### 1.10 `app/api/v1/chats/[id]/actions/outfit.ts` — `handleGetWardrobeHistory` (pin `:301-364`)

Dispatch: `handlers/get.ts:74` `'wardrobe-history': () =>
handleGetWardrobeHistory(req, chatId, ctx)` — in the `dispatchAction` thunk map
AFTER `'outfit-summary'` (`:73`) and BEFORE `'photo-albums'` (`:75`); doc lines
`get.ts:13`, `route.ts:14`; `actions/index.ts` re-export. The handler, step by step:

1. `characterId = req.nextUrl.searchParams.get('characterId')` (first occurrence);
   `!characterId` (absent OR `''`) → `badRequest('characterId is required')` →
   **400 `{ "error": "characterId is required" }`** (`:312-313`), BEFORE any read
   and outside the `try`.
2. `try` (`:314`): `chat = await repos.chats.findById(chatId)` (a FALLBACK read —
   error → `null`) → absent → `notFound('Chat')` → **404 `{ "error": "Chat not
   found" }`** (`:315-316`).
3. `rows = await repos.wardrobeHistory.findEntries(chatId, characterId)` (`:318`).
   Empty → **200 `{ "currentIndex": null, "entries": [] }`** (`:319-321`) — the pool
   is NOT loaded (v4's test pins `mockLoadPool` not called).
4. `pool = await loadWearablePool(repos, characterId, undefined, { chatId,
   operator: true })` (`:323`) — project tier resolved from the chat, roster NOT
   applied (operator), archived items included in `get`.
5. `nameOf(id)` (`:324-334`) — memoized per call: `(await repos.characters.
   findByIdRaw(id))?.name ?? null`, a throw → `null` (unreachable in practice:
   `findByIdRaw` is itself a fallback `safeQuery` → `null` + `Error finding entity by
   ID`). `selfName = (await nameOf(characterId)) ?? 'the character'` (`:335`) —
   resolved AFTER the pool load.
6. Per row ascending (`:337-353`): `actorName = row.actorCharacterId ? await
   nameOf(row.actorCharacterId) : null`; `slots = resolveHistorySlots(pool,
   row.slots)`; push, **in this key order**: `index: row.entryIndex`, `at:
   row.createdAt`, `source: row.source`, `actorCharacterId`, `actorName`,
   `actorToolName: row.actorToolName`, `changedBy: changedByLabel(row, actorName,
   selfName)`, `isCurrent: row.isCurrent`, `summary: summarizeHistorySlots(slots)`,
   `slots`.
7. DEBUG **`[Chats v1] Read wardrobe history`** `{ chatId, characterId, entries:
   entries.length, context: 'wardrobe' }` (`:355`); **200 `{ currentIndex: rows.find(
   isCurrent)?.entryIndex ?? null, entries }`** (`:356-359`) via `NextResponse.json`
   (no envelope — the outfit-summary shape).
8. Catch (`:360-363`): ERROR **`[Chats v1] Error reading wardrobe history`** `{
   chatId, characterId }` + the error → `serverError('Failed to read wardrobe
   history')` → **500 `{ "error": "Failed to read wardrobe history" }`**. Reachable
   through `findEntries`' `rawQuery` throw (e.g. `no such table`, on a v4 instance
   or fixture without the table) — on v5 the boot ensure makes it unreachable in
   production; a planted dropped table reaches it.

### 1.11 `lib/wardrobe/history/describe-entry.ts` (118 lines) — the FINAL shape

History: `ebb480edd` (created), `f46cf40a2` (summary flattening), `4ae20977f`
(labels). Pure, "No I/O: the caller loads the pool once" (`:6-7`).

- `HistorySlotItem` (`:21-26`) key order `{ itemId, title: string | null,
  archived: boolean, missing: boolean }`; `HistorySlots = Record<WardrobeItemType,
  HistorySlotItem[]>` (`:28`).
- **`resolveHistorySlots(pool, slots)`** (`:31-40`): `bySlot(slot => (slots[slot]
  ?? []).map(itemId => pool.get(itemId) ? { itemId, title: item.title, archived:
  Boolean(item.archivedAt), missing: false } : { itemId, title: null, archived:
  false, missing: true }))` — keys `top, bottom, footwear, accessories, hair`
  (`WARDROBE_SLOT_TYPES`, `wardrobe.types.ts:33`); duplicates and order kept;
  `Boolean('')` is `false`.
- **`staleItemsOf(resolved)`** (`:43-54`): walk slots in canonical order, keep
  `archived || missing`, dedupe by `itemId` (first seen).
- **`summarizeHistorySlots(resolved)`** (`:61-73`): `values = bySlot(slot =>
  resolved[slot].map(e => e.missing ? \`${e.itemId} (no longer exists)\` :
  e.archived ? \`${e.title} (archived)\` : (e.title ?? e.itemId)))`, then
  `describeOutfit(values).split('\n').map(line => line.replace(/^\s*[-*]\s+/,
  '').replace(/\*\*/g, '').trim()).filter(Boolean).join('; ')`. (Pre-`f46cf40a2` it
  was `describeOutfit(values).replace(/\s*\n+\s*/g, '; ')` — port ONLY the final
  shape.) Worked examples from `describeOutfit` (`outfit-description.ts:103-156`):
  all empty → `completely naked and unadorned`; top+bottom empty → `naked; footwear:
  barefoot; accessories: no accessories` (hair empty vanishes); `- **top,
  bottom:** Green Dress` → `top, bottom: Green Dress`; a hair slot carries ` (` +
  `HAIR_STYLING_PRECEDENCE_NOTE` + `)`. An item TITLE containing `**` loses them; a
  title beginning `- ` on its own line cannot occur (titles sit after the label).
- **`changedByLabel(entry, actorName, selfLabel)`** (`:84-118`), `entry: { source,
  actorCharacterId, actorToolName?, characterId }` — every arm verbatim:
  - `operator`: no `actorCharacterId` → `'the operator'`; else `as = actorName ??
    (actorCharacterId === characterId ? selfLabel : null)`; `as` →
    `` `the operator, as ${as}` `` else `'the operator'`.
  - `pascal`: `pascal = actorToolName ? \`Pascal's ${actorToolName}\` : 'Pascal'`
    (an empty-string tool name → `'Pascal'`); `actorCharacterId && actorCharacterId
    !== characterId && actorName` → `` `${pascal} (rolled by ${actorName})` `` else
    `pascal`.
  - `character`: no actor OR actor === characterId → `selfLabel`; else `actorName ??
    'another character'`.
  - `chat-start` → `'chat opened'`; `participant-added` → `'joined the chat'`;
    `merge` → `'merged'`; `seeded` → `'before history was kept'`.
  The GET passes `selfLabel = selfName` (the character's raw name, else `'the
  character'`); the `wardrobe_history` TOOL passes `'you'` (the tool sibling's).

---

## 2. v5 on `main` (`2e89d291a`) — the landing sites

Nothing in v5 names `chat_wardrobe_history` / `wardrobeHistory` /
`wardrobe_history` (grep over `crates`, `apps/web/src`, `harness`: zero hits).
Every site below is NEW code beside a named precedent.

### 2.1 The DDL + repository — NEW `crates/quilltap-core/src/db/chat_wardrobe_history.rs`

- **Precedent to copy: `db/wardrobe_wear_stats.rs`** (1,480 lines; P4.D255 — the
  same programme's plain-class repository): its header shape (`:1-40`), the
  table/DDL/SQL constants (`WARDROBE_WEAR_STATS_TABLE :62`, `WARDROBE_WEAR_STATS_DDL:
  [&str; 3] :74`, `WARDROBE_WEAR_INCREMENT_SQL :87`), `run_atomically` (`:447-469`
  — a named SAVEPOINT that opens a transaction or nests in the job applier's, v4's
  `db.transaction`; **reuse it or a `schema_probe::in_savepoint` (`:62`) — do not
  write a third**), the borrowing repository struct (`WardrobeWearStatsRepository<
  'a> { conn } :513-518`), `upsert_rows :640`, `find_all :878`, the log
  convention (`target: "quilltap::wardrobe_wear", module = "wardrobe-wear"`
  `:568-569` — so this module's lines are `target: "quilltap::wardrobe_history",
  module = "wardrobe-history"`). Register in `db/mod.rs` beside `:171-172`.
- Constants: `CHAT_WARDROBE_HISTORY_TABLE`, `CHAT_WARDROBE_HISTORY_DDL: [&str; 3]`
  (§1.1 byte for byte), `CHAT_WARDROBE_HISTORY_TOOL_NAME_COLUMN`,
  `CHAT_WARDROBE_HISTORY_INSERT_SQL` (11 columns).
- Types: `WardrobeHistorySource` (seven, serde kebab-case `chat-start` /
  `participant-added` / …), `ChatWardrobeHistoryRow` (serde camelCase, §1.6 key
  order — the carriers serialize it), `WardrobeHistoryProvenance { source,
  actor_character_id: Option<String>, tool_name: Option<String> }`,
  `WardrobeHistoryIntent::{Append(provenance), Cursor(index)}`, `SettleOutcome`.
- **`settle` on the single writer.** The ledger's rule: `settle` is a
  read-compare-write in ONE RW transaction on the writer. v5 shape:
  `pub fn settle(conn: &Connection, chat_id, character_id, intent) ->
  SettleOutcome` called INSIDE a `Db::write` / `write_blocking` closure
  (`db/runtime.rs:529`, `:550`) — the live-slot read
  (`db/chats_outfits.rs:133` `ChatOutfitsRepository::get_equipped_outfit_for_character`,
  then `wardrobe::normalize_equipped_slots` `wardrobe.rs:726` / `Slots::from_value`
  `:625`) on the SAME connection, then the savepoint. "Never throws": every `Err`
  inside → the savepoint rolled back → WARN → `Skipped { reason: "error" }`; the
  CALLER's writes in the same closure (the outfit commit) must survive — a savepoint
  rollback, never the outer transaction. `publish_realtime(RealtimeTopic::Chats,
  Some(chat_id))` (`realtime/bus.rs:188`) on `Appended` / `CursorMoved` only. The
  recording sibling (`05005c562`) owns every CALLER; this keystone owns the
  function and its tier-2 proof.
- Slot helpers already exist: `Slots` (`wardrobe.rs:581`, `to_value` in top→hair
  order), `hash_equipped_slots(Option<&Slots>)` (`:852`, `None` == empty —
  `outfit_hash_equivalence`), `normalize_equipped_slots` (`:726`), `Slots::fresh()`.
  `now_iso()` at `clock.rs:61`; ids `uuid::Uuid::new_v4()` (the wear seed's choice,
  `wardrobe_wear_stats_repair.rs:235`).

### 2.2 The three ensures — NEW `crates/quilltap-core/src/db/chat_wardrobe_history_repair.rs`

- **The precedent is `db/wardrobe_wear_stats_repair.rs` (714 lines; P4.D255) — a
  TABLE step + a STAMPED seed step in ONE module, through v4's real runner in its
  differential.** Copy: `ensure_migrations_tables` first (`:121`, `db/
  migrations_ledger.rs`); step 1 gated by `!ledger_has(TABLE_ID) &&
  !table_exists` (`:126`), the DDL in `run_atomically` + `stamp(conn, id, now, 1,
  "Created …")` (`:127-138`); step 2 `ledger_has(SEED_ID)` →
  `AlreadyCompleted` / `!seed_should_run` → `NotApplicable` / else seed + stamp in
  ONE savepoint, a failure → ERROR `"Failed to seed …; it will be retried next
  boot"` + `SeedOutcome::Failed` (NOT `Err`, R-D) (`:145-185`); `seed_should_run`
  (`:193-201` — the SAME two `chats` columns); `stamp` (`:317-338` —
  `recordCompletedMigration`'s row + the two metadata keys, `quilltapVersion` =
  this app's version); `js_entries` (`:292-311`, `Object.entries` order — **make it
  shared, `pub(crate)`, rather than copied**); `ensure_…_table_only` (`:357-364`)
  for harness fixtures. The middle (column) step copies `db/chat_settings_4_11_
  repair.rs`'s one-column ensure (`:90-107`, through `schema_probe::
  add_column_if_missing` `schema_probe.rs:44-57` — its `ALTER TABLE "{table}" ADD
  COLUMN "{column}" {decl}` text is v4's `addColumnIfMissing` text).
- Three ids as `pub const`s; `run_chat_wardrobe_history_ensures(main) ->
  Result<ChatWardrobeHistoryEnsureOutcome, DbError>` running table → tool-name →
  seed, called from THREE places that must not drift (the `p4d288_span` lesson):
  the host, `restore_vintage_state`'s mirror-boot, `zero_migrations_rerun`'s
  `run_chain`.
- The seed body: `outfits_from_equipped_outfit(raw: Option<&str>) -> Vec<(String,
  Slots)>` (the pure twin of `:48-66`, pinned by v4's `reads legacy and malformed
  outfit maps` cases), the SELECT with no `ORDER BY`, the per-pair `hasRows` skip,
  INSERT through `CHAT_WARDROBE_HISTORY_INSERT_SQL` with `(uuid, chat.id, characterId,
  0, slots.to_value() as text, hash, 'seeded', NULL, NULL, 1, chat.updatedAt)`; stamp
  message `format!("Seeded {seeded} outfit record(s) across {chats} chat(s)")`,
  `itemsAffected = seeded`.
- Module header records the NO-PORT class: v4's INFO/ERROR migration lines
  (§1.2-1.4), `reportProgress`, `prettify.ts:159-161` (verbatim, §1.9), the
  child-proxy override.

### 2.3 Host boot — `crates/quilltap-host/src/host.rs`

`seed_built_ins`: P4.D255 wear block `:2101-2136` (v5's boot INFO shape: `target:
"quilltap::boot", migrationId = …, "Created the wardrobe wear ledger"` `:2115-2120`;
the seed INFO `:2122-2133`); P4.D288 `run_span_ensures` `:2168-2169`; **P4.D304
`run_chat_settings_4_11_ensures(main)?` `:2180`, `// === end P4.D304 ===` `:2181`;
P4.160 begins `:2182`. Insert the new block between `:2181` and `:2182`** — v4's
order (§1.5). Table step FATAL on failure (`?`, like every table ensure); seed
failure non-fatal (R-D, §8.2); v5's ONE INFO per step that did something
(proposed: `"Created the chat wardrobe history"`, `"Fitted the wardrobe history's
tool-name column"`, `"Seeded the chat wardrobe history from current outfits"` with
`entriesSeeded` / `chatsScanned` / `pairsSkipped` fields — the lane may choose the
words; the field convention is the wear block's). Host-boot test precedent:
`crates/quilltap-host/tests/host_boot_p4d255_wardrobe_wear_stats.rs` (header `:1-21`:
(a) pre-round + chats with outfits → created, seeded, both stamped, second boot
writes nothing; (b) v4-booted (ledger rows planted) → nothing; (c) fresh `setup` →
the seed stamps over zero chats). Whole-ledger readers in other host tests filter by
their own ids (`host_boot_p4d288_ensures.rs:153-161`, `host_boot_p4d304_…:133-139`,
`host_boot_p4d255_…:118`) — the new stamps move none of them.

### 2.4 `restore_vintage_state` — `crates/quilltap-harness/tests/restore_vintage_state.rs`

`fn open(instance)` `:83-175` is the mirror-boot: P4.D288 `:147-159`, P4.D304
`:160-173`, `db` returned `:174`. **Append a P4.D-this-round block after `:173`**
calling `run_chat_wardrobe_history_ensures` (memory `vintage-restore-mirror-boot-
list`). The committed `crates/quilltap-web/tests/fixtures/migration-vintage/*.db`
predates the table; the mirror-boot creates it, so the restore sibling (`7953da3a0`
`22n-ter`) has a table to upsert into. `the_vintage_fixture_carries_the_columns_
v4s_chain_adds` (`:688-735`, the `expected` list `:704`) reads the RAW fixture —
add no row there unless the fixture is rebuilt at the pin (then add
`("chat_wardrobe_history", "actorToolName", "add-chat-wardrobe-history-tool-name-v1")`
— a rebuild through v4's real chain at the pin creates the table at its FINAL shape,
so that row would name the TABLE migration's column).

### 2.5 `zero_migrations_rerun_equivalence` — `crates/quilltap-harness/tests/zero_migrations_rerun_equivalence.rs`

Header `:1-77`; `SPAN :87-101`; `BASES :105`; `CHAT_SETTINGS_4_11 :117-120` (the
P4.D304 precedent const); `run_chain :124-130`; `write_after_v5 :144-` (the
`-unstamped` twins delete only `SPAN` ids, `:188-192`);
`no_migration_reruns_after_the_v5_chain :223-` (`build[0]["ran"].len() == 14` at
`:231-236`; the `CHAT_SETTINGS_4_11` loops `:239`, `:284`). Oracle side `harness/oracle/cases/zero-rerun.ts` +
`harness/oracle/lib/v4-migrations.ts` (`P4D288_SPAN_MIGRATIONS :72-91`,
`CHAT_SETTINGS_4_11_MIGRATIONS :101-104`, `runV4Migrations :122` with `runner:
true :142-158`). Growth by three ids is §4.4.

### 2.6 Cascade delete — `crates/quilltap-core/src/services/cascade_delete.rs`

`execute_cascade_delete :323`; plugin-data DELETE `:415-418`;
`fold_departed_wearer(main, character_id)` `:420` (defined `:446-459`, doc
`:434-445` — the `contextJson` field convention for `{ context: { characterId } }`,
`crate::db::fallback::error_text` `fallback.rs:44` for the bare SQLite text);
`delete_character_and_reconcile` `:424`. **Insert `drop_departed_wearer_history(main,
character_id)` between `:420` and `:424`** with v4's DEBUG `[CascadeDelete] Dropped
outfit history` / ERROR `Failed to drop outfit history for character {id}`; unit
tests in the fold's shape (`:530-570` — `captured_with`, the two-line success
INFO-then-DEBUG → here DEBUG-then-DEBUG, and the no-table ERROR arm). There is NO
differential over `executeCascadeDelete` (grep: no harness family drives it) — the
unit tests are the proof, as for P4.D256.

### 2.7 The GET — core `api/chat_outfits.rs` + the dispatch wire + the web edge

- **Core handler — NEW `pub fn chat_wardrobe_history_get(db: &Db, chat_id: &str,
  character_id: Option<&str>) -> Response` in `crates/quilltap-core/src/api/
  chat_outfits.rs`** beside `chat_outfit_summary` (`:157-262` — the EXACT shape:
  `fallback::find_by_id_or_none("chats", …)` for v4's fallback `findById` `:177-183`
  → `not_found("Chat")` `:132-134`; `db.read_main(|main| db.read_mount_index(|mount|
  …))`; the DEBUG with `context = "wardrobe"` `:243-248`; the ERROR + `internal(…)`
  `:252-261`; `Response::ChatDialog(json!(…))` `:249`). Reads: the new repository's
  `find_entries`; `wearable_pool::load_wearable_pool(main, mount, character_id,
  None, LoadWearablePoolOptions { operator: true, chat_id: Some(chat_id), .. })`
  (`wearable_pool.rs:343-357`, `:372`); names via `db::characters_read::
  find_by_id_raw_or_none` (`characters_read.rs:305` — v4's `findByIdRaw` fallback);
  `bad_request("characterId is required")` (`:128-130`) for `None | Some("")`.
- **Wire** (`api/types.rs`, `#[serde(tag = "type", rename_all = "camelCase")]`
  `:72`): a NEW `Request::ChatWardrobeHistory { chat_id: String, character_id:
  Option<String> }` beside `ChatOutfitSummary` (`:3552-3556`) — `Option` so a
  dispatch caller omitting it gets v4's 400 sentence, not a serde refusal;
  `engine.rs` arm beside `:1709-1712`; response `Response::ChatDialog` (`:4763-4768`).
- **Web edge** `crates/quilltap-web/src/wardrobe_routes.rs` `chat_action_get`
  (`:504-611`): a `Some("wardrobe-history")` arm after `Some("outfit-summary")`
  (`:537-543`) reading `crate::query::first(&query.0, "characterId")`
  (`query.rs:50` — `searchParams.get`'s first-occurrence rule; preserves `""`);
  **`CHAT_GET_ACTIONS` (`:613-629`) gains `"wardrobe-history"` after
  `"outfit-summary"` (`:619`)** — the `availableActions` of v4's unknown-action
  envelope is v4's thunk-map order (`get.ts:68-86` at the pin: 16 keys). Its pin
  lives in `query_param_semantics_equivalence` / the dispatch-action rows — grow
  them. `CHAT_POST_ACTIONS` (`:634-`) gains `"wardrobe-rollback"` after `"equip"` — the
  ROLLBACK sibling's (it changes the POST refusal envelope's list; whoever lands
  first must coordinate, §8.8).
- **`COLLAPSE_CENSUS` does NOT move for the GET**
  (`crates/quilltap-harness/tests/web_edge_body_parse_guard.rs:107-…`): it counts
  BODY-key collapse idioms per `*_routes.rs` file (`wardrobe_routes.rs` row `:186-197`,
  count 1); a query-param read through `query::first` is not the idiom. The memory
  note "new routes need a COLLAPSE_CENSUS row" applies to a NEW routes FILE. The
  POST rollback body (sibling) must parse through a ported Zod parser — a
  `PARSER_WIRING` row candidate, not a census row.

### 2.8 The pure renderer — NEW `crates/quilltap-core/src/wardrobe_history.rs` (or `wardrobe/history/describe_entry.rs`)

`resolve_history_slots(pool: &WearablePool, slots: &Slots) -> HistorySlots`,
`stale_items_of`, `summarize_history_slots`, `changed_by_label`. Reuses
`WearablePool::get` (`wearable_pool.rs:138`, returns the item `Value`; `title` /
`archivedAt` read off it), `wardrobe::describe_outfit` (`wardrobe.rs:323`) over an
`OutfitSlotValues` (`:239-245`), `WARDROBE_SLOT_TYPES` (`:31`). Shared by the GET
(here) and the `wardrobe_history` tool (`ebb480edd` sibling) — ONE module. The
summary's `/^\s*[-*]\s+/` must use JS `\s` semantics (§6.10).

---

## 3. The shared contract (what the SPA History-tab lane and the siblings read)

### 3.1 `GET /api/v1/chats/{id}?action=wardrobe-history&characterId=<id>` — VERBATIM wire

Over HTTP and over `/api/dispatch` (`{ "type": "chatWardrobeHistory", "chatId",
"characterId" }`, the response's `data`). 200, JSON keys IN THIS ORDER:

```json
{
  "currentIndex": 2,
  "entries": [
    {
      "index": 0,
      "at": "2026-10-16T10:00:00.000Z",
      "source": "chat-start",
      "actorCharacterId": null,
      "actorName": null,
      "actorToolName": null,
      "changedBy": "chat opened",
      "isCurrent": false,
      "summary": "top, bottom: Green Dress; footwear: Riding Boots; accessories: no accessories",
      "slots": {
        "top":         [{ "itemId": "…", "title": "Green Dress", "archived": false, "missing": false }],
        "bottom":      [{ "itemId": "…", "title": "Green Dress", "archived": false, "missing": false }],
        "footwear":    [{ "itemId": "…", "title": "Riding Boots", "archived": false, "missing": false }],
        "accessories": [],
        "hair":        []
      }
    }
  ]
}
```

- `currentIndex`: the cursor's `entryIndex`, or `null` (no current row).
- `entries`: ASCENDING by `index` (the SPA shows newest first — its reversal).
- `source`: one of the seven; an unknown stored value arrives as `"seeded"`.
- `actorName`: the RAW character row's `name` (no vault overlay) or `null`.
- `actorToolName`: string for `pascal` entries written after `4ae20977f`, else `null`.
- `changedBy` — the complete label set (§1.11): `the operator` · `the operator, as
  <name>` · `Pascal` · `Pascal's <tool>` · `Pascal (rolled by <name>)` · `Pascal's
  <tool> (rolled by <name>)` · `<self name>` (the character's own change; `the
  character` when its name is unreadable) · `<actor name>` · `another character` ·
  `chat opened` · `joined the chat` · `merged` · `before history was kept`.
- `summary`: ONE line, `; `-joined, no Markdown; stale items read `<title>
  (archived)` / `<itemId> (no longer exists)`.
- `slots`: ALL FIVE keys always, `top, bottom, footwear, accessories, hair`; each
  item `{ itemId, title, archived, missing }` in that order; `missing: true` ⇒
  `title: null, archived: false`.
- Refusals: 400 `{ "error": "characterId is required" }` (absent or empty); 404 `{
  "error": "Chat not found" }`; 500 `{ "error": "Failed to read wardrobe history" }`.
  No history ⇒ 200 `{ "currentIndex": null, "entries": [] }`.
- v4's client types (`components/wardrobe/outfit-history-tab.tsx:30-54` at the pin)
  for `core-contract.ts`: `OutfitHistorySlotItem { itemId: string; title: string |
  null; archived: boolean; missing: boolean }`, `OutfitHistoryEntry { index: number;
  at: string; source: string; actorCharacterId: string | null; actorName: string |
  null; actorToolName: string | null; changedBy: string; isCurrent: boolean; summary:
  string; slots: Record<WardrobeItemType, OutfitHistorySlotItem[]> }`,
  `OutfitHistoryResponse { currentIndex: number | null; entries:
  OutfitHistoryEntry[] }`. Proposed request type `ChatWardrobeHistoryRequest { type:
  'chatWardrobeHistory'; chatId: string; characterId: string }` beside
  `ChatOutfitSummaryRequest` (`apps/web/src/app/core/core-contract.ts:641-645`, union
  `:2627`).
- Realtime: every `settle` that appended or moved the cursor publishes `chats` with
  the chat id; v4's `topic-map.ts` refetches `queryKeys.wardrobe.histories(id)` =
  `['wardrobe', 'history', chatId]` (prefix of `history(chatId, characterId)` =
  `['wardrobe', 'history', chatId, characterId]`). v5's twin:
  `apps/web/src/app/core/realtime-topic-map.ts` (the `chats` arm, `:87` area).

### 3.2 `POST /api/v1/chats/{id}?action=wardrobe-rollback` — for the sibling (`8d8618819` `handleWardrobeRollback`, `outfit.ts:366-416`)

- Body `rollbackBodySchema = z.object({ characterId: z.string().min(1,
  'characterId is required'), index: z.number().int().nonnegative() })` (`:296-299`),
  parsed FIRST inside the `try`; a `ZodError` → 400 `{ error: issues.map(e =>
  e.message).join(', ') }`; a non-JSON body → NOT a ZodError → 500.
- Then `chats.findById` → 404 `Chat not found`; `loadWearablePool(repos,
  characterId, undefined, { chatId, operator: true })`; `rollbackWardrobe(repos, {
  chatId, pool, index, context: { userId, chatId, characterId, provenance:
  operatorWardrobeProvenance(chat) }, source: 'ui' }, '[Chats v1]
  wardrobe-rollback')`.
- Refusal → INFO `[Chats v1] Wardrobe rollback refused` `{ chatId, characterId,
  index, reason, context: 'wardrobe' }` + 400 `{ error: <message> }` — the four
  messages (`lib/wardrobe/history/rollback.ts:113, :125, :132, :139`): `No outfit
  history yet for <name> in this chat.` · `<name> is already in the first outfit of
  this chat (0); there is nothing earlier to return to.` · `There is no outfit <n>;
  this chat's outfits are numbered 0 to <max>.` · `<name> is already wearing outfit
  <n>.`
- Success → INFO `[Chats v1] Wardrobe rolled back` `{ chatId, characterId,
  previousIndex, restoredIndex, skipped: <count>, context: 'wardrobe' }` + 200 `{
  "equippedSlots": {top,bottom,footwear,accessories,hair}, "previousIndex": n,
  "restoredIndex": n, "skipped": [{ "itemId", "title": string|null, "reason":
  "archived"|"missing" }] }`.
- Other error → ERROR `[Chats v1] Error rolling back wardrobe` `{ chatId }` + 500 `{
  error: 'Failed to roll back the outfit' }`.
- The rollback ends in `settle(…, { kind: 'cursor', index })` — this keystone's
  `settle` is its only history write.

### 3.3 Rust names the siblings consume (proposed — the lane may rename once, then freeze)

`db::chat_wardrobe_history::{CHAT_WARDROBE_HISTORY_TABLE, CHAT_WARDROBE_HISTORY_DDL,
CHAT_WARDROBE_HISTORY_INSERT_SQL, ChatWardrobeHistoryRow, WardrobeHistorySource,
WardrobeHistoryProvenance, WardrobeHistoryIntent, SettleOutcome,
ChatWardrobeHistoryRepository::{settle, upsert_rows, delete_by_character,
find_entries, find_current, find_by_chat_id, find_all}}`;
`db::chat_wardrobe_history_repair::{TABLE_MIGRATION_ID, TOOL_NAME_MIGRATION_ID,
SEED_MIGRATION_ID, run_chat_wardrobe_history_ensures,
ensure_chat_wardrobe_history_table_only, outfits_from_equipped_outfit}`;
`wardrobe_history::{HistorySlotItem, HistorySlots, resolve_history_slots,
stale_items_of, summarize_history_slots, changed_by_label}`;
`test_support::ensure_chat_wardrobe_history_on(conn)` (the `ensure_wear_ledger_on`
twin, `test_support.rs:188-191`); oracle `v4-migrations.ts`
`CHAT_WARDROBE_HISTORY_MIGRATIONS` = `[{ file:
'migrations/scripts/add-chat-wardrobe-history-table-v1.ts', exportName:
'addChatWardrobeHistoryTableMigration' }, { file:
'migrations/scripts/add-chat-wardrobe-history-tool-name-v1.ts', exportName:
'addChatWardrobeHistoryToolNameMigration' }, { file:
'migrations/scripts/seed-chat-wardrobe-history-v1.ts', exportName:
'seedChatWardrobeHistoryMigration' }]` (these file names DO carry `-v1`, unlike the
4.11 pair).

---

## 4. The differential plan — v4's REAL code → NDJSON → the Rust diff

### 4.1 NEW `chat_wardrobe_history_ensure_equivalence` (closest recipe: `wardrobe_wear_stats_ensure_equivalence.rs` + `harness/oracle/cases/wardrobe-wear-stats-ensure.ts`)

v4's REAL `MigrationRunner` narrowed to the three (the wear case's reason, `.ts:6-11`:
the ledger gate + `recordCompletedMigration` ARE the behaviour; `runV4Migrations`'s
direct arm cannot see them). Base files built at the pin under
`QT_FIXTURE_OUT_DIR/cwh-ensure-<mode>.db`; one NDJSON line per mode with v4's
`ran` / `skipped` report:

| mode | base | v4's runner (expected) | v5 |
|---|---|---|---|
| **A — no table** (Friday-before-the-pull) | `chats` with outfits (the §6 corpus), no table, no ledger | `+RAN` table, column `not needed` (UNRECORDED), `+RAN` seed | the chain |
| **B — pre-column table** (the `83387d0e5 … f46cf40a2` vintage) | §1.1's 10-column text + both indexes + rows (incl. a `pascal` row), table ledger row only | column `+RAN`, seed `+RAN` (no seed row yet) — or plant the seed row too as **B′** → column only | the chain |
| **C — v4-migrated** | A after v4's runner (all its rows) | nothing | the chain (all no-ops, no new stamp) |
| **D — final table, unseeded, no ledger** (a fresh v5 instance after a failed seed / the R-A-overruled shape) | the pin's DDL, chats with outfits, no ledger | table `not needed`, column `not needed`, seed `+RAN` | the chain |
| **E — seed corpus** | A's shape with the §6.4 edge cells | as A | the chain |

Comparands per mode, byte-exact: `PRAGMA table_info` (order — B's ALTER appends);
`sqlite_master.sql` of the table AND both indexes (A/C/D/E's fresh text vs B's
ALTERed text); every history row minus `id` ORDER BY `chatId, characterId,
entryIndex` (`createdAt` compared — it is the chat's `updatedAt`, deterministic;
`slots` text and `slotsHash` compared); the `migrations_state` rows minus
`completedAt` / `quilltapVersion`, by id (the seed's `Seeded N outfit record(s)
across M chat(s)` bytes and `itemsAffected`). Assert v4's report per mode so no mode
is vacuous. Env `QT_ORACLE_CWH_ENSURE` + `QT_FIXTURE_CWH_ENSURE_DIR`; recipe
headers in the wear family's form (memory `sweep-driver-recipe-header-rules`: only
the literal `cd ~/source/quilltap-server` maps to `--v4`).

### 4.2 NEW `chat_wardrobe_history_tier2_equivalence` — the repository over v4's REAL `ChatWardrobeHistoryRepository` (closest recipe: `wardrobe_wear_stats_tier2_equivalence.rs` + `harness/oracle/cases/wardrobe-wear-stats-tier2.ts`)

A pin-built fixture (a builder in the `build-*-fixture.ts` family, or the ensure
case's mode-A base) with `chats` rows carrying `equippedOutfit`; an op corpus
`harness/oracle/fixtures/chat-wardrobe-history-tier2.json` run in order on both
sides over fresh copies, the live slots driven by v4's REAL `ChatsRepository.
setEquippedOutfit` (v5: `chats_outfits.rs` `set_equipped_outfit`) between settles.
Ops, mirroring v4's test (§7) plus the edges:
- `settle append` — arrival on empty (`chat-start` dressed / naked), non-arrival
  on empty with empty live (the lone seeded `0`), non-arrival on empty with a
  dressed live (seed `0` + entry `1`), a `pascal` provenance with `toolName` (and one
  WITHOUT, and a `pascal` arrival-shaped empty-history case where the tool name is
  DROPPED), `character` with `actorCharacterId`, `operator` with none, an unchanged
  re-settle, a live equal to an OLDER entry (appends), a settle after a cursor move
  (max + 1), two characters' timelines, a settle on a chat id that does not exist
  (FK → WARN → `skipped`), a settle on a table-less copy (WARN).
- `settle cursor` — back, forward, onto the current index, a missing index (WARN).
- `upsert_rows` — round-trip of `find_by_chat_id` output after a DELETE; a bogus
  second-current row (skipped, WARN); a duplicate `(chat, character, index)` under a
  new id (skipped); a row whose stored hash is WRONG (recomputed); an existing id
  (UPDATE arm).
- `delete_by_character`; the chat DELETE cascade (`foreign_keys = ON` both sides —
  v4 `client.ts:120`).
- reads after each op: `find_entries`, `find_current`, `find_by_chat_id`,
  `find_all`, and the `SettleOutcome` each settle returned.
Comparands: the table dump with `id` and the settle-minted `createdAt` REMAPPED /
masked (minted-values remap — `createdAt` per call is ONE `now`; mask, never order by
it); the plant-time rows unmasked; the outcomes; the WARN/DEBUG lines (module
`wardrobe-history`) by message + fields where the capture rig can see them (memory
`capture-rig-writer-thread`: a `Db::write` closure is invisible to `captured_with` —
use `global_capture`). v4 case: `npx tsx` with `getRawDatabase` the real client
(the `wardrobe-wear-stats-tier2.ts` shape), `publishRealtime` recorded (assert the
publish set per op: appended / cursor-moved only).

### 4.3 The GET route — grow `chat_export_equivalence` (closest recipe: `chat-dialogs-export.test.ts`'s `outfit_summary_*` cases, `:402-408`)

That family already drives v4's REAL chat GET (`route.ts` → `get.ts` →
`dispatchAction`) over a fresh copy of the committed `chat-dialogs-{main,mount}.db`
pair whose builder (`build-chat-dialogs-fixture.ts`) plants vault wardrobe items
for the outfit-summary rows. Add `wardrobe_history_*` cases: per case, run v4's
REAL table migration on the copy (`runV4Migrations({ migrations:
CHAT_WARDROBE_HISTORY_MIGRATIONS, runner: true })` — the pair predates the table)
then plant timelines through v4's REAL `repos.wardrobeHistory.upsertRows` (fixed
ids, fixed `createdAt`) — v5 side: `ensure_chat_wardrobe_history_on` + the same
rows. Cases: a rich timeline (`chat-start` 0, `character` by another character,
`operator` with and without an actor, a `pascal` with tool + roller, a `seeded`,
an unknown stored `source`, an archived item, a missing id, a hair item, a
composite-leaf id, the cursor on a middle entry); an empty timeline (`{currentIndex:
null, entries: []}`); `characterId` absent and `?characterId=` empty (400); a
missing chat (404); a character whose raw row is absent (`the character` self label);
a no-cursor timeline (`currentIndex: null`, entries present); a table DROPPED on the
copy (500 + the ERROR). Compare status + JSON body BYTES (key order — memory
`serde-json-map-eq-ignores-order`: byte-compare via `serde_json::to_string`).
Plus the unknown-action envelope row with the 16-key list.

### 4.4 `zero_migrations_rerun_equivalence` — grows by three ids

`v4-migrations.ts`: export `CHAT_WARDROBE_HISTORY_MIGRATIONS` (§3.3) and append it to
the narrowed runner after `CHAT_SETTINGS_4_11_MIGRATIONS`; `zero-rerun.ts`'s
`planted` build gains at least one dressed chat (else the seed is vacuous) — the
planted base is derived from `fresh_schema.json`, which has NO
`chat_wardrobe_history`, so the table is naturally absent there; `run_chain`
(`.rs:124-130`) runs the history chain after the 4.11 pair. Expected: on `planted`
v4's build runs **table + seed = 16 `ran`** (`:231-236`'s 14 grows by TWO, not three
— the column migration is `not needed` on a table created at the final shape and
is NOT recorded); after v5's chain v4 runs NOTHING on any base; every history row
v5 stamped is byte-equal to v4's build row (planted) / v4's twin re-run (fresh);
on `v4-migrated` v5 adds no row. A history-ids `-unstamped` twin (delete only the
seed row; plus the table row if v5 stamps it — §8.1) must show v4 re-running
EXACTLY the seed — the proof the seed stamp is necessary. Keep the existing
`STAMP_RULE_RED_FIRST` twins SPAN-only (they delete `SPAN` ids; the history rows
survive them).

### 4.5 NEW `wardrobe_history_describe_entry_equivalence` — tier-1 (closest recipes: `describe_outfit_equivalence.rs` + `wearable_pool_equivalence.rs`)

Corpus `harness/oracle/fixtures/wardrobe-history-describe-entry.json`: pools as
LAYERS fed to v4's REAL `buildWearablePool` (`lib/wardrobe/pool.ts`) and v5's
`build_wearable_pool` (`wearable_pool.rs:199`) — the `wearable_pool_equivalence`
method, so no stub pool on either side; per row: `slots` (incl. duplicates, unknown
ids, archived, `archivedAt: ''`, all-empty, top+bottom empty, hair-only, a title with
`**`, a title with leading/trailing spaces, non-ASCII whitespace edges §6.10) →
`resolveHistorySlots` (JSON, key order), `staleItemsOf`, `summarizeHistorySlots`
(exact string); a `changedByLabel` table over every arm × `{actorName null/present,
actor == self / other, toolName absent / '' / present}` × `selfLabel` (`you`,
`the character`, a name). Exact string compare. v4 runs under `npx tsx` (no DB).

### 4.6 Families that move or are touched (run, and regen where marked)

- `provisioning_equivalence` — **REDDENS at the pin without an edit** (§8.3): v4's
  migrations-first boot (`harness/oracle/provision/migrations-first.ts`) creates
  the table + two indexes; v5's provisioned instance does not (boot-created). Add
  both-ways rows: `ORACLE_ONLY_TABLES` (`provisioning_equivalence.rs:743-775`)
  `("main", "chat_wardrobe_history", "v5 makes it at boot
  (db::chat_wardrobe_history_repair)")`, and `ORACLE_ONLY_INDEXES` (`:893-897`) for
  `idx_chat_wardrobe_history_entry` / `idx_chat_wardrobe_history_current`. Regen
  the oracle from the pin. (1d) `QT_FRESH_SCHEMA_LIVE` stays green (generateDDL
  blind).
- `outfit_hash_equivalence`, `describe_outfit_equivalence`,
  `wearable_pool_equivalence`, `wearable_pool_tier2_equivalence` — consumed,
  unmoved (no span hunk in `outfit-hash.ts` / `outfit-description.ts`;
  `pool.ts`'s span hunk is `5cb0e0190`'s behaviour-neutral `findByTitle` refactor).
- `restore_vintage_state` (mirror-boot line), the new host-boot test, the
  cascade unit tests, the census guard (§7) — no oracle.

---

## 5. Fixtures consumed / delivered

- **Delivered by this keystone:** `harness/oracle/fixtures/chat-wardrobe-history-
  tier2.json` (committed spec), `wardrobe-history-describe-entry.json` (committed
  corpus), the ensure case's scratch base files (`/tmp`, recipe-regenerated, never
  committed), the grown `chat-dialogs-export.test.ts` cases, the `v4-migrations.ts`
  export, `test_support::ensure_chat_wardrobe_history_on`.
- **No committed `*-main.db` pair needs widening for the keystone.** The repository
  is a plain class — v4 never writes it through a whole-entity `$set` (the P4.52
  class that forced pair widening for `chat_settings`), and `settle` SWALLOWS a
  `no such table` (WARN + `skipped`), so no v4 regen dies on an old pair. The GET
  family plants the table per case through v4's real migration (§4.3). The
  `migration-vintage` fixture stays as is (the mirror-boot creates the table).
- **For the siblings** (recording `05005c562`, tools `ebb480edd`, rollback POST,
  carriers `7953da3a0`): any family whose v4 case now calls `settle` over a
  committed pair will see v4's WARN `Wardrobe history settle failed; the outfit
  change stands unrecorded` (`no such table: chat_wardrobe_history`) and no rows
  unless the case runs `CHAT_WARDROBE_HISTORY_MIGRATIONS` on its per-case copy —
  then the v5 side must call `ensure_chat_wardrobe_history_on` on its copy (the
  `ensure_wear_ledger_on` pattern, 11 harness families use it). Measure the set with
  the fixture migrator's report mode (`harness/oracle/fixtures/
  migrate-memories-fixture-columns.ts --report-only --module …`) — a WOULD RUN here
  means "the table is absent", which is EVERY pair; the measure that matters is
  "does this family's v4 case reach `settle` / the history reads". Widen a pair in
  place (through v4's real migrations, never by hand — memory
  `rebuilt-fixture-pair-vintage`) only if a sibling prefers committed rows to
  per-case plants.
- **Who regenerates:** every family above from the ONE pin (`e8e17b2d9`) in a
  pinned worktree with its own `npm ci --offline` (the pin's `node_modules`), the
  sweep driver with `--v5w ~/source/quilltap-v5`, `--families` comma-separated,
  never two sweeps at once.

---

## 6. Traps and risks

1. **The seed STAMP is the only once-gate** (§1.4). Without it v4's next boot on a
   shared instance re-runs the seed. The per-pair `hasRows` skip keeps that from
   duplicating any timeline v5 seeded, but it DOES seed every `(chat, character)`
   that gained a non-null outfit since without yet having a history row (a seat v5
   dressed before its first `settle`, e.g. a restore-written `equippedOutfit`), and
   v4 records the run — `zero_migrations_rerun` reddens. The human's ruling makes the
   stamp mandatory; stamp INSIDE the seed's savepoint (the wear `:157-166` shape).
2. **`createdAt = chats.updatedAt`** for seeded rows — NOT now. A chat with
   `updatedAt` NULL violates `"createdAt" TEXT NOT NULL` → the WHOLE seed fails
   and rolls back (v4's catch). A real Friday chat with a NULL `updatedAt` would
   leave the seed un-stamped and retried every boot — measure on the copy.
3. **The partial UNIQUE index `WHERE "isCurrent" = 1`** is what refuses a second
   cursor; `upsert_rows` relies on it to skip a bad restore row, and `settle`'s
   UPDATE-then-INSERT order relies on it being released before the INSERT. A v5
   DDL string that drops the `WHERE` clause would pass every happy-path test —
   the ensure differential's `sqlite_master` comparand is the guard.
4. **Seed edge cells** (`outfitsFromEquippedOutfit`): `{"c": null}` / `{"c": 5}` /
   `{"c": {"top": "x"}}` → an EMPTY entry 0 (normalized, not skipped); `{"": {…}}` →
   skipped; `'[]'` / `'nope'` / `'null'` → nothing but COUNTED in `chats.length`;
   array-index character keys first (`js_entries`); a non-TEXT `equippedOutfit` cell
   (BLOB / integer) — v4's `JSON.parse(<number>)` parses a number → non-object →
   nothing (the wear seed's `ValueRef::Text` branch `:212-217` treats non-text as
   `None` — same result, but a BLOB is `Buffer` in v4: `JSON.parse(buffer)` coerces
   to string; measure if planting one).
5. **`randomUUID` ids** everywhere (seed, every `buildRow`) → a minted-id remap /
   mask in every comparand; never `ORDER BY "id"` (memory
   `shared-fixture-rebuilds-and-jest-setup-mocks`).
6. **The first-touch seed in `settle`**: a non-arrival source on an empty timeline
   with a DRESSED live outfit writes TWO rows (empty `0` not current + `1` current);
   with an EMPTY live outfit writes ONE (`0`, `seeded`, current, `seeded: true`) —
   and DROPS `actorCharacterId` / `actorToolName` (buildRow without them). An
   arrival source on an empty timeline keeps its own source but also drops the
   actor (always `null`).
7. **"Never throws" vs the single writer.** v5's `settle` runs inside the caller's
   writer closure; a failure must roll back ONLY its savepoint and return
   `Skipped`, leaving the caller's outfit commit intact (v4: "the outfit change
   stands unrecorded"). A `?` that escapes the savepoint would roll back the
   commit too. Unit-test: commit + failing settle in one closure → the commit lands.
8. **The live read is v4's FALLBACK reader** (`getEquippedOutfitForCharacter` →
   `null` on error → EMPTY slots): a chat whose `equippedOutfit` cell is corrupt
   records an EMPTY outfit, not a failure. Use v5's fallback-shaped read, not a
   `?`-propagating one (memory `extraction-lane-oracle-gotchas`).
9. **`unchanged` compares the STORED `slotsHash`** of the current row against the
   live hash — a row restored with a stale hash (pre-recompute) would append once.
   `upsert_rows` recomputes, so the class is closed on v4's write paths; a hand
   plant can open it (use correct hashes in plants unless testing exactly this).
10. **JS `\s` in `summarizeHistorySlots`' `/^\s*[-*]\s+/`** and `.trim()`: JS `\s`
    / `trim` include U+FEFF and exclude U+0085; Rust `char::is_whitespace` is the
    reverse. Titles with those code points change the summary — use the repo's
    JS-whitespace helper (the Phase-1 JS string family) or pin with corpus rows.
11. **`Boolean(item.archivedAt)`** — `''` is NOT archived; `null` / absent not
    archived. The pool `Value`'s `archivedAt` must be read JS-truthy.
12. **Two `sqlite_master` texts** for the table (fresh vs ALTERed, §1.1). Any guard
    or test that pins the table's SQL text must accept both, or the dogfood Friday
    copy (which may have booted the `83387d0e5 … f46cf40a2` window) trips it.
13. **Unknown `source` reads as `seeded`** (§1.7 `normalizeRow`) — on the wire too
    (§3.1). Port the safeParse fallback, not a hard error.
14. **`entries` names are RAW-row names** (`findByIdRaw`, no vault overlay) — the
    v5 twin is `characters_read::find_by_id_raw_or_none`, NOT the overlaid read;
    the self name is looked up AFTER the pool load (only log-order relevant).
15. **The GET does not load the pool for an empty timeline** — a v5 port that
    loads it first emits `[WearablePool] Loaded wearable pool` DEBUG on an empty
    read where v4 does not.
16. **`CHAT_GET_ACTIONS` order** (`wardrobe_routes.rs:613-629`): the unknown-action
    envelope lists v4's 16 keys in thunk-map order; an append at the END passes
    every served-action row and fails only the envelope row.
17. **Boot order:** after the 4.11 `chat_settings` pair, BEFORE the P4.160
    backfill (`host.rs:2181/2182`). The P4.160 pass reads only
    `migration_indexes.json`, which does not carry these indexes — the table step
    must create both itself.
18. **The cascade's two lines** (repository DEBUG then cascade DEBUG) and the
    no-table ERROR arm — the P4.D256 fold's tests are the template; the delete
    must PROCEED on failure.
19. **Gate hygiene** — env inline per call, `--nocapture`, grep the test's own
    `OK:` line (a SKIP reports `ok`); no in-place regen mid-gate; `--no-fail-fast`
    for census batches (memories `oracle-runner-and-skip-traps`, `lane-gate-hygiene`).

---

## 7. v4 tests to mirror (at the pin)

- `__tests__/unit/lib/database/repositories/chat-wardrobe-history.repository.test.ts`
  (289 lines; real better-sqlite3 in memory, `chats(id)` + the shared DDL, fake
  `LiveOutfitReader`, `rawQuery` routed to the same DB, `publishRealtime` mocked;
  `assertInvariants` `:79-85`: contiguous indices, exactly one current, every
  `slotsHash` = the hash of its `slots`). `it`s: `records an arrival outfit as entry 0
  with its own source` (:102), `records a naked arrival as entry 0` (:114), `seeds an
  empty 0 alone when a pre-history seat is still undressed` (:120), `seeds an empty 0
  then records the change as 1 when a pre-history seat gets dressed` (:128), `records
  the custom tool behind a Pascal change, and no tool for anyone else` (:147), `is a
  no-op when the live outfit equals the current entry` (:172), `appends max + 1 after
  a rollback, moving the cursor there` (:183), `appends even when the live outfit
  matches an older, non-current entry` (:204), `moves the cursor without appending,
  forwards as well as back` (:215), `leaves the cursor alone for a missing index,
  without throwing` (:229), `never throws: a database failure is logged and
  swallowed` (:237 — via `getRawDatabase` → null; v5's analogue is a dropped table),
  `keeps each character on their own timeline` (:243), `upsertRows › round-trips rows
  whole, and skips one that would make a second current row` (:255), `deleteByCharacter
  drops only that character` (:272), `cascades with the chat` (:282). → the §4.2
  corpus covers all; port `assertInvariants` as a Rust unit helper too.
- `__tests__/unit/migrations/chat-wardrobe-history.test.ts` (186 lines): `creates the
  table once, with unique indices and one cursor per timeline` (:92), `seeds one current
  entry 0 per (chat, character), dated by the chat` (:108), `skips a pair that already
  has rows` (:148), `adds actorToolName to a table created before it, keeping its rows`
  (:157 — the pre-column plant, `itemsAffected 1`, then `shouldRun` false), `has nothing
  to add to a fresh table` (:173), `reads legacy and malformed outfit maps` (:178-185 —
  `outfitsFromEquippedOutfit` over `{A:{top:['x']}}`, `null`, `'[]'`, `'nope'`). → §4.1
  modes + Rust unit tests for `outfits_from_equipped_outfit`.
- `__tests__/unit/lib/wardrobe/outfit-history-fence.test.ts` (82 lines; source greps):
  `no code outside the DDL module creates chat_wardrobe_history` (:42), `no code
  outside the repository, the seed migration and the DDL module inserts history rows by
  hand` (:50), `wardrobeHistory.upsertRows is called only from restore and import`
  (:62), `no turn creates the announcement set by hand` (:70 — the recording
  sibling's), `settle is a buffered write in the job child` (:78 — NO-PORT). → a v5
  census guard `chat_wardrobe_history_writer_census_guard.rs` in the
  `set_equipped_outfit_census_guard.rs` shape (the DDL string only in the DDL module;
  `INSERT INTO "chat_wardrobe_history"` only in the repository + the seed ensure;
  `upsert_rows` callers only in restore / import — the carrier sibling grows the
  allow-list).
- `__tests__/unit/app/api/v1/chats/[id]/actions/wardrobe-history.test.ts` (174
  lines): `GET › lists entries with labels, the cursor, and stale items marked` (:103 —
  `[0,'chat opened',false], [1,'Jeeves',false], [2,'the operator',true]`; the pool
  called with `(repos, 'char-1', undefined, { chatId: 'chat-1', operator: true })`),
  `answers an empty record without loading the pool` (:126), `requires characterId`
  (:132); POST `rolls back as an operator change…` (:141), `answers a refusal with 400
  and changes nothing` (:163), `requires an index` (:170) — the POST three are the
  sibling's. → §4.3 rows (over the REAL pool, not a mock).
- `lib/wardrobe/history/__tests__/describe-entry.test.ts` (59 lines): `marks archived and
  missing ids instead of dropping them` (:26), `summarizes on one plain line, with no
  Markdown` (:36), `labels who changed it` (:43 — eleven `changedByLabel` expectations,
  incl. `the operator, as Madeline` for a self-actor with `actorName` null). → §4.5.
- `__tests__/unit/cascade-delete.test.ts` (740 lines): `drops the character's outfit
  history before deleting the character` (:712). → the cascade unit tests (§2.6).

---

## 8. Open questions, and where a ledger row disagrees with a hunk

1. **Stamp the TABLE (and column) step, or the seed only?** The ruling (§1
   `:176-181`) mandates the SEED stamp. The P4.D255 precedent stamps the table step
   too when it RAN (`wardrobe_wear_stats_repair.rs:126-138`), which makes v5's ledger
   byte-equal to v4's own first boot (v4 records table + seed; the column only on
   the pre-column vintage) and lets §4.1 compare the ledger whole. Neither stamp is
   NECESSARY for zero-rerun (both `shouldRun`s are structural). **Recommend: stamp
   each step that RAN, exactly as v4's runner would** (table `1` / `Created
   chat_wardrobe_history table`; column `1` / `Added actorToolName column to
   chat_wardrobe_history` — only reachable on mode B; seed `N` / `Seeded …`), and
   scope §4.4's history twin accordingly. Order owner to confirm.
2. **Seed failure: FATAL or continue?** v4's seed is not `resumable` (a failure stops
   v4's boot). P4.D255 R-D ruled the wear seed NON-fatal (rollback, ERROR, unstamped,
   retried next boot). Recommend the same R-D shape here (precedent), table step
   FATAL.
3. **Ledger §1 + the `83387d0e5` row: "`provisioning_equivalence` stays GREEN" —
   only half true.** (1d) (the generateDDL dump) stays green, but (1b) column SETS
   and (1c) index set compare v5's provisioned instance against v4's REAL
   migrations-first boot (`provisioning_equivalence.rs:8-21`), which at the pin
   creates `chat_wardrobe_history` + both indexes: (1b) will report `main: v4 has
   table chat_wardrobe_history, v5 does not` (`:985-991`) and (1c) two index names.
   Both arms are both-ways lists — add the `ORACLE_ONLY_TABLES` row + two
   `ORACLE_ONLY_INDEXES` rows (§4.6). This IS a tripwire firing — the first one for a
   boot-created table; the ledger's "the D23 tripwire will not fire" is correct only
   for the dump.
4. **Ledger `4ae20977f` row: "`settle` writes `actorToolName = source === 'pascal' ?
   toolName ?? null : null` on both append arms (incl. the first-touch seed's entry
   1)"** — correct, with one gap: the empty-timeline ONE-row arm (arrival, or empty
   live) never writes it (`buildRow` without `actorToolName`, `:224-232`), so a
   `pascal` change that leaves a pre-history seat EMPTY records `seeded` with no
   tool. Recorded so a port does not "fix" it.
5. **Ledger `83387d0e5` row: "per `(chat, characterId)` of every non-null
   `equippedOutfit` (`normalizeEquippedSlots`; unparseable / array maps skipped)"** —
   correct for the MAP; a null / malformed per-character VALUE is not skipped (seeds
   an empty entry 0, §6.4). And the seed message's chat count is every non-null
   cell, not "chats with outfits".
6. **Ledger `8d8618819` row: "ERROR → 500 `Failed to read wardrobe history`"** —
   the 500 body is right; the ERROR line's text is `[Chats v1] Error reading wardrobe
   history` (`:361`). And the row's entry key list predates `4ae20977f`; the FINAL
   order (with `actorToolName` between `actorName` and `changedBy`) is §3.1 — the
   `4ae20977f` row has it right.
7. **The ledger's "the family expected to trip is `zero_migrations_rerun_
   equivalence`"** — it trips only once the narrowed runner carries the three ids
   (§4.4); today's narrowed list cannot see them. And its `planted` count grows by
   TWO (table + seed), not three — the column migration is `not needed` on a table
   created at the final shape and v4 does not record it.
8. **`CHAT_POST_ACTIONS` / `CHAT_GET_ACTIONS` ownership.** The GET key is this
   keystone's; the POST key (`wardrobe-rollback`, after `equip`) is the rollback
   sibling's. Both lists feed refusal envelopes pinned by
   `query_param_semantics_equivalence`-class rows — name the owner of each list
   edit in the orders so two lanes do not both append.
9. **Friday's vintage.** If the live instance booted any v4 build in `83387d0e5 …
   f46cf40a2` before `4ae20977f`, the dogfood copy carries the pre-column table with
   v4's table + seed rows and the column row (mode B). If it went straight to a later
   build, mode A's result. The dogfood pass should record which (`SELECT sql FROM
   sqlite_master WHERE name = 'chat_wardrobe_history'` + the three ledger rows) before
   v5's first boot.
10. **No ledger row names the seed's relationship to the wear seed.** Both iterate
    `chats.equippedOutfit` with the same SELECT and the same `Object.entries` order;
    v5 should share `js_entries` and (if convenient) the chat read, but the two
    seeds' per-character rules DIFFER (wear: distinct item ids, skip empty; history:
    one row per character, empty included) — do not merge their bodies.
