/**
 * The committed SUBPROMPTS fixture builder (P4.D163, v4 `2f4254b42`) — the
 * shared test-pepper substrate for `subprompts_storage_tier2_equivalence`,
 * `subprompts_routes_equivalence` and (P4.D164) `subprompts_prompt_tier2_
 * equivalence`.
 *
 * Everything is baked through v4's REAL repositories and database-store
 * (never hand-rolled SQL for content — `project-store-fixture-needs-real-
 * create`), with the clock FROZEN to `seedNowMs` so every `lastModified` /
 * `createdAt` the store mints is the seed sentinel: a read of a baked file
 * then compares its `updatedAt` EXACTLY on both sides (that is the
 * `toISOString(mtime)` byte proof), and only the writes the cases perform
 * mint anything (which the harness tokenizes).
 *
 * ## What is in it and why (the order's fixture spec, trimmed to the question)
 *
 *   - ONE user.
 *   - CHAR_A — live, vault, a `Subprompts/` folder holding:
 *       `terse.md`        title "Be terse"
 *       `Verse.md`        MIXED-CASE file name, title "Answer in verse"
 *                         (the case-insensitive match arm: the seats carry
 *                         `"VERSE"`)
 *       `no-title.md`     NO frontmatter — the title falls back to the id
 *       `zulu.md`         title "Alpha-titled zulu"   ┐ titles collate the
 *       `alpha.md`        title "Zulu-titled alpha"   ┘ opposite way to the ids
 *       `broken.md`       a link whose DOCUMENT row is deleted after the write
 *                         (raw SQL — the "unreadable file is SKIPPED with a
 *                         warn" arm; it still LISTS as a link)
 *       `drafts/x.md`     nested — must NOT list (`isRootSubpromptFile`)
 *       `notes.txt`       in the folder but not `.md` — must NOT list
 *   - CHAR_B — live, vault, NO `Subprompts/` folder (a create must ensure it).
 *   - CHAR_C — live, NO vault: `characterDocumentMountPointId` is NULLed by
 *     raw SQL after the create (the provision-on-write arm — v4's
 *     `ensureCharacterVault` then ADOPTS the orphaned same-name populated
 *     store, exactly one candidate, so the adopted id is fixture-baked and
 *     the census stays remap-free; both sides run that same adopt logic).
 *   - CHAR_D — ARCHIVED (`archivedAt` = the sentinel, raw SQL — the column
 *     exists at this vintage), vault with one subprompt `keep.md`: reads
 *     resolve, every write refuses 409.
 *   - Five chats: CHAT_LLM (A as an LLM seat carrying `["terse","VERSE"]` —
 *     one id matching only case-insensitively); CHAT_USER (A USER-controlled
 *     carrying `["terse"]` — never fanned out); CHAT_REMOVED (A `status:
 *     'removed'` carrying `["terse"]` — never fanned out); CHAT_NOKEY (A with
 *     NO key — a pre-feature row); CHAT_TWO (A as an LLM seat carrying
 *     `["terse"]` beside B's LLM seat carrying `["terse"]` — B's must NOT be
 *     touched by a fan-out over A).
 *
 * Regenerate (Node 24, from the v4 checkout — a pinned worktree while v4 HEAD
 * is past the baseline; the committed .db files land in place):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC \
 *   QT_FIXTURE_SP_MAIN=$V5W/crates/quilltap-web/tests/fixtures/subprompts-main.db \
 *   QT_FIXTURE_SP_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/subprompts-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-subprompts-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  seedTimestamp: string;
  seedNowMs: number;
  userId: string;
  ids: Record<string, string>;
}

async function main(): Promise<void> {
  const offset = new Date().getTimezoneOffset();
  if (offset !== 0) {
    throw new Error(`subprompts fixture must be built under TZ=UTC (getTimezoneOffset=${offset})`);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'subprompts.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;
  const I = spec.ids;

  const mainOut = process.env.QT_FIXTURE_SP_MAIN;
  const mountOut = process.env.QT_FIXTURE_SP_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_SP_{MAIN,MOUNT} must point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }
  if (existsSync(mainOut + '.meta.json')) rmSync(mainOut + '.meta.json');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-subprompts-fixture-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Freeze the clock so every store-minted timestamp is the seed sentinel.
  const RealDate = Date;
  const iso = new RealDate(spec.seedNowMs).toISOString();
  if (iso !== TS) throw new Error(`seedNowMs ${spec.seedNowMs} is not seedTimestamp ${TS} (${iso})`);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  global.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      if (a.length === 0) super(iso);
      // @ts-expect-error forward variadic args
      else super(...a);
    }
    static now(): number {
      return spec.seedNowMs;
    }
  } as unknown as DateConstructor;

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema, ChatMetadataSchema, BackgroundJobSchema } = await import(
    '@/lib/schemas/types'
  );
  const { UserSchema } = await import('@/lib/schemas/auth.types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
    ProjectDocMountLinkSchema,
  } = await import('@/lib/schemas/mount-index.types');
  const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
  const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
  const { composeSubpromptContent, SUBPROMPTS_FOLDER } = await import(
    '@/lib/subprompts/subprompts'
  );

  await initializeDatabase();
  await ensureCollection('users', UserSchema);
  await ensureCollection('characters', CharacterSchema);
  await ensureCollection('chats', ChatMetadataSchema);
  await ensureCollection('background_jobs', BackgroundJobSchema);

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
  ];
  for (const [name, schema] of ddl) {
    for (const sql of generateDDL(name, schema as never)) midb.exec(sql);
  }
  const repos = getRepositories();
  // doc_mount_blobs has hand-written DDL (BLOB column) — trigger the CREATE via a read.
  await repos.docMountBlobs.listByMountPoint('00000000-0000-4000-8000-000000000000');
  await rawQuery(
    'CREATE TABLE IF NOT EXISTS "instance_settings" ("key" TEXT PRIMARY KEY, "value" TEXT NOT NULL)',
  );

  // 1. The user.
  await repos.users.create(
    { username: 'friday', email: null, name: 'Friday' } as never,
    { id: spec.userId, createdAt: TS, updatedAt: TS } as never,
  );

  // 2. Characters — `create` owns vault provisioning.
  const vaults: Record<string, string> = {};
  const mkChar = async (key: string, id: string, name: string, extra: object = {}) => {
    await repos.characters.create(
      { name, userId: spec.userId, controlledBy: 'llm', ...extra } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
    const raw = await repos.characters.findByIdRaw(id);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error(`vault not minted for ${name}`);
    vaults[key] = vault;
  };
  await mkChar('charA', I.charA, 'Ada', {
    description: 'A lamplighter who keeps to the far side of the river.',
    personality: 'Formal to a fault; speaks in short, weighted sentences.',
  });
  await mkChar('charB', I.charB, 'Bram', { description: 'A cartographer.' });
  await mkChar('charC', I.charC, 'Cleo', { description: 'A quartermaster.' });
  await mkChar('charD', I.charD, 'Dora', { description: 'A signaller, archived.' });

  // 3. CHAR_A's Subprompts/ folder.
  const A = vaults.charA;
  await ensureFolderPath(A, SUBPROMPTS_FOLDER);
  const w = (rel: string, content: string) => writeDatabaseDocument(A, rel, content);
  await w(`${SUBPROMPTS_FOLDER}/terse.md`, composeSubpromptContent('Be terse', 'You answer in one line, never two.'));
  await w(`${SUBPROMPTS_FOLDER}/Verse.md`, composeSubpromptContent('Answer in verse', 'Every reply is a quatrain.\nRhyme where you can.'));
  await w(`${SUBPROMPTS_FOLDER}/no-title.md`, 'Just a body, dropped straight into the folder.');
  await w(`${SUBPROMPTS_FOLDER}/zulu.md`, composeSubpromptContent('Alpha-titled zulu', 'The file is zulu; the title sorts first.'));
  await w(`${SUBPROMPTS_FOLDER}/alpha.md`, composeSubpromptContent('Zulu-titled alpha', 'The file is alpha; the title sorts last.'));
  await w(`${SUBPROMPTS_FOLDER}/broken.md`, composeSubpromptContent('Broken', 'This document row is deleted below.'));
  await w(`${SUBPROMPTS_FOLDER}/drafts/x.md`, composeSubpromptContent('Nested draft', 'Must never list.'));
  await w(`${SUBPROMPTS_FOLDER}/notes.txt`, 'Not a subprompt.');
  // The unreadable-file arm: the link + file rows stay, the document goes.
  midb
    .prepare(
      'DELETE FROM doc_mount_documents WHERE fileId IN (SELECT fileId FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath = ?)',
    )
    .run(A, `${SUBPROMPTS_FOLDER}/broken.md`);

  // 4. CHAR_D: one subprompt, then archived.
  await writeDatabaseDocument(vaults.charD, `${SUBPROMPTS_FOLDER}/keep.md`, composeSubpromptContent('Keep', 'Still readable after the archive.'));
  await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', [TS, I.charD]);

  // 5. CHAR_C: the vault link is severed (the orphan store stays — v4's
  //    ensureCharacterVault adopts it on the first write).
  await rawQuery('UPDATE characters SET characterDocumentMountPointId = NULL WHERE id = ?', [I.charC]);

  // 6. The five chats.
  const seat = (id: string, characterId: string, extra: object = {}) => ({
    id,
    type: 'CHARACTER',
    characterId,
    controlledBy: 'llm',
    status: 'active',
    isActive: true,
    connectionProfileId: null,
    imageProfileId: null,
    selectedSystemPromptId: null,
    displayOrder: 0,
    hasHistoryAccess: false,
    createdAt: TS,
    updatedAt: TS,
    ...extra,
  });
  const mkChat = async (id: string, title: string, participants: unknown[]) => {
    await repos.chats.create(
      {
        userId: spec.userId,
        title,
        chatType: 'salon',
        contextSummary: null,
        participants,
        impersonatingParticipantIds: [],
        activeTypingParticipantId: null,
        tags: [],
      } as never,
      { id, createdAt: TS, updatedAt: TS } as never,
    );
  };
  await mkChat(I.chatLlm, 'LLM seat', [seat(I.pLlm, I.charA, { selectedSubpromptIds: ['terse', 'VERSE'] })]);
  await mkChat(I.chatUser, 'User seat', [
    seat(I.pUser, I.charA, { controlledBy: 'user', selectedSubpromptIds: ['terse'] }),
  ]);
  await mkChat(I.chatRemoved, 'Removed seat', [
    seat(I.pRemoved, I.charA, { status: 'removed', isActive: false, removedAt: TS, selectedSubpromptIds: ['terse'] }),
  ]);
  await mkChat(I.chatNoKey, 'Pre-feature seat', [seat(I.pNoKey, I.charA)]);
  await mkChat(I.chatTwoSeats, 'Two seats', [
    seat(I.pTwoA, I.charA, { selectedSubpromptIds: ['terse'] }),
    seat(I.pTwoB, I.charB, { displayOrder: 1, selectedSubpromptIds: ['terse'] }),
  ]);

  // Materialize the lazily-created tables both sides read, so the fresh
  // copies share one schema.
  await repos.backgroundJobs.findByUserId(spec.userId, 'PENDING');
  for (const chatId of [I.chatLlm, I.chatUser, I.chatRemoved, I.chatNoKey, I.chatTwoSeats]) {
    await repos.chats.getMessages(chatId);
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();
  global.Date = RealDate;

  writeFileSync(mainOut + '.meta.json', JSON.stringify({ vaults }));
  process.stderr.write(
    `built subprompts fixture: main=${mainOut} mount=${mountOut} vaults=${JSON.stringify(vaults)}\n`,
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`subprompts fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
