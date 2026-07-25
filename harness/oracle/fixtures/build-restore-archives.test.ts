/**
 * @jest-environment node
 *
 * P4.9G5 restore-fixture builder — the committed archive family the restore
 * differential feeds to BOTH sides.
 *
 * ── WHY A COMMITTED ARCHIVE ──────────────────────────────────────────────────
 * If each side restored an archive it had produced itself, the differential
 * would silently depend on v5's zip WRITER being right. Feeding one archive to
 * both makes the claim strictly stronger: given identical bytes on disk, the two
 * restore paths reach identical database state.
 *
 * The base archive is produced by v4's **REAL** `createBackup(userId)` over the
 * committed `system-data-*` fixture family (already test-pepper synthetic — the
 * sanitizer's job is done at fixture-build time, so nothing here carries real
 * data). The four variants are derived from it by unzipping, editing the tree,
 * and re-zipping.
 *
 *   restore-archive.zip                   the full archive (files/ seeded)
 *   restore-archive-legacy.zip            + data/outfit-presets.json, and one
 *                                         chat's equippedOutfit rewritten to the
 *                                         pre-rework single-UUID-or-null shape
 *   restore-archive-minimal.zip           every OPTIONAL data file removed
 *   restore-archive-missing-required.zip  data/tags.json removed (must throw)
 *   restore-archive-malformed.zip         neither a quilltap-backup-* root nor a
 *                                         manifest.json (hand-built)
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-restore-archives
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/fixtures/build-restore-archives.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ARCHIVE_OUT=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- build-restore-archives
 */

import * as fs from 'fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const IMAGE_STORAGE_KEY = (userId: string) => `${userId}/portrait.png`;
const IMAGE_BYTES = Buffer.from('quilltap-fixture-portrait-bytes\n', 'utf8');

/**
 * The OPTIONAL collections, per `parseBackupZip` (`archive.ts:190-265`) —
 * everything reached through `readJsonArrayFileOptional`. The `minimal` variant
 * deletes exactly these; whatever survives is the required set.
 */
const OPTIONAL_DATA_FILES = [
  'prompt-templates', 'roleplay-templates', 'provider-models', 'projects', 'groups',
  'llm-logs', 'plugin-configs', 'chat-settings', 'folders', 'wardrobe-items',
  'character-plugin-data', 'conversation-annotations', 'chat-documents',
  'instance-settings', 'embedding-status', 'conversation-chunks', 'tfidf-vocabularies',
  'vector-index-metas', 'vector-entries', 'doc-mount-points', 'doc-mount-folders',
  'doc-mount-files', 'doc-mount-file-links', 'doc-mount-chunks', 'doc-mount-documents',
  'doc-mount-blobs', 'project-doc-mount-links', 'group-doc-mount-links',
  'group-character-members', 'text-replacement-rules',
];

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/file-storage/manager', () =>
    jest.requireActual('@/lib/file-storage/manager'),
  );
  jest.doMock('@/lib/file-storage/user-uploads-bridge', () =>
    jest.requireActual('@/lib/file-storage/user-uploads-bridge'),
  );
  jest.doMock('@/lib/file-storage/project-store-bridge', () =>
    jest.requireActual('@/lib/file-storage/project-store-bridge'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: userId } }),
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
}

/** `zip -r <out> <rootFolder>` from `cwd` — the archive shape v4 itself writes. */
function zipTree(cwd: string, rootFolder: string, out: string): void {
  rmSync(out, { force: true });
  execFileSync('zip', ['-q', '-r', '-X', out, rootFolder], { cwd });
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'system-data.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_SD_MAIN ?? '',
    mount: process.env.QT_FIXTURE_SD_MOUNT ?? '',
    llm: process.env.QT_FIXTURE_SD_LLM ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outDir = process.env.QT_ARCHIVE_OUT;
  if (!outDir) throw new Error('QT_ARCHIVE_OUT must point at the fixture directory to write');
  mkdirSync(outDir, { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-restore-archives-'));
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratchRoot, 'bk-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  copyFileSync(fixtures.main, join(work, 'main.db'));
  copyFileSync(fixtures.mount, join(work, 'mount.db'));
  copyFileSync(fixtures.llm, join(work, 'llm.db'));
  process.env.SQLITE_PATH = join(work, 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = join(work, 'mount.db');
  process.env.SQLITE_LLM_LOGS_PATH = join(work, 'llm.db');
  process.env.QUILLTAP_DATA_DIR = work;

  const imgDest = join(work, 'files', ...IMAGE_STORAGE_KEY(spec.userId).split('/'));
  mkdirSync(dirname(imgDest), { recursive: true });
  writeFileSync(imgDest, IMAGE_BYTES);

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  try {
    const { createBackup } = await import('@/lib/backup/backup-service');
    const { zipPath } = await createBackup(spec.userId);
    copyFileSync(zipPath, join(outDir, 'restore-archive.zip'));

    // ── the variants, derived from the base tree ────────────────────────────
    const stage = mkdtempSync(join(scratchRoot, 'stage-'));
    execFileSync('unzip', ['-q', '-o', zipPath, '-d', stage], { maxBuffer: 10 * 1024 * 1024 });
    const root = fs
      .readdirSync(stage, { withFileTypes: true })
      .filter((e) => e.isDirectory() && e.name.startsWith('quilltap-backup-'))
      .map((e) => e.name)[0];
    if (!root) throw new Error('no quilltap-backup-* root in the base archive');

    const rebuild = (mutate: (rootPath: string) => void, name: string): void => {
      const dir = mkdtempSync(join(scratchRoot, 'v-'));
      execFileSync('cp', ['-R', join(stage, root), join(dir, root)]);
      mutate(join(dir, root));
      zipTree(dir, root, join(outDir, name));
      rmSync(dir, { recursive: true, force: true });
    };

    // LEGACY: a pre-rework archive. `data/outfit-presets.json` folds into
    // composite wardrobe items at parse time, and the chat's per-character
    // equippedOutfit map is rewritten to the single-UUID-or-null shape.
    rebuild((rootPath) => {
      const chatsPath = join(rootPath, 'data', 'chats.json');
      const chats = JSON.parse(fs.readFileSync(chatsPath, 'utf8')) as Record<string, unknown>[];
      if (chats.length === 0) throw new Error('the fixture must carry at least one chat');
      const participants = (chats[0]['participants'] ?? []) as Record<string, unknown>[];
      const charId =
        (participants.find((p) => typeof p['characterId'] === 'string')?.['characterId'] as
          | string
          | undefined) ?? 'a1000000-0000-4000-8000-000000000001';
      chats[0]['equippedOutfit'] = {
        [charId]: {
          top: '11111111-0000-4000-8000-000000000001',
          bottom: null,
          footwear: null,
          accessories: null,
        },
      };
      writeFileSync(chatsPath, JSON.stringify(chats, null, 2));

      writeFileSync(
        join(rootPath, 'data', 'outfit-presets.json'),
        JSON.stringify(
          [
            {
              id: '22222222-0000-4000-8000-000000000001',
              characterId: charId,
              name: 'Travelling Coat',
              description: 'For the long road north.',
              slots: {
                top: '11111111-0000-4000-8000-000000000001',
                bottom: '11111111-0000-4000-8000-000000000002',
                footwear: null,
                accessories: null,
              },
              createdAt: '2026-03-01T00:00:00.000Z',
              updatedAt: '2026-03-01T00:00:00.000Z',
            },
            {
              // Every slot null: `dedupeAndOrderSlotTypes` must fall back to
              // ['accessories'] (`legacy-migrations.ts:70`).
              id: '22222222-0000-4000-8000-000000000002',
              characterId: null,
              name: 'The Empty Ensemble',
              description: null,
              slots: { top: null, bottom: null, footwear: null, accessories: null },
              createdAt: '2026-03-01T00:00:00.000Z',
              updatedAt: '2026-03-01T00:00:00.000Z',
            },
          ],
          null,
          2,
        ),
      );
    }, 'restore-archive-legacy.zip');

    // MINIMAL: every optional collection absent — the `[]` fallbacks.
    rebuild((rootPath) => {
      for (const name of OPTIONAL_DATA_FILES) {
        rmSync(join(rootPath, 'data', `${name}.json`), { force: true });
      }
    }, 'restore-archive-minimal.zip');

    // MISSING REQUIRED: `data/tags.json` gone — `readJsonArrayFile` throws.
    rebuild((rootPath) => {
      rmSync(join(rootPath, 'data', 'tags.json'), { force: true });
    }, 'restore-archive-missing-required.zip');

    // MALFORMED: neither a quilltap-backup-* root nor a manifest.json.
    const bad = mkdtempSync(join(scratchRoot, 'bad-'));
    mkdirSync(join(bad, 'not-a-backup'), { recursive: true });
    writeFileSync(join(bad, 'not-a-backup', 'notes.txt'), 'this is not a Quilltap backup\n');
    zipTree(bad, 'not-a-backup', join(outDir, 'restore-archive-malformed.zip'));

    for (const f of fs.readdirSync(outDir)) {
      process.stderr.write(`  ${f}  ${fs.statSync(join(outDir, f)).size} bytes\n`);
    }
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(scratchRoot, { recursive: true, force: true });
  }
}

test('build restore archives', async () => {
  await main();
});
