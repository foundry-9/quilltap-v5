/**
 * Read-differential fixture builder — the THREE databases (main + mount-index +
 * llm-logs) for the `self_inventory` tool handler (W4.1d).
 *
 * Seeds, through v4's REAL repos:
 *   - Four full-vault characters (`repos.characters.create`, pinned ids) — Aria
 *     (self, canBeCarina), Cass (user persona), Beck (present LLM peer), Dara (a
 *     group-only member, canBeCarina). Each mints a linked vault mount + scaffolds
 *     the preset folders/files.
 *   - A connection profile + a roleplay template (`repos.connections.create` /
 *     `repos.roleplayTemplates.create`).
 *   - A real project + a real group (`repos.projects.create` / `repos.groups.create`
 *     — each provisions an official store) and two group memberships (Aria, Dara)
 *     via the REAL `GroupCharacterMembersRepository`.
 *   - A chat (`repos.chats.create`) with three present participants
 *     (Aria=llm/self, Cass=user, Beck=llm), `allowCrossCharacterVaultReads=true`,
 *     `projectId`, `roleplayTemplateId`.
 *   - Aria's memories (`repos.memories.create`, pinned) at a spread of importances
 *     (three ≥ 0.7).
 *   - Two chat_documents for the chat (`repos.chatDocuments.create`).
 *   - An llm_logs row for the chat in the LLM-LOGS sibling DB (real
 *     `LLMLogsRepository`, with a usage object) so the lastTurn section resolves.
 *   - P4.D378 (v4 `0c47f4539`, the shared group walk): three NEW characters
 *     (Gale / Hale / Ivo), three groups, an orphan membership + link for a group
 *     id with NO `groups` row (its own plain store), Cartographers Circle linked
 *     to Weavers Guild's official store, and the R-F plant — Ruined Rotunda's
 *     `properties.json` overwritten with `{` (hydrated `findById` throws; raw
 *     row whole — self-checked here). Widening adds rows only: `sqlite_master`
 *     of all three databases is byte-identical before and after, and the only
 *     pre-existing case output that moves is the Carina roster (it lists every
 *     character; three non-answerers join it).
 *
 * Both the oracle and the Rust port then READ a COPY of the SAME baked fixtures, so
 * every cell is identical on both sides. The ONLY genuinely nondeterministic value
 * is the physicalDescription base's createdAt/updatedAt, minted at read time — but
 * self_inventory's prompt/character reads never surface a physicalDescription (the
 * seed characters carry none), and the sections it does surface are all pinned, so
 * the differential runs with NO normalization.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MAIN=/tmp/qt-selfinv-main.db \
 *   QT_FIXTURE_MOUNT=/tmp/qt-selfinv-mount.db \
 *   QT_FIXTURE_LLMLOGS=/tmp/qt-selfinv-llmlogs.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-self-inventory-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CharacterSpec {
  id: string;
  name: string;
  description: string | null;
  identity: string | null;
  personality?: string;
  canBeCarina: boolean;
  /** P4.D65: set only on the archived member of the cast (see `makeChar`). */
  archivedAt?: string;
}

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  userId: string;
  aria: CharacterSpec;
  beck: CharacterSpec;
  cass: CharacterSpec;
  dara: CharacterSpec;
  edda: CharacterSpec;
  fenn: CharacterSpec;
  general: { mountPointId: string; name: string };
  chatSettingsId: string;
  wardrobe: Array<{ characterId: string | null; item: { id: string; title: string; types: string[]; imagePrompt?: string; componentItemIds?: string[] } }>;
  inventory: Array<{ scope: string; owner: string | null; record: Record<string, unknown> }>;
  carried: { characterId: string; ids: string[] };
  customTool: { path: string; definition: Record<string, unknown> };
  coreFilePlants: { delete: string; empty: string };
  connectionProfile: { id: string; name: string; provider: string; modelName: string };
  roleplayTemplate: { id: string; name: string; systemPrompt: string };
  project: { id: string; name: string };
  group: { id: string; name: string };
  chat: { id: string; title: string; touchedUpdatedAt: string; participants: Array<Record<string, unknown>> };
  llmLog: {
    id: string;
    provider: string;
    modelName: string;
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    createdAt: string;
  };
  memories: Array<{ id: string; importance: number }>;
  /** P4.D378 (v4 `0c47f4539`): the shared group walk's three shapes. */
  pools: {
    characters: CharacterSpec[];
    groups: Array<{ id: string; name: string; corruptProperties?: string }>;
    orphan: {
      groupId: string;
      storeId: string;
      storeName: string;
      doc: { path: string; content: string };
    };
    memberships: Array<{ id: string; groupId: string; characterId: string }>;
    links: Array<{ id: string; groupId: string; mountPointId: string }>;
  };
  chatDocuments: Array<{
    id: string;
    filePath: string;
    scope: string;
    mountPoint: string | null;
    displayTitle: string | null;
  }>;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'self-inventory.json'), 'utf8')) as Spec;

  const outMain = process.env.QT_FIXTURE_MAIN;
  const outMount = process.env.QT_FIXTURE_MOUNT;
  const outLlm = process.env.QT_FIXTURE_LLMLOGS;
  if (!outMain || !outMount || !outLlm) {
    throw new Error('QT_FIXTURE_MAIN, QT_FIXTURE_MOUNT and QT_FIXTURE_LLMLOGS must be set');
  }
  for (const base of [outMain, outMount, outLlm]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = base + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-selfinv-build-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = outMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = outMount;
  process.env.SQLITE_LLM_LOGS_PATH = outLlm;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase, ensureCollection, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { GroupCharacterMembersRepository } = await import(
    '@/lib/database/repositories/group-character-members.repository'
  );
  const { LLMLogsRepository } = await import(
    '@/lib/database/repositories/llm-logs.repository'
  );
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const { UserSchema, CharacterSchema } = await import('@/lib/schemas/types');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
    GroupDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();

  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);

  // Materialize the mount-index store tables (the ports never issue CREATE TABLE).
  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
    ['project_doc_mount_links', ProjectDocMountLinkSchema],
    ['group_doc_mount_links', GroupDocMountLinkSchema],
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }

  // [40319484] `gcOrphanedFileRow` runs inside every content-addressed rewrite
  // and deletes from `doc_mount_blobs` unconditionally, so a mount index that
  // lacks the table now throws `no such table` on the SECOND write to any path.
  // v4's blobs repository creates it lazily from hand-written DDL
  // (`doc-mount-blobs.repository.ts:113`); copied verbatim so the fixture carries
  // exactly the table a real instance has (it is in `fresh_schema.json` too).
  midb.exec(`
    CREATE TABLE IF NOT EXISTS "doc_mount_blobs" (
      "id" TEXT PRIMARY KEY,
      "fileId" TEXT NOT NULL,
      "sha256" TEXT NOT NULL,
      "sizeBytes" INTEGER NOT NULL,
      "storedMimeType" TEXT NOT NULL,
      "data" BLOB NOT NULL,
      "createdAt" TEXT NOT NULL,
      "updatedAt" TEXT NOT NULL,
      FOREIGN KEY ("fileId") REFERENCES "doc_mount_files" ("id") ON DELETE CASCADE
    )
  `);
  midb.exec(
    'CREATE UNIQUE INDEX IF NOT EXISTS "idx_doc_mount_blobs_fileId" ON "doc_mount_blobs" ("fileId")'
  );

  const repos = getRepositories();

  // 1. User.
  await repos.users.create(
    { username: 'reader', name: 'Reader' } as never,
    { id: spec.userId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );

  // 2. Characters (each provisions + scaffolds a vault). canBeCarina is a DB
  // (slim) column; description/identity/personality are vault-managed and hydrate
  // back on read.
  const makeChar = async (c: CharacterSpec): Promise<void> => {
    await repos.characters.create(
      {
        name: c.name,
        userId: spec.userId,
        description: c.description,
        identity: c.identity,
        personality: c.personality ?? null,
        controlledBy: c.name === 'Cass' ? 'user' : 'llm',
        canBeCarina: c.canBeCarina,
      } as never,
      { id: c.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
    // P4.D65 (the banked P4.D63 unit-4 arm): the tombstone goes on by raw
    // UPDATE, because `validateCharacterArchivePatch` sanctions exactly one
    // patch on an archived row and the repository would refuse this one. The
    // character keeps her scaffolded vault, exactly as a real archived one
    // does — `buildCarinaSection` must exclude her on the FLAG.
    if (c.archivedAt) {
      await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', [c.archivedAt, c.id]);
    }
  };
  await makeChar(spec.aria);
  await makeChar(spec.beck);
  await makeChar(spec.cass);
  await makeChar(spec.dara);
  await makeChar(spec.edda);
  await makeChar(spec.fenn);

  // 3. Connection profile + roleplay template.
  await repos.connections.create(
    {
      userId: spec.userId,
      name: spec.connectionProfile.name,
      provider: spec.connectionProfile.provider,
      modelName: spec.connectionProfile.modelName,
    } as never,
    {
      id: spec.connectionProfile.id,
      createdAt: spec.seedTimestamp,
      updatedAt: spec.seedTimestamp,
    }
  );
  await repos.roleplayTemplates.create(
    { name: spec.roleplayTemplate.name, systemPrompt: spec.roleplayTemplate.systemPrompt } as never,
    { id: spec.roleplayTemplate.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // 4. Project + group (each provisions an official store). P4.D103 (v4
  // `8f868109`): both carry `instructions`, so the `buildPromptSection` report
  // reconstructs the standing-instructions section a live turn would carry —
  // the project's body holds a `{{char}}` placeholder so the template
  // processing is observable in the output.
  await repos.projects.create(
    {
      userId: spec.userId,
      name: spec.project.name,
      instructions: spec.project.instructions,
    } as never,
    { id: spec.project.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );
  await repos.groups.create(
    { name: spec.group.name, instructions: spec.group.instructions } as never,
    { id: spec.group.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never
  );

  // 5. Group memberships (Aria + Dara), pinned ids.
  const members = new GroupCharacterMembersRepository();
  await members.create(
    { groupId: spec.group.id, characterId: spec.aria.id },
    { id: 'a0a0a0a0-0000-4000-8000-000000000031', createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );
  await members.create(
    { groupId: spec.group.id, characterId: spec.dara.id },
    { id: 'a0a0a0a0-0000-4000-8000-000000000032', createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // 6. Chat with three present participants.
  await repos.chats.create(
    {
      userId: spec.userId,
      title: spec.chat.title,
      participants: spec.chat.participants,
      projectId: spec.project.id,
      roleplayTemplateId: spec.roleplayTemplate.id,
      allowCrossCharacterVaultReads: true,
    } as never,
    { id: spec.chat.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
  );

  // 7. Aria's memories.
  for (const m of spec.memories) {
    await repos.memories.create(
      {
        characterId: spec.aria.id,
        aboutCharacterId: null,
        chatId: null,
        projectId: null,
        content: `memory ${m.id}`,
        summary: `summary ${m.id}`,
        keywords: [],
        tags: [],
        importance: m.importance,
        embedding: null,
        source: 'AUTO',
        witnessedContext: null,
        sourceMessageId: null,
        lastAccessedAt: null,
        reinforcementCount: 1,
        lastReinforcedAt: null,
        relatedMemoryIds: [],
        reinforcedImportance: m.importance,
      } as never,
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // 8. chat_documents.
  for (const d of spec.chatDocuments) {
    await repos.chatDocuments.create(
      {
        chatId: spec.chat.id,
        filePath: d.filePath,
        scope: d.scope,
        mountPoint: d.mountPoint,
        displayTitle: d.displayTitle,
        isActive: true,
      } as never,
      { id: d.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp }
    );
  }

  // 9. llm_logs row (sibling DB) for the lastTurn section.
  const llmLogs = new LLMLogsRepository();
  await llmLogs.create(
    {
      userId: spec.userId,
      type: 'CHAT_MESSAGE',
      messageId: null,
      chatId: spec.chat.id,
      characterId: spec.aria.id,
      autonomousRunId: null,
      provider: spec.llmLog.provider,
      modelName: spec.llmLog.modelName,
      request: { messageCount: 1, messages: [] },
      response: { content: '', contentLength: 0, finishReason: 'stop' },
      usage: {
        promptTokens: spec.llmLog.promptTokens,
        completionTokens: spec.llmLog.completionTokens,
        totalTokens: spec.llmLog.totalTokens,
      },
      cacheUsage: null,
      rawProviderUsage: null,
      requestHashes: null,
      durationMs: null,
    } as never,
    { id: spec.llmLog.id, createdAt: spec.llmLog.createdAt, updatedAt: spec.llmLog.createdAt }
  );

  // 10. P4.D350 (v4 `074e1e01a`) — the holdings + core-files plants, all
  // through v4's REAL writers.
  //   a. Quilltap General (a database store + `instance_settings`).
  await repos.docMountPoints.create(
    {
      name: spec.general.name,
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: spec.general.mountPointId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );
  await rawQuery('CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)');
  await rawQuery('INSERT OR REPLACE INTO "instance_settings" ("key", "value") VALUES (?, ?)', [
    'generalMountPointId',
    spec.general.mountPointId,
  ]);
  //   b0. Two garments and one outfit made of them in Aria's vault (REAL
  //       `repos.wardrobe.create`).
  for (const w of spec.wardrobe) {
    await repos.wardrobe.create(
      {
        characterId: w.characterId,
        title: w.item.title,
        description: null,
        imagePrompt: w.item.imagePrompt ?? null,
        types: w.item.types,
        componentItemIds: w.item.componentItemIds ?? [],
        appropriateness: null,
        isDefault: false,
        replace: false,
        migratedFromClothingRecordId: null,
        archivedAt: null,
      } as never,
      { id: w.item.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
    );
  }
  //   b. The Inventory objects (REAL `resolveInventoryLocation` + `create`) and
  //      Aria's carried set (REAL `setEquippedInventory`).
  const { resolveInventoryLocation } = await import('@/lib/inventory/location');
  for (const o of spec.inventory) {
    const loc = await resolveInventoryLocation(o.scope as never, o.owner, repos, spec.userId, { ensure: true });
    if (!loc) throw new Error(`no inventory location for ${o.scope}:${o.owner}`);
    await loc.create(o.record as never);
  }
  if (!(await repos.chats.setEquippedInventory(spec.chat.id, spec.carried.characterId, spec.carried.ids))) {
    throw new Error('carried-set plant failed');
  }
  //   c. One custom tool in Aria's vault; her manifesto deleted and her
  //      example dialogues emptied (REAL database-store writers).
  const ariaRow = await repos.characters.findById(spec.aria.id);
  const ariaVault = ariaRow?.characterDocumentMountPointId;
  if (!ariaVault) throw new Error('Aria has no vault');
  const { writeDatabaseDocument, deleteDatabaseDocument } = await import('@/lib/mount-index/database-store');
  await writeDatabaseDocument(ariaVault, spec.customTool.path, JSON.stringify(spec.customTool.definition, null, 2));
  await deleteDatabaseDocument(ariaVault, spec.coreFilePlants.delete);
  await writeDatabaseDocument(ariaVault, spec.coreFilePlants.empty, '');
  //   d. One chat_settings row (customTools on — the default; one case turns it
  //      off for itself).
  const { ChatSettingsSchema } = await import('@/lib/schemas/settings.types');
  await ensureCollection('chat_settings', ChatSettingsSchema);
  await repos.chatSettings.create({ userId: spec.userId } as never, {
    id: spec.chatSettingsId,
    createdAt: spec.seedTimestamp,
    updatedAt: spec.seedTimestamp,
  });
  //   e. Fenn's vault comes unlinked (the pointer NULLed; the store stays).
  await rawQuery('UPDATE characters SET characterDocumentMountPointId = NULL WHERE id = ?', [spec.fenn.id]);

  // 11. P4.D378 (v4 `0c47f4539`) — the shared group walk (`groupMountsFor`):
  //     three NEW characters (Gale / Hale / Ivo, canBeCarina off, in no chat)
  //     so every pre-existing case reads exactly what it read before. All
  //     through v4's REAL repositories / document store.
  //   a. The characters (each provisions + scaffolds its own vault).
  for (const c of spec.pools.characters) await makeChar(c);
  //   b. The groups (each provisions an official store).
  for (const g of spec.pools.groups) {
    await repos.groups.create(
      { name: g.name } as never,
      { id: g.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp } as never,
    );
  }
  //   c. The orphan's store: a plain database store with one document. Its
  //      group id gets a membership + a link below but NO `groups` row.
  await repos.docMountPoints.create(
    {
      name: spec.pools.orphan.storeName,
      basePath: '',
      mountType: 'database',
      storeType: 'documents',
      includePatterns: [],
      excludePatterns: [],
      enabled: true,
      lastScannedAt: null,
      scanStatus: 'idle',
      lastScanError: null,
      conversionStatus: 'idle',
      conversionError: null,
      fileCount: 0,
      chunkCount: 0,
      totalSizeBytes: 0,
    } as never,
    { id: spec.pools.orphan.storeId, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
  );
  await writeDatabaseDocument(
    spec.pools.orphan.storeId,
    spec.pools.orphan.doc.path,
    spec.pools.orphan.doc.content,
  );
  //   d. Memberships, in join order (the walk reads them in rowid order).
  for (const m of spec.pools.memberships) {
    await members.create(
      { groupId: m.groupId, characterId: m.characterId },
      { id: m.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }
  //   e. Links. `official:<groupId>` resolves to that group's minted official
  //      store, read back from the RAW row.
  for (const l of spec.pools.links) {
    let mountPointId = l.mountPointId;
    if (mountPointId.startsWith('official:')) {
      const owner = await repos.groups.findByIdRaw(mountPointId.slice('official:'.length));
      if (!owner?.officialMountPointId) throw new Error(`no official store for ${mountPointId}`);
      mountPointId = owner.officialMountPointId;
    }
    await repos.groupDocMountLinks.create(
      { groupId: l.groupId, mountPointId } as never,
      { id: l.id, createdAt: spec.seedTimestamp, updatedAt: spec.seedTimestamp },
    );
  }
  //   f. R-F: the planted-UNREADABLE group store — malformed `properties.json`
  //      bytes through the REAL document writer (the `store-unavailable-routes`
  //      plant). The hydrated `groups.findById` throws; the raw row is whole.
  for (const g of spec.pools.groups) {
    if (g.corruptProperties === undefined) continue;
    const row = await repos.groups.findByIdRaw(g.id);
    if (!row?.officialMountPointId) throw new Error(`no official store for ${g.id}`);
    await writeDatabaseDocument(row.officialMountPointId, 'properties.json', g.corruptProperties);
    let threw = false;
    try {
      await repos.groups.findById(g.id);
    } catch {
      threw = true;
    }
    if (!threw) throw new Error(`R-F plant did not break the hydrated read of ${g.id}`);
  }

  // P4.D140 (v4 `735d9408c`): the chat has NO messages, so `lastMessageAt` is
  // NULL and `chatActivityAt` must fall back to `createdAt`. Push `updatedAt`
  // well past it so the chats section's `latestActivityAt` DISCRIMINATES: the
  // retired `lastMessageAt ?? updatedAt ?? createdAt` spelling would report this
  // value, the shipped one reports `createdAt`.
  await rawQuery('UPDATE chats SET updatedAt = ? WHERE id = ?', [
    spec.chat.touchedUpdatedAt,
    spec.chat.id,
  ]);

  closeMountIndexSQLiteClient();
  closeLLMLogsSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built self-inventory fixtures: main=${outMain} mount=${outMount} llmlogs=${outLlm}\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`self-inventory fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
