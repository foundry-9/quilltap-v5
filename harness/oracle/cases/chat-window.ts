/**
 * Oracle — P4.D375 (v4 4.12.1 `6121f59b9`, the `624d0d25c` pin): v4's REAL
 * `lib/scriptorium/chat-window.ts` and the `chatIds` scope of
 * `searchConversationChunks`.
 *
 * Tier 1 (`kind: 'span'` / `'overlap'`): `chatSpanOf` / `chatOverlapsWindow`
 * over a pure corpus — `lastMessageAt` null / `""` / unparseable / earlier
 * than `createdAt` (`Math.max`), `updatedAt` later, `createdAt` unparseable,
 * a NaN window bound, the inclusive edges.
 *
 * Tier 2 (`kind: 'db'`): over a COPY of the search-tools fixture pair (built
 * by `build-search-tools-fixture.ts`, read-only here), with chats planted
 * in-case through a keyed driver connection BEFORE the manager opens the file
 * (memory note `jest-oracle-in-case-plants`); the plant SQL rides the output
 * so the Rust side replays the SAME statements. `findChatsInWindow` by
 * character / by user, `excludeChatId`, equal-end ties (repository order
 * kept), a help and a Brahma chat counted, an unparseable bound; then
 * `searchConversationChunks` with `chatIds` restricting / empty / absent and
 * `minScore: 0` sticking; then the strict-mode arm (`withStrictRepositoryFailures`
 * over a renamed `chats` table) and its non-strict twin. Each op records its
 * result and the lines it logged.
 *
 * Run (Node 24, from the round's pin; the search-tools pair first, per its
 * own header):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_TMP_MAIN=/tmp/qt-cw-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-cw-mount.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-search-tools-fixture.ts
 *   QT_FIXTURE_CW_MAIN=/tmp/qt-cw-main.db QT_FIXTURE_CW_MOUNT=/tmp/qt-cw-mount.db \
 *     $N/npx tsx $V5W/harness/oracle/cases/chat-window.ts > /tmp/oracle-chat-window.ndjson
 */

import * as fs from 'fs';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

(globalThis as Record<string, unknown>).__quilltapJobHost = {
  child: null,
  spawning: false,
  shuttingDown: true,
  childCrashed: false,
  restartTimestamps: [],
  signalHandlersInstalled: false,
  invalidationListeners: new Set(),
};

interface Spec {
  testPepperBase64: string;
  userId: string;
  charAId: string;
  charBId: string;
  chatAId: string;
  cannedEmbeddings: Record<string, number[]>;
}

const cid = (n: number) => `c3750000-0000-4000-8000-${String(n).padStart(12, '0')}`;

/** One planted chat: a clone of chat A with its span (and type) replaced. */
function plantChat(
  chatAId: string,
  id: string,
  title: string,
  createdAt: string,
  updatedAt: string,
  lastMessageAt: string | null,
  chatType = 'salon',
): string {
  const q = (s: string | null) => (s === null ? 'NULL' : `'${s.replace(/'/g, "''")}'`);
  return [
    `CREATE TEMP TABLE cw_clone AS SELECT * FROM chats WHERE id = '${chatAId}'`,
    `UPDATE cw_clone SET id = ${q(id)}, title = ${q(title)}, createdAt = ${q(createdAt)}, updatedAt = ${q(updatedAt)}, lastMessageAt = ${q(lastMessageAt)}, chatType = ${q(chatType)}`,
    'INSERT INTO chats SELECT * FROM cw_clone',
    'DROP TABLE cw_clone',
  ].join(';\n');
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(fs.readFileSync(join(here, '..', 'fixtures', 'search-tools.json'), 'utf8')) as Spec;
  const out: Array<Record<string, unknown>> = [];
  // The logger reads LOG_LEVEL when it loads — before the first import below.
  process.env.LOG_LEVEL = 'debug';

  // ---- tier 1: the pure span / overlap corpus ----
  const { chatSpanOf, chatOverlapsWindow } = await import('@/lib/scriptorium/chat-window');
  const C = '2026-10-28T10:00:00.000Z';
  const spans: Array<[string, Record<string, unknown>]> = [
    ['last_first', { createdAt: C, lastMessageAt: '2026-10-28T11:00:00.000Z', updatedAt: '2026-11-04T12:00:00.000Z' }],
    ['last_null_updated', { createdAt: C, lastMessageAt: null, updatedAt: '2026-10-29T00:00:00.000Z' }],
    ['last_empty_updated', { createdAt: C, lastMessageAt: '', updatedAt: '2026-10-29T00:00:00.000Z' }],
    ['last_unparseable_updated', { createdAt: C, lastMessageAt: 'soon', updatedAt: '2026-10-30T00:00:00.000Z' }],
    ['last_before_created', { createdAt: C, lastMessageAt: '2026-10-01T00:00:00.000Z', updatedAt: C }],
    ['updated_garbage', { createdAt: C, lastMessageAt: null, updatedAt: 'garbage' }],
    ['created_unparseable', { createdAt: 'nope', lastMessageAt: '2026-10-28T11:00:00.000Z', updatedAt: 'nope' }],
    ['date_only_created', { createdAt: '2026-10-28', lastMessageAt: null, updatedAt: '2026-10-28' }],
    ['absent_fields', { createdAt: C }],
  ];
  for (const [label, chat] of spans) {
    out.push({ kind: 'span', label, chat, result: chatSpanOf(chat as never) });
  }
  const TODAY = { from: '2026-11-04T06:00:00.000Z', to: '2026-11-05T05:59:59.999Z' };
  const overlaps: Array<[string, Record<string, unknown>, { from: string; to: string }]> = [
    ['retitled_today', { createdAt: C, lastMessageAt: '2026-10-28T11:00:00.000Z', updatedAt: '2026-11-04T12:00:00.000Z' }, TODAY],
    ['ran_into_window', { createdAt: '2026-11-03T22:00:00.000Z', lastMessageAt: '2026-11-04T07:00:00.000Z', updatedAt: '2026-11-04T07:00:00.000Z' }, TODAY],
    ['ends_at_from', { createdAt: '2026-11-03T22:00:00.000Z', lastMessageAt: TODAY.from, updatedAt: TODAY.from }, TODAY],
    ['starts_at_to', { createdAt: TODAY.to, lastMessageAt: null, updatedAt: TODAY.to }, TODAY],
    ['ends_before_from', { createdAt: '2026-11-03T22:00:00.000Z', lastMessageAt: '2026-11-04T05:59:59.999Z', updatedAt: '2026-11-04T05:59:59.999Z' }, TODAY],
    ['nan_from', { createdAt: '2026-11-04T08:00:00.000Z', lastMessageAt: null, updatedAt: '2026-11-04T08:00:00.000Z' }, { from: 'soon', to: TODAY.to }],
    ['nan_to', { createdAt: '2026-11-04T08:00:00.000Z', lastMessageAt: null, updatedAt: '2026-11-04T08:00:00.000Z' }, { from: TODAY.from, to: '' }],
    ['created_unparseable', { createdAt: 'nope', lastMessageAt: '2026-11-04T08:00:00.000Z', updatedAt: 'nope' }, TODAY],
    ['inverted_window', { createdAt: '2026-11-04T08:00:00.000Z', lastMessageAt: null, updatedAt: '2026-11-04T08:00:00.000Z' }, { from: TODAY.to, to: TODAY.from }],
  ];
  for (const [label, chat, window] of overlaps) {
    out.push({ kind: 'overlap', label, chat, window, result: chatOverlapsWindow(chat as never, window) });
  }

  // ---- tier 2: the real DB ----
  const mainFixture = process.env.QT_FIXTURE_CW_MAIN;
  const mountFixture = process.env.QT_FIXTURE_CW_MOUNT;
  if (!mainFixture || !existsSync(mainFixture) || !mountFixture || !existsSync(mountFixture)) {
    throw new Error('QT_FIXTURE_CW_MAIN and QT_FIXTURE_CW_MOUNT must point at the search-tools pair');
  }
  const scratch = mkdtempSync(join(tmpdir(), 'qt-chat-window-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const mainWork = join(scratch, 'main-work.db');
  const mountWork = join(scratch, 'mount-work.db');
  copyFileSync(mainFixture, mainWork);
  copyFileSync(mountFixture, mountWork);

  const A = spec.chatAId;
  // Every planted span is schema-valid: v4's repository DROPS a chat row whose
  // timestamps fail `ChatMetadataSchema` (an unparseable `createdAt` /
  // `lastMessageAt` never reaches `findChatsInWindow`), where v5 validates no
  // chat row (the recorded `chats_read` divergence) — those shapes are pinned
  // by the tier-1 rows above instead.
  const plants = [
    plantChat(A, cid(1), 'In window', '2025-06-10T10:00:00.000Z', '2025-06-11T00:00:00.000Z', '2025-06-11T09:00:00.000Z'),
    plantChat(A, cid(2), 'Retitled later', '2025-05-01T10:00:00.000Z', '2025-06-15T12:00:00.000Z', '2025-05-02T09:00:00.000Z'),
    plantChat(A, cid(3), '', '2025-06-12T08:00:00.000Z', '2025-06-12T09:00:00.000Z', '2025-06-12T09:00:00.000Z', 'help'),
    plantChat(A, cid(4), 'Brahma', '2025-06-13T08:00:00.000Z', '2025-06-14T08:00:00.000Z', null, 'brahma'),
    plantChat(A, cid(5), 'Tie A', '2025-06-01T08:00:00.000Z', '2025-06-15T12:00:00.000Z', '2025-06-15T12:00:00.000Z'),
    plantChat(A, cid(6), 'Tie B', '2025-06-02T08:00:00.000Z', '2025-06-15T12:00:00.000Z', '2025-06-15T12:00:00.000Z'),
    plantChat(A, cid(8), 'Ran into window', '2025-06-09T20:00:00.000Z', '2025-06-10T01:00:00.000Z', '2025-06-10T01:00:00.000Z'),
  ];
  {
    const keyHex = Buffer.from(spec.testPepperBase64, 'base64').toString('hex');
    const { createRequire } = await import('node:module');
    const Driver = createRequire(join(process.cwd(), 'package.json'))('better-sqlite3');
    const conn = new Driver(mainWork);
    conn.pragma(`key = "x'${keyHex}'"`);
    for (const sql of plants) conn.exec(sql);
    conn.close();
  }

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    'Chats in time window',
    'Could not read chats for a time window',
    'Error finding entities by filter',
    'Failed to find chats by character ID',
    'Conversation search restricted to window chats',
  ]);
  const { logger } = await import('@/lib/logger');
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message?: string } };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, module: _m, ...fields } = data.context ?? {};
      if (data.error?.message !== undefined) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
  await initializeDatabase();
  const { findChatsInWindow } = await import('@/lib/scriptorium/chat-window');
  const { searchConversationChunks } = await import('@/lib/scriptorium/conversation-search');
  const { withStrictRepositoryFailures } = await import('@/lib/database/repositories/strict-failures');

  const run = async (op: string, input: Record<string, unknown>, f: () => Promise<unknown>) => {
    captured = [];
    let result: unknown;
    try {
      result = await f();
    } catch (error) {
      result = { threw: error instanceof Error ? error.message : String(error) };
    }
    out.push({ kind: 'db', op, input, result, lines: captured });
  };
  const ids = (chats: Array<{ id: string }>) => chats.map((c) => c.id);
  const W = { from: '2025-06-10T00:00:00.000Z', to: '2025-06-20T23:59:59.999Z' };

  await run('find', { characterId: spec.charAId, window: W }, async () =>
    ids(await findChatsInWindow({ characterId: spec.charAId, userId: spec.userId, window: W })));
  await run('find', { characterId: spec.charAId, window: W, excludeChatId: cid(1) }, async () =>
    ids(await findChatsInWindow({ characterId: spec.charAId, userId: spec.userId, window: W, excludeChatId: cid(1) })));
  await run('find', { window: W }, async () => ids(await findChatsInWindow({ userId: spec.userId, window: W })));
  await run('find', { characterId: spec.charBId, window: W }, async () =>
    ids(await findChatsInWindow({ characterId: spec.charBId, userId: spec.userId, window: W })));
  await run('find', { characterId: spec.charAId, window: { from: 'soon', to: W.to } }, async () =>
    ids(await findChatsInWindow({ characterId: spec.charAId, userId: spec.userId, window: { from: 'soon', to: W.to } })));

  const embedding = new Float32Array(spec.cannedEmbeddings['what did we say about the ledger']);
  const hits = (r: Array<{ chunkId: string; chatId: string; score: number }>) =>
    r.map((h) => ({ chunkId: h.chunkId, chatId: h.chatId, score: h.score }));
  for (const input of [
    { chatIds: [A], minScore: 0.3 },
    { chatIds: [A], minScore: 0 },
    { chatIds: [cid(1)], minScore: 0 },
    { chatIds: [] as string[], minScore: 0 },
    { minScore: 0 },
  ] as Array<{ chatIds?: string[]; minScore: number }>) {
    await run('conversations', input, async () =>
      hits(
        await searchConversationChunks(embedding, {
          characterId: spec.charAId,
          userId: spec.userId,
          limit: 10,
          minScore: input.minScore,
          query: 'what did we say about the ledger',
          applyLiteralPhraseBoost: true,
          ...(input.chatIds ? { chatIds: new Set(input.chatIds) } : {}),
        }) as never,
      ),
    );
  }

  // ---- the read failure: non-strict (the filter line, empty) and strict ----
  const gone = 'ALTER TABLE chats RENAME TO chats_gone';
  captured = [];
  await rawQuery(gone);
  out.push({ kind: 'db', op: 'plant', input: { sql: gone }, result: null, lines: captured });
  await run('find', { characterId: spec.charAId, window: W }, async () =>
    ids(await findChatsInWindow({ characterId: spec.charAId, userId: spec.userId, window: W })));
  await run('find', { characterId: spec.charAId, window: W, strict: true }, async () =>
    ids(await withStrictRepositoryFailures(() =>
      findChatsInWindow({ characterId: spec.charAId, userId: spec.userId, window: W }))));
  await run('find', { window: W, strict: true }, async () =>
    ids(await withStrictRepositoryFailures(() => findChatsInWindow({ userId: spec.userId, window: W }))));

  await closeDatabase();
  for (const line of out) process.stdout.write(JSON.stringify(line) + '\n');
  process.stdout.write(JSON.stringify({ kind: 'plants', plants }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`chat-window oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
