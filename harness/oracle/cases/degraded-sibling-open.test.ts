/**
 * @jest-environment node
 *
 * ORACLE for P4.159 (dogfood #150) — v4's DEGRADED sibling open. Drives v4's
 * REAL dedicated-database clients and their REAL integrity checks over planted
 * bytes, and v4's REAL PHASE 3.1 pass over a degraded partition:
 *
 *   - `open` rows: per partition (`mountIndex` / `llmLogs`) × plant, the
 *     module's globals cleared (a cold process), then exactly what
 *     `SQLiteBackend.connect()` runs for that partition
 *     (`backends/sqlite/backend.ts:571-617`): `getMountIndexSQLiteClient(
 *     loadMountIndexConfig())` / `getLLMLogsSQLiteClient(loadLLMLogsConfig())`
 *     and, when that answered a connection, `runMountIndexIntegrityCheck` /
 *     `runLLMLogsIntegrityCheck`. Records every line v4 logged on the way
 *     (level, message, the merged fields — the child logger's `module` first,
 *     as v4's `log()` merges it — with the plant's path normalized to
 *     `<path>`), whether the client answered a connection, and the final
 *     `isMountIndexDegraded()` / `isLLMLogsDegraded()`. The periodic
 *     checkpoints and the physical backup that follow in `connect()` are NOT
 *     driven (timers + async file copies; no line of theirs is ported here).
 *     The plants:
 *       - `garbage` — `len` bytes of `(i * mul + add) & 255` (the walk's C3
 *         shape: a sibling overwritten with junk);
 *       - `sound` — a fresh encrypted file built from `sql` (v4's own open:
 *         key, then `journal_mode = truncate`);
 *       - `integrity` — the sound build with page `page` (1-based, 4096-byte
 *         pages) overwritten by the garbage pattern: the open succeeds (page 1
 *         is intact) and `quick_check` answers a non-`ok` result.
 *     Each row carries its recipe so the Rust side rebuilds identical bytes.
 *   - `pass` rows: v4's REAL `verifyStructuralTables` over the REAL repository
 *     container (`createRepositories`, handed to the pass through the factory
 *     mock) with one or both dedicated partitions DEGRADED the way the client
 *     leaves them (`__quilltap<X>Degraded = true`, no connection) and the
 *     other on a fresh in-memory database. `getDatabaseAsync` is jest.setup's
 *     no-op mock; `helpDocChunks.verifyStructure` (the one MAIN repository —
 *     it would read through the mocked `rawQuery`) is stubbed to `null`, i.e.
 *     main sound — recorded. Records the returned problems and every line.
 *
 * Emits NDJSON to $QT_ORACLE_OUT.
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree). STAGE this case
 * OUTSIDE `.claude/` — v4's jest ignores those paths.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   STAGE=/tmp/qt-oracle-stage-degraded-sibling-open
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases
 *   cp $V5W/harness/oracle/cases/degraded-sibling-open.test.ts $STAGE/harness/oracle/cases/
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-degraded-sibling-open.ndjson \
 *     PATH=$N:$PATH npx jest --silent --watchman=false --testTimeout=240000 \
 *       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "degraded-sibling-open\.test\.ts$"
 */

import * as fs from 'fs';
import { join } from 'node:path';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

/** The throwaway test pepper (32 bytes, base64) — never a real instance's. */
const TEST_PEPPER = '3q2+796tvu3erb7t3q2+796tvu3erb7t3q2+796tvu0=';

const GARBAGE = { len: 4608, mul: 131, add: 17 };
/** Enough rows that the table spans many pages; page 3 is an interior page. */
const SOUND_SQL = [
  'CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)',
  "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000) INSERT INTO t (v) SELECT printf('%.200c%d', 'x', i) FROM n",
];
const INTEGRITY_PAGE = 3;
const PAGE = 4096;

type Partition = 'mountIndex' | 'llmLogs';
type Line = { level: string; message: string; fields: Record<string, unknown> };

function garbageBytes(): Buffer {
  const b = Buffer.alloc(GARBAGE.len);
  for (let i = 0; i < b.length; i++) b[i] = (i * GARBAGE.mul + GARBAGE.add) & 255;
  return b;
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-degraded-sibling-oracle-'));
  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  jest.resetModules();
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  const Database = jest.requireActual(cipherDriverPath);

  const { getMountIndexSQLiteClient, isMountIndexDegraded } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getLLMLogsSQLiteClient, isLLMLogsDegraded } = await import(
    '@/lib/database/backends/sqlite/llm-logs-client'
  );
  const { runMountIndexIntegrityCheck } = await import(
    '@/lib/database/backends/sqlite/mount-index-protection'
  );
  const { runLLMLogsIntegrityCheck } = await import(
    '@/lib/database/backends/sqlite/llm-logs-protection'
  );
  const { loadMountIndexConfig, loadLLMLogsConfig } = await import('@/lib/database/config');
  const { createRepositories } = await import('@/lib/database/repositories');
  const { verifyStructuralTables } = await import('@/lib/startup/verify-structural-tables');
  const { Logger } = await import('@/lib/logger');

  // Every level, singleton and children alike, off the prototype (before the
  // level check): the merged fields exactly as v4's `log()` builds them —
  // the child's bound context first, then the call's.
  let sink: Line[] | null = null;
  let pathToken: string | null = null;
  const levels = ['error', 'warn', 'info', 'debug'] as const;
  const originals: Record<string, unknown> = {};
  for (const level of levels) {
    const original = (Logger.prototype as unknown as Record<string, unknown>)[level];
    originals[level] = original;
    (Logger.prototype as unknown as Record<string, unknown>)[level] = function (
      this: { context?: Record<string, unknown> },
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (sink) {
        const fields: Record<string, unknown> = { ...(this.context ?? {}), ...(context ?? {}) };
        if (pathToken !== null && fields.path === pathToken) fields.path = '<path>';
        sink.push({ level, message, fields });
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    };
  }

  const g = globalThis as unknown as Record<string, unknown>;
  const clearGlobals = () => {
    for (const k of [
      '__quilltapMountIndexDatabase',
      '__quilltapMountIndexDegraded',
      '__quilltapLLMLogsDatabase',
      '__quilltapLLMLogsDegraded',
    ]) {
      const db = g[k] as { close?: () => void } | undefined;
      if (db && typeof db === 'object' && typeof db.close === 'function') {
        try { db.close(); } catch { /* ignore */ }
      }
      g[k] = undefined;
    }
  };

  const keyHex = Buffer.from(TEST_PEPPER, 'base64').toString('hex');
  const buildSound = (path: string) => {
    const db = new Database(path);
    db.pragma(`key = "x'${keyHex}'"`);
    db.pragma('journal_mode = truncate');
    for (const s of SOUND_SQL) db.exec(s);
    db.close();
  };

  const outLines: string[] = [];
  const out = (row: unknown) => outLines.push(JSON.stringify(row));
  try {
    // ---- open rows ----------------------------------------------------------
    const plants = ['garbage', 'sound', 'integrity'] as const;
    for (const partition of ['mountIndex', 'llmLogs'] as Partition[]) {
      for (const plant of plants) {
        const path = join(scratch, `${partition}-${plant}.db`);
        if (plant === 'garbage') {
          fs.writeFileSync(path, garbageBytes());
        } else {
          buildSound(path);
          if (plant === 'integrity') {
            const b = fs.readFileSync(path);
            const off = (INTEGRITY_PAGE - 1) * PAGE;
            const g2 = garbageBytes();
            for (let i = 0; i < PAGE; i++) b[off + i] = g2[i % g2.length];
            fs.writeFileSync(path, b);
          }
        }
        clearGlobals();
        if (partition === 'mountIndex') process.env.SQLITE_MOUNT_INDEX_PATH = path;
        else process.env.SQLITE_LLM_LOGS_PATH = path;
        pathToken = path;
        sink = [];
        let client = false;
        if (partition === 'mountIndex') {
          const db = getMountIndexSQLiteClient(loadMountIndexConfig());
          if (db) {
            client = true;
            runMountIndexIntegrityCheck(db);
          }
        } else {
          const db = getLLMLogsSQLiteClient(loadLLMLogsConfig());
          if (db) {
            client = true;
            runLLMLogsIntegrityCheck(db);
          }
        }
        const degraded = partition === 'mountIndex' ? isMountIndexDegraded() : isLLMLogsDegraded();
        out({
          kind: 'open',
          partition,
          plant,
          recipe: {
            garbage: GARBAGE,
            soundSql: SOUND_SQL,
            page: plant === 'integrity' ? INTEGRITY_PAGE : null,
            pageSize: PAGE,
          },
          client,
          degraded,
          lines: sink,
        });
        sink = null;
        pathToken = null;
        clearGlobals();
      }
    }

    // ---- pass rows ----------------------------------------------------------
    const factory = jest.requireMock('@/lib/repositories/factory') as {
      getRepositories: jest.Mock;
    };
    for (const degradedSet of [['mountIndex'], ['llmLogs'], ['mountIndex', 'llmLogs']] as Partition[][]) {
      clearGlobals();
      const opened: Array<{ close(): void }> = [];
      for (const partition of ['mountIndex', 'llmLogs'] as Partition[]) {
        const prefix = partition === 'mountIndex' ? '__quilltapMountIndex' : '__quilltapLLMLogs';
        if (degradedSet.includes(partition)) {
          g[`${prefix}Degraded`] = true;
        } else {
          const db = new Database(':memory:');
          opened.push(db);
          g[`${prefix}Database`] = db;
          g[`${prefix}Degraded`] = false;
        }
      }
      const repos = createRepositories() as unknown as Record<string, { verifyStructure?: () => Promise<string | null> }>;
      repos.helpDocChunks.verifyStructure = async () => null;
      factory.getRepositories.mockReturnValue(repos);
      sink = [];
      const problems = await verifyStructuralTables();
      out({ kind: 'pass', degraded: degradedSet, problems, lines: sink });
      sink = null;
      g.__quilltapMountIndexDatabase = undefined;
      g.__quilltapLLMLogsDatabase = undefined;
      for (const db of opened) db.close();
      clearGlobals();
    }
  } finally {
    for (const level of levels) {
      (Logger.prototype as unknown as Record<string, unknown>)[level] = originals[level];
    }
    clearGlobals();
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(`degraded-sibling-open oracle wrote ${outPath} (${outLines.length} rows)\n`);
}

test('degraded-sibling-open oracle', async () => {
  await main();
});
