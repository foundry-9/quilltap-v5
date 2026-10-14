/**
 * Tier-2 oracle case — the memory extraction cursor repository (P4.D288 unit
 * 1, C1-1; v4 `ac6348c5f` + `64743ee13` at the `83334b9a5` pin).
 *
 * Drives v4's REAL `MemoryExtractionCursorsRepository` through the op list in
 * `fixtures/memory-keystone-tier2.json` (`cursorOps`) over TWO base files the
 * case builds here, one per table shape every v4 instance can carry:
 *
 *   - `migration` — the shape a REAL v4 instance has (R-C): the table made by
 *     v4's REAL `add-memory-extraction-cursors-v1` (`UNIQUE ("chatId",
 *     "pass")` + a live `FOREIGN KEY … ON DELETE CASCADE`), then the
 *     repository pass's `ensureCollection` (its `createdAt` index), exactly as
 *     a real boot followed by the first cursor touch leaves it;
 *   - `bare` — generateDDL's shape (`ensureCollection` alone — the dump, jest
 *     and the tsx oracles): no UNIQUE, no FK.
 *
 * Each base carries `chats` (v4's own generateDDL) with the seed's three
 * chats. The case writes the bases to `QT_FIXTURE_OUT_DIR/cursors-<shape>.db`
 * BEFORE any op, so the Rust side runs the same ops over byte-identical
 * starting files.
 *
 * Comparands per shape: every op's RESULT (a thrown op records `{threw}`), the
 * lines an op marked `captureLogs` logged — ERROR / WARN / INFO / DEBUG, off
 * the `Logger` prototype, context in v4's key order — and the final table rows
 * (`id`, `chatId`, `pass`, `messageId`, and `createdAt` / `updatedAt` only when
 * they equal the seed's pinned `2020-…` stamp, else `<ts>`) plus the table's
 * `sqlite_master` text. A `deleteChat` op deletes the chat through raw SQL on
 * v4's own connection (`foreign_keys = ON`, `client.ts:120`), so the migration
 * shape cascades and the bare shape does not.
 *
 * `setCursorRaw` / `findCursorRaw` call the SAME verbs with a pass the enum
 * does not know (TypeScript's types are erased at run time): v4 writes a
 * `<chatId>:<raw>` row and its reads drop it in Zod (R-X; C1-1's raw arm).
 *
 * Run from the pinned v4 worktree under Node 24 (`TZ` irrelevant):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   rm -rf /tmp/qt-cursors-tier2 /tmp/oracle-cursors-tier2.ndjson
 *   QT_FIXTURE_OUT_DIR=/tmp/qt-cursors-tier2 \
 *     $N/npx tsx $V5W/harness/oracle/cases/memory-extraction-cursors-tier2.ts \
 *     > /tmp/oracle-cursors-tier2.ndjson
 */

import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { runV4Migrations } from '../lib/v4-migrations.js';

interface Op {
  kind: string;
  label: string;
  chatId?: string;
  pass?: string;
  messageId?: string;
  sql?: string;
  captureLogs?: boolean;
  strict?: boolean;
}

interface Seed {
  testPepperBase64: string;
  userId: string;
  chats: Array<{ id: string; title: string; summaryAnchorMessageIds: string | null }>;
  cursorOps: Op[];
}

const PINNED_TS = '2020-01-01T00:00:00.000Z';

function sweep(path: string): void {
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    if (existsSync(path + suffix)) rmSync(path + suffix);
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const seed = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'memory-keystone-tier2.json'), 'utf8'),
  ) as Seed;
  const outDir = process.env.QT_FIXTURE_OUT_DIR;
  if (!outDir) throw new Error('QT_FIXTURE_OUT_DIR must name the dir for the base files');
  mkdirSync(outDir, { recursive: true });

  const scratch = mkdtempSync(join(tmpdir(), 'qt-cursors-tier2-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = seed.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const manager = await import('@/lib/database/manager');
  const { ChatMetadataSchema } = await import('@/lib/schemas/types');
  const { MemoryExtractionCursorSchema } = await import(
    '@/lib/schemas/memory-extraction-cursor.types'
  );
  const { MemoryExtractionCursorsRepository } = await import(
    '@/lib/database/repositories/memory-extraction-cursors.repository'
  );
  const { withStrictRepositoryFailures } = await import(
    '@/lib/database/repositories/strict-failures'
  );
  const { Logger } = await import('@/lib/logger');

  // ---- the two base files -------------------------------------------------
  const buildBase = async (shape: 'migration' | 'bare'): Promise<string> => {
    const out = join(outDir, `cursors-${shape}.db`);
    sweep(out);
    process.env.SQLITE_PATH = out;
    await manager.initializeDatabase();
    await manager.ensureCollection('chats', ChatMetadataSchema);
    const ts = '2026-01-01T00:00:00.000Z';
    for (const chat of seed.chats) {
      await manager.rawQuery(
        'INSERT INTO chats (id, userId, title, participants, createdAt, updatedAt, summaryAnchorMessageIds) VALUES (?, ?, ?, ?, ?, ?, ?)',
        [chat.id, seed.userId, chat.title, '[]', ts, ts, chat.summaryAnchorMessageIds],
      );
    }
    if (shape === 'bare') {
      await manager.ensureCollection('memory_extraction_cursors', MemoryExtractionCursorSchema);
    }
    await manager.closeDatabase();
    if (shape === 'migration') {
      // v4's REAL migration (its own connection), then the repository pass.
      const report = await runV4Migrations({
        dbPath: out,
        pepperBase64: seed.testPepperBase64,
        migrations: [
          {
            file: 'migrations/scripts/add-memory-extraction-cursors-v1.ts',
            exportName: 'addMemoryExtractionCursorsMigration',
          },
        ],
      });
      if (!report.some((r) => r.startsWith('+RAN add-memory-extraction-cursors-v1'))) {
        throw new Error(`the cursor migration did not run: ${JSON.stringify(report)}`);
      }
      process.env.SQLITE_PATH = out;
      await manager.initializeDatabase();
      await manager.ensureCollection('memory_extraction_cursors', MemoryExtractionCursorSchema);
      await manager.closeDatabase();
    }
    return out;
  };

  const bases = {
    migration: await buildBase('migration'),
    bare: await buildBase('bare'),
  };

  // ---- the log spy (ERROR / WARN / INFO / DEBUG, gated per op) ------------
  let opLogs: Array<{ level: string; message: string; fields: Array<[string, unknown]> }> | null =
    null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      opLogs?.push({ level, message, fields: Object.entries(context ?? {}) });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  for (const shape of ['migration', 'bare'] as const) {
    const work = join(scratch, `work-${shape}.db`);
    sweep(work);
    copyFileSync(bases[shape], work);
    process.env.SQLITE_PATH = work;
    await manager.initializeDatabase();
    const repo = new MemoryExtractionCursorsRepository();

    const runOp = async (op: Op): Promise<unknown> => {
      switch (op.kind) {
        case 'execSql':
          await manager.rawQuery(op.sql!);
          return null;
        case 'dump': {
          // Rowid order; a stamp is compared only when it is the seed's pinned
          // one (`<ts>` otherwise), plus whether the row's two stamps are equal
          // (`setCursor` mints ONE `now` for both on insert).
          const rows = (await manager.rawQuery(
            'SELECT "id", "chatId", "pass", "messageId", "createdAt", "updatedAt" FROM "memory_extraction_cursors" ORDER BY rowid',
          )) as Array<Record<string, string>>;
          return rows.map((r) => ({
            id: r.id,
            chatId: r.chatId,
            pass: r.pass,
            messageId: r.messageId,
            createdAt: r.createdAt === PINNED_TS ? r.createdAt : '<ts>',
            updatedAt: r.updatedAt === PINNED_TS ? r.updatedAt : '<ts>',
            sameStamp: r.createdAt === r.updatedAt,
          }));
        }
        case 'deleteChat':
          await manager.rawQuery('DELETE FROM chats WHERE id = ?', [op.chatId!]);
          return null;
        case 'findByChatId':
          return repo.findByChatId(op.chatId!);
        case 'findCursor':
        case 'findCursorRaw':
          return repo.findCursor(op.chatId!, op.pass as never);
        case 'findAllByChat':
          return [...(await repo.findAllByChat()).entries()];
        case 'setCursor':
        case 'setCursorRaw':
          await repo.setCursor(op.chatId!, op.pass as never, op.messageId!);
          return null;
        case 'clearCursor':
          return repo.clearCursor(op.chatId!, op.pass as never);
        case 'clearForChat':
          return repo.clearForChat(op.chatId!);
        default:
          throw new Error(`unknown op kind: ${op.kind}`);
      }
    };

    const results: Array<{ label: string; result: unknown; logs?: unknown[] }> = [];
    for (const op of seed.cursorOps) {
      opLogs = op.captureLogs ? [] : null;
      let result: unknown;
      try {
        result = op.strict
          ? await withStrictRepositoryFailures(() => runOp(op))
          : await runOp(op);
      } catch (err) {
        result = { threw: err instanceof Error ? err.message : String(err) };
      }
      results.push({ label: op.label, result, ...(opLogs === null ? {} : { logs: opLogs }) });
      opLogs = null;
    }

    // The final table (the last op of the corpus drops it: record what is left
    // of `sqlite_master` for it, which is nothing — the comparand is the rows
    // BEFORE the drop, captured by the op results, plus the base's DDL).
    await manager.closeDatabase();

    const Database = (await import('module')).createRequire(process.cwd() + '/')('better-sqlite3');
    const base = new Database(bases[shape], { readonly: true });
    base.pragma(`key = "x'${Buffer.from(seed.testPepperBase64, 'base64').toString('hex')}'"`);
    const master = base
      .prepare(
        "SELECT type, name, sql FROM sqlite_master WHERE tbl_name = 'memory_extraction_cursors' ORDER BY name",
      )
      .all();
    base.close();

    process.stdout.write(
      JSON.stringify({ case: 'memory-extraction-cursors-tier2', shape, master, results, pinnedTs: PINNED_TS }) +
        '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-extraction-cursors-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
