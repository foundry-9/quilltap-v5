# Survey — the outfit-history CARRIERS + bug 252 (`7953da3a0`, `e8e17b2d9`, `4ae20977f`'s carrier hunks)

READ-ONLY planning survey for `/setupphase`. The round is **pinned at v4
`e8e17b2d9`** (v4 `main`, 2026-10-18). The baseline (what v5 has ported) is
`25b8fab6a`. Every v4 fact below was read through `git show e8e17b2d9:<path>`
or `git diff 25b8fab6a e8e17b2d9 -- <path>`. The v4 working tree (`a6292f169`,
which carries the Inventory programme) was never read. v5 line numbers are at
v5 `main` (`2e89d291a`).

`git log 25b8fab6a..e8e17b2d9 -- lib/backup lib/export lib/import
lib/chat/character-keyed-fields.ts public/schemas/qtap-export.schema.json`
names exactly three commits: `7953da3a0`, `4ae20977f`, `e8e17b2d9`. **No other
commit in the span touches a carrier file.** The rest of the outfit-history
programme (the table, the `settle` writer, `describe-entry`, the tools, the
API, the SPA) belongs to sibling lanes. This lane needs only the keystone's
repository (§3).

---

## §0 The span in one paragraph

`7953da3a0` (phase 5) carries `chat_wardrobe_history` through every carrier:

- **Backup.** A new `data/wardrobe-history.json` holds the whole table
  (`findAll`, ordered by `chatId, characterId, entryIndex`). The manifest
  gains the count `wardrobeHistory`.
- **Restore.**
  - Step **`22n-ter`** runs after the wear ledger and writes the rows through
    the repository's `upsertRows`. That call skips bad rows one at a time:
    each skipped row gets a WARN, and the restore never fails because of one.
  - `RestoreSummary` and the preview gain `wardrobeHistory`.
  - The replace-mode wipe list gains `chat_wardrobe_history`.
  - `remapBackupData` (new-account mode) remaps `id` / `chatId` /
    `characterId` / `actorCharacterId`, moves slot item ids through the wear
    ledger's item remap, and recomputes `slotsHash`.
- **`.qtap` export.** It emits `{kind: 'wardrobe_history', chatId, data}`
  records per chat, after the informs and before the annotations.
- **`.qtap` import.**
  - The stream assembler collects the new kind.
  - A new phase **`7f`** (`import-wardrobe-history.ts`) Zod-validates each
    row whole and mints a fresh id per row.
  - It remaps chat, character and item ids, recomputes the hash, and never
    merges into a timeline the instance already holds.
- **Public schema.** `$defs.WardrobeHistory` is added (+33 lines).

`4ae20977f` adds `actorToolName` to the row. Its only carrier effects come
through the row shape: Zod `.nullable().default(null)`, the repository's
`normalizeRow` / `insertParams` / `upsertRows`, and +2 schema lines. The rest
of the hunks are comments.

`e8e17b2d9` fixes **bug 252**, and **v5 has the same defect today in both
carriers.**

- **The defect.** Four chat columns are keyed by character id:
  `equippedOutfit`, `characterAvatars`, `courierCheckpoints` and
  `pendingOutfitNotifications`. Both the new-account restore and the `.qtap`
  import reconcile copy them through with no re-keying.
- **The fix.**
  - A pure pass, `remapCharacterKeyedChatFields`, re-keys all four columns.
  - It also moves the ids nested inside them: wardrobe items, the
    outfit-avatar `imageId`, and the Courier `lastResolvedMessageId`.
  - It rebuilds each outfit as a full five-slot bag.
  - In restore, an entry for a character the backup does not carry is
    **dropped**.
  - In import, such an entry is **kept under its own id**.
- **Side effect.** The restore's mint order moves: courier message ids and
  unseen avatar file ids now mint at the chat.

---

## §1 v4 at the pin, file by file

### §1.1 Backup — `lib/backup/backup-service.ts`

- **`:290-292`** — inside `collectUserData`, immediately after the wear
  ledger read (`:287-288`):
  ```ts
  // The outfit history: every timeline in every chat, cursor included.
  const wardrobeHistory = await globalRepos.wardrobeHistory.findAll();
  moduleLogger.debug('Collected outfit history for backup', { rowCount: wardrobeHistory.length });
  ```
  - **No try/catch.** `findAll` is a raw `rawQuery`, not the base
    repository's `safeQuery`.
  - **On an instance without the table, the WHOLE `createBackup` throws.**
    The sqlite backend's `rawQuery` (`backend.ts:819-843`) also logs ERROR
    `Raw query failed {query, error}` before it rethrows. See §6 trap 1.
- **`:399`** — `wardrobeHistory` is the LAST key of the returned `BackupData`,
  after `wardrobeWear`.
- **`:502`** — `createManifest` counts `wardrobeHistory: data.wardrobeHistory?.length || 0`.
  It is the LAST key of `manifest.counts`, after `wardrobeWear`.
- **`:709-710`** — staging writes the file:
  ```ts
  // Outfit history (4.11). Optional on the way back in, like the wear ledger.
  await writeJsonArrayFile(path.join(stagingDir, 'data', 'wardrobe-history.json'), data.wardrobeHistory || []);
  ```
  It goes right after `wardrobe-wear.json` and is written on EVERY archive,
  compact ones included (`[]` when empty).
- **`lib/backup/types.ts`**:
  - `:250-251` — `BackupManifest.counts.wardrobeHistory?: number`
    ("Number of outfit-history rows (`chat_wardrobe_history`)").
  - `:426-431` — `BackupData.wardrobeHistory?: ChatWardrobeHistoryRow[]`,
    last.
  - `:623-624` — `RestoreSummary.wardrobeHistory?: number`
    ("Number of outfit-history rows restored"), after `wardrobeWear` and
    before `embeddingReconcile`.

**The `data/wardrobe-history.json` row shape and ordering.** The rows are
whatever `ChatWardrobeHistoryRepository.findAll()` returns
(`lib/database/repositories/chat-wardrobe-history.repository.ts:401-406`):

- The query is `SELECT * … ORDER BY "chatId", "characterId", "entryIndex" ASC`.
- `SELECT *` returns the DDL's COLUMN order, but the rows are mapped through
  `normalizeRow` (`:90-105`). That function fixes the **key order**:
  `id, chatId, characterId, entryIndex, slots, slotsHash, source,
  actorCharacterId, actorToolName, isCurrent, createdAt`.
  - This is NOT the DDL's column order: in the DDL, `actorToolName` is LAST
    (`chat-wardrobe-history-ddl.ts:19-31`).
- Per-key normalization in `normalizeRow`:
  - `entryIndex: Number(...)`.
  - `slots: parseSlots(raw)` — `normalizeEquippedSlots(JSON.parse(raw))`,
    falling back to five empty slots on a parse error. Each `slots` object
    therefore always has five keys in canonical order
    (`top, bottom, footwear, accessories, hair`).
  - `slotsHash` — **as stored, NOT recomputed**.
  - `source` — Zod `WardrobeHistorySourceSchema.safeParse`; an unknown value
    becomes `'seeded'`.
  - `actorCharacterId ?? null`.
  - `actorToolName ?? null`. The key is always present, so a pre-column table
    reads `null`.
  - `isCurrent: Number(...) === 1` (a boolean).
  - `createdAt` raw.

Unlike the wear ledger's `findAll` (P4.D264's `FIND_ALL_DROPS_NULLABLE`),
**nothing is dropped**: a NULL `actorCharacterId` row survives.

### §1.2 `parseBackupZip` — `lib/backup/restore/archive.ts`

- `:51` — imports `ChatWardrobeHistoryRow`.
- `:275-276`:
  ```ts
  // Outfit history (4.11) — optional, so an older archive just has none.
  const wardrobeHistory = await readJsonArrayFileOptional<ChatWardrobeHistoryRow>(rootPath, 'data/wardrobe-history.json', []);
  ```
  - The rows are raw JSON, with NO validation.
- `:325` — `wardrobeHistory` is the last key of the parsed `data`, after
  `wardrobeWear`.
- `lib/backup/restore/preview.ts:74` — `wardrobeHistory:
  data.wardrobeHistory?.length || 0` sits after `wardrobeWear` and before
  `warnings: []`. **The preview summary goes from 42 to 43 keys.**

### §1.3 Restore step `22n-ter` — `lib/backup/restore/restore.ts:1173-1190`

```ts
    // 22n-ter. Outfit history. After the chats it cascades from. Rows go in
    // whole (ids, indices, cursor); `upsertRows` logs and skips any row that
    // would break a timeline (a hand-edited second current row) rather than
    // failing the restore.
    let wardrobeHistoryRestored = 0;
    const wardrobeHistoryRows = data.wardrobeHistory || [];
    if (wardrobeHistoryRows.length > 0) {
      try {
        wardrobeHistoryRestored = await globalRepos.wardrobeHistory.upsertRows(wardrobeHistoryRows);
      } catch (error) {
        warnings.push(`Failed to restore the outfit history: ${error instanceof Error ? error.message : String(error)}`);
        moduleLogger.warn('Failed to restore outfit history', { rowCount: wardrobeHistoryRows.length, error });
      }
    }
    moduleLogger.debug('Restored outfit history', {
      total: wardrobeHistoryRows.length,
      restored: wardrobeHistoryRestored,
    });
```

- **Position.** Immediately after 22n-bis, the wear ledger (`:1152-1171`),
  and before 22o, instance settings.
- **`data` is the remapped bundle in new-account mode.**
- **`restored` counts rows, not all-or-nothing.** It is the count
  `upsertRows` RETURNS. Contrast 22n-bis, which is all-or-nothing
  (`restored = rows.length` on success). Do not copy 22n-bis's shape.
- **The DEBUG fires on EVERY restore**, including archives with no file:
  `total: 0, restored: 0`. So every restore case's census gains exactly one
  line.
- **The outer catch is effectively UNREACHABLE.** `upsertRows` catches each
  row itself and never throws (§3). The warning sentence
  `Failed to restore the outfit history: <msg>` and the WARN
  `Failed to restore outfit history` `{ rowCount, error }` exist in the
  source, so they belong in the census (§2.6). No plant reaches them through
  the real repository; record them as unreachable, with the reason, in the
  lane record (the P4.158 census convention).
- **`RestoreSummary` key position** (`:1422`): `…, textReplacementRules,
  wardrobeWear, wardrobeHistory, embeddingReconcile, warnings`.
  `Restore operation completed` logs the whole summary, so that line moves in
  every case.
- **Census.** `restore.ts` `moduleLogger` sites go from **79 to 81**:
  - 51 warn (was 50), 7 info, 21 debug (was 20), 2 error.
  - The two new lines are WARN `Failed to restore outfit history` and DEBUG
    `Restored outfit history`.

### §1.4 Replace-mode clear list — `lib/backup/restore/delete-service.ts:34-74`

`clearFormat3Entities`' `mainTables` gains, LAST, after `'wardrobe_wear_stats'`
(`:58`):

```ts
    // The outfit history. The chat delete's cascade would remove it, but
    // clearing it here keeps a restore from colliding on the timelines of
    // chat ids it is about to recreate (the per-timeline unique indexes).
    'chat_wardrobe_history',
```

- Each table is in its own `try`. On an absent table, `rawQuery` logs ERROR
  `Raw query failed` and the catch logs DEBUG
  `Skipping table truncate (table missing or empty)` `{table, error}`.
- This function also serves "delete all my data": `deleteUserData` (`:133`)
  calls it at `:227`, and `deleteAllUserData` (`:287`) goes through it. So
  **`system_delete_data_equivalence` moves too.**

### §1.5 `remapBackupData` — `lib/backup/restore/uuid-remap.ts`

**Module logger (NEW, `e8e17b2d9`)** — `:74`:
`const moduleLogger = logger.child({ module: 'backup:uuid-remap' });`. This is
the module's first logger.

**Bug 252's arm** — in the chat map, after the `scenarioSource` re-point
(`:356-358`) and BEFORE the `PARTICIPANT_ID_JSON_FIELDS` loop (`:387-394`):

```ts
  const backupCharacterIds = new Set(data.characters.map((character) => character.id));   // :331, BEFORE data.chats.map
  ...
    const keyed = remapCharacterKeyedChatFields(chat, {                                  // :364
      characterId: (id) => (backupCharacterIds.has(id) ? remapper.remap(id) : null),
      wardrobeItemId: (id) => wardrobeItemIdRemap.get(id) ?? id,
      fileId: (id) => remapper.remap(id),
      messageId: (id) => remapper.remap(id),
    });
    Object.assign(remappedChat, keyed.patch);
    if (keyed.unreadable.length > 0) {
      moduleLogger.warn('Chat character-keyed columns unreadable; left as given', {
        chatId: chat.id,
        fields: keyed.unreadable,
      });
    }
    if (Object.keys(keyed.patch).length > 0) {
      moduleLogger.debug('Remapped character-keyed chat fields', {
        chatId: chat.id,
        fields: Object.keys(keyed.patch),
        droppedEntries: keyed.droppedEntries,
      });
    }
```

- **The input is `chat`, the ORIGINAL.** `chatId: chat.id` is the ARCHIVE id,
  not the remapped one.
- **`Object.assign` onto the spread copy keeps each key in its position.** A
  null or absent column is skipped, so no key is ever appended.
- **Characters (character remap).**
  - A character the backup carries goes through `remapper.remap`. It was
    already minted by the characters pass (characters precede chats), so no
    new mint.
  - A character the backup does NOT carry → `null`, so the entry is DROPPED
    and `droppedEntries++`. Nothing is minted.
- **Wardrobe items (`wardrobeItemId`).** The same `wardrobeItemIdRemap` the
  wear ledger and outfit history use (built up front, `:224`). An unknown id
  passes through unchanged. Nothing is minted.
- **Avatar files (`fileId`).** `remapper.remap`. The id is already mapped if
  the file is in the backup (files precede chats); otherwise **it MINTS
  here**.
- **Courier messages (`messageId`).** `remapper.remap`. **It MINTS here**,
  before the chat's own messages are remapped at `:407-414`, which then reuse
  that id. This is the mint-order move named in the ledger: a courier message
  id's counter is now lower than every participant's.

**The outfit-history arm (`7953da3a0`)** — `:664-675`. It runs after the wear
ledger arm (`:659-662`) and before instance settings (`:679`):

```ts
  // Outfit history. `id`, `chatId`, `characterId` and `actorCharacterId` go
  // through the remapper; slot item ids follow the same item remap as the
  // wear ledger (an id the backup carries no document for passes through and
  // falls under the stale-item rule). Indices, cursor and dates are kept.
  const remappedWardrobeHistory = (data.wardrobeHistory || []).map((row) => {
    const slots = bySlot((slot) => (row.slots?.[slot] ?? []).map((id) => wardrobeItemIdRemap.get(id) ?? id));
    return {
      ...remapper.remapFields(row, ['id', 'chatId', 'characterId', 'actorCharacterId']),
      slots,
      slotsHash: hashEquippedSlots(slots),
    };
  }) as ChatWardrobeHistoryRow[];
```

- **Mint order, per row:** `id` (always fresh), `chatId` (usually already
  mapped), `characterId`, then `actorCharacterId`. `remapFields` rewrites
  only STRING values, so a `null` actor stays `null`.
- **A `characterId` the backup does not carry is MINTED** — a phantom that
  names nobody. This is NOT the bug-252 drop rule; see §8.4.
- `slots` is rebuilt as a full five-slot bag in canonical order, read
  through `row.slots?.[slot] ?? []`. There is NO normalize here:
  - a non-array slot value would throw on `.map`;
  - a non-string element passes through `wardrobeItemIdRemap.get(id) ?? id`
    unchanged.
- `slotsHash` is recomputed. Both keys already exist on the row, so the
  spread keeps their positions.
- The `BackupData` return key is `wardrobeHistory: remappedWardrobeHistory`
  (`:733`), last and ALWAYS present (`[]`). **Every
  `backup_uuid_remap_equivalence` case's output therefore gains the key.**

### §1.6 `lib/chat/character-keyed-fields.ts` (NEW, `e8e17b2d9`, 121 lines) — whole

- **`:33-38` `CHARACTER_KEYED_CHAT_FIELDS`** —
  `['equippedOutfit', 'characterAvatars', 'courierCheckpoints',
  'pendingOutfitNotifications']`. This is also the rekey order.
  - The module doc deliberately excludes `compiledIdentityStacks` (keyed by
    participant, rebuilt on a miss) and `commonplaceSceneCache` (a miss only
    re-emits the scene block).
- **`:44-49` `CharacterKeyedRemaps`**:
  - `characterId: (id) => string | null`
  - `wardrobeItemId`, `fileId`, `messageId`: `(id) => string`
- **`:52-63` `asObject(value)`**:
  - A string is `JSON.parse`d first; a parse error returns `null`.
  - The result is returned only if it is a non-null, non-array object;
    otherwise `null`.
- **`:66-74` `CharacterKeyedChatRemap`** — `{ patch, unreadable:
  CharacterKeyedChatField[], droppedEntries: number }`.
- **`:76-80` `withMappedId(entry, field, map)`**:
  - `obj = asObject(entry)`. If there is no object, or `obj[field]` is not a
    string, the ORIGINAL `entry` is returned unchanged.
  - Otherwise it returns `{ ...obj, [field]: map(obj[field]) }`.
  - Consequence: a JSON-string entry whose field is a string comes back as an
    OBJECT.
- **`:86-120` `remapCharacterKeyedChatFields(chat, remaps)`** — the per-column
  `rekey`:
  - `value == null` → skipped. That covers `null` and absent.
  - `asObject(value)` fails → push to `unreadable` and leave the column
    alone. Unreadable values include a number, a boolean, an array, a
    non-JSON string, and the string `"null"` (`JSON.parse` gives `null`).
  - Otherwise, for each `[characterId, entry]` of `Object.entries(map)`:
    - `newId = remaps.characterId(characterId)`;
    - if `newId` is truthy, `next[newId] = mapValue(entry)`;
    - else `droppedEntries++`.
  - Then `patch[field] = next`. An EMPTY object `{}` is still "readable":
    the column lands in the patch as `{}`, and the DEBUG and `hasUpdates`
    fire.

  Per-column `mapValue`:
  - `equippedOutfit` (`:111-114`):
    `normalizeEquippedSlots(slots)`, then
    `bySlot((slot) => normalized[slot].map(remaps.wardrobeItemId))`.
    - A four-slot bag gains `hair: []`.
    - A malformed slot is salvaged by `normalizeEquippedSlots`
      (`wardrobe.types.ts:393-405`): Zod `EquippedSlotsSchema.safeParse(raw ??
      {})` first; on failure, per slot, an array keeps its string elements
      and anything else becomes `[]`.
    - A JSON-STRING entry is not parsed here, so it becomes five empty slots.
  - `characterAvatars` (`:116`):
    `withMappedId(entry, 'imageId', remaps.fileId)`. `generatedAt` and
    `afterMessageCount` ride along.
  - `courierCheckpoints` (`:117`):
    `withMappedId(entry, 'lastResolvedMessageId', remaps.messageId)`.
    `resolvedAt` rides along.
  - `pendingOutfitNotifications` (`:118`): the entry as given; only the KEY
    moves.
- It returns `{ patch, unreadable, droppedEntries }`. `Object.keys(patch)`
  follows `CHARACTER_KEYED_CHAT_FIELDS` order.

### §1.7 `.qtap` export — `lib/export/ndjson-writer.ts:423-437`

This is inside `streamChats` (`:336`), per chat. It comes after the inform
block (`:403-421`) and BEFORE the conversation annotations (`:439+`):

```ts
    // Every character's outfit timeline in this chat, cursor included.
    try {
      const historyRows = await getRepositories().wardrobeHistory.findByChatId(id);
      for (const row of historyRows) {
        yield { kind: 'wardrobe_history', chatId: id, data: row };
        bump(counts, 'wardrobeHistory');
      }
      logger.debug('Exported outfit history', { chatId: id, count: historyRows.length });
    } catch (error) {
      logger.warn('Failed to load outfit history for export', {
        chatId: id,
        error: error instanceof Error ? error.message : String(error),
      });
    }
```

- **The record shape** is `{"kind":"wardrobe_history","chatId":<id>,"data":<row>}`.
  `data` is the repository's `normalizeRow` object (key order as in §1.1).
  **No Zod parse and no key-order template.**
- **Row order:** `findByChatId` orders by `"characterId", "entryIndex" ASC`
  (repository `:392-398`).
- **The count is a `bump`.** The key appears in the footer only when at least
  one row was emitted, inserted in first-bump order: after `chatInforms`'
  first bump and before `conversationAnnotations`' in single-chat streams.
- **The DEBUG fires per chat**, including when the chat has 0 rows.
- **The WARN fires on a THROWING read.** That includes an absent table: the
  read is `rawQuery`, which throws and also logs ERROR `Raw query failed`.
  This is not the fallback-read pattern of memory note
  `strict-export-vs-v4-fallback-reads`: the read really throws on both sides,
  and the export continues.
- `streamChats` has one caller (`:1153`, the `chats` export type). No other
  export type emits the kind.
- **`lib/export/types.ts`:**
  - `:113-114` — `QuilltapExportCounts.wardrobeHistory?`, declared after
    `chatInforms` and before `wardrobeWear`. The declaration order is
    irrelevant at runtime: `bump` insertion order rules.
  - `:272-278` — `ChatsExportData.wardrobeHistory?: ChatWardrobeHistoryRow[]`.
  - `:787-798` — `QtapWardrobeHistoryRecord { kind: 'wardrobe_history';
    chatId: string; data: ChatWardrobeHistoryRow }`. Its doc comment
    (reworded by `4ae20977f`) reads: "…the importer mints a fresh `id`,
    remaps `chatId`, `characterId` and `actorCharacterId`, and copies
    `entryIndex`, `isCurrent`, `slots`, `actorToolName` and `createdAt` as
    they are."
  - `:935` — added to the `QtapRecord` union, after `QtapChatInformRecord`.
- **`previewExport` is unchanged** — it has no `wardrobeHistory` count.

### §1.8 Stream assembler — `lib/import/quilltap-import-stream.ts`

- `:154` — `const wardrobeHistory: ChatWardrobeHistoryRow[] = [];`
- `:330-334`:
  ```ts
      // Outfit-history rows. Source-instance ids; the executor remaps them
      // once every chat and character has landed.
      case 'wardrobe_history':
        wardrobeHistory.push((record as { data: ChatWardrobeHistoryRow }).data);
        break;
  ```
- `:532` (the `CollectedArrays` literal) and `:573` (the interface).
- **`buildExportDataForType` `'chats'` arm (`:602-610`)** — `wardrobeHistory`
  is LAST, only when non-empty:
  ```ts
        ...(c.wardrobeHistory.length > 0 && { wardrobeHistory: c.wardrobeHistory }),
  ```
  It comes after `chatInforms`. **No other arm carries it.**
- `default:` (`:455-457`) — WARN `Unknown NDJSON record kind — skipping`
  `{ kind }`. That is what a pre-`7953da3a0` reader does with the kind; see
  §2.8 for v5's silent arm.

### §1.9 Import executor — `lib/import/quilltap-import/execute.ts`

- `:34-35` — imports `buildImportedWardrobeItemIdMap` (from
  `./import-wardrobe-wear`) and `importWardrobeHistory`.
- **Step `7f`, `:925-944`** — after 7e (wear ledger, `:901-923`) and before
  `// 8. Memories` (`:946`):
  ```ts
    // 7f. Outfit history. After the chats and characters (whose id maps it
    //    reads) and the wardrobe phases (whose item-id map remaps the slots).
    //    A timeline already live on this instance is never merged into.
    if (data.wardrobeHistory && data.wardrobeHistory.length > 0) {
      try {
        imported.wardrobeHistory = await importWardrobeHistory(
          data.wardrobeHistory,
          data.documents ?? [],
          idMaps,
          warnings
        );
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        warnings.push(`Failed to import the outfit history: ${msg}`);
        moduleLogger.warn('Failed to import outfit history', {
          rowCount: data.wardrobeHistory.length,
          error: msg,
        });
      }
    }
  ```
  - `imported.wardrobeHistory` exists ONLY when 7f ran and did not throw.
  - Its key goes in after `wardrobeWear` (7e) and before `files` (9) in the
    insertion order.
  - **`ImportResult`'s `imported` therefore gains an optional key after
    `wardrobeWear`.**
- **The reconcile call, `:1005-1007`** (bug 252):
  ```ts
    await reconcileRelationships(userId, repos, idMaps, warnings, {
      wardrobeItemIds: buildImportedWardrobeItemIdMap(data.documents ?? [], idMaps),
    });
  ```
- `lib/import/quilltap-import/types.ts:65-66` adds
  `AnyExportData.wardrobeHistory?`.

### §1.10 `lib/import/quilltap-import/import-wardrobe-history.ts` (NEW, 127 lines) — whole

- **Module logger** (`:38`): `logger.child({ module: 'import:wardrobe-history' })`.
- **`WardrobeHistoryRemapContext`** (`:40-45`): `{ chatIds, characterIds,
  itemIds: ReadonlyMap<string,string>; mintId: () => string }`.
- **`remapWardrobeHistoryRows(incoming, ctx)`** (`:53-81`) — pure. For each
  raw row:
  1. `ChatWardrobeHistoryRowSchema.safeParse(raw)`. On failure,
     `droppedInvalid += 1; continue`. The schema
     (`lib/schemas/chat-wardrobe-history.types.ts:37-66`) requires:
     - `id` and `chatId`: `z.uuid()` — RFC-variant UUID.
     - `characterId`: `z.string().min(1)`.
     - `entryIndex`: `z.number().int().nonnegative()`.
     - `slots`: `EquippedSlotsSchema`. The object is REQUIRED. Each of the
       five slots is `z.array(z.uuid()).default([])`: an absent slot is OK,
       a non-array fails, and **a non-UUID element fails the whole row**.
       Unknown keys are stripped.
     - `slotsHash`: `z.string().length(16)`, in UTF-16 units.
     - `source`: the seven-value enum.
     - `actorCharacterId`: `z.string().nullable()`. **The key is REQUIRED;
       an absent key fails.**
     - `actorToolName`: `z.string().nullable().default(null)`. Absent is
       OK and becomes `null` (`4ae20977f`: "Defaults to null so bundles and
       backups from before the column still parse").
     - `isCurrent`: `z.boolean()`, strict.
     - `createdAt`: `TimestampSchema` = `z.iso.datetime().or(z.date())`.
     - Unknown keys are stripped. **Output key order = schema order**
       (`id … actorCharacterId, actorToolName, isCurrent, createdAt`).
  2. `slots = bySlot((slot) => row.slots[slot].map((id) => ctx.itemIds.get(id) ?? id))`.
  3. Push `{...row, id: ctx.mintId(), chatId: ctx.chatIds.get(row.chatId) ??
     row.chatId, characterId: ctx.characterIds.get(row.characterId) ??
     row.characterId, actorCharacterId: row.actorCharacterId ?
     ctx.characterIds.get(row.actorCharacterId) ?? row.actorCharacterId :
     null, slots, slotsHash: hashEquippedSlots(slots)}`.
     - `actorCharacterId` uses truthiness: `""` becomes `null`.
     - **Mint order = input order**, valid rows only.
     - `entryIndex`, `isCurrent`, `source`, `actorToolName` and `createdAt`
       are copied.
- **`importWardrobeHistory(incoming, documents, idMaps, warnings)`**
  (`:83-127`):
  1. `if (incoming.length === 0) return 0;`
  2. Remap with `chatIds: idMaps.chats`, `characterIds: idMaps.characters`,
     `itemIds: buildImportedWardrobeItemIdMap(documents, idMaps)` (documents
     WIN over `idMaps.wardrobeItems`, P4.D264), and `mintId: () =>
     randomUUID()`.
     - **`preserveIds` is ignored**: history ids are always fresh.
  3. If `droppedInvalid > 0`, push the warning (`:99`):
     `Dropped ${droppedInvalid} malformed outfit-history row(s).`
  4. **Never merge** (`:103-117`):
     - Group the rows by `${row.chatId}\u0000${row.characterId}`, using the
       REMAPPED ids, in a `Map` (pairs in first-seen order).
     - For each pair, call `repos.wardrobeHistory.findEntries(chatId,
       characterId)`. **That is one read per pair, IN PAIR ORDER.** If the
       timeline already has rows, `skippedTimelines += 1` and the pair is
       skipped; otherwise the pair's rows are appended to `toWrite`.
     - **The write order is GROUPED BY PAIR, not input order.** Where pairs
       interleave in the input, the write order differs from the mint order.
  5. `written = await repos.wardrobeHistory.upsertRows(toWrite)` runs ALWAYS,
     even on `[]`. The repository DEBUG `Upserted wardrobe history rows`
     `{requested: 0, written: 0}` fires.
  6. INFO `Imported outfit history` (`:120-125`), with keys in this order:
     `{ incoming: incoming.length, written, droppedInvalid, skippedTimelines }`.
     `incoming` counts the RAW rows, invalid ones included.
  7. Return `written`.
- **What reaches the outer catch (§1.9)** — a throwing `findEntries` or
  `upsertRows`. In practice that means an absent table:
  `no such table: chat_wardrobe_history`, plus ERROR `Raw query failed` from
  the backend.
- **The never-merge hole** (v4 behaviour; port it faithfully and record it,
  §8.6). The guard only protects a NON-EMPTY local timeline. A chat that was
  skipped as a duplicate has no entry in `idMaps.chats`, so its rows keep the
  SOURCE `chatId`:
  - if a local chat with that id exists and the pair has no rows, the
    bundle's history IS written into the local chat;
  - if no such chat exists, every row fails the FK. Each gets the
    repository's per-row WARN and is skipped.

### §1.11 The `.qtap` import side of bug 252 — `lib/import/quilltap-import/reconcile.ts`

- `:23` imports `remapCharacterKeyedChatFields`. The module logger is
  `import:quilltap-import-service` (`:25`).
- **The signature** (`:176-189`) gains a 5th parameter: `options: {
  wardrobeItemIds?: ReadonlyMap<string, string> } = {}`.
- **The chat loop** (`:369+`): **the new block sits AFTER the participants
  remap (`:377-416`) and BEFORE the tags (`:444`)**. The scenario-source
  re-point comes later still (`:462+`). See §8.1: the ledger's "after the
  scenario-source re-point" is wrong.
  ```ts
      const keyed = remapCharacterKeyedChatFields(chat, {                     // :423
        characterId: (id) => idMaps.characters.get(id) ?? id,
        wardrobeItemId: (id) => options.wardrobeItemIds?.get(id) ?? id,
        fileId: (id) => idMaps.docMountFileLinks.get(id) ?? id,
        messageId: (id) => id,
      });
      if (keyed.unreadable.length > 0) {
        moduleLogger.warn('Chat character-keyed columns unreadable; left as imported', {
          chatId: newId,
          fields: keyed.unreadable,
        });
      }
      if (Object.keys(keyed.patch).length > 0) {
        Object.assign(updates, keyed.patch);
        hasUpdates = true;
        moduleLogger.debug('Remapped character-keyed chat fields', {
          chatId: newId,
          fields: Object.keys(keyed.patch),
        });
      }
  ```
- **The callbacks:**
  - The character remap never returns null, so **an unmapped character keeps
    its own id and nothing is dropped**.
  - `messageId` is identity: the message ids were already reminted at create
    (`remintChatMessageIds`, which also moves `courierCheckpoints[*]
    .lastResolvedMessageId`).
  - `fileId` follows `idMaps.docMountFileLinks`, "as a character's avatar
    does".
- **The log lines** use `chatId: newId`, the CREATED chat's id. The DEBUG has
  **no** `droppedEntries` (it is always 0 here).
- **Any readable non-null column sets `hasUpdates`**, an identity re-key and
  `{}` included. The `chats.update` then runs.
- **The input `chat`** comes from `repos.chats.findById(newId)`: the
  repository's parsed `ChatMetadata`, where each of the four columns is
  `JsonSchema.nullable().optional()` (`lib/schemas/chat.types.ts:1036`,
  `:1039`, `:1083`, `:973`).
- **No `warnings[]` text is added** by bug 252 on either carrier. So
  `import_warning_text_guard` does not move for bug 252, and `RESTORE_TS_
  MESSAGES` does not move for it either: the two uuid-remap lines belong to
  the `backup:uuid-remap` module, not to `restore.ts`.

### §1.12 The public schema — `public/schemas/qtap-export.schema.json` (+33 / −1 over the span; **108,678 bytes at the pin**, was 105,774)

- `:51` — the `chats` data object's `properties` gain `"wardrobeHistory": {
  "type": "array", "items": { "$ref": "#/$defs/WardrobeHistory" } }`, LAST.
  That object is `additionalProperties: false`, so **a v5 schema without this
  property refuses every chats bundle that carries history.**
- `:382` — `counts` gains `"wardrobeHistory": { "type": "integer",
  "minimum": 0 }`, after `chatInforms` and before `wardrobeWear`.
- `:1378-1406` — `$defs.WardrobeHistory`, between `$defs` `ChatInform` …
  and `WardrobeItem`:
  - `required: ["id", "chatId", "characterId", "entryIndex", "slots",
    "slotsHash", "source", "actorCharacterId", "isCurrent", "createdAt"]`.
    `actorToolName` is NOT required.
  - `additionalProperties: true`.
  - `id` and `chatId` are `$ref UUID`; `characterId` is `minLength 1`;
    `entryIndex` is an integer ≥ 0.
  - `slots` is an object with five string-array properties.
  - `slotsHash` is a string of length 16–16 ("hashEquippedSlots(slots);
    recomputed on import.").
  - `source` is the seven-value enum.
  - `actorCharacterId` and `actorToolName` are `["string","null"]`.
  - `isCurrent` is a boolean; `createdAt` is `$ref Timestamp`.
  - `4ae20977f` added `actorToolName` and reworded `actorCharacterId`'s
    description ("Who made the change: for `character`, …; for `pascal`, the
    rolling character; for `operator`, the character the user was speaking
    as at the time (null when the user drove no character). Null for
    arrivals and seeds.").
- The v5 copy is byte-identical to `25b8fab6a`'s today (`cmp` clean). The
  re-vendor is a plain byte copy from the pin.

---

## §2 v5 landing sites (file:line at `main`)

### §2.1 Backup

| Site | Change |
|---|---|
| `crates/quilltap-core/src/services/backup/collect.rs:137-142` (`BackupData`) | Add `pub wardrobe_history: Vec<Value>`, LAST, after `wardrobe_wear` (`:141`). |
| `collect.rs:757-770` (the `main_side` read) | Read through the keystone's `find_all`, serialized in **`normalizeRow` key order** (§3). See the absent-table ruling in §8.2. |
| `collect.rs:773-778` | DEBUG `Collected outfit history for backup` `rowCount`, target `quilltap::backup`, AFTER the wear DEBUG. |
| `collect.rs:875` + `MainSide` (`:883-910`, field at `:907`) | Thread the field through. |
| `collect.rs:1159-1210` (tests) | Plant tests modelled on the wear tests. |
| `services/backup/manifest.rs:102` | `n(c, "wardrobeHistory", data.wardrobe_history.len());` after `wardrobeWear`. |
| `services/backup/staging.rs:138` | `("wardrobe-history.json", &data.wardrobe_history)` after `wardrobe-wear.json`. Not in the compact omit list. |

`BackupData` is also constructed at `restore/orchestrator.rs:4662-4703` (a
test helper, `wardrobe_wear: Vec::new()` at `:4702`), at `uuid_remap.rs:466-512`,
and at `crates/quilltap-harness/tests/backup_uuid_remap_equivalence.rs:179`
(`rows(data, "wardrobeHistory")` there). Each needs the new field.

### §2.2 Restore archive, preview, summary

| Site | Change |
|---|---|
| `services/backup/restore/archive.rs:203` | `let wardrobe_history = opt("data/wardrobe-history.json")?;` |
| `archive.rs:248` | Add to `BackupData` last. |
| `services/backup/restore/mod.rs:96` (`RestoreSummary`, struct at `:46`) | `pub wardrobe_history: usize` after `wardrobe_wear` (serde camelCase ⇒ `wardrobeHistory`), before `embedding_reconcile`. |
| `restore/preview.rs:75` | `wardrobe_history: d.wardrobe_history.len()`, the 43rd key. |
| `restore/orchestrator.rs:4410-4453` (`Counters`, `wardrobe_wear` at `:4451`) | Add a counter. |
| `restore/orchestrator.rs:4542` (`into_summary`) | `wardrobe_history: self.wardrobe_history` after `wardrobe_wear`. |
| `restore/orchestrator.rs` — after 22n-bis (`:2024-2062`), before 22o (`:2064`) | **NEW 22n-ter block** (§1.3): only when non-empty, `c.wardrobe_history = <repo>.upsert_*(rows)` (the per-row count); the unreachable outer catch with the warning `Failed to restore the outfit history: {error}` + WARN `Failed to restore outfit history` (`rowCount`, `error`); the ALWAYS DEBUG `Restored outfit history` (`total`, `restored`); all on target `quilltap::restore`. Raw rows → the keystone's raw-row upsert (§3.2). |

### §2.3 Replace-mode clear list

`crates/quilltap-core/src/services/delete_all.rs:194-224`, `FORMAT3_MAIN_TABLES`:

- Append `"chat_wardrobe_history"` after `"wardrobe_wear_stats"` (`:220`),
  with v4's comment and a `=== P4.Dxxx ===` fence.
- The loop at `:284` already ignores errors (`let _ = main.execute(...)`), so
  an absent table is silent. v4 logs ERROR + DEBUG there; that is
  pre-existing and nobody compares it.
- Update the unit test `p4d293_truncate_list_tests` (`:636-651`) to v4's
  list at the pin.

### §2.4 New-account remap

`crates/quilltap-core/src/services/backup/uuid_remap.rs`:

**Facts on `main`:**

- `remap_backup_data` is at `:116`.
- `build_wardrobe_item_id_remap(data, r)` is built at `:132`. It returns
  `HashMap<String, String>` (`:532`). The order is tags → this →
  files → characters (`:160`) → profiles → chats (`:190`).
- `remap_chat(r, chat, target_user_id)` is at `:852-958`. **It proves the
  defect:**
  - `chain_owned` (`:853-876`) remaps only the listed scalars and arrays and
    clones every other key.
  - So `equippedOutfit`, `characterAvatars`, `courierCheckpoints` and
    `pendingOutfitNotifications` reach the restored chat with the ARCHIVE's
    character ids as keys, and the archive's item, file and message ids
    inside.
  - Nothing after `:876` touches them: the scenario arm is at `:884-906`,
    `PARTICIPANT_ID_JSON_FIELDS` at `:908-915`, participants and messages at
    `:917-956`.
- The wear arm is at `:417-437`; the return literal is at `:466-512`
  (`wardrobe_wear` at `:511`).

**The port:**

- **`remap_chat` grows two parameters:** the wardrobe item remap and the
  backup's character-id set. Build the set once, before `each(&data.chats,
  …)` at `:190`, as v4 does at `:331`.
- **Insert the bug-252 pass between the scenario arm (ends `:906`) and the
  `PARTICIPANT_ID_JSON_FIELDS` loop (`:908`).** This is the mint-order
  position.
  - Character: in the set → `r.remap_str`; else `None` (drop).
  - Item: `item_map.get(id).cloned().unwrap_or(id)`.
  - File and message: `r.remap_str`.
- **Assign each patched column onto the EXISTING key** (`obj.insert` on an
  existing IndexMap key keeps its slot). Never append.
- **Logs:**
  - WARN `Chat character-keyed columns unreadable; left as given`
    `{chatId (ARCHIVE id), fields}`.
  - DEBUG `Remapped character-keyed chat fields` `{chatId, fields,
    droppedEntries}`.
  - Both on a NEW target for v4's `backup:uuid-remap`. The ledger names it
    "the module's FIRST tracing target". Pick per house convention, e.g.
    `quilltap::backup::uuid_remap`.
  - `fields` is an array: render it through the file layer's `…Json`
    convention.
- **The history arm** goes after the wear arm (`:437`) and before instance
  settings (`:440`), as in §1.5:
  - `r.remap_fields(row, &["id","chatId","characterId","actorCharacterId"])`;
  - then `slots` = the five canonical keys, each mapped through the item
    remap (no normalize, §6 trap 6);
  - `slotsHash` = `wardrobe::hash_equipped_slots`;
  - both inserted onto existing keys.
  - The return literal gains `wardrobe_history` after `wardrobe_wear`
    (`:511`): ALWAYS present, `[]` when empty.

### §2.5 The pure bug-252 twin

NEW module, e.g. `crates/quilltap-core/src/chat_keyed_fields.rs` or
`services/carriers/character_keyed_fields.rs`, next to C1-11's
`carriers/scenario_source_remap.rs`. It needs:

- `CHARACTER_KEYED_CHAT_FIELDS: [&str; 4]` in v4's order.
- `remap_character_keyed_chat_fields(chat: &Value, remaps: &mut impl
  CharacterKeyedRemaps) -> CharacterKeyedChatRemap { patch: Map (field
  order), unreadable: Vec<&'static str>, dropped_entries: usize }`. Remaps
  can be closures: the restore's mint through a `&mut UuidRemapper`
  borrowed by four callbacks needs `RefCell`, as at `:887`.
- `as_object(value)`: a string → `serde_json::from_str`, an object only.
  This is V8 `JSON.parse` acceptance; see trap 9.
- `with_mapped_id`.
- The `equippedOutfit` mapValue reuses `wardrobe::normalize_equipped_slots`
  (`wardrobe.rs:726`, which already IS v4's salvage over
  `Slots::from_value`), then maps every slot's ids.

It needs its own tier-1 family (§4.8).

### §2.6 The restore census (BOTH lists)

- **`harness/oracle/cases/system-restore.test.ts:884-969`
  (`RESTORE_TS_MESSAGES`)** and
  **`crates/quilltap-harness/tests/system_restore_state.rs:6036+`** — add, in
  both, `"Failed to restore outfit history"` and `"Restored outfit
  history"`.
  - Update both doc counts: TS `:871-883` and Rust `:6022-6035` become "81 at
    `e8e17b2d9` (51 warn, 7 info, 21 debug, 2 error)".
  - Remember the target check at `system_restore_state.rs:6304-6316`: v5's
    twins must log on `quilltap::restore`.
- **The repository's per-row lines** belong to the keystone's repository:
  - WARN `Skipped a wardrobe history row that would break the timeline`
    `{id, chatId, characterId, entryIndex, error}`;
  - DEBUG `Upserted wardrobe history rows` `{requested, written}`.
  - They are not `restore.ts` lines. To compare them, add them to
    `REPO_LEVEL_MESSAGES` (or a new list) on the outfit-history cases ONLY.
    `REPO_LEVEL_CASES` gates them in Rust; `system-restore.test.ts:1288`
    joins the lists.
- **The two `backup:uuid-remap` lines**: whitelist them in the restore oracle
  on the new-account bug-252 case (a new list, e.g. `UUID_REMAP_MESSAGES`),
  or capture them in the tier-1 remap oracle (§4.2).

### §2.7 `.qtap` export

| Site | Change |
|---|---|
| `crates/quilltap-core/src/services/qtap_export/records.rs:202` (`stream_chats`) | Insert between the inform block (`:312-342`, the `// === end P4.D205 ===` line) and the annotations (`:344`). |

What to insert:

- `find_by_chat_id(id)`; on `Ok`, per row push `{"kind":"wardrobe_history",
  "chatId":id,"data":row}` with `counts.bump("wardrobeHistory")`.
- DEBUG `Exported outfit history` with `chat_id` / `count` fields. Use the
  same field spelling as the inform DEBUG at `:334`, which renders v4's
  `chatId`; follow the house rule.
- On `Err`, WARN `Failed to load outfit history for export` `{chatId,
  error}` and continue.
- **NO `reorder`.** No `schema-key-order.json` entry is needed: v4 emits the
  repository row, not a Zod parse, and the `wardrobe_wear` precedent at
  `:751-797` emits RAW as well. The `data` object's key order must therefore
  come from the row's serde field order (§3).

`schema-key-order.json` and `key_order.rs` **do not move**:
`dump-export-key-order.ts` dumps Zod schemas, and this record kind has none in
the writer. If the lane wants a guard, add a test that the kind has no
template, mirroring `wardrobe_item` at `key_order.rs:117`.

The `Counts` map (`qtap_export/mod.rs:162-170`) already keeps insertion order.

### §2.8 `.qtap` import — assembler (`services/quilltap_import/ndjson.rs`)

- **`:623`, the unknown-kind arm `_ => {}`, SILENTLY drops `wardrobe_history`
  today.** This is the P4.D264 `wardrobe_wear` precedent (`:617-621`). Note
  that the arm's comment says "v4 warns and skips", but v5 emits no WARN.
  v4's `Unknown NDJSON record kind — skipping` has no v5 twin at all; see
  §8.7.
- **Add:**
  - `let mut wardrobe_history: Vec<Value>` beside `:198-201`;
  - the arm `"wardrobe_history" => wardrobe_history.push(data())`;
  - the `Collected` field (`:760-782`);
  - the struct literal at `:742`;
  - the `"chats"` arm (`:830-857`): insert `wardrobeHistory` LAST, after
    `chatInforms`, only when non-empty.
- **The legacy (document-form) JSON path** passes `data.wardrobeHistory`
  straight through. Step 7f reads `data["wardrobeHistory"]` whatever the
  format.

### §2.9 `.qtap` import — executor + phase 7f (`services/quilltap_import/mod.rs`)

**`ImportCounts`** (`:147-199`): add
`#[serde(skip_serializing_if = "Option::is_none")] pub wardrobe_history:
Option<u32>` right after `wardrobe_wear` (`:185`).

**Step 7f** goes between 7e (`:1938-1963`) and `// 8. Memories.` (`:1965`):

- `if let Some(rows) = non_empty_array(data, "wardrobeHistory")`, call NEW
  `wardrobe_history::import_wardrobe_history(main, rows, documents, id_maps,
  warnings)`.
- `Ok(n)` → `imported.wardrobe_history = Some(n)`.
- `Err(e)`:
  - `let msg = item_error_text(&e)`;
  - push `format!("Failed to import the outfit history: {msg}")`;
  - WARN `Failed to import outfit history` `rowCount`, `error = %msg`.

**NEW `services/quilltap_import/wardrobe_history.rs`**, modelled on
`wardrobe_wear.rs`:

- `parse_wardrobe_history_row(raw) -> Option<Row>` — a whole-row port of
  `ChatWardrobeHistoryRowSchema` in P4.161's idiom, as at
  `wardrobe_wear.rs:77-125`:
  - `zod_uuid_ok` (`api/zod_issues.rs:651`) for `id`, `chatId` and every
    slot element;
  - `zod_iso_datetime_ok` (`:1837`) for `createdAt`;
  - the UTF-16 length-16 hash;
  - a SAFE integer ≥ 0 for `entryIndex`;
  - the strict bool;
  - the enum;
  - `actorCharacterId` REQUIRED-nullable;
  - `actorToolName` absent → `null`, non-string non-null → fail.
- `remap_wardrobe_history_rows(incoming, ctx)` — pure, with an injectable
  `mint_id`.
- `import_wardrobe_history(...)`:
  - item map via `build_imported_wardrobe_item_id_map(documents,
    &pairs(&id_maps.mount_points), &pairs(&id_maps.wardrobe_items))`
    (`wardrobe_wear.rs:245`);
  - the `Dropped … malformed outfit-history row(s).` warning;
  - the pair grouping with `\u{0}` keys in first-seen order;
  - `find_entries` per pair;
  - `upsert_rows(&to_write)` ALWAYS;
  - INFO `Imported outfit history` with the four keys in order.

**Reconcile** (`mod.rs:2026`): pass `build_imported_wardrobe_item_id_map(
documents, …)` as v4's 5th argument.

**`import_warning_text_guard`**
(`crates/quilltap-harness/tests/import_warning_text_guard.rs:269-308`, the
P4.D264 literal test). Add a sibling test, or extend the list, for the three
new literals, VERBATIM:

- `quilltap_import/wardrobe_history.rs`:
  `"Dropped {} malformed outfit-history row(s)."`
- `quilltap_import/mod.rs`:
  `"Failed to import the outfit history: {msg}"`
- `backup/restore/orchestrator.rs`:
  `"Failed to restore the outfit history: {error}"`

The general rule (no raw `{e}` on a `DbError`) already holds the tails to
`item_error_text` / `WarnText`. `MAIN_OFFENDERS` (103) is measured on an old
extract and does not move.

### §2.10 `.qtap` import — reconcile (bug 252)

`crates/quilltap-core/src/services/quilltap_import/reconcile.rs`:

- **The signature** (`:161-166`) gains `wardrobe_item_ids: &HashMap<String,
  String>` (or `&[(String,String)]`, as `build_imported_wardrobe_item_id_map`
  returns pairs).
- **The chat loop** (`:376-509`). Today it remaps participants (`:389-426`),
  tags (`:428-435`), project (`:437-440`) and scenario (`:442-494`), then
  calls `ChatsRepository::update` (`:496-502`). **It never touches the four
  columns: the defect, proven.**
- **Insert the keyed pass AFTER the participants block (`:426`) and BEFORE
  the tags (`:428`)**, at v4's position:
  - character: `id_maps.characters.get(id).unwrap_or(id)` (no drop);
  - item: the new map;
  - file: `id_maps.doc_mount_file_links.get(id).unwrap_or(id)`;
  - message: identity.
- **Logs:**
  - WARN `Chat character-keyed columns unreadable; left as imported`
    `{chatId: new_id, fields}`;
  - DEBUG `Remapped character-keyed chat fields` `{chatId: new_id, fields}`
    (no `droppedEntries`).
  - The module is `import:quilltap-import-service`, the same target as the
    file's other lines.
- **Set `has_updates` whenever the patch is non-empty.**
- **The chat is read through `chats_read::find_by_id`.** Its `put_opt_json`
  (`db/chats_read.rs:132-142`) DROPS an unparsable or `"null"` text column,
  where v4's parsed `ChatMetadata` may still hand the pass a string. See trap
  9: the unreadable arm can differ in reach between the two sides.
- **`db/chats.rs` `ChatUpdate` (`:611`) LACKS `pending_outfit_notifications`.**
  `equipped_outfit` (`:689`, `Option<Option<Value>>`), `courier_checkpoints`
  (`:771`) and `character_avatars` (`:775`) exist. Add the fourth, with its
  SQL arm. `ChatCreate` already has it (`:437`).
  - `db/chats.rs` is a shared file. Fence the hunk append-only and name it a
    HANDOFF if another lane in the round owns it. The Inventory round adds
    `equippedInventory` to the same list later.

### §2.11 Schema re-vendor

- `crates/quilltap-core/src/generators/qtap-export.schema.json` — byte copy
  from the pin (108,678 bytes).
- The size pins:
  - `generators/qtap_schema.rs:197`: `assert_eq!(QTAP_EXPORT_SCHEMA_JSON.len(),
    105_774)` → `108_678`, plus a history line in the comment block at
    `:164-196`.
  - `crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:24-78`: the
    size history plus the constant. The byte-equality leg reads
    `QT_V4_CHECKOUT`; point it at the pin worktree, never the live tree.
- `qtap_schema_validate_equivalence` regenerates: a chats bundle carrying
  `wardrobeHistory` turns `valid` on both sides; on the old schema it is
  invalid (`additionalProperties: false`).

---

## §3 Shared contract — what this lane reads from the keystone

The keystone (table + boot ensures + repository; the 2026-10-19 boot-created
ruling) must expose, in e.g. `crates/quilltap-core/src/db/chat_wardrobe_history.rs`:

### §3.1 The row

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatWardrobeHistoryRow {
    pub id: String,
    pub chat_id: String,
    pub character_id: String,
    pub entry_index: i64,
    pub slots: Slots,                       // serializes as the five canonical keys
    pub slots_hash: String,
    pub source: String,                     // or an enum; reads unknown → "seeded"
    pub actor_character_id: Option<String>, // serialize null, never skip
    pub actor_tool_name: Option<String>,    // serialize null, never skip
    pub is_current: bool,
    pub created_at: String,
}
```

**The FIELD DECLARATION ORDER IS A CONTRACT.** It is `normalizeRow`'s order,
which the backup file, the export's `data` and the `.qtap` import's comparand
all emit. **It is not the DDL column order**, where `actorToolName` is last.
The reads must reproduce `normalizeRow` (repository `:90-105`):

- `parseSlots`: `normalize_equipped_slots`, falling back to empty slots on a
  JSON error;
- `slotsHash` returned as STORED;
- `source` safe-parsed;
- `isCurrent = (n == 1)`.

### §3.2 Methods (v4 signatures at the pin)

- `async findAll(): Promise<Row[]>` — `ORDER BY "chatId", "characterId",
  "entryIndex" ASC`. Used by the backup.
- `async findByChatId(chatId): Promise<Row[]>` — `ORDER BY "characterId",
  "entryIndex" ASC`. Used by the export.
- `async findEntries(chatId, characterId): Promise<Row[]>` — `ORDER BY
  "entryIndex" ASC`. Used by 7f's never-merge check.
- `async upsertRows(rows: Row[]): Promise<number>` (`:330-361`):
  - per row: `slots = normalizeEquippedSlots(row.slots)`; then `INSERT … ON
    CONFLICT ("id") DO UPDATE SET` every non-key column (incl.
    `actorToolName`), with `insertParams({...row, slots, slotsHash:
    hashEquippedSlots(slots)})`. **The hash is RECOMPUTED.**
  - `insertParams` binds `actorCharacterId ?? null`, `actorToolName ?? null`
    and `isCurrent ? 1 : 0` (JS truthiness).
  - A throwing row (unique index, FK, bind error) logs WARN `Skipped a
    wardrobe history row that would break the timeline` `{ id, chatId,
    characterId, entryIndex, error }` and is skipped.
  - After the loop, DEBUG `Upserted wardrobe history rows` `{ requested,
    written }` (context module `wardrobe-history`).
  - The return value is `written`. **It never throws.**

**The carriers need TWO entry points** (ask the keystone for both, or for the
raw one with the typed one built on it):

- `upsert_rows(&[ChatWardrobeHistoryRow]) -> usize` — for 7f, where the rows
  are Zod-validated.
- **A raw-row form, `upsert_raw_rows(&[Value]) -> usize`, for restore.** v4
  passes the UNVALIDATED archive JSON straight to `upsertRows`, so the JS bind
  semantics must apply PER ROW:
  - `slots` salvaged;
  - `isCurrent` by JS truthiness;
  - `source` written AS GIVEN, with no enum check: a bogus string lands in
    the table and reads back `seeded`;
  - a value better-sqlite3 cannot bind, or a NOT NULL violation → that row's
    WARN-and-skip, not the whole step.
  - Contrast `wardrobe_wear_rows` (`orchestrator.rs:4467-4479`), which fails
    the WHOLE ledger on one bad row. That shape is wrong here.

The table and its two unique indexes (`idx_chat_wardrobe_history_entry`,
`idx_chat_wardrobe_history_current … WHERE "isCurrent" = 1`) and its FK
`"chatId" REFERENCES "chats"("id") ON DELETE CASCADE` must exist on every
booted instance and every v5 harness target. **Measure the shape against
v4's REAL migration** (the keystone's differential).

### §3.3 Shared helpers this lane consumes, not owns

- `wardrobe::hash_equipped_slots` (`wardrobe.rs:852`, already v4-exact).
- `wardrobe::normalize_equipped_slots` (`:726`).
- `WARDROBE_SLOT_TYPES` (`:31`).
- `quilltap_import::wardrobe_wear::build_imported_wardrobe_item_id_map`
  (`:245`).
- `backup::uuid_remap::build_wardrobe_item_id_remap` (`:532`).
- If the keystone also ports `bySlot`, use it; a five-key literal in
  canonical order is equivalent.

---

## §4 The differential plan

**General rules.**

- Every family regenerates **from the pin worktree** (`--v4 "$PIN"`, ledger
  §5.1) through `harness/tools/recipe_sweep.py --families a,b,c --v5w
  ~/source/quilltap-v5` (the spaceless path, memory
  `sweep-driver-spaced-path`). The commands are each test's own header
  recipe.
- **Red-first:** regenerate the oracle at the pin with NO port and record the
  red set, then port.
- **§5.2 discipline:** every fresh NDJSON is grepped for the new bytes; the
  markers are given per family.

### §4.1 `system_backup` — `system_backup_equivalence.rs` + `harness/oracle/cases/system-backup.test.ts`

- **At the pin with no plant, EVERY case fails on the v4 side**: `createBackup`
  throws `no such table: chat_wardrobe_history` (§1.1).
- **Step 1, red-first:** add a `plantWardrobeHistoryDdl` prelude to EVERY
  case. It runs `CHAT_WARDROBE_HISTORY_DDL` (v4's REAL
  `@/lib/database/backends/sqlite/chat-wardrobe-history-ddl`) on the copy. Do
  NOT use `ensureCollection`, and do NOT run the seed migration: it would plant
  entry-0 rows from every chat's `equippedOutfit` (trap 3).
- **The v5 side** runs the keystone's table ensure (not the seed) on its copy.
- **Expected red after step 1:** every case's tree gains
  `data/wardrobe-history.json` (`[]`) and `manifest.counts.wardrobeHistory:
  0`.
- **NEW case `backup_with_wardrobe_history`.** Plant rows through v4's REAL
  `upsertRows`:
  - a three-entry timeline for Lorian in chat `c1…01`, with the cursor on
    entry 1;
  - a `pascal` row with `actorToolName: 'dress_for_dinner'`;
  - an `operator` row with a non-null actor;
  - a `seeded` row with a null actor;
  - a second character's timeline in the same chat, so the `characterId`
    ordering shows;
  - a row in chat `c1…02`;
  - one row whose stored `slots` text is a four-key pre-hair bag. Plant it
    by raw SQL after the upsert. It reads back five keys, with the STORED
    hash, not a recomputed one.
- **Grep markers:** `grep -c '"actorToolName"'` and `grep -c
  'wardrobe-history.json'` > 0; `"wardrobeHistory": 7` (or the planted count)
  in the manifest.
- **Pin the absent-table behaviour as a recorded row** (§8.2): either v5
  answers `[]` where v4 throws, or both fail.

### §4.2 `backup_uuid_remap` — `backup_uuid_remap_equivalence.rs` + `backup-uuid-remap.test.ts` + `harness/oracle/fixtures/uuid-remap-corpus.json`

- This is a tier-1 exact family with a counter-mocked `randomUUID`, so there
  is no normalization.
- It is a **deliberate repo writer** (`recipe_sweep.py:173-180`): the oracle
  rewrites the committed corpus, and the `corpusSha256` restamps.
- **The `wide` case is built by `createBackup` over `system-data-*`**, so it
  needs the same DDL prelude or the generator throws.
- **Expected red-first at the pin:** EVERY case's output gains `wardrobeHistory:
  []`, the always-present return key.
- The corpus today carries ZERO occurrences of the four columns and of
  `wardrobeHistory` (grep-measured).
- **NEW hand cases**, each named — v4's own tests (§7) plus:
  - `outfit_history_timeline` — five entries, the cursor mid-timeline, a
    null actor, an actor, a `pascal` row with `actorToolName`, and a
    pre-`4ae20977f` row with no `actorToolName` key (it stays absent in the
    remap output).
  - `outfit_history_items` — a frontmatter item (stays), a path-derived item
    (recomputed against the remapped mount, built from a
    `docMountFileLinks` / `docMountDocuments` pair as in v4's test), and an
    unknown item (passes through).
  - `outfit_history_phantom_character` — a `characterId` NOT in
    `data.characters`. It is MINTED, so its counter id lands after the row's
    `id` and `chatId` (§8.4).
  - `outfit_history_slot_shapes` — a missing slot, a four-slot bag, a
    `null` `slots`, and a `slots` slot holding a non-string element (passes
    through).
  - `chat_keyed_bug252` — one chat carrying all four columns, with entries
    for a carried character, a NON-carried character (DROPPED,
    `droppedEntries`), an avatar `imageId` for a carried file (no mint) and
    for an unseen file (MINTS at the chat), a courier
    `lastResolvedMessageId` for one of the chat's own messages (MINTS before
    the participants: **the mint-order proof**), a four-slot outfit (gains
    `hair`), and a malformed slot.
  - `chat_keyed_edges` — one chat per edge:
    - a column as a JSON STRING (parsed);
    - `"null"` as a string (unreadable);
    - a number, an array, `true` (each unreadable);
    - `{}` (patch `{}`, DEBUG fires);
    - `null` and absent (skipped, the key untouched or absent);
    - an `equippedOutfit` entry that is a JSON string (becomes five empty
      slots);
    - a `characterAvatars` entry that is a JSON string with a string
      `imageId` (becomes an OBJECT);
    - an avatar entry whose `imageId` is a number (kept as given).
  - Keep `wide` (no columns there).
- **Capture the two `backup:uuid-remap` lines per case** in the oracle (a
  `logs` array; `fields` as the array) and compare them on the Rust side.
- **Marker:** `grep -c droppedEntries` > 0 and `grep -c
  wardrobe_history\|wardrobeHistory` > 0 in the NDJSON.

### §4.3 `system_restore` + `system_restore_state` — `system_restore_equivalence.rs`, `system_restore_state.rs`, `system-restore.test.ts`

**Preview.**

- Every case gains `wardrobeHistory` (43 keys): `0` for every committed
  archive, `N` for the new one.
- Add `preview_outfit_history` over the derived archive (§5).
- Marker: `"wardrobeHistory":` in every preview line.

**Restore state.**

- **Every case** gains:
  - the summary key;
  - the `Restored outfit history total=0 restored=0` DEBUG in the census;
  - the moved `Restore operation completed` line.
- **The target** is the oracle's fresh generateDDL target, which never
  renders the table.
  - Add a per-case `historyDdl` plant, mirroring `wearLedgerDdl`
    (`system-restore.test.ts:191`, used at `:654`): `CHAT_WARDROBE_HISTORY_DDL` on the
    TARGET.
  - Without it v4's 22n-ter fails EVERY row. Each row gets the repository
    WARN (`no such table`) and `restored: 0`.
  - The v5 target already carries the table via the keystone's ensures, so
    confirm that the harness's target provisioning runs them (§6 trap 2).
- **NEW cases over `restore-archive-outfit-history.zip`:**
  - `restore_outfit_history_replace`. Rows land with ARCHIVE ids. A second
    replace restore does not double them (the wipe list). The pre-hair /
    bogus-hash row is recomputed by `upsertRows`.
  - `restore_outfit_history_new_account`:
    - the history remapped;
    - the slots through the item map;
    - the bug-252 columns re-keyed on BOTH chats;
    - the phantom character's history row MINTED, and the chat-keyed entry
      DROPPED.
  - `restore_outfit_history_row_skips_replace`. The archive carries:
    - a second `isCurrent: true` row in one timeline (the unique cursor
      index fires);
    - a duplicate `(chat, character, entryIndex)`;
    - a row whose `chatId` names no archived chat (FK);
    - a row with `entryIndex: true` (bind failure);
    - a row with `source: "bogus"` (written; reads back `seeded`);
    - a row with `isCurrent: "false"` (truthy → 1 — choose its timeline so
      that this is the only cursor);
    - a row with no `actorToolName` key.
    - Each skip gets the repository WARN. `restored` < `total`, and the
      summary's `warnings` list is UNCHANGED (the per-row skips push nothing).
- **Census:** grow both `RESTORE_TS_MESSAGES` lists (§2.6). Add the repository
  lines to `REPO_LEVEL_MESSAGES` for the outfit-history cases.
- **Markers:** `grep -c 'Restored outfit history'` equals the number of
  restore cases; `grep -c 'Skipped a wardrobe history row'` > 0.
- **The existing `*_new_account` cases do NOT move for bug 252.** No
  committed archive carries the four columns in a new-account case:
  - `restore-archive-legacy.zip` has ONE legacy `equippedOutfit` on chat
    `c1…01`, but it restores in `replace` mode only (`restore_legacy_archive`,
    `restore_phase_warns_replace`).
  - Every other archive has none (measured).
  - The mint-order move is therefore invisible there. Only the new cases see
    it.
- **`restore_vintage_state`** consumes the keystone's boot-align list (memory
  note `vintage-restore-mirror-boot-list`; the keystone's obligation). Its
  replace / new-account runs gain the summary key and the DEBUG.
  - Re-measure it. A vintage target lacking the table would make 22n-ter
    skip rows; the archive it restores has none, so expect `0/0`.

### §4.4 `system_export` — `system_export_equivalence.rs` + `system-export.test.ts`

- Add a history plant to the chats-type cases, via the DDL prelude on the
  copy plus v4's REAL `upsertRows`.
- **Without the table**, v4's per-chat read throws: a WARN `Failed to load
  outfit history for export` per chat, plus a backend ERROR. The bytes are
  unchanged, but the lines differ if they are captured. Plant the DDL on
  every case or capture nothing.
- **Expect:**
  - `wardrobe_history` records after each chat's `chat_inform` records and
    before its `conversation_annotation` records;
  - the record `data` in `normalizeRow` order;
  - the footer `counts.wardrobeHistory` placed by the first bump;
  - a chat with no history emitting no record and no count key.
- **Negative rows:** `previewExport` carries no `wardrobeHistory`, and no
  non-chats export type emits the kind.
- **Marker:** `grep -c '"kind":"wardrobe_history"'` > 0.

### §4.5 `system_import` (assembly) — `system_import_equivalence.rs` + `system-import.test.ts`

- A chats bundle carrying `wardrobe_history` records assembles
  `data.wardrobeHistory` LAST in the chats arm.
- v5 today SILENTLY drops it (`ndjson.rs:623`): **red-first proof**.
- **Marker:** `grep -c wardrobeHistory` > 0.

### §4.6 `system_import_state` (execute) — `system_import_state.rs` + `system-import-execute.test.ts`

The tier-2 state family. It emits all tables, so the target needs the table
on BOTH sides: v4 via the DDL prelude, v5 via the ensures. Otherwise the
table set differs (trap 2).

**Phase 7f cases** (a `chats` export of a planted copy, merged with
characters + document stores so `idMaps.characters` and the item map are
populated):

- fresh ids, matched by first encounter through the minted-id remap (§6
  trap 4);
- remapped chat, character and actor;
- a path-derived item remapped;
- `actorToolName` copied;
- a malformed row: `Dropped 1 malformed outfit-history row(s).` (verbatim);
- a row with a non-UUID slot element (dropped as malformed);
- **`execute_twice`**: the second import skips every timeline
  (`skippedTimelines` = the pair count, written 0);
- an interleaved-pairs input, so the write order differs from the mint order;
- a duplicate-chat skip whose source chat id names a LOCAL chat with an empty
  timeline (the never-merge hole, §1.10, written — pinned as v4's);
- a planted absent table on the target (the 7f catch: `Failed to import the
  outfit history: no such table: chat_wardrobe_history`).

**Bug-252 cases.** A merged bundle whose chat carries:

- all four columns keyed by bundle characters that import with FRESH ids;
- an avatar `imageId` naming a bundled doc-store file link;
- a character the bundle did not carry (kept);
- a column stored unreadable, if reachable on both sides (trap 9).

**Lists to grow:**

- `IMPORT_WARN_MESSAGES` (TS `:2729`, Rust `:987`) + `Failed to
  import outfit history`.
- `WARDROBE_LOG_MESSAGES` (TS `:2749`, Rust `:1017`) + `Imported
  outfit history`. Also the repository's `Upserted wardrobe history rows` /
  `Skipped a wardrobe history row that would break the timeline` if the lane
  compares them.
- `CARRIER_LOG_MESSAGES` (TS `:2773`) + `Remapped character-keyed chat
  fields` and `Chat character-keyed columns unreadable; left as imported`.
  `carrierLine` renders `fields=` through `String(v)`, i.e. comma-joined
  (trap 10).

**Re-measure** the P4.D294 `remintBundle` cases (`:2405-2480`, used at
`:3979-4090`). Their `courierCheckpoints` is keyed by `P4D294_LORIAN` in a
CHATS-ONLY bundle, so `idMaps.characters` is empty: the re-key is identity,
but `hasUpdates` now fires the `chats.update`. Expect identical state, and the
new DEBUG line where `CARRIER_LOG_MESSAGES` records it. The ledger's "moves
on regen" is a prediction; measure it.

**Markers:** `grep -c 'Imported outfit history'` > 0; `grep -c 'Remapped
character-keyed chat fields'` > 0.

### §4.7 Families that move only by inclusion

- **`system_delete_data_equivalence`** — the truncate list (§1.4). With the
  table planted and a row in it, delete-all clears it. Without the table,
  nothing observable changes. Add one row.
- **`qtap_import_equivalence`** (seed `lorian-and-riya.qtap`, a characters
  bundle). Its five committed fixture bundles carry no chats and none of the
  four columns (measured), so **it does not move**. Re-run it as a
  regression. A new chats-bearing bundle is optional (§5).
- **`qtap_schema_validate_equivalence`** + **`qtap_schema_embed_guard`** —
  §2.11.
- **`chat_export_equivalence`** (the chat GET `?action=export` JSONL / Markdown
  and `outfit-summary`) has **no hunk in the span** (`app/api/v1/chats/[id]/
  handlers` untouched). Re-run it as a regression only.

### §4.8 NEW tier-1 family for the pure bug-252 pass

`chat_keyed_fields_equivalence`, with an oracle case
`harness/oracle/cases/character-keyed-fields.ts` (tsx). It drives v4's REAL
`remapCharacterKeyedChatFields` over a corpus. Remaps are expressed as
lookup tables plus a "null for these ids" set, with a counter for
mint-on-miss.

- **Corpus:** every v4 test row of `character-keyed-fields.test.ts` plus the
  edges of §4.2.
- **Emit:** `{patch, unreadable, droppedEntries}` exactly. The patch key order
  is part of the comparand: compare `serde_json::to_string` bytes (memory
  note `serde-json-map-eq-ignores-order`).

Optionally add a tier-1 for `remapWardrobeHistoryRows` with a counter
`mintId`. It is pure and covers the Zod whole-row port cheaply. This mirrors
`wardrobe_wear_import_remap_equivalence` + `wardrobe-wear-import-remap.ts`.

---

## §5 Fixtures

- **No committed fixture carries outfit history.** None of the
  restore-archives, the `system-data-*` family, `chat-dialogs-*`, the five
  `harness/oracle/fixtures/*.qtap` or `lorian-and-riya.qtap` does.
- **None carries the four character-keyed columns in a reachable mode**, except
  `restore-archive-legacy.zip` (replace only). Measured: `system-data.json`
  and `chat-dialogs-web.json` hold 0 occurrences of each column; the archives
  were scanned through `data/chats.json`.
- **DERIVE, never rebuild** (the P4.D264 rule — a rebuild would add
  `data/wardrobe-history.json` to every archive):
  - NEW `crates/quilltap-web/tests/fixtures/restore-archives/restore-archive-outfit-history.zip`,
    derived from `restore-archive.zip` by a NEW
    `harness/oracle/fixtures/derive-restore-archive-outfit-history.py` on
    `restore_archive_derive.py`. The shape is
    `derive-restore-archive-wardrobe.py`.
    - Append `data/wardrobe-history.json` LAST, with `counts.wardrobeHistory`
      LAST.
    - The archive has characters `a1…01` (Lorian) and `a1…02` (Riya), chats
      `c1…01` (Lorian, messages `d1…01`/`d1…02`) and `c1…02` (Riya), file
      `f0000001-…0001`, and the coat item `ac000000-0000-4000-8000-000000000001`.
    - Plant:
      - the timelines;
      - the phantom-character row;
      - on `c1…01`: `equippedOutfit` (Lorian + a phantom), `characterAvatars`
        (Lorian → `f0000001…`), `courierCheckpoints` (Lorian → `d1…02`) and
        `pendingOutfitNotifications`.
  - NEW `restore-archive-outfit-history-skips.zip` — the malformed-rows
    variant (§4.3).
- **`system-data-*`:** never rewritten. Grow it per-regen on a COPY:
  - v4 side: `CHAT_WARDROBE_HISTORY_DDL` plus `upsertRows`, and for bug 252
    a raw `UPDATE chats SET …` of the four columns;
  - v5 side: the keystone ensure plus the keystone upsert, through the same
    plant SQL.
- **`uuid-remap-corpus.json`:** REGENERATED by its oracle (a committed, hashed
  deliberate repo-writer).
- **Optional:** a derived `harness/oracle/fixtures/qtap-import-outfit-history.qtap`
  (a chats bundle from v4's REAL exporter over a planted copy) for
  `qtap_import`. Not required if §4.6 covers 7f.
- **Who regenerates:** this lane regenerates every family above from the pin.
  - The `restore_vintage_state` boot-align and the `migration-vintage/` trio
    are the KEYSTONE's.
  - If the lanes run in parallel, stage in-lane copies under
    `/tmp/<lane>/…` (memory `parallel-lane-shared-resources`).

---

## §6 Traps

1. **v4's absent-table behaviour differs per carrier.** The repository is
   `rawQuery`, not the base repository's lazy `ensureCollection` +
   `safeQuery` (contrast P4.D264's wear ledger, R-G in `collect.rs:20-30`).
   - **Backup THROWS whole.**
   - **Export WARNs per chat.**
   - The **restore** replace-wipe is caught; per-row upserts skip.
   - **Import 7f** fails into its catch.
   - Every throw also logs backend ERROR `Raw query failed {query, error}`.
   - Every v4 oracle case must plant the DDL or its outputs change for the
     wrong reason.
2. **Table presence must MATCH on both sides of a state dump.** v4's
   generateDDL never renders this table; the v5 keystone creates it at boot.
   A `system_import_state` / `system_restore_state` table set will differ
   unless the v4 target gets the DDL. That is the `wearLedgerDdl` precedent;
   here plant it on every case, not just the outfit cases.
3. **Do not run v4's seed migration in a plant.**
   `seed-chat-wardrobe-history-v1` writes entry-0 rows from every chat's
   `equippedOutfit`. Plant the DDL (or the table + tool-name migrations) only.
   The keystone's seed ensure likewise must not run on harness copies, or the
   two sides must both run it.
4. **Fresh ids on import** (`randomUUID` per valid row, in input order).
   Diff through the minted-id remap by first encounter (memory
   `shared-fixture-rebuilds-and-jest-setup-mocks`: never `ORDER BY` a minted
   id). Write order ≠ mint order when pairs interleave (§1.10), so dump by
   `(chatId, characterId, entryIndex)`.
5. **`slotsHash` recomputation has three behaviours:**
   - `findAll` / `findByChatId` return the STORED hash;
   - `remapBackupData` and `remapWardrobeHistoryRows` recompute it from the
     remapped slots;
   - `upsertRows` recomputes it AGAIN from the normalized slots.
   - So a backup can carry a stale hash, and the restored row never does.
   - The import's Zod gate checks only the LENGTH (16), never the value.
6. **No normalize in the restore remap's slots.** `bySlot((slot) =>
   (row.slots?.[slot] ?? []).map(…))` on a non-array slot value throws
   `.map is not a function`. That aborts the WHOLE `remapBackupData`, so a
   new-account restore of such an archive fails, while `replace` mode
   salvages it in `upsertRows`.
   - Port faithfully only if a comparand reaches it, else record it. **Ask**
     (§8.5).
7. **Per-row WARN-and-skip, not all-or-nothing.** 22n-ter's `restored` is
   `upsertRows`' count. Copying 22n-bis's shape (`restored = rows.len()` or
   0) is the likely port error. The per-row skips add NOTHING to
   `summary.warnings`.
8. **JS bind semantics in the raw restore upsert** (§3.2).
   - `isCurrent: "false"` → 1.
   - `source` is not validated.
   - better-sqlite3 refuses booleans, objects and `undefined` as bind
     values. Measure each planted edge against the oracle rather than
     guessing the error text. The text appears only in the WARN's `error`
     field.
9. **`asObject` is `JSON.parse`.**
   - A V8-acceptable string that `serde_json` refuses (a lone surrogate) is
     the known gap (`jsstr.rs` class). Record it.
   - Separately, v5's `chats_read::put_opt_json` DROPS an unparsable column
     on read, so the import reconcile's "unreadable" arm may be reachable on
     v4 (a column read back as a raw string) and not on v5. **Measure before
     promising the WARN on the import side.** On the restore side both see
     the raw archive value.
10. **Log field rendering.**
    - `fields` is an ARRAY on both uuid-remap / reconcile lines.
    - The system-import-execute `carrierLine` renders `String(array)`, i.e.
      comma-joined without brackets.
    - The restore census renders through the `…Json` convention.
    - `chatId` is the ARCHIVE id in the restore line and the NEW id in the
      import line.
11. **Restore drops; import keeps.**
    - Restore's character remap returns `null` for a character not in the
      backup, and the ENTRY is dropped.
    - Import keeps the unmapped id.
    - The history arm in restore MINTS a phantom for the same case.
    - Three behaviours for "unknown character": do not unify them.
12. **Mint order** (tier-1 counter ids; S2-U1 survey trap 2). The bug-252 pass
    sits AFTER `scenarioSource` and BEFORE the participant-JSON lists.
    Courier message ids and unseen avatar file ids mint there. The four
    columns are walked in `CHARACTER_KEYED_CHAT_FIELDS` order, entries in
    object order, the character remap before the value remap.
13. **The `Object.assign` key position.** Patch onto existing keys only
    (`IndexMap::insert` on an existing key keeps its slot). A column that is
    `null` stays `null`. An absent one stays absent: v4's test "leaves a chat
    with no outfit alone" checks both.
14. **The census is a whitelist** (memory `restore-census-whitelist`). The
    two new `restore.ts` lines are invisible to the fresh oracle until BOTH
    the TS and the Rust lists grow. A `grep -c 'Restored outfit history'` of
    0 means the list, not a stale regen.
15. **`vintage-restore-mirror-boot-list`:** if the keystone's ensures are not
    in `restore_vintage_state`'s boot-align, 22n-ter on the vintage instance
    fails every row with `no such table`. That is the keystone's obligation;
    this lane must SEE the arm green.
16. **`strict-export-vs-v4-fallback-reads`:** NOT this shape. v4's history read
    is not a fallback base read, so its WARN genuinely fires on a throwing
    read. v5's strict export scope must still WARN-and-continue, not abort
    the export: a history read failure is not a corrupt chat row.
17. **Absent `actorToolName` at three layers:**
    - an old (`7953da3a0`-vintage) archive row with no key restores as
      `null` (`?? null`);
    - an old bundle row passes Zod (default `null`);
    - the new-account remap keeps the key ABSENT in its output (it does not
      add it). The tier-1 comparand sees the absence.
18. **Shared `/tmp` and the two-gate cap** (memory
    `parallel-lane-shared-resources`). The restore and system-data recipes
    write `/tmp/qt-*` paths shared across lanes; stage copies per lane.
19. **`pdf`-style IMPORT-DEAD census:** none. This lane imports v4 modules
    that EXIST at the pin (`chat-wardrobe-history-ddl`, `character-keyed-
    fields`, `import-wardrobe-history`). Name them in the oracle headers.

---

## §7 v4 tests to mirror (at the pin)

- **`__tests__/unit/lib/backup/uuid-remapper.test.ts`:**
  - `remapBackupData() - outfit history` (`:1126`): "round-trips a five-entry
    timeline with its indices, hashes and cursor under remapped ids".
  - `remapBackupData() - chat equipped outfit (bug 252)` (`:1179`):
    - "re-keys every character to its new id";
    - "moves slot item ids with the wardrobe item remap" (frontmatter id
      kept, path-derived id recomputed against the remapped mount);
    - "agrees with the outfit history on the character and item ids";
    - "leaves a chat with no outfit alone" (`null` stays `null`, `undefined`
      stays absent).
  - `… - other character-keyed chat columns (bug 252)` (`:1292`):
    - "re-keys outfit avatars and follows the picture to its new file id";
    - "re-keys Courier checkpoints and follows the message to its new id";
    - "drops an entry for a character the backup does not carry instead of
      minting an id";
    - "re-keys queued outfit notices".
  - → corpus rows in §4.2.
- **`__tests__/unit/lib/chat/character-keyed-fields.test.ts`** (`:26-97`): "lists
  the four keyed columns"; "re-keys the outfit and moves slot item ids,
  keeping unknown ones"; "fills a pre-hair four-slot bag out to five slots";
  "repairs a malformed slot instead of copying it"; "re-keys outfit avatars
  and moves their image id"; "re-keys Courier checkpoints and moves their
  message id"; "re-keys queued outfit notices and keeps their values"; "drops
  the entry of a character the remap cannot place, and counts it"; "reads a
  column stored as a JSON string"; "skips null or missing columns and reports
  unreadable ones". → §4.8.
- **`__tests__/unit/lib/import/quilltap-import/reconcile-character-keyed-fields.test.ts`**
  (`:68-108`): "re-keys the outfit to the imported character and moves item
  ids"; "re-keys outfit avatars and follows the picture through the link
  map"; "re-keys Courier checkpoints, keeping the already-reminted message
  id"; "keeps a character the bundle did not carry under its own id". → §4.6.
- **`__tests__/unit/lib/export/wardrobe-history-roundtrip.test.ts`**
  (`:106-191`): "re-imports with the same indices, hashes and cursor under
  remapped ids"; "remaps slot item ids that imported with new ids, and keeps
  the rest"; "drops a malformed row and counts it"; "emits nothing for a chat
  with no history". → §4.4 / §4.6 and the optional 7f tier-1.
- **`__tests__/unit/lib/backup/wardrobe-wear-backup.test.ts`:**
  - "writes data/wardrobe-history.json with every timeline, cursor included"
    (`:234`);
  - "writes the outfit history back whole and counts it" (`:328`);
  - "writes no history for an archive that predates it" (`:339`).
  - → §4.1 / §4.3.
- Repository-level (the keystone's, but the carriers depend on them):
  `chat-wardrobe-history.repository.test.ts` (the `upsertRows` skip rule and
  `actorToolName` round trip).

---

## §8 Open questions and ledger disagreements

1. **Ledger disagreement — the reconcile position.** The `e8e17b2d9` ledger
   row says the import pass sits "after the scenario-source re-point, before
   the tags". **At the pin it sits AFTER the participants and BEFORE the
   tags; the scenario re-point comes after `projectId`** (`reconcile.ts:418-442`
   vs `:462+`). Port v4's position: after `reconcile.rs:426`. It is
   unobservable in state, but it orders the DEBUG lines (the scenario DEBUG
   follows the keyed DEBUG).
2. **The backup on an absent table (ruling owed, small).** v4's `createBackup`
   THROWS. Every booted v5 instance has the table (the keystone's ensure), so
   this is reachable only on an un-booted harness copy. Two options:
   - (a) answer `[]` (the P4.D264 R-G precedent; recorded divergence,
     unreachable in production);
   - (b) propagate.
   Recommend (a), pinned both ways by one row.
3. **22n-ter's outer catch is unreachable** through the real repository (it
   never throws). Port the arm and its two strings for the census, and record
   it as unreachable with the reason (the P4.158 table).
4. **v4's inconsistency on unknown characters.**
   - The history remap MINTS a phantom `characterId` (and `actorCharacterId`).
   - Bug 252 deliberately DROPS the chat-keyed entry.
   - The phantom row still inserts: `characterId` has no FK, and only
     `chatId` is FK'd.
   - Port it as v4 (the stated rule is "v5 fixes v4's restore bugs" — the
     2026-07-25 / 2026-08-03 standing ruling), or file it? **Ask the human.**
     It is a candidate v4 bug filing, not a silent divergence.
5. **The restore remap throws on a non-array slot** (trap 6). It is a v4
   crash on a hand-edited archive. Port faithfully (the whole new-account
   restore fails) or fix (normalize first)? This falls under the same
   standing ruling. **Ask**; recommend: record and file, port faithfully
   unless the human says fix.
6. **The never-merge hole** (§1.10). History can be written into a LOCAL
   chat that has the same id as a skipped-duplicate bundle chat, when the
   local timeline is empty. Port it faithfully and record it. A v4 filing is
   possible.
7. **v5's unknown-kind arm is silent** (`ndjson.rs:623`), and its own comment
   says v4 warns. v4's WARN `Unknown NDJSON record kind — skipping {kind}`
   has no v5 twin. This is pre-existing, out of this slice and outside
   history once the arm exists. Flag it for the follow-ups smalls round.
8. **`ChatUpdate` lacks `pending_outfit_notifications`** (confirmed). Who owns
   `db/chats.rs` this round? If the keystone or another lane touches the file,
   make this an append-only HANDOFF.
9. **The ledger's "the P4.D294 remint plant … moves on regen" is a
   prediction.** The bundle is chats-only, so the re-key is identity. Expect
   identical state plus the new DEBUG. Measure it.
10. **The tracing target for `backup:uuid-remap`.** This is a new module
    target in v5. Pick the house spelling and make the restore census's
    target check (`system_restore_state.rs:6304-6316`) exclude these lines:
    they are not `restore.ts` lines.
11. **Help/docs.** The span's `help/` and `docs/v4/` edits (spec, `wardrobe.md`)
    are the vendor lane's. No carrier text appears in `help/` at the pin
    (`7953da3a0` touches no `help/` file; `README.md` / CHANGELOG only).
