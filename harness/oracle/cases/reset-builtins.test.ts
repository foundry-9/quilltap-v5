/**
 * @jest-environment node
 *
 * P4.4u4 unit 2 (tier 2) ORACLE: drives v4's REAL `handleResetBuiltins` (via the
 * collection route `POST /api/v1/characters?action=reset-builtins`) over a
 * PRE-SEEDED instance (Lorian + Riya + both avatars), then emits the response
 * shape + the normalized post-reset `characters` / `memories` dumps so the Rust
 * service (`quilltap_core::services::quilltap_import::reset::reset_builtins`) can
 * be diffed.
 *
 * Pre-seed uses v4's REAL `executeImport` + `reseedAvatarsForCharacters`; the
 * reset then cascade-deletes the built-ins, re-imports with the seed-id →
 * preserved-id remap, and reseeds the avatars. Every id is minted on both sides,
 * so the character / memory dumps are remapped to first-seen tokens by the Rust
 * harness (FKs verify by relationship); timestamps are placeholdered.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/;
 * cwd must be the v4 root so first-startup/{imports,avatars} resolve):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-reset-builtins-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
 *   cp "$V5W/harness/oracle/cases/reset-builtins.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/qtap-import-tier2.json" "$TMPO/fixtures/"
 *   cp "$V5W/harness/oracle/lib/tier2.ts" "$TMPO/lib/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_QTAPIMPORT_MAIN=/tmp/qt-qtapimport-main.db \
 *   QT_FIXTURE_QTAPIMPORT_MOUNT=/tmp/qt-qtapimport-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-reset-builtins.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- reset-builtins
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2';

interface Spec {
  testPepperBase64: string;
}

const SINGLE_USER_ID = 'ffffffff-ffff-ffff-ffff-ffffffffffff';

function mockRequest(url: string): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue({}),
  };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'qtap-import-tier2.json'), 'utf8'),
  ) as Spec;
  // Read the seed from the v4 checkout's own first-startup (cwd = the v4 root;
  // byte-identical to the committed v5 asset). `import.meta.url`/`here` points at
  // the /tmp jest mirror, so a worktree-relative path won't resolve.
  const qtapPath = join(process.cwd(), 'first-startup', 'imports', 'lorian-and-riya.qtap');
  const exportData = JSON.parse(readFileSync(qtapPath, 'utf8'));

  const mainFixture = process.env.QT_FIXTURE_QTAPIMPORT_MAIN;
  const mountFixture = process.env.QT_FIXTURE_QTAPIMPORT_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_QTAPIMPORT_MAIN / MOUNT must point at the qtap-import fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-reset-builtins-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: SINGLE_USER_ID } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });

  const work = mkdtempSync(join(scratch, 'rb-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient, getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { executeImport } = await import('@/lib/import/quilltap-import-service');
  const { reseedAvatarsForCharacters } = await import('@/lib/startup/seed-initial-data');
  const { getRepositories } = await import('@/lib/database/repositories');

  await initializeDatabase();

  try {
    // The auth middleware (`repos.users.findById(SINGLE_USER_ID)`) needs the
    // single-user row; the empty fixture has none. Seeding it is a v4-middleware
    // artifact only — the Rust reset reads characters by userId directly and needs
    // no users row, and this row is not among the diffed tables.
    await getRepositories().users.create(
      { username: 'localUser', email: 'user@localhost.localdomain', name: 'Local User' } as never,
      { id: SINGLE_USER_ID } as never,
    );

    // PRE-SEED: create Lorian + Riya + both avatars (the reset's precondition).
    await executeImport(SINGLE_USER_ID, exportData, {
      conflictStrategy: 'skip',
      includeMemories: true,
      includeRelatedEntities: false,
    });
    await reseedAvatarsForCharacters(['Lorian', 'Riya']);

    // Drive the RESET via the collection route.
    const { POST } = (await import('@/app/api/v1/characters/route')) as {
      POST: (...a: unknown[]) => Promise<{ status: number; json: () => Promise<unknown> }>;
    };
    const url = 'http://localhost/api/v1/characters?action=reset-builtins';
    const response = await POST(mockRequest(url));
    const status = response.status;
    const body = (await response.json()) as {
      deletedCharacterIds: string[];
      preservedIds: Record<string, string | null>;
      postResetIds: Record<string, string | null>;
      remappedIdCount: number;
      importResult: { imported: { characters: number; memories: number } };
    };

    // Post-reset dumps: characters + memories (main).
    const dumpMain = async (table: string, orderBy: string) => {
      const columns = (
        (await rawQuery(`PRAGMA table_info(${table})`)) as Array<{ name: string }>
      ).map((c) => c.name);
      const rawRows = (await rawQuery(`SELECT * FROM ${table}`)) as Array<
        Record<string, unknown>
      >;
      return canonicalizeRows({ table, columns, rawRows, orderBy });
    };
    const characters = await dumpMain('characters', 'name');
    const memories = await dumpMain('memories', 'content');

    // Avatar links across all vaults.
    const midb = getRawMountIndexDatabase();
    if (!midb) throw new Error('mount-index handle unavailable');
    const avatarLinkCount = (
      midb
        .prepare(
          "SELECT count(*) AS n FROM doc_mount_file_links WHERE relativePath = 'images/avatar.webp'",
        )
        .get() as { n: number }
    ).n;
    const charsWithDefaultImage = (
      (await rawQuery(
        'SELECT count(*) AS n FROM characters WHERE defaultImageId IS NOT NULL',
      )) as Array<{ n: number }>
    )[0].n;

    const pn = (o: Record<string, string | null>, k: string) => o[k] !== null && o[k] !== undefined;
    const payload = {
      case: 'reset-builtins-tier2',
      status,
      result: {
        deletedCharacterCount: body.deletedCharacterIds.length,
        remappedIdCount: body.remappedIdCount,
        preservedLorianPresent: pn(body.preservedIds, 'Lorian'),
        preservedRiyaPresent: pn(body.preservedIds, 'Riya'),
        postResetLorianPresent: pn(body.postResetIds, 'Lorian'),
        postResetRiyaPresent: pn(body.postResetIds, 'Riya'),
        // The re-imported characters get FRESH minted ids (create strips the id),
        // so the post-reset ids differ from the preserved (pre-reset) ids.
        postResetDiffersFromPreserved:
          body.postResetIds.Lorian !== body.preservedIds.Lorian &&
          body.postResetIds.Riya !== body.preservedIds.Riya,
        importedCharacters: body.importResult.imported.characters,
        importedMemories: body.importResult.imported.memories,
      },
      postState: {
        characterCount: (characters as { rows: unknown[] }).rows.length,
        memoryCount: (memories as { rows: unknown[] }).rows.length,
        avatarLinkCount,
        charsWithDefaultImage,
      },
      characters,
      memories,
    };
    fs.writeFileSync(outPath, JSON.stringify(payload) + '\n');
    process.stderr.write(`reset-builtins oracle wrote ${outPath}\n`);
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

test('reset-builtins-tier2 oracle', async () => {
  await main();
});
