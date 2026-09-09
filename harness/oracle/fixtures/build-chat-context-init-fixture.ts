/**
 * Fixture builder for the buildChatContext read-differential (P4.4 unit 2,
 * sub-unit 2).
 *
 * Bakes three characters via v4's REAL repos.characters.create (each mints a
 * linked vault, pinned ids/ts) across main + mount-index:
 *   - Aria (llm): description/personality/firstMessage/systemPrompts/scenarios
 *     with {{user}}/{{char}} template vars, two system prompts.
 *   - Sam (user-controlled): aliases + pronouns + a description whose {{char}}
 *     refers to Sam (the minimal-context processTemplate path).
 *   - Bob (llm): defaultPartnerId = Sam (the default-partner resolution path).
 *
 * Both v4's real buildChatContext and the Rust port
 * (services::chat_initialize::build_chat_context) read the SAME baked fixture.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CCTX_MAIN=/tmp/qt-cctx-main.db QT_FIXTURE_CCTX_MOUNT=/tmp/qt-cctx-mount.db \
 *     $N/node --import tsx $V5/harness/oracle/fixtures/build-chat-context-init-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  ariaId: string;
  samId: string;
  bobId: string;
  aria: Record<string, unknown>;
  sam: Record<string, unknown>;
  bob: Record<string, unknown>;
}

const TS = '2026-02-01T00:00:00.000Z';

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'chat-context-init.json'), 'utf8')) as Spec;

  const mainOut = process.env.QT_FIXTURE_CCTX_MAIN;
  const mountOut = process.env.QT_FIXTURE_CCTX_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error('QT_FIXTURE_CCTX_MAIN and QT_FIXTURE_CCTX_MOUNT must both point at the .db files to write');
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cctx-fixture-build-'));
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
  await bake(spec.samId, spec.sam);
  await bake(spec.bobId, spec.bob);

  // [P4.D164 / v4 `2f4254b42`] Aria's `Subprompts/` for the greeting arms:
  // one file uses `{{scenario}}` so the greeting's RAW-parameter context (no
  // `firstActiveScenarioContent` fallback, unlike the identity stack's) can be
  // told apart from the stack's.
  {
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    const { ensureFolderPath } = await import('@/lib/mount-index/folder-paths');
    const { composeSubpromptContent, SUBPROMPTS_FOLDER } = await import('@/lib/subprompts/subprompts');
    const raw = await repos.characters.findByIdRaw(spec.ariaId);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error('Aria has no vault');
    await ensureFolderPath(vault, SUBPROMPTS_FOLDER);
    await writeDatabaseDocument(vault, `${SUBPROMPTS_FOLDER}/terse.md`, composeSubpromptContent('Be terse', '{{char}} answers {{user}} in one line.'));
    await writeDatabaseDocument(vault, `${SUBPROMPTS_FOLDER}/scene.md`, composeSubpromptContent('Mind the scene', 'The scene is: [{{scenario}}]. Persona: [{{persona}}].'));
  }

  // [P4.D168 / v4 `25f534c0b`] Sam's vault carries progressions. Sam, not Aria:
  // Sam is only ever the USER character in the pre-existing cases, never the
  // greeted one, so every one of those rows stays byte-identical and the new
  // `sam_*` cases are the only place the forced section appears.
  //
  // Instants are anchored on the case's frozen clock, 1718452800000 =
  // 2024-06-15T12:00:00Z.
  {
    const { writeDatabaseDocument } = await import('@/lib/mount-index/database-store');
    const raw = await repos.characters.findByIdRaw(spec.samId);
    const vault = raw?.characterDocumentMountPointId as string | null;
    if (!vault) throw new Error('Sam has no vault');
    await writeDatabaseDocument(
      vault,
      'metadata.json',
      JSON.stringify(
        {
          faction: 'The Harbour Watch',
          progressions: {
            // Active mid-span: the ordinary in-progress line.
            recharge: {
              name: 'Lantern recharge',
              startTime: '2024-06-15T11:58:00Z',
              endTime: '2024-06-15T12:08:00Z',
              timeIncrement: 'minute',
            },
            // Complete and `once`. It still appears in the greeting — MEASURED:
            // by rule 1 (no last turn ⇒ report everything), not by the `force`
            // flag, which is inert at this call site because the greeting passes
            // no event loader. What its presence proves is that the opener
            // reports unconditionally, silencing nothing.
            oath: {
              name: 'The oath',
              startTime: '2024-06-14T00:00:00Z',
              endTime: '2024-06-14T12:00:00Z',
              timeIncrement: 'day',
              onComplete: 'once',
            },
            // Refused by the schema: dropped, siblings survive.
            broken: {
              name: 'Broken',
              startTime: '2024-06-15T11:58:00Z',
              endTime: '2024-06-15T11:58:00Z',
              timeIncrement: 'minute',
            },
          },
        },
        null,
        2
      )
    );
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(`built chat-context-init fixtures: main=${mainOut} mount=${mountOut}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-context-init fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
