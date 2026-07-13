import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { makeDbKeyFile } from './dbkey';
import { ARTIFACTS_DIR, E2E_PASSPHRASE, FIXTURES_DIR, INSTANCE_DIR, TEST_PEPPER } from './env';

/**
 * The P4.6ab/P4.6ac unification wire: seed the courier + chat-images beats'
 * fixture chats into the SHARED e2e instance, copied row-for-row from lane A's
 * committed `courier-images-{main,mount}.db` (the differential fixture — the
 * beats in `salon-courier-images-flow.spec.ts` discover these chats by content
 * and were probe-guarded until this data existed on the shared server).
 *
 * Mechanics: a throwaway READ instance is assembled over the committed courier
 * fixture (both families share TEST_PEPPER and the same fixture userId, so the
 * global-setup's existing userId rewrite covers the copied rows — call this
 * BEFORE that rewrite loop); rows are read via the CLI's `--json` raw SQL and
 * re-inserted into the shared instance with `--write`, intersected against the
 * destination's columns (the salon fixture may trail the courier fixture by
 * ALTER-materialized columns). The ingested image's mount-blob BYTES come from
 * the committed `.meta.json` sidecar (the CLI's JSON surface has no blob
 * literal), inserted as an `X'…'` hex literal so the thumbnail/lightbox serve
 * real PNG bytes. Data seeding for the walk, NOT a fixture regen — the
 * committed fixtures are never mutated.
 */

// Pinned ids from `harness/oracle/fixtures/build-courier-images-web-fixture.ts`.
const CHAT_PENDING = 'c1000000-0000-4000-8000-000000000001';
const CHAT_IMG = 'c1000000-0000-4000-8000-000000000002';
const LORA = 'a1000000-0000-4000-8000-000000000001';
const RIYA = 'a1000000-0000-4000-8000-000000000002';
const PROJ = 'a3000000-0000-4000-8000-000000000001';

interface CourierMeta {
  img1FileId: string;
  img1Bytes: number[];
}

type Row = Record<string, unknown>;

export function seedCourierImagesFixture(cli: string): void {
  // 1. The throwaway read instance over the committed courier fixture.
  const src = resolve(ARTIFACTS_DIR, 'courier-src');
  const srcData = resolve(src, 'data');
  rmSync(src, { recursive: true, force: true });
  mkdirSync(srcData, { recursive: true });
  copyFileSync(resolve(FIXTURES_DIR, 'courier-images-main.db'), resolve(srcData, 'quilltap.db'));
  copyFileSync(
    resolve(FIXTURES_DIR, 'courier-images-mount.db'),
    resolve(srcData, 'quilltap-mount-index.db'),
  );
  writeFileSync(resolve(srcData, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

  // 2. The rows the two beats render (single-user rows only — the fixture's
  //    OTHER_USER ownership arms stay behind).
  const chats = readRows(
    cli,
    src,
    `SELECT * FROM chats WHERE id IN ('${CHAT_PENDING}','${CHAT_IMG}')`,
  );
  const characters = readRows(
    cli,
    src,
    `SELECT * FROM characters WHERE id IN ('${LORA}','${RIYA}')`,
  );
  const projects = readRows(cli, src, `SELECT * FROM projects WHERE id = '${PROJ}'`);
  const files = readRows(
    cli,
    src,
    `SELECT * FROM files WHERE linkedTo LIKE '%${CHAT_PENDING}%' OR linkedTo LIKE '%${CHAT_IMG}%'`,
  );

  // 3. Insert into the shared instance.
  insertRows(cli, INSTANCE_DIR, 'chats', chats, src);
  insertRows(cli, INSTANCE_DIR, 'characters', characters, src);
  insertRows(cli, INSTANCE_DIR, 'projects', projects, src);
  insertRows(cli, INSTANCE_DIR, 'files', files, src);

  // 4. The ingested image's mount blob — the row from the courier mount db, the
  //    bytes from the committed meta sidecar (X'hex' literal).
  const meta = JSON.parse(
    readFileSync(resolve(FIXTURES_DIR, 'courier-images-main.db.meta.json'), 'utf8'),
  ) as CourierMeta;
  const imgRow = files.find((f) => f['id'] === meta.img1FileId);
  const storageKey = String(imgRow?.['storageKey'] ?? '');
  const blobId = storageKey.startsWith('mount-blob:') ? storageKey.split(':')[2] : undefined;
  if (blobId) {
    const blobRows = readRows(
      cli,
      src,
      `SELECT id, fileId, sha256, sizeBytes, storedMimeType, createdAt, updatedAt FROM doc_mount_blobs WHERE id = '${blobId}'`,
      { mount: true },
    );
    if (blobRows.length === 1) {
      const b = blobRows[0];
      const hex = Buffer.from(meta.img1Bytes).toString('hex');
      ensureDestTable(cli, INSTANCE_DIR, 'doc_mount_blobs', src, { mount: true });
      runWrite(
        cli,
        INSTANCE_DIR,
        `INSERT OR REPLACE INTO doc_mount_blobs (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) VALUES (${lit(b['id'])}, ${lit(b['fileId'])}, ${lit(b['sha256'])}, ${lit(b['sizeBytes'])}, ${lit(b['storedMimeType'])}, X'${hex}', ${lit(b['createdAt'])}, ${lit(b['updatedAt'])});`,
        { mount: true },
      );
    }
  }

  rmSync(src, { recursive: true, force: true });
}

/** One CLI `--json` raw-SQL read against `dir` (the instance root). */
function readRows(cli: string, dir: string, sql: string, opts: { mount?: boolean } = {}): Row[] {
  const args = ['db', '--data-dir', dir, ...(opts.mount ? ['--mount-points'] : []), '--json', sql];
  const res = spawnSync(cli, args, {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (res.status !== 0) {
    throw new Error(`courier-fixture read failed (${sql}):\n${res.stdout}\n${res.stderr}`);
  }
  return JSON.parse(res.stdout) as Row[];
}

function runWrite(cli: string, dir: string, sql: string, opts: { mount?: boolean } = {}): void {
  const args = ['db', '--data-dir', dir, ...(opts.mount ? ['--mount-points'] : []), '--write', sql];
  const res = spawnSync(cli, args, {
    env: { ...process.env, QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (res.status !== 0) {
    throw new Error(
      `courier-fixture write failed (${sql.slice(0, 120)}…):\n${res.stdout}\n${res.stderr}`,
    );
  }
}

/** A SQL literal for a CLI-JSON value (TEXT/NUMBER/NULL — blobs never ride this path). */
function lit(v: unknown): string {
  if (v === null || v === undefined) {
    return 'NULL';
  }
  if (typeof v === 'number') {
    return Number.isFinite(v) ? String(v) : 'NULL';
  }
  return `'${String(v).replace(/'/g, "''")}'`;
}

/** Destination column names (empty when the table is absent). */
function destColumns(
  cli: string,
  dir: string,
  table: string,
  opts: { mount?: boolean } = {},
): string[] {
  try {
    return readRows(cli, dir, `PRAGMA table_info(${table})`, opts).map((r) => String(r['name']));
  } catch {
    return [];
  }
}

/** Materialize an absent destination table from the SOURCE fixture's own DDL. */
function ensureDestTable(
  cli: string,
  destDir: string,
  table: string,
  srcDir: string,
  opts: { mount?: boolean } = {},
): void {
  if (destColumns(cli, destDir, table, opts).length > 0) {
    return;
  }
  const ddl = readRows(
    cli,
    srcDir,
    `SELECT sql FROM sqlite_master WHERE type = 'table' AND name = '${table}'`,
    opts,
  );
  const sql = String(ddl[0]?.['sql'] ?? '');
  if (!sql) {
    throw new Error(`courier-fixture: no source DDL for ${table}`);
  }
  runWrite(cli, destDir, `${sql};`, opts);
}

/** Insert `rows` into the destination `table`, intersected with its columns. */
function insertRows(
  cli: string,
  destDir: string,
  table: string,
  rows: Row[],
  srcDir: string,
): void {
  if (rows.length === 0) {
    return;
  }
  ensureDestTable(cli, destDir, table, srcDir);
  const dest = new Set(destColumns(cli, destDir, table));
  for (const row of rows) {
    const cols = Object.keys(row).filter((c) => dest.has(c));
    const sql = `INSERT OR REPLACE INTO ${table} (${cols.map((c) => `"${c}"`).join(', ')}) VALUES (${cols.map((c) => lit(row[c])).join(', ')});`;
    runWrite(cli, destDir, sql);
  }
}
