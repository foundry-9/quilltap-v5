/**
 * Oracle — P4.D288 (C1-11): v4's REAL `lib/memory/extraction/cursor-bundle.ts`
 * (`ac6348c5f` + bug 245, at the `83334b9a5` pin).
 *
 * Tier-1 (pure): `cursorsToBundleFields` over cursor maps (a blank cursor
 * left out) and `takeBundleCursors` over bundle chats (every bundle key
 * deleted, only non-empty strings kept, the other keys in place).
 *
 * Tier-2 (stateful, over a real database v4's own code builds): for each
 * cursor-table SHAPE — `migration` (the table v4's `ac6348c5f` migration
 * writes, with its foreign key to `chats`), `fresh` (the repository's
 * generateDDL table, no foreign key) and `absent` (no table at all) — the
 * case writes `base-<shape>.db` (a minimal `chats` table, the cursor table,
 * three seeded cursors via the REAL `setCursor`), copies it for the Rust side,
 * re-opens and runs the op list: `loadBundleCursors`, `withBundleCursorFields`
 * (preloaded / read per chat; a stale bundle key overwritten in place),
 * `restoreBundleCursors` (two written + the DEBUG, nothing, a foreign-key
 * refusal → the WARN on `migration`); then a column renamed under the table
 * so every read and write fails, and each wrapper again outside and inside
 * v4's strict scope (`withStrictRepositoryFailures`) — the three WARNs.
 * Each op: `{shape, op, input, result, lines}`; every captured line's own
 * context (a child's `module` first).
 *
 * ONE shape per process (`QT_CURSOR_BUNDLE_SHAPE`): v4's repository factory
 * caches each collection's "initialized" flag, so a second database in the
 * same process would never get its lazy table. The tier-1 lines ride the
 * `migration` run.
 *
 * Run (Node 24, from the round's pin), one invocation per shape:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -rf /tmp/qt-cursor-bundle /tmp/oracle-cursor-bundle-*.ndjson
 *   cd ~/source/quilltap-server
 *   for S in migration fresh absent; do
 *     QT_CURSOR_BUNDLE_SHAPE=$S QT_FIXTURE_CURSOR_BUNDLE_DIR=/tmp/qt-cursor-bundle \
 *       $N/npx tsx $V5W/harness/oracle/cases/cursor-bundle.ts > /tmp/oracle-cursor-bundle-$S.ndjson
 *   done
 */

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
const cid = (n: number) => `e2900000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const A = cid(1);
const B = cid(2);
const C = cid(3);
const D = cid(4);
const MISSING = cid(99);
const MIGRATION_DDL = `CREATE TABLE "memory_extraction_cursors" (
          "id" TEXT PRIMARY KEY,
          "chatId" TEXT NOT NULL,
          "pass" TEXT NOT NULL,
          "messageId" TEXT NOT NULL,
          "createdAt" TEXT NOT NULL,
          "updatedAt" TEXT NOT NULL,
          UNIQUE ("chatId", "pass"),
          FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE
        )`;

async function main(): Promise<void> {
  const out: string[] = [];
  const dir = process.env.QT_FIXTURE_CURSOR_BUNDLE_DIR;
  if (!dir) throw new Error('QT_FIXTURE_CURSOR_BUNDLE_DIR must name the dir for the base dbs');
  mkdirSync(dir, { recursive: true });
  const scratch = mkdtempSync(join(tmpdir(), 'qt-cursor-bundle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = PEPPER;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'debug';

  const CAPTURED = new Set([
    '[ExtractionCursors] Could not read the cursor table for a bundle; reading per chat',
    "[ExtractionCursors] Could not read a chat's cursors for a bundle; leaving them out",
    '[ExtractionCursors] Could not restore a bundled cursor; the pass will re-read',
    '[ExtractionCursors] Restored bundled cursors',
  ]);
  const { logger } = await import('@/lib/logger');
  let capturing = false;
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!capturing || !CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const bundle = await import('@/lib/memory/extraction/cursor-bundle');
  const { withStrictRepositoryFailures } = await import('@/lib/database/repositories/strict-failures');

  const only = process.env.QT_CURSOR_BUNDLE_SHAPE as 'migration' | 'fresh' | 'absent' | undefined;
  if (!only || !['migration', 'fresh', 'absent'].includes(only)) {
    throw new Error('QT_CURSOR_BUNDLE_SHAPE must be migration | fresh | absent');
  }

  // ---- tier-1 (on the migration run) ----
  const maps: Array<Record<string, string>> = [
    {},
    { 'other-fold': 'm1' },
    { turn: 't', 'other-fold': 'o', 'episode-fold': 'e' },
    { 'episode-fold': '', turn: 't2' },
  ];
  if (only === 'migration') for (const m of maps) out.push(JSON.stringify({ case: 'fields', input: m, result: bundle.cursorsToBundleFields(m) }));
  const chats: Array<Record<string, unknown>> = [
    { id: 'c1', title: 'T' },
    {
      turnExtractionWatermarkMessageId: 't1',
      id: 'c2',
      otherExtractionWatermarkMessageId: 'o1',
      title: 'T',
      episodeExtractionWatermarkMessageId: 'e1',
      tail: true,
    },
    {
      id: 'c3',
      otherExtractionWatermarkMessageId: null,
      episodeExtractionWatermarkMessageId: '',
      turnExtractionWatermarkMessageId: 7,
    },
  ];
  for (const c of only === 'migration' ? chats : []) {
    const { chat, cursors } = bundle.takeBundleCursors(c);
    out.push(JSON.stringify({ case: 'take', input: c, chat, cursors }));
  }

  // ---- tier-2 ----
  for (const shape of [only]) {
    const work = join(dir, `work-${shape}.db`);
    const base = join(dir, `base-${shape}.db`);
    for (const p of [work, base]) {
      for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(p + suffix)) rmSync(p + suffix);
    }
    process.env.SQLITE_PATH = work;
    const manager = await import('@/lib/database/manager');
    await manager.initializeDatabase();
    const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
    const raw = getRawDatabase();
    if (!raw) throw new Error('main DB handle unavailable');
    raw.exec('CREATE TABLE IF NOT EXISTS "chats" ("id" TEXT PRIMARY KEY)');
    for (const id of [A, B, C, D]) raw.prepare('INSERT INTO "chats" ("id") VALUES (?)').run(id);
    const { getRepositories } = await import('@/lib/repositories/factory');
    if (shape === 'migration') raw.exec(MIGRATION_DDL);
    if (shape !== 'absent') {
      const repo = getRepositories().memoryExtractionCursors;
      await repo.setCursor(A, 'other-fold', 'm-a1');
      await repo.setCursor(A, 'episode-fold', 'm-a2');
      await repo.setCursor(B, 'turn', 'm-b1');
    }
    await manager.closeDatabase();
    copyFileSync(work, base);
    await manager.initializeDatabase();

    capturing = true;
    const run = async (op: string, input: Record<string, unknown>, strict: boolean, f: () => Promise<unknown>) => {
      captured = [];
      const result = strict ? await withStrictRepositoryFailures(f) : await f();
      out.push(JSON.stringify({ case: 'stateful', shape, op, input: { ...input, strict }, result, lines: captured }));
    };
    const entries = (m: ReadonlyMap<string, unknown> | null) => (m ? [...m.entries()] : null);
    const preload = async () => bundle.loadBundleCursors();
    await run('load', {}, false, async () => entries(await bundle.loadBundleCursors()));
    const pre = await preload();
    await run('with', { chat: { id: A, title: 'A', otherExtractionWatermarkMessageId: 'stale' }, preloaded: true }, false, () =>
      bundle.withBundleCursorFields({ id: A, title: 'A', otherExtractionWatermarkMessageId: 'stale' }, pre),
    );
    await run('with', { chat: { id: C, title: 'C' }, preloaded: true }, false, () =>
      bundle.withBundleCursorFields({ id: C, title: 'C' }, pre),
    );
    await run('with', { chat: { id: B }, preloaded: false }, false, () => bundle.withBundleCursorFields({ id: B }));
    if (shape !== 'absent') {
      await run('restore', { chatId: D, cursors: { 'other-fold': 'm-d1', turn: 'm-d3' } }, false, () =>
        bundle.restoreBundleCursors(D, { 'other-fold': 'm-d1', turn: 'm-d3' }),
      );
      await run('restore', { chatId: D, cursors: {} }, false, () => bundle.restoreBundleCursors(D, {}));
      await run('restore', { chatId: MISSING, cursors: { 'episode-fold': 'm-x' } }, false, () =>
        bundle.restoreBundleCursors(MISSING, { 'episode-fold': 'm-x' }),
      );
      // The failure arms. A renamed `chatId` fails every FILTERED read (the
      // per-chat read) on both sides; v4's one-read `findAllByChat` reads
      // `SELECT *` and only Zod-drops the rows, so its WARN arm is posed
      // here by a throwing method (v5 fails for real — the line's error text
      // is masked on that op, `maskError`). A renamed `messageId` then fails
      // every write.
      const reopened = getRawDatabase();
      if (!reopened) throw new Error('main DB handle unavailable');
      const plant = (sql: string) => {
        reopened.exec(sql);
        out.push(JSON.stringify({ case: 'stateful', shape, plant: sql }));
      };
      plant('ALTER TABLE "memory_extraction_cursors" RENAME COLUMN "chatId" TO "cid"');
      await run('load', {}, false, async () => entries(await bundle.loadBundleCursors()));
      const cursorsRepo = getRepositories().memoryExtractionCursors as unknown as Record<string, unknown>;
      const realFindAll = cursorsRepo.findAllByChat;
      cursorsRepo.findAllByChat = async () => {
        throw new Error('posed: the cursor table could not be read');
      };
      await run('load', { maskError: true }, true, async () => entries(await bundle.loadBundleCursors()));
      cursorsRepo.findAllByChat = realFindAll;
      await run('with', { chat: { id: A }, preloaded: false }, false, () => bundle.withBundleCursorFields({ id: A }));
      await run('with', { chat: { id: A }, preloaded: false }, true, () => bundle.withBundleCursorFields({ id: A }));
      plant('ALTER TABLE "memory_extraction_cursors" RENAME COLUMN "messageId" TO "mid"');
      await run('restore', { chatId: D, cursors: { 'other-fold': 'm-z', turn: 'm-y' } }, false, () =>
        bundle.restoreBundleCursors(D, { 'other-fold': 'm-z', turn: 'm-y' }),
      );
    }
    capturing = false;
    await manager.closeDatabase();
  }
  process.stdout.write(out.join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`cursor-bundle oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
