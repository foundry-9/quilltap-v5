/**
 * Shared tier-2 (DB-state) oracle helpers — pure canonicalization only.
 *
 * Tier-1 compares pure-function outputs; tier-2 compares *database state* after
 * a repo/service op. This module holds the repo-agnostic canonical-dump shaping:
 * given the raw rows + column order read from the finished DB, it produces the
 * structural snapshot both the v4 oracle and the Rust port must emit identically.
 *
 *   - columns in on-disk (CREATE TABLE) order;
 *   - rows sorted by a stable key (default `id`, code-unit string order to match
 *     a deterministic byte sort);
 *   - BLOBs as lowercase hex, nulls explicit, no float reformatting
 *     (deterministic Float32 BLOBs compare bit-exact via hex).
 *
 * The actual SELECT / PRAGMA read is done by the case script through v4's own
 * connected backend (`rawQuery`), so no npm package is imported from the v5
 * tree (where there is no node_modules). This file is dependency-free on purpose.
 */

/** One canonicalized table snapshot. */
export interface TableDump {
  table: string;
  /** Column names in on-disk (CREATE TABLE) order. */
  columns: string[];
  /** Rows as column->value maps, sorted by `orderBy`. */
  rows: Array<Record<string, unknown>>;
}

/**
 * Canonicalize a single value for the dump:
 *   - Buffer / typed array (BLOB) -> lowercase hex string
 *   - null / undefined            -> null (explicit)
 *   - everything else             -> as-is (strings, numbers)
 */
function canonValue(v: unknown): unknown {
  if (v === null || v === undefined) return null;
  if (typeof Buffer !== 'undefined' && Buffer.isBuffer(v)) {
    return v.toString('hex');
  }
  if (v instanceof Uint8Array) return Buffer.from(v).toString('hex');
  return v;
}

/**
 * Shape raw rows (column->value maps, e.g. from `SELECT *`) plus a column order
 * (e.g. from `PRAGMA table_info`) into a canonical {@link TableDump}.
 */
export function canonicalizeRows(opts: {
  table: string;
  columns: string[];
  rawRows: Array<Record<string, unknown>>;
  orderBy?: string;
}): TableDump {
  const { table, columns, rawRows, orderBy = 'id' } = opts;

  const rows = rawRows
    .map((r) => {
      const out: Record<string, unknown> = {};
      for (const col of columns) out[col] = canonValue(r[col]);
      return out;
    })
    .sort((a, b) => {
      const av = String(a[orderBy] ?? '');
      const bv = String(b[orderBy] ?? '');
      return av < bv ? -1 : av > bv ? 1 : 0;
    });

  return { table, columns, rows };
}
