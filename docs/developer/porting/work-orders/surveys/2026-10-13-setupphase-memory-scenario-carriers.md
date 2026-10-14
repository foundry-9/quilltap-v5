# Survey — the CARRIERS (backup / restore / `.qtap` export / `.qtap` import) across `4a38222e6..64743ee13`

Read-only planning survey, 2026-10-13. v4 = `~/source/quilltap-server` read
ONLY through `git show 64743ee13:<path>` (the PIN, `4.10.0-dev.175`),
`git diff 4a38222e6 64743ee13 -- <paths>` (the span) and `git show <sha> --
<paths>`; v5 = `main` at `a7c6f0d03`. **Every v4 line number below is AT THE PIN
`64743ee13`** unless marked `(pre)` (= `4a38222e6`, the baseline v5 ported).
Every v5 line number is on `a7c6f0d03`. Nothing here comes from commit prose
(ledger §5.3); the drift ledger's §1 (incl. LIVE-DATA HAZARDS + Schema / D23)
and the §3 rows for `8050958be`, `545e299bb`, `c2795733f`, `e0ac7493a`,
`af48f6cc2`, `ac6348c5f`, `93c5af6ad`, `4ae0c4ca6`, `64743ee13` were read first
and are built on, not restated.

⚠ **Checkout note:** at survey time the v4 working tree is NOT clean — nine
modified files (`app/api/v1/chats/[id]/actions/{impersonation-voice-preview,
inform,rebuild-summary,story-background}.ts`, `…/handlers/get.ts`,
`connection-profiles/[id]/route.ts`, `memories/route.ts`, `plugins/[name]/
route.ts`, `lib/mount-index/scenario-item-route-factory.ts`). None is a carrier
file and every read here went through `git show <sha>:`, so this survey is
unaffected — but the brief's "clean" premise no longer holds; every regen must
come from a pinned worktree (§5.1) regardless.

**Scope (this slice):** `lib/backup/**` (backup-service, types, restore/
{archive, preview, restore, uuid-remap, delete-service}), `lib/export/
{ndjson-writer,types}.ts`, `lib/import/quilltap-import/{execute,import-entities,
import-files,reconcile,types,message-id-remap}.ts`, the three carrier helper
modules the span adds (`lib/memory/extraction/cursor-bundle.ts`,
`lib/memory/settings-legacy.ts`, `lib/scenarios/source-remap.ts`),
`public/schemas/qtap-export.schema.json`, and the carrier-visible shape of the
rows they move (chat / memory / tfidf / chat-settings / instance-settings).
**Out of scope** (sibling surveys, recorded here only as NEEDS): the cursor
repository + table ensure, the vector-table retirement ensure, the settings
move ensure, the D23 re-dump, `retireMemories`, the TF-IDF refit policy, the
scenario browser / `resolveScenarioSelection`, the reconcile-embedding-
dimensions pass, the SillyTavern import's extraction enqueue.

---

## 0. The span for the carriers, in one paragraph

Nine of the nineteen commits touch the carriers. `545e299bb` adds
`chats.scenarioSource` / `scenarioNotes` and teaches the new-account restore
(`uuid-remap.ts`) and the `.qtap` import reconcile (`reconcile.ts`) to re-point
`scenarioSource.mountPointId` through ONE new pure helper
(`lib/scenarios/source-remap.ts`), plus two schema keys. `8050958be` (bug 228)
makes the `duplicate` and fresh-id chat import paths ONE, REMINTS message ids
for every chat created under a fresh id through a NEW `message-id-remap.ts`, and
re-points every sidecar that names a message (informs, annotations, memories,
file `linkedTo`) through a per-chat `idMaps.messages`; the chat duplicate's map
entry stops being a phantom. `c2795733f` carries `episodeExtractionWatermark
MessageId` (remap + remint) and memory `supersededAt` (bug 225) on import, and
adds both to the schema. `e0ac7493a` translates a pre-Phase-A backup's
`chat_settings.autoHousekeepingSettings` into `instance_settings.memoryMaintenance`
on restore and drops the dead `memoryExtractionConcurrency` key (NEW
`settings-legacy.ts`). `af48f6cc2` (Phase B) stops backups writing
`vector-entries.json` / `vector-index-metas.json` (manifest counts, compact
list, staging, collect all lose them), deletes restore step 22j, keeps reading
a LEGACY `vector-entries.json` only to fill a memory that arrived WITHOUT an
embedding (`legacyVectorsAdopted`), swaps the replace-restore memory wipe onto
`retireMemories(…, {reason:'restore-replace'})`, drops `vector_entries` /
`vector_indices` from the replace-truncate list and the reconcile summary's two
vector counters. `8af764514` touches only a test mock and a comment.
`ac6348c5f` (Phase D) moves the cursors off the chat row: backup / `.qtap`
export APPEND them at the end of each chat under the legacy key names through
NEW `cursor-bundle.ts`, restore / import STRIP and write them to
`memory_extraction_cursors`, a replace restore truncates that table.
`93c5af6ad` (bug 240) lets a chats-only `.qtap`'s memories resolve to a
character the instance already holds. `4ae0c4ca6` forces the import's BUILTIN
refit (`scheduleRefit(…, {force:true})`) and gives `tfidf_vocabularies` a
`textShapeVersion` the backup / restore carry implicitly. `64743ee13` adds the
third cursor key, `turnExtractionWatermarkMessageId`, everywhere the other two
go. `104e986c7` / `b045dd986` / `48790fb73` / `9ec4fa2e9` / `124c8aca3` /
`69d38dd60` / `584d28164` / `97f0d8cb8` / `4ce64fa1e` touch no carrier file.

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `lib/memory/extraction/cursor-bundle.ts` (124 lines; NEW in `ac6348c5f`, `turn` added `64743ee13`)

Module logger `logger.child({ module: 'memory:extraction:cursor-bundle' })` (`:26`).

```ts
export const BUNDLE_CURSOR_FIELDS = {          // :29-33 — ORDER IS THE CARRIER KEY ORDER
  'other-fold': 'otherExtractionWatermarkMessageId',
  'episode-fold': 'episodeExtractionWatermarkMessageId',
  turn: 'turnExtractionWatermarkMessageId',
} as const satisfies Record<ExtractionCursorPass, string>
export type BundleCursorFields = Partial<Record<BundleCursorField, string | null>>   // :38
```

- `cursorsToBundleFields(cursors: ExtractionCursorMap): BundleCursorFields`
  (`:43-50`) — walks ENTRIES in the order above; a field is set ONLY when the
  pass's messageId is truthy (**absent, never null, when no cursor**).
- `loadBundleCursors(): Promise<ReadonlyMap<string, ExtractionCursorMap> | null>`
  (`:56-66`) — `repos.memoryExtractionCursors.findAllByChat()`; `instanceof
  Map ? byChat : null`; on throw **WARN** `'[ExtractionCursors] Could not read
  the cursor table for a bundle; reading per chat'` `{error}` → `null`.
- `withBundleCursorFields<T extends {id}>(chat, preloaded?)` (`:74-89`) —
  `preloaded` truthy → `{ ...chat, ...cursorsToBundleFields(preloaded.get(chat.id)
  ?? {}) }` (cursor keys land AFTER every chat key); else one
  `findByChatId(chat.id)`; on throw **WARN** `'[ExtractionCursors] Could not
  read a chat\'s cursors for a bundle; leaving them out'` `{chatId, error}` →
  `{ ...chat }`. Never fails the writer.
- `takeBundleCursors<T>(chat): { chat: Omit<T, BundleCursorField>; cursors }`
  (`:95-104`) — copies, DELETES all three keys unconditionally, and keeps a
  cursor only for `typeof value === 'string' && value.length > 0` ("Blank
  values are not cursors"; `null`, `""`, non-strings dropped).
- `restoreBundleCursors(chatId, cursors): Promise<number>` (`:107-124`) — for
  each `[pass, messageId]` of `Object.entries(cursors)` (i.e. insertion order
  other → episode → turn), skip falsy, `setCursor(chatId, pass, messageId)`;
  on throw **WARN** `'[ExtractionCursors] Could not restore a bundled cursor;
  the pass will re-read'` `{chatId, pass, error}` and continue; after the loop,
  when `written > 0`, **DEBUG** `'[ExtractionCursors] Restored bundled cursors'`
  `{chatId, written}`. Best-effort, never throws.

The repository it calls (keystone — NOT this slice, quoted for the contract):
`lib/database/repositories/memory-extraction-cursors.repository.ts` (151 lines).
`setCursor` (`:102-120`) is a raw upsert
`INSERT INTO "memory_extraction_cursors" ("id","chatId","pass","messageId","createdAt","updatedAt") VALUES (?,?,?,?,?,?) ON CONFLICT("id") DO UPDATE SET "messageId" = excluded."messageId", "updatedAt" = excluded."updatedAt"`
with `id = \`${chatId}:${pass}\`` (`extractionCursorId`), preceded by
`getCollection()` (lazy ensure of the generateDDL shape) and followed by
**DEBUG** `'[ExtractionCursors] Cursor set'` `{chatId, pass, messageId}` on the
ROOT logger. `findByChatId` / `findAllByChat` are `safeQuery` with NO default —
they **throw** on failure (header `:9-11`). `ExtractionCursorPassEnum =
z.enum(['other-fold','episode-fold','turn'])`
(`lib/schemas/memory-extraction-cursor.types.ts:22`); `ExtractionCursorMap =
Partial<Record<ExtractionCursorPass, string>>` (`:48`).

### 1.2 `lib/memory/settings-legacy.ts` (91 lines; NEW in `e0ac7493a`, unchanged since)

Pure; shared by the keystone's `move-memory-maintenance-settings-v1` and the
restore.

- `legacyHousekeepingToMaintenance(raw: unknown): {enabled, perCharacterCap,
  perCharacterCapOverrides} | null` (`:29-61`): a string is `JSON.parse`d
  (parse error → `null`); non-object / array / null → `null`; `clampCap(n)` =
  `typeof n === 'number' ? n : Number(n)`, non-finite → `null`, else
  `min(CAP_MAX, max(CAP_MIN, floor(num)))` with `MEMORY_MAINTENANCE_CAP_MIN =
  100`, `MEMORY_MAINTENANCE_CAP_MAX = 100_000` (`settings.types.ts:135-136`);
  overrides kept only when `perCharacterCapOverrides` is an object, each value
  clamped, unclampable entries DROPPED; `enabled: v.enabled === true`;
  `perCharacterCap: clampCap(v.perCharacterCap) ?? DEFAULT_CAP` where
  `DEFAULT_CAP = MemoryMaintenanceSettingsSchema.parse({}).perCharacterCap` (=
  2000). Object key order of the result: `enabled`, `perCharacterCap`,
  `perCharacterCapOverrides` (no `phases` — that defaults at read time via
  `.prefault({})`).
- `DEAD_INSTANCE_SETTING_KEYS = new Set(['memoryExtractionConcurrency'])` (`:65`);
  `SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff'` (`:67`).
- `withMemoryMaintenanceFromLegacy(instanceSettings, chatSettings)` (`:77-91`):
  `rows = instanceSettings.filter(!dead)`; if any row's key is
  `'memoryMaintenance'` → return `rows` (the backup's own key wins); else
  `candidates = chatSettings.filter(s => s && typeof s === 'object' &&
  'autoHousekeepingSettings' in s)` (KEY PRESENCE — a `null` value qualifies),
  `source = candidates.find(s => s.userId === SINGLE_USER_ID) ?? candidates[0]`,
  `maintenance = legacyHousekeepingToMaintenance(source.autoHousekeepingSettings)`;
  `null` → `rows`; else `[...rows, { key: 'memoryMaintenance', value:
  JSON.stringify(maintenance) }]` (APPENDED LAST). No log line.

### 1.3 `lib/scenarios/source-remap.ts` (48 lines; NEW in `545e299bb`)

Client-safe, pure.

- `readScenarioSource(value: unknown): ScenarioSource | null` (`:21-33`):
  `null`/`undefined`/`''` → `null`; a string is `JSON.parse`d (error → `null`);
  `ScenarioSourceSchema.safeParse` → `.data` or `null`.
- `remapScenarioSource(value, mapMount: (id) => string | null)` (`:40-48`):
  `readScenarioSource` → `null` stays `null`; `mapMount(source.mountPointId)`
  falsy → `null`; else `{ ...source, mountPointId }` (the PARSED object — key
  order `mountPointId, path, bodySha256`, unknown keys stripped by Zod).
- `ScenarioSourceSchema` (`lib/schemas/chat.types.ts:795-801`):
  `{ mountPointId: UUIDSchema, path: z.string().min(1).max(500), bodySha256:
  z.string().length(64) }` — note Zod checks LENGTH only (JS UTF-16 length);
  the JSON schema (§1.11) additionally demands `^[0-9a-f]{64}$`.

### 1.4 `lib/import/quilltap-import/message-id-remap.ts` (178 lines; NEW in `8050958be`, widened `c2795733f` / `ac6348c5f` / `64743ee13`)

- `remintChatMessageIds(chat, messages, mintId = randomUUID): RemintedChatMessages`
  (`:97-159`), PURE (injectable minter — a tier-1 seam):
  1. `newIds = messages.map(m => { const id = mintId(); if (m.id &&
     !idMap.has(m.id)) idMap.set(m.id, id); return id })` — every occurrence
     gets its own id; a repeated source id maps to its FIRST occurrence; an
     id-less message still gets a minted id (`:102-107`).
  2. `reminted = messages.map((m, i) => ({ ...m, id: newIds[i], ...(m.swipeGroupId
     ? { swipeGroupId: remapSwipeGroupId(m.swipeGroupId, idMap) } : {}) }))`
     (`:109-113`); `remapSwipeGroupId` (`:71-75`) rewrites only `swipe-<id>`
     whose `<id>` is in the map; any other key (SillyTavern `swipe-<index>`, a
     bare id) is kept.
  3. `summaryAnchorMessageIds` (only when `Array.isArray`): all resolve →
     `chatPatch.summaryAnchorMessageIds = mapped`; ANY unresolved →
     `droppedRefs += unresolved; summaryCleared = true; chatPatch.contextSummary
     = null; summaryAnchorMessageIds = []; lastSummaryTurn = 0` (`:119-131`).
  4. For each of the three `BUNDLE_CURSOR_FIELDS` values in order: skip falsy;
     `mapped = idMap.get(w) ?? null`; unresolved → `droppedRefs++`;
     `chatPatch[field] = mapped` (may be `null`) (`:133-139`).
  5. `courierCheckpoints != null` → `readJsonObject` (object, or JSON string,
     else `null`) → per `[characterId, raw]`: checkpoint unreadable or its
     `lastResolvedMessageId` unmapped → `droppedRefs++`, dropped; else
     `{ ...checkpoint, lastResolvedMessageId: mapped }`; whole value unreadable
     → `droppedRefs++`; `chatPatch.courierCheckpoints = Object.keys(next).length
     > 0 ? next : null` (`:141-156`).
  Returns `{ messages, chatPatch, idMap, droppedRefs, summaryCleared }`;
  `chatPatch` carries ONLY fields present on the source chat.
- `remapImportedMessageId(idMaps, sourceChatId, messageId): string | null`
  (`:170-178`): no per-chat map for `sourceChatId` → the id UNCHANGED (chat kept
  its ids / was skipped / not in this import); map present → `chatMap.get(id)
  ?? null` (null = reminted but never landed).

### 1.5 `lib/import/quilltap-import/import-entities.ts` (606 lines; `(pre)` 555)

- NEW `addImportedMessages(newChatId, chat, reminted, idMaps, repos, warnings)`
  (`:328-369`): `toInsert = reminted ? reminted.messages : chat.messages`;
  when reminted, `landed = new Map()` is registered **BEFORE** any insert
  (`idMaps.messages.set(chat.id, landed)` — so a chat whose every insert
  fails still has an (empty) map and every sidecar pointing into it resolves
  to `null`, NOT to the identity). **DEBUG** `'Importing chat messages'`
  `{sourceChatId, newChatId, messageCount, remintedMessageIds: Boolean(reminted),
  droppedMessageRefs: reminted?.droppedRefs ?? 0, summaryCleared:
  reminted?.summaryCleared ?? false}` (`:339-346`). Per message: `addMessage`;
  success → `added++` and (reminted) `landed.set(chat.messages[index].id,
  message.id)` only if the source id is truthy and not yet present; failure →
  warning `Failed to import message in chat "${chat.title}": ${message}`
  (UNCHANGED text) **plus NEW WARN** `'Failed to import chat message'`
  `{newChatId, messageId: <the INSERTED (reminted) id>, error: <message>}`
  (`:361-365`).
- `importChats` (`:371-450`), ONE create path:
  - `existing = repos.chats.findById(chat.id)`; `skip` → `skipped++`,
    `idMaps.chats.set(id, id)`, continue; `overwrite` → `repos.chats.delete(id,
    { syncVaults: false })`.
  - `isDuplicate = Boolean(existing) && conflictStrategy === 'duplicate'`
    (`:401`); `keepsSourceIds = Boolean(options.preserveIds) && !isDuplicate`.
  - `reminted = keepsSourceIds ? null : remintChatMessageIds(chatData,
    chat.messages)` (`:409`) — **every** chat created under a fresh id
    (duplicate, overwrite-without-preserveIds, plain import without
    preserveIds, the second import of one bundle) gets fresh message ids.
  - `{ chat: bundledChat, cursors } = takeBundleCursors(reminted ? { ...chatData,
    ...reminted.chatPatch } : chatData)` → `cleaned = withConciergeModeFromLegacy(
    stripScenarioSeededSummary(bundledChat))`.
  - `createData = keepsSourceIds ? { ...cleaned, id: chat.id } : isDuplicate ?
    { ...cleaned, title: \`${chatData.title} (imported)\` } : cleaned`;
    `createOptions = keepsSourceIds ? getPreserveIdsCreateOptions(chat.id,
    options) : undefined`.
  - `idMaps.chats.set(chat.id, newChat.id)` — the REAL id for every arm
    (**the chat phantom-map quirk is GONE**; templates / projects / groups keep
    theirs — `randomUUID()` at `:151`, `:216`, `:283`).
  - `await restoreBundleCursors(newChat.id, cursors)` (`:431`) — after create,
    BEFORE messages.
  - `messages += await addImportedMessages(...)`.
  - Outer catch unchanged: warning `Failed to import chat "${title}": …` + WARN
    `'Failed to import chat'` `{chatId, error}`.
- `importMemories` (`:465-606`):
  - Up-front id minting unchanged (`:482-486`).
  - NEW `resolveMemoryCharacter(id)` (`:490-503`, bug 240): `idMaps.characters.get(id)`
    → else ONE cached `getRepositories().characters.findByIdRaw(id)` per id per
    import (`localCharacters` Map); usable iff found and `!archivedAt`; found but
    archived → **DEBUG** `'Memory names an archived local character; not
    reviving it'` `{characterId}`; returns `null` when unusable. Used for
    `characterId` (`:518`; `null` → the UNCHANGED warning `Memory references
    non-existent character ${memory.characterId}` + `skipped++`) AND for a truthy
    `aboutCharacterId` (`:531`, `|| null`).
  - Payload literal order (`:570-586`): `...createData, relatedMemoryIds, tier,
    supersededById, supersededAt, consolidatedFrom, consolidatedAt, characterId,
    aboutCharacterId, chatId, projectId, tags, sourceMessageId`. NEW
    `supersededAt: memory.supersededById ? (memory.supersededAt ?? null) : null`
    (`:574`, bug 225 — truthiness of `supersededById`); NEW `sourceMessageId:
    memory.sourceMessageId ? remapImportedMessageId(idMaps, memory.chatId,
    memory.sourceMessageId) : memory.sourceMessageId` (`:583-585` — keyed by the
    SOURCE `chatId`).
  - The doc comment now names `admitCandidate` (`:455-457`) — prose only.

### 1.6 `lib/import/quilltap-import/execute.ts` (pin; +16/−6 in the span)

- `IdMappingState` literal gains `messages: new Map()` (`:469`).
- 7a annotations (`:658-686`): `create({ ...annotationData, chatId:
  remappedChatId, ...(annotation.sourceMessageId && { sourceMessageId:
  remapImportedMessageId(idMaps, annotation.chatId, annotation.sourceMessageId)
  }) })` (`:668-674`) — a reminted-but-missing message yields
  `sourceMessageId: null`.
- 7b-ii informs: `remapChatInform(inform, idMaps, known)` (`:761`) — signature
  unchanged, body moved (§1.7).
- `enqueueImportedMemoryEmbeddings`: BUILTIN → `scheduleRefit(userId,
  profile.id, { force: true })` (`:408`) — "imported rows keep their original
  `createdAt`, so the drift count that gates a manual edit's refit cannot see
  them".
- Phase order unchanged: 7 chats → 7a → 7b → 7b-ii → 7c stores → 7d → 7e → 8
  memories (gated `options.includeMemories`) → 9 files → 10 config →
  `reconcileRelationships` (`:983`).

### 1.7 `lib/import/quilltap-import/reconcile.ts` (pin; +46/−6)

- `remapChatInform(inform, idMaps, known)` (`:64-114`):
  `sourceRecordId = inform.recordMessageId ?? null`; `recordMessageId =
  sourceRecordId ? remapImportedMessageId(idMaps, inform.chatId, sourceRecordId)
  : null`; drop when `sourceRecordId && (!recordMessageId ||
  !known.messageIds.has(recordMessageId))` with reason `record message
  ${sourceRecordId} is missing from chat ${chatId}` (the SOURCE id).
  `consumedByMessageId` the same; `consumedByMessageIdCleared =
  Boolean(sourceConsumedById && (!mapped || !known.has(mapped)))`. **Behaviour
  delta hidden in the hunk:** a FALSY source pointer (`""`) used to ride
  through as `""` (`(pre)` `inform.recordMessageId ?? null`); it is now
  written as **`null`** (both `recordMessageId` and `consumedByMessageId`).
- Chat reconcile (`:358-449`) gains, after `projectId` (`:424-444`):
  `if (chat.scenarioSource)`: `mapped = idMaps.mountPoints.get(src.mountPointId)
  ?? ((await getRepositories().docMountPoints.findById(src.mountPointId)) ?
  src.mountPointId : null)`; `remapped = remapScenarioSource(source, () =>
  mapped)`; when `!remapped || remapped.mountPointId !== source.mountPointId` →
  `updates.scenarioSource = remapped` (possibly `null`), `hasUpdates = true`,
  **DEBUG** `'Remapped chat scenario source'` `{chatId: newId, fromMountPointId,
  toMountPointId: remapped?.mountPointId ?? null}`. Runs after 7c, so
  `idMaps.mountPoints` is populated. Because the duplicate now maps to its REAL
  id, a duplicated chat is now reconciled at all (participants, tags,
  projectId, scenarioSource) — `(pre)` its phantom id found nothing.

### 1.8 `lib/import/quilltap-import/import-files.ts` (pin)

`remapLinkedTo` (`:118-189`): a chat id resolved through `idMaps.chats` also
pushes its SOURCE id onto `sourceChatIds` (`:139-142`); for each unresolved id,
FIRST `sourceChatIds.map(c => idMaps.messages.get(c)?.get(id)).find(Boolean)`
(`:161-167`) — a hit is KEPT as the reminted id; otherwise the old
`resolvedChatIds` message-scan. A message resolves only through a chat named in
the SAME `linkedTo` array.

### 1.9 `lib/import/quilltap-import/types.ts`

`IdMappingState.messages: Map<string, Map<string, string>>` (`:240`) — "Source
chat id → (source message id → the id the chat import minted for it). Present
only for chats created under a fresh id … an entry is recorded only once its
message has actually been inserted."

### 1.10 `lib/export/{ndjson-writer,types}.ts`

- `streamChats` (`ndjson-writer.ts:336-…`): `bundleCursors = ids.length > 1 ?
  await loadBundleCursors() : null` (`:349`) — one table read for a
  multi-chat export, a per-chat `findByChatId` for a single chat (same bytes,
  different WARN on failure); the chat record is `{ ...(await
  withBundleCursorFields(chatForExport, bundleCursors)), ...(tagNames.length >
  0 && { _tagNames }), ...(participantInfo.length > 0 && { _participantInfo }) }`
  (`:381-385`) — **cursor keys after every schema key, before `_tagNames`**.
- `ExportedChat extends ChatMetadata, BundleCursorFields` (`types.ts:191`).
- The chat schema itself (the record's key order) lost
  `otherExtractionWatermarkMessageId` (both `ChatMetadataSchema` and
  `ChatMetadataBaseSchema`; replaced by the comment at `chat.types.ts:1114-1115`)
  and gained `scenarioSource` / `scenarioNotes` right after `scenarioText`
  (`:918-929`, `:1319-1322`); `MemorySchema` gained `supersededAt:
  TimestampSchema.nullable().optional()` between `supersededById` and
  `consolidatedFrom` (`memory.types.ts:139-147`).
- `NON_PORTABLE_INSTANCE_SETTING_KEYS` (`lib/instance-settings/index.ts:77-83`)
  UNCHANGED — so `memoryMaintenance` travels in an `instance-settings` export by
  default, and a `.qtap` import does NOT drop a dead `memoryExtractionConcurrency`
  (`import-configuration.ts` untouched) — only the restore does.

### 1.11 `public/schemas/qtap-export.schema.json` (1,837 lines; 103,075 → **105,310** bytes)

`git diff 4a38222e6 64743ee13 -- public/schemas` touches ONLY this file (the
custom-tool / progression schemas are unmoved):

- `ExportedChat` (`:544-557`): `scenarioSource` `{type: ["object","null"],
  required: [mountPointId, path, bodySha256], properties: {mountPointId:
  {string, format uuid}, path: {string, maxLength 500}, bodySha256: {string,
  pattern ^[0-9a-f]{64}$}}}` (no `additionalProperties`) and `scenarioNotes`
  `{type: [string, null]}` after `scenarioText` (`545e299bb`).
- `otherExtractionWatermarkMessageId` description rewritten ("…Stored in
  memory_extraction_cursors, carried on the chat under this name…"), NEW
  `episodeExtractionWatermarkMessageId` (`c2795733f`, description `ac6348c5f`)
  and `turnExtractionWatermarkMessageId` (`64743ee13`), each `{type: [string,
  null]}` (`:567-578`).
- `Memory.supersededAt` `{anyOf: [Timestamp, null]}` after `supersededById`
  (`:1023-1026`, `c2795733f`).
Both `$defs` remain `additionalProperties: true` — tightening for PRESENT keys
only.

### 1.12 `lib/backup/backup-service.ts` (835 lines)

- `collectUserData`: `bundleCursors = await loadBundleCursors()` once (`:194`);
  each chat `{ ...(await withBundleCursorFields(chat, bundleCursors)), messages:
  messageEvents }` (`:202-206`) — cursor keys after the chat keys, BEFORE
  `messages`.
- The vector-index block (`(pre)` `:263-280`) replaced by the comment at
  `:266-268` ("a memory's vector rides on its own row … the in-process index is
  rebuilt from the rows on first use"). `vectorIndexMetas` / `vectorEntries`
  gone from the returned data, from `createManifest`'s `counts` (now
  `tfidfVocabularies` → `docMountPoints`, `backupFormat: 4` unchanged), from
  `compactBackupData`, from the compact DEBUG `'Compact backup — embeddings and
  derived tables omitted'` (now `{userId, memories, droppedConversationChunks,
  droppedDocMountChunks}`), and from staging (`:672-680` — `if (!compact)`
  writes only `embedding-status`, `conversation-chunks`, `tfidf-vocabularies`).
- `tfidfVocabularies = globalRepos.tfidfVocabularies.findAll()` (`:272`) is
  UNCHANGED code, but its rows are now `TfidfVocabularySchema`-parsed WITH
  `textShapeVersion: z.number().int().nonnegative().default(0)` between
  `fittedAt` and `createdAt` (`embedding-job.types.ts:51-57`, `4ae0c4ca6`).
- `dumpInstanceSettings` (`:92-104`) raw `SELECT "key","value"` — carries
  `memoryMaintenance` (and whatever else) untouched.
- The chat-settings row is the repo's parse — the three dropped columns are
  simply absent.

### 1.13 `lib/backup/types.ts`

`SerializedVectorEntry` kept, re-documented as the LEGACY mirror row (`:63-73`);
`BackupManifest.counts` loses `vectorIndexMetas` / `vectorEntries`; `BackupData`
keeps `vectorEntries?: SerializedVectorEntry[]` "Legacy only … never written"
and loses `vectorIndexMetas`; `ChatWithMessages extends ChatMetadata,
BundleCursorFields` (`:434`); `RestoreSummary` loses `vectorIndexMetas` /
`vectorEntries`, gains `legacyVectorsAdopted?: number` (`:571-575`);
`embeddingReconcile` loses `vectorEntriesDeleted` / `vectorIndexMetaFixed`.

### 1.14 `lib/backup/restore/archive.ts`

`parseBackupZip` no longer reads `data/vector-index-metas.json` (ignored, comment
`:257-259`); STILL reads `data/vector-entries.json` optionally (`:260`) into
`vectorEntries`.

### 1.15 `lib/backup/restore/preview.ts`

The two vector counts removed; preview summary keys now: … `tfidfVocabularies`,
`docMountPoints` … `wardrobeWear`, `warnings` (39 keys; no
`legacyVectorsAdopted`, no `embeddingReconcile`).

### 1.16 `lib/backup/restore/restore.ts` (1,423 lines; `(pre)` 1,431)

- **7. Chats** (`:258-304`): `{ userId, createdAt, updatedAt, messages,
  ...bundledChat } = chat`; `{ chat: chatData, cursors } =
  takeBundleCursors(bundledChat)` (`:265`); `create(withConciergeModeFromLegacy(
  stripScenarioSeededSummary(chatData)), { id: chat.id })`; `await
  restoreBundleCursors(createdChat.id, cursors)` (`:274`) — after create,
  before messages. No validation of the cursor message ids (a restore writes a
  cursor naming a message that is not in the archive). `scenarioSource` /
  `scenarioNotes` ride the create untouched (replace mode: an INVALID source
  fails `ChatMetadataSchema` → the whole chat refused with the existing
  `Failed to restore chat` warning + WARN).
- **9. Memories** (`:306-376`): `legacyVectors = new Map((data.vectorEntries ||
  []).map(e => [e.id, e.embedding]))` (`:335` — duplicate ids: LAST wins);
  per memory, after the index-keyed decode: `hasEmbedding =
  Array.isArray(emb) && emb.length > 0`; `adoptsLegacyVector = !hasEmbedding &&
  Array.isArray(legacy) && legacy.length > 0` → `embedding = legacy`; ownership
  check; `create`; `memoriesRestored++`; `if (adoptsLegacyVector)
  legacyVectorsAdopted++` (counted only on a successful create). After the
  loop: `memoryIndex.reloadAll()` (`:371`) and **DEBUG** `'Restored memories'`
  `{total: data.memories.length, restored: memoriesRestored,
  legacyVectorsAdopted}` (`:372-376`). An `{}` index-keyed embedding (not
  decoded) is "no embedding" here and WOULD adopt a matching legacy vector.
- **22j** (`(pre)` `:1048-1077` — vector metas + entries, WARNs `'Failed to
  restore vector index meta'` / `'Failed to restore vector entries batch'`)
  DELETED. 22k / 22l follow directly (`:1075`, `:1093`).
- **22l TF-IDF** (`:1093-1107`) unchanged code — `create({ ...vocData, userId:
  targetUserId }, { id })`; the schema default supplies `textShapeVersion: 0`
  for an archive that lacks it; an explicit `null` FAILS Zod (`int()` on null)
  → `Failed to restore TF-IDF vocabulary: <ZodError>` + WARN.
- **22o Instance settings** (`:1174-…`): `instanceSettingRows =
  withMemoryMaintenanceFromLegacy(data.instanceSettings || [], data.chatSettings
  || [])` (`:1178`) — `instanceSettingsRestored` counts the TRANSLATED rows
  (+1 for a derived key, −1 per dead key), while preview reports the archive's.
- **25** DEBUG `'Post-restore embedding reconcile complete'` (`:1338`) now
  `{targetDimensions, skippedReason, reindexEnqueued, mismatched}`.
- Summary literal (`:1356-1408`): `… tfidfVocabularies, legacyVectorsAdopted,
  docMountPoints …`, `embeddingReconcile: {targetDimensions, skippedReason,
  reindexEnqueued}` (41 keys).
- **Log census at the pin: 79 `moduleLogger` messages (50 warn, 7 info, 20
  debug, 2 error)** — vs `(pre)` 80: **+** `debug 'Restored memories'`;
  **−** `warn 'Failed to restore vector entries batch'`, **−** `warn 'Failed to
  restore vector index meta'`. (Measured by extracting every
  `moduleLogger.<level>('…'` from both shas.)

### 1.17 `lib/backup/restore/uuid-remap.ts` (649 lines)

- Chat scalar pass (`:300-316`): `remapFields(chat, [... 'imageProfileId',
  'otherExtractionWatermarkMessageId', 'episodeExtractionWatermarkMessageId',
  'turnExtractionWatermarkMessageId'])` — the three cursor keys LAST, in
  `BUNDLE_CURSOR_FIELDS` order (mint order matters to the uuid-remap corpus).
- NEW (`:320-324`), after the scalar pass + `userId`, BEFORE participants:
  `if (chat.scenarioSource) remappedChat.scenarioSource =
  remapScenarioSource(chat.scenarioSource, id => remapper.remap(id))` — the
  remapper always mints, so a VALID source always survives (re-pointed); an
  INVALID one (or a JSON string that fails the schema) becomes `null`; a JSON
  string that passes becomes an OBJECT. Assignment onto an existing key — key
  position unchanged.
- Memories (`:348-357`) unchanged (`supersededAt` is not an id).
- `vectorIndexMetas` remap removed; legacy `vectorEntries` still remapped on
  `['id','characterId']` (`:525-527`) so adoption matches the remapped memory
  id; returned object loses `vectorIndexMetas`.
- `chatSettings` remap (`:432-470`) and `instanceSettings` remap (`:594-600`,
  mount-point keys only) UNCHANGED — so neither a legacy
  `autoHousekeepingSettings.perCharacterCapOverrides` nor a
  `memoryMaintenance.perCharacterCapOverrides` (keyed by CHARACTER id) is
  remapped in a new-account restore (§9 open point).

### 1.18 `lib/backup/restore/delete-service.ts`

- `clearFormat3Entities` list (`:33-60`): `'memory_extraction_cursors'` added
  right after `'chat_informs'` (`:41-44`); `'vector_entries'` / `'vector_indices'`
  removed.
- `deleteUserData`: `await retireMemories(memoryIds, { reason:
  'restore-replace' })` (`:169`) replaces `deleteMemoriesWithUnlinkBatch` — ONE
  call over every collected memory id (no `keep`), so the retire path logs its
  own DEBUG `'Retired memories'` (`lib/memory/retire.ts:446`, fields `{reason,
  keep, requested, retired, spared, sparedBy, digestsRetiredWithLastMember,
  vectorsRemoved, neighboursTouched, digestLinksScrubbed, charactersAffected,
  durationMs}`) and announces (`announceRetirement` — realtime `memories` +
  Commonplace mirror refresh for retired digests) — that is the retire lane's
  contract, consumed here.

### 1.19 Other carrier-adjacent hunks (recorded, not this slice's port)

- `lib/services/ai-import.service.ts:583` `reinforcedImportance:
  calculateReinforcedImportance(clamped, 1)` — at the pin formula
  (`log2(max(1, 1)) × 0.05 = 0`) VALUE-IDENTICAL to the clamp v5 already writes
  (`generators/ai_import.rs:754-757`). Shape-only.
- `lib/import/sillytavern-import-service.ts` `queueMemoryExtractionJobs` →
  `listExtractionTurns(loadChatMessageEvents(chatId), null)` — v5 has NO twin
  of the ST chat import's extraction enqueue (`services/sillytavern.rs` ports
  the character card only). Extraction lane / no-op here.
- `lib/startup/reconcile-embedding-dimensions.ts` loses the `vector_entries`
  DELETE / `vector_indices` snap and both counters — the restore summary
  consumes the narrowed result type (embedding lane provides).
- `app/api/v1/system/tools/route.ts` loses `memory-dedup(-preview)` and
  `INTER_CHARACTER_MEMORY` — the route the import/export oracle cases import
  still exists (maintenance lane).

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 deletion | v5 twin (file:line on `a7c6f0d03`) |
|---|---|
| backup vector collect (`(pre)` `backup-service.ts:263-280`) | `services/backup/collect.rs:118-119` (fields), `:345-352` (`VECTOR_INDEX_METAS` spec), `:647-673` (the read), `:733-734`, `:842-843`, `:886-887` (MainSide plumbing), `:969-998` (`read_vector_entries`) |
| compact's two vector clears | `collect.rs:521-522` |
| manifest `vectorIndexMetas` / `vectorEntries` | `services/backup/manifest.rs:80-81` |
| staging of the two files + their compact-omit entries | `services/backup/staging.rs:75-76`, `:117-118` |
| `parseBackupZip` reading `vector-index-metas.json` | `services/backup/restore/archive.rs:185`, `:232` (keep `:186` / `:233` — `vector_entries` is still read, legacy) |
| new-account remap of `vectorIndexMetas` | `services/backup/uuid_remap.rs:352-355`, `:495` |
| `RestoreSummary.vectorIndexMetas` / `vectorEntries` | `services/backup/restore/mod.rs:74-75`; preview `restore/preview.ts:58-59`; counters `restore/orchestrator.rs:4347-4348`, `:4439-4440`, empty-data `:4602-4603` |
| `embeddingReconcile.vectorEntriesDeleted` / `vectorIndexMetaFixed` | `restore/mod.rs:110-111`; `orchestrator.rs:2143-2144` (DEBUG fields), `:2163-2164` |
| restore step 22j | `restore/orchestrator.rs:1766-1803` (whole block, incl. WARNs `Failed to restore vector index meta` / `Failed to restore vector entries batch`) |
| `vector_entries` / `vector_indices` in the replace truncate | `services/delete_all.rs:205-206` |
| `deleteMemoriesWithUnlinkBatch` in the wipe | `services/delete_all.rs:341-357` (`delete_many_with_unlink`) |
| the chat-column watermark carriage (read, write, remap) | `db/chats_read.rs:96,322` + `db/chats.rs:411-417,999` (keystone retires); `services/backup/uuid_remap.rs:826-829`; `services/qtap_export/schema-key-order.json` `chat` slot after `timelineMode` (header `key_order.rs:19-25`) |
| the chats phantom-map duplicate arm (bug 228) | `services/quilltap_import/entities.rs:13-17` (module header), `:828-856` (`Duplicate` → phantom id, `DUPLICATE_MINTS`), `:784-789` (doc: "Message ids/timestamps come from the payload … without a message id map") |
| "sourceMessageId stays as-is because the message import preserves message ids" | `services/quilltap_import/mod.rs:1521-1524` (comment + the as-is copy `:1546-1549`) |
| the inform remap's `""` pass-through | `services/quilltap_import/reconcile.rs:1006-1025` (`remap_chat_inform`, comment "the `""` passes through to the destination row unchanged") |
| `RESTORE_TS_MESSAGES`' two vector lines | `harness/oracle/cases/system-restore.test.ts:858-859`; `crates/quilltap-harness/tests/system_restore_state.rs:5838-5839` |

---

## 3. Behaviour deltas a port must hit (NUMBERED)

Backup (v4 `backup-service.ts` ↔ v5 `services/backup/`):

1. **No vector files.** Full and compact backups write NEITHER
   `vector-index-metas.json` NOR `vector-entries.json`; `counts` lose both keys
   (v4 `:482-488`; v5 `staging.rs:75-76,117-118`, `manifest.rs:80-81`,
   `collect.rs:647-673,969-998`). `backupFormat` stays 4.
2. **Cursors on every chat** — after every chat key, before `messages`, in the
   order other → episode → turn, each ONLY when set (v4 `:194`, `:204`; v5
   `collect.rs:567-581` — insert BEFORE `messages` at `:579`). One table read
   for the whole backup; a failed read WARNs and falls back per chat; an absent
   table reads as none (v5 must not write schema on the source — the P4.D264
   R-G precedent; v4's `findAllByChat` lazily CREATES it via `getCollection()`).
3. **`tfidfVocabularies[*].textShapeVersion`** between `fittedAt` and `createdAt`
   (v4 schema `embedding-job.types.ts:51-57`; v5 `collect.rs:318-330` gains
   `("textShapeVersion", F::Num)` — the column is `INTEGER DEFAULT 0` in both v4
   shapes, so NULL only if explicitly written; v4's `.default(0)` applies to
   `undefined`, not `null`).
4. **Memory rows carry `supersededAt`** (in `MemorySchema` order, after
   `supersededById`) — inherited from `memories_read` once the keystone adds the
   column (v5 `collect.rs:584-596` reuses `memories_read::find_by_character_id`).
5. **Chat rows carry `scenarioSource` (object) / `scenarioNotes`** after
   `scenarioText`, and NOT `otherExtractionWatermarkMessageId` — inherited from
   `chats_read` (keystone / scenarios lane).
6. **Chat-settings rows lose `autoHousekeepingSettings` /
   `memoryExtractionLimits` / `memoryExtractionConcurrency`** — inherited from
   `chat_settings::find_by_user_id` (keystone).
7. `instance-settings.json` stays a raw dump (no filtering at backup time).

Restore (v4 `restore/*.ts` ↔ v5 `services/backup/restore/` + `uuid_remap.rs` + `delete_all.rs`):

8. **22j deleted** — v5 `orchestrator.rs:1766-1803` goes; `RestoreSummary`
   loses `vectorIndexMetas` / `vectorEntries` (`restore/mod.rs:74-75`).
9. **Legacy vector adoption** in phase 9 (v4 `:335-363`; v5
   `orchestrator.rs:862-937`): build the `id → embedding` map from
   `data.vector_entries` (last duplicate wins), and AFTER
   `decode_index_keyed_embedding` set the row's `embedding` to the legacy one
   iff the row's is not a non-empty array and the legacy one is a non-empty
   array; count it only after a successful create.
10. After the memory loop: `memoryIndex.reloadAll()` (v5 has NO in-process
    vector cache — `embedding_dimension_reconcile.rs:59` documents
    `unloadAll()` as a no-op — so this is a no-op unless the keystone adds an
    index cache) and DEBUG `Restored memories {total, restored,
    legacyVectorsAdopted}` on `quilltap::restore`.
11. `RestoreSummary.legacyVectorsAdopted` between `tfidfVocabularies` and
    `docMountPoints` on RESTORE ONLY (`#[serde(skip_serializing_if =
    "Option::is_none")] legacy_vectors_adopted: Option<usize>`, preview `None`,
    restore `Some(n)` even when 0). Preview 39 keys, restore 41.
12. `embeddingReconcile` = `{targetDimensions, skippedReason, reindexEnqueued}`;
    the step-25 DEBUG drops `vectorEntriesDeleted` / `vectorIndexMetaFixed`
    (v5 `orchestrator.rs:2136-2165`, `restore/mod.rs:103-112`).
13. **`parseBackupZip` ignores `vector-index-metas.json`** (v5 `archive.rs:185`)
    and keeps the optional `vector-entries.json` read (`:186`).
14. **Cursors restored, not written to the chat row** (v4 `:262-274`; v5
    `orchestrator.rs:700-783`): strip the three keys from the archive chat JSON
    BEFORE the typed `ChatCreate` decode (and before the Concierge derive /
    bug-158 strip, matching v4's composition), create, then
    `restore_bundle_cursors(chat_id, cursors)` BEFORE the message loop. Works
    identically for (a) a v5-written or baseline-v4 archive carrying the OTHER
    key from the old column, (b) a `c2795733f`-era archive carrying the EPISODE
    key, (c) a pin archive carrying any of three. Blank / null values are not
    cursors. No message-id validation.
15. **New-account remap** (v5 `uuid_remap.rs:815-880` `remap_chat`): the scalar
    list gains `episodeExtractionWatermarkMessageId`,
    `turnExtractionWatermarkMessageId` after `otherExtraction…` (LAST); then
    `scenarioSource` re-pointed through `remap_scenario_source` with the shared
    remapper (minting order: after the chat scalars, before participants);
    `vector_index_metas` remap removed (`:352-355`).
16. **Instance settings through `with_memory_maintenance_from_legacy`** (v4
    `:1178`; v5 `orchestrator.rs:1987-2010`): drop `memoryExtractionConcurrency`
    rows; derive `memoryMaintenance` from the archive's raw chat-settings rows
    (`data.chat_settings` VALUES — the legacy key survives in the archive JSON
    even though `ChatSettingsCreate` will no longer read it) only when the
    archive has no `memoryMaintenance` row; APPEND it; `c.instance_settings`
    counts the translated rows.
17. **TF-IDF restore carries `textShapeVersion`** (v5 `orchestrator.rs:1835-1864`
    — `TvCreate` gains the field; absent → 0; an explicit `null` is a v4 Zod
    refusal — §9).
18. **Replace wipe**: `memory_extraction_cursors` truncated right after
    `chat_informs`; `vector_entries` / `vector_indices` no longer listed (v5
    `delete_all.rs:194-213`; a missing table is already tolerated — `let _ =`
    at `:276-278`); the memory wipe goes through `retire_memories(ids,
    RetireReason::RestoreReplace)` (v5 `delete_all.rs:341-357`).
19. Census: `RESTORE_TS_MESSAGES` −2 +1 (§1.16).

`.qtap` export (v4 `ndjson-writer.ts` ↔ v5 `services/qtap_export/`):

20. **Cursor keys on each chat record** after the schema-ordered keys, before
    `_tagNames` / `_participantInfo` (v4 `:349`, `:381-385`; v5
    `records.rs:217-270` — insert into the source map BEFORE `with_tag_names`,
    since `key_order::reorder` appends non-template keys in their original
    order, `key_order.rs:52-69`). Preloaded read when `ids.len() > 1`, per-chat
    read otherwise (two different WARN lines on failure).
21. **Re-dump `schema-key-order.json`** (`harness/oracle/fixtures/
    dump-export-key-order.ts`) from the pin: `chat` 102 → 103 keys
    (−`otherExtractionWatermarkMessageId`, +`scenarioSource`, `scenarioNotes`
    after `scenarioText`); `memory` 29 → 30 (+`supersededAt` after
    `supersededById`). ⚠ Skipping the re-dump would place a CURSOR-carried
    `otherExtractionWatermarkMessageId` after `timelineMode` (the stale template
    slot) instead of at the end — a byte divergence only the export
    differential sees.

`.qtap` import (v4 `quilltap-import/*` ↔ v5 `services/quilltap_import/`):

22. **ONE chat create path + remint** (bug 228; v4 `import-entities.ts:371-450`;
    v5 `entities.rs:791-955`): `keeps_source_ids = preserve_ids &&
    !is_duplicate`; NOT keeping → `remint_chat_message_ids` over the bundle chat
    + messages; the chat patch spread over the chat JSON before
    `take_bundle_cursors` → Concierge derive → bug-158 strip → typed decode;
    duplicate → title `"{title} (imported)"`; create under the source id only
    when keeping; `id_maps.chats.set(source, REAL new id)` (retire the
    `DUPLICATE_MINTS` phantom for chats — keep it for templates / projects /
    groups); `restore_bundle_cursors` after create; messages through ONE
    `add_imported_messages` with the per-chat `landed` map registered before
    the first insert.
23. NEW DEBUG `Importing chat messages {sourceChatId, newChatId, messageCount,
    remintedMessageIds, droppedMessageRefs, summaryCleared}` and NEW WARN
    `Failed to import chat message {newChatId, messageId, error}` (the warning
    text `Failed to import message in chat "…": …` unchanged — v5 already has
    it, `entities.rs:863-874`; the WARN is new on BOTH arms, incl. v5's serde
    arm, which v4 does not have — keep v5's serde-arm warning as the
    established refusal).
24. **Sidecars follow reminted messages** through `remap_imported_message_id(
    &id_maps.messages, source_chat_id, id)`: annotations (v5 `mod.rs:1521-1575`,
    `sourceMessageId` → remapped, `null` when it never landed), informs (v5
    `reconcile.rs:985-1048` + the caller `mod.rs:1718` — new params: the SOURCE
    chat id + the message maps; refusal reason keeps the SOURCE id; a falsy
    pointer now becomes `null`), memories (`sourceMessageId`, keyed by the
    memory's SOURCE `chatId`), file `linkedTo` (v5 `files.rs:288-363` — collect
    `source_chat_ids`, try the reminted map first).
25. **Bug 240 — memories resolve to local characters** (v4 `:490-503`; v5
    `memories.rs:266-281` + the `aboutCharacterId` arm `:300-313`): bundle map
    first, else ONE strict `characters_read::find_by_id_raw` per id per import
    (cached; import runs inside the strict scope — the `wardrobe_wear.rs:343-372`
    precedent reads the same way), usable iff not `archivedAt`; archived →
    DEBUG `Memory names an archived local character; not reviving it
    {characterId}`.
26. **`supersededAt`** in the memory payload right after `supersededById`
    (v5 `memories.rs:211-238` `insert_consolidation_fields`):
    `js_truthy(supersededById) ? (supersededAt ?? null) : null`.
27. **Scenario reconcile** (v4 `reconcile.ts:424-444`; v5 `reconcile.rs:376-450`
    chat block): after `projectId`, re-point / keep / drop `scenarioSource` via
    `id_maps.mount_points` then the target's own `doc_mount_points` (mount
    connection), DEBUG `Remapped chat scenario source`, through
    `ChatUpdate.scenario_source` (keystone / scenarios lane field).
28. The duplicated chat is now RECONCILED (its participants / tags / project /
    scenario source remapped) because its map entry is real (v5 reconcile
    iterates `id_maps.chats` — free once 22 lands).
29. `scheduleRefit(…, {force:true})` — stays a LOUD DEFERRAL (v5
    `mod.rs:1262-1286`, P4.D62); update the deferral's prose to name the
    `force` flag (the TF-IDF lane owns any refit scheduler).
30. **Schema re-vendor**: `generators/qtap-export.schema.json` → 105,310 bytes;
    `qtap_schema_embed_guard.rs:59` and `generators/qtap_schema.rs:191` move
    together.

---

## 4. v5 on `main` today — per file

- **`crates/quilltap-core/src/services/backup/collect.rs`** (1,262) — v4
  `collectUserData`/`compactBackupData`. Changes: `:118-119` drop
  `vector_index_metas` (keep a `vector_entries` field ONLY if the struct stays
  shared with the parse side — see §7.6), `:318-330` tfidf spec +1,
  `:345-352` delete, `:521-522` delete, `:567-581` cursor fields before
  `messages`, `:647-673` delete, `:733-734` / `:842-843` / `:886-887` delete,
  `:969-998` delete.
- **`…/backup/manifest.rs`** (115) — `:80-81` delete.
- **`…/backup/staging.rs`** (439) — `:75-76`, `:117-118` delete.
- **`…/backup/marshal.rs`** (296) — no change (`F::Num` exists).
- **`…/backup/uuid_remap.rs`** (1,004) — `:826-829` scalar list +2,
  `remap_chat` gains the scenario-source remap between `chain_owned` and
  participants (`:832`), `:352-355` + `:495` delete.
- **`…/backup/uuid_remapper.rs`** (345) — no change (`remap` always mints).
- **`…/backup/restore/archive.rs`** (545) — `:185` + `:232` delete.
- **`…/backup/restore/mod.rs`** (128) — summary struct (`:46-96`): −2 fields,
  +`legacy_vectors_adopted`; `EmbeddingReconcileSummary` −2 (`:103-112`); the
  "41-key" doc comment (`:34`) → preview 39 / restore 41.
- **`…/backup/restore/preview.rs`** (78) — `:58-59` delete, `legacy_vectors_adopted: None`.
- **`…/backup/restore/orchestrator.rs`** (4,789) — phase 7 `:696-842` (cursor
  take before `serde_json::from_value` at `:739`, restore after create
  `:760-783`), phase 9 `:844-937` (adoption + the post-loop DEBUG), 22j
  `:1766-1803` delete, 22l `:1835-1864` (+`text_shape_version`), 22o
  `:1987-2010` (translated rows), 25 `:2136-2165` (−2 fields), the counters
  `:4340-4350` / `into_summary` `:4397-4445` / empty-data `:4596-4606`.
- **`…/backup/restore/rows.rs`** (215) — no change (`decode_index_keyed_embedding`
  is reused).
- **`…/backup/restore/store_claims.rs`, `legacy_migrations.rs`,
  `mount_index_coercion.rs`, `json_stream.rs`** — untouched by the span.
- **`crates/quilltap-core/src/services/delete_all.rs`** — `:194-213` list,
  `:341-357` memory wipe.
- **`…/services/qtap_export/records.rs`** (1,524) — `stream_chats` `:202-270`:
  preload when `ids.len() > 1`, cursor fields into `chat` before
  `with_tag_names` (`:261`).
- **`…/services/qtap_export/key_order.rs`** (220) + **`schema-key-order.json`**
  — re-dump; header `:19-25` gains the pin's line.
- **`…/services/qtap_export/mod.rs`** (791) — `with_tag_names` `:580-589`
  unchanged.
- **`…/services/quilltap_import/mod.rs`** (2,890) — `IdMaps` `:321-360`
  (+`messages`), the chats call `:1512-1519` (pass `id_maps`), annotations
  `:1521-1575`, informs `:1631-1760` (caller `:1718`), memories `:1937`,
  reconcile `:1994`, the refit deferral prose `:1262-1286`.
- **`…/services/quilltap_import/entities.rs`** (1,395) — header `:13-17`,
  `import_chats` `:784-891`, `create_chat` `:893-955` (takes the patched JSON).
- **`…/services/quilltap_import/memories.rs`** (746) — `:211-238`, `:266-281`,
  `:300-313`, plus the `sourceMessageId` remap after the tags block.
- **`…/services/quilltap_import/reconcile.rs`** (1,139) — chat block
  `:376-450`, `remap_chat_inform` `:952-1048`, `remapped_chat_inform_input`
  `:1056-1090` (its "as they came" rule for the two pointers changes with the
  `""` → `null` delta).
- **`…/services/quilltap_import/files.rs`** (596) — `remap_linked_to` `:288-363`.
- **`…/services/quilltap_import/{wardrobe_wear,characters,document_stores,…}.rs`**
  — untouched.
- **`crates/quilltap-core/src/generators/qtap-export.schema.json`** +
  **`qtap_schema.rs:191`** — re-vendor.
- **`crates/quilltap-core/src/generators/ai_import.rs:754-757`** — no value
  change (§1.19).
- **`crates/quilltap-core/src/api/system_backup.rs:223`** — serializes the
  summary straight; no `api/types.rs` variant moves.

---

## 5. Harness

All carrier oracles drive v4's REAL code from a pinned worktree (PIN REQUIRED).
None of the carrier CASES imports a deleted module directly (every `@/…` import
in `system-backup`, `system-restore`, `system-restore-guards`,
`backup-uuid-remap`, `system-export`, `system-import`, `system-import-execute`,
`qtap-import`, `qtap-import-guards`, `qtap-schema-validate`,
`chat-dialogs-export`, `ai-import-assembly`, `wardrobe-wear-import-remap`,
`backup-mount-index-coercion` resolves at `64743ee13` — `lib/embedding/
vector-store.ts` SURVIVES Phase B). The dead ones are BUILDERS:

**IMPORT-DEAD / RUNTIME-DEAD at the pin**
- `harness/oracle/fixtures/build-qtap-import-fixture.ts:103-107` — dynamic
  `import('@/lib/database/repositories/vector-indices.repository')` (DELETED by
  `af48f6cc2`). `qtap-import.ts` requires the fixture to be BUILT per run
  ("AFTER building the fixtures") → `qtap_import_equivalence` cannot regen
  until the block is removed (the tables are no longer needed — nothing writes
  them).
- `harness/oracle/fixtures/build-system-data-fixture.ts:777-782` —
  `repos.vectorIndices.saveMeta` / `addEntry`: `repos.vectorIndices` is
  undefined at the pin → TypeError. A REBUILD is impossible (it was already
  migrate-in-place only — `migrate-system-data-schema.ts:1-25`); the family must
  NARROW in place through v4's real migrations at the pin (the keystone's job:
  `retire-memory-vector-entries-v1` copies 0 — `VECTOR_ENTRY_1 =
  'bb000000-…01'` names no memory — and drops both tables;
  `add-memory-extraction-cursors-v1`; `add-memory-superseded-at-v1`;
  `move-memory-maintenance-settings-v1`; `add-chat-scenario-source-v1`;
  `add-tfidf-text-shape-version-v1`).
- `harness/oracle/cases/system-export.test.ts:517` (+ Rust twin
  `system_export_equivalence.rs:550-553`) — `MEMORY_TIERS_PLANT_SQL`'s
  `UPDATE "chats" SET "otherExtractionWatermarkMessageId" = …` fails `no such
  column` on a narrowed fixture → every export case throws in its plant.
- `harness/oracle/cases/system-restore.test.ts:530` (rename
  `vector_entries.embedding`) and `:572` (trigger on `vector_indices`) + Rust
  twins `system_restore_state.rs:1390-1395`, `:1557-1562` — the planted tables
  no longer exist on a pin target → `restore_phase_warns_replace` dies in its
  plant (and v4 writes neither table any more, so the plants measure nothing).
- `build-restore-archives-dedupe.test.ts:201` / `build-restore-archive-bug117.test.ts:215`
  / `system-restore.test.ts:1071` — `anyRepos.vectorIndices?.…` is optional-
  chained → survives (harmless no-op).

**RED at the pin (behaviour moves; ported → green)**

| family (Rust test ← oracle) | tier | why |
|---|---|---|
| `system_backup_equivalence` ← `system-backup.test.ts` | 2 (archive tree) | no vector files (`:655-668` compact list too); cursor keys on chats; `textShapeVersion`; chat-settings −3 keys; `supersededAt`; scenario keys |
| `system_restore_state` / `system_restore_equivalence` ← `system-restore.test.ts` | 2 | 22j gone on EVERY archive (all but compact / minimal / malformed / legacy-profiles carry `vector-entries.json` — an ORPHAN `bb…01`, so adoption = 0 everywhere today); summary keys; census −2 +1; `restore_memory_tiers_*` asserts the CHAT-COLUMN watermark (`system_restore_state.rs:4958-4964`) → must read the cursor table; `restore_phase_warns_replace` plants (above) |
| `restore_vintage_state` (v5-only) | 2 | the boot-align list (`:95-146`) must grow every new ensure the restore INSERTs depend on (cursor table, `memories.supersededAt`, `chats.scenarioSource/Notes`, `tfidf_vocabularies.textShapeVersion`) and DROP P4.D265's chat-column call at `:139-146` once retired — else `table … has no column named …` (memory note `vintage-restore-mirror-boot-list`) |
| `backup_uuid_remap_equivalence` ← `backup-uuid-remap.test.ts` | 1 (pure remap over a sha-guarded corpus `uuid-remap-corpus.json`, 37 vector hits) | `remapBackupData` returns no `vectorIndexMetas`; `COLLECTIONS` (`:74-114`) must drop `['vectorIndexMetas', …]`; corpus regenerated |
| `system_export_equivalence` ← `system-export.test.ts` | 2 (byte-exact NDJSON) | plant (above); chat lines gain cursor keys at the end; scenario keys; memory `supersededAt`; key-order template |
| `system_import_state` ← `system-import-execute.test.ts` | 2 | bug 228 everywhere a chat is created under a fresh id: `execute_duplicate_all`, `execute_twice`, `execute_overwrite_all` (overwrite without preserveIds), `execute_chats_informs_{duplicate,cross_instance}`, `execute_files_{duplicate,cross_instance}`, `execute_cross_instance_skip`, `execute_memory_tiers` (`supersededAt`, `sourceMessageId`), `execute_chat_watermark` (`assert_chat_watermark_lands` `:3225-3255` reads the chat column → cursor table), `execute_inform_refusals` (the `recordMessageId: ""` row — "falsy, so the remap neither drops nor clears it", oracle `:1945`, row `:1975` — now LANDS with `null`: flips by design). Measure the rest; most fresh-id cases move because v5 keeps source message ids |
| `qtap_schema_embed_guard` | guard | 103,075 → 105,310 — the re-vendor obligation, by design |
| `other_extraction_watermark_writers_census` | census | retires with the keystone's chat-column arm |

**Predicted GREEN** — `public_schemas_vendor_guard` (the two SPA-served
schemas unmoved), `system_restore_guards_equivalence`,
`backup_mount_index_coercion_equivalence`, `system_import_equivalence`
(replays v4's OWN export bytes through v5's reader + preview — measure),
`qtap_schema_validate_equivalence` (nullable additions), `qtap_import_equivalence`
DATA (the seed `lorian-and-riya.qtap` is a `characters` export with NO chats —
nothing reminted; aboutCharacterId now also tries local rows, none on a fresh
target) once the builder is fixed, `ai_import_assembly_equivalence`,
`wardrobe_wear_import_remap_equivalence`, `import_warning_text_guard` (no new
warning tails), `chat_export_equivalence` (chat GET — the scenarios lane may
move it).

**Corpus rows to ADD**

- Restore archives (copy `derive-restore-archive-memory-tiers.py`'s shape —
  it already plants a chat watermark; md5-check the source archive):
  1. `restore-archive-legacy-vectors.zip`: from `restore-archive.zip`, re-id the
     vector entries so one names `ad…01` (archive memory with NO embedding →
     ADOPTS), one names a memory given its OWN `embedding` (keeps its own), plus
     the orphan `bb…01`; a fourth names a memory carrying `{}` (adopts — the
     index-keyed empty is "no embedding" here). Both modes; assert
     `legacyVectorsAdopted` and the restored `memories.embedding` bytes.
  2. `restore-archive-cursors.zip`: one chat with all three legacy keys
     (other/episode → its messages, turn → an id NOT in the archive — still
     written), one with `""` / `null` values (no cursor), one with only the
     OTHER key (the baseline-vintage shape); replace + new-account (the
     remapped cursor must equal its message's new id); a replace onto a target
     already holding a cursor for that chat id (the truncate).
  3. `restore-archive-scenario-source.zip`: a valid source naming an archived
     store; a source as a JSON STRING; an INVALID source (bad uuid) — replace
     refuses the chat, new-account nulls it and restores (§9).
  4. `restore-archive-legacy-settings.zip`: the chat-settings row carrying
     `autoHousekeepingSettings` as a JSON string with `perCharacterCap: 50`
     (→ 100), an override `"abc"` (dropped) and one `250000` (→ 100000), and an
     instance row `memoryExtractionConcurrency`; a sibling carrying its OWN
     `memoryMaintenance` (kept, legacy ignored); one whose legacy value is
     `null` (no key derived).
  5. TF-IDF rows: one without `textShapeVersion` (→ 0), one with 1.
- `uuid-remap-corpus.json`: `chat_episode_watermark`, `chat_turn_watermark`
  (copy `chat_other_watermark`, `backup-uuid-remap.test.ts:768-782`),
  `chat_scenario_source` (store first seen at the chat vs. at
  `docMountPoints` — mint order), `chat_scenario_source_json_string`,
  `chat_scenario_source_invalid` (→ null).
- Backup / export plants (system-data family, after narrowing): cursor rows for
  CHAT_1 (all three passes) and none for a second chat; a `scenarioSource` /
  `scenarioNotes` on a chat; `supersededAt` on the cold member `MEM_3`;
  `textShapeVersion = 1` on the vocabulary; export both `scope: all` (preloaded
  read) and a single selected chat (per-chat read).
- Import-execute (copy `execute_chat_watermark` + `chatWatermarkPayload`,
  oracle `:1922-1938`): `execute_duplicate_remint` (same-instance duplicate —
  every message lands, `swipe-<id>` groups rewritten, an opaque `swipe-3`
  kept, summary anchors all resolving vs one unresolved → summary cleared,
  `courierCheckpoints` as a JSON string and an unreadable one, informs /
  annotations / memories / `linkedTo` re-pointed); `execute_twice_fresh_ids`
  (no `preserveIds`, twice); `execute_shared_message_ids` (two bundle chats
  with the same message id); `execute_message_insert_fails` (plant a trigger
  refusing one message → the sidecar pointing at it lands `null` / is
  dropped, and the NEW WARN); `execute_preserve_ids_keeps_messages`;
  `execute_chats_only_memories` (bug 240: a local character, an ARCHIVED local
  character, an unknown one, an unknown `aboutCharacterId`);
  `execute_cursor_bundle` (three keys through reminting and through
  preserveIds); `execute_scenario_source_{mapped,local,dropped}`.
- NEW tier-1 families (pure v4 functions with injectable inputs — cheapest
  exactness): `message_id_remap_equivalence` (`remintChatMessageIds` with a
  deterministic counter `mintId`, `remapImportedMessageId`; mirror the 14
  jest cases of `duplicate-chat-message-ids.test.ts`), `cursor_bundle_equivalence`
  (`cursorsToBundleFields`, `takeBundleCursors`), `settings_legacy_equivalence`
  (`legacyHousekeepingToMaintenance`, `withMemoryMaintenanceFromLegacy` — bytes
  of the `JSON.stringify`ed value), `scenario_source_remap_equivalence`
  (`readScenarioSource` / `remapScenarioSource` incl. UTF-16 length edges on
  `path` / `bodySha256`). Closest recipe: any `harness/oracle/cases/*.ts`
  NDJSON pure case (e.g. `wardrobe-wear-import-remap.ts`, which imports one v4
  module and emits per-row outputs).
- Census: add `Restored memories`, remove the two vector lines in BOTH lists
  (`system-restore.test.ts:815-…` and `system_restore_state.rs:5795-…`); the
  `seen == N` / `FRESH_STORE_CARVED_CASES` counts move per added replace case
  (memory note `restore-census-whitelist`). Consider a sibling whitelist for
  the four `[ExtractionCursors]` cursor-bundle lines + the repository's
  `Cursor set` DEBUG (root logger) so the carriers' cursor writes are compared.

---

## 6. v4 jest tests at the pin covering the slice (mirror these)

- `__tests__/unit/lib/import/quilltap-import/duplicate-chat-message-ids.test.ts`
  — 14 cases (8 `executeImport` + 6 `remintChatMessageIds`): duplicate beside
  original; twice without preserveIds; memory `sourceMessageId`; summary kept
  when every anchor resolves; repeated message id in one chat; two chats
  sharing ids (per-chat keying); no sidecar to a failed insert; preserveIds
  keeps ids; opaque swipe key; repeated-id first-occurrence mapping; bare swipe
  key equal to an id; Courier checkpoints as a JSON string; unreadable
  checkpoints dropped; unresolvable watermarks + checkpoints nulled, absent
  fields absent.
- `…/quilltap-import/import-memories-local-characters.test.ts` — 5 (bug 240).
- `…/quilltap-import/reconcile-scenario-source.test.ts` — 3 (mapped / kept
  local / dropped).
- `lib/scenarios/__tests__/source-remap.test.ts` — 3.
- `__tests__/unit/lib/backup/restore-field-fidelity.test.ts` — 23 (incl. the
  bundled cursors leaving the chat row → `setCursor` ×3, and the legacy-vector
  adoption: bare adopts / own kept / no-mirror stays null / orphan creates
  nothing / `legacyVectorsAdopted === 1` / `reloadAll` once).
- `__tests__/unit/lib/backup/uuid-remapper.test.ts` — 59 (incl. scenario-source
  re-point, episode + turn watermark follow their messages).
- `__tests__/unit/lib/database/migration/move-memory-maintenance-settings-v1.test.ts`
  — the `withMemoryMaintenanceFromLegacy` restore arm ("keeps a backup's own
  memoryMaintenance key and adds nothing without a legacy value").
- `__tests__/unit/lib/backup/compact-backup.test.ts` — 4; `delete-service.test.ts`
  — 6 (`retireMemories` mocked); `chat-informs-backup.test.ts`,
  `wardrobe-wear-backup.test.ts`, `restore-embedding-reconcile.test.ts`,
  `restore-store-binding.test.ts` (mock-shape edits only).
- `__tests__/unit/lib/import/chat-informs-import.test.ts` — 10 (the remap
  through `idMaps`); `import-files.test.ts` — 9 (reminted `linkedTo`);
  `import-memory-embeddings.test.ts` (the forced refit).

---

## 7. Cross-lane meeting points

### 7.1 NEEDS from the schema KEYSTONE

1. **The cursor repository** (twin of `memory-extraction-cursors.repository.ts`):
   proposed `crate::db::memory_extraction_cursors`:
   ```rust
   #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
   pub enum ExtractionCursorPass { OtherFold, EpisodeFold, Turn }   // as_str(): "other-fold" | "episode-fold" | "turn"
   impl ExtractionCursorPass { pub const ALL: [Self; 3] = [Self::OtherFold, Self::EpisodeFold, Self::Turn]; }
   /// v4 `ExtractionCursorMap` — at most one message id per pass.
   #[derive(Clone, Default, PartialEq, Eq, Debug)]
   pub struct CursorMap { pub other_fold: Option<String>, pub episode_fold: Option<String>, pub turn: Option<String> }
   pub fn extraction_cursor_id(chat_id: &str, pass: ExtractionCursorPass) -> String; // "{chatId}:{pass}"
   pub struct MemoryExtractionCursorsRepository<'c> { … }
   impl MemoryExtractionCursorsRepository<'_> {
       pub fn find_by_chat_id(&self, chat_id: &str) -> Result<CursorMap, DbError>;          // throws on failure (v4 :56-65)
       pub fn find_all_by_chat(&self) -> Result<IndexMap<String, CursorMap>, DbError>;     // v4 :77-91
       pub fn set_cursor(&self, chat_id: &str, pass: ExtractionCursorPass, message_id: &str) -> Result<(), DbError>; // the raw upsert + DEBUG `[ExtractionCursors] Cursor set`
       pub fn clear_for_chat(&self, chat_id: &str) -> Result<usize, DbError>;
   }
   ```
   plus an absent-table read that answers "none" WITHOUT creating the table
   (backup / export run on the source instance).
2. **Table present on every booted instance** (fresh_schema re-dump + the
   ruled `add-memory-extraction-cursors-v1` ensure, both shapes) — restore /
   import `INSERT`s depend on it; add to `restore_vintage_state`'s boot-align.
3. **The chat row**: `ChatCreate` / `ChatUpdate` lose
   `other_extraction_watermark_message_id` (`db/chats.rs:411-417`, `:999`);
   `chats_read` stops emitting it (`:96`, `:322`); `ChatCreate` gains
   `scenario_source: Option<ScenarioSource>` (serialized as JSON TEXT, refusing
   an invalid object as `ChatMetadataSchema` does — that refusal IS the
   replace-restore's / import's "Failed to … chat" arm) and `scenario_notes:
   Option<String>`; `ChatUpdate` gains `scenario_source: Option<Option<
   ScenarioSource>>` (reconcile writes `null`); `chats_read` emits
   `scenarioSource` as the PARSED object right after `scenarioText`, then
   `scenarioNotes`. Proposed `ScenarioSource { mount_point_id, path,
   body_sha256 }` with `#[serde(rename_all = "camelCase")]` and a
   `validate()` mirroring Zod (UUID, `path` 1..=500 UTF-16 units, `bodySha256`
   exactly 64 UTF-16 units) — home `crate::db::chats` or the scenarios lane's
   module; carriers only CONSUME it.
4. **The memory row**: `MemCreate` / `parse_create_memory`
   (`db/memories.rs:172-260`) gain `superseded_at` (`TimestampSchema.nullable()
   .optional()`); `memories_read` emits it after `supersededById`.
5. **TF-IDF**: `TvCreate` (`db/tfidf_vocabulary.rs:109-124`) gains
   `text_shape_version: f64` (or `i64`); collect reads the column.
6. **Chat settings**: `ChatSettingsCreate` (`db/chat_settings.rs:814-…`) loses
   the three fields and must keep IGNORING unknown keys (no
   `deny_unknown_fields`) — the restore hands it legacy rows.
7. **The settings translator** (one copy, used first by the keystone's
   `move-memory-maintenance-settings-v1` ensure): proposed
   `crate::memory_settings_legacy::{legacy_housekeeping_to_maintenance(raw:
   &Value) -> Option<Value>, with_memory_maintenance_from_legacy(rows:
   &[Value], chat_settings: &[Value]) -> Vec<Value>, DEAD_INSTANCE_SETTING_KEYS,
   MEMORY_MAINTENANCE_KEY}` — `Value` in / out so the JSON bytes (`{"enabled":
   …,"perCharacterCap":…,"perCharacterCapOverrides":{…}}`, insertion-ordered
   overrides) are exactly `JSON.stringify`'s.
8. **Retire path** (retire lane, Phase B): `crate::services::memory_retire::
   retire_memories(conn, ids: &[String], opts: RetireOptions { reason:
   RetireReason::RestoreReplace, keep: None }) -> Result<RetireReport, DbError>`
   runnable inside `delete_user_data_on_writer`'s `Db::write` closure, with the
   announce effects returned for after-commit (the v4 parent-commit hook
   shape).
9. **The vector tables retired**: `db::vector_indices` / `db::vector_store`
   deleted by the keystone / retire lane — the carriers must delete their four
   uses (`collect.rs:647-673,969-998`, `orchestrator.rs:1766-1803`,
   `delete_all.rs:205-206`) in the SAME union or the build breaks.
10. **Embedding reconcile result** narrowed (embedding lane):
    `EmbeddingDimensionReconcileResult` without `vector_entries_deleted` /
    `vector_index_meta_fixed` (also `quilltap-host/src/host.rs:1229`).
11. **Memory index cache**: if any lane introduces an in-process index (v4
    `memoryIndex`), expose `reload_all()`; otherwise step 10 is a recorded
    no-op.

### 7.2 PROVIDES to siblings

- `crate::services::extraction_cursor_bundle` (proposed home; twin of
  `cursor-bundle.ts`): `BUNDLE_CURSOR_FIELDS: [(ExtractionCursorPass, &str); 3]`,
  `cursors_to_bundle_fields(&CursorMap) -> Map<String, Value>`,
  `load_bundle_cursors(conn) -> Option<IndexMap<String, CursorMap>>` (WARN),
  `with_bundle_cursor_fields(conn, chat: &mut Map<String, Value>, preloaded:
  Option<&IndexMap<…>>)` (appends; WARN), `take_bundle_cursors(chat: &mut
  Map<String, Value>) -> CursorMap`, `restore_bundle_cursors(conn, chat_id,
  &CursorMap) -> usize` (WARN / DEBUG). Consumers: backup, restore, export,
  import, `message_id_remap`. (The extraction lane reads cursors through the
  repository, not this module.)
- `crate::services::quilltap_import::message_id_remap::{remint_chat_message_ids(
  chat: &Map<String, Value>, messages: &[Value], mint: impl FnMut() -> String)
  -> RemintedChatMessages, remap_imported_message_id(maps: &MessageIdMaps,
  source_chat_id: Option<&str>, message_id: &str) -> Option<String>}` where
  `MessageIdMaps = HashMap<String, HashMap<String, String>>` and
  `RemintedChatMessages { messages: Vec<Value>, chat_patch: Map<String, Value>,
  id_map: HashMap<String, String>, dropped_refs: usize, summary_cleared: bool }`
  — JSON-level (the patch is spread over the bundle JSON before the typed
  decode).
- `crate::services::scenario_source_remap::{read_scenario_source(&Value) ->
  Option<ScenarioSource>, remap_scenario_source(&Value, impl FnOnce(&str) ->
  Option<String>) -> Option<ScenarioSource>}` — used only by the carriers at
  the pin (`git grep` finds no other caller).
- `IdMaps.messages: MessageIdMaps` (`quilltap_import/mod.rs:321`).

### 7.3 Carrier key order (the C1 contract)

- **Backup chat** (`data/chats.json`): `chats_read` keys (schema order, with
  `scenarioSource`, `scenarioNotes` after `scenarioText`, no watermark) →
  `otherExtractionWatermarkMessageId`? → `episodeExtractionWatermarkMessageId`?
  → `turnExtractionWatermarkMessageId`? → `messages`.
- **Export chat record** (`{kind:"chat", data}`): schema-template keys →
  the three cursor keys (present ones, that order) → `_tagNames`? →
  `_participantInfo`?.
- **Memory** (backup / export / import payload): … `tier`, `supersededById`,
  `supersededAt`, `consolidatedFrom`, `consolidatedAt` ….
- **TF-IDF vocabulary**: … `includeBigrams`, `fittedAt`, `textShapeVersion`,
  `createdAt`, `updatedAt`.
- **Manifest counts**: … `tfidfVocabularies`, `docMountPoints` …; staging file
  order … `tfidf-vocabularies.json`, `doc-mount-points.json` ….
- **RestoreSummary**: … `tfidfVocabularies`, [`legacyVectorsAdopted` — restore
  only], `docMountPoints` … `wardrobeWear`, [`embeddingReconcile`
  `{targetDimensions, skippedReason, reindexEnqueued}` — restore only],
  `warnings`.
- **Restored instance settings**: archive rows minus dead keys, then the derived
  `memoryMaintenance` LAST.

### 7.4 Wire (`api/types.rs`)

No `Request` / `Response` variant changes: `SystemRestorePreview` /
`SystemRestoreExecute` (`api/types.rs:3087-3108`) answer
`serde_json::to_value(summary)` (`api/system_backup.rs:223`); only the
summary's bytes move (§7.3). The `.qtap` import/export requests are unchanged.

### 7.5 Ordering inside the union

The keystone's deletions (vector repos, the chat watermark field, the three
chat-settings fields) and the carriers' removal of their callers must land in
one compile; propose the carriers lane builds on the keystone branch
(P4.D279-style keystone → lanes) and owns the four vector call sites.

### 7.6 `BackupData.vector_entries`

v5 shares one `BackupData` between collect and parse (`collect.rs:89-133`).
v4 keeps `vectorEntries` on the type as "legacy only … never written". Keep the
field (parse fills it, collect leaves it empty, staging never writes it,
`uuid_remap.rs:357-361` keeps remapping it) — or split a `LegacyArchiveExtras`
off the parse side. Planner's call; the former is the smaller diff.

---

## 8. SPA / help / docs / CLI touch points

- **SPA:** none. `apps/web/src/app/screens/settings/system/restore-dialog.ts:14`
  types only a subset of `RestoreSummary` with no vector keys; v4's components
  are untouched by the span for the carriers.
- **help:** `help/system-import-export.md` — two hunks (`8050958be` +1 line
  under "Create New": the "(imported)" copy keeps its transcript, swipes,
  summaries and Informs; `93c5af6ad` +2 lines: a chats-only export's memories
  find their characters, an archived one "is not receiving visitors"). v5's
  copy is byte-identical to the baseline today. (Steampunk register — vendored,
  not rewritten.)
- **docs/v4:** `docs/developer/bugs/fixed/bug-228-duplicate-import-drops-messages.md`,
  `bug-240-chat-export-memories-dropped.md` (NEW); `bugs.md` (the doc lane).
  `docs/BACKUP-RESTORE.md` is UNCHANGED in the span.
- **CLI:** none — v5's `quilltap` has no backup/restore/import summary printer;
  v4's `packages/quilltap` carrier commands are untouched.

---

## 9. Open points for the order (observations — not rulings)

1. **Lane split.** The slice is two loosely coupled halves: (A) backup +
   restore + delete_all (vector retirement, cursors, settings, tfidf, scenario
   remap, census) and (B) `.qtap` export + import (bug 228 remint + sidecars,
   bug 240, cursors, scenario reconcile, inform `""`→`null`, schema re-vendor,
   key-order re-dump). They share only `extraction_cursor_bundle` and
   `scenario_source_remap`. Either one carriers lane (~2.5 k lines of change +
   corpus) or two with the two helper modules landed by the keystone (or by
   lane A with B sequenced). Bug 228 alone is a sizeable tier-2 surface.
2. **Who homes the shared helpers** — `memory_settings_legacy` (keystone's
   ensure is first consumer), `extraction_cursor_bundle` (carriers only, but
   needs the keystone's repo types), `scenario_source_remap` (carriers only, but
   needs the scenarios lane's `ScenarioSource`).
3. **`memoryIndex.reloadAll()`** — v5 has no index cache; record a no-op
   unless the round introduces one.
4. **v4 gap (candidate filing):** a NEW-ACCOUNT restore remaps neither a legacy
   `autoHousekeepingSettings.perCharacterCapOverrides` nor a
   `memoryMaintenance.perCharacterCapOverrides` — both keyed by character id —
   so per-character caps silently stop applying after a new-account restore.
   Mirror (v4-faithful) or fix-and-record?
5. **v4 asymmetry:** the restore drops the dead `memoryExtractionConcurrency`
   key but a `.qtap` `instance-settings` import writes it raw. Faithful by
   default.
6. **v4 gap:** v4's chat delete never clears `memory_extraction_cursors`; on a
   generateDDL-shaped (fresh) table there is no FK, so an `overwrite` import
   without `preserveIds` (delete + fresh id) and every ordinary chat delete
   orphan cursor rows (the migration-shaped table cascades). Only the replace
   restore truncates. Cascade-delete lane's call; flag to the keystone (two
   table shapes = two behaviours).
7. **Scenario-source asymmetry:** replace mode REFUSES a chat whose archived
   `scenarioSource` fails the schema; new-account mode NULLS it and restores.
   v4-faithful.
8. **Inform `""` → `null`** silently changes `execute_inform_refusals`' planted
   row from "refused" to "lands" — a by-design flip; the order should name it
   so the lane doesn't "fix" the oracle back.
9. **Retire side effects inside the replace wipe** (realtime `memories`,
   Commonplace mirror refresh for retired digests) now run during a restore;
   v5's wipe runs inside one writer closure — the retire lane's after-commit
   contract must be honoured here.
10. **TF-IDF `textShapeVersion: null`** in an archive: v4 Zod-refuses the
    vocabulary (warning + WARN); v5's `n(v, key, 0.0)` would default it.
    Mirror the refusal or record a divergence.
11. **Legacy adoption edge cases:** duplicate mirror ids (last wins, Map
    semantics); a legacy vector whose dimension mismatches the target profile
    restores anyway and is caught by step 25's reconcile — no special casing.
12. **Pre-`c2795733f` backups** restore superseded rows with `supersededAt =
    null` (v4 backfills only in the boot migration, not at restore); retention
    falls back to `updatedAt` (the schema description says so). Faithful.
13. **The compact DEBUG `Compact backup — embeddings and derived tables
    omitted`** has never been ported to v5 (pre-existing gap, not span) —
    smalls candidate if the lane touches `collect`/`createBackup`.
14. **Tier-3 loud refusal candidates:** none in this slice — every carrier
    change is deterministic (tier 1 / 2). The import refit stays the existing
    P4.D62 loud deferral.
15. **Live-data:** a v5 build at the CURRENT baseline restoring a pin-era v4
    backup loses every cursor except none (it reads only the chat column it
    re-adds) — hazard (5) of the ledger; this slice closes it. No v5 carrier
    may be dogfooded on a post-2026-10-13-22:24-UTC Friday copy until the
    keystone lands.
16. **Ledger corrections:** none of substance — the §3 rows' carrier claims
    match the hunks. Two refinements: (a) `8050958be`'s row says the inform
    refusal "reason keeps the SOURCE id" — true, AND the falsy-pointer
    `""` → `null` change rides the same hunk (unrecorded); (b) `545e299bb`'s
    "Backup: whole-table, nothing to do" holds for v4 — for v5 it depends on
    `chats_read` emitting the two keys (keystone / scenarios lane), and the
    new-account remap needs `uuid_remap.rs` work.
