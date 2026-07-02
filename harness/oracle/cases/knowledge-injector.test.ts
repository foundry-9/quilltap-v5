/**
 * @jest-environment node
 *
 * Read-differential (mocked-embedding) ORACLE for the per-turn knowledge injector
 * (v4 `retrieveKnowledgeForTurn`, lib/chat/context/knowledge-injector.ts).
 *
 * Drives v4's REAL `retrieveKnowledgeForTurn` over the committed query corpus
 * (knowledge-injector.json) against a REAL mount-index fixture, with ONLY
 * `generateEmbeddingForUser` stubbed to the corpus's canned vectors (keyed by the
 * exact query text) — the same vectors the Rust `CannedEmbeddingProvider` injects.
 * Everything else (the chunk search, cosine, literal boost, budget pack, rendering)
 * is genuine. Each query's `{ content, tokenCount, debug }` result is emitted as one
 * NDJSON line; the Rust harness compares byte-for-byte (read-only → zero
 * normalization).
 *
 * Runs under v4's JEST (not tsx): the same real-DB-under-jest trick as
 * memory-gate-tier3 (resetModules + doMock past jest.setup's global mocks; the
 * cipher driver mapped to better-sqlite3-multiple-ciphers by absolute path). Needs
 * `--watchman=false` with the v5 --roots.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-knowledge-injector-fixture.db \
 *     $N/npx tsx $V5/harness/oracle/fixtures/build-knowledge-injector-fixture.ts
 *   QT_FIXTURE_KNOWLEDGE=/tmp/qt-knowledge-injector-fixture.db \
 *   QT_ORACLE_OUT=/tmp/oracle-knowledge-injector.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- knowledge-injector
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Query {
  label: string;
  query: string;
  characterMountPointId: string | null;
  groupMountPointIds: string[];
  projectMountPointIds: string[];
  globalMountPointId: string | null;
  budgetTokens: number;
  provider: string;
  charsPerToken: number;
  minScore?: number;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  cannedEmbeddings: Record<string, number[]>;
  cannedFailures: string[];
  queries: Query[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'knowledge-injector.json'), 'utf8')
  ) as Spec;

  const fixture = process.env.QT_FIXTURE_KNOWLEDGE;
  if (!fixture || !existsSync(fixture)) {
    throw new Error('QT_FIXTURE_KNOWLEDGE must point at the fixture from build-knowledge-injector-fixture.ts');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-knowledge-injector-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'knowledge-injector-mount-index-work.db');
  copyFileSync(fixture, work);

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db');
  process.env.SQLITE_MOUNT_INDEX_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  // Restore the real DB stack (past jest.setup's global mocks); only the embedding
  // call is pinned. Same mechanics as memory-gate-tier3.
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers'
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
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
          throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
        }
        return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
      },
    };
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { retrieveKnowledgeForTurn } = await import('@/lib/chat/context/knowledge-injector');

  await initializeDatabase();

  const lines: string[] = [];
  for (const q of spec.queries) {
    const result = await retrieveKnowledgeForTurn({
      characterId: 'char-ada',
      userId: spec.userId,
      query: q.query,
      characterMountPointId: q.characterMountPointId,
      groupMountPointIds: q.groupMountPointIds,
      projectMountPointIds: q.projectMountPointIds,
      globalMountPointId: q.globalMountPointId,
      budgetTokens: q.budgetTokens,
      provider: q.provider as never,
      ...(q.minScore !== undefined ? { minScore: q.minScore } : {}),
    });
    lines.push(
      JSON.stringify({
        label: q.label,
        content: result.content,
        tokenCount: result.tokenCount,
        debug: result.debug,
      })
    );
  }

  await closeDatabase();
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`knowledge-injector oracle wrote ${outPath}\n`);
}

test('knowledge-injector read-differential oracle', async () => {
  await main();
});
