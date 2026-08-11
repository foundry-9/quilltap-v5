/**
 * @jest-environment node
 *
 * P4.9H2A EMBEDDING-REAPPLY ORACLE: drives v4's REAL `reapplyEmbeddingProfile`
 * over a FRESH copy of the committed `embedding-profiles-{main,mount}.db` fixture
 * for the Reapply Target profile (trunc=4, over 8-dim vectors), then dumps every
 * embedding-bearing table's `{id, embedding-hex}` so `services::
 * embedding_reapply_profile` can be diffed byte-for-byte. Both sides quantize
 * with the same codec, so the stored blobs must be identical. The result JSON is
 * normalized (durationMs, backup paths).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-ep-reapply-oracle ; rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp $V5W/harness/oracle/cases/embedding-reapply.test.ts "$TMPO/cases/"
 *   cd ~/source/quilltap-server   # or the pinned detached worktree on drift
 *   QT_EP_MGMT_MAIN=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-main.db \
 *   QT_EP_MGMT_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/embedding-profiles-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-ep-reapply.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- embedding-reapply
 */

import * as fs from 'fs';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PEPPER = 'ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk=';
const EP_REAPPLY = 'e0000000-0000-4000-8000-000000000005';

function applyMocks(): void {
  const cipherDriverPath = join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  for (const m of [
    '@/lib/database/manager',
    '@/lib/database/repositories',
    '@/lib/repositories/factory',
    '@/lib/database/backends/sqlite/client',
    '@/lib/database/backends/sqlite/mount-index-client',
    '@/lib/database/backends/sqlite/introspection',
    '@/lib/paths',
    '@/lib/embedding/reapply-profile',
    '@/lib/embedding/float32-conversion',
    '@/lib/embedding/vector-store',
    '@/lib/mount-index/mount-chunk-cache',
  ]) {
    jest.doMock(m, () => jest.requireActual(m));
  }
  jest.doMock('@/lib/startup/startup-state', () => {
    const a = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...a,
      startupState: {
        ...a.startupState,
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

const MAIN_TABLES = ['memories', 'vector_entries', 'conversation_chunks', 'help_docs'];

async function dumpTables(): Promise<unknown> {
  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const main = getRawDatabase() as unknown as {
    prepare: (s: string) => { all: () => Array<{ id: string; embedding: Buffer | null }> };
  };
  const mount = getRawMountIndexDatabase() as unknown as {
    prepare: (s: string) => { all: () => Array<{ id: string; embedding: Buffer | null }> };
  };
  const dumpOne = (
    db: { prepare: (s: string) => { all: () => Array<{ id: string; embedding: Buffer | null }> } },
    table: string,
  ) =>
    db
      .prepare(`SELECT id, embedding FROM "${table}" WHERE embedding IS NOT NULL ORDER BY id`)
      .all()
      .map((r) => ({ id: r.id, embedding: r.embedding ? Buffer.from(r.embedding).toString('hex') : null }));
  const out: Record<string, unknown> = {};
  for (const t of MAIN_TABLES) out[t] = dumpOne(main, t);
  out['doc_mount_chunks'] = dumpOne(mount, 'doc_mount_chunks');
  return out;
}

async function main(): Promise<void> {
  const fixtures = {
    main: process.env.QT_EP_MGMT_MAIN ?? '',
    mount: process.env.QT_EP_MGMT_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  jest.resetModules();
  applyMocks();

  const scratch = mkdtempSync(join(tmpdir(), 'qt-ep-reapply-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main.db');
  // ⚠ reapply-profile.ts opens the mount via `getMountIndexDatabasePath()` =
  // `<dataDir>/data/quilltap-mount-index.db` (it does NOT honour
  // SQLITE_MOUNT_INDEX_PATH like the rest of the app). In production the two
  // agree; here the mount copy must live at the derived path so v4's reapply
  // processes it (as it does in production, and as v5 always does).
  const mountWork = join(scratch, 'data', 'quilltap-mount-index.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { reapplyEmbeddingProfile } = await import('@/lib/embedding/reapply-profile');

  await initializeDatabase();
  const repos = getRepositories();
  const profile = await repos.embeddingProfiles.findById(EP_REAPPLY);
  if (!profile) throw new Error('EP_REAPPLY profile missing from fixture');

  const result = await reapplyEmbeddingProfile(profile);
  const tables = await dumpTables();

  await closeDatabase();

  // Normalize the non-deterministic fields (duration + backup paths).
  const normalized = {
    profileId: result.profileId,
    targetDimensions: result.targetDimensions,
    perTable: result.perTable,
    totalTruncated: result.totalTruncated,
    backupPath: result.backupPath ? '<backup>' : null,
    mountBackupPath: result.mountBackupPath ? '<mountBackup>' : null,
  };

  fs.writeFileSync(outPath, JSON.stringify({ result: normalized, tables }) + '\n');
  rmSync(scratch, { recursive: true, force: true });
}

it('embedding-reapply oracle', async () => {
  await main();
}, 120000);
