/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for the established-fact memory job handler (P4.D372, v4
 * `db6190b44`, pin `624d0d25c`): v4's REAL `handleEstablishedFactMemories`
 * (`lib/background-jobs/handlers/established-fact-memories.ts`) over the
 * committed corpus (`harness/oracle/fixtures/established-fact-memories-tier3.json`)
 * on ONE copy of the carina-memory-extraction fixture pair, the cases run in
 * order (so a repeat post meets the first post's memories).
 *
 * No LLM is called. The only stubbed seam is the embedding boundary
 * (`generateEmbeddingForUser`, canned by the exact `summary\n\ncontent` base of
 * the admitted text; each exact text RECORDED as `kind:"cannedEmbedding"` for
 * the Rust replay). The handler, admission, the repositories and the vector
 * store are v4's REAL code. The handler is called in-process, so no child job
 * scope is set (the Rust side passes `key_scope: None` to match).
 *
 * Plants (identical on both sides): each chat's participant list (raw UPDATE),
 * the fact rows (v4's REAL `addMessage`), and the archive before its case.
 * `Date.now()` is pinned to the corpus's `nowMs`.
 *
 * Emits per case `{kind:'ran', case, lines}` (every `[EstablishedFactMemories]`
 * line, every level, message + fields), then the recorded embeddings and the
 * `memories` table dump.
 *
 * Run (Node 24, from the pinned v4 worktree; cp to a /tmp mirror — jest ignores
 * `.claude/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_OUT=/tmp/qt-efm-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-efm-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-carina-memory-extraction-fixture.ts
 *   TMPO=/tmp/qt-efm-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/established-fact-memories-tier3.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/established-fact-memories-tier3.json "$TMPO/fixtures/"
 *   QT_FIXTURE_EFM_MAIN=/tmp/qt-efm-main.db QT_FIXTURE_EFM_MOUNT=/tmp/qt-efm-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-ef-memories.ndjson TZ=UTC \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- established-fact-memories-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface CaseSpec {
  name: string;
  chatId: string;
  messageId: string;
  archiveBefore?: string;
}
interface Spec {
  testPepperBase64: string;
  userId: string;
  nowMs: number;
  chatParticipants: Record<string, unknown[]>;
  facts: Array<{ chatId: string; event: Record<string, unknown> }>;
  cannedEmbeddings: Record<string, number[]>;
  cases: CaseSpec[];
}

function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) return v.toString('hex');
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'established-fact-memories-tier3.json'), 'utf8'),
  ) as Spec;
  const fixtureMain = process.env.QT_FIXTURE_EFM_MAIN;
  const fixtureMount = process.env.QT_FIXTURE_EFM_MOUNT;
  if (!fixtureMain || !existsSync(fixtureMain) || !fixtureMount || !existsSync(fixtureMount)) {
    throw new Error('QT_FIXTURE_EFM_MAIN / QT_FIXTURE_EFM_MOUNT must point at the seed fixtures');
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-efm-oracle-'));
  try {
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const workMain = join(scratch, 'efm-main.db');
    const workMount = join(scratch, 'efm-mount.db');
    copyFileSync(fixtureMain, workMain);
    copyFileSync(fixtureMount, workMount);
    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = workMain;
    process.env.SQLITE_MOUNT_INDEX_PATH = workMount;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'error';

    const embeddingRecorded = new Map<string, number[]>();
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
      const { EmbeddingError } = actual;
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          const key = Object.keys(spec.cannedEmbeddings).find((k) => text === k || text.startsWith(`${k}\n`));
          const vec = key === undefined ? undefined : spec.cannedEmbeddings[key];
          if (!vec) throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
          embeddingRecorded.set(text, vec);
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
        },
      };
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
    await initializeDatabase();

    // The handler's lines, every level, off the root logger singleton.
    const { logger } = await import('@/lib/logger');
    let caseLines: Array<{ level: string; message: string; fields: unknown }> = [];
    for (const level of ['debug', 'info', 'warn', 'error'] as const) {
      const original = (logger as unknown as Record<string, (...a: unknown[]) => unknown>)[level].bind(logger);
      (logger as unknown as Record<string, unknown>)[level] = (message: string, fields?: unknown, ...rest: unknown[]) => {
        if (typeof message === 'string' && message.startsWith('[EstablishedFactMemories]')) {
          caseLines.push({ level, message, fields: JSON.parse(JSON.stringify(fields ?? {})) });
        }
        return original(message, fields, ...rest);
      };
    }

    // Plants: participants, facts.
    for (const [chatId, participants] of Object.entries(spec.chatParticipants)) {
      await rawQuery('UPDATE chats SET participants = ? WHERE id = ?', [JSON.stringify(participants), chatId]);
    }
    const { getRepositories } = await import('@/lib/repositories/factory');
    for (const f of spec.facts) {
      await getRepositories().chats.addMessage(f.chatId, f.event as never);
    }

    const { handleEstablishedFactMemories } = await import(
      '@/lib/background-jobs/handlers/established-fact-memories'
    );
    const RealDateNow = Date.now;
    Date.now = () => spec.nowMs;
    const lines: string[] = [];
    try {
      for (const c of spec.cases) {
        if (c.archiveBefore) {
          await rawQuery('UPDATE characters SET archivedAt = ? WHERE id = ?', ['2023-07-01T00:00:00.000Z', c.archiveBefore]);
        }
        caseLines = [];
        await handleEstablishedFactMemories({
          id: `job-${c.name}`,
          userId: spec.userId,
          type: 'ESTABLISHED_FACT_MEMORIES',
          payload: { chatId: c.chatId, messageId: c.messageId },
        } as never);
        await new Promise((resolve) => setTimeout(resolve, 30));
        lines.push(JSON.stringify({ kind: 'ran', case: c.name, lines: caseLines }));
      }
    } finally {
      Date.now = RealDateNow;
    }

    for (const [text, vector] of embeddingRecorded) {
      lines.push(JSON.stringify({ kind: 'cannedEmbedding', text, vector }));
    }
    const columns = ((await rawQuery('PRAGMA table_info(memories)')) as Array<{ name: string }>).map((c) => c.name);
    const rawRows = (await rawQuery('SELECT * FROM memories')) as Array<Record<string, unknown>>;
    const rows = rawRows
      .map((r) => Object.fromEntries(columns.map((col) => [col, canonValue(r[col])])))
      .sort((a, b) => {
        const ka = `${a.characterId}|${a.content}|${a.id}`;
        const kb = `${b.characterId}|${b.content}|${b.id}`;
        return ka < kb ? -1 : ka > kb ? 1 : 0;
      });
    lines.push(JSON.stringify({ kind: 'table', table: 'memories', columns, rows }));

    closeMountIndexSQLiteClient();
    await closeDatabase();
    fs.writeFileSync(outPath, lines.join('\n') + '\n');
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

test('established-fact-memories tier-3 oracle', async () => {
  await main();
}, 600_000);
