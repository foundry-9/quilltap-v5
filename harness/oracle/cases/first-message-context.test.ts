/**
 * @jest-environment node
 *
 * Read-differential (mocked-embedding) ORACLE for the first-message-context builder
 * (v4 `loadParticipantMemories` / `loadProjectContext` / `buildFirstMessageContext`,
 * lib/chat/first-message-context.ts + the `searchMemoriesSemantic` it composes).
 *
 * Drives v4's REAL functions over the committed case corpus against a REAL two-DB
 * fixture (main + mount-index), with ONLY `generateEmbeddingForUser` stubbed to the
 * corpus's canned vectors (keyed by the exact semantic-search query text — the
 * participant `name` or `name - description`), the same vectors the Rust side
 * injects. Each case's result object is emitted as one NDJSON line; the Rust harness
 * compares byte-for-byte (read-only → zero normalization).
 *
 * Same real-DB-under-jest trick as memory-gate-tier3 (resetModules + doMock past
 * jest.setup's global mocks; cipher driver by absolute path). Needs
 * `--watchman=false` with the v5 --roots.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-fmc-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-fmc-mount.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-first-message-context-fixture.ts
 *   QT_FIXTURE_FMC_MAIN=/tmp/qt-fmc-main.db QT_FIXTURE_FMC_MOUNT=/tmp/qt-fmc-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-first-message-context.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- first-message-context
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
  nowMs: number;
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cases: any[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'first-message-context.json'), 'utf8')
  ) as Spec;

  const fixtureMain = process.env.QT_FIXTURE_FMC_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_FMC_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_FMC_MAIN and QT_FIXTURE_FMC_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-fmc-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const workMain = join(scratch, 'fmc-main-work.db');
  const workMount = join(scratch, 'fmc-mount-work.db');
  copyFileSync(fixtureMain, workMain);
  copyFileSync(fixtureMount, workMount);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = workMain;
  process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    const { EmbeddingError } = actual;
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (text: string) => {
        if (spec.cannedFailures.includes(text)) {
          throw new EmbeddingError(`canned failure for input (${text.length} chars)`);
        }
        const vec = spec.cannedEmbeddings[text];
        if (!vec) {
          throw new EmbeddingError(`no canned embedding registered for input: ${JSON.stringify(text)}`);
        }
        return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
      },
    };
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { loadParticipantMemories, loadProjectContext, buildFirstMessageContext } = await import(
    '@/lib/chat/first-message-context'
  );

  await initializeDatabase();
  const repos = getRepositories();

  const lines: string[] = [];
  for (const c of spec.cases) {
    let payload: unknown;
    if (c.kind === 'loadParticipantMemories') {
      const mems = await loadParticipantMemories(c.speakingCharacterId, c.otherParticipants, {
        userId: spec.userId,
        memoriesPerParticipant: 5,
      });
      payload = { participantMemories: mems };
    } else if (c.kind === 'loadProjectContext') {
      const ctx = await loadProjectContext(c.projectId, repos);
      payload = { projectContext: ctx };
    } else if (c.kind === 'buildFirstMessageContext') {
      const res = await buildFirstMessageContext(c.speakingCharacterId, c.participants, {
        userId: spec.userId,
        projectId: c.projectId,
      });
      payload = res;
    } else {
      throw new Error(`unknown case kind ${c.kind}`);
    }
    lines.push(JSON.stringify({ label: c.label, ...(payload as object) }));
  }

  // Let any fire-and-forget bumpAccessTimes promises settle before closing.
  await new Promise((resolve) => setTimeout(resolve, 100));
  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`first-message-context oracle wrote ${outPath}\n`);
}

test('first-message-context read-differential oracle', async () => {
  await main();
});
