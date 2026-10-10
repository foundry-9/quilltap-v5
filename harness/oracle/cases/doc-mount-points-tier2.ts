/**
 * Tier-2 oracle case — the `doc_mount_points` repo (the WIDEST mount-index
 * sibling-DB repo of Phase 2: enums, a boolean, two JSON arrays, three REAL-int
 * counters, nullable strings/timestamp).
 *
 * Proves what state v4 leaves the MOUNT-INDEX database in after a fixed
 * create + update + delete sequence, so the Rust port can be diffed against it.
 *
 * ── THE SIBLING-DB MACHINERY (mirrors the group_character_members pilot) ──────
 * The data lives in `quilltap-mount-index.db`, NOT the main DB. So:
 *   - SQLITE_MOUNT_INDEX_PATH points at the working COPY of the fixture (seed +
 *     ops applied here); SQLITE_PATH points at a fresh throwaway main DB;
 *   - ops run through the REAL `DocMountPointsRepository` (overridden
 *     getCollection routes to the mount-index DB);
 *   - the raw read-back is done through the mount-index handle directly
 *     (`getRawMountIndexDatabase()`), NOT `rawQuery` — `rawQuery` targets the
 *     MAIN backend, so it would read the wrong (empty) database. The handle is a
 *     better-sqlite3 connection, so `pragma('table_info')` gives schema column
 *     order and `prepare('SELECT *').all()` gives the persisted rows.
 *
 * NORMALIZATION SPEC: none. Every id and timestamp is pinned on both sides, so
 * the dumps must match outright — no id remap, no timestamp placeholder.
 *
 * P4.D274 (v4 `ed8b15b50`, bug 186) adds a SECOND line, `doc-mount-points-
 * name-gate`: after the committed ops (whose line is unchanged), the steps of
 * `fixtures/doc-mount-points-name-gate.json` run through the same REAL
 * repository on the same working copy — the name chokepoint (`create` /
 * `update` → `assertNameFree`, `findNameHolder` over the Zod-dropping
 * `findAll`), mirroring v4's `doc-mount-points-find-by-name.test.ts:70-110`.
 * Per step: the thrown message (or null), the captured lines (the gate's WARN
 * and the drop lines, through a spliced logger transport — nothing reaches
 * stdout), and every `(id, name)` by id.
 *
 * Run from the v4 server checkout under Node 24, AFTER building the fixture:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_DOC_MOUNT_POINTS=/tmp/qt-dmp-fixture.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-points-tier2.ts \
 *     > /tmp/oracle-dmp.ndjson
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  copyFileSync,
  existsSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { canonicalizeRows } from '../lib/tier2.js';

interface Op {
  kind: 'create' | 'update' | 'delete';
  id?: string;
  data?: Record<string, unknown>;
  options?: { id: string; createdAt: string; updatedAt: string };
}

interface Spec {
  testPepperBase64: string;
  ops: Op[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, '..', 'fixtures', 'doc-mount-points-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const fixture = process.env.QT_FIXTURE_DOC_MOUNT_POINTS;
  if (!fixture || !existsSync(fixture)) {
    throw new Error(
      'QT_FIXTURE_DOC_MOUNT_POINTS must point at the seed fixture from build-doc-mount-points-fixture.ts'
    );
  }

  // Work on a fresh copy of the mount-index fixture so the shared seed stays pristine.
  const scratch = mkdtempSync(join(tmpdir(), 'qt-dmp-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  const work = join(scratch, 'dmp-mount-index-work.db');
  copyFileSync(fixture, work);

  // Env MUST be set before importing v4 config/manager modules.
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = join(scratch, 'data', 'main.db'); // fresh throwaway main DB
  process.env.SQLITE_MOUNT_INDEX_PATH = work; // the working copy we mutate + read
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE; // journal_mode = TRUNCATE on both DBs
  process.env.LOG_LEVEL = 'warn'; // the gate's WARN; the transport splice keeps stdout clean

  const { logger } = await import('@/lib/logger');
  const CAPTURED = new Set([
    'Refused a document-store name another store holds',
    'Data validation failed',
    'Safe validation failed',
  ]);
  let captured: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
      if (!CAPTURED.has(data.message)) return;
      const { service: _s, environment: _e, ...fields } = data.context ?? {};
      if (data.error) fields.error = data.error.message;
      captured.push({ level: data.level, message: data.message, fields });
    },
  });

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRawMountIndexDatabase, closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { DocMountPointsRepository } = await import(
    '@/lib/database/repositories/doc-mount-points.repository'
  );

  await initializeDatabase();
  const repo = new DocMountPointsRepository();

  for (const op of spec.ops) {
    if (op.kind === 'create') {
      await repo.create(op.data as never, op.options);
    } else if (op.kind === 'update') {
      await repo.update(op.id as string, op.data as never);
    } else {
      await repo.delete(op.id as string);
    }
  }

  // Read RAW on-disk state through the MOUNT-INDEX handle directly (rawQuery
  // targets the MAIN backend, which would be the wrong DB). table_info gives
  // schema column order; SELECT * gives the persisted rows.
  const midb = getRawMountIndexDatabase();
  if (!midb) {
    throw new Error('mount-index DB handle unavailable (degraded open?)');
  }
  const columns = (
    midb.pragma('table_info(doc_mount_points)') as Array<{ name: string }>
  ).map((c) => c.name);
  const rawRows = midb
    .prepare('SELECT * FROM doc_mount_points')
    .all() as Array<Record<string, unknown>>;

  // P4.D274: the name chokepoint, on the same working copy.
  const gate = JSON.parse(
    readFileSync(join(here, '..', 'fixtures', 'doc-mount-points-name-gate.json'), 'utf8'),
  ) as {
    stamp: string;
    base: Record<string, unknown>;
    steps: Array<{ label: string; kind: 'create' | 'update' | 'plantRaw'; id: string; name?: string }>;
  };
  const steps: unknown[] = [];
  for (const step of gate.steps) {
    captured = [];
    let error: string | null = null;
    try {
      if (step.kind === 'create') {
        await repo.create({ ...gate.base, name: step.name } as never, {
          id: step.id,
          createdAt: gate.stamp,
          updatedAt: gate.stamp,
        });
      } else if (step.kind === 'update') {
        const data: Record<string, unknown> = { updatedAt: gate.stamp };
        if (step.name !== undefined) data.name = step.name;
        else data.scanStatus = 'idle';
        await repo.update(step.id, data as never);
      } else {
        midb
          .prepare(
            `INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, includePatterns,
               excludePatterns, enabled, scanStatus, conversionStatus, fileCount, chunkCount,
               totalSizeBytes, createdAt, updatedAt)
             VALUES (?, ?, '', 'database', 'documents', '[]', '[]', 1, 'idle', 'idle', 0, 0, 0, ?, ?)`,
          )
          .run(step.id, step.name, gate.stamp, gate.stamp);
      }
    } catch (e) {
      error = (e as Error).message;
    }
    const names = midb.prepare('SELECT id, name FROM doc_mount_points ORDER BY id').all();
    steps.push({ label: step.label, error, lines: captured, names });
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  const dump = canonicalizeRows({
    table: 'doc_mount_points',
    columns,
    rawRows,
    orderBy: 'id',
  });

  process.stdout.write(
    JSON.stringify({ case: 'doc-mount-points-tier2', ...dump }) + '\n'
  );
  process.stdout.write(JSON.stringify({ case: 'doc-mount-points-name-gate', steps }) + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`doc-mount-points-tier2 oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
