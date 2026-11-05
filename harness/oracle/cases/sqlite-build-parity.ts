/**
 * Oracle case: SQLite build parity (P4.D365, v4 `8832cfd20`).
 *
 * Drives v4's REAL database driver — `better-sqlite3`, the npm alias of
 * `better-sqlite3-multiple-ciphers` (13.0.3 at v4 `624d0d25c`: SQLite3
 * Multiple Ciphers 2.4.0 on SQLite 3.53.4), resolved from the CWD's
 * node_modules, so run it with cwd = the v4 checkout (the round's pin).
 *
 * Two modes:
 *
 *   (default)       npx tsx sqlite-build-parity.ts
 *     Emits a probe row (the driver's package version, Node's version) and
 *     the build facts v5's linked SQLite must match: `sqlite_version()`,
 *     `sqlite_source_id()`, `sqlite3mc_version()`, the whole
 *     `PRAGMA compile_options` list in SQLite's order, and `PRAGMA cipher` on
 *     a fresh keyed file. Then the v4 -> v5 half of the cipher proof: it
 *     CREATES `/tmp/p4d365/v4-written.db` keyed with the harness test pepper
 *     through the raw-hex form (`key = "x'<hex>'"`, no `cipher=` pragma,
 *     `journal_mode = TRUNCATE` — v4's own open sequence) and writes a TEXT,
 *     a REAL and a BLOB row plus a filler row big enough to need more pages;
 *     the Rust side reads them back. Then the planted refusals (Tier-2 item
 *     7): a WRONG pepper on that file, and a TAMPERED copy
 *     (`/tmp/p4d365/v4-written-tampered.db`, 16 bytes flipped inside page 2,
 *     self-checked) read through `quick_check` and a full scan.
 *
 *   --read <path>   npx tsx sqlite-build-parity.ts --read /tmp/p4d365/v5-written.db
 *     The v5 -> v4 half: opens a file v5's WRITABLE path wrote, reads its rows
 *     back, runs `PRAGMA quick_check`, reports `PRAGMA cipher`, and tries the
 *     same file with a wrong pepper.
 *
 * Every row is `{ kind, ... }` NDJSON on stdout. Recipe: the header of
 * `crates/quilltap-harness/tests/sqlite_build_parity_equivalence.rs`.
 */

import { createRequire } from 'node:module';
import * as fs from 'node:fs';

const requireFromV4 = createRequire(process.cwd() + '/');
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const Database: any = requireFromV4('better-sqlite3');
const driverVersion: string = requireFromV4('better-sqlite3/package.json').version;

/** The harness corpus test pepper (synthetic; never a real instance's). */
const TEST_PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
/** A different, equally synthetic pepper — the wrong-key plant. */
const WRONG_PEPPER = 'd3Jvbmd3cm9uZ3dyb25nd3Jvbmd3cm9uZ3dyb25nMDI=';

const DIR = '/tmp/p4d365';
const V4_WRITTEN = `${DIR}/v4-written.db`;
const V4_TAMPERED = `${DIR}/v4-written-tampered.db`;

function emit(row: Record<string, unknown>): void {
  process.stdout.write(JSON.stringify(row) + '\n');
}

function keyHex(pepperB64: string): string {
  return Buffer.from(pepperB64, 'base64').toString('hex');
}

/** v4's key pragma — the raw-hex form, first and only before a read. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function applyKey(db: any, pepperB64: string): void {
  db.pragma(`key = "x'${keyHex(pepperB64)}'"`);
}

function errorOf(e: unknown): { code: string; message: string } {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const err = e as any;
  return { code: String(err?.code ?? 'error'), message: String(err?.message ?? err) };
}

/** Read the parity table back as plain JSON (BLOBs as lowercase hex). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function readRows(db: any): unknown[] {
  return db
    .prepare('SELECT id, t, r, b, typeof(t) AS tt, typeof(r) AS tr, typeof(b) AS tb FROM parity ORDER BY id')
    .all()
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    .map((row: any) => ({
      id: row.id,
      t: row.t,
      r: row.r,
      b: row.b == null ? null : Buffer.from(row.b).toString('hex'),
      tt: row.tt,
      tr: row.tr,
      tb: row.tb,
    }));
}

/** A read-only open with `pepper`; the first statement's outcome. */
function tryRead(path: string, pepperB64: string): Record<string, unknown> {
  let db;
  try {
    db = new Database(path, { readonly: true, fileMustExist: true });
    applyKey(db, pepperB64);
    const n = db.prepare('SELECT count(*) AS n FROM sqlite_master').get().n;
    return { ok: true, sqliteMasterCount: n };
  } catch (e) {
    return { ok: false, ...errorOf(e) };
  } finally {
    try {
      db?.close();
    } catch {
      /* closing a failed handle is not the measurement */
    }
  }
}

function buildFacts(): void {
  const mem = new Database(':memory:');
  emit({ kind: 'row', id: 'sqliteVersion', value: mem.prepare('select sqlite_version() AS v').get().v });
  emit({ kind: 'row', id: 'sourceId', value: mem.prepare('select sqlite_source_id() AS v').get().v });
  emit({ kind: 'row', id: 'sqlite3mcVersion', value: mem.prepare('select sqlite3mc_version() AS v').get().v });
  emit({
    kind: 'row',
    id: 'compileOptions',
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    value: mem.pragma('compile_options').map((r: any) => r.compile_options),
  });
  mem.close();

  const fresh = `${DIR}/v4-cipher-probe.db`;
  fs.rmSync(fresh, { force: true });
  const db = new Database(fresh);
  applyKey(db, TEST_PEPPER);
  db.exec('CREATE TABLE t (x)');
  emit({ kind: 'row', id: 'cipher', value: db.pragma('cipher', { simple: true }) });
  db.close();
  fs.rmSync(fresh, { force: true });
}

function writeV4File(): void {
  for (const p of [V4_WRITTEN, `${V4_WRITTEN}-journal`]) fs.rmSync(p, { force: true });
  const db = new Database(V4_WRITTEN);
  applyKey(db, TEST_PEPPER);
  db.pragma('journal_mode = TRUNCATE');
  db.exec('CREATE TABLE parity (id INTEGER PRIMARY KEY, t TEXT, r REAL, b BLOB)');
  const ins = db.prepare('INSERT INTO parity (id, t, r, b) VALUES (?, ?, ?, ?)');
  ins.run(1, 'Quilltap — the quill taps; café \u{1F58B}', null, null);
  ins.run(2, null, 3.141592653589793, null);
  ins.run(3, null, null, Buffer.from([0x00, 0x01, 0x7f, 0x80, 0xfe, 0xff]));
  // A filler row larger than one 4096-byte page, so page 2 onward holds data
  // the tampered plant can hit.
  ins.run(4, 'x'.repeat(9000), -0.5, Buffer.alloc(3000, 0xab));
  emit({ kind: 'row', id: 'v4Written', path: V4_WRITTEN, rows: readRows(db) });
  emit({ kind: 'row', id: 'v4WrittenCipher', value: db.pragma('cipher', { simple: true }) });
  db.close();

  emit({ kind: 'row', id: 'v4WrittenWrongKey', path: V4_WRITTEN, ...tryRead(V4_WRITTEN, WRONG_PEPPER) });

  // The tampered copy: flip 16 bytes in the middle of page 2. Self-check the
  // plant (a plant that fails to plant is a vacuous pass): the bytes differ.
  fs.copyFileSync(V4_WRITTEN, V4_TAMPERED);
  const buf = fs.readFileSync(V4_TAMPERED);
  if (buf.length < 3 * 4096) throw new Error(`v4-written.db too small to tamper: ${buf.length}`);
  const at = 4096 + 2048;
  const before = Buffer.from(buf.subarray(at, at + 16));
  for (let i = 0; i < 16; i += 1) buf[at + i] ^= 0x5a;
  fs.writeFileSync(V4_TAMPERED, buf);
  const after = fs.readFileSync(V4_TAMPERED).subarray(at, at + 16);
  if (before.equals(after)) throw new Error('tamper plant did not change page 2');

  let db2;
  const tampered: Record<string, unknown> = { kind: 'row', id: 'v4Tampered', path: V4_TAMPERED, offset: at };
  try {
    db2 = new Database(V4_TAMPERED, { readonly: true, fileMustExist: true });
    applyKey(db2, TEST_PEPPER);
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      tampered.quickCheck = db2.pragma('quick_check').map((r: any) => r.quick_check);
    } catch (e) {
      tampered.quickCheckError = errorOf(e);
    }
    try {
      tampered.scan = db2.prepare('SELECT count(*) AS n, sum(length(t)) AS l FROM parity').get();
    } catch (e) {
      tampered.scanError = errorOf(e);
    }
  } finally {
    db2?.close();
  }
  emit(tampered);
}

function readBack(path: string): void {
  const db = new Database(path, { readonly: true, fileMustExist: true });
  applyKey(db, TEST_PEPPER);
  emit({ kind: 'row', id: 'readbackRows', path, rows: readRows(db) });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  emit({ kind: 'row', id: 'readbackQuickCheck', value: db.pragma('quick_check').map((r: any) => r.quick_check) });
  emit({ kind: 'row', id: 'readbackCipher', value: db.pragma('cipher', { simple: true }) });
  db.close();
  emit({ kind: 'row', id: 'readbackWrongKey', path, ...tryRead(path, WRONG_PEPPER) });
}

fs.mkdirSync(DIR, { recursive: true });
emit({ kind: 'probe', driver: driverVersion, node: process.version });
const readIdx = process.argv.indexOf('--read');
if (readIdx >= 0) {
  const p = process.argv[readIdx + 1];
  if (!p) throw new Error('--read needs a path');
  readBack(p);
} else {
  buildFacts();
  writeV4File();
}
