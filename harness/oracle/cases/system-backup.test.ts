/**
 * @jest-environment node
 *
 * P4.9G5 backup ORACLE: drives v4's REAL `createBackup(userId)`
 * (`lib/backup/backup-service.ts:548`) over a FRESH copy of the committed
 * `system-data-*` fixture family, unzips the produced archive, and emits the
 * **extracted archive tree** — the manifest plus every staged file's relative
 * path and content.
 *
 * ── WHY THE TREE, NEVER THE ZIP BYTES ────────────────────────────────────────
 * v4 shells out to `zip -r`; v5 uses a Rust zip crate. The framing (extra
 * fields, ordering, compression levels) differs by design — the DB→disk
 * projection is the thing under test, so the differential diffs the extracted
 * tree: relative paths first, then file contents byte-for-byte for the JSON
 * data files (v4's `writeJsonArrayFile` layout is part of the contract) and
 * sha256+size for binary payloads.
 *
 * ── WHAT IS NORMALIZED ───────────────────────────────────────────────────────
 *   - the staging root folder name  (`quilltap-backup-<ISO>` → `ROOT`)
 *   - `manifest.createdAt`          (wall clock)
 *   - `manifest.appVersion`         (v4 reads `process.env.npm_package_version`
 *                                    and falls back to '2.0.0'; v5 carries its
 *                                    own crate version — the Rust side passes
 *                                    the same pinned string)
 * Nothing else: entity counts, file order, key order and JSON formatting are
 * all diffed exactly.
 *
 * ── CASES ────────────────────────────────────────────────────────────────────
 *   `backup_full`         — the user file's bytes are seeded on disk, so the
 *                           `files/<storageKey>` staging leg runs.
 *   `backup_missing_file` — nothing seeded: v4 warns and continues, so no
 *                           `files/` subtree exists at all (the warn-and-
 *                           continue arm, `backup-service.ts:647`).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-sysbackup-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/system-backup.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/system-data.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SD_MAIN=$V5W/crates/quilltap-web/tests/fixtures/system-data-main.db \
 *   QT_FIXTURE_SD_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/system-data-mount.db \
 *   QT_FIXTURE_SD_LLM=$V5W/crates/quilltap-web/tests/fixtures/system-data-llmlogs.db \
 *   QT_ORACLE_OUT=/tmp/oracle-system-backup.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=300000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- system-backup
 */

import * as fs from 'fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join, relative, sep } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

/** The IMAGE file seeded by the fixture builder (`FILE_IMG`). */
const IMAGE_STORAGE_KEY = (userId: string) => `${userId}/portrait.png`;
/** Deterministic bytes so both sides stage the identical payload. */
const IMAGE_BYTES = Buffer.from('quilltap-fixture-portrait-bytes\n', 'utf8');

function applyMocks(userId: string): void {
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  // jest.setup.ts stubs the file-storage manager (`downloadFile` → the literal
  // 'mock file content'). The staging leg is exactly what this oracle measures,
  // so restore the REAL manager + its bridges (the `jest-real-db-oracle` recipe).
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

/** Recursively list every regular file under `root`, relative + `/`-joined. */
function walk(root: string): string[] {
  const out: string[] = [];
  const rec = (dir: string): void => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) rec(full);
      else if (entry.isFile()) out.push(relative(root, full).split(sep).join('/'));
    }
  };
  rec(root);
  return out.sort();
}

interface TreeEntry {
  path: string;
  /** Present for text/JSON payloads — the exact bytes as UTF-8. */
  text?: string;
  /** Present for binary payloads. */
  sha256?: string;
  size: number;
}

function describeTree(rootPath: string): TreeEntry[] {
  return walk(rootPath).map((p) => {
    const buf = fs.readFileSync(join(rootPath, p));
    const isText = p === 'manifest.json' || p.startsWith('data/');
    return isText
      ? { path: p, text: buf.toString('utf8'), size: buf.length }
      : { path: p, sha256: createHash('sha256').update(buf).digest('hex'), size: buf.length };
  });
}

interface CaseSpec {
  name: string;
  /** Seed the user file's bytes into the data dir before running. */
  seedFileBytes: boolean;
  /** [P4.D46] v4 `createBackup(userId, {compact: true})` (`7189a968`). */
  compact?: boolean;
}

const CASES: CaseSpec[] = [
  { name: 'backup_full', seedFileBytes: true },
  { name: 'backup_missing_file', seedFileBytes: false },
  // The compact arm: memory embeddings nulled, the six derived embedding
  // data files ABSENT from the tree (not empty), manifest.compact stamped.
  { name: 'backup_compact', seedFileBytes: true, compact: true },
];

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratchRoot: string,
  fixtures: { main: string; mount: string; llm: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();
  applyMocks(spec.userId);

  const work = mkdtempSync(join(scratchRoot, 'bk-'));
  mkdirSync(join(work, 'data'), { recursive: true });
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  const llmWork = join(work, 'llm.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  copyFileSync(fixtures.llm, llmWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.SQLITE_LLM_LOGS_PATH = llmWork;
  process.env.QUILLTAP_DATA_DIR = work;

  if (c.seedFileBytes) {
    const dest = join(work, 'files', ...IMAGE_STORAGE_KEY(spec.userId).split('/'));
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, IMAGE_BYTES);
  }

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { closeLLMLogsSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  await initializeDatabase();

  const extractDir = mkdtempSync(join(scratchRoot, 'ex-'));
  try {
    const { createBackup } = await import('@/lib/backup/backup-service');
    const { zipPath, manifest } = await createBackup(
      spec.userId,
      c.compact ? { compact: true } : undefined,
    );

    execFileSync('unzip', ['-q', '-o', zipPath, '-d', extractDir], { maxBuffer: 10 * 1024 * 1024 });
    const roots = fs
      .readdirSync(extractDir, { withFileTypes: true })
      .filter((e) => e.isDirectory() && e.name.startsWith('quilltap-backup-'));
    if (roots.length !== 1) throw new Error(`expected one staging root, saw ${roots.length}`);
    const rootPath = join(extractDir, roots[0].name);

    const m = manifest as unknown as Record<string, unknown>;
    return {
      name: c.name,
      rootFolderPrefix: 'quilltap-backup-',
      manifest: { ...m, createdAt: '<normalized>', appVersion: '<normalized>' },
      tree: describeTree(rootPath),
    };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    closeLLMLogsSQLiteClient();
    rmSync(extractDir, { recursive: true, force: true });
    rmSync(work, { recursive: true, force: true });
  }
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
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratchRoot = mkdtempSync(join(tmpdir(), 'qt-sysbackup-oracle-'));
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const outLines: string[] = [];
  for (const c of CASES) {
    outLines.push(JSON.stringify(await runCase(spec, c, scratchRoot, fixtures)));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  rmSync(scratchRoot, { recursive: true, force: true });
  process.stderr.write(`system-backup oracle wrote ${outPath} (${outLines.length} cases)\n`);
}

test('system-backup oracle', async () => {
  await main();
});
