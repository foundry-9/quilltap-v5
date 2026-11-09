/**
 * Oracle case (P4.D374, tier 1): `run_sql`'s result size budget.
 *
 * Drives the REAL exported `fitToBudget` of v4's
 * `lib/tools/handlers/run-sql-handler.ts` (added by `c539c82e0`): the 24,000
 * JSON-character budget (measured as `JSON.stringify(rows).length`, UTF-16
 * units), the 600-unit long-cell clip with its suffix, the 0.75 trailing-row
 * drop loop, and the note (emitted WHENEVER the budget is exceeded — with no
 * clip and no drop its parts list is EMPTY).
 *
 * Each line: `{ id, inputJson, outJson }`, both `JSON.stringify` bytes (so a
 * `1e21` reads `1e+21` and a lone surrogate reads `\ud83d`, exactly as V8
 * writes them — the Rust side compares against these bytes).
 *
 * Run from inside the pinned v4 checkout:
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/run-sql-fit-to-budget.ts \
 *     > /tmp/oracle-run-sql-fit-to-budget.ndjson
 */

import { fitToBudget } from '@/lib/tools/handlers/run-sql-handler';

type Row = Record<string, unknown>;

const x = (n: number) => 'x'.repeat(n);
const cases: Array<{ id: string; rows: Row[] }> = [];
const add = (id: string, rows: Row[]) => cases.push({ id, rows });

// Under budget: unchanged, no note.
add('fit_under', [{ a: 'x', b: 1 }, { a: x(700), b: 2 }]);
add('fit_empty', []);
// `[{"a":"<s>"}]` is 10 + s units: exactly 24,000 → under (a 23,990-unit cell
// over 600 stays whole); 24,001 → over (the clip fires, one row, no drop).
add('fit_exactly_24000', [{ a: x(23990) }]);
add('fit_24001', [{ a: x(23991) }]);
// One 30,000-unit cell.
add('fit_one_huge_cell', [{ id: 1, body: x(30000) }]);
// Drop only: no cell over 600.
add('fit_drop_only_100x400', Array.from({ length: 100 }, (_, i) => ({ i, t: x(400) })));
// Clip then fit.
add('fit_clip_then_drop_60x700', Array.from({ length: 60 }, (_, i) => ({ i, t: x(700) })));
// One over-budget row, no long string: the empty-parts note.
const wide: Row = {};
for (let k = 0; k < 100; k++) wide[`c${k}`] = x(300);
add('fit_one_row_no_long_string', [wide]);
// Numeric-heavy rows: reals as V8 prints them (`1e+21`, `0.1`, `-0` → `0`).
add('fit_numeric_heavy', Array.from({ length: 2000 }, (_, i) => ({ i, a: 1e21, b: 0.1, c: -0, d: 1.5e-7 })));
// The L-4 boundary. MEASURED (P4.D374): serde_json ALSO writes `1e+21`, so a
// `1e21` row cannot tell the two measures apart — these two rows stay as
// boundary rows only. The real split is `1e20`: V8 writes
// `100000000000000000000` (21 units), serde_json `1e20` (4). The `1e20` rows
// are 24,001 / 24,000 units on V8 and 17 units shorter under a serde measure.
{
  const probe20 = JSON.stringify([{ a: 1e20, s: '' }]).length;
  add('fit_1e20_boundary', [{ a: 1e20, s: x(24001 - probe20) }]);
  add('fit_1e20_boundary_under', [{ a: 1e20, s: x(24000 - probe20) }]);
}
{
  const probe = JSON.stringify([{ a: 1e21, s: '' }]).length; // V8 length with an empty s
  add('fit_1e21_boundary', [{ a: 1e21, s: x(24001 - probe) }]);
  add('fit_1e21_boundary_under', [{ a: 1e21, s: x(24000 - probe) }]);
}
// An astral pair straddling unit 600 (L-3: v4 keeps a lone surrogate).
add('fit_astral_600', Array.from({ length: 40 }, (_, i) => ({ i, t: `${x(599)}😀${x(200)}` })));
// An astral pair wholly inside the kept prefix (no split → equal both sides).
add('fit_astral_inside', Array.from({ length: 40 }, (_, i) => ({ i, t: `😀${x(700)}` })));
// Keys needing JSON escapes (the suffix names the key raw).
add('fit_key_escapes', Array.from({ length: 40 }, (_, i) => ({ i, 'a"b': x(700), 'line\nbreak': x(650) })));
// Integer-like keys (JS own-property order: integer keys first, ascending).
add('fit_integer_like_keys', Array.from({ length: 40 }, () => ({ b: x(700), 2: x(650), 1: 'one' })));
// Mixed non-string cells over 600 "length" are never clipped (only strings).
add('fit_non_string_cells', Array.from({ length: 300 }, (_, i) => ({ i, n: null, t: x(80), o: true })));

for (const c of cases) {
  const out = fitToBudget(c.rows);
  process.stdout.write(
    JSON.stringify({ id: c.id, inputJson: JSON.stringify(c.rows), outJson: JSON.stringify(out) }) + '\n',
  );
}
