/**
 * @jest-environment node
 *
 * Tier-3 ORACLE — P4.D292: WHICH TEXT the EMBEDDING_GENERATE memory branch
 * hands the embedding service, at the `83334b9a5` pin. v4's REAL
 * `handleEmbeddingGenerate` runs over planted memories; ONLY the embedding
 * service's `generateEmbeddingForUser` is replaced, by a recorder that keeps
 * `(text, userId, profileId, opts.priority, opts.lexicalText)` and answers a
 * fixed unit vector (the `embedding-generate-jobs` precedent — the writer's
 * hand-off is the unit under test; the BUILTIN service's use of `lexicalText`
 * is `embedding_provider_tier3`'s rows).
 *
 * The memory calendar is the INSTANCE zone (`chat_settings.timezone` =
 * `Pacific/Auckland`), read through v4's real `refreshMemoryTimeZone`, so the
 * anchored date crosses the UTC day boundary (`2026-06-15T15:30Z` → `when:
 * 2026-06-16`) independent of jest's forced `TZ=UTC`.
 *
 * Rows: an anchored memory (date + story time + places + a non-BMP summary),
 * an anchor-free one, a COLD anchored one (the upsert writes `embedding` only),
 * an oversize one whose LEXICAL text is exactly at the 131,072 cap and whose
 * ANCHORED text is over it (the guard reads the anchored text — skipped, no
 * call), a whitespace memory WITH an anchor (the anchored text is not blank —
 * embedded), and a missing memory. Recorded: every service call, the lines,
 * the `memories` vectors and the `embedding_status` rows.
 *
 * Run (Node 24, from the round's pin; the /tmp mirror dodges jest's
 * `/.claude/` testPathIgnorePatterns):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-ets-oracle /tmp/oracle-embedding-text-shape.ndjson
 *   mkdir -p /tmp/qt-ets-oracle/cases /tmp/qt-ets-oracle/db
 *   cp $V5W/harness/oracle/cases/embedding-text-shape.test.ts /tmp/qt-ets-oracle/cases/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_ETS_DIR=/tmp/qt-ets-oracle/db QT_ORACLE_OUT=/tmp/oracle-embedding-text-shape.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *     --roots "$PWD" --roots /tmp/qt-ets-oracle/cases -- embedding-text-shape
 */

import * as fs from 'fs';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const USER = 'e2920000-0000-4000-8000-0000000000aa';
const CHAR = 'e2920000-0000-4000-8000-0000000000c1';
const PROFILE = 'e2920000-0000-4000-8000-0000000000e2';
const T0 = '2026-01-01T00:00:00.000Z';
const mid = (n: number) => `e2920000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const VECTOR = [1, 0, 0, 0];
const CAP = 128 * 1024;

interface Plant {
  id: string;
  summary: string;
  content: string;
  occurredAt: string | null;
  narrativeTime: string | null;
  entities: string[] | null;
  tier: 'hot' | 'cold';
}

const PLANTS: Plant[] = [
  {
    id: mid(1),
    summary: 'Ada watches the 🌊 rise',
    content: 'The storm broke the pier at Lighthouse Point.',
    occurredAt: '2026-06-15T15:30:00.000Z',
    narrativeTime: 'the third night of the storm',
    entities: ['Lighthouse Point', ' Ada ', ''],
    tier: 'hot',
  },
  { id: mid(2), summary: 'A plain fact', content: 'Ada prefers tea to coffee.', occurredAt: null, narrativeTime: null, entities: null, tier: 'hot' },
  { id: mid(3), summary: 'An archived scene', content: 'The lantern was relit.', occurredAt: null, narrativeTime: null, entities: ['Harbour'], tier: 'cold' },
  // Lexical text exactly at the cap ('big' + '\n\n' + content); the anchor line pushes it over.
  { id: mid(4), summary: 'big', content: 'x'.repeat(CAP - 5), occurredAt: '2026-06-15T15:30:00.000Z', narrativeTime: null, entities: null, tier: 'hot' },
  // Blank fields, but the anchor line makes the embedded text non-blank.
  { id: mid(5), summary: ' ', content: ' ', occurredAt: null, narrativeTime: 'dawn', entities: null, tier: 'hot' },
];
const MISSING = mid(99);

function sqlStr(v: string | null): string {
  return v === null ? 'NULL' : `'${v.replace(/'/g, "''")}'`;
}

async function main(): Promise<void> {
  const dir = process.env.QT_FIXTURE_ETS_DIR;
  if (!dir) throw new Error('QT_FIXTURE_ETS_DIR must name the dir for base.db');
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  mkdirSync(dir, { recursive: true });
  const work = join(dir, 'work.db');
  for (const p of [work, join(dir, 'base.db')]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-ets-oracle-'));
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
        calls.push({
          text,
          userId,
          profileId: profileId ?? null,
          priority: opts.priority ?? null,
          lexicalText: opts.lexicalText ?? null,
        });
        return { embedding: new Float32Array(VECTOR), model: 'canned', dimensions: VECTOR.length, provider: 'canned' };
      },
    };
  });

  const CAPTURED = new Set([
    '[EmbeddingGenerate] Built memory embedding text',
    '[EmbeddingGenerate] Embedding generated successfully',
    '[EmbeddingGenerate] Memory not found',
  ]);
  const captured: Array<Record<string, unknown>> = [];
  const tagged: string[] = [];
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      // Sibling lines (the index store's, the memory calendar's, the guard's)
      // are asserted by TAG / message only (§R.5).
      if (!CAPTURED.has(data.message)) {
        if (/^\[(MemoryCalendar|EmbeddingGenerate)\]|^Upserted memory vectors/.test(data.message)) tagged.push(data.message);
        return;
      }
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const { initializeDatabase, ensureCollection, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  const { MemorySchema, EmbeddingStatusSchema, ChatSettingsSchema } = await import('@/lib/schemas/types');
  const ensure = async () => {
    await initializeDatabase();
    await ensureCollection('memories', MemorySchema);
    await ensureCollection('embedding_status', EmbeddingStatusSchema);
    await ensureCollection('chat_settings', ChatSettingsSchema);
  };
  await ensure();
  for (const m of PLANTS) {
    await rawQuery(
      `INSERT INTO memories (id, characterId, content, summary, occurredAt, narrativeTime, entities, tier, createdAt, updatedAt) VALUES (` +
        [m.id, CHAR, m.content, m.summary].map(sqlStr).join(', ') +
        `, ${sqlStr(m.occurredAt)}, ${sqlStr(m.narrativeTime)}, ${sqlStr(m.entities === null ? null : JSON.stringify(m.entities))}, '${m.tier}', '${T0}', '${T0}')`,
    );
  }
  await rawQuery(
    `INSERT INTO chat_settings (id, userId, createdAt, updatedAt, timezone) VALUES ('${mid(50)}', '${USER}', '${T0}', '${T0}', 'Pacific/Auckland')`,
  );
  await closeDatabase();
  copyFileSync(work, join(dir, 'base.db'));
  await ensure();

  const { handleEmbeddingGenerate } = await import('@/lib/background-jobs/handlers/embedding-generate');
  const out: string[] = [];
  for (const id of [...PLANTS.map((p) => p.id), MISSING]) {
    calls.length = 0;
    captured.length = 0;
    tagged.length = 0;
    const job = {
      id: `job-${id.slice(-2)}`,
      userId: USER,
      type: 'EMBEDDING_GENERATE',
      status: 'PROCESSING',
      payload: { entityType: 'MEMORY', entityId: id, characterId: CHAR, profileId: PROFILE },
    };
    let error: string | null = null;
    try {
      await handleEmbeddingGenerate(job as never);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    out.push(JSON.stringify({ kind: 'job', jobId: job.id, entityId: id, error, calls: [...calls], lines: [...captured], tagged: [...tagged] }));
  }

  const memories = (await rawQuery(`SELECT id, hex(embedding) AS embedding FROM memories ORDER BY id`)) as unknown[];
  const status = (await rawQuery(
    `SELECT entityType, entityId, profileId, status, error FROM embedding_status ORDER BY entityId`,
  )) as unknown[];
  out.push(JSON.stringify({ kind: 'end', memories, status }));
  await closeDatabase();
  rmSync(scratch, { recursive: true, force: true });
  fs.writeFileSync(outPath, out.join('\n') + '\n');
}

test('embedding text shape oracle', async () => {
  await main();
});
