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
 * ALTER-materialized columns). Two hard-won rules:
 *
 *  - The MOUNT-side substrate must ride along: the copied characters carry
 *    provisioned vaults (`characterDocumentMountPointId`) and the
 *    document-store overlay FAILS HARD on a flagged character whose vault
 *    mount is absent — which 500s every `listChats` and takes the whole walk
 *    down. The courier mount db holds ONLY this fixture's rows (fresh UUIDs),
 *    so the six text tables copy whole; `doc_mount_chunks` (embeddings) is
 *    deliberately skipped.
 *  - The CLI runs ONE statement per invocation and unwraps the .dbkey
 *    (PBKDF2) each time, so inserts batch as ONE multi-row INSERT per table.
 *  - The two fixture families PIN ids from the same scheme (the salon
 *    fixture's "Solo Voyage" IS `c1000000-…01` — the courier fixture's
 *    pending chat), so a verbatim copy CLOBBERS salon rows via
 *    INSERT OR REPLACE (found the hard way: six specs went down). Every
 *    pinned courier id is therefore remapped to an e2e-only twin
 *    (`XY000000…` → `XYe2e000…`) as a plain string substitution over every
 *    copied value — which also rewrites the JSON columns (participants,
 *    linkedTo, activeTyping). v4-minted ids (vault mounts, the ingested
 *    file, the blob) pass through — different mint runs cannot collide.
 *
 * The ingested image's mount-blob BYTES come from the committed `.meta.json`
 * sidecar (the CLI's JSON surface has no blob literal), inserted as an `X'…'`
 * hex literal so the thumbnail/lightbox serve real PNG bytes. Data seeding for
 * the walk, NOT a fixture regen — the committed fixtures are never mutated.
 */

// Pinned ids from `harness/oracle/fixtures/build-courier-images-web-fixture.ts`.
const CHAT_PENDING = 'c1000000-0000-4000-8000-000000000001';
const CHAT_IMG = 'c1000000-0000-4000-8000-000000000002';
const LORA = 'a1000000-0000-4000-8000-000000000001';
const RIYA = 'a1000000-0000-4000-8000-000000000002';
const PROJ = 'a3000000-0000-4000-8000-000000000001';

/** `XY000000-…` → `XYe2e000-…`: the e2e-only twin of a pinned fixture id. */
function e2eTwin(id: string): string {
  return id.slice(0, 2) + 'e2e000' + id.slice(8);
}

/**
 * Every pinned id the courier fixture bakes (the builder's constant block) —
 * chats, characters, project, mounts, participants, messages, files, the
 * connection profile — remapped wholesale so nothing collides with the salon
 * fixture's identically-pinned rows.
 */
const PINNED_IDS = [
  'c0000001-0000-4000-8000-000000000001', // CONN (referenced from participants JSON)
  'a0000001-0000-4000-8000-000000000001', // APIKEY
  LORA,
  RIYA,
  PROJ,
  'b0000000-0000-4000-8000-000000000001', // PROJ_EXTRA_MP
  'b0000000-0000-4000-8000-0000000000a0', // GENERAL_MP
  'b0000000-0000-4000-8000-0000000000ff', // UPLOADS_MP
  CHAT_PENDING,
  CHAT_IMG,
  'e1000000-0000-4000-8000-000000000001', // PP_LORA
  'e1000000-0000-4000-8000-000000000002', // PP_USER
  'd1000000-0000-4000-8000-000000000001', // MSG_PENDING
  'd1000000-0000-4000-8000-000000000002', // MSG_SETTLED
  'f2000000-0000-4000-8000-000000000001', // ATT_FILE
  'e2000000-0000-4000-8000-000000000001', // PI_LORA
  'e2000000-0000-4000-8000-000000000002', // PI_RIYA
  'd2000000-0000-4000-8000-000000000001', // MSG_IMG
  'f1000000-0000-4000-8000-000000000001', // CF_NOTES
  'f1000000-0000-4000-8000-000000000002', // CF_DUP
  'f1000000-0000-4000-8000-000000000003', // CF_GEN
];

/** Rewrite every pinned id inside one copied value (covers the JSON columns). */
function remapPinned(v: unknown): unknown {
  if (typeof v !== 'string') {
    return v;
  }
  let out = v;
  for (const id of PINNED_IDS) {
    out = out.split(id).join(e2eTwin(id));
  }
  return out;
}

const MOUNT_TABLES = [
  'doc_mount_points',
  'doc_mount_folders',
  'doc_mount_files',
  'doc_mount_documents',
  'doc_mount_file_links',
  'project_doc_mount_links',
];

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

  // 2. The MAIN-side rows the two beats render (single-user rows only — the
  //    fixture's OTHER_USER ownership arms stay behind).
  const main: Array<[string, Row[]]> = [
    ['chats', readRows(cli, src, `SELECT * FROM chats WHERE id IN ('${CHAT_PENDING}','${CHAT_IMG}')`)],
    ['characters', readRows(cli, src, `SELECT * FROM characters WHERE id IN ('${LORA}','${RIYA}')`)],
    ['projects', readRows(cli, src, `SELECT * FROM projects WHERE id = '${PROJ}'`)],
    [
      'files',
      readRows(
        cli,
        src,
        `SELECT * FROM files WHERE linkedTo LIKE '%${CHAT_PENDING}%' OR linkedTo LIKE '%${CHAT_IMG}%'`,
      ),
    ],
    [
      'chat_messages',
      readRows(cli, src, `SELECT * FROM chat_messages WHERE chatId IN ('${CHAT_PENDING}','${CHAT_IMG}')`),
    ],
  ];
  insertTables(cli, src, main, { mount: false });

  // 3. The MOUNT-side substrate (vaults + stores — see the module header).
  const mount: Array<[string, Row[]]> = MOUNT_TABLES.map((t) => [
    t,
    readRows(cli, src, `SELECT * FROM ${t}`, { mount: true }),
  ]);
  insertTables(cli, src, mount, { mount: true });

  // 4. The ingested image's mount blob — the row from the courier mount db, the
  //    bytes from the committed meta sidecar (X'hex' literal).
  const meta = JSON.parse(
    readFileSync(resolve(FIXTURES_DIR, 'courier-images-main.db.meta.json'), 'utf8'),
  ) as CourierMeta;
  const filesRows = main.find(([t]) => t === 'files')?.[1] ?? [];
  const imgRow = filesRows.find((f) => f['id'] === meta.img1FileId);
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
      ensureDestTables(cli, src, ['doc_mount_blobs'], { mount: true });
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

/** Column names per table on ONE destination db, in a single CLI read. */
function destColumnsBulk(
  cli: string,
  tables: string[],
  opts: { mount?: boolean },
): Map<string, Set<string>> {
  const sql = tables
    .map((t) => `SELECT '${t}' AS t, name FROM pragma_table_info('${t}')`)
    .join(' UNION ALL ');
  const rows = readRows(cli, INSTANCE_DIR, sql, opts);
  const map = new Map<string, Set<string>>();
  for (const r of rows) {
    const t = String(r['t']);
    if (!map.has(t)) {
      map.set(t, new Set());
    }
    map.get(t)!.add(String(r['name']));
  }
  return map;
}

/** Materialize absent destination tables from the SOURCE fixture's own DDL. */
function ensureDestTables(
  cli: string,
  srcDir: string,
  tables: string[],
  opts: { mount?: boolean },
): Map<string, Set<string>> {
  let cols = destColumnsBulk(cli, tables, opts);
  const missing = tables.filter((t) => !cols.get(t)?.size);
  for (const table of missing) {
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
    runWrite(cli, INSTANCE_DIR, `${sql};`, opts);
  }
  if (missing.length > 0) {
    cols = destColumnsBulk(cli, tables, opts);
  }
  return cols;
}

/** One multi-row INSERT per table, columns intersected with the destination. */
function insertTables(
  cli: string,
  srcDir: string,
  tables: Array<[string, Row[]]>,
  opts: { mount?: boolean },
): void {
  const nonEmpty = tables.filter(([, rows]) => rows.length > 0);
  if (nonEmpty.length === 0) {
    return;
  }
  const cols = ensureDestTables(
    cli,
    srcDir,
    nonEmpty.map(([t]) => t),
    opts,
  );
  for (const [table, rows] of nonEmpty) {
    const dest = cols.get(table);
    if (!dest?.size) {
      throw new Error(`courier-fixture: destination table ${table} has no columns`);
    }
    // CLI-JSON rows carry every column (nulls included), so one shared column
    // list covers all rows of a table. Every value passes through the pinned-id
    // remap (module header) on the way in.
    const shared = Object.keys(rows[0]).filter((c) => dest.has(c));
    const values = rows
      .map((row) => `(${shared.map((c) => lit(remapPinned(row[c]))).join(', ')})`)
      .join(',\n');
    runWrite(
      cli,
      INSTANCE_DIR,
      `INSERT OR REPLACE INTO ${table} (${shared.map((c) => `"${c}"`).join(', ')}) VALUES ${values};`,
      opts,
    );
  }
}
