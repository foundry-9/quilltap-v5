/**
 * @jest-environment node
 *
 * Tier-2/3 ORACLE — P4.D292: v4's REAL index-store backfill pair
 * (`lib/memory/index-store/backfill.ts` — `generateMissingEmbeddings`,
 * `rebuildVectorIndex`, moved by `584d28164`, rows-as-index since `af48f6cc2`)
 * and `enqueueEmbeddingGenerateMany` (`queue-service.ts:1125-1182`,
 * `b045dd986`) at the `83334b9a5` pin.
 *
 * Only `generateEmbeddingForUser` is replaced — a recorder keyed on `text`
 * that answers a fixed unit vector, or THROWS for two pinned texts (one v4
 * `EmbeddingError`, one plain `Error` — `String(error)` differs). The memory
 * calendar is the instance zone (`Asia/Kolkata`).
 *
 * Phases on ONE database (state accumulates; the Rust side replays in order):
 *   1. `generateMissingEmbeddings` over a mixed set: two embedded rows (one
 *      cold), a NULL row, a zero-length-blob row, a cold NULL row, an anchored
 *      NULL row, and the two failing rows; another character's NULL row is
 *      untouched.
 *   2. `rebuildVectorIndex` — `indexed` = decodable rows, `failed` = `IS NULL`
 *      rows (the zero-length blob in neither), nothing written.
 *   3. `enqueueEmbeddingGenerateMany` — empty input; a batch with a PENDING-busy
 *      and a PROCESSING-busy entity, a duplicate, all four entity types; then
 *      the same batch again (every entity busy → the DEBUG, no row).
 * Recorded per op: result, service calls, captured lines; at the end the
 * `memories` vectors (hex) and the `background_jobs` rows.
 *
 * Run (Node 24, from the round's pin; the /tmp mirror dodges jest's
 * `/.claude/` testPathIgnorePatterns):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-mib-oracle /tmp/oracle-memory-index-backfill.ndjson
 *   mkdir -p /tmp/qt-mib-oracle/cases /tmp/qt-mib-oracle/db
 *   cp $V5W/harness/oracle/cases/memory-index-backfill.test.ts /tmp/qt-mib-oracle/cases/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_MIB_DIR=/tmp/qt-mib-oracle/db QT_ORACLE_OUT=/tmp/oracle-memory-index-backfill.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *     --roots "$PWD" --roots /tmp/qt-mib-oracle/cases -- memory-index-backfill
 */

import * as fs from 'fs';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

(globalThis as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'e2920000-0000-4000-8000-0000000000aa';
const CHAR = 'e2920000-0000-4000-8000-0000000000c1';
const OTHER = 'e2920000-0000-4000-8000-0000000000c2';
const PROFILE = 'e2920000-0000-4000-8000-0000000000e2';
const T0 = '2026-01-01T00:00:00.000Z';
const mid = (n: number) => `e2920000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const VECTOR = [0, 1, 0, 0];
const FAIL_EMBEDDING = 'fails with an EmbeddingError\n\nprovider refused';
const FAIL_PLAIN = 'fails with a plain Error\n\nsocket closed';

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_MIB_DIR;
  if (!dir) throw new Error('QT_FIXTURE_MIB_DIR must name the dir for base.db');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  mkdirSync(dir, { recursive: true });
  const work = join(dir, 'work.db');
  for (const p of [work, join(dir, 'base.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-mib-oracle-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.SQLITE_PATH = work;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const calls: Array<Record<string, unknown>> = [];
  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
  jest.doMock('@/lib/embedding/embedding-service', () => {
    const actual = jest.requireActual('@/lib/embedding/embedding-service');
    return {
      __esModule: true,
      ...actual,
      generateEmbeddingForUser: async (
        text: string,
        userId: string,
        profileId?: string,
        opts: { priority?: string; lexicalText?: string } = {},
      ) => {
        calls.push({ text, userId, profileId: profileId ?? null, priority: opts.priority ?? null, lexicalText: opts.lexicalText ?? null });
        if (text === FAIL_EMBEDDING) throw new actual.EmbeddingError('provider refused the batch', 'OPENAI');
        if (text === FAIL_PLAIN) throw new Error('socket closed');
        return { embedding: new Float32Array(VECTOR), model: 'canned', dimensions: VECTOR.length, provider: 'canned' };
      },
    };
  });

  const CAPTURED = (m: string) =>
    m.startsWith('[Memory] Failed to generate embedding for memory') ||
    m === '[Memory] Backfilled missing embeddings' ||
    m === '[Memory] Rebuilt vector index from memory rows' ||
    m === 'Embedding generate batch enqueued' ||
    m === 'Embedding batch: every entity already has a job';
  const captured: Array<Record<string, unknown>> = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!CAPTURED(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemorySchema, BackgroundJobSchema, ChatSettingsSchema } = await import('@/lib/schemas/types');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('memories', MemorySchema);
    await ensureCollection('background_jobs', BackgroundJobSchema);
    await ensureCollection('chat_settings', ChatSettingsSchema);
  };
  await ensure();
  {
    const { MemoriesRepository } = await import('@/lib/database/repositories/memories.repository');
    const repo = new MemoriesRepository();
    const make = async (n: number, characterId: string, summary: string, content: string, extra: Record<string, unknown> = {}) =>
      repo.create({ characterId, summary, content, ...extra } as never, { id: mid(n), createdAt: T0, updatedAt: T0 });
    await make(1, CHAR, 'embedded hot', 'already has a vector', { embedding: new Float32Array([1, 0, 0, 0]) });
    await make(2, CHAR, 'embedded cold', 'already has a vector', { embedding: new Float32Array([0, 0, 1, 0]), tier: 'cold' });
    await make(3, CHAR, 'a plain fact', 'tea over coffee');
    await make(4, CHAR, 'zero-length blob', 'an empty vector');
    await make(5, CHAR, 'a cold one', 'archived', { tier: 'cold' });
    await make(6, CHAR, 'an anchored one', 'the pier broke', { occurredAt: '2026-06-15T20:30:00.000Z', entities: ['Lighthouse Point'] });
    await make(7, CHAR, 'fails with an EmbeddingError', 'provider refused');
    await make(8, CHAR, 'fails with a plain Error', 'socket closed');
    await make(9, OTHER, 'another character', 'left alone');
  }
  await rawQuery(`UPDATE memories SET embedding = X'' WHERE id = '${mid(4)}'`);
  await rawQuery(
    `INSERT INTO chat_settings (id, userId, createdAt, updatedAt, timezone) VALUES ('${mid(50)}', '${USER}', '${T0}', '${T0}', 'Asia/Kolkata')`,
  );
  for (const [n, status, entityId] of [
    [70, 'PENDING', mid(3)],
    [71, 'PROCESSING', mid(5)],
  ] as Array<[number, string, string]>) {
    await rawQuery(
      `INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, maxAttempts, scheduledAt, createdAt, updatedAt) VALUES ('${mid(n)}', '${USER}', 'EMBEDDING_GENERATE', '${status}', '{"entityType":"MEMORY","entityId":"${entityId}","characterId":"${CHAR}","profileId":"${PROFILE}"}', 10, 0, 3, '${T0}', '${T0}', '${T0}')`,
    );
  }
  await closeDatabase();
  copyFileSync(work, join(dir, 'base.db'));
  await ensure();

  const { generateMissingEmbeddings, rebuildVectorIndex } = await import('@/lib/memory/index-store/backfill');
  const { enqueueEmbeddingGenerateMany } = await import('@/lib/background-jobs/queue-service');
  const out: string[] = [];
  const run = async (op: string, input: unknown, f: () => Promise<unknown>) => {
    calls.length = 0;
    captured.length = 0;
    const result = await f();
    out.push(JSON.stringify({ kind: 'op', op, input, result, calls: [...calls], lines: [...captured] }));
  };

  await run('generateMissingEmbeddings', { characterId: CHAR }, () =>
    generateMissingEmbeddings(CHAR, { userId: USER, embeddingProfileId: PROFILE }),
  );
  await run('rebuildVectorIndex', { characterId: CHAR }, () => rebuildVectorIndex(CHAR));
  await run('rebuildVectorIndex', { characterId: OTHER }, () => rebuildVectorIndex(OTHER));
  const many = [
    { entityType: 'MEMORY', entityId: mid(3), characterId: CHAR, profileId: PROFILE },
    { entityType: 'MEMORY', entityId: mid(5), characterId: CHAR, profileId: PROFILE },
    { entityType: 'MEMORY', entityId: mid(9), characterId: OTHER, profileId: PROFILE },
    { entityType: 'MEMORY', entityId: mid(9), characterId: OTHER, profileId: PROFILE },
    { entityType: 'CONVERSATION_CHUNK', entityId: mid(20), chatId: mid(21), profileId: PROFILE },
    { entityType: 'HELP_DOC', entityId: mid(22), profileId: PROFILE },
    { entityType: 'MOUNT_CHUNK', entityId: mid(23), profileId: PROFILE },
  ];
  await run('enqueueEmbeddingGenerateMany', [], () => enqueueEmbeddingGenerateMany(USER, [] as never));
  await run('enqueueEmbeddingGenerateMany', many, async () => (await enqueueEmbeddingGenerateMany(USER, many as never)).length);
  await run('enqueueEmbeddingGenerateMany', many, async () => (await enqueueEmbeddingGenerateMany(USER, many as never)).length);

  const memories = await rawQuery(`SELECT id, tier, hex(embedding) AS embedding FROM memories ORDER BY id`);
  const jobs = await rawQuery(
    `SELECT id, userId, type, status, payload, priority, attempts, maxAttempts, lastError, startedAt, completedAt FROM background_jobs ORDER BY createdAt, id`,
  );
  out.push(JSON.stringify({ kind: 'end', memories, jobs }));
  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  fs.writeFileSync(outPath, out.join('\n') + '\n');
}

test('memory index backfill oracle', async () => {
  await main();
});
