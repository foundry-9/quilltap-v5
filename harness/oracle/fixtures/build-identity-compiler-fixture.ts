/**
 * Fixture builder for the identity-stack-compiler tier-2 differential (P4.4
 * unit 2, sub-unit 3).
 *
 * Bakes four characters (Aria/llm rich, Bob/llm simple, Sam/user, Ghost/llm) via
 * v4's REAL repos.characters.create + one chat via repos.chats.create whose
 * participants are Aria(llm active), Bob(llm active), Sam(user active), and
 * Ghost(llm REMOVED), with a scenarioText carrying a {{char}} template. All ids
 * pinned; compiledIdentityStacks is left null (both sides compile then diff it).
 *
 * P4.D103 (v4 `a6870c5a`) adds EIGHT more chats with the same participant set,
 * each pre-seeded with a different `compiledIdentityStacks` VALUE, so the
 * version-stamped envelope's read/merge/drop arms are measurable:
 *   mergeIntoCurrent   — `{version: CURRENT, stacks:{bob: sentinel}}` (merge keeps the sibling)
 *   mergeIntoLegacy    — a bare `{participantId: stack}` map, no version key
 *   mergeIntoOlder     — `{version: 1, ...}`   (older stamp)
 *   mergeIntoNewer     — `{version: 999, ...}` (newer stamp — the downgrade guard)
 *   dropFromCurrent    — current envelope CONTAINING the ineligible participant's key
 *   dropKeyAbsent      — current envelope NOT containing it (v4 writes nothing)
 *   dropFromLegacy     — a legacy map present on the drop path (v4 CLEARS to null)
 *   dropFromNullColumn — the column already null (v4 writes nothing)
 * The seeded stacks are SENTINEL strings no builder would ever emit, so a port
 * that rewrites a stale map back is visible in the output rather than inferred.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_IDC_MAIN=/tmp/qt-idc-main.db QT_FIXTURE_IDC_MOUNT=/tmp/qt-idc-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-identity-compiler-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  chatId: string;
  ariaP: string;
  bobP: string;
  samP: string;
  ghostP: string;
  ariaId: string;
  bobId: string;
  samId: string;
  ghostId: string;
  scenarioText: string;
  envelopeChats: Record<string, string>;
  aria: Record<string, unknown>;
  bob: Record<string, unknown>;
  sam: Record<string, unknown>;
  ghost: Record<string, unknown>;
}

const TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'identity-compiler.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_IDC_MAIN;
  const mountOut = process.env.QT_FIXTURE_IDC_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_IDC_MAIN and QT_FIXTURE_IDC_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-idc-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { CharacterSchema } = await import('@/lib/schemas/types');
  const { generateDDL } = await import('@/lib/database/schema-translator');
  const {
    DocMountPointSchema,
    DocMountFileSchema,
    DocMountDocumentSchema,
    DocMountFolderSchema,
    DocMountFileLinkSchema,
    DocMountChunkSchema,
  } = await import('@/lib/schemas/mount-index.types');

  await initializeDatabase();
  await ensureCollection('characters', CharacterSchema);

  const midb = getRawMountIndexDatabase();
  if (!midb) throw new Error('mount-index DB handle unavailable');
  const ddl: Array<[string, unknown]> = [
    ['doc_mount_points', DocMountPointSchema],
    ['doc_mount_files', DocMountFileSchema],
    ['doc_mount_documents', DocMountDocumentSchema],
    ['doc_mount_folders', DocMountFolderSchema],
    ['doc_mount_file_links', DocMountFileLinkSchema],
    ['doc_mount_chunks', DocMountChunkSchema],
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
  const bake = async (id: string, char: Record<string, unknown>): Promise<void> => {
    await repos.characters.create(char as never, { id, createdAt: TS, updatedAt: TS } as never);
  };
  await bake(spec.ariaId, spec.aria);
  await bake(spec.bobId, spec.bob);
  await bake(spec.samId, spec.sam);
  await bake(spec.ghostId, spec.ghost);

  const participants = [
    { id: spec.ariaP, type: 'CHARACTER', characterId: spec.ariaId, controlledBy: 'llm', status: 'active', displayOrder: 0, isActive: true, createdAt: TS, updatedAt: TS },
    { id: spec.bobP, type: 'CHARACTER', characterId: spec.bobId, controlledBy: 'llm', status: 'active', displayOrder: 1, isActive: true, createdAt: TS, updatedAt: TS },
    { id: spec.samP, type: 'CHARACTER', characterId: spec.samId, controlledBy: 'user', status: 'active', displayOrder: 2, isActive: true, createdAt: TS, updatedAt: TS },
    { id: spec.ghostP, type: 'CHARACTER', characterId: spec.ghostId, controlledBy: 'llm', status: 'removed', displayOrder: 3, isActive: false, createdAt: TS, updatedAt: TS },
  ];
  await repos.chats.create(
    {
      userId: spec.userId,
      title: 'Compiler fixture',
      participants,
      chatType: 'salon',
      scenarioText: spec.scenarioText,
    } as never,
    { id: spec.chatId, createdAt: TS, updatedAt: TS },
  );

  // --- P4.D103: the envelope-arm chats -------------------------------------
  // Same participant set; only the pre-seeded `compiledIdentityStacks` differs.
  const { IDENTITY_STACK_BUILDER_VERSION } = await import(
    '@/lib/chat/context/system-prompt-builder'
  );
  const SENTINEL_ARIA = '<<seeded-stale-stack-for-aria>>';
  const SENTINEL_BOB = '<<seeded-stale-stack-for-bob>>';
  const SENTINEL_SAM = '<<seeded-stale-stack-for-sam>>';
  const seeds: Array<[string, unknown]> = [
    [
      spec.envelopeChats.mergeIntoCurrent,
      { version: IDENTITY_STACK_BUILDER_VERSION, stacks: { [spec.bobP]: SENTINEL_BOB } },
    ],
    [spec.envelopeChats.mergeIntoLegacy, { [spec.ariaP]: SENTINEL_ARIA, [spec.bobP]: SENTINEL_BOB }],
    [spec.envelopeChats.mergeIntoOlder, { version: 1, stacks: { [spec.bobP]: SENTINEL_BOB } }],
    [spec.envelopeChats.mergeIntoNewer, { version: 999, stacks: { [spec.bobP]: SENTINEL_BOB } }],
    [
      spec.envelopeChats.dropFromCurrent,
      {
        version: IDENTITY_STACK_BUILDER_VERSION,
        stacks: { [spec.samP]: SENTINEL_SAM, [spec.bobP]: SENTINEL_BOB },
      },
    ],
    [
      spec.envelopeChats.dropKeyAbsent,
      { version: IDENTITY_STACK_BUILDER_VERSION, stacks: { [spec.bobP]: SENTINEL_BOB } },
    ],
    [spec.envelopeChats.dropFromLegacy, { [spec.samP]: SENTINEL_SAM, [spec.bobP]: SENTINEL_BOB }],
    [spec.envelopeChats.dropFromNullColumn, null],
  ];
  for (const [chatId, compiled] of seeds) {
    await repos.chats.create(
      {
        userId: spec.userId,
        title: `Envelope fixture ${chatId.slice(-2)}`,
        participants,
        chatType: 'salon',
        scenarioText: spec.scenarioText,
        compiledIdentityStacks: compiled,
      } as never,
      { id: chatId, createdAt: TS, updatedAt: TS },
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built identity-compiler fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`identity-compiler fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
