/**
 * P4.D341 — the small toolkit the round's four NEW ensure cases share
 * (`cases/memory-keystone-ensure.ts`, `storyteller-mode-ensure.ts`,
 * `llm-logs-storyteller-round-id-ensure.ts`, `roleplay-templates-unmarked-
 * ensure.ts`): the test pepper, base-file hygiene, the exactly-once line strip,
 * a NOT-NULL-filling row plant, and the per-table dumps every family compares
 * (`PRAGMA table_info`, `sqlite_master.sql`, `quote()`d / `typeof` cells, the
 * `migrations_state` ids). Run with cwd = the v4 pin (better-sqlite3 is
 * required from it — v4's sqleet/ChaCha20 build).
 */

import { existsSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';

export const PEPPER = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
export const KEY_PRAGMA = `key = "x'${Buffer.from(PEPPER, 'base64').toString('hex')}'"`;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Db = any;

export function openDb(path: string, readonly = false): Db {
  const Database = createRequire(process.cwd() + '/')('better-sqlite3');
  const db = new Database(path, readonly ? { readonly: true } : undefined);
  db.pragma(KEY_PRAGMA);
  return db;
}

/** Remove a file and its journal / WAL residue. */
export function sweep(path: string): void {
  for (const suffix of ['', '-journal', '-wal', '-shm']) if (existsSync(path + suffix)) rmSync(path + suffix);
}

/** Remove each line, asserting it is present EXACTLY once first. */
export function strip(ddl: string, lines: string[]): string {
  let out = ddl;
  for (const line of lines) {
    if (out.split(line).length !== 2) throw new Error(`expected exactly one ${JSON.stringify(line)} in:\n${ddl}`);
    out = out.replace(line, '');
  }
  return out;
}

/** Insert a row: every NOT NULL column without a default gets a placeholder
 *  (`''` for TEXT, `0` otherwise), then `values` override. */
export function insertRow(db: Db, table: string, values: Record<string, unknown>): void {
  const info = db.prepare(`PRAGMA table_info("${table}")`).all() as Array<{
    name: string;
    type: string;
    notnull: number;
    dflt_value: string | null;
    pk: number;
  }>;
  const row: Record<string, unknown> = {};
  for (const c of info) {
    if (c.notnull && c.dflt_value === null && !c.pk) row[c.name] = /TEXT/i.test(c.type) ? '' : 0;
  }
  Object.assign(row, values);
  const cols = Object.keys(row);
  db.prepare(
    `INSERT INTO "${table}" (${cols.map((c) => `"${c}"`).join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`,
  ).run(...cols.map((c) => row[c]));
}

export function tableSql(db: Db, name: string): string | null {
  const r = db.prepare(`SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?`).get(name) as
    | { sql: string }
    | undefined;
  return r ? r.sql : null;
}

export function indexSql(db: Db, name: string): string | null {
  const r = db.prepare(`SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?`).get(name) as
    | { sql: string }
    | undefined;
  return r ? r.sql : null;
}

export function tableInfo(db: Db, table: string): unknown[] {
  return tableSql(db, table) ? db.prepare(`PRAGMA table_info("${table}")`).all() : [];
}

/** `id` plus each named column as `quote()` / `typeof()` (`'absent'` when the
 *  column is not there), ordered by id. `[]` when the table is absent. */
export function cells(db: Db, table: string, columns: string[]): unknown[] {
  if (!tableSql(db, table)) return [];
  const names = new Set((db.prepare(`PRAGMA table_info("${table}")`).all() as Array<{ name: string }>).map((c) => c.name));
  const parts = ['id'];
  for (const c of columns) {
    parts.push(names.has(c) ? `quote("${c}") AS "${c}"` : `'absent' AS "${c}"`);
    parts.push(names.has(c) ? `typeof("${c}") AS "${c}Type"` : `'absent' AS "${c}Type"`);
  }
  return db.prepare(`SELECT ${parts.join(', ')} FROM "${table}" ORDER BY id`).all();
}

export function ledger(db: Db): string[] {
  if (!tableSql(db, 'migrations_state')) return [];
  return (db.prepare('SELECT id FROM migrations_state ORDER BY id').all() as Array<{ id: string }>).map((r) => r.id);
}

/** Read a file's header journal mode (lower-case, as SQLite answers it). */
export function journalMode(path: string): string {
  const db = openDb(path, true);
  try {
    return String(db.pragma('journal_mode', { simple: true }));
  } finally {
    db.close();
  }
}
